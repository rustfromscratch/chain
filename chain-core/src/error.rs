//! Error types for the core crate

use thiserror::Error;

/// Core blockchain errors
#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Invalid hash: {0}")]
    InvalidHash(String),

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Trie error: {0}")]
    Trie(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Hex decode error: {0}")]
    HexDecode(#[from] hex::FromHexError),
    #[error("Bincode error: {0}")]
    Bincode(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result type for core operations
pub type CoreResult<T> = Result<T, CoreError>;
