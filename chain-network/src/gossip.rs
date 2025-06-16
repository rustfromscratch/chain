//! Gossip-based message propagation

use crate::message::{BlockAnnounce, GossipMessage, TransactionPropagate};
use crate::{NetworkError, NetworkResult};
use libp2p::PeerId;
use std::collections::HashSet;
use tokio::sync::mpsc;

/// Gossip manager handles message propagation via GossipSub
#[derive(Debug)]
pub struct GossipManager {
    /// Channel for sending gossip messages
    tx: mpsc::UnboundedSender<GossipCommand>,
}

#[derive(Debug)]
pub enum GossipCommand {
    /// Publish a message to a topic
    Publish {
        topic: String,
        message: Box<GossipMessage>,
    },
    /// Subscribe to a topic
    Subscribe(String),
    /// Unsubscribe from a topic
    Unsubscribe(String),
    /// List connected peers for a topic
    ListPeers {
        topic: String,
        response: tokio::sync::oneshot::Sender<Vec<PeerId>>,
    },
}

impl GossipManager {
    /// Create a new gossip manager
    pub fn new() -> (Self, mpsc::UnboundedReceiver<GossipCommand>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    /// Publish a block announcement
    pub async fn announce_block(&self, announce: BlockAnnounce) -> NetworkResult<()> {
        let message = GossipMessage::BlockAnnounce(Box::new(announce));
        self.publish("blocks".to_string(), message).await
    }
    /// Propagate transactions
    pub async fn propagate_transactions(
        &self,
        propagate: TransactionPropagate,
    ) -> NetworkResult<()> {
        let message = GossipMessage::TransactionPropagate(propagate);
        self.publish("transactions".to_string(), message).await
    }

    /// Publish a message to a topic
    pub async fn publish(&self, topic: String, message: GossipMessage) -> NetworkResult<()> {
        self.tx
            .send(GossipCommand::Publish {
                topic,
                message: Box::new(message),
            })
            .map_err(|_| NetworkError::Gossip("Failed to send publish command".to_string()))?;
        Ok(())
    }

    /// Subscribe to a topic
    pub async fn subscribe(&self, topic: String) -> NetworkResult<()> {
        self.tx
            .send(GossipCommand::Subscribe(topic))
            .map_err(|_| NetworkError::Gossip("Failed to send subscribe command".to_string()))?;
        Ok(())
    }

    /// Unsubscribe from a topic
    pub async fn unsubscribe(&self, topic: String) -> NetworkResult<()> {
        self.tx
            .send(GossipCommand::Unsubscribe(topic))
            .map_err(|_| NetworkError::Gossip("Failed to send unsubscribe command".to_string()))?;
        Ok(())
    }

    /// Get connected peers for a topic
    pub async fn list_peers(&self, topic: String) -> NetworkResult<Vec<PeerId>> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.tx
            .send(GossipCommand::ListPeers {
                topic,
                response: response_tx,
            })
            .map_err(|_| NetworkError::Gossip("Failed to send list peers command".to_string()))?;

        response_rx
            .await
            .map_err(|_| NetworkError::Gossip("Failed to receive peer list response".to_string()))
    }
}

impl Default for GossipManager {
    fn default() -> Self {
        Self::new().0
    }
}

/// Gossip message handler
pub struct GossipHandler {
    /// Set of seen message hashes to prevent loops
    seen_messages: HashSet<Vec<u8>>,
    /// Maximum number of seen messages to track
    max_seen_messages: usize,
}

impl GossipHandler {
    /// Create a new gossip handler
    pub fn new() -> Self {
        Self {
            seen_messages: HashSet::new(),
            max_seen_messages: 10_000,
        }
    }

    /// Check if a message has been seen before
    pub fn is_seen(&self, message_hash: &[u8]) -> bool {
        self.seen_messages.contains(message_hash)
    }

    /// Mark a message as seen
    pub fn mark_seen(&mut self, message_hash: Vec<u8>) {
        if self.seen_messages.len() >= self.max_seen_messages {
            // Remove oldest entries (simplified - in practice would use LRU)
            self.seen_messages.clear();
        }
        self.seen_messages.insert(message_hash);
    }

    /// Process an incoming gossip message
    pub async fn handle_message(
        &mut self,
        message: GossipMessage,
        peer_id: PeerId,
    ) -> NetworkResult<()> {
        // Calculate message hash for deduplication
        let message_bytes = serde_json::to_vec(&message).map_err(NetworkError::Json)?;
        let message_hash = blake3::hash(&message_bytes).as_bytes().to_vec();

        // Skip if already seen
        if self.is_seen(&message_hash) {
            return Ok(());
        }

        // Mark as seen
        self.mark_seen(message_hash);
        // Process message based on type
        match message {
            GossipMessage::BlockAnnounce(announce) => {
                self.handle_block_announce(*announce, peer_id).await
            }
            GossipMessage::TransactionPropagate(propagate) => {
                self.handle_transaction_propagate(propagate, peer_id).await
            }
        }
    }

    /// Handle block announcement
    async fn handle_block_announce(
        &self,
        announce: BlockAnnounce,
        peer_id: PeerId,
    ) -> NetworkResult<()> {
        tracing::info!(
            "Received block announcement: block #{} from peer {}",
            announce.block_number(),
            peer_id
        );

        // TODO: Validate block header
        // TODO: Forward to block processing
        // TODO: Request full block if needed

        Ok(())
    }

    /// Handle transaction propagation
    async fn handle_transaction_propagate(
        &self,
        propagate: TransactionPropagate,
        peer_id: PeerId,
    ) -> NetworkResult<()> {
        tracing::info!(
            "Received {} transactions from peer {}",
            propagate.len(),
            peer_id
        );

        // TODO: Validate transactions
        // TODO: Add to mempool
        // TODO: Re-propagate if valid

        Ok(())
    }
}

impl Default for GossipHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chain_core::{BlockHeader, Hash};
    #[tokio::test]
    async fn test_gossip_manager() {
        let (manager, mut rx) = GossipManager::new();

        // Test block announcement
        let header = BlockHeader::new(
            Hash::zero(),
            1,
            Hash::zero(),
            Hash::zero(),
            Hash::zero(),
            0,
            chrono::Utc::now().timestamp_millis() as u64,
            vec![],
            0,
            21000, // gas_limit
            0,     // gas_used
        );
        let announce = BlockAnnounce::new(header);

        // This should not block since the channel is unbounded
        manager.announce_block(announce).await.unwrap();

        // Check that command was sent
        if let Some(cmd) = rx.recv().await {
            match cmd {
                GossipCommand::Publish { topic, message } => {
                    assert_eq!(topic, "blocks");
                    match *message {
                        GossipMessage::BlockAnnounce(_) => {}
                        _ => panic!("Wrong message type"),
                    }
                }
                _ => panic!("Wrong command type"),
            }
        }
    }

    #[tokio::test]
    async fn test_gossip_handler() {
        let mut handler = GossipHandler::new();
        let peer_id = PeerId::random();
        // Create a test block announcement
        let header = BlockHeader::new(
            Hash::zero(),
            1,
            Hash::zero(),
            Hash::zero(),
            Hash::zero(),
            0,
            chrono::Utc::now().timestamp_millis() as u64,
            vec![],
            0,
            21000, // gas_limit
            0,     // gas_used
        );
        let announce = BlockAnnounce::new(header);
        let message = GossipMessage::BlockAnnounce(Box::new(announce));

        // Handle message first time - should succeed
        handler
            .handle_message(message.clone(), peer_id)
            .await
            .unwrap();

        // Handle same message again - should still succeed but be deduplicated
        handler.handle_message(message, peer_id).await.unwrap();
    }

    #[test]
    fn test_message_deduplication() {
        let mut handler = GossipHandler::new();
        let hash1 = vec![1, 2, 3];
        let hash2 = vec![4, 5, 6];

        assert!(!handler.is_seen(&hash1));
        handler.mark_seen(hash1.clone());
        assert!(handler.is_seen(&hash1));
        assert!(!handler.is_seen(&hash2));
    }
}
