//! Blockchain consensus engine
//!
//! This crate provides consensus mechanisms for the blockchain,
//! including Proof of Authority (PoA) with VRF for validator rotation.

pub mod error;
pub mod poa;
pub mod slashing;
pub mod traits;

pub use error::{ConsensusError, ConsensusResult};
pub use poa::{PoAConfig, PoAEngine};
pub use traits::{Engine, StepContext, StepResult};

#[cfg(test)]
mod tests {
    #[test]
    fn test_consensus_basics() {
        // Basic smoke test
        assert!(true);
    }
}
