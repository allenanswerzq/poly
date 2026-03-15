//! # Execution Context
//!
//! Contains all information needed for EVM execution:
//! - Call context (caller, value, data)
//! - Block context (number, timestamp, etc.)
//! - Transaction context (origin, gas price)

use eth_primitives::{Address, H256, U256};

/// Call/message context
#[derive(Debug, Clone)]
pub struct CallContext {
    /// Current executing address
    pub address: Address,
    /// Caller address
    pub caller: Address,
    /// Call value (wei)
    pub value: U256,
    /// Input data (calldata)
    pub data: Vec<u8>,
    /// Gas available
    pub gas: u64,
    /// Is this a static call (read-only)
    pub is_static: bool,
    /// Call depth
    pub depth: usize,
}

impl Default for CallContext {
    fn default() -> Self {
        CallContext {
            address: Address::zero(),
            caller: Address::zero(),
            value: U256::ZERO,
            data: Vec::new(),
            gas: 1_000_000,
            is_static: false,
            depth: 0,
        }
    }
}

impl CallContext {
    /// Create a new call context
    pub fn new(
        address: Address,
        caller: Address,
        value: U256,
        data: Vec<u8>,
        gas: u64,
    ) -> Self {
        CallContext {
            address,
            caller,
            value,
            data,
            gas,
            is_static: false,
            depth: 0,
        }
    }
}

/// Block context
#[derive(Debug, Clone)]
pub struct BlockContext {
    /// Block number
    pub number: u64,
    /// Block timestamp
    pub timestamp: u64,
    /// Block gas limit
    pub gas_limit: u64,
    /// Coinbase (block producer)
    pub coinbase: Address,
    /// Previous block's randao/difficulty
    pub prevrandao: H256,
    /// Base fee per gas (EIP-1559)
    pub base_fee: U256,
    /// Chain ID
    pub chain_id: u64,
    /// Blob base fee (EIP-4844)
    pub blob_base_fee: U256,
}

impl Default for BlockContext {
    fn default() -> Self {
        BlockContext {
            number: 1,
            timestamp: 1704067200, // Jan 1, 2024
            gas_limit: 30_000_000,
            coinbase: Address::zero(),
            prevrandao: H256::zero(),
            base_fee: U256::from_u64(1_000_000_000), // 1 gwei
            chain_id: 1,
            blob_base_fee: U256::from_u64(1),
        }
    }
}

/// Transaction context
#[derive(Debug, Clone)]
pub struct TxContext {
    /// Transaction origin (original sender)
    pub origin: Address,
    /// Gas price
    pub gas_price: U256,
    /// Blob hashes (EIP-4844)
    pub blob_hashes: Vec<H256>,
}

impl Default for TxContext {
    fn default() -> Self {
        TxContext {
            origin: Address::zero(),
            gas_price: U256::from_u64(1_000_000_000), // 1 gwei
            blob_hashes: Vec::new(),
        }
    }
}

/// Complete execution context
#[derive(Debug, Clone, Default)]
pub struct ExecutionContext {
    pub call: CallContext,
    pub block: BlockContext,
    pub tx: TxContext,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new(call: CallContext, block: BlockContext, tx: TxContext) -> Self {
        ExecutionContext { call, block, tx }
    }

    /// Simple context for testing
    pub fn simple(code_address: Address, caller: Address, value: U256, data: Vec<u8>) -> Self {
        let mut ctx = ExecutionContext::default();
        ctx.call.address = code_address;
        ctx.call.caller = caller;
        ctx.call.value = value;
        ctx.call.data = data;
        ctx.tx.origin = caller;
        ctx
    }
}

/// Log entry (emitted by LOG0-LOG4)
#[derive(Debug, Clone)]
pub struct Log {
    /// Contract that emitted the log
    pub address: Address,
    /// Topics (0-4)
    pub topics: Vec<H256>,
    /// Log data
    pub data: Vec<u8>,
}

impl Log {
    pub fn new(address: Address, topics: Vec<H256>, data: Vec<u8>) -> Self {
        Log { address, topics, data }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_context() {
        let ctx = ExecutionContext::default();
        assert_eq!(ctx.call.gas, 1_000_000);
        assert_eq!(ctx.block.chain_id, 1);
    }

    #[test]
    fn test_simple_context() {
        let addr = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let caller = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        let ctx = ExecutionContext::simple(addr, caller, U256::from_u64(100), vec![1, 2, 3]);

        assert_eq!(ctx.call.address, addr);
        assert_eq!(ctx.call.caller, caller);
        assert_eq!(ctx.call.value, U256::from_u64(100));
        assert_eq!(ctx.call.data, vec![1, 2, 3]);
    }
}
