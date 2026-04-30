//! EVM Executor - Executes transactions
//!
//! Provides transaction execution capabilities:
//! - Simple value transfers
//! - Contract deployment (placeholder)
//! - Contract calls (placeholder)

use eth_primitives::{Address, H256, U256};

use crate::types::{SignedTransaction, Receipt, Log, Account};
use crate::state::WorldState;
use crate::error::{MiniEthError, Result};

/// Gas costs
pub mod gas {
    pub const TX_BASE: u64 = 21000;
    pub const TX_DATA_ZERO: u64 = 4;
    pub const TX_DATA_NONZERO: u64 = 16;
    pub const TX_CREATE: u64 = 32000;
}

/// Transaction executor
#[derive(Clone)]
pub struct Executor {
    /// Chain ID
    chain_id: u64,
}

impl Executor {
    /// Create a new executor
    pub fn new(chain_id: u64) -> Self {
        Executor { chain_id }
    }

    /// Execute a transaction
    pub fn execute_tx(
        &self,
        tx: &SignedTransaction,
        state: &mut WorldState,
        block_number: u64,
        _block_timestamp: u64,
        base_fee: u64,
        beneficiary: Address,
    ) -> Result<Receipt> {
        // Take snapshot for potential revert
        let snapshot = state.snapshot();

        // Validate nonce
        let expected_nonce = state.get_nonce(tx.from)?;
        if tx.nonce != expected_nonce {
            return Err(MiniEthError::NonceTooLow {
                expected: expected_nonce,
                got: tx.nonce,
            });
        }

        // Calculate gas costs
        let intrinsic_gas = self.calculate_intrinsic_gas(tx);
        if tx.gas_limit < intrinsic_gas {
            return Err(MiniEthError::Transaction("Gas limit too low".into()));
        }

        // Calculate effective gas price
        let effective_gas_price = std::cmp::min(
            tx.max_fee_per_gas,
            base_fee + tx.max_priority_fee_per_gas
        );
        let max_gas_cost = U256::from(tx.gas_limit) * U256::from(effective_gas_price);

        // Check balance
        let total_cost = max_gas_cost + tx.value;
        let balance = state.get_balance(tx.from)?;
        if balance < total_cost {
            return Err(MiniEthError::InsufficientBalance {
                required: total_cost,
                available: balance,
            });
        }

        // Deduct gas upfront
        state.sub_balance(tx.from, max_gas_cost)?;

        // Increment nonce
        state.increment_nonce(tx.from)?;

        // Execute based on transaction type
        let (success, gas_used, logs, return_data, contract_address) = if tx.is_contract_creation() {
            self.execute_create(tx, state)?
        } else {
            self.execute_call(tx, state)?
        };

        // Refund unused gas
        let gas_refund = tx.gas_limit.saturating_sub(gas_used);
        let refund_amount = U256::from(gas_refund) * U256::from(effective_gas_price);
        state.add_balance(tx.from, refund_amount)?;

        // Pay beneficiary (miner/validator)
        let priority_fee = effective_gas_price.saturating_sub(base_fee);
        let beneficiary_reward = U256::from(gas_used) * U256::from(priority_fee);
        state.add_balance(beneficiary, beneficiary_reward)?;

        // Revert on failure
        if !success {
            state.revert(snapshot);
        }

        Ok(Receipt {
            tx_hash: tx.hash,
            block_hash: H256::zero(),
            block_number,
            tx_index: 0,
            from: tx.from,
            to: tx.to,
            contract_address,
            gas_used,
            cumulative_gas_used: gas_used,
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
    ) -> Result<(bool, u64, Vec<Log>, Vec<u8>, Option<Address>)> {
        // Compute contract address
        let contract_address = WorldState::compute_contract_address(tx.from, tx.nonce);

        // Create account with value
        if tx.value > U256::zero() {
            state.sub_balance(tx.from, tx.value)?;
            state.add_balance(contract_address, tx.value)?;
        }

        // Store bytecode (simplified - in real impl would execute constructor)
        if !tx.data.is_empty() {
            state.set_code(contract_address, tx.data.clone())?;
        }

        let gas_used = gas::TX_BASE + gas::TX_CREATE + self.data_gas(&tx.data);

        Ok((true, gas_used, vec![], vec![], Some(contract_address)))
    }

    /// Execute contract call or simple transfer
    fn execute_call(
        &self,
        tx: &SignedTransaction,
        state: &mut WorldState,
    ) -> Result<(bool, u64, Vec<Log>, Vec<u8>, Option<Address>)> {
        let to = tx.to.ok_or_else(|| MiniEthError::Transaction("Missing recipient".into()))?;

        // Transfer value
        if tx.value > U256::zero() {
            state.sub_balance(tx.from, tx.value)?;
            state.add_balance(to, tx.value)?;
        }

        // Check if contract (has code)
        let code = state.get_code(to)?;

        if code.is_empty() {
            // Simple transfer to EOA
            let gas_used = gas::TX_BASE + self.data_gas(&tx.data);
            return Ok((true, gas_used, vec![], vec![], None));
        }

        // Contract call - simplified execution
        // In a full implementation, this would run the EVM
        let gas_used = gas::TX_BASE + self.data_gas(&tx.data) + 10000; // Placeholder

        Ok((true, gas_used, vec![], vec![], None))
    }

    /// Call contract (read-only, for eth_call)
    pub fn call(
        &self,
        _from: Address,
        to: Option<Address>,
        _value: U256,
        _data: Vec<u8>,
        state: &mut WorldState,
    ) -> Result<Vec<u8>> {
        if let Some(addr) = to {
            let code = state.get_code(addr)?;
            if code.is_empty() {
                return Ok(vec![]);
            }
            // Simplified - would execute EVM
            Ok(vec![])
        } else {
            Ok(vec![])
        }
    }

    /// Calculate intrinsic gas cost
    fn calculate_intrinsic_gas(&self, tx: &SignedTransaction) -> u64 {
        let mut gas = gas::TX_BASE;
        gas += self.data_gas(&tx.data);
        if tx.is_contract_creation() {
            gas += gas::TX_CREATE;
        }
        gas
    }

    /// Calculate gas for data bytes
    fn data_gas(&self, data: &[u8]) -> u64 {
        let mut gas = 0u64;
        for byte in data {
            if *byte == 0 {
                gas += gas::TX_DATA_ZERO;
            } else {
                gas += gas::TX_DATA_NONZERO;
            }
        }
        gas
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new(1337)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_transfer() {
        let mut state = WorldState::new();
        let executor = Executor::default();

        let sender = Address::zero();
        let recipient = Address::from([1u8; 20]);

        // Fund sender
        state.set_balance(sender, U256::from(1_000_000_000_000_000_000u128)).unwrap();

        let tx = SignedTransaction {
            from: sender,
            to: Some(recipient),
            value: U256::from(1_000_000_000_000_000u64),
            data: vec![],
            nonce: 0,
            gas_limit: 21000,
            max_fee_per_gas: 1_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            hash: H256::zero(),
            signature: vec![],
        };

        let beneficiary = Address::zero();

        let receipt = executor.execute_tx(
            &tx, &mut state, 1, 1000, 1_000_000_000, beneficiary
        ).unwrap();

        assert!(receipt.status);
        assert_eq!(receipt.gas_used, 21000);

        // Check recipient got value
        assert!(state.get_balance(recipient).unwrap() > U256::zero());
    }

    #[test]
    fn test_contract_creation() {
        let mut state = WorldState::new();
        let executor = Executor::default();

        let sender = Address::zero();

        // Fund sender
        state.set_balance(sender, U256::from(1_000_000_000_000_000_000u128)).unwrap();

        let bytecode = vec![0x60, 0x80, 0x60, 0x40, 0x52]; // Simple bytecode

        let tx = SignedTransaction {
            from: sender,
            to: None, // Contract creation
            value: U256::zero(),
            data: bytecode.clone(),
            nonce: 0,
            gas_limit: 100_000,
            max_fee_per_gas: 1_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            hash: H256::zero(),
            signature: vec![],
        };

        let beneficiary = Address::zero();

        let receipt = executor.execute_tx(
            &tx, &mut state, 1, 1000, 1_000_000_000, beneficiary
        ).unwrap();

        assert!(receipt.status);
        assert!(receipt.contract_address.is_some());

        // Check code was stored
        let contract = receipt.contract_address.unwrap();
        let stored_code = state.get_code(contract).unwrap();
        assert_eq!(stored_code, bytecode);
    }
}
