//! Network configuration

use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Node's listening addresses
    pub listen_addresses: Vec<Multiaddr>,

    /// Bootstrap nodes to connect to
    pub bootstrap_nodes: Vec<Multiaddr>,

    /// Path to store node identity keypair
    pub keystore_path: PathBuf,

    /// Maximum number of peer connections
    pub max_peers: usize,

    /// Connection timeout
    pub connection_timeout: Duration,

    /// Heartbeat interval for gossip
    pub gossip_heartbeat: Duration,

    /// Maximum message size for gossip
    pub max_message_size: usize,

    /// Mesh network parameters
    pub mesh_n: usize,
    pub mesh_n_low: usize,
    pub mesh_n_high: usize,

    /// Enable/disable protocols
    pub enable_gossip: bool,
    pub enable_sync: bool,
    pub enable_mdns: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/30333".parse().unwrap(),
                "/ip6/::/tcp/30333".parse().unwrap(),
            ],
            bootstrap_nodes: Vec::new(),
            keystore_path: PathBuf::from("./keystore"),
            max_peers: 50,
            connection_timeout: Duration::from_secs(10),
            gossip_heartbeat: Duration::from_secs(1),
            max_message_size: 128 * 1024, // 128 KB
            mesh_n: 12,
            mesh_n_low: 9,
            mesh_n_high: 15,
            enable_gossip: true,
            enable_sync: true,
            enable_mdns: true,
        }
    }
}

impl NetworkConfig {
    /// Create a new network configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set listening addresses
    pub fn with_listen_addresses(mut self, addresses: Vec<Multiaddr>) -> Self {
        self.listen_addresses = addresses;
        self
    }

    /// Add bootstrap nodes
    pub fn with_bootstrap_nodes(mut self, nodes: Vec<Multiaddr>) -> Self {
        self.bootstrap_nodes = nodes;
        self
    }

    /// Set keystore path
    pub fn with_keystore_path(mut self, path: PathBuf) -> Self {
        self.keystore_path = path;
        self
    }

    /// Set maximum peers
    pub fn with_max_peers(mut self, max_peers: usize) -> Self {
        self.max_peers = max_peers;
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.listen_addresses.is_empty() {
            return Err("At least one listen address must be specified".to_string());
        }

        if self.max_peers == 0 {
            return Err("Maximum peers must be greater than 0".to_string());
        }

        if self.mesh_n_low >= self.mesh_n || self.mesh_n >= self.mesh_n_high {
            return Err("Invalid mesh parameters: mesh_n_low < mesh_n < mesh_n_high".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NetworkConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.max_peers, 50);
        assert_eq!(config.mesh_n, 12);
    }

    #[test]
    fn test_config_builder() {
        let config = NetworkConfig::new()
            .with_max_peers(100)
            .with_keystore_path(PathBuf::from("/tmp/test"));

        assert_eq!(config.max_peers, 100);
        assert_eq!(config.keystore_path, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_config_validation() {
        let mut config = NetworkConfig::default();
        config.max_peers = 0;
        assert!(config.validate().is_err());

        config.max_peers = 10;
        config.mesh_n_low = 15;
        assert!(config.validate().is_err());
    }
}
