//! PoA consensus configuration

use crate::traits::{AuthoritySet, Validator};
use crate::{ConsensusError, ConsensusResult};
use chain_core::Address;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Duration;

/// PoA consensus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoAConfig {
    /// Slot duration in seconds
    pub slot_duration: u64,
    /// Authority set
    pub authorities: Vec<AuthorityConfig>,
    /// VRF seed for randomness
    pub vrf_seed: [u8; 32],
    /// Epoch length in slots
    pub epoch_length: u64,
}

/// Authority configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorityConfig {
    /// Validator address
    pub address: String,
    /// Validator weight (voting power)
    pub weight: u64,
}

impl Default for PoAConfig {
    fn default() -> Self {
        Self {
            slot_duration: 3, // 3 seconds per slot
            authorities: vec![],
            vrf_seed: [0u8; 32],
            epoch_length: 100, // 100 slots per epoch
        }
    }
}

impl PoAConfig {
    /// Create a new PoA configuration
    pub fn new(slot_duration: u64, authorities: Vec<AuthorityConfig>) -> Self {
        Self {
            slot_duration,
            authorities,
            vrf_seed: rand::random(),
            epoch_length: 100,
        }
    }

    /// Load configuration from file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> ConsensusResult<Self> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| ConsensusError::Config(format!("Failed to read config file: {}", e)))?;
        
        let config: PoAConfig = serde_json::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> ConsensusResult<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path.as_ref(), content)
            .map_err(|e| ConsensusError::Config(format!("Failed to write config file: {}", e)))?;
        Ok(())
    }

    /// Validate the configuration
    pub fn validate(&self) -> ConsensusResult<()> {
        if self.slot_duration == 0 {
            return Err(ConsensusError::Config(
                "Slot duration must be greater than 0".to_string(),
            ));
        }

        if self.authorities.is_empty() {
            return Err(ConsensusError::Config(
                "At least one authority is required".to_string(),
            ));
        }

        if self.epoch_length == 0 {
            return Err(ConsensusError::Config(
                "Epoch length must be greater than 0".to_string(),
            ));
        }

        // Validate authority addresses
        for (i, authority) in self.authorities.iter().enumerate() {
            if authority.address.len() != 42 || !authority.address.starts_with("0x") {
                return Err(ConsensusError::Config(format!(
                    "Invalid address format for authority {}: {}",
                    i, authority.address
                )));
            }

            if authority.weight == 0 {
                return Err(ConsensusError::Config(format!(
                    "Authority {} weight must be greater than 0",
                    i
                )));
            }
        }

        Ok(())
    }

    /// Convert to authority set
    pub fn to_authority_set(&self, epoch: u64) -> ConsensusResult<AuthoritySet> {
        let validators: Result<Vec<Validator>, _> = self
            .authorities
            .iter()
            .map(|auth| {
                let address_bytes = hex::decode(&auth.address[2..])
                    .map_err(|e| {
                        ConsensusError::Config(format!("Invalid hex address: {}", e))
                    })?;
                
                if address_bytes.len() != 20 {
                    return Err(ConsensusError::Config(
                        "Address must be 20 bytes".to_string(),
                    ));
                }

                let mut addr = [0u8; 20];
                addr.copy_from_slice(&address_bytes);                Ok(Validator {
                    address: Address::from_slice(&addr),
                    weight: auth.weight,
                })
            })
            .collect();

        Ok(AuthoritySet::new(validators?, epoch))
    }

    /// Get slot duration as Duration
    pub fn slot_duration_as_duration(&self) -> Duration {
        Duration::from_secs(self.slot_duration)
    }

    /// Set VRF seed
    pub fn with_vrf_seed(mut self, seed: [u8; 32]) -> Self {
        self.vrf_seed = seed;
        self
    }

    /// Set epoch length
    pub fn with_epoch_length(mut self, length: u64) -> Self {
        self.epoch_length = length;
        self
    }
}

/// Default authorities configuration for testing
pub fn default_test_authorities() -> Vec<AuthorityConfig> {
    vec![
        AuthorityConfig {
            address: "0x1234567890123456789012345678901234567890".to_string(),
            weight: 1,
        },
        AuthorityConfig {
            address: "0x2345678901234567890123456789012345678901".to_string(),
            weight: 1,
        },
        AuthorityConfig {
            address: "0x3456789012345678901234567890123456789012".to_string(),
            weight: 1,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let config = PoAConfig::default();
        assert_eq!(config.slot_duration, 3);
        assert!(config.authorities.is_empty());
    }

    #[test]
    fn test_config_validation() {
        let mut config = PoAConfig::default();
        
        // Empty authorities should fail
        assert!(config.validate().is_err());

        // Add valid authorities
        config.authorities = default_test_authorities();
        assert!(config.validate().is_ok());

        // Zero slot duration should fail
        config.slot_duration = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_serialization() {
        let config = PoAConfig {
            slot_duration: 5,
            authorities: default_test_authorities(),
            vrf_seed: [1u8; 32],
            epoch_length: 200,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: PoAConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.slot_duration, deserialized.slot_duration);
        assert_eq!(config.authorities.len(), deserialized.authorities.len());
        assert_eq!(config.vrf_seed, deserialized.vrf_seed);
        assert_eq!(config.epoch_length, deserialized.epoch_length);
    }

    #[test]
    fn test_file_operations() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("authorities.json");

        let config = PoAConfig {
            slot_duration: 5,
            authorities: default_test_authorities(),
            vrf_seed: [1u8; 32],
            epoch_length: 200,
        };

        // Save to file
        config.save_to_file(&file_path).unwrap();

        // Load from file
        let loaded_config = PoAConfig::load_from_file(&file_path).unwrap();

        assert_eq!(config.slot_duration, loaded_config.slot_duration);
        assert_eq!(config.authorities.len(), loaded_config.authorities.len());
    }

    #[test]
    fn test_to_authority_set() {
        let config = PoAConfig {
            slot_duration: 3,
            authorities: default_test_authorities(),
            vrf_seed: [0u8; 32],
            epoch_length: 100,
        };

        let authority_set = config.to_authority_set(1).unwrap();
        assert_eq!(authority_set.len(), 3);
        assert_eq!(authority_set.epoch, 1);
        assert_eq!(authority_set.total_weight(), 3);
    }
}
