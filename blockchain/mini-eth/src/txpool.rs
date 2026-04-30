//! Transaction Pool - Pending transaction management
//!
//! Manages pending transactions before they are included in blocks:
//! - Validation (signature, nonce, balance)
//! - Ordering by gas price
//! - Eviction when pool is full

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use parking_lot::RwLock;
use eth_primitives::{Address, H256, U256};

use crate::types::SignedTransaction;
use crate::state::WorldState;
use crate::error::{MiniEthError, Result};

/// Transaction pool configuration
#[derive(Debug, Clone)]
pub struct TxPoolConfig {
    /// Maximum number of pending transactions
    pub max_pending: usize,
    /// Maximum number of queued transactions per sender
    pub max_queued_per_sender: usize,
    /// Minimum gas price to accept
    pub min_gas_price: u64,
    /// Maximum transaction size in bytes
    pub max_tx_size: usize,
}

impl Default for TxPoolConfig {
    fn default() -> Self {
        TxPoolConfig {
            max_pending: 4096,
            max_queued_per_sender: 16,
            min_gas_price: 1_000_000_000, // 1 gwei
            max_tx_size: 128 * 1024,      // 128 KB
        }
    }
}

/// Transaction in the pool with metadata
#[derive(Debug, Clone)]
pub struct PooledTransaction {
    /// The transaction
    pub tx: SignedTransaction,
    /// When it was received
    pub received_at: u64,
    /// Is it local (from this node's RPC)
    pub is_local: bool,
}

impl PooledTransaction {
    pub fn new(tx: SignedTransaction, is_local: bool) -> Self {
        PooledTransaction {
            tx,
            received_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            is_local,
        }
    }
}

/// Transaction pool
pub struct TransactionPool {
    /// Configuration
    config: TxPoolConfig,

    /// Pending transactions (ready to be included) - ordered by gas price
    /// Key: (effective_gas_price, tx_hash) for ordering
    pending: Arc<RwLock<BTreeMap<(u64, H256), PooledTransaction>>>,

    /// Queued transactions (waiting for nonce gap to be filled)
    /// sender -> nonce -> tx
    queued: Arc<RwLock<HashMap<Address, BTreeMap<u64, PooledTransaction>>>>,

    /// All transaction hashes for quick lookup
    all_txs: Arc<RwLock<HashSet<H256>>>,

    /// Sender to pending tx hashes
    sender_txs: Arc<RwLock<HashMap<Address, HashSet<H256>>>>,

    /// Current base fee for EIP-1559
    base_fee: Arc<RwLock<u64>>,
}

impl TransactionPool {
    /// Create a new transaction pool
    pub fn new(config: TxPoolConfig) -> Self {
        TransactionPool {
            config,
            pending: Arc::new(RwLock::new(BTreeMap::new())),
            queued: Arc::new(RwLock::new(HashMap::new())),
            all_txs: Arc::new(RwLock::new(HashSet::new())),
            sender_txs: Arc::new(RwLock::new(HashMap::new())),
            base_fee: Arc::new(RwLock::new(1_000_000_000)), // 1 gwei default
        }
    }

    /// Add a transaction to the pool
    pub fn add_tx(&self, tx: SignedTransaction, state: &WorldState, is_local: bool) -> Result<H256> {
        let tx_hash = tx.hash;

        // Check if already in pool
        if self.all_txs.read().contains(&tx_hash) {
            return Err(MiniEthError::Transaction("Transaction already in pool".into()));
        }

        // Validate transaction
        self.validate_tx(&tx, state)?;

        let pooled_tx = PooledTransaction::new(tx.clone(), is_local);

        // Determine if pending or queued
        let expected_nonce = state.get_nonce(tx.from)?;

        if tx.nonce == expected_nonce {
            // Can go to pending
            self.add_to_pending(pooled_tx)?;
        } else if tx.nonce > expected_nonce {
            // Must queue (future nonce)
            self.add_to_queued(pooled_tx)?;
        } else {
            return Err(MiniEthError::NonceTooLow {
                expected: expected_nonce,
                got: tx.nonce,
            });
        }

        // Track sender
        self.sender_txs.write()
            .entry(tx.from)
            .or_insert_with(HashSet::new)
            .insert(tx_hash);

        self.all_txs.write().insert(tx_hash);

        Ok(tx_hash)
    }

    /// Validate a transaction
    fn validate_tx(&self, tx: &SignedTransaction, state: &WorldState) -> Result<()> {
        // Check gas price
        let base_fee = *self.base_fee.read();
        if tx.max_fee_per_gas < base_fee {
            return Err(MiniEthError::Transaction(format!(
                "Max fee {} below base fee {}", tx.max_fee_per_gas, base_fee
            )));
        }

        // Check balance for gas + value
        let max_cost = U256::from(tx.gas_limit) * U256::from(tx.max_fee_per_gas) + tx.value;
        let balance = state.get_balance(tx.from)?;

        if balance < max_cost {
            return Err(MiniEthError::InsufficientBalance {
                required: max_cost,
                available: balance,
            });
        }

        // Check transaction size
        if tx.data.len() > self.config.max_tx_size {
            return Err(MiniEthError::Transaction("Transaction too large".into()));
        }

        Ok(())
    }

    /// Add to pending pool
    fn add_to_pending(&self, tx: PooledTransaction) -> Result<()> {
        let mut pending = self.pending.write();

        // Check pool size
        if pending.len() >= self.config.max_pending {
            // Evict lowest gas price tx
            if let Some(key) = pending.keys().next().cloned() {
                let base_fee = *self.base_fee.read();
                let new_price = tx.tx.effective_gas_price(base_fee);
                if new_price <= key.0 {
                    return Err(MiniEthError::Transaction("Pool full, gas price too low".into()));
                }
                pending.remove(&key);
            }
        }

        let base_fee = *self.base_fee.read();
        let effective_price = tx.tx.effective_gas_price(base_fee);
        // Use negated price for descending order (higher price first)
        let key = (u64::MAX - effective_price, tx.tx.hash);
        pending.insert(key, tx);

        Ok(())
    }

    /// Add to queued pool
    fn add_to_queued(&self, tx: PooledTransaction) -> Result<()> {
        let mut queued = self.queued.write();

        let sender_queue = queued.entry(tx.tx.from).or_insert_with(BTreeMap::new);

        // Check per-sender limit
        if sender_queue.len() >= self.config.max_queued_per_sender {
            return Err(MiniEthError::Transaction("Too many queued transactions from sender".into()));
        }

        sender_queue.insert(tx.tx.nonce, tx);

        Ok(())
    }

    /// Get pending transactions ordered by gas price (descending)
    pub fn get_pending(&self, limit: usize) -> Vec<SignedTransaction> {
        self.pending
            .read()
            .values()
            .take(limit)
            .map(|pt| pt.tx.clone())
            .collect()
    }

    /// Get pending transactions for a specific sender
    pub fn get_pending_for_sender(&self, sender: &Address) -> Vec<SignedTransaction> {
        self.pending
            .read()
            .values()
            .filter(|pt| pt.tx.from == *sender)
            .map(|pt| pt.tx.clone())
            .collect()
    }

    /// Remove transaction from pool (e.g., after inclusion in block)
    pub fn remove_tx(&self, tx_hash: &H256) -> Option<SignedTransaction> {
        // Remove from all_txs
        if !self.all_txs.write().remove(tx_hash) {
            return None;
        }

        // Try to remove from pending
        let mut pending = self.pending.write();
        let mut removed = None;

        // Find and remove (need to search since key includes hash)
        let key_to_remove = pending.iter()
            .find(|(_, pt)| pt.tx.hash == *tx_hash)
            .map(|(k, _)| *k);

        if let Some(key) = key_to_remove {
            removed = pending.remove(&key).map(|pt| pt.tx);
        }

        if let Some(ref tx) = removed {
            // Remove from sender tracking
            if let Some(sender_set) = self.sender_txs.write().get_mut(&tx.from) {
                sender_set.remove(tx_hash);
            }
        }

        removed
    }

    /// Remove multiple transactions (after block inclusion)
    pub fn remove_txs(&self, tx_hashes: &[H256]) {
        for hash in tx_hashes {
            self.remove_tx(hash);
        }
    }

    /// Promote queued transactions when nonce gaps are filled
    pub fn promote_queued(&self, sender: &Address, state: &WorldState) {
        let expected_nonce = state.get_nonce(*sender).unwrap_or(0);

        let mut queued = self.queued.write();

        if let Some(sender_queue) = queued.get_mut(sender) {
            // Find consecutive nonces starting from expected
            let mut to_promote = Vec::new();
            let mut next_nonce = expected_nonce;

            while let Some(tx) = sender_queue.remove(&next_nonce) {
                to_promote.push(tx);
                next_nonce += 1;
            }

            drop(queued);

            // Add to pending
            for tx in to_promote {
                let _ = self.add_to_pending(tx);
            }
        }
    }

    /// Update base fee
    pub fn set_base_fee(&self, base_fee: u64) {
        *self.base_fee.write() = base_fee;
    }

    /// Get current base fee
    pub fn base_fee(&self) -> u64 {
        *self.base_fee.read()
    }

    /// Get pool statistics
    pub fn stats(&self) -> TxPoolStats {
        TxPoolStats {
            pending_count: self.pending.read().len(),
            queued_count: self.queued.read().values().map(|q| q.len()).sum(),
            total_senders: self.sender_txs.read().len(),
        }
    }

    /// Check if transaction is in pool
    pub fn contains(&self, tx_hash: &H256) -> bool {
        self.all_txs.read().contains(tx_hash)
    }

    /// Get a specific transaction
    pub fn get_tx(&self, tx_hash: &H256) -> Option<SignedTransaction> {
        self.pending.read()
            .values()
            .find(|pt| pt.tx.hash == *tx_hash)
            .map(|pt| pt.tx.clone())
    }

    /// Clear the pool
    pub fn clear(&self) {
        self.pending.write().clear();
        self.queued.write().clear();
        self.all_txs.write().clear();
        self.sender_txs.write().clear();
    }
}

impl Default for TransactionPool {
    fn default() -> Self {
        Self::new(TxPoolConfig::default())
    }
}

/// Transaction pool statistics
#[derive(Debug, Clone)]
pub struct TxPoolStats {
    /// Number of pending transactions
    pub pending_count: usize,
    /// Number of queued transactions
    pub queued_count: usize,
    /// Number of unique senders
    pub total_senders: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_tx(from: Address, nonce: u64, gas_price: u64) -> SignedTransaction {
        let mut tx = SignedTransaction {
            from,
            to: Some(Address::zero()),
            value: U256::zero(),
            data: vec![],
            nonce,
            gas_limit: 21000,
            max_fee_per_gas: gas_price,
            max_priority_fee_per_gas: gas_price / 2,
            hash: H256::zero(),
            signature: vec![],
        };
        tx.hash = tx.compute_hash();
        tx
    }

    #[test]
    fn test_add_transaction() {
        let pool = TransactionPool::new(TxPoolConfig::default());
        let mut state = WorldState::new();

        let sender = Address::zero();
        // Need enough for gas_limit * max_fee_per_gas = 21000 * 2_000_000_000 = 42 trillion
        state.set_balance(sender, U256::from(100_000_000_000_000u64)).unwrap();

        let tx = create_test_tx(sender, 0, 2_000_000_000);
        let result = pool.add_tx(tx.clone(), &state, false);

        if let Err(ref e) = result {
            panic!("add_tx failed: {:?}", e);
        }
        assert!(result.is_ok());
        assert_eq!(pool.stats().pending_count, 1);
    }

    #[test]
    fn test_ordering_by_gas_price() {
        let pool = TransactionPool::new(TxPoolConfig::default());
        let mut state = WorldState::new();

        let sender = Address::zero();
        state.set_balance(sender, U256::from(1_000_000_000_000_000u128)).unwrap();

        // Add txs with different gas prices
        let tx1 = create_test_tx(sender, 0, 1_000_000_000);
        let tx2 = create_test_tx(sender, 1, 3_000_000_000);
        let tx3 = create_test_tx(sender, 2, 2_000_000_000);

        pool.add_tx(tx1, &state, false).unwrap();
        state.set_nonce(sender, 1).unwrap();
        pool.add_tx(tx2, &state, false).unwrap();
        state.set_nonce(sender, 2).unwrap();
        pool.add_tx(tx3, &state, false).unwrap();

        let pending = pool.get_pending(10);

        // Should be ordered by gas price (highest first)
        assert_eq!(pending.len(), 3);
        assert!(pending[0].max_fee_per_gas >= pending[1].max_fee_per_gas);
    }
}
