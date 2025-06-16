//! Peer management and connection handling

use crate::identity::PeerInfo;
use crate::NetworkResult;
use libp2p::{Multiaddr, PeerId};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;

pub use crate::identity::PeerInfo as Peer;

/// Peer manager handles peer discovery, connection management, and scoring
#[derive(Debug)]
pub struct PeerManager {
    /// Known peers
    peers: RwLock<HashMap<PeerId, PeerInfo>>,
    /// Connection limits
    max_peers: usize,
    /// Connection timeout
    connection_timeout: Duration,
}

impl PeerManager {
    /// Create a new peer manager
    pub fn new(max_peers: usize, connection_timeout: Duration) -> Self {
        Self {
            peers: RwLock::new(HashMap::new()),
            max_peers,
            connection_timeout,
        }
    }

    /// Add a discovered peer
    pub async fn add_peer(&self, peer_id: PeerId, addresses: Vec<Multiaddr>) -> NetworkResult<()> {
        let mut peers = self.peers.write().await;

        match peers.get_mut(&peer_id) {
            Some(peer_info) => {
                // Update existing peer
                for addr in addresses {
                    peer_info.add_address(addr);
                }
                peer_info.update_last_seen();
            }
            None => {
                // Add new peer
                let mut peer_info = PeerInfo::new(peer_id);
                for addr in addresses {
                    peer_info.add_address(addr);
                }
                peers.insert(peer_id, peer_info);
            }
        }

        Ok(())
    }

    /// Remove a peer
    pub async fn remove_peer(&self, peer_id: &PeerId) -> NetworkResult<()> {
        let mut peers = self.peers.write().await;
        peers.remove(peer_id);
        Ok(())
    }

    /// Get peer information
    pub async fn get_peer(&self, peer_id: &PeerId) -> Option<PeerInfo> {
        let peers = self.peers.read().await;
        peers.get(peer_id).cloned()
    }

    /// Get all known peers
    pub async fn get_all_peers(&self) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }

    /// Get good peers (positive score)
    pub async fn get_good_peers(&self) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        peers
            .values()
            .filter(|peer| peer.is_good_peer())
            .cloned()
            .collect()
    }

    /// Update peer score
    pub async fn update_peer_score(&self, peer_id: &PeerId, delta: f64) -> NetworkResult<()> {
        let mut peers = self.peers.write().await;

        if let Some(peer_info) = peers.get_mut(peer_id) {
            peer_info.update_score(delta);
            peer_info.update_last_seen();
        }

        Ok(())
    }

    /// Mark peer as connected
    pub async fn mark_connected(
        &self,
        peer_id: &PeerId,
        protocols: Vec<String>,
    ) -> NetworkResult<()> {
        let mut peers = self.peers.write().await;

        if let Some(peer_info) = peers.get_mut(peer_id) {
            for protocol in protocols {
                peer_info.add_protocol(protocol);
            }
            peer_info.update_last_seen();
            peer_info.update_score(1.0); // Small bonus for successful connection
        }

        Ok(())
    }

    /// Mark peer as disconnected
    pub async fn mark_disconnected(&self, peer_id: &PeerId) -> NetworkResult<()> {
        let mut peers = self.peers.write().await;

        if let Some(peer_info) = peers.get_mut(peer_id) {
            peer_info.protocols.clear();
            // Small penalty for disconnection
            peer_info.update_score(-0.5);
        }

        Ok(())
    }

    /// Get peers to connect to
    pub async fn get_peers_to_connect(&self, count: usize) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        let mut good_peers: Vec<_> = peers
            .values()
            .filter(|peer| peer.is_good_peer() && !peer.addresses.is_empty())
            .cloned()
            .collect();

        // Sort by score (descending)
        good_peers.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        good_peers.into_iter().take(count).collect()
    }

    /// Clean up old/bad peers
    pub async fn cleanup_peers(&self) -> NetworkResult<()> {
        let mut peers = self.peers.write().await;
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Remove peers that haven't been seen for more than 24 hours or have very low scores
        peers.retain(|_, peer| {
            let age = current_time.saturating_sub(peer.last_seen);
            age < 24 * 60 * 60 && peer.score > -50.0
        });

        Ok(())
    }

    /// Get current peer count
    pub async fn peer_count(&self) -> usize {
        let peers = self.peers.read().await;
        peers.len()
    }

    /// Check if we can accept more peers
    pub async fn can_accept_more_peers(&self) -> bool {
        let peer_count = self.peer_count().await;
        peer_count < self.max_peers
    }

    /// Get connection timeout
    pub fn connection_timeout(&self) -> Duration {
        self.connection_timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_peer_manager_basic() {
        let manager = PeerManager::new(50, Duration::from_secs(10));
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/30333".parse().unwrap();

        // Add peer
        manager.add_peer(peer_id, vec![addr.clone()]).await.unwrap();

        // Get peer
        let peer_info = manager.get_peer(&peer_id).await.unwrap();
        assert_eq!(peer_info.peer_id, peer_id);
        assert!(peer_info.addresses.contains(&addr));

        // Check peer count
        assert_eq!(manager.peer_count().await, 1);

        // Remove peer
        manager.remove_peer(&peer_id).await.unwrap();
        assert_eq!(manager.peer_count().await, 0);
    }

    #[tokio::test]
    async fn test_peer_scoring() {
        let manager = PeerManager::new(50, Duration::from_secs(10));
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/30333".parse().unwrap();

        // Add peer
        manager.add_peer(peer_id, vec![addr]).await.unwrap();

        // Update score negatively
        manager.update_peer_score(&peer_id, -15.0).await.unwrap();

        let peer_info = manager.get_peer(&peer_id).await.unwrap();
        assert!(!peer_info.is_good_peer());

        // Get good peers should be empty
        let good_peers = manager.get_good_peers().await;
        assert!(good_peers.is_empty());

        // Improve score
        manager.update_peer_score(&peer_id, 20.0).await.unwrap();

        let good_peers = manager.get_good_peers().await;
        assert_eq!(good_peers.len(), 1);
    }

    #[tokio::test]
    async fn test_connection_management() {
        let manager = PeerManager::new(50, Duration::from_secs(10));
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/30333".parse().unwrap();

        // Add and connect peer
        manager.add_peer(peer_id, vec![addr]).await.unwrap();
        manager
            .mark_connected(&peer_id, vec!["test-protocol".to_string()])
            .await
            .unwrap();

        let peer_info = manager.get_peer(&peer_id).await.unwrap();
        assert!(peer_info.protocols.contains(&"test-protocol".to_string()));
        assert!(peer_info.score > 0.0);

        // Disconnect peer
        manager.mark_disconnected(&peer_id).await.unwrap();

        let peer_info = manager.get_peer(&peer_id).await.unwrap();
        assert!(peer_info.protocols.is_empty());
    }
}
