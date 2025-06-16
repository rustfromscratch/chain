//! Network transport layer configuration

use crate::{NetworkConfig, NetworkResult};
use libp2p::{
    core::Transport,
    dns,
    noise::Config as NoiseConfig,
    tcp::{self, Config as TcpConfig},
    yamux::Config as YamuxConfig,
    Multiaddr, PeerId,
};
use std::time::Duration;

/// Build the libp2p transport stack
pub fn build_transport(
    keypair: &libp2p::identity::Keypair,
    config: &NetworkConfig,
) -> NetworkResult<libp2p::core::transport::Boxed<(PeerId, libp2p::core::muxing::StreamMuxerBox)>> {
    // TCP transport with custom configuration
    let tcp_config = TcpConfig::new().nodelay(true);

    let tcp_transport = tcp::tokio::Transport::new(tcp_config);
    // DNS resolution for domain names in multiaddrs
    let dns_transport = dns::tokio::Transport::system(tcp_transport)
        .map_err(|e| crate::NetworkError::Config(format!("DNS transport error: {}", e)))?;
    // Noise encryption for secure communication
    let noise_config = NoiseConfig::new(keypair)
        .map_err(|e| crate::NetworkError::Config(format!("Noise config error: {}", e)))?;

    // Yamux multiplexing for multiple streams over single connection
    let yamux_config = YamuxConfig::default();

    // Combine all layers
    let transport = dns_transport
        .upgrade(libp2p::core::upgrade::Version::V1Lazy)
        .authenticate(noise_config)
        .multiplex(yamux_config)
        .timeout(config.connection_timeout)
        .boxed();

    Ok(transport)
}

/// Transport configuration options
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Enable TCP nodelay
    pub tcp_nodelay: bool,
    /// Enable port reuse
    pub tcp_port_reuse: bool,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Maximum number of concurrent connections
    pub max_connections: u32,
    /// Connection upgrade timeout
    pub upgrade_timeout: Duration,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            tcp_nodelay: true,
            tcp_port_reuse: true,
            connection_timeout: Duration::from_secs(10),
            max_connections: 100,
            upgrade_timeout: Duration::from_secs(5),
        }
    }
}

impl TransportConfig {
    /// Create new transport configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set TCP nodelay option
    pub fn with_tcp_nodelay(mut self, enable: bool) -> Self {
        self.tcp_nodelay = enable;
        self
    }

    /// Set port reuse option
    pub fn with_port_reuse(mut self, enable: bool) -> Self {
        self.tcp_port_reuse = enable;
        self
    }

    /// Set connection timeout
    pub fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// Set maximum connections
    pub fn with_max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.connection_timeout.is_zero() {
            return Err("Connection timeout must be greater than zero".to_string());
        }

        if self.max_connections == 0 {
            return Err("Maximum connections must be greater than zero".to_string());
        }

        Ok(())
    }
}

/// Address filtering and validation
pub struct AddressFilter;

impl AddressFilter {
    /// Check if an address is valid for connection
    pub fn is_valid_address(addr: &Multiaddr) -> bool {
        // Basic validation - ensure it has required components
        let mut has_ip = false;
        let mut has_tcp = false;

        for protocol in addr.iter() {
            match protocol {
                libp2p::multiaddr::Protocol::Ip4(_)
                | libp2p::multiaddr::Protocol::Ip6(_)
                | libp2p::multiaddr::Protocol::Dns(_)
                | libp2p::multiaddr::Protocol::Dns4(_)
                | libp2p::multiaddr::Protocol::Dns6(_) => {
                    has_ip = true;
                }
                libp2p::multiaddr::Protocol::Tcp(_) => {
                    has_tcp = true;
                }
                _ => {}
            }
        }

        has_ip && has_tcp
    }

    /// Filter out invalid or unwanted addresses
    pub fn filter_addresses(addresses: Vec<Multiaddr>) -> Vec<Multiaddr> {
        addresses
            .into_iter()
            .filter(Self::is_valid_address)
            .filter(|addr| !Self::is_private_address(addr))
            .collect()
    }

    /// Check if address is in private/local range
    pub fn is_private_address(addr: &Multiaddr) -> bool {
        for protocol in addr.iter() {
            match protocol {
                libp2p::multiaddr::Protocol::Ip4(ip) => {
                    if ip.is_private() || ip.is_loopback() {
                        return true;
                    }
                }
                libp2p::multiaddr::Protocol::Ip6(ip) => {
                    if ip.is_loopback() {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Get public addresses from a list
    pub fn get_public_addresses(addresses: Vec<Multiaddr>) -> Vec<Multiaddr> {
        addresses
            .into_iter()
            .filter(Self::is_valid_address)
            .filter(|addr| !Self::is_private_address(addr))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    #[test]
    fn test_transport_config() {
        let config = TransportConfig::new()
            .with_tcp_nodelay(false)
            .with_max_connections(50);

        assert!(!config.tcp_nodelay);
        assert_eq!(config.max_connections, 50);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_transport_config_validation() {
        let mut config = TransportConfig::new();
        config.connection_timeout = Duration::from_secs(0);
        assert!(config.validate().is_err());

        config.connection_timeout = Duration::from_secs(10);
        config.max_connections = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_address_validation() {
        let valid_addr: Multiaddr = "/ip4/127.0.0.1/tcp/30333".parse().unwrap();
        assert!(AddressFilter::is_valid_address(&valid_addr));

        let invalid_addr: Multiaddr = "/ip4/127.0.0.1".parse().unwrap();
        assert!(!AddressFilter::is_valid_address(&invalid_addr));

        let dns_addr: Multiaddr = "/dns4/example.com/tcp/30333".parse().unwrap();
        assert!(AddressFilter::is_valid_address(&dns_addr));
    }

    #[test]
    fn test_private_address_detection() {
        let private_addr: Multiaddr = "/ip4/127.0.0.1/tcp/30333".parse().unwrap();
        assert!(AddressFilter::is_private_address(&private_addr));

        let local_addr: Multiaddr = "/ip4/192.168.1.1/tcp/30333".parse().unwrap();
        assert!(AddressFilter::is_private_address(&local_addr));

        let public_addr: Multiaddr = "/ip4/8.8.8.8/tcp/30333".parse().unwrap();
        assert!(!AddressFilter::is_private_address(&public_addr));
    }

    #[tokio::test]
    async fn test_build_transport() {
        let keypair = Keypair::generate_ed25519();
        let config = NetworkConfig::default();

        let result = build_transport(&keypair, &config);
        assert!(result.is_ok());
    }
}
