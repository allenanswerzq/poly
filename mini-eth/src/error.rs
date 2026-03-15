//! Error types for mini-eth

use thiserror::Error;

pub type Result<T> = std::result::Result<T, MiniEthError>;

#[derive(Debug, Error)]
pub enum MiniEthError {
    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("State error: {0}")]
    State(String),

    #[error("EVM execution error: {0}")]
    Evm(String),

    #[error("Consensus error: {0}")]
    Consensus(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Block error: {0}")]
    Block(String),

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: eth_primitives::U256, available: eth_primitives::U256 },

    #[error("Nonce too low: expected {expected}, got {got}")]
    NonceTooLow { expected: u64, got: u64 },

    #[error("Gas limit exceeded")]
    GasLimitExceeded,

    #[error("Contract not found: {0}")]
    ContractNotFound(String),

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Genesis already initialized")]
    GenesisAlreadyInitialized,

    #[error("Node not connected")]
    NotConnected,

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
