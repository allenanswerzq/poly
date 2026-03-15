//! EVM Executor - Executes transactions using mini-evm
//!
//! Integrates mini-evm with world state for:
//! - Transaction execution
//! - Smart contract deployment
//! - Contract calls

use eth_primitives::{Address, H256, U256};
use mini_evm::{Interpreter, ExecutionContext, ExecutionResult};
use mini_evm::storage::StateDB;

use crate::types::{SignedTransaction, Receipt, Log};
use crate::state::WorldState;
use crate::error::{MiniEthError, Result};

/// Gas costs
pub mod gas {
    pub const TX_BASE: u64 = 21000;
    pub const TX_DATA_ZERO: u64 = 4;
    pub const TX_DATA_NONZERO: u64 = 16;
    pub const TX_CREATE: u64 = 32000;
    pub const CALL_VALUE: u64 = 9000;
    pub const CALL_NEW_ACCOUNT: u64 = 25000;
}

/// Transaction executor
pub struct Executor {
    /// Gas limit for blocks
    block_gas_limit: u64,
    /// Chain ID
    chain_id: u64,
}

impl Executor {
    /// Create a new executor
    pub fn new(block_gas_limit: u64, chain_id: u64) -> Self {
        Executor {
            block_gas_limit,
            chain_id,
        }
    }

    /// Execute a transaction
    pub fn execute_tx(
        &self,
        tx: &SignedTransaction,
        state: &mut WorldState,
        block_number: u64,
        block_timestamp: u64,
        base_fee: u64,
        beneficiary: Address,
    ) -> Result<Receipt> {
        // Take snapshot for potential revert
        let snapshot = state.snapshot();
        
        // Validate nonce
        let expected_nonce = state.get_nonce(&tx.from);
        if tx.nonce != expected_nonce {
            return Err(MiniEthError::NonceTooLow {
                expected: expected_nonce,
                got: tx.nonce,
            });
        }
        
        // Calculate gas costs
        let intrinsic_gas = self.calculate_intrinsic_gas(tx);
        if tx.gas_limit < intrinsic_gas {
            return Err(MiniEthError::GasLimitExceeded);
        }
        
        // Calculate effective gas price
        let effective_gas_price = tx.effective_gas_price(base_fee);
        let max_gas_cost = U256::from(tx.gas_limit) * U256::from(effective_gas_price);
        
        // Check balance
        let total_cost = max_gas_cost + tx.value;
        let balance = state.get_balance(&tx.from);
        if balance < total_cost {
            return Err(MiniEthError::InsufficientBalance);
        }
        
        // Deduct gas upfront
        state.sub_balance(&tx.from, max_gas_cost)?;
        
        // Increment nonce
        state.increment_nonce(&tx.from);
        
        // Execute
        let (success, gas_used, logs, return_data, contract_address) = if tx.is_contract_creation() {
            self.execute_create(tx, state, block_number, block_timestamp)?
        } else {
            self.execute_call(tx, state, block_number, block_timestamp)?
        };
        
        // Refund unused gas
        let gas_refund = tx.gas_limit.saturating_sub(gas_used);
        let refund_amount = U256::from(gas_refund) * U256::from(effective_gas_price);
        state.add_balance(&tx.from, refund_amount)?;
        
        // Pay beneficiary (miner/validator)
        let priority_fee = effective_gas_price.saturating_sub(base_fee);
        let beneficiary_reward = U256::from(gas_used) * U256::from(priority_fee);
        state.add_balance(&beneficiary, beneficiary_reward)?;
        
        // Revert if failed
        if !success {
            state.revert(snapshot);
            // But keep nonce increment and gas deduction
            state.increment_nonce(&tx.from);
            state.sub_balance(&tx.from, U256::from(gas_used) * U256::from(effective_gas_price))?;
        }
        
        Ok(Receipt {
            tx_hash: tx.hash,
            block_hash: H256::zero(), // Set by caller
            block_number,
            tx_index: 0, // Set by caller
            from: tx.from,
            to: tx.to,
            contract_address,
            gas_used,
            cumulative_gas_used: gas_used, // Set by caller
            status: success,
            logs,
            return_data,
        })
    }

    /// Execute contract creation
    fn execute_create(
        &self,
        tx: &SignedTransaction,
        state: &mut WorldState,
        block_number: u64,
        block_timestamp: u64,
    ) -> Result<(bool, u64, Vec<Log>, Vec<u8>, Option<Address>)> {
        // Compute contract address
        let contract_address = WorldState::compute_contract_address(&tx.from, tx.nonce);
        
        // Check if address already exists
        if state.account_exists(&contract_address) && state.get_account(&contract_address).is_contract() {
            return Ok((false, tx.gas_limit, vec![], vec![], None));
        }
        
        // Create account
        let mut account = Account::new();
        account.balance = tx.value;
        state.set_account(contract_address, account);
        
        // Transfer value
        if tx.value > U256::zero() {
            state.sub_balance(&tx.from, tx.value)?;
        }
        
        // Execute constructor
        let gas_available = tx.gas_limit - gas::TX_BASE - gas::TX_CREATE;
        
        if tx.data.is_empty() {
            // No code to execute
            return Ok((true, gas::TX_BASE + gas::TX_CREATE, vec![], vec![], Some(contract_address)));
        }
        
        // Create execution context
        let ctx = ExecutionContext {
            address: contract_address,
            caller: tx.from,
            origin: tx.from,
            value: tx.value,
            gas_price: U256::from(tx.max_fee_per_gas),
            gas_limit: gas_available,
            block_number,
            block_timestamp,
            chain_id: self.chain_id,
        };
        
        // Create state bridge
        let mut evm_state = EvmStateAdapter::new(state, contract_address);
        
        // Run constructor
        let mut interpreter = Interpreter::new(tx.data.clone(), gas_available);
        let result = interpreter.run(&ctx, &mut evm_state);
        
        match result {
            ExecutionResult::Success { return_data, gas_used, .. } => {
                // Store deployed code (return data is the runtime code)
                if !return_data.is_empty() {
                    state.set_code(&contract_address, return_data.clone());
                }
                
                let logs = evm_state.take_logs();
                let total_gas = gas::TX_BASE + gas::TX_CREATE + gas_used;
                
                Ok((true, total_gas, logs, return_data, Some(contract_address)))
            }
            ExecutionResult::Revert { return_data, gas_used } => {
                let total_gas = gas::TX_BASE + gas::TX_CREATE + gas_used;
                Ok((false, total_gas, vec![], return_data, None))
            }
            ExecutionResult::Error { gas_used, .. } => {
                let total_gas = gas::TX_BASE + gas::TX_CREATE + gas_used;
                Ok((false, total_gas, vec![], vec![], None))
            }
        }
    }

    /// Execute contract call or simple transfer
    fn execute_call(
        &self,
        tx: &SignedTransaction,
        state: &mut WorldState,
        block_number: u64,
        block_timestamp: u64,
    ) -> Result<(bool, u64, Vec<Log>, Vec<u8>, Option<Address>)> {
        let to = tx.to.ok_or_else(|| MiniEthError::Transaction("Missing recipient".into()))?;
        
        // Transfer value
        if tx.value > U256::zero() {
            state.sub_balance(&tx.from, tx.value)?;
            state.add_balance(&to, tx.value)?;
        }
        
        // Check if this is a simple transfer or contract call
        let code = state.get_code(&to);
        
        if code.is_none() || tx.data.is_empty() {
            // Simple transfer or call to EOA
            let gas_used = self.calculate_intrinsic_gas(tx);
            return Ok((true, gas_used, vec![], vec![], None));
        }
        
        let code = code.unwrap();
        let gas_available = tx.gas_limit - self.calculate_intrinsic_gas(tx);
        
        // Create execution context
        let ctx = ExecutionContext {
            address: to,
            caller: tx.from,
            origin: tx.from,
            value: tx.value,
            gas_price: U256::from(tx.max_fee_per_gas),
            gas_limit: gas_available,
            block_number,
            block_timestamp,
            chain_id: self.chain_id,
        };
        
        // Create state bridge
        let mut evm_state = EvmStateAdapter::new(state, to);
        
        // Set input data
        let mut interpreter = Interpreter::new(code, gas_available);
        interpreter.set_calldata(tx.data.clone());
        
        let result = interpreter.run(&ctx, &mut evm_state);
        
        match result {
            ExecutionResult::Success { return_data, gas_used, .. } => {
                let logs = evm_state.take_logs();
                let total_gas = self.calculate_intrinsic_gas(tx) + gas_used;
                Ok((true, total_gas, logs, return_data, None))
            }
            ExecutionResult::Revert { return_data, gas_used } => {
                let total_gas = self.calculate_intrinsic_gas(tx) + gas_used;
                Ok((false, total_gas, vec![], return_data, None))
            }
            ExecutionResult::Error { gas_used, .. } => {
                let total_gas = self.calculate_intrinsic_gas(tx) + gas_used;
                Ok((false, total_gas, vec![], vec![], None))
            }
        }
    }

    /// Call contract without state modification (eth_call)
    pub fn call(
        &self,
        from: Address,
        to: Address,
        data: Vec<u8>,
        value: U256,
        gas_limit: u64,
        state: &WorldState,
        block_number: u64,
        block_timestamp: u64,
    ) -> Result<Vec<u8>> {
        let code = state.get_code(&to)
            .ok_or_else(|| MiniEthError::ContractNotFound(to.to_string()))?;
        
        let ctx = ExecutionContext {
            address: to,
            caller: from,
            origin: from,
            value,
            gas_price: U256::zero(),
            gas_limit,
            block_number,
            block_timestamp,
            chain_id: self.chain_id,
        };
        
        // Clone state for read-only execution
        let mut temp_state = state.snapshot();
        let mut state_copy = WorldState::new();
        state_copy.revert(temp_state);
        
        let mut evm_state = EvmStateAdapter::new(&mut state_copy, to);
        
        let mut interpreter = Interpreter::new(code, gas_limit);
        interpreter.set_calldata(data);
        
        let result = interpreter.run(&ctx, &mut evm_state);
        
        match result {
            ExecutionResult::Success { return_data, .. } => Ok(return_data),
            ExecutionResult::Revert { return_data, .. } => {
                Err(MiniEthError::Evm(format!("Reverted: {}", hex::encode(&return_data))))
            }
            ExecutionResult::Error { error, .. } => {
                Err(MiniEthError::Evm(error))
            }
        }
    }

    /// Calculate intrinsic gas cost
    fn calculate_intrinsic_gas(&self, tx: &SignedTransaction) -> u64 {
        let mut gas = gas::TX_BASE;
        
        // Add data costs
        for byte in &tx.data {
            if *byte == 0 {
                gas += gas::TX_DATA_ZERO;
            } else {
                gas += gas::TX_DATA_NONZERO;
            }
        }
        
        // Add create cost
        if tx.is_contract_creation() {
            gas += gas::TX_CREATE;
        }
        
        gas
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new(30_000_000, 1337)
    }
}

/// Adapter to bridge WorldState to mini-evm's Storage trait
struct EvmStateAdapter<'a> {
    state: &'a mut WorldState,
    contract_address: Address,
    logs: Vec<Log>,
}

impl<'a> EvmStateAdapter<'a> {
    fn new(state: &'a mut WorldState, contract_address: Address) -> Self {
        EvmStateAdapter {
            state,
            contract_address,
            logs: Vec::new(),
        }
    }

    fn take_logs(self) -> Vec<Log> {
        self.logs
    }
}

impl<'a> Storage for EvmStateAdapter<'a> {
    fn get(&self, key: &[u8; 32]) -> [u8; 32] {
        let slot = H256::from_bytes(*key);
        let value = self.state.get_storage(&self.contract_address, &slot);
        *value.as_bytes()
    }

    fn set(&mut self, key: [u8; 32], value: [u8; 32]) {
        let slot = H256::from_bytes(key);
        let val = H256::from_bytes(value);
        self.state.set_storage(&self.contract_address, slot, val);
    }

    fn get_balance(&self, address: &[u8; 20]) -> [u8; 32] {
        let addr = Address::from_bytes(*address);
        let balance = self.state.get_balance(&addr);
        balance.to_be_bytes()
    }

    fn get_code(&self, address: &[u8; 20]) -> Vec<u8> {
        let addr = Address::from_bytes(*address);
        self.state.get_code(&addr).unwrap_or_default()
    }

    fn emit_log(&mut self, topics: Vec<[u8; 32]>, data: Vec<u8>) {
        self.logs.push(Log {
            address: self.contract_address,
            topics: topics.into_iter().map(H256::from_bytes).collect(),
            data,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_transfer() {
        let mut state = WorldState::new();
        let executor = Executor::default();
        
        let sender = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let recipient = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();
        
        // Fund sender
        state.set_balance(&sender, U256::from(1_000_000_000_000_000_000u64));
        
        let tx = SignedTransaction {
            from: sender,
            to: Some(recipient),
            value: U256::from(1_000_000_000_000_000u64), // 0.001 ETH
            data: vec![],
            nonce: 0,
            gas_limit: 21000,
            max_fee_per_gas: 1_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            hash: H256::zero(),
            signature: vec![],
        };
        
        let beneficiary = Address::from_hex("0x0000000000000000000000000000000000000000").unwrap();
        
        let receipt = executor.execute_tx(
            &tx, &mut state, 1, 1000, 1_000_000_000, beneficiary
        ).unwrap();
        
        assert!(receipt.status);
        assert_eq!(receipt.gas_used, 21000);
        
        // Check balances
        assert!(state.get_balance(&recipient) > U256::zero());
    }
}
