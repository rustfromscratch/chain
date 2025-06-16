//! Blockchain networking layer
//!
//! This crate provides P2P networking functionality for the blockchain,
//! including peer discovery, message propagation, and synchronization protocols.

pub mod bootstrap;
pub mod config;
pub mod error;
pub mod gossip;
pub mod identity;
pub mod message;
pub mod peer;
pub mod sync;
pub mod transport;

pub use config::NetworkConfig;
pub use error::{NetworkError, NetworkResult};
pub use gossip::GossipManager;
pub use identity::{NodeId, PeerIdentity};
pub use message::{SyncRequest, SyncResponse};
pub use peer::{Peer, PeerManager};
pub use sync::SyncManager;

/// Re-export commonly used types
pub use libp2p::{Multiaddr, PeerId};

#[cfg(test)]
mod tests {
    #[test]
    fn test_network_basics() {
        // Basic smoke test
        assert!(true);
    }
}
