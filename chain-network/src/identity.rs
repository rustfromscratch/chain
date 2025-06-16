//! Peer identity and key management

use crate::{NetworkError, NetworkResult};
use libp2p::{identity::Keypair, PeerId};
use std::fs;
use std::path::Path;

pub type NodeId = PeerId;

#[derive(Debug, Clone)]
pub struct PeerIdentity {
    /// The node's keypair
    keypair: Keypair,
    /// The derived peer ID
    peer_id: PeerId,
}

impl PeerIdentity {
    /// Generate a new random identity
    pub fn generate() -> Self {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(&keypair.public());

        Self { keypair, peer_id }
    }

    /// Load identity from keystore file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> NetworkResult<Self> {
        let data = fs::read(path.as_ref()).map_err(NetworkError::Io)?;

        let keypair = Keypair::from_protobuf_encoding(&data)
            .map_err(|e| NetworkError::InvalidPeerId(format!("Failed to decode keypair: {}", e)))?;

        let peer_id = PeerId::from(&keypair.public());

        Ok(Self { keypair, peer_id })
    }

    /// Save identity to keystore file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> NetworkResult<()> {
        // Create directory if it doesn't exist
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }

        let encoded = self
            .keypair
            .to_protobuf_encoding()
            .map_err(|e| NetworkError::InvalidPeerId(format!("Failed to encode keypair: {}", e)))?;

        fs::write(path.as_ref(), encoded)?;
        Ok(())
    }

    /// Load or generate identity from keystore path
    pub fn load_or_generate<P: AsRef<Path>>(path: P) -> NetworkResult<Self> {
        match Self::load_from_file(&path) {
            Ok(identity) => {
                tracing::info!("Loaded existing peer identity: {}", identity.peer_id);
                Ok(identity)
            }
            Err(_) => {
                tracing::info!("Generating new peer identity");
                let identity = Self::generate();
                identity.save_to_file(&path)?;
                tracing::info!("Saved new peer identity: {}", identity.peer_id);
                Ok(identity)
            }
        }
    }

    /// Get the keypair
    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    /// Get the peer ID
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Convert to libp2p keypair (consuming self)
    pub fn into_keypair(self) -> Keypair {
        self.keypair
    }
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Peer ID
    pub peer_id: PeerId,
    /// Multiaddresses where this peer can be reached
    pub addresses: Vec<libp2p::Multiaddr>,
    /// Protocol versions supported
    pub protocols: Vec<String>,
    /// Last seen timestamp (Unix timestamp)
    pub last_seen: u64,
    /// Connection quality score
    pub score: f64,
}

impl PeerInfo {
    pub fn new(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            addresses: Vec::new(),
            protocols: Vec::new(),
            last_seen: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            score: 0.0,
        }
    }

    /// Update last seen timestamp
    pub fn update_last_seen(&mut self) {
        self.last_seen = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Add an address if not already present
    pub fn add_address(&mut self, addr: libp2p::Multiaddr) {
        if !self.addresses.contains(&addr) {
            self.addresses.push(addr);
        }
    }

    /// Add a protocol if not already present
    pub fn add_protocol(&mut self, protocol: String) {
        if !self.protocols.contains(&protocol) {
            self.protocols.push(protocol);
        }
    }

    /// Update peer score
    pub fn update_score(&mut self, delta: f64) {
        self.score += delta;
        // Clamp score between -100.0 and 100.0
        self.score = self.score.clamp(-100.0, 100.0);
    }

    /// Check if peer is considered "good"
    pub fn is_good_peer(&self) -> bool {
        self.score > -10.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_identity() {
        let identity = PeerIdentity::generate();
        let peer_id = identity.peer_id();

        // Peer ID should be deterministic from keypair
        let keypair = identity.keypair();
        let expected_peer_id = PeerId::from(&keypair.public());
        assert_eq!(peer_id, expected_peer_id);
    }

    #[test]
    fn test_save_and_load_identity() {
        let temp_dir = tempdir().unwrap();
        let keystore_path = temp_dir.path().join("peer_key");

        // Generate and save identity
        let original_identity = PeerIdentity::generate();
        original_identity.save_to_file(&keystore_path).unwrap();

        // Load identity and verify it's the same
        let loaded_identity = PeerIdentity::load_from_file(&keystore_path).unwrap();
        assert_eq!(original_identity.peer_id(), loaded_identity.peer_id());
    }

    #[test]
    fn test_load_or_generate() {
        let temp_dir = tempdir().unwrap();
        let keystore_path = temp_dir.path().join("peer_key");

        // First call should generate new identity
        let identity1 = PeerIdentity::load_or_generate(&keystore_path).unwrap();
        assert!(keystore_path.exists());

        // Second call should load existing identity
        let identity2 = PeerIdentity::load_or_generate(&keystore_path).unwrap();
        assert_eq!(identity1.peer_id(), identity2.peer_id());
    }

    #[test]
    fn test_peer_info() {
        let peer_id = PeerId::random();
        let mut peer_info = PeerInfo::new(peer_id);

        assert_eq!(peer_info.peer_id, peer_id);
        assert_eq!(peer_info.score, 0.0);
        assert!(peer_info.is_good_peer());

        peer_info.update_score(-15.0);
        assert!(!peer_info.is_good_peer());

        peer_info.update_score(20.0);
        assert!(peer_info.is_good_peer());
    }
}
