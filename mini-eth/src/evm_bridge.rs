//! EVM Bridge - Connects mini-eth state to mini-evm execution
//!
//! This module bridges the WorldState from mini-eth to the StateDB from mini-evm,
//! enabling full smart contract execution.

use eth_primitives::{Address, H256, U256, keccak256};
use mini_evm::{execute, ExecutionContext, ExecutionResult};
use mini_evm::context::{CallContext, BlockContext, TxContext};
use mini_evm::storage::StateDB;

use crate::state::WorldState;
use crate::types::{SignedTransaction, Log};
use crate::error::{MiniEthError, Result};

/// EVM execution wrapper
pub struct EvmBridge {
    /// Chain ID
    chain_id: u64,
}

impl EvmBridge {
    /// Create new EVM bridge
    pub fn new(chain_id: u64) -> Self {
        EvmBridge { chain_id }
    }

    /// Convert WorldState to mini-evm StateDB
    fn world_to_statedb(&self, world: &WorldState, addresses: &[Address]) -> StateDB {
        let mut statedb = StateDB::new();

        for addr in addresses {
            // Copy balance
            if let Ok(balance) = world.get_balance(*addr) {
                statedb.set_balance(*addr, balance);
            }

            // Copy nonce
            if let Ok(nonce) = world.get_nonce(*addr) {
                for _ in 0..nonce {
                    statedb.increment_nonce(addr);
                }
            }

            // Copy code
            if let Ok(code) = world.get_code(*addr) {
                if !code.is_empty() {
                    statedb.set_code(*addr, code);
                }
            }

            // Note: Storage copying would need iteration support in WorldState
        }

        statedb
    }

    /// Apply StateDB changes back to WorldState
    fn apply_statedb_changes(
        &self,
        statedb: &StateDB,
        world: &mut WorldState,
        modified_addresses: &[Address],
    ) -> Result<()> {
        for addr in modified_addresses {
            // Update balance
            let new_balance = statedb.balance(addr);
            world.set_balance(*addr, new_balance)?;

            // Update nonce
            let new_nonce = statedb.nonce(addr);
            world.set_nonce(*addr, new_nonce)?;

            // Update code if it changed
            let code = statedb.code(addr);
            if !code.is_empty() {
                world.set_code(*addr, code.to_vec())?;
            }
        }

        Ok(())
    }

    /// Execute contract creation
    pub fn execute_create(
        &self,
        tx: &SignedTransaction,
        world: &mut WorldState,
        block_number: u64,
        block_timestamp: u64,
        coinbase: Address,
    ) -> Result<(bool, u64, Vec<Log>, Vec<u8>, Option<Address>)> {
        // Compute contract address
        let contract_address = WorldState::compute_contract_address(tx.from, tx.nonce);

        // Create execution context
        let call = CallContext {
            address: contract_address,
            caller: tx.from,
            value: tx.value,
            data: vec![], // For create, init code is executed separately
            gas: tx.gas_limit,
            is_static: false,
            depth: 0,
        };

        let block = BlockContext {
            number: block_number,
            timestamp: block_timestamp,
            gas_limit: 30_000_000,
            coinbase,
            prevrandao: H256::zero(),
            base_fee: U256::from(tx.max_fee_per_gas),
            chain_id: self.chain_id,
            blob_base_fee: U256::from(1u64),
        };

        let tx_ctx = TxContext {
            origin: tx.from,
            gas_price: U256::from(tx.max_fee_per_gas),
            blob_hashes: vec![],
        };

        let ctx = ExecutionContext::new(call, block, tx_ctx);

        // Create StateDB with relevant accounts
        let mut statedb = self.world_to_statedb(world, &[tx.from, contract_address]);

        // Run init code (the tx.data is the constructor bytecode)
        let result = execute(&tx.data, &ctx, &mut statedb);

        if result.success {
            // The output of constructor is the runtime bytecode
            if !result.output.is_empty() {
                statedb.set_code(contract_address, result.output.clone());
            } else {
                // If no output, store the init code as runtime code (simplified)
                statedb.set_code(contract_address, tx.data.clone());
            }

            // Apply changes back
            self.apply_statedb_changes(&statedb, world, &[tx.from, contract_address])?;
        }

        let logs = result.logs.iter().map(|l| Log {
            address: l.address,
            topics: l.topics.clone(),
            data: l.data.clone(),
        }).collect();

        Ok((
            result.success,
            result.gas_used,
            logs,
            result.output,
            Some(contract_address),
        ))
    }

    /// Execute contract call
    pub fn execute_call(
        &self,
        tx: &SignedTransaction,
        world: &mut WorldState,
        block_number: u64,
        block_timestamp: u64,
        coinbase: Address,
    ) -> Result<(bool, u64, Vec<Log>, Vec<u8>, Option<Address>)> {
        let to = tx.to.ok_or_else(|| MiniEthError::Transaction("No recipient".into()))?;

        // Get contract code
        let code = world.get_code(to)?;

        if code.is_empty() {
            // Simple value transfer
            if tx.value > U256::zero() {
                world.sub_balance(tx.from, tx.value)?;
                world.add_balance(to, tx.value)?;
            }
            return Ok((true, 21000, vec![], vec![], None));
        }

        // Create execution context
        let call = CallContext {
            address: to,
            caller: tx.from,
            value: tx.value,
            data: tx.data.clone(),
            gas: tx.gas_limit,
            is_static: false,
            depth: 0,
        };

        let block = BlockContext {
            number: block_number,
            timestamp: block_timestamp,
            gas_limit: 30_000_000,
            coinbase,
            prevrandao: H256::zero(),
            base_fee: U256::from(tx.max_fee_per_gas),
            chain_id: self.chain_id,
            blob_base_fee: U256::from(1u64),
        };

        let tx_ctx = TxContext {
            origin: tx.from,
            gas_price: U256::from(tx.max_fee_per_gas),
            blob_hashes: vec![],
        };

        let ctx = ExecutionContext::new(call, block, tx_ctx);

        // Create StateDB with relevant accounts
        let mut statedb = self.world_to_statedb(world, &[tx.from, to]);

        // Handle value transfer
        if tx.value > U256::zero() {
            let sender_balance = statedb.balance(&tx.from);
            statedb.set_balance(tx.from, sender_balance - tx.value);
            let recipient_balance = statedb.balance(&to);
            statedb.set_balance(to, recipient_balance + tx.value);
        }

        // Execute contract
        let result = execute(&code, &ctx, &mut statedb);

        if result.success {
            // Apply changes back
            self.apply_statedb_changes(&statedb, world, &[tx.from, to])?;
        }

        let logs = result.logs.iter().map(|l| Log {
            address: l.address,
            topics: l.topics.clone(),
            data: l.data.clone(),
        }).collect();

        Ok((
            result.success,
            result.gas_used,
            logs,
            result.output,
            None,
        ))
    }

    /// Simulate a call (for eth_call, read-only)
    pub fn simulate_call(
        &self,
        from: Address,
        to: Address,
        value: U256,
        data: Vec<u8>,
        world: &WorldState,
        block_number: u64,
        block_timestamp: u64,
    ) -> Result<ExecutionResult> {
        let code = world.get_code(to)?;

        if code.is_empty() {
            return Ok(ExecutionResult::success(vec![], 21000, 0, vec![]));
        }

        let call = CallContext {
            address: to,
            caller: from,
            value,
            data,
            gas: 10_000_000,
            is_static: true, // Read-only call
            depth: 0,
        };

        let block = BlockContext {
            number: block_number,
            timestamp: block_timestamp,
            gas_limit: 30_000_000,
            coinbase: Address::zero(),
            prevrandao: H256::zero(),
            base_fee: U256::zero(),
            chain_id: self.chain_id,
            blob_base_fee: U256::from(1u64),
        };

        let tx_ctx = TxContext {
            origin: from,
            gas_price: U256::zero(),
            blob_hashes: vec![],
        };

        let ctx = ExecutionContext::new(call, block, tx_ctx);

        let mut statedb = self.world_to_statedb(world, &[from, to]);

        Ok(execute(&code, &ctx, &mut statedb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_create() {
        let mut world = WorldState::new();
        let bridge = EvmBridge::new(1337);

        let sender = Address::zero();
        world.set_balance(sender, U256::from(1_000_000_000_000_000_000u128)).unwrap();

        // Simple PUSH1 0x42 PUSH1 0 MSTORE PUSH1 32 PUSH1 0 RETURN
        // Stores 0x42 at memory and returns it
        let init_code = vec![
            0x60, 0x42,  // PUSH1 0x42
            0x60, 0x00,  // PUSH1 0
            0x52,        // MSTORE
            0x60, 0x20,  // PUSH1 32
            0x60, 0x00,  // PUSH1 0
            0xf3,        // RETURN
        ];

        let tx = SignedTransaction {
            from: sender,
            to: None,
            value: U256::zero(),
            data: init_code,
            nonce: 0,
            gas_limit: 100_000,
            max_fee_per_gas: 1_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            hash: H256::zero(),
            signature: vec![],
        };

        let (success, _gas, _logs, output, contract_addr) = bridge.execute_create(
            &tx, &mut world, 1, 1000, Address::zero()
        ).unwrap();

        assert!(success);
        assert!(contract_addr.is_some());
        // Output should contain 0x42 padded to 32 bytes
        assert!(!output.is_empty());
    }
}
