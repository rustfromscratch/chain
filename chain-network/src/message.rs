//! Network message types and protocols

use chain_core::{Block, BlockHeader, Hash, Transaction};
use serde::{Deserialize, Serialize};

/// Protocol identifiers
pub mod protocols {
    /// Block announcement gossip
    pub const BLOCK_ANNOUNCE: &str = "/chain/block/announce/1.0.0";
    /// Transaction propagation gossip
    pub const TX_PROPAGATE: &str = "/chain/tx/propagate/1.0.0";
    /// Sync request-response
    pub const SYNC_REQUEST: &str = "/chain/sync/request/1.0.0";
    /// State sync
    pub const STATE_SYNC: &str = "/chain/state/sync/1.0.0";
}

/// Messages sent over the gossip network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    /// Block announcement
    BlockAnnounce(Box<BlockAnnounce>),
    /// Transaction propagation
    TransactionPropagate(TransactionPropagate),
}

/// Block announcement message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockAnnounce {
    /// Block header
    pub header: BlockHeader,
    /// Optional full block (if small enough)
    pub block: Option<Block>,
}

impl BlockAnnounce {
    pub fn new(header: BlockHeader) -> Self {
        Self {
            header,
            block: None,
        }
    }

    pub fn with_block(mut self, block: Block) -> Self {
        self.block = Some(block);
        self
    }
    pub fn block_hash(&self) -> Hash {
        self.header.hash().unwrap_or_else(|_| Hash::zero())
    }

    pub fn block_number(&self) -> u64 {
        self.header.number
    }
}

/// Transaction propagation message
#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode)]
pub struct TransactionPropagate {
    /// Transactions to propagate
    pub transactions: Vec<Transaction>,
}

impl TransactionPropagate {
    pub fn new(transactions: Vec<Transaction>) -> Self {
        Self { transactions }
    }

    pub fn single(transaction: Transaction) -> Self {
        Self {
            transactions: vec![transaction],
        }
    }

    pub fn len(&self) -> usize {
        self.transactions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }
}

/// Sync request-response messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    Request(SyncRequest),
    Response(SyncResponse),
}

/// Sync request types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncRequest {
    /// Get block headers
    GetHeaders {
        /// Starting hash
        start: Hash,
        /// Number of headers to retrieve
        amount: u32,
        /// Skip interval (for light sync)
        skip: u32,
        /// Request in reverse order
        reverse: bool,
    },
    /// Get block bodies
    GetBodies {
        /// Block hashes
        hashes: Vec<Hash>,
    },
    /// Get receipts
    GetReceipts {
        /// Block hashes
        hashes: Vec<Hash>,
    },
    /// Get state snapshot
    GetStateSnapshot {
        /// Root hash
        root: Hash,
        /// Key prefix
        prefix: Vec<u8>,
        /// Maximum number of entries
        limit: u32,
    },
}

/// Sync response types  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncResponse {
    /// Block headers response
    Headers {
        /// Block headers
        headers: Vec<BlockHeader>,
    },
    /// Block bodies response
    Bodies {
        /// Block bodies (transactions)
        bodies: Vec<Vec<Transaction>>,
    },
    /// Receipts response
    Receipts {
        /// Transaction receipts (placeholder for now)
        receipts: Vec<Vec<u8>>,
    },
    /// State snapshot response
    StateSnapshot {
        /// State entries
        entries: Vec<(Vec<u8>, Vec<u8>)>,
        /// Whether this is the last chunk
        complete: bool,
    },
    /// Error response
    Error {
        /// Error message
        message: String,
    },
}

impl SyncRequest {
    /// Create a header request
    pub fn headers(start: Hash, amount: u32) -> Self {
        Self::GetHeaders {
            start,
            amount,
            skip: 0,
            reverse: false,
        }
    }

    /// Create a bodies request
    pub fn bodies(hashes: Vec<Hash>) -> Self {
        Self::GetBodies { hashes }
    }

    /// Create a receipts request
    pub fn receipts(hashes: Vec<Hash>) -> Self {
        Self::GetReceipts { hashes }
    }

    /// Create a state snapshot request
    pub fn state_snapshot(root: Hash, prefix: Vec<u8>, limit: u32) -> Self {
        Self::GetStateSnapshot {
            root,
            prefix,
            limit,
        }
    }
}

impl SyncResponse {
    /// Create a headers response
    pub fn headers(headers: Vec<BlockHeader>) -> Self {
        Self::Headers { headers }
    }

    /// Create a bodies response
    pub fn bodies(bodies: Vec<Vec<Transaction>>) -> Self {
        Self::Bodies { bodies }
    }

    /// Create an error response
    pub fn error(message: String) -> Self {
        Self::Error { message }
    }
}

/// Message size limits
pub mod limits {
    /// Maximum gossip message size (128 KB)
    pub const MAX_GOSSIP_MESSAGE_SIZE: usize = 128 * 1024;
    /// Maximum sync request size (32 KB)
    pub const MAX_SYNC_REQUEST_SIZE: usize = 32 * 1024;
    /// Maximum sync response size (1 MB)
    pub const MAX_SYNC_RESPONSE_SIZE: usize = 1024 * 1024;
    /// Maximum number of headers per request
    pub const MAX_HEADERS_PER_REQUEST: u32 = 192;
    /// Maximum number of bodies per request
    pub const MAX_BODIES_PER_REQUEST: usize = 32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chain_core::{Address, Hash};
    #[test]
    fn test_block_announce() {
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

        let announce = BlockAnnounce::new(header.clone());
        assert_eq!(announce.block_number(), 1);
        assert_eq!(announce.block_hash(), header.hash().unwrap());
    }
    #[test]
    fn test_transaction_propagate() {
        let tx = Transaction::new(
            0,                     // nonce
            1000,                  // gas_price
            21000,                 // gas_limit
            Some(Address::zero()), // to
            0,                     // value
            vec![],                // data
        );

        let propagate = TransactionPropagate::single(tx);
        assert_eq!(propagate.len(), 1);
        assert!(!propagate.is_empty());
    }

    #[test]
    fn test_sync_requests() {
        let start_hash = Hash::zero();

        let headers_req = SyncRequest::headers(start_hash, 10);
        match headers_req {
            SyncRequest::GetHeaders { start, amount, .. } => {
                assert_eq!(start, start_hash);
                assert_eq!(amount, 10);
            }
            _ => panic!("Wrong request type"),
        }

        let bodies_req = SyncRequest::bodies(vec![start_hash]);
        match bodies_req {
            SyncRequest::GetBodies { hashes } => {
                assert_eq!(hashes.len(), 1);
                assert_eq!(hashes[0], start_hash);
            }
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_sync_responses() {
        let headers = vec![];
        let response = SyncResponse::headers(headers);

        match response {
            SyncResponse::Headers { headers } => {
                assert!(headers.is_empty());
            }
            _ => panic!("Wrong response type"),
        }

        let error_response = SyncResponse::error("Test error".to_string());
        match error_response {
            SyncResponse::Error { message } => {
                assert_eq!(message, "Test error");
            }
            _ => panic!("Wrong response type"),
        }
    }
}
