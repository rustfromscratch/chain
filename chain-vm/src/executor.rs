//! Transaction execution engine

use crate::account::{Account, AccountChanges, AccountState};
use crate::gas::{GasMeter, GasSchedule, StorageOp};
use crate::state::{SharedStateDB, StateDB};
use crate::{VmError, VmResult};
use chain_core::{Address, Hash, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn, info};

/// State change record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateChange {
    /// Account balance changed
    BalanceChange {
        address: Address,
        old_balance: u64,
        new_balance: u64,
    },
    /// Account nonce changed
    NonceChange {
        address: Address,
        old_nonce: u64,
        new_nonce: u64,
    },
    /// Account created
    AccountCreated { address: Address },
    /// Account deleted
    AccountDeleted { address: Address },
    /// Storage value changed
    StorageChange {
        address: Address,
        key: Hash,
        old_value: Option<Vec<u8>>,
        new_value: Option<Vec<u8>>,
    },
    /// Contract code set
    CodeSet {
        address: Address,
        code_hash: Hash,
    },
}

/// Transaction execution result
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Whether the transaction was successful
    pub success: bool,
    /// Gas consumed
    pub gas_used: u64,
    /// State changes made
    pub state_changes: Vec<StateChange>,
    /// Return data (for contract calls)
    pub return_data: Vec<u8>,
    /// Error message if failed
    pub error: Option<String>,
    /// Gas refund (for storage deletions, etc.)
    pub gas_refund: u64,
}

impl ExecutionResult {
    /// Create successful result
    pub fn success(gas_used: u64, state_changes: Vec<StateChange>) -> Self {
        Self {
            success: true,
            gas_used,
            state_changes,
            return_data: Vec::new(),
            error: None,
            gas_refund: 0,
        }
    }

    /// Create failed result
    pub fn failure(gas_used: u64, error: String) -> Self {
        Self {
            success: false,
            gas_used,
            state_changes: Vec::new(),
            return_data: Vec::new(),
            error: Some(error),
            gas_refund: 0,
        }
    }

    /// Add return data
    pub fn with_return_data(mut self, data: Vec<u8>) -> Self {
        self.return_data = data;
        self
    }

    /// Add gas refund
    pub fn with_gas_refund(mut self, refund: u64) -> Self {
        self.gas_refund = refund;
        self
    }
}

/// Transaction execution context
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Block number
    pub block_number: u64,
    /// Block timestamp
    pub timestamp: u64,
    /// Block gas limit
    pub gas_limit: u64,
    /// Coinbase address
    pub coinbase: Address,
}

/// Native balance transfer engine (Engine 0)
pub struct BalanceTransferEngine {
    /// Gas schedule
    gas_schedule: GasSchedule,
}

impl BalanceTransferEngine {
    /// Create new balance transfer engine
    pub fn new(gas_schedule: GasSchedule) -> Self {
        Self { gas_schedule }
    }

    /// Apply a balance transfer transaction
    pub fn apply(
        &self,
        tx: &Transaction,
        state: &SharedStateDB,
        context: &ExecutionContext,
    ) -> VmResult<ExecutionResult> {
        info!("Executing balance transfer from {:?} to {:?}", tx.from()?, tx.to);

        let mut gas_meter = GasMeter::new(tx.gas_limit, self.gas_schedule.clone());
        let mut state_changes = Vec::new();
        let mut account_changes = AccountChanges::new();

        // 1. Charge base transaction cost
        gas_meter.consume_tx_base(tx.data.len())?;

        // 2. Get sender account
        let sender = tx.from()?;
        let mut sender_account = state.get_account(&sender)?.unwrap_or_default();

        // 3. Verify nonce
        if sender_account.nonce != tx.nonce {
            return Ok(ExecutionResult::failure(
                gas_meter.consumed(),
                format!("Invalid nonce: expected {}, got {}", sender_account.nonce, tx.nonce),
            ));
        }

        // 4. Check balance for value + gas
        let total_cost = tx.value + (tx.gas_limit * tx.gas_price);
        if sender_account.balance < total_cost {
            return Ok(ExecutionResult::failure(
                gas_meter.consumed(),
                format!("Insufficient balance: required {}, available {}", total_cost, sender_account.balance),
            ));
        }

        // 5. Charge for transfer
        gas_meter.consume_transfer()?;

        // 6. Get or create recipient account
        let recipient = tx.to;
        let recipient_exists = state.get_account(&recipient)?.is_some();
        let mut recipient_account = state.get_account(&recipient)?.unwrap_or_default();

        // 7. Charge for account creation if needed
        if !recipient_exists && tx.value > 0 {
            gas_meter.consume_account_creation()?;
            state_changes.push(StateChange::AccountCreated { address: recipient });
        }

        // 8. Perform the transfer
        if tx.value > 0 {
            let old_sender_balance = sender_account.balance;
            let old_recipient_balance = recipient_account.balance;

            sender_account.sub_balance(tx.value)?;
            recipient_account.add_balance(tx.value)?;

            state_changes.push(StateChange::BalanceChange {
                address: sender,
                old_balance: old_sender_balance,
                new_balance: sender_account.balance,
            });

            state_changes.push(StateChange::BalanceChange {
                address: recipient,
                old_balance: old_recipient_balance,
                new_balance: recipient_account.balance,
            });
        }

        // 9. Update nonce
        let old_nonce = sender_account.nonce;
        sender_account.increment_nonce();
        state_changes.push(StateChange::NonceChange {
            address: sender,
            old_nonce,
            new_nonce: sender_account.nonce,
        });

        // 10. Pay gas fees to coinbase
        let gas_cost = gas_meter.consumed() * tx.gas_price;
        sender_account.sub_balance(gas_cost)?;

        let mut coinbase_account = state.get_account(&context.coinbase)?.unwrap_or_default();
        coinbase_account.add_balance(gas_cost)?;

        // 11. Apply changes to state
        account_changes.update_account(sender, sender_account);
        account_changes.update_account(recipient, recipient_account);
        account_changes.update_account(context.coinbase, coinbase_account);

        state.apply_changes(account_changes)?;

        debug!("Balance transfer completed successfully, gas used: {}", gas_meter.consumed());

        Ok(ExecutionResult::success(gas_meter.consumed(), state_changes))
    }
}

/// Main transaction executor
pub struct TransactionExecutor {
    /// Balance transfer engine
    balance_engine: BalanceTransferEngine,
    /// Gas schedule
    gas_schedule: GasSchedule,
}

impl TransactionExecutor {
    /// Create new transaction executor
    pub fn new(gas_schedule: GasSchedule) -> Self {
        Self {
            balance_engine: BalanceTransferEngine::new(gas_schedule.clone()),
            gas_schedule,
        }
    }

    /// Execute a transaction
    pub fn execute(
        &self,
        tx: &Transaction,
        state: &SharedStateDB,
        context: &ExecutionContext,
    ) -> VmResult<ExecutionResult> {
        debug!("Executing transaction: {:?}", tx.hash());

        // Basic validation
        if tx.gas_limit == 0 {
            return Ok(ExecutionResult::failure(0, "Gas limit cannot be zero".to_string()));
        }

        if tx.gas_limit > context.gas_limit {
            return Ok(ExecutionResult::failure(0, "Gas limit exceeds block gas limit".to_string()));
        }

        // Determine execution engine based on transaction type
        if tx.data.is_empty() {
            // Simple balance transfer (Engine 0)
            self.balance_engine.apply(tx, state, context)
        } else {
            // Contract call/deployment - not implemented yet
            self.execute_contract_call(tx, state, context)
        }
    }

    /// Execute contract call (placeholder for future WASM implementation)
    fn execute_contract_call(
        &self,
        tx: &Transaction,
        _state: &SharedStateDB,
        _context: &ExecutionContext,
    ) -> VmResult<ExecutionResult> {
        let mut gas_meter = GasMeter::new(tx.gas_limit, self.gas_schedule.clone());
        gas_meter.consume_tx_base(tx.data.len())?;

        warn!("Contract execution not yet implemented");
        
        Ok(ExecutionResult::failure(
            gas_meter.consumed(),
            "Contract execution not yet implemented".to_string(),
        ))
    }

    /// Estimate gas for a transaction
    pub fn estimate_gas(
        &self,
        tx: &Transaction,
        state: &SharedStateDB,
        context: &ExecutionContext,
    ) -> VmResult<u64> {
        // Create a snapshot to avoid modifying state
        let snapshot = state.snapshot();
        let test_state = SharedStateDB::new(snapshot.fork());

        // Execute transaction on test state
        let result = self.execute(tx, &test_state, context)?;
        
        // Add some buffer for gas estimation
        let estimated = result.gas_used + (result.gas_used / 10); // 10% buffer
        
        Ok(estimated.min(context.gas_limit))
    }

    /// Validate transaction without execution
    pub fn validate_transaction(
        &self,
        tx: &Transaction,
        state: &SharedStateDB,
        context: &ExecutionContext,
    ) -> VmResult<()> {
        // Basic validation
        if tx.gas_limit == 0 {
            return Err(VmError::InvalidTransaction("Gas limit cannot be zero".to_string()));
        }

        if tx.gas_limit > context.gas_limit {
            return Err(VmError::InvalidTransaction("Gas limit exceeds block gas limit".to_string()));
        }

        // Verify signature
        if !tx.verify_signature()? {
            return Err(VmError::InvalidTransaction("Invalid signature".to_string()));
        }

        // Check sender account
        let sender = tx.from()?;
        let sender_account = state.get_account(&sender)?.unwrap_or_default();

        // Check nonce
        if sender_account.nonce != tx.nonce {
            return Err(VmError::InvalidNonce {
                expected: sender_account.nonce,
                actual: tx.nonce,
            });
        }

        // Check balance
        let total_cost = tx.value + (tx.gas_limit * tx.gas_price);
        if sender_account.balance < total_cost {
            return Err(VmError::InsufficientBalance {
                required: total_cost,
                available: sender_account.balance,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chain_core::{PrivateKey};

    fn create_test_transaction() -> VmResult<Transaction> {
        let private_key = PrivateKey::generate();
        let sender = private_key.public_key().address();
        let recipient = Address([2u8; 20]);

        let mut tx = Transaction {
            nonce: 0,
            gas_price: 1000,
            gas_limit: 21000,
            to: recipient,
            value: 100,
            data: vec![],
            signature: None,
        };

        tx.sign(&private_key)?;
        Ok(tx)
    }

    #[test]
    fn test_balance_transfer() {
        let gas_schedule = GasSchedule::default();
        let engine = BalanceTransferEngine::new(gas_schedule);
        let state = SharedStateDB::memory();

        let tx = create_test_transaction().unwrap();
        let sender = tx.from().unwrap();

        // Fund sender account
        let mut changes = AccountChanges::new();
        changes.update_account(sender, Account::with_balance(1000000));
        state.apply_changes(changes).unwrap();

        let context = ExecutionContext {
            block_number: 1,
            timestamp: 1000000,
            gas_limit: 1000000,
            coinbase: Address([3u8; 20]),
        };

        let result = engine.apply(&tx, &state, &context).unwrap();
        assert!(result.success);
        assert!(result.gas_used > 0);
        assert!(!result.state_changes.is_empty());
    }

    #[test]
    fn test_transaction_executor() {
        let gas_schedule = GasSchedule::default();
        let executor = TransactionExecutor::new(gas_schedule);
        let state = SharedStateDB::memory();

        let tx = create_test_transaction().unwrap();
        let sender = tx.from().unwrap();

        // Fund sender account
        let mut changes = AccountChanges::new();
        changes.update_account(sender, Account::with_balance(1000000));
        state.apply_changes(changes).unwrap();

        let context = ExecutionContext {
            block_number: 1,
            timestamp: 1000000,
            gas_limit: 1000000,
            coinbase: Address([3u8; 20]),
        };

        // Test validation
        assert!(executor.validate_transaction(&tx, &state, &context).is_ok());

        // Test execution
        let result = executor.execute(&tx, &state, &context).unwrap();
        assert!(result.success);

        // Test gas estimation
        let estimated_gas = executor.estimate_gas(&tx, &state, &context).unwrap();
        assert!(estimated_gas >= result.gas_used);
    }

    #[test]
    fn test_insufficient_balance() {
        let gas_schedule = GasSchedule::default();
        let engine = BalanceTransferEngine::new(gas_schedule);
        let state = SharedStateDB::memory();

        let tx = create_test_transaction().unwrap();
        let sender = tx.from().unwrap();

        // Fund sender account with insufficient balance
        let mut changes = AccountChanges::new();
        changes.update_account(sender, Account::with_balance(10)); // Too low
        state.apply_changes(changes).unwrap();

        let context = ExecutionContext {
            block_number: 1,
            timestamp: 1000000,
            gas_limit: 1000000,
            coinbase: Address([3u8; 20]),
        };

        let result = engine.apply(&tx, &state, &context).unwrap();
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_invalid_nonce() {
        let gas_schedule = GasSchedule::default();
        let executor = TransactionExecutor::new(gas_schedule);
        let state = SharedStateDB::memory();

        let mut tx = create_test_transaction().unwrap();
        tx.nonce = 5; // Wrong nonce

        let sender = tx.from().unwrap();

        // Fund sender account
        let mut changes = AccountChanges::new();
        changes.update_account(sender, Account::with_balance(1000000));
        state.apply_changes(changes).unwrap();

        let context = ExecutionContext {
            block_number: 1,
            timestamp: 1000000,
            gas_limit: 1000000,
            coinbase: Address([3u8; 20]),
        };

        // Should fail validation
        assert!(executor.validate_transaction(&tx, &state, &context).is_err());
    }
}
