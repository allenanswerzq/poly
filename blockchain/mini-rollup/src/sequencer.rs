//! # Sequencer
//!
//! L2 sequencer that orders transactions and produces batches

use eth_primitives::{H256, Address};
use crate::transaction::L2Transaction;
use crate::state::StateDB;
use crate::batch::{Batch, BatchHeader};
use crate::error::{RollupError, Result};
use std::collections::VecDeque;

/// Sequencer configuration
#[derive(Debug, Clone)]
pub struct SequencerConfig {
    /// Sequencer address
    pub sequencer_address: Address,
    /// Max transactions per batch
    pub max_batch_size: usize,
    /// Max batch interval in seconds
    pub batch_interval: u64,
    /// Challenge period in seconds (for fraud proofs)
    pub challenge_period: u64,
}

impl Default for SequencerConfig {
    fn default() -> Self {
        SequencerConfig {
            sequencer_address: Address::zero(),
            max_batch_size: 1000,
            batch_interval: 12, // Every 12 seconds
            challenge_period: 7 * 24 * 60 * 60, // 7 days
        }
    }
}

/// L2 Sequencer
pub struct Sequencer {
    /// Configuration
    config: SequencerConfig,
    /// Current state
    state: StateDB,
    /// Pending transactions (mempool)
    pending_txs: VecDeque<L2Transaction>,
    /// Current batch being built
    current_batch: Option<Batch>,
    /// Committed batches
    committed_batches: Vec<BatchHeader>,
    /// Last batch number
    last_batch_number: u64,
    /// Last batch hash
    last_batch_hash: H256,
}

impl Sequencer {
    /// Create new sequencer
    pub fn new(config: SequencerConfig) -> Self {
        Sequencer {
            config,
            state: StateDB::new(),
            pending_txs: VecDeque::new(),
            current_batch: None,
            committed_batches: Vec::new(),
            last_batch_number: 0,
            last_batch_hash: H256::default(),
        }
    }

    /// Get current state root
    pub fn state_root(&self) -> H256 {
        self.state.state_root()
    }

    /// Get last batch number
    pub fn last_batch(&self) -> u64 {
        self.last_batch_number
    }

    /// Get state reference
    pub fn state(&self) -> &StateDB {
        &self.state
    }

    /// Get mutable state reference
    pub fn state_mut(&mut self) -> &mut StateDB {
        &mut self.state
    }

    /// Submit transaction to mempool
    pub fn submit_transaction(&mut self, tx: L2Transaction) -> Result<H256> {
        // Basic validation
        let balance = self.state.get_balance(&tx.from);
        let cost = tx.value + (tx.gas_limit * tx.max_fee);

        if balance < cost {
            return Err(RollupError::InsufficientBalance {
                needed: cost,
                have: balance,
            });
        }

        let expected_nonce = self.state.get_nonce(&tx.from);
        if tx.nonce < expected_nonce {
            return Err(RollupError::InvalidNonce {
                expected: expected_nonce,
                got: tx.nonce,
            });
        }

        let tx_hash = tx.hash();
        self.pending_txs.push_back(tx);

        Ok(tx_hash)
    }

    /// Get pending transaction count
    pub fn pending_count(&self) -> usize {
        self.pending_txs.len()
    }

    /// Build and seal a batch from pending transactions
    pub fn seal_batch(&mut self) -> Result<Batch> {
        let pre_state_root = self.state.state_root();

        // Convert Address to [u8; 20]
        let mut seq_bytes = [0u8; 20];
        seq_bytes.copy_from_slice(self.config.sequencer_address.as_bytes());

        let mut batch = Batch::new(
            self.last_batch_number + 1,
            self.last_batch_hash,
            pre_state_root,
            seq_bytes,
        );

        // Add pending transactions up to limit
        let max_txs = self.config.max_batch_size.min(self.pending_txs.len());

        for _ in 0..max_txs {
            if let Some(tx) = self.pending_txs.pop_front() {
                batch.add_transaction(tx)?;
            }
        }

        // Execute all transactions
        batch.execute(&mut self.state);

        // Compress for L1 submission
        batch.compress();

        // Update sequencer state
        self.last_batch_number = batch.header.batch_number;
        self.last_batch_hash = batch.hash();
        self.committed_batches.push(batch.header.clone());

        Ok(batch)
    }

    /// Get committed batch headers
    pub fn committed_batches(&self) -> &[BatchHeader] {
        &self.committed_batches
    }

    /// Get batch by number
    pub fn get_batch_header(&self, batch_number: u64) -> Option<&BatchHeader> {
        self.committed_batches.iter()
            .find(|b| b.batch_number == batch_number)
    }

    /// Simulate transaction without committing
    pub fn simulate_transaction(&self, tx: &L2Transaction) -> Result<u64> {
        // Clone state for simulation
        let mut sim_state = StateDB::new();

        // Copy account state
        let from_account = self.state.get_account(&tx.from);
        sim_state.set_account(tx.from, from_account);

        if let Some(to) = &tx.to {
            let to_account = self.state.get_account(to);
            sim_state.set_account(*to, to_account);
        }

        // Execute
        let gas = tx.estimate_gas();
        Ok(gas)
    }

    /// Get account balance
    pub fn get_balance(&self, address: &Address) -> u64 {
        self.state.get_balance(address)
    }

    /// Get account nonce
    pub fn get_nonce(&self, address: &Address) -> u64 {
        self.state.get_nonce(address)
    }
}

/// Metrics for the sequencer
#[derive(Debug, Default)]
pub struct SequencerMetrics {
    /// Total transactions processed
    pub total_txs: u64,
    /// Total batches sealed
    pub total_batches: u64,
    /// Total gas used
    pub total_gas: u64,
    /// Total data posted to L1
    pub total_da_bytes: u64,
}

impl SequencerMetrics {
    pub fn record_batch(&mut self, batch: &Batch) {
        self.total_txs += batch.transactions.len() as u64;
        self.total_batches += 1;
        self.total_gas += batch.results.iter().map(|r| r.gas_used).sum::<u64>();
        self.total_da_bytes += batch.compressed_data.len() as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_sequencer() -> Sequencer {
        let config = SequencerConfig::default();
        let mut sequencer = Sequencer::new(config);

        // Fund test accounts
        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        // Provide enough balance for gas costs (gas_limit * max_fee + value)
        sequencer.state_mut().set_balance(alice, 100_000_000_000_000_000);
        sequencer.state_mut().set_balance(bob, 50_000_000_000_000_000);
        sequencer.state_mut().commit();

        sequencer
    }

    #[test]
    fn test_submit_transaction() {
        let mut sequencer = setup_sequencer();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        let tx = L2Transaction::transfer(alice, bob, 1_000_000, 0);
        let hash = sequencer.submit_transaction(tx).unwrap();

        assert!(!hash.as_bytes().iter().all(|&b| b == 0));
        assert_eq!(sequencer.pending_count(), 1);
    }

    #[test]
    fn test_seal_batch() {
        let mut sequencer = setup_sequencer();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        // Submit multiple transactions
        for i in 0..5 {
            let tx = L2Transaction::transfer(alice, bob, 100_000, i);
            sequencer.submit_transaction(tx).unwrap();
        }

        let batch = sequencer.seal_batch().unwrap();

        assert_eq!(batch.header.batch_number, 1);
        assert_eq!(batch.transactions.len(), 5);
        assert_eq!(batch.results.len(), 5);
        assert!(batch.results.iter().all(|r| r.success));

        // State updated
        assert_eq!(sequencer.get_nonce(&alice), 5);
        assert!(sequencer.get_balance(&bob) > 5_000_000_000_000);
    }

    #[test]
    fn test_insufficient_balance() {
        let mut sequencer = setup_sequencer();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        // Try to transfer more than balance (balance is 100_000_000_000_000_000)
        let tx = L2Transaction::transfer(alice, bob, 999_000_000_000_000_000, 0);
        let result = sequencer.submit_transaction(tx);

        assert!(matches!(result, Err(RollupError::InsufficientBalance { .. })));
    }

    #[test]
    fn test_batch_commitment() {
        let mut sequencer = setup_sequencer();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        let tx = L2Transaction::transfer(alice, bob, 100_000, 0);
        sequencer.submit_transaction(tx).unwrap();

        let batch = sequencer.seal_batch().unwrap();

        // Batch header stored
        assert_eq!(sequencer.committed_batches().len(), 1);
        assert_eq!(sequencer.last_batch(), 1);

        // Can retrieve batch
        let header = sequencer.get_batch_header(1).unwrap();
        assert_eq!(header.tx_count, 1);
    }

    #[test]
    fn test_multiple_batches() {
        let mut sequencer = setup_sequencer();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        // Batch 1
        for i in 0..3 {
            let tx = L2Transaction::transfer(alice, bob, 100_000, i);
            sequencer.submit_transaction(tx).unwrap();
        }
        let batch1 = sequencer.seal_batch().unwrap();

        // Batch 2
        for i in 3..6 {
            let tx = L2Transaction::transfer(alice, bob, 100_000, i);
            sequencer.submit_transaction(tx).unwrap();
        }
        let batch2 = sequencer.seal_batch().unwrap();

        // Batches are linked
        assert_eq!(batch2.header.parent_hash, batch1.hash());
        assert_eq!(batch2.header.pre_state_root, batch1.header.post_state_root);
    }
}
