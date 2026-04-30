//! # ECDSA Signatures (secp256k1)
//!
//! Ethereum uses ECDSA signatures on the secp256k1 curve.
//!
//! A signature consists of:
//! - `r`: x-coordinate of the random point (32 bytes)
//! - `s`: signature proof (32 bytes)
//! - `v`: recovery id (1 byte) - allows recovering public key from signature

use crate::error::{EthError, Result};
use crate::hash::{keccak256, H256};
use crate::address::Address;
use k256::ecdsa::{
    SigningKey, VerifyingKey,
    signature::{Signer, Verifier},
    Signature as K256Signature,
    RecoveryId,
};
use std::fmt;

/// ECDSA signature with recovery id
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    /// R component (32 bytes)
    pub r: H256,
    /// S component (32 bytes)
    pub s: H256,
    /// Recovery id (0 or 1, or 27/28 for legacy Ethereum)
    pub v: u8,
}

impl Signature {
    /// Create a new signature from components
    pub fn new(r: H256, s: H256, v: u8) -> Self {
        Signature { r, s, v }
    }

    /// Create from raw 65-byte signature (r || s || v)
    pub fn from_bytes(bytes: &[u8; 65]) -> Self {
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[0..32]);
        s.copy_from_slice(&bytes[32..64]);
        Signature {
            r: H256(r),
            s: H256(s),
            v: bytes[64],
        }
    }

    /// Convert to 65-byte array (r || s || v)
    pub fn to_bytes(&self) -> [u8; 65] {
        let mut result = [0u8; 65];
        result[0..32].copy_from_slice(&self.r.0);
        result[32..64].copy_from_slice(&self.s.0);
        result[64] = self.v;
        result
    }

    /// Get the recovery id (0 or 1)
    /// Handles legacy Ethereum format (27/28) and EIP-155 format
    pub fn recovery_id(&self) -> Result<u8> {
        match self.v {
            0 | 1 => Ok(self.v),
            27 | 28 => Ok(self.v - 27),
            // EIP-155: v = chain_id * 2 + 35 + recovery_id
            v if v >= 35 => Ok((v - 35) % 2),
            _ => Err(EthError::InvalidRecoveryId(self.v)),
        }
    }

    /// Recover the signer's address from a message hash
    ///
    /// This is the Rust equivalent of Solidity's `ecrecover`
    ///
    /// # Example
    /// ```ignore
    /// let digest = keccak256(b"hello");
    /// let address = signature.recover(&digest)?;
    /// ```
    pub fn recover(&self, message_hash: &H256) -> Result<Address> {
        let recovery_id = self.recovery_id()?;
        let recid = RecoveryId::try_from(recovery_id)
            .map_err(|_| EthError::InvalidSignature("Invalid recovery id".into()))?;

        // Combine r and s into signature
        let mut sig_bytes = [0u8; 64];
        sig_bytes[0..32].copy_from_slice(&self.r.0);
        sig_bytes[32..64].copy_from_slice(&self.s.0);

        let signature = K256Signature::from_bytes((&sig_bytes).into())
            .map_err(|e| EthError::InvalidSignature(e.to_string()))?;

        // Recover public key
        let recovered_key = VerifyingKey::recover_from_prehash(
            &message_hash.0,
            &signature,
            recid,
        ).map_err(|e| EthError::InvalidSignature(e.to_string()))?;

        // Convert to address
        let pubkey_bytes = recovered_key.to_encoded_point(false);
        let pubkey_uncompressed = pubkey_bytes.as_bytes();

        // Skip the 0x04 prefix (uncompressed point marker)
        let mut pubkey_64 = [0u8; 64];
        pubkey_64.copy_from_slice(&pubkey_uncompressed[1..65]);

        Ok(Address::from_public_key(&pubkey_64))
    }

    /// Verify that this signature was created by the given address
    pub fn verify(&self, message_hash: &H256, expected_signer: &Address) -> bool {
        match self.recover(message_hash) {
            Ok(recovered) => recovered == *expected_signer,
            Err(_) => false,
        }
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signature")
            .field("r", &self.r.to_hex())
            .field("s", &self.s.to_hex())
            .field("v", &self.v)
            .finish()
    }
}

/// Sign a message hash with a private key
///
/// # Example
/// ```ignore
/// let private_key = [0u8; 32]; // Your private key
/// let message_hash = keccak256(b"hello");
/// let signature = sign(&message_hash, &private_key)?;
/// ```
pub fn sign(message_hash: &H256, private_key: &[u8; 32]) -> Result<Signature> {
    let signing_key = SigningKey::from_bytes(private_key.into())
        .map_err(|e| EthError::InvalidSignature(e.to_string()))?;

    // Sign with recovery
    let (signature, recid) = signing_key
        .sign_prehash_recoverable(&message_hash.0)
        .map_err(|e| EthError::InvalidSignature(e.to_string()))?;

    let sig_bytes = signature.to_bytes();
    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&sig_bytes[0..32]);
    s.copy_from_slice(&sig_bytes[32..64]);

    Ok(Signature {
        r: H256(r),
        s: H256(s),
        v: recid.to_byte(),
    })
}

/// Get the public key from a private key
pub fn private_to_public(private_key: &[u8; 32]) -> Result<[u8; 64]> {
    let signing_key = SigningKey::from_bytes(private_key.into())
        .map_err(|e| EthError::InvalidSignature(e.to_string()))?;

    let verifying_key = signing_key.verifying_key();
    let pubkey_bytes = verifying_key.to_encoded_point(false);
    let pubkey_uncompressed = pubkey_bytes.as_bytes();

    let mut result = [0u8; 64];
    result.copy_from_slice(&pubkey_uncompressed[1..65]);
    Ok(result)
}

/// Get the address from a private key
pub fn private_to_address(private_key: &[u8; 32]) -> Result<Address> {
    let pubkey = private_to_public(private_key)?;
    Ok(Address::from_public_key(&pubkey))
}

/// Hash a message with Ethereum's signed message prefix
///
/// This is what wallets do when you call `personal_sign`:
/// `keccak256("\x19Ethereum Signed Message:\n" + len + message)`
pub fn hash_message(message: &[u8]) -> H256 {
    let prefix = format!("\x19Ethereum Signed Message:\n{}", message.len());
    let mut data = prefix.into_bytes();
    data.extend_from_slice(message);
    keccak256(&data)
}

/// Create EIP-712 domain separator
///
/// Used for typed structured data signing
pub fn eip712_domain_separator(
    name: &str,
    version: &str,
    chain_id: u64,
    verifying_contract: &Address,
) -> H256 {
    let type_hash = keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"
    );

    let name_hash = keccak256(name.as_bytes());
    let version_hash = keccak256(version.as_bytes());

    let mut encoded = Vec::new();
    encoded.extend_from_slice(&type_hash.0);
    encoded.extend_from_slice(&name_hash.0);
    encoded.extend_from_slice(&version_hash.0);

    // Chain ID as 32 bytes (big-endian, left-padded)
    let mut chain_id_bytes = [0u8; 32];
    chain_id_bytes[24..32].copy_from_slice(&chain_id.to_be_bytes());
    encoded.extend_from_slice(&chain_id_bytes);

    // Address as 32 bytes (left-padded)
    let mut addr_bytes = [0u8; 32];
    addr_bytes[12..32].copy_from_slice(&verifying_contract.0);
    encoded.extend_from_slice(&addr_bytes);

    keccak256(&encoded)
}

/// Create EIP-712 typed data hash (ready for signing)
pub fn eip712_hash(domain_separator: &H256, struct_hash: &H256) -> H256 {
    let mut data = Vec::with_capacity(66);
    data.extend_from_slice(b"\x19\x01");
    data.extend_from_slice(&domain_separator.0);
    data.extend_from_slice(&struct_hash.0);
    keccak256(&data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_recover() {
        // Use a simple test key
        let private_key: [u8; 32] = [1u8; 32];
        let message = b"Hello, Ethereum!";
        let message_hash = hash_message(message);

        let signature = sign(&message_hash, &private_key).unwrap();
        let signer = private_to_address(&private_key).unwrap();
        let recovered = signature.recover(&message_hash).unwrap();

        assert_eq!(recovered, signer);
    }

    #[test]
    fn test_signature_bytes_roundtrip() {
        let sig = Signature {
            r: H256([1u8; 32]),
            s: H256([2u8; 32]),
            v: 27,
        };

        let bytes = sig.to_bytes();
        let recovered = Signature::from_bytes(&bytes);

        assert_eq!(sig, recovered);
    }

    #[test]
    fn test_hash_message() {
        // Well-known test vector
        let message = b"Hello World";
        let hash = hash_message(message);

        // The hash should be consistent
        let hash2 = hash_message(message);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_recovery_id() {
        // Standard format
        assert_eq!(Signature::new(H256::zero(), H256::zero(), 0).recovery_id().unwrap(), 0);
        assert_eq!(Signature::new(H256::zero(), H256::zero(), 1).recovery_id().unwrap(), 1);

        // Legacy format
        assert_eq!(Signature::new(H256::zero(), H256::zero(), 27).recovery_id().unwrap(), 0);
        assert_eq!(Signature::new(H256::zero(), H256::zero(), 28).recovery_id().unwrap(), 1);

        // EIP-155 format (chain_id = 1)
        assert_eq!(Signature::new(H256::zero(), H256::zero(), 37).recovery_id().unwrap(), 0);
        assert_eq!(Signature::new(H256::zero(), H256::zero(), 38).recovery_id().unwrap(), 1);
    }

    #[test]
    fn test_eip712_domain_separator() {
        let name = "Test";
        let version = "1";
        let chain_id = 1u64;
        let contract = Address::zero();

        let domain = eip712_domain_separator(name, version, chain_id, &contract);

        // Should be deterministic
        let domain2 = eip712_domain_separator(name, version, chain_id, &contract);
        assert_eq!(domain, domain2);

        // Different chain should give different separator
        let domain3 = eip712_domain_separator(name, version, 137, &contract);
        assert_ne!(domain, domain3);
    }
}
