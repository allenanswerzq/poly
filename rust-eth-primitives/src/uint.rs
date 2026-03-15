//! # U256 - 256-bit Unsigned Integer
//!
//! Ethereum's primary numeric type. Used for:
//! - Token balances
//! - Gas prices
//! - Storage values
//! - Arithmetic in the EVM
//!
//! Implemented as four u64 limbs in little-endian order.

use crate::error::{EthError, Result};
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use std::fmt;
use std::ops::{Add, Sub, Mul, Div, Rem, BitAnd, BitOr, BitXor, Not, Shl, Shr};

/// 256-bit unsigned integer
///
/// Stored as 4 x 64-bit limbs in little-endian order:
/// `value = limbs[0] + limbs[1] * 2^64 + limbs[2] * 2^128 + limbs[3] * 2^192`
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct U256(pub [u64; 4]);

impl Serialize for U256 {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for U256 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        U256::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

impl U256 {
    /// Zero
    pub const ZERO: U256 = U256([0, 0, 0, 0]);

    /// One
    pub const ONE: U256 = U256([1, 0, 0, 0]);

    /// Maximum value (2^256 - 1)
    pub const MAX: U256 = U256([u64::MAX, u64::MAX, u64::MAX, u64::MAX]);

    /// Zero (alias for ZERO)
    pub const fn zero() -> Self {
        U256::ZERO
    }

    /// Create from a single u64
    pub const fn from_u64(val: u64) -> Self {
        U256([val, 0, 0, 0])
    }

    /// Create from a u128
    pub const fn from_u128(val: u128) -> Self {
        U256([val as u64, (val >> 64) as u64, 0, 0])
    }

    /// Convert to u64 (truncates if value exceeds u64::MAX)
    pub fn as_u64(&self) -> u64 {
        self.0[0]
    }

    /// Try to convert to u64, returns None if value exceeds u64::MAX
    pub fn try_to_u64(&self) -> Option<u64> {
        if self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0 {
            Some(self.0[0])
        } else {
            None
        }
    }

    /// Check if zero
    pub fn is_zero(&self) -> bool {
        self.0 == [0, 0, 0, 0]
    }

    /// Create from big-endian bytes (32 bytes)
    pub fn from_be_bytes(bytes: &[u8; 32]) -> Self {
        let mut limbs = [0u64; 4];
        // bytes[0..8] is most significant (limbs[3])
        limbs[3] = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        limbs[2] = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        limbs[1] = u64::from_be_bytes(bytes[16..24].try_into().unwrap());
        limbs[0] = u64::from_be_bytes(bytes[24..32].try_into().unwrap());
        U256(limbs)
    }

    /// Convert to big-endian bytes (32 bytes)
    pub fn to_be_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(&self.0[3].to_be_bytes());
        bytes[8..16].copy_from_slice(&self.0[2].to_be_bytes());
        bytes[16..24].copy_from_slice(&self.0[1].to_be_bytes());
        bytes[24..32].copy_from_slice(&self.0[0].to_be_bytes());
        bytes
    }

    /// Create from hex string
    pub fn from_hex(s: &str) -> Result<Self> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        // Pad to 64 characters (32 bytes)
        let padded = format!("{:0>64}", s);
        let bytes = hex::decode(&padded)
            .map_err(|e| EthError::InvalidHex(e.to_string()))?;

        if bytes.len() != 32 {
            return Err(EthError::InvalidHex("Too many digits".into()));
        }

        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(U256::from_be_bytes(&arr))
    }

    /// Convert to hex string with 0x prefix
    pub fn to_hex(&self) -> String {
        format!("0x{}", hex::encode(self.to_be_bytes()))
    }

    /// Get the number of bits needed to represent this number
    pub fn bits(&self) -> u32 {
        for i in (0..4).rev() {
            if self.0[i] != 0 {
                return (i as u32 + 1) * 64 - self.0[i].leading_zeros();
            }
        }
        0
    }

    /// Checked addition (returns error on overflow)
    pub fn checked_add(self, other: Self) -> Result<Self> {
        let (result, overflow) = self.overflowing_add(other);
        if overflow {
            Err(EthError::Overflow)
        } else {
            Ok(result)
        }
    }

    /// Checked subtraction (returns error on underflow)
    pub fn checked_sub(self, other: Self) -> Result<Self> {
        let (result, underflow) = self.overflowing_sub(other);
        if underflow {
            Err(EthError::Underflow)
        } else {
            Ok(result)
        }
    }

    /// Checked multiplication (returns error on overflow)
    pub fn checked_mul(self, other: Self) -> Result<Self> {
        let (result, overflow) = self.overflowing_mul(other);
        if overflow {
            Err(EthError::Overflow)
        } else {
            Ok(result)
        }
    }

    /// Checked division (returns error on division by zero)
    pub fn checked_div(self, other: Self) -> Result<Self> {
        if other.is_zero() {
            Err(EthError::DivisionByZero)
        } else {
            Ok(self.wrapping_div(other))
        }
    }

    /// Addition with overflow flag
    pub fn overflowing_add(self, other: Self) -> (Self, bool) {
        let mut result = [0u64; 4];
        let mut carry = 0u64;

        for i in 0..4 {
            let (sum1, c1) = self.0[i].overflowing_add(other.0[i]);
            let (sum2, c2) = sum1.overflowing_add(carry);
            result[i] = sum2;
            carry = (c1 as u64) + (c2 as u64);
        }

        (U256(result), carry != 0)
    }

    /// Subtraction with underflow flag
    pub fn overflowing_sub(self, other: Self) -> (Self, bool) {
        let mut result = [0u64; 4];
        let mut borrow = 0u64;

        for i in 0..4 {
            let (diff1, b1) = self.0[i].overflowing_sub(other.0[i]);
            let (diff2, b2) = diff1.overflowing_sub(borrow);
            result[i] = diff2;
            borrow = (b1 as u64) + (b2 as u64);
        }

        (U256(result), borrow != 0)
    }

    /// Multiplication with overflow flag
    pub fn overflowing_mul(self, other: Self) -> (Self, bool) {
        let mut result = [0u64; 4];
        let mut overflow = false;

        // Simple schoolbook multiplication
        for i in 0..4 {
            let mut carry = 0u128;
            for j in 0..4 {
                if i + j < 4 {
                    let prod = (self.0[i] as u128) * (other.0[j] as u128)
                             + (result[i + j] as u128) + carry;
                    result[i + j] = prod as u64;
                    carry = prod >> 64;
                } else if self.0[i] != 0 && other.0[j] != 0 {
                    overflow = true;
                }
            }
            if carry != 0 && i < 3 {
                // carry would go into position i + 4, which is overflow
                overflow = true;
            }
        }

        (U256(result), overflow)
    }

    /// Wrapping division (panics on divide by zero in debug, wraps in release)
    pub fn wrapping_div(self, other: Self) -> Self {
        if other.is_zero() {
            return U256::ZERO; // EVM returns 0 for division by zero
        }

        // Simple long division
        if self < other {
            return U256::ZERO;
        }

        let mut quotient = U256::ZERO;
        let mut remainder = U256::ZERO;

        // Process bit by bit from most significant
        for i in (0..256).rev() {
            // Shift remainder left by 1
            remainder = remainder << 1;

            // Set LSB of remainder to bit i of self
            let byte_idx = i / 64;
            let bit_idx = i % 64;
            if (self.0[byte_idx] >> bit_idx) & 1 == 1 {
                remainder.0[0] |= 1;
            }

            // If remainder >= other, subtract and set quotient bit
            if remainder >= other {
                remainder = (remainder.overflowing_sub(other)).0;
                let q_byte_idx = i / 64;
                let q_bit_idx = i % 64;
                quotient.0[q_byte_idx] |= 1u64 << q_bit_idx;
            }
        }

        quotient
    }

    /// Wrapping remainder
    pub fn wrapping_rem(self, other: Self) -> Self {
        if other.is_zero() {
            return U256::ZERO;
        }

        let quotient = self.wrapping_div(other);
        let (product, _) = quotient.overflowing_mul(other);
        (self.overflowing_sub(product)).0
    }
}

// Comparison traits
impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare from most significant limb
        for i in (0..4).rev() {
            match self.0[i].cmp(&other.0[i]) {
                std::cmp::Ordering::Equal => continue,
                ord => return ord,
            }
        }
        std::cmp::Ordering::Equal
    }
}

// Arithmetic operators (wrapping behavior like EVM)
impl Add for U256 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        self.overflowing_add(other).0
    }
}

impl Sub for U256 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        self.overflowing_sub(other).0
    }
}

impl Mul for U256 {
    type Output = Self;
    fn mul(self, other: Self) -> Self {
        self.overflowing_mul(other).0
    }
}

impl Div for U256 {
    type Output = Self;
    fn div(self, other: Self) -> Self {
        self.wrapping_div(other)
    }
}

impl Rem for U256 {
    type Output = Self;
    fn rem(self, other: Self) -> Self {
        self.wrapping_rem(other)
    }
}

// Bitwise operators
impl BitAnd for U256 {
    type Output = Self;
    fn bitand(self, other: Self) -> Self {
        U256([
            self.0[0] & other.0[0],
            self.0[1] & other.0[1],
            self.0[2] & other.0[2],
            self.0[3] & other.0[3],
        ])
    }
}

impl BitOr for U256 {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        U256([
            self.0[0] | other.0[0],
            self.0[1] | other.0[1],
            self.0[2] | other.0[2],
            self.0[3] | other.0[3],
        ])
    }
}

impl BitXor for U256 {
    type Output = Self;
    fn bitxor(self, other: Self) -> Self {
        U256([
            self.0[0] ^ other.0[0],
            self.0[1] ^ other.0[1],
            self.0[2] ^ other.0[2],
            self.0[3] ^ other.0[3],
        ])
    }
}

impl Not for U256 {
    type Output = Self;
    fn not(self) -> Self {
        U256([!self.0[0], !self.0[1], !self.0[2], !self.0[3]])
    }
}

impl Shl<u32> for U256 {
    type Output = Self;
    fn shl(self, shift: u32) -> Self {
        if shift >= 256 {
            return U256::ZERO;
        }
        if shift == 0 {
            return self;
        }

        let limb_shift = (shift / 64) as usize;
        let bit_shift = shift % 64;
        let mut result = [0u64; 4];

        for i in limb_shift..4 {
            result[i] = self.0[i - limb_shift] << bit_shift;
            if bit_shift > 0 && i > limb_shift {
                result[i] |= self.0[i - limb_shift - 1] >> (64 - bit_shift);
            }
        }

        U256(result)
    }
}

impl Shr<u32> for U256 {
    type Output = Self;
    fn shr(self, shift: u32) -> Self {
        if shift >= 256 {
            return U256::ZERO;
        }
        if shift == 0 {
            return self;
        }

        let limb_shift = (shift / 64) as usize;
        let bit_shift = shift % 64;
        let mut result = [0u64; 4];

        for i in 0..(4 - limb_shift) {
            result[i] = self.0[i + limb_shift] >> bit_shift;
            if bit_shift > 0 && i + limb_shift + 1 < 4 {
                result[i] |= self.0[i + limb_shift + 1] << (64 - bit_shift);
            }
        }

        U256(result)
    }
}

impl fmt::Debug for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U256({})", self.to_hex())
    }
}

impl fmt::Display for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // For display, show in a more readable format
        if self.is_zero() {
            write!(f, "0")
        } else {
            write!(f, "{}", self.to_hex())
        }
    }
}

impl fmt::LowerHex for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.to_be_bytes();
        for byte in bytes.iter() {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

impl From<u64> for U256 {
    fn from(val: u64) -> Self {
        U256::from_u64(val)
    }
}

impl From<u128> for U256 {
    fn from(val: u128) -> Self {
        U256::from_u128(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_u64() {
        let a = U256::from_u64(42);
        assert_eq!(a.0, [42, 0, 0, 0]);
    }

    #[test]
    fn test_addition() {
        let a = U256::from_u64(100);
        let b = U256::from_u64(200);
        let c = a + b;
        assert_eq!(c, U256::from_u64(300));
    }

    #[test]
    fn test_addition_overflow() {
        let a = U256::MAX;
        let b = U256::ONE;
        let (c, overflow) = a.overflowing_add(b);
        assert!(overflow);
        assert_eq!(c, U256::ZERO); // Wraps around
    }

    #[test]
    fn test_subtraction() {
        let a = U256::from_u64(300);
        let b = U256::from_u64(100);
        let c = a - b;
        assert_eq!(c, U256::from_u64(200));
    }

    #[test]
    fn test_multiplication() {
        let a = U256::from_u64(100);
        let b = U256::from_u64(200);
        let c = a * b;
        assert_eq!(c, U256::from_u64(20000));
    }

    #[test]
    fn test_division() {
        let a = U256::from_u64(1000);
        let b = U256::from_u64(3);
        let c = a / b;
        assert_eq!(c, U256::from_u64(333)); // Integer division
    }

    #[test]
    fn test_division_by_zero() {
        let a = U256::from_u64(100);
        let b = U256::ZERO;
        let c = a / b; // EVM behavior: returns 0
        assert_eq!(c, U256::ZERO);
    }

    #[test]
    fn test_remainder() {
        let a = U256::from_u64(1000);
        let b = U256::from_u64(3);
        let c = a % b;
        assert_eq!(c, U256::from_u64(1)); // 1000 % 3 = 1
    }

    #[test]
    fn test_comparison() {
        let a = U256::from_u64(100);
        let b = U256::from_u64(200);
        assert!(a < b);
        assert!(b > a);
        assert!(a != b);
        assert!(a == U256::from_u64(100));
    }

    #[test]
    fn test_bitwise() {
        let a = U256::from_u64(0b1100);
        let b = U256::from_u64(0b1010);

        assert_eq!(a & b, U256::from_u64(0b1000));
        assert_eq!(a | b, U256::from_u64(0b1110));
        assert_eq!(a ^ b, U256::from_u64(0b0110));
    }

    #[test]
    fn test_shift_left() {
        let a = U256::from_u64(1);
        let b = a << 64;
        assert_eq!(b.0, [0, 1, 0, 0]);

        let c = a << 128;
        assert_eq!(c.0, [0, 0, 1, 0]);
    }

    #[test]
    fn test_shift_right() {
        let a = U256([0, 1, 0, 0]); // 2^64
        let b = a >> 64;
        assert_eq!(b, U256::from_u64(1));
    }

    #[test]
    fn test_hex_conversion() {
        let a = U256::from_hex("0xff").unwrap();
        assert_eq!(a, U256::from_u64(255));

        let b = U256::from_hex("0x100").unwrap();
        assert_eq!(b, U256::from_u64(256));

        // Full 256-bit number
        let c = U256::MAX;
        let hex = c.to_hex();
        let d = U256::from_hex(&hex).unwrap();
        assert_eq!(c, d);
    }

    #[test]
    fn test_bits() {
        assert_eq!(U256::ZERO.bits(), 0);
        assert_eq!(U256::ONE.bits(), 1);
        assert_eq!(U256::from_u64(255).bits(), 8);
        assert_eq!(U256::from_u64(256).bits(), 9);
        assert_eq!(U256::MAX.bits(), 256);
    }
}
