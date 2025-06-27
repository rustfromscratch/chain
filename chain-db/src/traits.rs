//! Database traits and interfaces

use crate::{DbError, DbResult};
use std::sync::Arc;

/// Key-value database trait
pub trait KeyValueDB: Send + Sync {
    /// Get value by key from a column family
    fn get(&self, cf: &str, key: &[u8]) -> DbResult<Option<Vec<u8>>>;

    /// Put key-value pair into a column family
    fn put(&self, cf: &str, key: &[u8], value: &[u8]) -> DbResult<()>;

    /// Delete key from a column family
    fn delete(&self, cf: &str, key: &[u8]) -> DbResult<()>;

    /// Check if key exists in a column family
    fn exists(&self, cf: &str, key: &[u8]) -> DbResult<bool>;

    /// Create a new transaction
    fn transaction(&self) -> Box<dyn DbTx>;

    /// Create a snapshot reader
    fn snapshot(&self) -> Box<dyn SnapshotReader>;

    /// Get iterator over keys in a column family
    fn iter(&self, cf: &str) -> DbResult<Box<dyn Iterator<Item = DbResult<(Vec<u8>, Vec<u8>)>>>>;

    /// Get iterator with prefix
    fn iter_prefix(&self, cf: &str, prefix: &[u8]) -> DbResult<Box<dyn Iterator<Item = DbResult<(Vec<u8>, Vec<u8>)>>>>;

    /// Compact database
    fn compact(&self) -> DbResult<()>;

    /// Compact specific range
    fn compact_range(&self, cf: &str, start: Option<&[u8]>, end: Option<&[u8]>) -> DbResult<()>;

    /// Flush WAL to disk
    fn flush(&self) -> DbResult<()>;

    /// Get database statistics
    fn stats(&self) -> DbResult<DatabaseStats>;
}

/// Database transaction trait
pub trait DbTx: Send {
    /// Get value by key from a column family
    fn get(&self, cf: &str, key: &[u8]) -> DbResult<Option<Vec<u8>>>;

    /// Put key-value pair into a column family
    fn put(&mut self, cf: &str, key: &[u8], value: &[u8]) -> DbResult<()>;

    /// Delete key from a column family
    fn delete(&mut self, cf: &str, key: &[u8]) -> DbResult<()>;

    /// Commit the transaction
    fn commit(self: Box<Self>) -> DbResult<()>;

    /// Rollback the transaction
    fn rollback(self: Box<Self>) -> DbResult<()>;
}

/// Snapshot reader trait
pub trait SnapshotReader: Send + Sync {
    /// Get value by key from a column family
    fn get(&self, cf: &str, key: &[u8]) -> DbResult<Option<Vec<u8>>>;

    /// Check if key exists in a column family
    fn exists(&self, cf: &str, key: &[u8]) -> DbResult<bool>;

    /// Get iterator over keys in a column family
    fn iter(&self, cf: &str) -> DbResult<Box<dyn Iterator<Item = DbResult<(Vec<u8>, Vec<u8>)>>>>;

    /// Clone this snapshot
    fn clone_snapshot(&self) -> Box<dyn SnapshotReader>;
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    /// Total size in bytes
    pub total_size: u64,
    /// Number of keys
    pub num_keys: u64,
    /// Memory usage
    pub memory_usage: u64,
    /// Column family statistics
    pub cf_stats: std::collections::HashMap<String, ColumnFamilyStats>,
}

/// Column family statistics
#[derive(Debug, Clone)]
pub struct ColumnFamilyStats {
    /// Size in bytes
    pub size: u64,
    /// Number of keys
    pub num_keys: u64,
    /// Number of files
    pub num_files: u64,
}

/// Shared database reference
pub type SharedDatabase = Arc<dyn KeyValueDB>;

/// Database transaction builder
pub struct TransactionBuilder {
    operations: Vec<Operation>,
}

/// Database operation
#[derive(Debug, Clone)]
pub enum Operation {
    Put {
        cf: String,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Delete {
        cf: String,
        key: Vec<u8>,
    },
}

impl TransactionBuilder {
    /// Create new transaction builder
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    /// Add put operation
    pub fn put(&mut self, cf: &str, key: &[u8], value: &[u8]) -> &mut Self {
        self.operations.push(Operation::Put {
            cf: cf.to_string(),
            key: key.to_vec(),
            value: value.to_vec(),
        });
        self
    }

    /// Add delete operation
    pub fn delete(&mut self, cf: &str, key: &[u8]) -> &mut Self {
        self.operations.push(Operation::Delete {
            cf: cf.to_string(),
            key: key.to_vec(),
        });
        self
    }

    /// Execute all operations in a transaction
    pub fn execute(self, db: &dyn KeyValueDB) -> DbResult<()> {
        let mut tx = db.transaction();
        
        for operation in self.operations {
            match operation {
                Operation::Put { cf, key, value } => {
                    tx.put(&cf, &key, &value)?;
                }
                Operation::Delete { cf, key } => {
                    tx.delete(&cf, &key)?;
                }
            }
        }
        
        tx.commit()
    }

    /// Get number of operations
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// Check if builder is empty
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self::new()
    }
}
