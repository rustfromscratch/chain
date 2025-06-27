//! Consensus engine traits and types

use crate::ConsensusResult;
use chain_core::{BlockHeader, Hash};
use std::time::Duration;

/// Context for a consensus step
#[derive(Debug, Clone)]
pub struct StepContext {
    /// Current block number
    pub block_number: u64,
    /// Parent block hash
    pub parent_hash: Hash,
    /// Current timestamp
    pub timestamp: u64,
    /// Local node's validator index (if any)
    pub validator_index: Option<usize>,
}

/// Result of a consensus step
#[derive(Debug, Clone)]
pub enum StepResult {
    /// Continue consensus with new timeout
    Continue { timeout: Duration },
    /// Propose a new block
    Propose {
        /// Proposed block header
        header: BlockHeader,
        /// Timeout for proposal
        timeout: Duration,
    },
    /// Wait for the next round
    Wait { timeout: Duration },
    /// Consensus completed for this round
    Complete,
}

/// Main consensus engine trait
pub trait Engine: Send + Sync {
    /// Step the consensus engine forward
    fn step(&mut self, ctx: StepContext) -> ConsensusResult<StepResult>;

    /// Verify a block header
    fn verify_block(&self, header: &BlockHeader) -> ConsensusResult<()>;

    /// Get the current round/epoch
    fn current_round(&self) -> u64;

    /// Check if this node should propose a block
    fn should_propose(&self, ctx: &StepContext) -> bool;

    /// Get the expected proposer for a given slot
    fn expected_proposer(&self, slot: u64) -> Option<usize>;
}

/// Validator information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validator {
    /// Validator address/public key
    pub address: chain_core::Address,
    /// Validator weight (voting power)
    pub weight: u64,
}

/// Authority set for consensus
#[derive(Debug, Clone)]
pub struct AuthoritySet {
    /// List of validators
    pub validators: Vec<Validator>,
    /// Current epoch
    pub epoch: u64,
    /// Set ID for tracking changes
    pub set_id: u64,
}

impl AuthoritySet {
    /// Create a new authority set
    pub fn new(validators: Vec<Validator>, epoch: u64) -> Self {
        Self {
            validators,
            epoch,
            set_id: 0,
        }
    }

    /// Get validator by index
    pub fn get_validator(&self, index: usize) -> Option<&Validator> {
        self.validators.get(index)
    }

    /// Get validator index by address
    pub fn get_validator_index(&self, address: &chain_core::Address) -> Option<usize> {
        self.validators
            .iter()
            .position(|v| &v.address == address)
    }

    /// Get total number of validators
    pub fn len(&self) -> usize {
        self.validators.len()
    }

    /// Check if authority set is empty
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }

    /// Get total weight
    pub fn total_weight(&self) -> u64 {
        self.validators.iter().map(|v| v.weight).sum()
    }
}
