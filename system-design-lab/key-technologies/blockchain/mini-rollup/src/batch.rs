//! # Transaction Batch
//!
//! Batches of L2 transactions for submission to L1

use eth_primitives::{H256, keccak256};
use crate::transaction::{L2Transaction, TransactionResult};
use crate::state::StateDB;
use crate::error::{RollupError, Result};

/// Maximum transactions per batch
pub const MAX_BATCH_SIZE: usize = 1000;

/// Batch header (committed to L1)
#[derive(Debug, Clone)]
pub struct BatchHeader {
    /// Batch number
    pub batch_number: u64,
    /// Parent batch hash
    pub parent_hash: H256,
    /// State root before batch
    pub pre_state_root: H256,
    /// State root after batch
    pub post_state_root: H256,
    /// Transaction root (merkle root of txs)
    pub tx_root: H256,
    /// Number of transactions
    pub tx_count: u64,
    /// Timestamp
    pub timestamp: u64,
    /// Sequencer address
    pub sequencer: [u8; 20],
}

impl BatchHeader {
    /// Compute batch hash
    pub fn hash(&self) -> H256 {
        let mut data = Vec::new();
        data.extend_from_slice(&self.batch_number.to_be_bytes());
        data.extend_from_slice(self.parent_hash.as_bytes());
        data.extend_from_slice(self.pre_state_root.as_bytes());
        data.extend_from_slice(self.post_state_root.as_bytes());
        data.extend_from_slice(self.tx_root.as_bytes());
        data.extend_from_slice(&self.tx_count.to_be_bytes());
        data.extend_from_slice(&self.timestamp.to_be_bytes());
        data.extend_from_slice(&self.sequencer);

        keccak256(&data)
    }
}

/// Full batch with transactions
#[derive(Debug, Clone)]
pub struct Batch {
    /// Batch header
    pub header: BatchHeader,
    /// Transactions in this batch
    pub transactions: Vec<L2Transaction>,
    /// Execution results
    pub results: Vec<TransactionResult>,
    /// Compressed batch data (for L1 submission)
    pub compressed_data: Vec<u8>,
}

impl Batch {
    /// Create new batch
    pub fn new(
        batch_number: u64,
        parent_hash: H256,
        pre_state_root: H256,
        sequencer: [u8; 20],
    ) -> Self {
        Batch {
            header: BatchHeader {
                batch_number,
                parent_hash,
                pre_state_root,
                post_state_root: pre_state_root,
                tx_root: H256::default(),
                tx_count: 0,
                timestamp: current_timestamp(),
                sequencer,
            },
            transactions: Vec::new(),
            results: Vec::new(),
            compressed_data: Vec::new(),
        }
    }

    /// Add transaction to batch
    pub fn add_transaction(&mut self, tx: L2Transaction) -> Result<()> {
        if self.transactions.len() >= MAX_BATCH_SIZE {
            return Err(RollupError::BatchTooLarge {
                size: self.transactions.len() + 1,
                max: MAX_BATCH_SIZE,
            });
        }

        self.transactions.push(tx);
        self.header.tx_count = self.transactions.len() as u64;

        Ok(())
    }

    /// Execute all transactions against state
    pub fn execute(&mut self, state: &mut StateDB) -> Vec<TransactionResult> {
        self.results.clear();

        for tx in &self.transactions {
            let result = execute_transaction(tx, state);
            self.results.push(result);
        }

        // Commit state and update header
        self.header.post_state_root = state.commit();
        self.header.tx_root = self.compute_tx_root();

        self.results.clone()
    }

    /// Compute merkle root of transactions
    fn compute_tx_root(&self) -> H256 {
        if self.transactions.is_empty() {
            return H256::default();
        }

        // Simplified: just hash all tx hashes together
        let mut data = Vec::new();
        for tx in &self.transactions {
            data.extend_from_slice(tx.hash().as_bytes());
        }

        keccak256(&data)
    }

    /// Compress batch for L1 submission
    pub fn compress(&mut self) -> Vec<u8> {
        let mut data = Vec::new();

        // Header
        data.extend_from_slice(&self.header.batch_number.to_be_bytes());
        data.extend_from_slice(self.header.parent_hash.as_bytes());
        data.extend_from_slice(self.header.pre_state_root.as_bytes());
        data.extend_from_slice(self.header.post_state_root.as_bytes());

        // Transaction count
        data.extend_from_slice(&(self.transactions.len() as u32).to_be_bytes());

        // Compressed transactions
        for tx in &self.transactions {
            let tx_data = tx.compress();
            data.extend_from_slice(&(tx_data.len() as u32).to_be_bytes());
            data.extend_from_slice(&tx_data);
        }

        self.compressed_data = data.clone();
        data
    }

    /// Get batch hash
    pub fn hash(&self) -> H256 {
        self.header.hash()
    }

    /// Get data availability cost estimate (calldata gas)
    pub fn da_cost(&self) -> u64 {
        if self.compressed_data.is_empty() {
            // Estimate based on transactions
            let tx_size: usize = self.transactions.iter()
                .map(|tx| tx.compress().len())
                .sum();
            // 16 gas per non-zero byte, 4 per zero byte (estimate 12 avg)
            return (tx_size as u64) * 12 + 1000; // 1000 for header
        }

        self.compressed_data.iter()
            .map(|&b| if b == 0 { 4u64 } else { 16u64 })
            .sum()
    }
}

/// Execute single transaction
fn execute_transaction(tx: &L2Transaction, state: &mut StateDB) -> TransactionResult {
    let tx_hash = tx.hash();

    // Check nonce
    let expected_nonce = state.get_nonce(&tx.from);
    if tx.nonce != expected_nonce {
        return TransactionResult::failure(
            tx_hash,
            21000,
            format!("Invalid nonce: expected {}, got {}", expected_nonce, tx.nonce),
        );
    }

    // Check balance
    let balance = state.get_balance(&tx.from);
    let total_cost = tx.value + (tx.gas_limit * tx.max_fee);
    if balance < total_cost {
        return TransactionResult::failure(
            tx_hash,
            21000,
            format!("Insufficient balance: need {}, have {}", total_cost, balance),
        );
    }

    // Execute transfer
    if let Some(to) = &tx.to {
        // Deduct value + gas
        state.sub_balance(&tx.from, tx.value);
        state.add_balance(*to, tx.value);
    }

    // Deduct gas cost (simplified - just base gas)
    let gas_used = tx.estimate_gas();
    let gas_cost = gas_used * tx.max_fee;
    state.sub_balance(&tx.from, gas_cost);

    // Increment nonce
    state.increment_nonce(&tx.from);

    TransactionResult::success(tx_hash, gas_used)
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use eth_primitives::Address;

    #[test]
    fn test_batch_creation() {
        let batch = Batch::new(
            1,
            H256::default(),
            H256::default(),
            [0u8; 20],
        );

        assert_eq!(batch.header.batch_number, 1);
        assert_eq!(batch.transactions.len(), 0);
    }

    #[test]
    fn test_batch_add_transaction() {
        let mut batch = Batch::new(1, H256::default(), H256::default(), [0u8; 20]);

        let from = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let to = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        let tx = L2Transaction::transfer(from, to, 1000, 0);
        batch.add_transaction(tx).unwrap();

        assert_eq!(batch.transactions.len(), 1);
        assert_eq!(batch.header.tx_count, 1);
    }

    #[test]
    fn test_batch_execute() {
        let mut state = StateDB::new();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        // Give Alice initial balance (enough for gas + value)
        state.set_balance(alice, 100_000_000_000_000_000);
        state.commit();

        let pre_state_root = state.state_root();

        let mut batch = Batch::new(1, H256::default(), pre_state_root, [0u8; 20]);

        let tx = L2Transaction::transfer(alice, bob, 100_000, 0);
        batch.add_transaction(tx).unwrap();

        let results = batch.execute(&mut state);

        assert_eq!(results.len(), 1);
        assert!(results[0].success);

        // State updated
        assert!(state.get_balance(&bob) > 0);
        assert_eq!(state.get_nonce(&alice), 1);
    }

    #[test]
    fn test_batch_compression() {
        let mut batch = Batch::new(1, H256::default(), H256::default(), [0u8; 20]);

        let from = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let to = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        batch.add_transaction(L2Transaction::transfer(from, to, 1000, 0)).unwrap();
        batch.add_transaction(L2Transaction::transfer(from, to, 2000, 1)).unwrap();

        let compressed = batch.compress();
        assert!(!compressed.is_empty());

        // DA cost should be reasonable
        let cost = batch.da_cost();
        assert!(cost > 0);
    }

    #[test]
    fn test_invalid_nonce() {
        let mut state = StateDB::new();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        state.set_balance(alice, 100_000_000_000_000_000);
        state.commit();

        let mut batch = Batch::new(1, H256::default(), state.state_root(), [0u8; 20]);

        // Wrong nonce (should be 0, using 5)
        let tx = L2Transaction::transfer(alice, bob, 100_000, 5);
        batch.add_transaction(tx).unwrap();

        let results = batch.execute(&mut state);
        assert!(!results[0].success);
        assert!(results[0].error.as_ref().unwrap().contains("nonce"));
    }
}
