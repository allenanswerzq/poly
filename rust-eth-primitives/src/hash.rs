//! # H256 - 32-byte Hash
//!
//! Represents Ethereum's primary hash type (Keccak256 output).
//! Used for: transaction hashes, block hashes, storage keys, state roots.

use crate::error::{EthError, Result};
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use std::fmt;
use tiny_keccak::{Hasher, Keccak};

/// 32-byte hash (256 bits)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct H256(pub [u8; 32]);

impl Serialize for H256 {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for H256 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        H256::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

impl H256 {
    /// Create from a 32-byte array
    pub const fn new(bytes: [u8; 32]) -> Self {
        H256(bytes)
    }

    /// Zero hash (all zeros)
    pub const fn zero() -> Self {
        H256([0u8; 32])
    }

    /// Check if hash is zero
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }

    /// Create from hex string (with or without 0x prefix)
    pub fn from_hex(s: &str) -> Result<Self> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s)
            .map_err(|e| EthError::InvalidHex(e.to_string()))?;

        if bytes.len() != 32 {
            return Err(EthError::InvalidHashLength(bytes.len()));
        }

        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(H256(arr))
    }

    /// Convert to hex string with 0x prefix
    pub fn to_hex(&self) -> String {
        format!("0x{}", hex::encode(self.0))
    }

    /// Get underlying bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to fixed-size byte array
    pub fn to_fixed_bytes(&self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for H256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "H256({})", self.to_hex())
    }
}

impl fmt::Display for H256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl From<[u8; 32]> for H256 {
    fn from(bytes: [u8; 32]) -> Self {
        H256(bytes)
    }
}

impl AsRef<[u8]> for H256 {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Compute Keccak256 hash of input bytes
///
/// # Example
/// ```
/// use eth_primitives::keccak256;
///
/// let hash = keccak256(b"hello");
/// println!("Hash: {}", hash.to_hex());
/// ```
pub fn keccak256(input: &[u8]) -> H256 {
    let mut hasher = Keccak::v256();
    let mut output = [0u8; 32];
    hasher.update(input);
    hasher.finalize(&mut output);
    H256(output)
}

/// Compute Keccak256 hash of multiple inputs concatenated
pub fn keccak256_concat(inputs: &[&[u8]]) -> H256 {
    let mut hasher = Keccak::v256();
    for input in inputs {
        hasher.update(input);
    }
    let mut output = [0u8; 32];
    hasher.finalize(&mut output);
    H256(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keccak256_empty() {
        // Keccak256 of empty string
        let hash = keccak256(b"");
        assert_eq!(
            hash.to_hex(),
            "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }

    #[test]
    fn test_keccak256_hello() {
        // Well-known test vector
        let hash = keccak256(b"hello");
        assert_eq!(
            hash.to_hex(),
            "0x1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8"
        );
    }

    #[test]
    fn test_h256_from_hex() {
        let hash = H256::from_hex(
            "0x1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8"
        ).unwrap();

        assert_eq!(hash, keccak256(b"hello"));
    }

    #[test]
    fn test_h256_zero() {
        let zero = H256::zero();
        assert!(zero.is_zero());
        assert_eq!(
            zero.to_hex(),
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        );
    }
}
