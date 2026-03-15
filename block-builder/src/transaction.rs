//! # Pending Transaction
//!
//! Transactions waiting in the mempool

use eth_primitives::{Address, H256, keccak256};
use std::cmp::Ordering;

/// Transaction priority for ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionPriority {
    /// High priority (MEV bundles)
    High,
    /// Normal priority (regular transactions)
    Normal,
    /// Low priority (low fee transactions)
    Low,
}

/// A pending transaction in the mempool
#[derive(Debug, Clone)]
pub struct PendingTransaction {
    /// Transaction hash
    pub hash: H256,
    /// Sender
    pub from: Address,
    /// Recipient
    pub to: Option<Address>,
    /// Value in wei
    pub value: u64,
    /// Calldata
    pub data: Vec<u8>,
    /// Nonce
    pub nonce: u64,
    /// Gas limit
    pub gas_limit: u64,
    /// Max fee per gas (EIP-1559)
    pub max_fee_per_gas: u64,
    /// Max priority fee per gas (EIP-1559)
    pub max_priority_fee: u64,
    /// When transaction was received
    pub received_at: u64,
    /// Priority level
    pub priority: TransactionPriority,
}

impl PendingTransaction {
    /// Create new pending transaction
    pub fn new(
        from: Address,
        to: Option<Address>,
        value: u64,
        data: Vec<u8>,
        nonce: u64,
        gas_limit: u64,
        max_fee_per_gas: u64,
        max_priority_fee: u64,
    ) -> Self {
        let mut tx_data = Vec::new();
        tx_data.extend_from_slice(from.as_bytes());
        if let Some(ref addr) = to {
            tx_data.extend_from_slice(addr.as_bytes());
        }
        tx_data.extend_from_slice(&value.to_be_bytes());
        tx_data.extend_from_slice(&nonce.to_be_bytes());
        tx_data.extend_from_slice(&data);

        let hash = keccak256(&tx_data);

        PendingTransaction {
            hash,
            from,
            to,
            value,
            data,
            nonce,
            gas_limit,
            max_fee_per_gas,
            max_priority_fee,
            received_at: current_timestamp(),
            priority: TransactionPriority::Normal,
        }
    }

    /// Create simple transfer
    pub fn transfer(
        from: Address,
        to: Address,
        value: u64,
        nonce: u64,
        max_fee_per_gas: u64,
        max_priority_fee: u64,
    ) -> Self {
        PendingTransaction::new(
            from,
            Some(to),
            value,
            Vec::new(),
            nonce,
            21000, // Standard transfer gas
            max_fee_per_gas,
            max_priority_fee,
        )
    }

    /// Calculate effective gas price at given base fee
    pub fn effective_gas_price(&self, base_fee: u64) -> u64 {
        let priority_fee = self.max_priority_fee.min(self.max_fee_per_gas.saturating_sub(base_fee));
        base_fee + priority_fee
    }

    /// Calculate miner/builder tip
    pub fn tip(&self, base_fee: u64) -> u64 {
        self.max_priority_fee.min(self.max_fee_per_gas.saturating_sub(base_fee))
    }

    /// Calculate total fee paid
    pub fn total_fee(&self, base_fee: u64) -> u64 {
        self.effective_gas_price(base_fee) * self.gas_limit
    }

    /// Is this a contract call?
    pub fn is_contract_call(&self) -> bool {
        self.to.is_some() && !self.data.is_empty()
    }

    /// Set priority
    pub fn with_priority(mut self, priority: TransactionPriority) -> Self {
        self.priority = priority;
        self
    }
}

/// Compare transactions for ordering (higher priority/fee first)
impl Ord for PendingTransaction {
    fn cmp(&self, other: &Self) -> Ordering {
        // First compare priority
        match (&self.priority, &other.priority) {
            (TransactionPriority::High, TransactionPriority::High) => {}
            (TransactionPriority::High, _) => return Ordering::Greater,
            (_, TransactionPriority::High) => return Ordering::Less,
            (TransactionPriority::Normal, TransactionPriority::Normal) => {}
            (TransactionPriority::Normal, TransactionPriority::Low) => return Ordering::Greater,
            (TransactionPriority::Low, TransactionPriority::Normal) => return Ordering::Less,
            (TransactionPriority::Low, TransactionPriority::Low) => {}
        }

        // Then compare by priority fee (higher is better)
        self.max_priority_fee.cmp(&other.max_priority_fee)
    }
}

impl PartialOrd for PendingTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for PendingTransaction {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Eq for PendingTransaction {}

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

    fn test_addresses() -> (Address, Address) {
        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();
        (alice, bob)
    }

    #[test]
    fn test_create_transaction() {
        let (alice, bob) = test_addresses();

        let tx = PendingTransaction::transfer(
            alice, bob, 1000, 0,
            100_000_000_000, // 100 gwei max fee
            2_000_000_000,   // 2 gwei priority fee
        );

        assert_eq!(tx.from, alice);
        assert_eq!(tx.to, Some(bob));
        assert_eq!(tx.value, 1000);
        assert_eq!(tx.gas_limit, 21000);
    }

    #[test]
    fn test_effective_gas_price() {
        let (alice, bob) = test_addresses();

        let tx = PendingTransaction::transfer(
            alice, bob, 1000, 0,
            100_000_000_000, // 100 gwei max fee
            2_000_000_000,   // 2 gwei priority fee
        );

        // Base fee = 50 gwei
        let base_fee = 50_000_000_000;
        let effective = tx.effective_gas_price(base_fee);

        // Should be base_fee + min(priority, max_fee - base_fee)
        // = 50 + min(2, 100 - 50) = 50 + 2 = 52 gwei
        assert_eq!(effective, 52_000_000_000);
    }

    #[test]
    fn test_transaction_ordering() {
        let (alice, bob) = test_addresses();

        let tx1 = PendingTransaction::transfer(alice, bob, 1000, 0, 100, 10);
        let tx2 = PendingTransaction::transfer(alice, bob, 1000, 0, 100, 20);
        let tx3 = PendingTransaction::transfer(alice, bob, 1000, 0, 100, 5);

        // Higher priority fee should come first
        assert!(tx2 > tx1);
        assert!(tx1 > tx3);

        // High priority overrides fee
        let tx4 = PendingTransaction::transfer(alice, bob, 1000, 0, 100, 1)
            .with_priority(TransactionPriority::High);
        assert!(tx4 > tx2);
    }
}
