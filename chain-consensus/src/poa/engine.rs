//! PoA consensus engine implementation

use crate::poa::{PoAConfig, VrfSelector, VrfSeed};
use crate::slashing::{SlashingDetector, SlashingOffence};
use crate::traits::{AuthoritySet, Engine, StepContext, StepResult};
use crate::{ConsensusError, ConsensusResult};
use chain_core::{BlockHeader, Hash};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// PoA consensus engine state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoAState {
    /// Waiting for slot
    Waiting,
    /// Proposing a block
    Proposing,
    /// Validating proposals
    Validating,
}

/// PoA consensus engine
pub struct PoAEngine {
    /// Configuration
    config: PoAConfig,
    /// Current authority set
    authority_set: Arc<RwLock<AuthoritySet>>,
    /// VRF selector for validator rotation
    vrf_selector: VrfSelector,
    /// Current engine state
    state: PoAState,
    /// Current round/slot
    current_slot: u64,
    /// Slashing detector
    slashing_detector: SlashingDetector,
    /// Local validator index (if this node is a validator)
    local_validator_index: Option<usize>,
    /// Genesis timestamp
    genesis_timestamp: u64,
    /// Event sender for notifications
    event_sender: Option<mpsc::UnboundedSender<ConsensusEvent>>,
}

/// Consensus events
#[derive(Debug, Clone)]
pub enum ConsensusEvent {
    /// New slot started
    SlotStarted { slot: u64, validator: Option<usize> },
    /// Block should be proposed
    ShouldPropose { slot: u64 },
    /// Block received for validation
    BlockReceived { header: BlockHeader },
    /// Slashing offence detected
    SlashingDetected { offence: SlashingOffence },
}

impl PoAEngine {
    /// Create a new PoA engine
    pub fn new(
        config: PoAConfig,
        local_validator_address: Option<chain_core::Address>,
        genesis_timestamp: u64,
    ) -> ConsensusResult<Self> {
        config.validate()?;

        let authority_set = Arc::new(RwLock::new(config.to_authority_set(0)?));
        let vrf_selector = VrfSelector::new(
            VrfSeed::from_bytes(config.vrf_seed),
            config.authorities.len(),
        );

        // Find local validator index
        let local_validator_index = if let Some(addr) = local_validator_address {
            authority_set.read().unwrap().get_validator_index(&addr)
        } else {
            None
        };

        if local_validator_index.is_some() {
            info!("Local node is validator #{:?}", local_validator_index);
        } else {
            info!("Local node is not a validator");
        }

        Ok(Self {
            config,
            authority_set,
            vrf_selector,
            state: PoAState::Waiting,
            current_slot: 0,
            slashing_detector: SlashingDetector::new(10), // Allow 10 missed slots
            local_validator_index,
            genesis_timestamp,
            event_sender: None,
        })
    }

    /// Set event sender for notifications
    pub fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<ConsensusEvent>) {
        self.event_sender = Some(sender);
    }

    /// Get current slot from timestamp
    pub fn current_slot_from_timestamp(&self, timestamp: u64) -> u64 {
        if timestamp < self.genesis_timestamp {
            return 0;
        }
        (timestamp - self.genesis_timestamp) / self.config.slot_duration
    }

    /// Get timestamp for a slot
    pub fn slot_timestamp(&self, slot: u64) -> u64 {
        self.genesis_timestamp + slot * self.config.slot_duration
    }

    /// Check if we are the proposer for a given slot
    pub fn is_proposer_for_slot(&self, slot: u64) -> bool {
        if let Some(local_index) = self.local_validator_index {
            let expected_proposer = self.vrf_selector.select_validator(slot);
            expected_proposer == local_index
        } else {
            false
        }
    }

    /// Get the expected proposer for a slot
    pub fn get_proposer_for_slot(&self, slot: u64) -> usize {
        self.vrf_selector.select_validator(slot)
    }

    /// Update authority set (hot-swappable)
    pub fn update_authorities(&mut self, new_config: PoAConfig) -> ConsensusResult<()> {
        new_config.validate()?;
        
        let new_authority_set = new_config.to_authority_set(
            self.authority_set.read().unwrap().epoch + 1
        )?;

        info!("Updating authority set to epoch {}", new_authority_set.epoch);
        
        // Update VRF selector with new validator count
        self.vrf_selector = VrfSelector::new(
            VrfSeed::from_bytes(new_config.vrf_seed),
            new_config.authorities.len(),
        );

        // Update local validator index
        if let Some(local_addr) = self.get_local_validator_address() {
            self.local_validator_index = new_authority_set.get_validator_index(&local_addr);
        }

        // Update authority set
        *self.authority_set.write().unwrap() = new_authority_set;
        self.config = new_config;

        Ok(())
    }

    /// Get local validator address if this node is a validator
    fn get_local_validator_address(&self) -> Option<chain_core::Address> {
        if let Some(index) = self.local_validator_index {
            self.authority_set.read().unwrap()
                .get_validator(index)
                .map(|v| v.address)
        } else {
            None
        }
    }

    /// Send event notification
    fn send_event(&self, event: ConsensusEvent) {
        if let Some(sender) = &self.event_sender {
            if let Err(e) = sender.send(event) {
                warn!("Failed to send consensus event: {}", e);
            }
        }
    }

    /// Process a received block
    pub fn process_block(&mut self, header: BlockHeader) -> ConsensusResult<()> {
        debug!("Processing block #{} with hash {:?}", header.number, header.hash());

        // Verify the block
        self.verify_block(&header)?;

        // Check for slashing
        let proposer_slot = self.current_slot_from_timestamp(header.timestamp);
        let expected_proposer = self.get_proposer_for_slot(proposer_slot);

        if let Some(offence) = self.slashing_detector.record_signature(expected_proposer, header.clone())? {
            warn!("Slashing offence detected: {:?}", offence);
            self.send_event(ConsensusEvent::SlashingDetected { offence });
        }

        // Reset missed slots for the proposer
        self.slashing_detector.reset_missed_slots(expected_proposer);

        self.send_event(ConsensusEvent::BlockReceived { header });
        Ok(())
    }
}

impl Engine for PoAEngine {
    fn step(&mut self, ctx: StepContext) -> ConsensusResult<StepResult> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let current_slot = self.current_slot_from_timestamp(now);
        
        // Update current slot
        if current_slot > self.current_slot {
            self.current_slot = current_slot;
            debug!("Advanced to slot {}", current_slot);

            // Check for missed slots by validators
            for slot in (self.current_slot - 1)..current_slot {
                let expected_proposer = self.get_proposer_for_slot(slot);
                if let Some(offence) = self.slashing_detector.record_missed_slot(expected_proposer) {
                    warn!("Validator {} missed too many slots", expected_proposer);
                    self.send_event(ConsensusEvent::SlashingDetected { offence });
                }
            }

            self.send_event(ConsensusEvent::SlotStarted {
                slot: current_slot,
                validator: if self.is_proposer_for_slot(current_slot) {
                    self.local_validator_index
                } else {
                    None
                },
            });
        }

        // Check if we should propose
        if self.should_propose(&ctx) {
            self.state = PoAState::Proposing;
            self.send_event(ConsensusEvent::ShouldPropose { slot: current_slot });

            // Create block header
            let header = BlockHeader {
                parent_hash: ctx.parent_hash,
                number: ctx.block_number,
                state_root: Hash::zero(), // Will be filled by state execution
                transactions_root: Hash::zero(),    // Will be filled by transaction processing
                receipts_root: Hash::zero(), // Will be filled by receipt processing
                gas_limit: 1000000, // Default gas limit
                gas_used: 0, // Will be filled after execution
                difficulty: 1, // PoA doesn't use difficulty
                timestamp: now,
                extra_data: vec![], // Could include VRF proof
                nonce: current_slot, // Use slot as nonce
            };

            return Ok(StepResult::Propose {
                header,
                timeout: self.config.slot_duration_as_duration(),
            });
        }

        // Calculate time to next slot
        let next_slot = current_slot + 1;
        let next_slot_time = self.slot_timestamp(next_slot);
        let timeout = Duration::from_secs(next_slot_time.saturating_sub(now).max(1));

        match self.state {
            PoAState::Waiting => Ok(StepResult::Continue { timeout }),
            PoAState::Proposing => {
                self.state = PoAState::Waiting;
                Ok(StepResult::Wait { timeout })
            }
            PoAState::Validating => Ok(StepResult::Continue { timeout }),
        }
    }

    fn verify_block(&self, header: &BlockHeader) -> ConsensusResult<()> {
        debug!("Verifying block #{}", header.number);

        // Check timestamp is reasonable
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if header.timestamp > now + self.config.slot_duration {
            return Err(ConsensusError::InvalidTimestamp {
                expected: now,
                actual: header.timestamp,
            });
        }

        if header.timestamp < self.genesis_timestamp {
            return Err(ConsensusError::InvalidTimestamp {
                expected: self.genesis_timestamp,
                actual: header.timestamp,
            });
        }

        // Verify the proposer
        let slot = self.current_slot_from_timestamp(header.timestamp);
        let expected_proposer = self.get_proposer_for_slot(slot);
        
        // In a real implementation, we would verify the signature/VRF proof here
        // For now, we just check the slot timing
        let expected_timestamp = self.slot_timestamp(slot);
        let tolerance = self.config.slot_duration / 2; // Allow some clock drift

        if header.timestamp < expected_timestamp.saturating_sub(tolerance) ||
           header.timestamp > expected_timestamp + tolerance {
            return Err(ConsensusError::InvalidTimestamp {
                expected: expected_timestamp,
                actual: header.timestamp,
            });
        }

        debug!("Block #{} verified successfully (proposer: {})", header.number, expected_proposer);
        Ok(())
    }

    fn current_round(&self) -> u64 {
        self.current_slot
    }

    fn should_propose(&self, _ctx: &StepContext) -> bool {
        if self.local_validator_index.is_none() {
            return false;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let current_slot = self.current_slot_from_timestamp(now);
        self.is_proposer_for_slot(current_slot) && self.state == PoAState::Waiting
    }

    fn expected_proposer(&self, slot: u64) -> Option<usize> {
        Some(self.get_proposer_for_slot(slot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poa::config::default_test_authorities;

    fn create_test_engine() -> PoAEngine {
        let config = PoAConfig {
            slot_duration: 3,
            authorities: default_test_authorities(),
            vrf_seed: [1u8; 32],
            epoch_length: 100,
        };

        let genesis_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        PoAEngine::new(config, None, genesis_time).unwrap()
    }

    #[test]
    fn test_poa_engine_creation() {
        let engine = create_test_engine();
        assert_eq!(engine.state, PoAState::Waiting);
        assert_eq!(engine.current_slot, 0);
        assert!(engine.local_validator_index.is_none());
    }

    #[test]
    fn test_slot_calculation() {
        let engine = create_test_engine();
        let genesis = engine.genesis_timestamp;

        assert_eq!(engine.current_slot_from_timestamp(genesis), 0);
        assert_eq!(engine.current_slot_from_timestamp(genesis + 3), 1);
        assert_eq!(engine.current_slot_from_timestamp(genesis + 6), 2);
    }

    #[test]
    fn test_proposer_selection() {
        let engine = create_test_engine();
        
        // Same slot should always return same proposer
        let proposer1 = engine.get_proposer_for_slot(10);
        let proposer2 = engine.get_proposer_for_slot(10);
        assert_eq!(proposer1, proposer2);

        // Proposer should be in valid range
        assert!(proposer1 < 3); // We have 3 test authorities
    }

    #[test]
    fn test_block_verification() {
        let engine = create_test_engine();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let header = BlockHeader {
            parent_hash: Hash::zero(),
            number: 1,
            state_root: Hash::zero(),            transactions_root: Hash::zero(),
            receipts_root: Hash::zero(),
            gas_limit: 1000000,
            gas_used: 0,
            difficulty: 1,
            timestamp: now,
            extra_data: vec![],
            nonce: 0,
        };

        // Should verify successfully
        assert!(engine.verify_block(&header).is_ok());

        // Future timestamp should fail
        let mut future_header = header.clone();
        future_header.timestamp = now + 3600; // 1 hour in future
        assert!(engine.verify_block(&future_header).is_err());

        // Past timestamp should fail
        let mut past_header = header.clone();
        past_header.timestamp = engine.genesis_timestamp - 1;
        assert!(engine.verify_block(&past_header).is_err());
    }

    #[test]
    fn test_authority_update() {
        let mut engine = create_test_engine();
        let original_count = engine.authority_set.read().unwrap().len();

        // Create new config with different authorities
        let mut new_config = engine.config.clone();
        new_config.authorities.push(crate::poa::config::AuthorityConfig {
            address: "0x4567890123456789012345678901234567890123".to_string(),
            weight: 1,
        });

        // Update authorities
        engine.update_authorities(new_config).unwrap();

        // Should have more authorities now
        let new_count = engine.authority_set.read().unwrap().len();
        assert_eq!(new_count, original_count + 1);
    }
}
