//! EVM Error types

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum EvmError {
    #[error("Stack overflow: maximum 1024 items")]
    StackOverflow,

    #[error("Stack underflow: not enough items")]
    StackUnderflow,

    #[error("Invalid jump destination: {0}")]
    InvalidJump(usize),

    #[error("Out of gas: needed {needed}, had {had}")]
    OutOfGas { needed: u64, had: u64 },

    #[error("Invalid opcode: 0x{0:02x}")]
    InvalidOpcode(u8),

    #[error("Write in static context")]
    WriteInStaticContext,

    #[error("Return data out of bounds")]
    ReturnDataOutOfBounds,

    #[error("Invalid memory access")]
    InvalidMemoryAccess,

    #[error("Revert: {0}")]
    Revert(String),

    #[error("Contract creation failed")]
    CreateFailed,

    #[error("Call depth exceeded")]
    CallDepthExceeded,

    #[error("Execution stopped")]
    Stop,
}

pub type Result<T> = std::result::Result<T, EvmError>;
