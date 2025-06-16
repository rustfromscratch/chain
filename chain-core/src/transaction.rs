//! Transaction data structures and operations

use crate::{Address, CoreError, CoreResult, Gas, Hash, Nonce, Wei};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

/// Transaction signature
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, bincode::Encode)]
pub struct Signature {
    pub r: [u8; 32],
    pub s: [u8; 32],
    pub v: u8,
}

impl Signature {
    /// Create new signature
    pub fn new(r: [u8; 32], s: [u8; 32], v: u8) -> Self {
        Self { r, s, v }
    }

    /// Convert to bytes (65 bytes total)
    pub fn to_bytes(&self) -> [u8; 65] {
        let mut bytes = [0u8; 65];
        bytes[0..32].copy_from_slice(&self.r);
        bytes[32..64].copy_from_slice(&self.s);
        bytes[64] = self.v;
        bytes
    }

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> CoreResult<Self> {
        if bytes.len() != 65 {
            return Err(CoreError::InvalidSignature);
        }

        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[0..32]);
        s.copy_from_slice(&bytes[32..64]);
        let v = bytes[64];

        Ok(Self { r, s, v })
    }
}

/// Transaction data structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, bincode::Encode)]
pub struct Transaction {
    /// Transaction nonce (number of transactions sent from this address)
    pub nonce: Nonce,
    /// Gas price in wei
    pub gas_price: Wei,
    /// Maximum gas to use for this transaction
    pub gas_limit: Gas,
    /// Recipient address (None for contract creation)
    pub to: Option<Address>,
    /// Value to transfer in wei
    pub value: Wei,
    /// Transaction data/input
    pub data: Vec<u8>,
    /// Transaction signature
    pub signature: Option<Signature>,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(
        nonce: Nonce,
        gas_price: Wei,
        gas_limit: Gas,
        to: Option<Address>,
        value: Wei,
        data: Vec<u8>,
    ) -> Self {
        Self {
            nonce,
            gas_price,
            gas_limit,
            to,
            value,
            data,
            signature: None,
        }
    }

    /// Create a simple transfer transaction
    pub fn transfer(nonce: Nonce, to: Address, value: Wei, gas_price: Wei, gas_limit: Gas) -> Self {
        Self::new(nonce, gas_price, gas_limit, Some(to), value, Vec::new())
    }

    /// Create a contract creation transaction
    pub fn create_contract(
        nonce: Nonce,
        value: Wei,
        gas_price: Wei,
        gas_limit: Gas,
        code: Vec<u8>,
    ) -> Self {
        Self::new(nonce, gas_price, gas_limit, None, value, code)
    }
    /// Encode transaction for hashing (without signature)
    pub fn encode_for_signing(&self) -> CoreResult<Vec<u8>> {
        let tx_data = TransactionForSigning {
            nonce: self.nonce,
            gas_price: self.gas_price,
            gas_limit: self.gas_limit,
            to: self.to,
            value: self.value,
            data: self.data.clone(),
        };

        bincode::encode_to_vec(&tx_data, bincode::config::standard())
            .map_err(|e| CoreError::Bincode(e.to_string()))
    }

    /// Calculate transaction hash (including signature)
    pub fn hash(&self) -> CoreResult<Hash> {
        let encoded = bincode::encode_to_vec(self, bincode::config::standard())
            .map_err(|e| CoreError::Bincode(e.to_string()))?;
        let hash_bytes = Keccak256::digest(&encoded);
        Ok(Hash::from_slice(hash_bytes.as_slice()))
    }

    /// Calculate hash for signing (without signature)
    pub fn signing_hash(&self) -> CoreResult<Hash> {
        let encoded = self.encode_for_signing()?;
        let hash_bytes = Keccak256::digest(&encoded);
        Ok(Hash::from_slice(hash_bytes.as_slice()))
    }

    /// Sign the transaction with private key
    pub fn sign(&mut self, private_key: &[u8]) -> CoreResult<()> {
        let signing_hash = self.signing_hash()?;

        // Create secp256k1 context
        let secp = secp256k1::Secp256k1::new();
        let secret_key = secp256k1::SecretKey::from_slice(private_key)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        // Sign the hash
        let message = secp256k1::Message::from_digest_slice(signing_hash.as_bytes())
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let sig = secp.sign_ecdsa_recoverable(message, &secret_key);
        let (recovery_id, sig_bytes) = sig.serialize_compact();

        // Extract r, s, v
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&sig_bytes[0..32]);
        s.copy_from_slice(&sig_bytes[32..64]);
        let v = recovery_id as u8;

        self.signature = Some(Signature::new(r, s, v));
        Ok(())
    }

    /// Verify transaction signature
    pub fn verify_signature(&self) -> CoreResult<bool> {
        let signature = match &self.signature {
            Some(sig) => sig,
            None => return Ok(false),
        };

        let signing_hash = self.signing_hash()?;

        // Create secp256k1 context
        let secp = secp256k1::Secp256k1::new();

        // Recreate signature
        let recovery_id = secp256k1::ecdsa::RecoveryId::from_u8_masked(signature.v);

        let mut sig_bytes = [0u8; 64];
        sig_bytes[0..32].copy_from_slice(&signature.r);
        sig_bytes[32..64].copy_from_slice(&signature.s);

        let recoverable_sig =
            secp256k1::ecdsa::RecoverableSignature::from_compact(&sig_bytes, recovery_id)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

        // Recover public key
        let message = secp256k1::Message::from_digest_slice(signing_hash.as_bytes())
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let _public_key = secp
            .recover_ecdsa(message, &recoverable_sig)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        Ok(true)
    }

    /// Get the sender address from signature
    pub fn sender(&self) -> CoreResult<Address> {
        let signature = match &self.signature {
            Some(sig) => sig,
            None => return Err(CoreError::InvalidSignature),
        };

        let signing_hash = self.signing_hash()?;

        // Create secp256k1 context
        let secp = secp256k1::Secp256k1::new();

        // Recreate signature
        let recovery_id = secp256k1::ecdsa::RecoveryId::from_u8_masked(signature.v);

        let mut sig_bytes = [0u8; 64];
        sig_bytes[0..32].copy_from_slice(&signature.r);
        sig_bytes[32..64].copy_from_slice(&signature.s);

        let recoverable_sig =
            secp256k1::ecdsa::RecoverableSignature::from_compact(&sig_bytes, recovery_id)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;

        // Recover public key
        let message = secp256k1::Message::from_digest_slice(signing_hash.as_bytes())
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        let public_key = secp
            .recover_ecdsa(message, &recoverable_sig)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        // Convert public key to address (last 20 bytes of Keccak256 hash)
        let pubkey_bytes = public_key.serialize_uncompressed();
        let pubkey_hash = Keccak256::digest(&pubkey_bytes[1..]); // Skip first byte (0x04)
        let mut addr_bytes = [0u8; 20];
        addr_bytes.copy_from_slice(&pubkey_hash[12..32]); // Last 20 bytes

        Ok(Address::new(addr_bytes))
    }
}

/// Helper struct for encoding transaction data for signing
#[derive(Serialize, bincode::Encode)]
struct TransactionForSigning {
    nonce: Nonce,
    gas_price: Wei,
    gas_limit: Gas,
    to: Option<Address>,
    value: Wei,
    data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_creation() {
        let to = Address::from_hex("1234567890abcdef1234567890abcdef12345678").unwrap();
        let tx = Transaction::transfer(1, to, 1000, 20_000_000_000, 21_000);

        assert_eq!(tx.nonce, 1);
        assert_eq!(tx.value, 1000);
        assert_eq!(tx.to, Some(to));
        assert!(tx.signature.is_none());
    }

    #[test]
    fn test_transaction_hash() {
        let to = Address::from_hex("1234567890abcdef1234567890abcdef12345678").unwrap();
        let tx = Transaction::transfer(1, to, 1000, 20_000_000_000, 21_000);

        let hash = tx.hash().unwrap();
        // Hash should be deterministic
        let hash2 = tx.hash().unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_contract_creation() {
        let code = vec![0x60, 0x60, 0x60, 0x40]; // Sample bytecode
        let tx = Transaction::create_contract(0, 0, 20_000_000_000, 100_000, code.clone());

        assert_eq!(tx.nonce, 0);
        assert_eq!(tx.to, None);
        assert_eq!(tx.data, code);
    }
}
