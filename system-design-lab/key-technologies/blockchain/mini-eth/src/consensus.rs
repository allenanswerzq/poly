//! Consensus - Block production and validation
//!
//! Simple PoA (Proof of Authority) consensus for the mini chain:
//! - Configurable block time
//! - Validator set
//! - Block validation rules

use std::collections::HashSet;
use std::sync::Arc;
use parking_lot::RwLock;
use eth_primitives::{Address, H256};

use crate::types::{Block, BlockHeader, SignedTransaction, LogsBloom};
use crate::state::WorldState;
use crate::executor::Executor;
use crate::error::{MiniEthError, Result};

/// Consensus configuration
#[derive(Debug, Clone)]
pub struct ConsensusConfig {
    /// Block time in seconds
    pub block_time: u64,
    /// Block gas limit
    pub block_gas_limit: u64,
    /// Initial validators
    pub validators: Vec<Address>,
    /// Chain ID
    pub chain_id: u64,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        ConsensusConfig {
            block_time: 2, // 2 second blocks
            block_gas_limit: 30_000_000,
            chain_id: 1337,
            validators: vec![],
        }
    }
}

/// Consensus engine
pub struct Consensus {
    /// Configuration
    config: ConsensusConfig,

    /// Validator set
    validators: Arc<RwLock<HashSet<Address>>>,

    /// Current validator index (for round-robin)
    current_validator_index: Arc<RwLock<usize>>,

    /// Transaction executor
    executor: Executor,
}

impl Consensus {
    /// Create a new consensus engine
    pub fn new(config: ConsensusConfig) -> Self {
        let validators: HashSet<_> = config.validators.iter().cloned().collect();
        let executor = Executor::new(config.chain_id);

        Consensus {
            config,
            validators: Arc::new(RwLock::new(validators)),
            current_validator_index: Arc::new(RwLock::new(0)),
            executor,
        }
    }

    /// Check if address is a validator
    pub fn is_validator(&self, address: &Address) -> bool {
        self.validators.read().contains(address)
    }

    /// Add a validator
    pub fn add_validator(&self, address: Address) {
        self.validators.write().insert(address);
    }

    /// Remove a validator
    pub fn remove_validator(&self, address: &Address) {
        self.validators.write().remove(address);
    }

    /// Get current proposer (round-robin)
    pub fn get_proposer(&self, block_number: u64) -> Option<Address> {
        let validators = self.validators.read();
        if validators.is_empty() {
            return None;
        }

        let validators_vec: Vec<_> = validators.iter().cloned().collect();
        let index = (block_number as usize) % validators_vec.len();
        Some(validators_vec[index])
    }

    /// Create a new block
    pub fn create_block(
        &self,
        parent: &Block,
        transactions: Vec<SignedTransaction>,
        beneficiary: Address,
        state: &mut WorldState,
    ) -> Result<Block> {
        let block_number = parent.number() + 1;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Calculate base fee (EIP-1559 style)
        let base_fee = self.calculate_base_fee(parent);

        // Execute transactions
        let mut executed_txs = Vec::new();
        let mut gas_used = 0u64;
        let mut receipts = Vec::new();

        for tx in transactions {
            // Check gas limit
            if gas_used + tx.gas_limit > self.config.block_gas_limit {
                continue;
            }

            match self.executor.execute_tx(
                &tx,
                state,
                block_number,
                timestamp,
                base_fee,
                beneficiary,
            ) {
                Ok(receipt) => {
                    gas_used += receipt.gas_used;
                    receipts.push(receipt);
                    executed_txs.push(tx);
                }
                Err(e) => {
                    tracing::warn!("Transaction failed: {}", e);
                    continue;
                }
            }
        }

        // Commit state
        state.commit()?;

        // Create header
        let header = BlockHeader {
            parent_hash: parent.hash(),
            beneficiary,
            state_root: state.state_root(),
            transactions_root: self.compute_tx_root(&executed_txs),
            receipts_root: self.compute_receipts_root(&receipts),
            logs_bloom: LogsBloom::default(),
            number: block_number,
            gas_limit: self.config.block_gas_limit,
            gas_used,
            timestamp,
            extra_data: vec![],
            base_fee,
        };

        Ok(Block {
            header,
            transactions: executed_txs,
        })
    }

    /// Validate a block
    pub fn validate_block(
        &self,
        block: &Block,
        parent: &Block,
        state: &WorldState,
    ) -> Result<()> {
        // Check parent hash
        if block.header.parent_hash != parent.hash() {
            return Err(MiniEthError::Block("Invalid parent hash".into()));
        }

        // Check block number
        if block.number() != parent.number() + 1 {
            return Err(MiniEthError::Block("Invalid block number".into()));
        }

        // Check timestamp
        if block.header.timestamp <= parent.header.timestamp {
            return Err(MiniEthError::Block("Invalid timestamp".into()));
        }

        // Check gas limit
        if block.header.gas_used > block.header.gas_limit {
            return Err(MiniEthError::Block("Gas used exceeds limit".into()));
        }

        // Check beneficiary is a validator
        if !self.is_validator(&block.header.beneficiary) {
            return Err(MiniEthError::Block("Invalid beneficiary".into()));
        }

        // Verify transactions root
        let computed_tx_root = self.compute_tx_root(&block.transactions);
        if computed_tx_root != block.header.transactions_root {
            return Err(MiniEthError::Block("Invalid transactions root".into()));
        }

        Ok(())
    }

    /// Calculate base fee for next block (EIP-1559)
    fn calculate_base_fee(&self, parent: &Block) -> u64 {
        let parent_gas_target = parent.header.gas_limit / 2;
        let parent_base_fee = parent.header.base_fee;
        let parent_gas_used = parent.header.gas_used;

        if parent_gas_used == parent_gas_target {
            return parent_base_fee;
        }

        if parent_gas_used > parent_gas_target {
            // Increase base fee
            let gas_delta = parent_gas_used - parent_gas_target;
            let fee_delta = (parent_base_fee * gas_delta / parent_gas_target / 8).max(1);
            parent_base_fee + fee_delta
        } else {
            // Decrease base fee
            let gas_delta = parent_gas_target - parent_gas_used;
            let fee_delta = parent_base_fee * gas_delta / parent_gas_target / 8;
            parent_base_fee.saturating_sub(fee_delta)
        }
    }

    /// Compute transactions root
    fn compute_tx_root(&self, txs: &[SignedTransaction]) -> H256 {
        use eth_primitives::keccak256;

        if txs.is_empty() {
            return H256::zero();
        }

        let mut data = Vec::new();
        for tx in txs {
            data.extend_from_slice(tx.hash.as_bytes());
        }
        keccak256(&data)
    }

    /// Compute receipts root
    fn compute_receipts_root(&self, receipts: &[crate::types::Receipt]) -> H256 {
        use eth_primitives::keccak256;

        if receipts.is_empty() {
            return H256::zero();
        }

        let mut data = Vec::new();
        for receipt in receipts {
            data.extend_from_slice(receipt.tx_hash.as_bytes());
            data.push(if receipt.status { 1 } else { 0 });
        }
        keccak256(&data)
    }

    /// Get configuration
    pub fn config(&self) -> &ConsensusConfig {
        &self.config
    }

    /// Get executor
    pub fn executor(&self) -> &Executor {
        &self.executor
    }

    /// Get block time
    pub fn block_time(&self) -> u64 {
        self.config.block_time
    }

    /// Get validators
    pub fn validators(&self) -> Vec<Address> {
        self.validators.read().iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_management() {
        let config = ConsensusConfig::default();
        let consensus = Consensus::new(config);

        let validator = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();

        assert!(!consensus.is_validator(&validator));

        consensus.add_validator(validator);
        assert!(consensus.is_validator(&validator));

        consensus.remove_validator(&validator);
        assert!(!consensus.is_validator(&validator));
    }

    #[test]
    fn test_proposer_selection() {
        let mut config = ConsensusConfig::default();
        config.validators = vec![
            Address::from_hex("0x1111111111111111111111111111111111111111").unwrap(),
            Address::from_hex("0x2222222222222222222222222222222222222222").unwrap(),
            Address::from_hex("0x3333333333333333333333333333333333333333").unwrap(),
        ];

        let consensus = Consensus::new(config.clone());

        // Round robin should cycle through validators
        let p0 = consensus.get_proposer(0).unwrap();
        let p1 = consensus.get_proposer(1).unwrap();
        let p2 = consensus.get_proposer(2).unwrap();
        let p3 = consensus.get_proposer(3).unwrap();

        assert_eq!(p0, p3); // Should wrap around
        assert_ne!(p0, p1);
        assert_ne!(p1, p2);
    }
}
