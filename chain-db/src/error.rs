//! Database error types

use thiserror::Error;

/// Database error type
#[derive(Error, Debug)]
pub enum DbError {
    /// RocksDB error
    #[error("RocksDB error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    /// Sled error
    #[cfg(feature = "sled-backend")]
    #[error("Sled error: {0}")]
    Sled(#[from] sled::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Key not found
    #[error("Key not found: {0}")]
    KeyNotFound(String),

    /// Invalid data
    #[error("Invalid data: {0}")]
    InvalidData(String),

    /// Transaction error
    #[error("Transaction error: {0}")]
    Transaction(String),

    /// Snapshot error
    #[error("Snapshot error: {0}")]
    Snapshot(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Other error
    #[error("Database error: {0}")]
    Other(String),
}

impl From<bincode::Error> for DbError {
    fn from(err: bincode::Error) -> Self {
        DbError::Serialization(err.to_string())
    }
}

impl From<serde_json::Error> for DbError {
    fn from(err: serde_json::Error) -> Self {
        DbError::Serialization(err.to_string())
    }
}

/// Result type for database operations
pub type DbResult<T> = Result<T, DbError>;
