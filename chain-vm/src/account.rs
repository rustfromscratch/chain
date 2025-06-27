//! Account model and state

use crate::{VmError, VmResult};
use chain_core::{Address, Hash};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Account information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// Account nonce (number of transactions sent)
    pub nonce: u64,
    /// Account balance
    pub balance: u64,
    /// Code hash (empty for externally owned accounts)
    pub code_hash: Hash,
    /// Storage root hash
    pub storage_root: Hash,
}

impl Account {
    /// Create a new empty account
    pub fn new() -> Self {
        Self {
            nonce: 0,
            balance: 0,
            code_hash: Hash::zero(),
            storage_root: Hash::zero(),
        }
    }

    /// Create an account with initial balance
    pub fn with_balance(balance: u64) -> Self {
        Self {
            nonce: 0,
            balance,
            code_hash: Hash::zero(),
            storage_root: Hash::zero(),
        }
    }

    /// Check if account is empty
    pub fn is_empty(&self) -> bool {
        self.nonce == 0 && self.balance == 0 && self.code_hash == Hash::zero()
    }

    /// Check if account is a contract
    pub fn is_contract(&self) -> bool {
        self.code_hash != Hash::zero()
    }

    /// Increment nonce
    pub fn increment_nonce(&mut self) {
        self.nonce += 1;
    }

    /// Add to balance
    pub fn add_balance(&mut self, amount: u64) -> VmResult<()> {
        self.balance = self.balance.checked_add(amount)
            .ok_or_else(|| VmError::Other("Balance overflow".to_string()))?;
        Ok(())
    }

    /// Subtract from balance
    pub fn sub_balance(&mut self, amount: u64) -> VmResult<()> {
        if self.balance < amount {
            return Err(VmError::InsufficientBalance {
                required: amount,
                available: self.balance,
            });
        }
        self.balance -= amount;
        Ok(())
    }

    /// Set code hash (for contract accounts)
    pub fn set_code_hash(&mut self, code_hash: Hash) {
        self.code_hash = code_hash;
    }

    /// Set storage root
    pub fn set_storage_root(&mut self, storage_root: Hash) {
        self.storage_root = storage_root;
    }
}

impl Default for Account {
    fn default() -> Self {
        Self::new()
    }
}

/// Account state with storage
#[derive(Debug, Clone)]
pub struct AccountState {
    /// Account information
    pub account: Account,
    /// Contract storage (for in-memory operations)
    pub storage: HashMap<Hash, Vec<u8>>,
    /// Contract code (if any)
    pub code: Option<Vec<u8>>,
}

impl AccountState {
    /// Create new account state
    pub fn new(account: Account) -> Self {
        Self {
            account,
            storage: HashMap::new(),
            code: None,
        }
    }

    /// Get storage value
    pub fn get_storage(&self, key: &Hash) -> Option<&Vec<u8>> {
        self.storage.get(key)
    }

    /// Set storage value
    pub fn set_storage(&mut self, key: Hash, value: Vec<u8>) {
        if value.is_empty() {
            self.storage.remove(&key);
        } else {
            self.storage.insert(key, value);
        }
    }

    /// Get contract code
    pub fn get_code(&self) -> Option<&Vec<u8>> {
        self.code.as_ref()
    }

    /// Set contract code
    pub fn set_code(&mut self, code: Vec<u8>) {
        let code_hash = Hash::from_data(&code);
        self.account.set_code_hash(code_hash);
        self.code = Some(code);
    }

    /// Check if account has code
    pub fn has_code(&self) -> bool {
        self.code.is_some() && !self.code.as_ref().unwrap().is_empty()
    }
}

/// Account changes for batch updates
#[derive(Debug, Clone)]
pub struct AccountChanges {
    /// Updated accounts
    pub accounts: HashMap<Address, Account>,
    /// Deleted accounts
    pub deleted: Vec<Address>,
    /// Storage changes
    pub storage_changes: HashMap<Address, HashMap<Hash, Vec<u8>>>,
    /// Code changes
    pub code_changes: HashMap<Address, Vec<u8>>,
}

impl AccountChanges {
    /// Create new empty changes
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            deleted: Vec::new(),
            storage_changes: HashMap::new(),
            code_changes: HashMap::new(),
        }
    }

    /// Update account
    pub fn update_account(&mut self, address: Address, account: Account) {
        self.accounts.insert(address, account);
    }

    /// Delete account
    pub fn delete_account(&mut self, address: Address) {
        self.deleted.push(address);
        self.accounts.remove(&address);
        self.storage_changes.remove(&address);
        self.code_changes.remove(&address);
    }

    /// Update storage
    pub fn update_storage(&mut self, address: Address, key: Hash, value: Vec<u8>) {
        self.storage_changes
            .entry(address)
            .or_insert_with(HashMap::new)
            .insert(key, value);
    }

    /// Update code
    pub fn update_code(&mut self, address: Address, code: Vec<u8>) {
        self.code_changes.insert(address, code);
    }

    /// Check if changes are empty
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty() 
            && self.deleted.is_empty() 
            && self.storage_changes.is_empty() 
            && self.code_changes.is_empty()
    }

    /// Merge with another changes set
    pub fn merge(&mut self, other: AccountChanges) {
        // Merge accounts
        for (addr, account) in other.accounts {
            self.accounts.insert(addr, account);
        }

        // Merge deletions
        self.deleted.extend(other.deleted);

        // Merge storage changes
        for (addr, storage) in other.storage_changes {
            let entry = self.storage_changes.entry(addr).or_insert_with(HashMap::new);
            for (key, value) in storage {
                entry.insert(key, value);
            }
        }

        // Merge code changes
        for (addr, code) in other.code_changes {
            self.code_changes.insert(addr, code);
        }
    }
}

impl Default for AccountChanges {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_creation() {
        let account = Account::new();
        assert_eq!(account.nonce, 0);
        assert_eq!(account.balance, 0);
        assert!(account.is_empty());
        assert!(!account.is_contract());
    }

    #[test]
    fn test_account_with_balance() {
        let account = Account::with_balance(1000);
        assert_eq!(account.balance, 1000);
        assert!(!account.is_empty());
    }

    #[test]
    fn test_balance_operations() {
        let mut account = Account::new();

        // Add balance
        account.add_balance(500).unwrap();
        assert_eq!(account.balance, 500);

        // Subtract balance
        account.sub_balance(200).unwrap();
        assert_eq!(account.balance, 300);

        // Insufficient balance should fail
        assert!(account.sub_balance(400).is_err());
    }

    #[test]
    fn test_nonce_increment() {
        let mut account = Account::new();
        assert_eq!(account.nonce, 0);

        account.increment_nonce();
        assert_eq!(account.nonce, 1);
    }

    #[test]
    fn test_contract_account() {
        let mut account = Account::new();
        assert!(!account.is_contract());

        let code_hash = Hash::from_data(b"some code");
        account.set_code_hash(code_hash);
        assert!(account.is_contract());
    }

    #[test]
    fn test_account_state() {
        let account = Account::with_balance(1000);
        let mut state = AccountState::new(account);

        // Test storage
        let key = Hash::from_data(b"storage_key");
        let value = b"storage_value".to_vec();
        
        state.set_storage(key, value.clone());
        assert_eq!(state.get_storage(&key), Some(&value));

        // Test code
        let code = b"contract code".to_vec();
        state.set_code(code.clone());
        assert_eq!(state.get_code(), Some(&code));
        assert!(state.has_code());
    }

    #[test]
    fn test_account_changes() {
        let mut changes = AccountChanges::new();
        assert!(changes.is_empty());

        let address = Address([1u8; 20]);
        let account = Account::with_balance(1000);

        changes.update_account(address, account);
        assert!(!changes.is_empty());

        let key = Hash::from_data(b"key");
        let value = b"value".to_vec();
        changes.update_storage(address, key, value);

        let code = b"code".to_vec();
        changes.update_code(address, code);

        assert!(!changes.is_empty());
        assert!(changes.accounts.contains_key(&address));
        assert!(changes.storage_changes.contains_key(&address));
        assert!(changes.code_changes.contains_key(&address));
    }
}
