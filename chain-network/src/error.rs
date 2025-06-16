//! Network error types

use thiserror::Error;

pub type NetworkResult<T> = Result<T, NetworkError>;

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Libp2p transport error: {0}")]
    Transport(#[from] libp2p::TransportError<std::io::Error>),

    #[error("Peer connection error: {0}")]
    Connection(String),
    #[error("Message encoding error: {0}")]
    Encoding(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid peer ID: {0}")]
    InvalidPeerId(String),

    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    #[error("Network timeout")]
    Timeout,

    #[error("Protocol not supported: {0}")]
    UnsupportedProtocol(String),

    #[error("Bootstrap error: {0}")]
    Bootstrap(String),

    #[error("Sync error: {0}")]
    Sync(String),

    #[error("Gossip error: {0}")]
    Gossip(String),

    #[error("Configuration error: {0}")]
    Config(String),
}
