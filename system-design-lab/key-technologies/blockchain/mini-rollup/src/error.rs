//! # Rollup Error types

use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum RollupError {
    #[error("Insufficient balance: need {needed}, have {have}")]
    InsufficientBalance { needed: u64, have: u64 },

    #[error("Invalid nonce: expected {expected}, got {got}")]
    InvalidNonce { expected: u64, got: u64 },

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Invalid state root")]
    InvalidStateRoot,

    #[error("Batch too large: {size} > {max}")]
    BatchTooLarge { size: usize, max: usize },

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Invalid proof")]
    InvalidProof,

    #[error("Sequencer error: {0}")]
    SequencerError(String),
}

pub type Result<T> = std::result::Result<T, RollupError>;
