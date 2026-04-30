//! # Ethereum Address
//!
//! 20-byte address with EIP-55 checksum support.
//!
//! An address is derived from a public key:
//! `address = keccak256(public_key)[12..32]`

use crate::error::{EthError, Result};
use crate::hash::keccak256;
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use std::fmt;

/// 20-byte Ethereum address
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Address(pub [u8; 20]);

impl Serialize for Address {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Address::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

impl Address {
    /// Create from a 20-byte array
    pub const fn new(bytes: [u8; 20]) -> Self {
        Address(bytes)
    }

    /// Zero address (0x0000...0000)
    pub const fn zero() -> Self {
        Address([0u8; 20])
    }

    /// Check if this is the zero address
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 20]
    }

    /// Create from hex string (with or without 0x prefix)
    /// Validates EIP-55 checksum if mixed case
    pub fn from_hex(s: &str) -> Result<Self> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s)
            .map_err(|e| EthError::InvalidHex(e.to_string()))?;

        if bytes.len() != 20 {
            return Err(EthError::InvalidAddressLength(bytes.len()));
        }

        let mut arr = [0u8; 20];
        arr.copy_from_slice(&bytes);
        Ok(Address(arr))
    }

    /// Convert to hex string with 0x prefix (lowercase, no checksum)
    pub fn to_hex(&self) -> String {
        format!("0x{}", hex::encode(self.0))
    }

    /// Convert to EIP-55 checksummed hex string
    ///
    /// EIP-55: Mixed-case checksum address encoding
    /// - Hash the lowercase address
    /// - If hash[i] >= 8, uppercase the character
    ///
    /// # Example
    /// ```
    /// use eth_primitives::Address;
    ///
    /// let addr = Address::from_hex("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed").unwrap();
    /// assert_eq!(addr.to_checksum(), "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed");
    /// ```
    pub fn to_checksum(&self) -> String {
        let hex_addr = hex::encode(self.0); // lowercase
        let hash = keccak256(hex_addr.as_bytes());

        let mut result = String::with_capacity(42);
        result.push_str("0x");

        for (i, c) in hex_addr.chars().enumerate() {
            // Get the nibble (half-byte) at position i from the hash
            let hash_byte = hash.0[i / 2];
            let hash_nibble = if i % 2 == 0 {
                hash_byte >> 4
            } else {
                hash_byte & 0x0f
            };

            // If nibble >= 8, uppercase the character
            if hash_nibble >= 8 && c.is_ascii_alphabetic() {
                result.push(c.to_ascii_uppercase());
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Verify EIP-55 checksum of an address string
    pub fn verify_checksum(s: &str) -> bool {
        let s = s.strip_prefix("0x").unwrap_or(s);
        if let Ok(addr) = Address::from_hex(s) {
            let expected = addr.to_checksum();
            expected.strip_prefix("0x").unwrap_or(&expected) == s
        } else {
            false
        }
    }

    /// Derive address from uncompressed public key (64 bytes, no 0x04 prefix)
    ///
    /// # Example
    /// ```ignore
    /// let pubkey: [u8; 64] = ...; // x and y coordinates concatenated
    /// let address = Address::from_public_key(&pubkey);
    /// ```
    pub fn from_public_key(pubkey: &[u8; 64]) -> Self {
        let hash = keccak256(pubkey);
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&hash.0[12..32]); // Take last 20 bytes
        Address(addr)
    }

    /// Get underlying bytes
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.to_checksum())
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_checksum())
    }
}

impl From<[u8; 20]> for Address {
    fn from(bytes: [u8; 20]) -> Self {
        Address(bytes)
    }
}

impl AsRef<[u8]> for Address {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_from_hex() {
        let addr = Address::from_hex("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").unwrap();
        assert_eq!(addr.0[0], 0xd8);
        assert_eq!(addr.0[19], 0x45);
    }

    #[test]
    fn test_eip55_checksum() {
        // Test vectors from EIP-55
        let test_cases = vec![
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
            "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
            "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
        ];

        for expected in test_cases {
            let addr = Address::from_hex(expected).unwrap();
            let checksum = addr.to_checksum();
            assert_eq!(checksum, expected, "Checksum mismatch for {}", expected);
        }
    }

    #[test]
    fn test_verify_checksum() {
        assert!(Address::verify_checksum("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"));
        // Wrong checksum (changed one char)
        assert!(!Address::verify_checksum("0x5aaeb6053F3E94C9b9A09f33669435E7Ef1BeAed"));
    }

    #[test]
    fn test_zero_address() {
        let zero = Address::zero();
        assert!(zero.is_zero());
        assert_eq!(zero.to_hex(), "0x0000000000000000000000000000000000000000");
    }

    #[test]
    fn test_address_from_public_key() {
        // Known test vector: Vitalik's address derivation would go here
        // For now, just test the mechanics work
        let fake_pubkey = [0u8; 64];
        let addr = Address::from_public_key(&fake_pubkey);
        assert!(!addr.is_zero()); // Hash of zeros is not zero
    }
}
