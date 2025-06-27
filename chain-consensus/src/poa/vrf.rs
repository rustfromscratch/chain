//! VRF (Verifiable Random Function) implementation for PoA

use crate::{ConsensusError, ConsensusResult};
use blake3::Hasher;
use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::{RistrettoPoint, CompressedRistretto},
    scalar::Scalar,
};
use serde::{Deserialize, Serialize};

/// VRF seed for randomness generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VrfSeed(pub [u8; 32]);

impl VrfSeed {
    /// Generate a new random seed
    pub fn random() -> Self {
        VrfSeed(rand::random())
    }

    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        VrfSeed(bytes)
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Default for VrfSeed {
    fn default() -> Self {
        VrfSeed([0u8; 32])
    }
}

/// VRF proof
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VrfProof {
    /// The proof value
    pub proof: [u8; 32],
    /// The output hash
    pub output: [u8; 32],
}

/// VRF key pair for signing
#[derive(Debug, Clone)]
pub struct VrfKeypair {
    /// Private scalar
    secret: Scalar,
    /// Public point
    public: RistrettoPoint,
}

impl VrfKeypair {    /// Generate a new keypair
    pub fn generate() -> Self {
        let secret_bytes: [u8; 32] = rand::random();
        let secret = Scalar::from_bytes_mod_order(secret_bytes);
        let public = secret * RISTRETTO_BASEPOINT_POINT;

        Self { secret, public }
    }

    /// Create from seed
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let secret = Scalar::from_bytes_mod_order(*seed);
        let public = secret * RISTRETTO_BASEPOINT_POINT;

        Self { secret, public }
    }

    /// Get public key
    pub fn public(&self) -> VrfPublicKey {
        VrfPublicKey {
            point: self.public,
        }
    }

    /// Sign a message with VRF
    pub fn sign(&self, message: &[u8]) -> VrfProof {
        // Simple VRF implementation using hash-based approach
        // In production, this should use a proper VRF construction like ECVRF
        
        let mut hasher = Hasher::new();
        hasher.update(&self.secret.to_bytes());
        hasher.update(message);
        let hash1 = hasher.finalize();

        let mut hasher2 = Hasher::new();
        hasher2.update(hash1.as_bytes());
        hasher2.update(&self.public.compress().to_bytes());
        let hash2 = hasher2.finalize();

        VrfProof {
            proof: hash1.as_bytes()[..32].try_into().unwrap(),
            output: hash2.as_bytes()[..32].try_into().unwrap(),
        }
    }
}

/// VRF public key
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VrfPublicKey {
    point: RistrettoPoint,
}

impl VrfPublicKey {
    /// Verify a VRF proof
    pub fn verify(&self, message: &[u8], proof: &VrfProof) -> bool {
        // Recreate the proof and compare
        let mut hasher = Hasher::new();
        hasher.update(&proof.proof);
        hasher.update(message);
        let reconstructed_hash = hasher.finalize();

        let mut hasher2 = Hasher::new();
        hasher2.update(reconstructed_hash.as_bytes());
        hasher2.update(&self.point.compress().to_bytes());
        let expected_output = hasher2.finalize();

        expected_output.as_bytes()[..32] == proof.output
    }

    /// Serialize public key
    pub fn to_bytes(&self) -> [u8; 32] {
        self.point.compress().to_bytes()
    }

    /// Deserialize public key
    pub fn from_bytes(bytes: &[u8; 32]) -> ConsensusResult<Self> {
        let compressed = CompressedRistretto::from_slice(bytes)
            .map_err(|e| ConsensusError::VrfError(format!("Invalid compressed point: {}", e)))?;
        
        let point = compressed.decompress()
            .ok_or_else(|| ConsensusError::VrfError("Failed to decompress point".to_string()))?;

        Ok(Self { point })
    }
}

/// VRF-based validator selection
pub struct VrfSelector {
    /// Current VRF seed
    seed: VrfSeed,
    /// Number of validators
    validator_count: usize,
}

impl VrfSelector {
    /// Create a new VRF selector
    pub fn new(seed: VrfSeed, validator_count: usize) -> Self {
        Self {
            seed,
            validator_count,
        }
    }

    /// Select validator for a given slot
    pub fn select_validator(&self, slot: u64) -> usize {
        if self.validator_count == 0 {
            return 0;
        }

        // Combine seed with slot number
        let mut hasher = Hasher::new();
        hasher.update(self.seed.as_bytes());
        hasher.update(&slot.to_le_bytes());
        let hash = hasher.finalize();

        // Convert hash to validator index
        let hash_bytes = hash.as_bytes();
        let mut index_bytes = [0u8; 8];
        index_bytes.copy_from_slice(&hash_bytes[..8]);
        let index = u64::from_le_bytes(index_bytes);

        (index % self.validator_count as u64) as usize
    }

    /// Update seed (for new epoch)
    pub fn update_seed(&mut self, new_seed: VrfSeed) {
        self.seed = new_seed;
    }

    /// Get current seed
    pub fn current_seed(&self) -> VrfSeed {
        self.seed
    }
}

/// Generate VRF input for a given slot and seed
pub fn vrf_input(seed: &VrfSeed, slot: u64) -> Vec<u8> {
    let mut input = Vec::new();
    input.extend_from_slice(seed.as_bytes());
    input.extend_from_slice(&slot.to_le_bytes());
    input
}

/// Verify VRF output determines the correct validator
pub fn verify_vrf_selection(
    seed: &VrfSeed,
    slot: u64,
    expected_validator: usize,
    validator_count: usize,
    proof: &VrfProof,
    public_key: &VrfPublicKey,
) -> bool {
    // Verify the VRF proof first
    let input = vrf_input(seed, slot);
    if !public_key.verify(&input, proof) {
        return false;
    }

    // Check if the VRF output selects the expected validator
    let selector = VrfSelector::new(*seed, validator_count);
    selector.select_validator(slot) == expected_validator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vrf_keypair_generation() {
        let keypair1 = VrfKeypair::generate();
        let keypair2 = VrfKeypair::generate();

        // Different keypairs should have different public keys
        assert_ne!(keypair1.public().to_bytes(), keypair2.public().to_bytes());
    }

    #[test]
    fn test_vrf_sign_verify() {
        let keypair = VrfKeypair::generate();
        let message = b"test message";

        let proof = keypair.sign(message);
        let public_key = keypair.public();

        // Valid proof should verify
        assert!(public_key.verify(message, &proof));

        // Different message should not verify
        assert!(!public_key.verify(b"different message", &proof));
    }

    #[test]
    fn test_vrf_selector() {
        let seed = VrfSeed::random();
        let selector = VrfSelector::new(seed, 5);

        // Same slot should always return same validator
        let validator1 = selector.select_validator(100);
        let validator2 = selector.select_validator(100);
        assert_eq!(validator1, validator2);

        // Validator should be in valid range
        assert!(validator1 < 5);

        // Different slots may return different validators
        let validator3 = selector.select_validator(101);
        assert!(validator3 < 5);
    }

    #[test]
    fn test_seed_serialization() {
        let seed = VrfSeed::random();
        let bytes = seed.as_bytes();
        let restored = VrfSeed::from_bytes(*bytes);

        assert_eq!(seed, restored);
    }

    #[test]
    fn test_public_key_serialization() {
        let keypair = VrfKeypair::generate();
        let public_key = keypair.public();
        let bytes = public_key.to_bytes();
        let restored = VrfPublicKey::from_bytes(&bytes).unwrap();

        assert_eq!(public_key, restored);
    }

    #[test]
    fn test_vrf_deterministic() {
        let seed = [42u8; 32];
        let keypair1 = VrfKeypair::from_seed(&seed);
        let keypair2 = VrfKeypair::from_seed(&seed);

        let message = b"test";
        let proof1 = keypair1.sign(message);
        let proof2 = keypair2.sign(message);

        // Same seed should produce same keypair and same proof
        assert_eq!(proof1, proof2);
    }
}
