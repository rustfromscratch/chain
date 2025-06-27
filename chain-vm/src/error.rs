//! VM error types

use thiserror::Error;

/// VM error type
#[derive(Error, Debug, Clone)]
pub enum VmError {
    /// Insufficient gas
    #[error("Out of gas: required {required}, available {available}")]
    OutOfGas { required: u64, available: u64 },

    /// Invalid transaction
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),

    /// Insufficient balance
    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: u64, available: u64 },

    /// Invalid nonce
    #[error("Invalid nonce: expected {expected}, got {actual}")]
    InvalidNonce { expected: u64, actual: u64 },

    /// Account not found
    #[error("Account not found: {0:?}")]
    AccountNotFound(chain_core::Address),

    /// State error
    #[error("State error: {0}")]
    State(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Contract execution error
    #[error("Contract execution error: {0}")]
    ContractExecution(String),

    /// Other error
    #[error("VM error: {0}")]
    Other(String),
}

impl From<bincode::Error> for VmError {
    fn from(err: bincode::Error) -> Self {
        VmError::Serialization(err.to_string())
    }
}

impl From<anyhow::Error> for VmError {
    fn from(err: anyhow::Error) -> Self {
        VmError::Other(err.to_string())
    }
}

/// Result type for VM operations
pub type VmResult<T> = Result<T, VmError>;
