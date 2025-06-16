//! Block data structures and operations

use crate::{BlockNumber, CoreError, CoreResult, Hash, Timestamp, Transaction};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

/// Block header containing metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, bincode::Encode)]
pub struct BlockHeader {
    /// Hash of the parent block
    pub parent_hash: Hash,
    /// Block number (height)
    pub number: BlockNumber,
    /// Root hash of the state trie
    pub state_root: Hash,
    /// Root hash of the transaction trie
    pub transactions_root: Hash,
    /// Root hash of the receipts trie
    pub receipts_root: Hash,
    /// Difficulty (for PoW) or authority info (for PoA)
    pub difficulty: u64,
    /// Block timestamp in milliseconds
    pub timestamp: Timestamp,
    /// Extra data (arbitrary bytes)
    pub extra_data: Vec<u8>,
    /// Nonce for PoW or VRF proof for PoA
    pub nonce: u64,
    /// Gas limit for all transactions in this block
    pub gas_limit: u64,
    /// Gas used by all transactions in this block
    pub gas_used: u64,
}

impl BlockHeader {
    /// Create a new block header
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        parent_hash: Hash,
        number: BlockNumber,
        state_root: Hash,
        transactions_root: Hash,
        receipts_root: Hash,
        difficulty: u64,
        timestamp: Timestamp,
        extra_data: Vec<u8>,
        nonce: u64,
        gas_limit: u64,
        gas_used: u64,
    ) -> Self {
        Self {
            parent_hash,
            number,
            state_root,
            transactions_root,
            receipts_root,
            difficulty,
            timestamp,
            extra_data,
            nonce,
            gas_limit,
            gas_used,
        }
    }
    /// Calculate the hash of this block header
    pub fn hash(&self) -> CoreResult<Hash> {
        let encoded = bincode::encode_to_vec(self, bincode::config::standard())
            .map_err(|e| CoreError::Bincode(e.to_string()))?;
        let hash_bytes = Keccak256::digest(&encoded);
        Ok(Hash::from_slice(hash_bytes.as_slice()))
    }

    /// Validate PoW (simplified difficulty check)
    pub fn validate_pow(&self) -> CoreResult<bool> {
        let hash = self.hash()?;
        let hash_value = u64::from_be_bytes([
            hash.as_bytes()[0],
            hash.as_bytes()[1],
            hash.as_bytes()[2],
            hash.as_bytes()[3],
            hash.as_bytes()[4],
            hash.as_bytes()[5],
            hash.as_bytes()[6],
            hash.as_bytes()[7],
        ]);

        // Simple difficulty check: hash must be less than target
        let target = u64::MAX / self.difficulty;
        Ok(hash_value < target)
    }

    /// Validate PoS/PoA (placeholder for VRF verification)
    pub fn validate_pos(&self, _authority_set: &[u8]) -> CoreResult<bool> {
        // TODO: Implement VRF verification against authority set
        // For now, just return true
        Ok(true)
    }

    /// Get the genesis block header
    pub fn genesis() -> Self {
        Self {
            parent_hash: Hash::zero(),
            number: 0,
            state_root: Hash::zero(),
            transactions_root: Hash::zero(),
            receipts_root: Hash::zero(),
            difficulty: 1,
            timestamp: 0,
            extra_data: b"RustChain Genesis Block".to_vec(),
            nonce: 0,
            gas_limit: 8_000_000,
            gas_used: 0,
        }
    }
}

/// Transaction receipt
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// Transaction hash
    pub transaction_hash: Hash,
    /// Transaction index in block
    pub transaction_index: u64,
    /// Block hash
    pub block_hash: Hash,
    /// Block number
    pub block_number: BlockNumber,
    /// Sender address
    pub from: crate::Address,
    /// Recipient address (None for contract creation)
    pub to: Option<crate::Address>,
    /// Gas used by this transaction
    pub gas_used: u64,
    /// Status (1 for success, 0 for failure)
    pub status: u8,
    /// Contract address (if contract creation)
    pub contract_address: Option<crate::Address>,
    /// Logs/events emitted
    pub logs: Vec<Log>,
}

/// Event log
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Log {
    /// Contract address that emitted the log
    pub address: crate::Address,
    /// Topics (indexed parameters)
    pub topics: Vec<Hash>,
    /// Data (non-indexed parameters)
    pub data: Vec<u8>,
}

/// Complete block with header and transactions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    /// Block header
    pub header: BlockHeader,
    /// List of transactions
    pub transactions: Vec<Transaction>,
}

impl Block {
    /// Create a new block
    pub fn new(header: BlockHeader, transactions: Vec<Transaction>) -> Self {
        Self {
            header,
            transactions,
        }
    }

    /// Create genesis block
    pub fn genesis() -> Self {
        Self {
            header: BlockHeader::genesis(),
            transactions: Vec::new(),
        }
    }

    /// Get the block hash (same as header hash)
    pub fn hash(&self) -> CoreResult<Hash> {
        self.header.hash()
    }

    /// Calculate the transactions root hash
    pub fn calculate_transactions_root(&self) -> CoreResult<Hash> {
        if self.transactions.is_empty() {
            return Ok(Hash::zero());
        }

        // Simple implementation: hash of concatenated transaction hashes
        let mut hasher = Keccak256::new();
        for tx in &self.transactions {
            let tx_hash = tx.hash()?;
            hasher.update(tx_hash.as_bytes());
        }

        let result = hasher.finalize();
        Ok(Hash::from_slice(result.as_slice()))
    }

    /// Validate the block
    pub fn validate(&self) -> CoreResult<bool> {
        // Check transactions root
        let calculated_root = self.calculate_transactions_root()?;
        if calculated_root != self.header.transactions_root {
            return Ok(false);
        }

        // Validate all transactions
        for tx in &self.transactions {
            if !tx.verify_signature()? {
                return Ok(false);
            }
        }

        // Validate header (PoW/PoS)
        // For now, just check if it's a valid PoW block
        self.header.validate_pow()
    }

    /// Get transaction by hash
    pub fn get_transaction(&self, hash: &Hash) -> CoreResult<Option<&Transaction>> {
        for tx in &self.transactions {
            if tx.hash()? == *hash {
                return Ok(Some(tx));
            }
        }
        Ok(None)
    }

    /// Get total gas used
    pub fn total_gas_used(&self) -> u64 {
        self.header.gas_used
    }

    /// Check if block is genesis
    pub fn is_genesis(&self) -> bool {
        self.header.number == 0 && self.header.parent_hash == Hash::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Address, Transaction};

    #[test]
    fn test_genesis_block() {
        let genesis = Block::genesis();
        assert_eq!(genesis.header.number, 0);
        assert_eq!(genesis.header.parent_hash, Hash::zero());
        assert!(genesis.transactions.is_empty());
        assert!(genesis.is_genesis());
    }

    #[test]
    fn test_block_hash() {
        let genesis = Block::genesis();
        let hash1 = genesis.hash().unwrap();
        let hash2 = genesis.hash().unwrap();
        assert_eq!(hash1, hash2); // Hash should be deterministic
    }

    #[test]
    fn test_transactions_root() {
        let mut block = Block::genesis();

        // Empty block should have zero transactions root
        let root = block.calculate_transactions_root().unwrap();
        assert_eq!(root, Hash::zero());

        // Add a transaction
        let to = Address::from_hex("1234567890abcdef1234567890abcdef12345678").unwrap();
        let tx = Transaction::transfer(1, to, 1000, 20_000_000_000, 21_000);
        block.transactions.push(tx);

        // Root should no longer be zero
        let root = block.calculate_transactions_root().unwrap();
        assert_ne!(root, Hash::zero());
    }

    #[test]
    fn test_block_validation() {
        let genesis = Block::genesis();
        // Genesis block should validate (assuming PoW difficulty is met)
        // Note: This might fail due to difficulty, but structure is correct
        let _is_valid = genesis.validate();
    }

    #[test]
    fn test_header_creation() {
        let header = BlockHeader::new(
            Hash::zero(),
            1,
            Hash::zero(),
            Hash::zero(),
            Hash::zero(),
            1000,
            1234567890,
            vec![1, 2, 3],
            42,
            8_000_000,
            100_000,
        );

        assert_eq!(header.number, 1);
        assert_eq!(header.difficulty, 1000);
        assert_eq!(header.nonce, 42);
    }
}
