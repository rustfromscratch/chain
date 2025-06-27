//! Consensus error types

use thiserror::Error;

/// Consensus error type
#[derive(Error, Debug, Clone)]
pub enum ConsensusError {
    /// Invalid block header
    #[error("Invalid block header: {0}")]
    InvalidBlock(String),

    /// Invalid validator
    #[error("Invalid validator: {0}")]
    InvalidValidator(String),

    /// VRF verification failed
    #[error("VRF verification failed: {0}")]
    VrfError(String),

    /// Double signing detected
    #[error("Double signing detected by validator {validator_index}")]
    DoubleSigning { validator_index: usize },

    /// Not authorized to propose
    #[error("Not authorized to propose at slot {slot}")]
    NotAuthorized { slot: u64 },

    /// Invalid timestamp
    #[error("Invalid timestamp: expected {expected}, got {actual}")]
    InvalidTimestamp { expected: u64, actual: u64 },

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Authority set error
    #[error("Authority set error: {0}")]
    AuthoritySet(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Other error
    #[error("Consensus error: {0}")]
    Other(String),
}

impl From<serde_json::Error> for ConsensusError {
    fn from(err: serde_json::Error) -> Self {
        ConsensusError::Serialization(err.to_string())
    }
}

impl From<anyhow::Error> for ConsensusError {
    fn from(err: anyhow::Error) -> Self {
        ConsensusError::Other(err.to_string())
    }
}

/// Result type for consensus operations
pub type ConsensusResult<T> = Result<T, ConsensusError>;
