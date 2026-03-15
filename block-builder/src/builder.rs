//! # Block Builder
//!
//! Construct blocks from pending transactions

use eth_primitives::{H256, keccak256};
use crate::transaction::{PendingTransaction, TransactionPriority};
use crate::mempool::Mempool;
use crate::mev::MevBundle;
use crate::error::{BuilderError, Result};

/// Builder configuration
#[derive(Debug, Clone)]
pub struct BuilderConfig {
    /// Block gas limit
    pub gas_limit: u64,
    /// Base fee per gas
    pub base_fee: u64,
    /// Builder fee (tip share, 0-100)
    pub builder_fee_percent: u8,
    /// Minimum priority fee to include
    pub min_priority_fee: u64,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        BuilderConfig {
            gas_limit: 30_000_000, // 30M gas
            base_fee: 50_000_000_000, // 50 gwei
            builder_fee_percent: 10,
            min_priority_fee: 1_000_000_000, // 1 gwei
        }
    }
}

/// A built block
#[derive(Debug, Clone)]
pub struct Block {
    /// Block number
    pub number: u64,
    /// Parent hash
    pub parent_hash: H256,
    /// State root
    pub state_root: H256,
    /// Transactions root
    pub transactions_root: H256,
    /// Receipts root
    pub receipts_root: H256,
    /// Gas used
    pub gas_used: u64,
    /// Gas limit
    pub gas_limit: u64,
    /// Base fee
    pub base_fee: u64,
    /// Timestamp
    pub timestamp: u64,
    /// Transactions
    pub transactions: Vec<PendingTransaction>,
    /// Builder profit (in wei)
    pub builder_profit: u64,
}

impl Block {
    /// Compute block hash
    pub fn hash(&self) -> H256 {
        let mut data = Vec::new();
        data.extend_from_slice(&self.number.to_be_bytes());
        data.extend_from_slice(self.parent_hash.as_bytes());
        data.extend_from_slice(self.state_root.as_bytes());
        data.extend_from_slice(&self.gas_used.to_be_bytes());
        data.extend_from_slice(&self.timestamp.to_be_bytes());
        keccak256(&data)
    }
}

/// Block builder
pub struct BlockBuilder {
    /// Configuration
    config: BuilderConfig,
    /// Mempool
    mempool: Mempool,
    /// MEV bundles
    bundles: Vec<MevBundle>,
    /// Last block number
    last_block: u64,
    /// Last block hash
    last_hash: H256,
}

impl BlockBuilder {
    /// Create new block builder
    pub fn new(config: BuilderConfig) -> Self {
        BlockBuilder {
            config,
            mempool: Mempool::default(),
            bundles: Vec::new(),
            last_block: 0,
            last_hash: H256::default(),
        }
    }

    /// Get mempool reference
    pub fn mempool(&self) -> &Mempool {
        &self.mempool
    }

    /// Get mutable mempool reference
    pub fn mempool_mut(&mut self) -> &mut Mempool {
        &mut self.mempool
    }

    /// Submit MEV bundle
    pub fn submit_bundle(&mut self, bundle: MevBundle) -> Result<()> {
        // Validate bundle
        if bundle.transactions.is_empty() {
            return Err(BuilderError::InvalidBundle("Empty bundle".to_string()));
        }

        // Check target block
        if bundle.target_block != 0 && bundle.target_block != self.last_block + 1 {
            return Err(BuilderError::InvalidBundle("Wrong target block".to_string()));
        }

        self.bundles.push(bundle);
        Ok(())
    }

    /// Build a block
    pub fn build_block(&mut self) -> Result<Block> {
        let block_number = self.last_block + 1;
        let mut transactions = Vec::new();
        let mut gas_used = 0u64;
        let mut builder_profit = 0u64;

        // First, include MEV bundles (sorted by profit)
        let mut bundles = std::mem::take(&mut self.bundles);
        bundles.sort_by(|a, b| b.total_tip().cmp(&a.total_tip()));

        for bundle in bundles {
            // Check if bundle fits
            let bundle_gas: u64 = bundle.transactions.iter()
                .map(|tx| tx.gas_limit)
                .sum();

            if gas_used + bundle_gas > self.config.gas_limit {
                continue;
            }

            // Add bundle transactions
            for tx in bundle.transactions {
                let tip = tx.tip(self.config.base_fee);
                builder_profit += tip * tx.gas_limit;
                gas_used += tx.gas_limit;
                transactions.push(tx);
            }
        }

        // Then fill with regular transactions from mempool
        let mut pending = self.mempool.get_ordered(
            self.config.gas_limit - gas_used,
            self.config.base_fee,
        );

        // Filter by minimum priority fee
        pending.retain(|tx| tx.max_priority_fee >= self.config.min_priority_fee);

        for tx in pending {
            if gas_used + tx.gas_limit > self.config.gas_limit {
                continue;
            }

            let tip = tx.tip(self.config.base_fee);
            builder_profit += tip * tx.gas_limit;
            gas_used += tx.gas_limit;

            // Remove from mempool
            self.mempool.remove(&tx.hash);
            transactions.push(tx);
        }

        // Calculate roots
        let transactions_root = self.compute_tx_root(&transactions);

        let block = Block {
            number: block_number,
            parent_hash: self.last_hash,
            state_root: H256::default(), // Would be computed by execution
            transactions_root,
            receipts_root: H256::default(),
            gas_used,
            gas_limit: self.config.gas_limit,
            base_fee: self.config.base_fee,
            timestamp: current_timestamp(),
            transactions,
            builder_profit,
        };

        // Update state
        self.last_block = block_number;
        self.last_hash = block.hash();

        Ok(block)
    }

    /// Compute transactions root
    fn compute_tx_root(&self, transactions: &[PendingTransaction]) -> H256 {
        if transactions.is_empty() {
            return H256::default();
        }

        let mut data = Vec::new();
        for tx in transactions {
            data.extend_from_slice(tx.hash.as_bytes());
        }
        keccak256(&data)
    }

    /// Update base fee (based on previous block usage)
    pub fn update_base_fee(&mut self, gas_used: u64) {
        let target = self.config.gas_limit / 2;

        if gas_used > target {
            // Increase base fee
            let increase = self.config.base_fee * (gas_used - target) / target / 8;
            self.config.base_fee = self.config.base_fee.saturating_add(increase);
        } else if gas_used < target {
            // Decrease base fee
            let decrease = self.config.base_fee * (target - gas_used) / target / 8;
            self.config.base_fee = self.config.base_fee.saturating_sub(decrease);
        }

        // Minimum base fee
        if self.config.base_fee == 0 {
            self.config.base_fee = 1;
        }
    }

    /// Get current base fee
    pub fn base_fee(&self) -> u64 {
        self.config.base_fee
    }

    /// Get config
    pub fn config(&self) -> &BuilderConfig {
        &self.config
    }

    /// Simulate block building to estimate profit
    pub fn simulate_build(&self) -> (usize, u64, u64) {
        let mut tx_count = 0;
        let mut gas_used = 0u64;
        let mut profit = 0u64;

        // Bundles
        for bundle in &self.bundles {
            let bundle_gas: u64 = bundle.transactions.iter()
                .map(|tx| tx.gas_limit)
                .sum();

            if gas_used + bundle_gas <= self.config.gas_limit {
                tx_count += bundle.transactions.len();
                gas_used += bundle_gas;
                profit += bundle.total_tip();
            }
        }

        // Regular transactions
        let pending = self.mempool.get_ordered(
            self.config.gas_limit - gas_used,
            self.config.base_fee,
        );

        for tx in pending {
            if gas_used + tx.gas_limit > self.config.gas_limit {
                continue;
            }
            if tx.max_priority_fee < self.config.min_priority_fee {
                continue;
            }

            tx_count += 1;
            gas_used += tx.gas_limit;
            profit += tx.tip(self.config.base_fee) * tx.gas_limit;
        }

        (tx_count, gas_used, profit)
    }
}

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

    fn test_addresses() -> (Address, Address) {
        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();
        (alice, bob)
    }

    #[test]
    fn test_build_empty_block() {
        let mut builder = BlockBuilder::new(BuilderConfig::default());
        let block = builder.build_block().unwrap();

        assert_eq!(block.number, 1);
        assert_eq!(block.transactions.len(), 0);
        assert_eq!(block.gas_used, 0);
    }

    #[test]
    fn test_build_block_with_transactions() {
        let (alice, bob) = test_addresses();

        let config = BuilderConfig {
            gas_limit: 1_000_000,
            base_fee: 10,
            min_priority_fee: 1,
            ..Default::default()
        };

        let mut builder = BlockBuilder::new(config);

        // Add transactions
        for i in 0..5 {
            let tx = PendingTransaction::transfer(alice, bob, 1000, i, 100, 10);
            builder.mempool_mut().add(tx).unwrap();
        }

        let block = builder.build_block().unwrap();

        assert_eq!(block.number, 1);
        assert_eq!(block.transactions.len(), 5);
        assert_eq!(block.gas_used, 5 * 21000);
        assert!(block.builder_profit > 0);
    }

    #[test]
    fn test_gas_limit_respected() {
        let (alice, bob) = test_addresses();

        let config = BuilderConfig {
            gas_limit: 42_000, // Only 2 transfers fit
            base_fee: 10,
            min_priority_fee: 1,
            ..Default::default()
        };

        let mut builder = BlockBuilder::new(config);

        // Add 5 transactions
        for i in 0..5 {
            let tx = PendingTransaction::transfer(alice, bob, 1000, i, 100, 10);
            builder.mempool_mut().add(tx).unwrap();
        }

        let block = builder.build_block().unwrap();

        // Only 2 should fit
        assert_eq!(block.transactions.len(), 2);
        assert_eq!(block.gas_used, 42000);
    }

    #[test]
    fn test_bundles_prioritized() {
        let (alice, bob) = test_addresses();

        let config = BuilderConfig {
            gas_limit: 100_000,
            base_fee: 10,
            min_priority_fee: 1,
            ..Default::default()
        };

        let mut builder = BlockBuilder::new(config);

        // Add low priority regular transaction
        let regular_tx = PendingTransaction::transfer(alice, bob, 1000, 0, 100, 5);
        builder.mempool_mut().add(regular_tx).unwrap();

        // Add MEV bundle with high tip
        let bundle_tx = PendingTransaction::transfer(alice, bob, 1000, 1, 100, 50)
            .with_priority(TransactionPriority::High);
        let bundle = MevBundle::new(vec![bundle_tx], 1);
        builder.submit_bundle(bundle).unwrap();

        let block = builder.build_block().unwrap();

        // Bundle transaction should be first
        assert_eq!(block.transactions.len(), 2);
        assert_eq!(block.transactions[0].max_priority_fee, 50);
    }

    #[test]
    fn test_base_fee_adjustment() {
        let mut builder = BlockBuilder::new(BuilderConfig::default());
        let initial_base_fee = builder.base_fee();

        // Simulate full block
        builder.update_base_fee(30_000_000); // Full gas
        assert!(builder.base_fee() > initial_base_fee);

        // Simulate empty block
        let high_fee = builder.base_fee();
        builder.update_base_fee(0);
        assert!(builder.base_fee() < high_fee);
    }
}
