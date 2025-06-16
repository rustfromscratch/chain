//! Basic blockchain types

use serde::{Deserialize, Serialize};
use std::fmt;

/// Block number type (64-bit unsigned integer)
pub type BlockNumber = u64;

/// Timestamp in milliseconds since Unix epoch
pub type Timestamp = u64;

/// 32-byte hash type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, bincode::Encode)]
pub struct Hash([u8; 32]);

impl Hash {
    /// Create a new hash from byte array
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Create hash from slice (panics if length != 32)
    pub fn from_slice(slice: &[u8]) -> Self {
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(slice);
        Self(bytes)
    }

    /// Get the underlying byte array
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(hex)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        Ok(Self::from_slice(&bytes))
    }

    /// Zero hash (all bytes are 0)
    pub fn zero() -> Self {
        Self([0u8; 32])
    }
}

impl Default for Hash {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl From<[u8; 32]> for Hash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// 20-byte address type  
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, bincode::Encode)]
pub struct Address([u8; 20]);

impl Address {
    /// Create a new address from byte array
    pub fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Create address from slice (panics if length != 20)
    pub fn from_slice(slice: &[u8]) -> Self {
        let mut bytes = [0u8; 20];
        bytes.copy_from_slice(slice);
        Self(bytes)
    }

    /// Get the underlying byte array
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(hex)?;
        if bytes.len() != 20 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        Ok(Self::from_slice(&bytes))
    }

    /// Zero address (all bytes are 0)
    pub fn zero() -> Self {
        Self([0u8; 20])
    }
}

impl Default for Address {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl From<[u8; 20]> for Address {
    fn from(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for Address {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Gas amount type
pub type Gas = u64;

/// Wei amount type (smallest unit of currency)
pub type Wei = u128;

/// Nonce type for transactions
pub type Nonce = u64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_creation() {
        let hash = Hash::zero();
        assert_eq!(
            hash.to_hex(),
            "0000000000000000000000000000000000000000000000000000000000000000"
        );

        let bytes = [1u8; 32];
        let hash2 = Hash::new(bytes);
        assert_eq!(
            hash2.to_hex(),
            "0101010101010101010101010101010101010101010101010101010101010101"
        );
    }

    #[test]
    fn test_address_creation() {
        let addr = Address::zero();
        assert_eq!(addr.to_hex(), "0000000000000000000000000000000000000000");

        let bytes = [1u8; 20];
        let addr2 = Address::new(bytes);
        assert_eq!(addr2.to_hex(), "0101010101010101010101010101010101010101");
    }

    #[test]
    fn test_hash_from_hex() {
        let hex = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let hash = Hash::from_hex(hex).unwrap();
        assert_eq!(hash.to_hex(), hex);
    }

    #[test]
    fn test_address_from_hex() {
        let hex = "1234567890abcdef1234567890abcdef12345678";
        let addr = Address::from_hex(hex).unwrap();
        assert_eq!(addr.to_hex(), hex);
    }
}
