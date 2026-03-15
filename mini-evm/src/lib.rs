//! # Mini EVM
//!
//! A minimal Ethereum Virtual Machine interpreter.
//!
//! This implements the core EVM execution model:
//! - Stack-based architecture (1024 items max)
//! - Expandable memory
//! - Persistent storage
//! - Gas metering

pub mod opcode;
pub mod stack;
pub mod memory;
pub mod storage;
pub mod context;
pub mod interpreter;
pub mod error;

pub use opcode::Opcode;
pub use stack::Stack;
pub use memory::Memory;
pub use storage::Storage;
pub use context::ExecutionContext;
pub use interpreter::{Interpreter, ExecutionResult};
pub use error::EvmError;

use storage::StateDB;

/// Default gas limit for execution
const DEFAULT_GAS_LIMIT: u64 = 10_000_000;

/// Execute bytecode with given context
pub fn execute(code: &[u8], ctx: &ExecutionContext, state: &mut StateDB) -> ExecutionResult {
    let mut interpreter = Interpreter::new(code.to_vec(), DEFAULT_GAS_LIMIT);
    interpreter.run(ctx, state)
}

