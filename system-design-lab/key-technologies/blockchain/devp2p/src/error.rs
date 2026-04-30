//! # P2P Error types

use thiserror::Error;

/// P2P error types
#[derive(Debug, Error)]
pub enum P2pError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Timeout")]
    Timeout,

    #[error("Disconnected: {0}")]
    Disconnected(String),

    #[error("Crypto error: {0}")]
    CryptoError(String),

    #[error("RLP decode error: {0}")]
    RlpError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Node not found: {0}")]
    NodeNotFound(String),
}

pub type Result<T> = std::result::Result<T, P2pError>;
