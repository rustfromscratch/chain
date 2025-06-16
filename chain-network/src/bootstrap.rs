//! Bootstrap node configuration and management

use libp2p::Multiaddr;
use std::str::FromStr;

/// Well-known bootstrap nodes for the blockchain network
pub struct BootstrapNodes;

impl BootstrapNodes {
    /// Get the default bootstrap nodes
    pub fn default_nodes() -> Vec<Multiaddr> {
        vec![
            // Add mainnet bootstrap nodes here when available
            // For now, these are placeholder/testnet nodes
        ]
    }

    /// Get testnet bootstrap nodes
    pub fn testnet_nodes() -> Vec<Multiaddr> {
        vec![
            // Testnet bootstrap nodes
            "/dns4/testnet-boot-1.chain.example.com/tcp/30333/p2p/12D3KooWBootstrap1"
                .parse()
                .unwrap_or_else(|_| "/ip4/127.0.0.1/tcp/30334".parse().unwrap()),
            "/dns4/testnet-boot-2.chain.example.com/tcp/30333/p2p/12D3KooWBootstrap2"
                .parse()
                .unwrap_or_else(|_| "/ip4/127.0.0.1/tcp/30335".parse().unwrap()),
        ]
    }

    /// Get local development bootstrap nodes
    pub fn local_nodes() -> Vec<Multiaddr> {
        vec![
            "/ip4/127.0.0.1/tcp/30334".parse().unwrap(),
            "/ip4/127.0.0.1/tcp/30335".parse().unwrap(),
        ]
    }

    /// Parse bootstrap nodes from command line arguments
    pub fn from_strings(nodes: Vec<String>) -> Result<Vec<Multiaddr>, String> {
        nodes
            .into_iter()
            .map(|s| {
                Multiaddr::from_str(&s).map_err(|e| format!("Invalid multiaddr '{}': {}", s, e))
            })
            .collect()
    }

    /// Get bootstrap nodes based on network type
    pub fn for_network(network: &str) -> Vec<Multiaddr> {
        match network {
            "mainnet" => Self::default_nodes(),
            "testnet" => Self::testnet_nodes(),
            "local" | "dev" => Self::local_nodes(),
            _ => {
                tracing::warn!("Unknown network type: {}, using default nodes", network);
                Self::default_nodes()
            }
        }
    }
}

/// Bootstrap configuration
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// List of bootstrap nodes
    pub nodes: Vec<Multiaddr>,
    /// Maximum number of bootstrap connections to attempt
    pub max_bootstrap_peers: usize,
    /// Bootstrap timeout in seconds
    pub bootstrap_timeout: u64,
    /// Whether to enable automatic bootstrap on start
    pub auto_bootstrap: bool,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            nodes: BootstrapNodes::default_nodes(),
            max_bootstrap_peers: 5,
            bootstrap_timeout: 30,
            auto_bootstrap: true,
        }
    }
}

impl BootstrapConfig {
    /// Create a new bootstrap configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set bootstrap nodes
    pub fn with_nodes(mut self, nodes: Vec<Multiaddr>) -> Self {
        self.nodes = nodes;
        self
    }

    /// Add a bootstrap node
    pub fn add_node(mut self, node: Multiaddr) -> Self {
        self.nodes.push(node);
        self
    }

    /// Set maximum bootstrap peers
    pub fn with_max_bootstrap_peers(mut self, max: usize) -> Self {
        self.max_bootstrap_peers = max;
        self
    }

    /// Set bootstrap timeout
    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.bootstrap_timeout = timeout;
        self
    }

    /// Enable/disable auto bootstrap
    pub fn with_auto_bootstrap(mut self, enable: bool) -> Self {
        self.auto_bootstrap = enable;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.auto_bootstrap && self.nodes.is_empty() {
            return Err(
                "Auto bootstrap is enabled but no bootstrap nodes are configured".to_string(),
            );
        }

        if self.max_bootstrap_peers == 0 {
            return Err("Maximum bootstrap peers must be greater than 0".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bootstrap_nodes() {
        let testnet_nodes = BootstrapNodes::testnet_nodes();
        assert!(!testnet_nodes.is_empty());

        let local_nodes = BootstrapNodes::local_nodes();
        assert!(!local_nodes.is_empty());

        let network_nodes = BootstrapNodes::for_network("testnet");
        assert_eq!(network_nodes.len(), testnet_nodes.len());
    }

    #[test]
    fn test_from_strings() {
        let valid_strings = vec![
            "/ip4/127.0.0.1/tcp/30333".to_string(),
            "/ip6/::1/tcp/30333".to_string(),
        ];

        let result = BootstrapNodes::from_strings(valid_strings);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);

        let invalid_strings = vec!["not-a-multiaddr".to_string()];
        let result = BootstrapNodes::from_strings(invalid_strings);
        assert!(result.is_err());
    }
    #[test]
    fn test_bootstrap_config() {
        let config = BootstrapConfig::new()
            .with_max_bootstrap_peers(10)
            .with_timeout(60)
            .with_auto_bootstrap(false); // Disable auto bootstrap since we have no nodes

        assert_eq!(config.max_bootstrap_peers, 10);
        assert_eq!(config.bootstrap_timeout, 60);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_bootstrap_config_validation() {
        let mut config = BootstrapConfig::new();
        config.nodes.clear();
        config.auto_bootstrap = true;
        assert!(config.validate().is_err());

        config.auto_bootstrap = false;
        assert!(config.validate().is_ok());

        config.max_bootstrap_peers = 0;
        assert!(config.validate().is_err());
    }
}
