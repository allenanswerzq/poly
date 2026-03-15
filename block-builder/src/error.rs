//! # Error types for block builder

use thiserror::Error;

pub type Result<T> = std::result::Result<T, BuilderError>;

#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("Transaction already in mempool: {0}")]
    DuplicateTransaction(String),

    #[error("Mempool full, cannot add transaction")]
    MempoolFull,

    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),

    #[error("Block gas limit exceeded")]
    GasLimitExceeded,

    #[error("Invalid bundle: {0}")]
    InvalidBundle(String),

    #[error("No profitable opportunity found")]
    NoProfitableOpportunity,

    #[error("Build error: {0}")]
    BuildError(String),

    #[error("{0}")]
    Custom(String),

    #[error("Relay error: {0}")]
    RelayError(String),

    #[error("Proposer error: {0}")]
    ProposerError(String),

    #[error("Chain error: {0}")]
    ChainError(String),
}
