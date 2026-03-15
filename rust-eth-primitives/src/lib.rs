//! # Ethereum Primitives Library
//!
//! Low-level implementation of Ethereum data structures:
//! - Address (20-byte with EIP-55 checksum)
//! - H256 (32-byte hash, Keccak256)
//! - U256 (256-bit unsigned integer)
//! - RLP encoding/decoding
//! - Transactions (Legacy, EIP-1559, EIP-4844)
//! - ECDSA Signatures (secp256k1)

pub mod address;
pub mod hash;
pub mod uint;
pub mod rlp;
pub mod transaction;
pub mod signature;
pub mod error;

// Re-exports for convenience
pub use address::Address;
pub use hash::{H256, keccak256};
pub use uint::U256;
pub use signature::Signature;
pub use transaction::{Transaction, TxType};
pub use error::EthError;
