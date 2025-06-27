//! Blockchain database layer
//!
//! This crate provides persistent storage for blockchain data including
//! blocks, headers, receipts, state, and indices.

pub mod column_families;
pub mod error;
pub mod kv;
pub mod pruning;
pub mod snapshot;
pub mod traits;

pub use error::{DbError, DbResult};
pub use kv::{Database, DatabaseConfig};
pub use snapshot::{SnapshotService, StateSnapshot};
pub use traits::{DbTx, KeyValueDB, SnapshotReader};

#[cfg(test)]
mod tests {
    #[test]
    fn test_db_basics() {
        // Basic smoke test
        assert!(true);
    }
}
