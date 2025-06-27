//! State database and snapshots

use crate::account::{Account, AccountChanges, AccountState};
use crate::{VmError, VmResult};
use chain_core::{Address, Hash};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// State database trait
pub trait StateDB: Send + Sync {
    /// Get account by address
    fn get_account(&self, address: &Address) -> VmResult<Option<Account>>;
    
    /// Set account
    fn set_account(&mut self, address: Address, account: Account) -> VmResult<()>;
    
    /// Delete account
    fn delete_account(&mut self, address: &Address) -> VmResult<()>;
    
    /// Get storage value
    fn get_storage(&self, address: &Address, key: &Hash) -> VmResult<Option<Vec<u8>>>;
    
    /// Set storage value
    fn set_storage(&mut self, address: Address, key: Hash, value: Vec<u8>) -> VmResult<()>;
    
    /// Get contract code
    fn get_code(&self, address: &Address) -> VmResult<Option<Vec<u8>>>;
    
    /// Set contract code
    fn set_code(&mut self, address: Address, code: Vec<u8>) -> VmResult<()>;
    
    /// Apply batch changes
    fn apply_changes(&mut self, changes: AccountChanges) -> VmResult<()>;
    
    /// Get state root hash
    fn state_root(&self) -> Hash;
    
    /// Create a snapshot
    fn snapshot(&self) -> Box<dyn StateSnapshot>;
}

/// State snapshot for rollback/forking
pub trait StateSnapshot: Send + Sync {
    /// Restore state from this snapshot
    fn restore(&self, state: &mut dyn StateDB) -> VmResult<()>;
    
    /// Create a new state DB from this snapshot
    fn fork(&self) -> Box<dyn StateDB>;
}

/// In-memory state database implementation
#[derive(Debug, Clone)]
pub struct MemoryStateDB {
    /// Account data
    accounts: HashMap<Address, Account>,
    /// Storage data
    storage: HashMap<Address, HashMap<Hash, Vec<u8>>>,
    /// Contract code
    code: HashMap<Address, Vec<u8>>,
    /// State root cache
    state_root: Hash,
}

impl MemoryStateDB {
    /// Create new memory state DB
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            storage: HashMap::new(),
            code: HashMap::new(),
            state_root: Hash::zero(),
        }
    }

    /// Create with initial accounts
    pub fn with_accounts(accounts: HashMap<Address, Account>) -> Self {
        let mut db = Self::new();
        db.accounts = accounts;
        db.update_state_root();
        db
    }

    /// Update state root hash
    fn update_state_root(&mut self) {
        // Simple hash of all account data - in production this would use a Merkle tree
        let mut hasher = blake3::Hasher::new();
        
        // Sort accounts for deterministic hashing
        let mut sorted_accounts: Vec<_> = self.accounts.iter().collect();
        sorted_accounts.sort_by_key(|(addr, _)| *addr);
        
        for (address, account) in sorted_accounts {
            hasher.update(&address.0);
            hasher.update(&account.nonce.to_le_bytes());
            hasher.update(&account.balance.to_le_bytes());
            hasher.update(&account.code_hash.0);
            hasher.update(&account.storage_root.0);
        }
        
        let hash = hasher.finalize();
        self.state_root = Hash(hash.as_bytes()[..32].try_into().unwrap());
    }

    /// Get account state for modification
    pub fn get_account_state(&self, address: &Address) -> VmResult<AccountState> {
        let account = self.get_account(address)?.unwrap_or_default();
        let mut state = AccountState::new(account);
        
        // Load storage
        if let Some(storage) = self.storage.get(address) {
            state.storage = storage.clone();
        }
        
        // Load code
        if let Some(code) = self.code.get(address) {
            state.code = Some(code.clone());
        }
        
        Ok(state)
    }

    /// Set account state
    pub fn set_account_state(&mut self, address: Address, state: AccountState) -> VmResult<()> {
        self.set_account(address, state.account)?;
        
        // Update storage
        if !state.storage.is_empty() {
            self.storage.insert(address, state.storage);
        }
        
        // Update code
        if let Some(code) = state.code {
            self.set_code(address, code)?;
        }
        
        Ok(())
    }
}

impl Default for MemoryStateDB {
    fn default() -> Self {
        Self::new()
    }
}

impl StateDB for MemoryStateDB {
    fn get_account(&self, address: &Address) -> VmResult<Option<Account>> {
        Ok(self.accounts.get(address).cloned())
    }

    fn set_account(&mut self, address: Address, account: Account) -> VmResult<()> {
        if account.is_empty() {
            self.accounts.remove(&address);
        } else {
            self.accounts.insert(address, account);
        }
        self.update_state_root();
        Ok(())
    }

    fn delete_account(&mut self, address: &Address) -> VmResult<()> {
        self.accounts.remove(address);
        self.storage.remove(address);
        self.code.remove(address);
        self.update_state_root();
        Ok(())
    }

    fn get_storage(&self, address: &Address, key: &Hash) -> VmResult<Option<Vec<u8>>> {
        Ok(self.storage.get(address)
            .and_then(|storage| storage.get(key))
            .cloned())
    }

    fn set_storage(&mut self, address: Address, key: Hash, value: Vec<u8>) -> VmResult<()> {
        let storage = self.storage.entry(address).or_insert_with(HashMap::new);
        
        if value.is_empty() {
            storage.remove(&key);
            if storage.is_empty() {
                self.storage.remove(&address);
            }
        } else {
            storage.insert(key, value);
        }
        
        self.update_state_root();
        Ok(())
    }

    fn get_code(&self, address: &Address) -> VmResult<Option<Vec<u8>>> {
        Ok(self.code.get(address).cloned())
    }

    fn set_code(&mut self, address: Address, code: Vec<u8>) -> VmResult<()> {
        if code.is_empty() {
            self.code.remove(&address);
        } else {
            self.code.insert(address, code);
        }
        self.update_state_root();
        Ok(())
    }

    fn apply_changes(&mut self, changes: AccountChanges) -> VmResult<()> {
        // Apply account updates
        for (address, account) in changes.accounts {
            self.set_account(address, account)?;
        }

        // Apply deletions
        for address in changes.deleted {
            self.delete_account(&address)?;
        }

        // Apply storage changes
        for (address, storage_changes) in changes.storage_changes {
            for (key, value) in storage_changes {
                self.set_storage(address, key, value)?;
            }
        }

        // Apply code changes
        for (address, code) in changes.code_changes {
            self.set_code(address, code)?;
        }

        Ok(())
    }

    fn state_root(&self) -> Hash {
        self.state_root
    }

    fn snapshot(&self) -> Box<dyn StateSnapshot> {
        Box::new(MemoryStateSnapshot {
            accounts: self.accounts.clone(),
            storage: self.storage.clone(),
            code: self.code.clone(),
            state_root: self.state_root,
        })
    }
}

/// Memory state snapshot
#[derive(Debug, Clone)]
pub struct MemoryStateSnapshot {
    accounts: HashMap<Address, Account>,
    storage: HashMap<Address, HashMap<Hash, Vec<u8>>>,
    code: HashMap<Address, Vec<u8>>,
    state_root: Hash,
}

impl StateSnapshot for MemoryStateSnapshot {
    fn restore(&self, state: &mut dyn StateDB) -> VmResult<()> {
        // This is a bit tricky with trait objects, so we'll just implement for MemoryStateDB
        if let Some(memory_state) = state.as_any_mut().downcast_mut::<MemoryStateDB>() {
            memory_state.accounts = self.accounts.clone();
            memory_state.storage = self.storage.clone();
            memory_state.code = self.code.clone();
            memory_state.state_root = self.state_root;
            Ok(())
        } else {
            Err(VmError::State("Incompatible state DB type".to_string()))
        }
    }

    fn fork(&self) -> Box<dyn StateDB> {
        Box::new(MemoryStateDB {
            accounts: self.accounts.clone(),
            storage: self.storage.clone(),
            code: self.code.clone(),
            state_root: self.state_root,
        })
    }
}

// Add downcast support for MemoryStateDB
impl MemoryStateDB {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Thread-safe state DB wrapper
pub struct SharedStateDB {
    inner: Arc<RwLock<Box<dyn StateDB>>>,
}

impl SharedStateDB {
    /// Create new shared state DB
    pub fn new(state_db: Box<dyn StateDB>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(state_db)),
        }
    }

    /// Create from memory state
    pub fn memory() -> Self {
        Self::new(Box::new(MemoryStateDB::new()))
    }

    /// Get account (read-only)
    pub fn get_account(&self, address: &Address) -> VmResult<Option<Account>> {
        self.inner.read().get_account(address)
    }

    /// Apply changes atomically
    pub fn apply_changes(&self, changes: AccountChanges) -> VmResult<()> {
        self.inner.write().apply_changes(changes)
    }

    /// Create snapshot
    pub fn snapshot(&self) -> Box<dyn StateSnapshot> {
        self.inner.read().snapshot()
    }

    /// Fork state for parallel execution
    pub fn fork(&self) -> SharedStateDB {
        let snapshot = self.snapshot();
        Self::new(snapshot.fork())
    }

    /// Get state root
    pub fn state_root(&self) -> Hash {
        self.inner.read().state_root()
    }
}

impl Clone for SharedStateDB {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_state_db() {
        let mut state = MemoryStateDB::new();
        let address = Address([1u8; 20]);

        // Test account operations
        assert!(state.get_account(&address).unwrap().is_none());

        let account = Account::with_balance(1000);
        state.set_account(address, account.clone()).unwrap();

        let retrieved = state.get_account(&address).unwrap().unwrap();
        assert_eq!(retrieved.balance, 1000);

        // Test storage operations
        let key = Hash::from_data(b"test_key");
        let value = b"test_value".to_vec();

        state.set_storage(address, key, value.clone()).unwrap();
        let retrieved_value = state.get_storage(&address, &key).unwrap().unwrap();
        assert_eq!(retrieved_value, value);

        // Test code operations
        let code = b"contract code".to_vec();
        state.set_code(address, code.clone()).unwrap();
        let retrieved_code = state.get_code(&address).unwrap().unwrap();
        assert_eq!(retrieved_code, code);
    }

    #[test]
    fn test_account_changes() {
        let mut state = MemoryStateDB::new();
        let address = Address([1u8; 20]);

        let mut changes = AccountChanges::new();
        changes.update_account(address, Account::with_balance(1000));

        let key = Hash::from_data(b"key");
        changes.update_storage(address, key, b"value".to_vec());

        changes.update_code(address, b"code".to_vec());

        state.apply_changes(changes).unwrap();

        assert_eq!(state.get_account(&address).unwrap().unwrap().balance, 1000);
        assert!(state.get_storage(&address, &key).unwrap().is_some());
        assert!(state.get_code(&address).unwrap().is_some());
    }

    #[test]
    fn test_state_snapshot() {
        let mut state = MemoryStateDB::new();
        let address = Address([1u8; 20]);

        // Initial state
        state.set_account(address, Account::with_balance(1000)).unwrap();
        let snapshot = state.snapshot();

        // Modify state
        state.set_account(address, Account::with_balance(2000)).unwrap();
        assert_eq!(state.get_account(&address).unwrap().unwrap().balance, 2000);

        // Fork from snapshot
        let forked = snapshot.fork();
        assert_eq!(forked.get_account(&address).unwrap().unwrap().balance, 1000);
    }

    #[test]
    fn test_shared_state_db() {
        let shared = SharedStateDB::memory();
        let address = Address([1u8; 20]);

        let mut changes = AccountChanges::new();
        changes.update_account(address, Account::with_balance(1000));

        shared.apply_changes(changes).unwrap();
        assert_eq!(shared.get_account(&address).unwrap().unwrap().balance, 1000);

        // Test forking
        let forked = shared.fork();
        assert_eq!(forked.get_account(&address).unwrap().unwrap().balance, 1000);
    }
}
