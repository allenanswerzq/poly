//! # Mempool
//!
//! Transaction pool management

use eth_primitives::{Address, H256};
use crate::transaction::{PendingTransaction, TransactionPriority};
use crate::error::{BuilderError, Result};
use std::collections::{HashMap, BinaryHeap};

/// Configuration for the mempool
#[derive(Debug, Clone)]
pub struct MempoolConfig {
    /// Maximum number of transactions
    pub max_size: usize,
    /// Maximum transactions per sender
    pub max_per_sender: usize,
    /// Transaction lifetime in seconds
    pub tx_lifetime: u64,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        MempoolConfig {
            max_size: 10_000,
            max_per_sender: 100,
            tx_lifetime: 3600, // 1 hour
        }
    }
}

/// The mempool holds pending transactions
pub struct Mempool {
    /// Configuration
    config: MempoolConfig,
    /// All transactions by hash
    by_hash: HashMap<H256, PendingTransaction>,
    /// Transactions per sender (for nonce tracking)
    by_sender: HashMap<Address, Vec<H256>>,
    /// Priority queue for ordering
    priority_queue: BinaryHeap<TxRef>,
}

/// Reference to transaction for priority queue
#[derive(Debug, Clone)]
struct TxRef {
    hash: H256,
    priority_fee: u64,
    priority: TransactionPriority,
}

impl Ord for TxRef {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (&self.priority, &other.priority) {
            (TransactionPriority::High, TransactionPriority::High) => {}
            (TransactionPriority::High, _) => return std::cmp::Ordering::Greater,
            (_, TransactionPriority::High) => return std::cmp::Ordering::Less,
            _ => {}
        }
        self.priority_fee.cmp(&other.priority_fee)
    }
}

impl PartialOrd for TxRef {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for TxRef {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Eq for TxRef {}

impl Mempool {
    /// Create new mempool
    pub fn new(config: MempoolConfig) -> Self {
        Mempool {
            config,
            by_hash: HashMap::new(),
            by_sender: HashMap::new(),
            priority_queue: BinaryHeap::new(),
        }
    }

    /// Create with default config
    pub fn default() -> Self {
        Self::new(MempoolConfig::default())
    }

    /// Add transaction to mempool
    pub fn add(&mut self, tx: PendingTransaction) -> Result<()> {
        // Check if already exists
        if self.by_hash.contains_key(&tx.hash) {
            return Err(BuilderError::DuplicateTransaction(tx.hash.to_string()));
        }

        // Check mempool size
        if self.by_hash.len() >= self.config.max_size {
            // Try to evict lowest priority transaction
            self.evict_lowest()?;
        }

        // Check per-sender limit
        let sender_txs = self.by_sender.entry(tx.from).or_insert_with(Vec::new);
        if sender_txs.len() >= self.config.max_per_sender {
            return Err(BuilderError::MempoolFull);
        }

        // Add to priority queue
        self.priority_queue.push(TxRef {
            hash: tx.hash,
            priority_fee: tx.max_priority_fee,
            priority: tx.priority,
        });

        // Add to sender map
        sender_txs.push(tx.hash);

        // Add to main map
        self.by_hash.insert(tx.hash, tx);

        Ok(())
    }

    /// Remove transaction from mempool
    pub fn remove(&mut self, hash: &H256) -> Option<PendingTransaction> {
        if let Some(tx) = self.by_hash.remove(hash) {
            // Remove from sender map
            if let Some(sender_txs) = self.by_sender.get_mut(&tx.from) {
                sender_txs.retain(|h| h != hash);
            }
            Some(tx)
        } else {
            None
        }
    }

    /// Get transaction by hash
    pub fn get(&self, hash: &H256) -> Option<&PendingTransaction> {
        self.by_hash.get(hash)
    }

    /// Get all transactions for a sender
    pub fn get_by_sender(&self, sender: &Address) -> Vec<&PendingTransaction> {
        self.by_sender.get(sender)
            .map(|hashes| {
                hashes.iter()
                    .filter_map(|h| self.by_hash.get(h))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get ordered transactions up to gas limit
    pub fn get_ordered(&self, gas_limit: u64, base_fee: u64) -> Vec<PendingTransaction> {
        let mut result = Vec::new();
        let mut total_gas = 0u64;

        // Clone and sort all transactions
        let mut txs: Vec<_> = self.by_hash.values().cloned().collect();
        txs.sort_by(|a, b| b.cmp(a)); // Descending order

        for tx in txs {
            // Skip if gas would exceed limit
            if total_gas + tx.gas_limit > gas_limit {
                continue;
            }

            // Skip if max fee is below base fee
            if tx.max_fee_per_gas < base_fee {
                continue;
            }

            total_gas += tx.gas_limit;
            result.push(tx);
        }

        result
    }

    /// Get pending transaction count
    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    /// Is mempool empty?
    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }

    /// Get total pending gas
    pub fn pending_gas(&self) -> u64 {
        self.by_hash.values().map(|tx| tx.gas_limit).sum()
    }

    /// Evict lowest priority transaction
    fn evict_lowest(&mut self) -> Result<()> {
        // Find lowest priority transaction
        let mut lowest: Option<H256> = None;
        let mut lowest_fee = u64::MAX;

        for (hash, tx) in &self.by_hash {
            if matches!(tx.priority, TransactionPriority::Low) && tx.max_priority_fee < lowest_fee {
                lowest = Some(*hash);
                lowest_fee = tx.max_priority_fee;
            }
        }

        if let Some(hash) = lowest {
            self.remove(&hash);
            Ok(())
        } else {
            Err(BuilderError::MempoolFull)
        }
    }

    /// Clear all transactions
    pub fn clear(&mut self) {
        self.by_hash.clear();
        self.by_sender.clear();
        self.priority_queue.clear();
    }

    /// Statistics
    pub fn stats(&self) -> MempoolStats {
        MempoolStats {
            total_txs: self.by_hash.len(),
            total_gas: self.pending_gas(),
            unique_senders: self.by_sender.len(),
        }
    }
}

/// Mempool statistics
#[derive(Debug, Clone)]
pub struct MempoolStats {
    pub total_txs: usize,
    pub total_gas: u64,
    pub unique_senders: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_addresses() -> (Address, Address, Address) {
        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();
        let charlie = Address::from_hex("0x3333333333333333333333333333333333333333").unwrap();
        (alice, bob, charlie)
    }

    #[test]
    fn test_add_transaction() {
        let mut mempool = Mempool::default();
        let (alice, bob, _) = test_addresses();

        let tx = PendingTransaction::transfer(alice, bob, 1000, 0, 100, 10);
        mempool.add(tx.clone()).unwrap();

        assert_eq!(mempool.len(), 1);
        assert!(mempool.get(&tx.hash).is_some());
    }

    #[test]
    fn test_remove_transaction() {
        let mut mempool = Mempool::default();
        let (alice, bob, _) = test_addresses();

        let tx = PendingTransaction::transfer(alice, bob, 1000, 0, 100, 10);
        let hash = tx.hash;
        mempool.add(tx).unwrap();

        let removed = mempool.remove(&hash);
        assert!(removed.is_some());
        assert_eq!(mempool.len(), 0);
    }

    #[test]
    fn test_duplicate_rejection() {
        let mut mempool = Mempool::default();
        let (alice, bob, _) = test_addresses();

        let tx = PendingTransaction::transfer(alice, bob, 1000, 0, 100, 10);
        mempool.add(tx.clone()).unwrap();

        let result = mempool.add(tx);
        assert!(matches!(result, Err(BuilderError::DuplicateTransaction(_))));
    }

    #[test]
    fn test_ordered_by_priority() {
        let mut mempool = Mempool::default();
        let (alice, bob, _) = test_addresses();

        let tx1 = PendingTransaction::transfer(alice, bob, 1000, 0, 100, 10);
        let tx2 = PendingTransaction::transfer(alice, bob, 1000, 1, 100, 20);
        let tx3 = PendingTransaction::transfer(alice, bob, 1000, 2, 100, 5);

        mempool.add(tx1.clone()).unwrap();
        mempool.add(tx2.clone()).unwrap();
        mempool.add(tx3.clone()).unwrap();

        let ordered = mempool.get_ordered(1_000_000, 10);

        // Should be ordered by priority fee (highest first)
        assert_eq!(ordered[0].hash, tx2.hash);
        assert_eq!(ordered[1].hash, tx1.hash);
        assert_eq!(ordered[2].hash, tx3.hash);
    }

    #[test]
    fn test_gas_limit_constraint() {
        let mut mempool = Mempool::default();
        let (alice, bob, _) = test_addresses();

        // Each transfer is 21000 gas
        for i in 0..10 {
            let tx = PendingTransaction::transfer(alice, bob, 1000, i, 100, 10);
            mempool.add(tx).unwrap();
        }

        // Limit to 3 transactions (63000 gas)
        let ordered = mempool.get_ordered(63000, 10);
        assert_eq!(ordered.len(), 3);
    }
}
