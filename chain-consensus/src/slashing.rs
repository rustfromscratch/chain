//! Slashing detection and penalty mechanisms

use crate::{ConsensusError, ConsensusResult};
use chain_core::BlockHeader;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Evidence of double signing
#[derive(Debug, Clone)]
pub struct DoubleSignEvidence {
    /// Validator index who double signed
    pub validator_index: usize,
    /// First signed block header
    pub header1: BlockHeader,
    /// Second signed block header (same height)
    pub header2: BlockHeader,
    /// Timestamp when evidence was detected
    pub detected_at: u64,
}

/// Slashing offence types
#[derive(Debug, Clone)]
pub enum SlashingOffence {
    /// Double signing at the same height
    DoubleSigning(DoubleSignEvidence),
    /// Being offline for too long
    Offline {
        validator_index: usize,
        missed_slots: u64,
    },
}

/// Tracks validator behavior for slashing detection
#[derive(Debug)]
pub struct SlashingDetector {
    /// Track signed blocks by validator and block number
    signed_blocks: HashMap<(usize, u64), BlockHeader>,
    /// Track missed slots
    missed_slots: HashMap<usize, u64>,
    /// Maximum allowed missed slots before slashing
    max_missed_slots: u64,
}

impl SlashingDetector {
    /// Create a new slashing detector
    pub fn new(max_missed_slots: u64) -> Self {
        Self {
            signed_blocks: HashMap::new(),
            missed_slots: HashMap::new(),
            max_missed_slots,
        }
    }

    /// Record a block signature and check for double signing
    pub fn record_signature(
        &mut self,
        validator_index: usize,
        header: BlockHeader,
    ) -> ConsensusResult<Option<SlashingOffence>> {
        let block_number = header.number;
        let key = (validator_index, block_number);

        // Check if validator already signed a block at this height
        if let Some(existing_header) = self.signed_blocks.get(&key) {
            // Check if it's the same block (same hash)
            let existing_hash = existing_header.hash().map_err(|e| {
                ConsensusError::Other(format!("Failed to hash existing header: {}", e))
            })?;
            let new_hash = header.hash().map_err(|e| {
                ConsensusError::Other(format!("Failed to hash new header: {}", e))
            })?;

            if existing_hash != new_hash {
                // Double signing detected!
                let evidence = DoubleSignEvidence {
                    validator_index,
                    header1: existing_header.clone(),
                    header2: header.clone(),
                    detected_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                };
                return Ok(Some(SlashingOffence::DoubleSigning(evidence)));
            }
        } else {
            // Record the signature
            self.signed_blocks.insert(key, header);
        }

        Ok(None)
    }

    /// Record a missed slot for a validator
    pub fn record_missed_slot(&mut self, validator_index: usize) -> Option<SlashingOffence> {
        let missed = self.missed_slots.entry(validator_index).or_insert(0);
        *missed += 1;

        if *missed >= self.max_missed_slots {
            Some(SlashingOffence::Offline {
                validator_index,
                missed_slots: *missed,
            })
        } else {
            None
        }
    }

    /// Reset missed slots for a validator (called when they produce a block)
    pub fn reset_missed_slots(&mut self, validator_index: usize) {
        self.missed_slots.remove(&validator_index);
    }

    /// Get current missed slots for a validator
    pub fn get_missed_slots(&self, validator_index: usize) -> u64 {
        self.missed_slots.get(&validator_index).copied().unwrap_or(0)
    }

    /// Clean up old records to prevent memory leaks
    pub fn cleanup_old_records(&mut self, current_block: u64, keep_blocks: u64) {
        let cutoff = current_block.saturating_sub(keep_blocks);
        
        self.signed_blocks.retain(|(_, block_number), _| *block_number > cutoff);
    }
}

/// Detect double signing from block headers
pub fn detect_double_sign(
    headers: &[BlockHeader],
    validator_index: usize,
) -> ConsensusResult<Option<DoubleSignEvidence>> {
    let mut blocks_by_height: HashMap<u64, Vec<&BlockHeader>> = HashMap::new();

    // Group headers by block number
    for header in headers {
        blocks_by_height
            .entry(header.number)
            .or_insert_with(Vec::new)
            .push(header);
    }    // Check for multiple blocks at the same height
    for (_block_number, headers_at_height) in blocks_by_height {
        if headers_at_height.len() > 1 {
            // Found potential double signing
            let header1 = headers_at_height[0];
            let header2 = headers_at_height[1];

            // Verify they are different blocks
            let hash1 = header1.hash().map_err(|e| {
                ConsensusError::Other(format!("Failed to hash header1: {}", e))
            })?;
            let hash2 = header2.hash().map_err(|e| {
                ConsensusError::Other(format!("Failed to hash header2: {}", e))
            })?;

            if hash1 != hash2 {
                return Ok(Some(DoubleSignEvidence {
                    validator_index,
                    header1: (*header1).clone(),
                    header2: (*header2).clone(),
                    detected_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                }));
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use chain_core::{BlockHeader, Hash};fn create_test_header(number: u64, nonce: u64) -> BlockHeader {
        BlockHeader {
            parent_hash: Hash::zero(),
            number,
            state_root: Hash::zero(),
            transactions_root: Hash::zero(),
            receipts_root: Hash::zero(),
            gas_limit: 1000000,
            gas_used: 0,
            difficulty: 1,
            timestamp: 1000000 + number,
            extra_data: vec![],
            nonce,
        }
    }

    #[test]
    fn test_slashing_detector_basic() {
        let mut detector = SlashingDetector::new(5);
        let header = create_test_header(1, 123);

        // First signature should be fine
        let result = detector.record_signature(0, header.clone()).unwrap();
        assert!(result.is_none());

        // Same signature should be fine too
        let result = detector.record_signature(0, header).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_double_signing_detection() {
        let mut detector = SlashingDetector::new(5);
        let header1 = create_test_header(1, 123);
        let header2 = create_test_header(1, 456); // Same height, different nonce

        // Record first signature
        let result = detector.record_signature(0, header1).unwrap();
        assert!(result.is_none());

        // Record second signature at same height - should detect double signing
        let result = detector.record_signature(0, header2).unwrap();
        assert!(result.is_some());

        match result.unwrap() {
            SlashingOffence::DoubleSigning(evidence) => {
                assert_eq!(evidence.validator_index, 0);
                assert_eq!(evidence.header1.number, 1);
                assert_eq!(evidence.header2.number, 1);
            }
            _ => panic!("Expected double signing evidence"),
        }
    }

    #[test]
    fn test_missed_slots() {
        let mut detector = SlashingDetector::new(3);

        // Record missed slots
        assert!(detector.record_missed_slot(0).is_none());
        assert!(detector.record_missed_slot(0).is_none());
        
        // Third missed slot should trigger slashing
        let result = detector.record_missed_slot(0);
        assert!(result.is_some());

        match result.unwrap() {
            SlashingOffence::Offline { validator_index, missed_slots } => {
                assert_eq!(validator_index, 0);
                assert_eq!(missed_slots, 3);
            }
            _ => panic!("Expected offline slashing"),
        }
    }
}
