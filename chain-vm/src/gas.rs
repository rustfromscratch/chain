//! Gas metering and scheduling

use crate::{VmError, VmResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Gas costs for different operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasSchedule {
    /// Base transaction cost
    pub tx_base: u64,
    /// Cost per byte of transaction data
    pub tx_data_per_byte: u64,
    /// Account creation cost
    pub account_creation: u64,
    /// Storage write cost
    pub storage_write: u64,
    /// Storage read cost
    pub storage_read: u64,
    /// Balance transfer cost
    pub balance_transfer: u64,
    /// Contract call cost
    pub contract_call: u64,
    /// Memory allocation cost per byte
    pub memory_per_byte: u64,
    /// CPU instruction cost
    pub cpu_instruction: u64,
}

impl Default for GasSchedule {
    fn default() -> Self {
        Self {
            tx_base: 21000,
            tx_data_per_byte: 68,
            account_creation: 32000,
            storage_write: 20000,
            storage_read: 800,
            balance_transfer: 9000,
            contract_call: 700,
            memory_per_byte: 3,
            cpu_instruction: 1,
        }
    }
}

impl GasSchedule {
    /// Load gas schedule from TOML configuration
    pub fn from_toml(toml_str: &str) -> VmResult<Self> {
        toml::from_str(toml_str)
            .map_err(|e| VmError::Other(format!("Failed to parse gas schedule: {}", e)))
    }

    /// Convert to TOML string
    pub fn to_toml(&self) -> VmResult<String> {
        toml::to_string(self)
            .map_err(|e| VmError::Other(format!("Failed to serialize gas schedule: {}", e)))
    }

    /// Calculate transaction base cost
    pub fn transaction_cost(&self, data_size: usize) -> u64 {
        self.tx_base + (data_size as u64 * self.tx_data_per_byte)
    }

    /// Calculate storage operation cost
    pub fn storage_cost(&self, operation: StorageOp) -> u64 {
        match operation {
            StorageOp::Read => self.storage_read,
            StorageOp::Write => self.storage_write,
        }
    }
}

/// Storage operation types
#[derive(Debug, Clone, Copy)]
pub enum StorageOp {
    Read,
    Write,
}

/// Gas meter for tracking gas consumption
#[derive(Debug, Clone)]
pub struct GasMeter {
    /// Gas limit for the transaction
    limit: u64,
    /// Gas consumed so far
    consumed: u64,
    /// Gas schedule
    schedule: GasSchedule,
    /// Detailed gas consumption by operation
    breakdown: HashMap<String, u64>,
}

impl GasMeter {
    /// Create a new gas meter
    pub fn new(limit: u64, schedule: GasSchedule) -> Self {
        Self {
            limit,
            consumed: 0,
            schedule,
            breakdown: HashMap::new(),
        }
    }

    /// Get remaining gas
    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.consumed)
    }

    /// Get consumed gas
    pub fn consumed(&self) -> u64 {
        self.consumed
    }

    /// Get gas limit
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// Check if we have enough gas for an operation
    pub fn check_gas(&self, required: u64) -> VmResult<()> {
        if self.consumed + required > self.limit {
            return Err(VmError::OutOfGas {
                required,
                available: self.remaining(),
            });
        }
        Ok(())
    }

    /// Consume gas for an operation
    pub fn consume(&mut self, amount: u64, operation: &str) -> VmResult<()> {
        self.check_gas(amount)?;
        self.consumed += amount;
        
        // Track breakdown
        *self.breakdown.entry(operation.to_string()).or_insert(0) += amount;
        
        Ok(())
    }

    /// Consume gas for transaction base cost
    pub fn consume_tx_base(&mut self, data_size: usize) -> VmResult<()> {
        let cost = self.schedule.transaction_cost(data_size);
        self.consume(cost, "tx_base")
    }

    /// Consume gas for balance transfer
    pub fn consume_transfer(&mut self) -> VmResult<()> {
        self.consume(self.schedule.balance_transfer, "transfer")
    }

    /// Consume gas for account creation
    pub fn consume_account_creation(&mut self) -> VmResult<()> {
        self.consume(self.schedule.account_creation, "account_creation")
    }

    /// Consume gas for storage operation
    pub fn consume_storage(&mut self, operation: StorageOp) -> VmResult<()> {
        let cost = self.schedule.storage_cost(operation);
        let op_name = match operation {
            StorageOp::Read => "storage_read",
            StorageOp::Write => "storage_write",
        };
        self.consume(cost, op_name)
    }

    /// Consume gas for contract call
    pub fn consume_contract_call(&mut self) -> VmResult<()> {
        self.consume(self.schedule.contract_call, "contract_call")
    }

    /// Consume gas for memory allocation
    pub fn consume_memory(&mut self, bytes: usize) -> VmResult<()> {
        let cost = bytes as u64 * self.schedule.memory_per_byte;
        self.consume(cost, "memory")
    }

    /// Consume gas for CPU instructions
    pub fn consume_instructions(&mut self, count: u64) -> VmResult<()> {
        let cost = count * self.schedule.cpu_instruction;
        self.consume(cost, "cpu")
    }

    /// Get gas consumption breakdown
    pub fn breakdown(&self) -> &HashMap<String, u64> {
        &self.breakdown
    }

    /// Reset the meter for a new transaction
    pub fn reset(&mut self, new_limit: u64) {
        self.limit = new_limit;
        self.consumed = 0;
        self.breakdown.clear();
    }

    /// Refund gas (for storage deletions, etc.)
    pub fn refund(&mut self, amount: u64, operation: &str) {
        self.consumed = self.consumed.saturating_sub(amount);
        
        // Update breakdown
        if let Some(current) = self.breakdown.get_mut(operation) {
            *current = current.saturating_sub(amount);
        }
    }

    /// Calculate gas price impact on transaction cost
    pub fn calculate_cost(&self, gas_price: u64) -> u64 {
        self.consumed * gas_price
    }
}

/// Gas estimation for different operations
pub struct GasEstimator {
    schedule: GasSchedule,
}

impl GasEstimator {
    /// Create a new gas estimator
    pub fn new(schedule: GasSchedule) -> Self {
        Self { schedule }
    }

    /// Estimate gas for a simple transfer
    pub fn estimate_transfer(&self, data_size: usize) -> u64 {
        self.schedule.transaction_cost(data_size) + self.schedule.balance_transfer
    }

    /// Estimate gas for contract deployment
    pub fn estimate_contract_deploy(&self, code_size: usize, data_size: usize) -> u64 {
        self.schedule.transaction_cost(data_size)
            + self.schedule.account_creation
            + (code_size as u64 * self.schedule.memory_per_byte)
    }

    /// Estimate gas for contract call
    pub fn estimate_contract_call(&self, data_size: usize) -> u64 {
        self.schedule.transaction_cost(data_size) + self.schedule.contract_call
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_schedule_default() {
        let schedule = GasSchedule::default();
        assert_eq!(schedule.tx_base, 21000);
        assert_eq!(schedule.tx_data_per_byte, 68);
    }

    #[test]
    fn test_gas_meter_basic() {
        let schedule = GasSchedule::default();
        let mut meter = GasMeter::new(100000, schedule);

        assert_eq!(meter.remaining(), 100000);
        assert_eq!(meter.consumed(), 0);

        // Consume some gas
        meter.consume(1000, "test").unwrap();
        assert_eq!(meter.consumed(), 1000);
        assert_eq!(meter.remaining(), 99000);
    }

    #[test]
    fn test_gas_meter_out_of_gas() {
        let schedule = GasSchedule::default();
        let mut meter = GasMeter::new(1000, schedule);

        // Should fail when trying to consume more than limit
        let result = meter.consume(2000, "test");
        assert!(result.is_err());
        
        match result.unwrap_err() {
            VmError::OutOfGas { required, available } => {
                assert_eq!(required, 2000);
                assert_eq!(available, 1000);
            }
            _ => panic!("Expected OutOfGas error"),
        }
    }

    #[test]
    fn test_gas_meter_operations() {
        let schedule = GasSchedule::default();
        let mut meter = GasMeter::new(100000, schedule);

        // Test transaction base cost
        meter.consume_tx_base(100).unwrap();
        
        // Test transfer
        meter.consume_transfer().unwrap();
        
        // Test storage operations
        meter.consume_storage(StorageOp::Read).unwrap();
        meter.consume_storage(StorageOp::Write).unwrap();

        assert!(meter.consumed() > 0);
        assert!(meter.breakdown().len() > 0);
    }

    #[test]
    fn test_gas_meter_refund() {
        let schedule = GasSchedule::default();
        let mut meter = GasMeter::new(100000, schedule);

        meter.consume(1000, "test").unwrap();
        assert_eq!(meter.consumed(), 1000);

        meter.refund(500, "test");
        assert_eq!(meter.consumed(), 500);
    }

    #[test]
    fn test_gas_estimator() {
        let schedule = GasSchedule::default();
        let estimator = GasEstimator::new(schedule);

        let transfer_gas = estimator.estimate_transfer(0);
        assert!(transfer_gas > 0);

        let deploy_gas = estimator.estimate_contract_deploy(1000, 100);
        assert!(deploy_gas > transfer_gas);

        let call_gas = estimator.estimate_contract_call(100);
        assert!(call_gas > 0);
    }

    #[test]
    fn test_gas_meter_reset() {
        let schedule = GasSchedule::default();
        let mut meter = GasMeter::new(100000, schedule);

        meter.consume(1000, "test").unwrap();
        assert_eq!(meter.consumed(), 1000);

        meter.reset(200000);
        assert_eq!(meter.consumed(), 0);
        assert_eq!(meter.limit(), 200000);
        assert!(meter.breakdown().is_empty());
    }
}
