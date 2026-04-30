//! Custom error types for the library

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EthError {
    #[error("Invalid hex string: {0}")]
    InvalidHex(String),

    #[error("Invalid address length: expected 20 bytes, got {0}")]
    InvalidAddressLength(usize),

    #[error("Invalid hash length: expected 32 bytes, got {0}")]
    InvalidHashLength(usize),

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Arithmetic overflow")]
    Overflow,

    #[error("Arithmetic underflow")]
    Underflow,

    #[error("Division by zero")]
    DivisionByZero,

    #[error("RLP decoding error: {0}")]
    RlpError(String),

    #[error("Invalid recovery id: {0}")]
    InvalidRecoveryId(u8),
}

pub type Result<T> = std::result::Result<T, EthError>;
