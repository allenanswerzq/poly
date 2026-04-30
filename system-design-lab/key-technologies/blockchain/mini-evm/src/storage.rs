//! # EVM Storage
//!
//! Persistent key-value storage (H256 -> H256).
//! This is where contract state lives.

use eth_primitives::{H256, U256, Address};
use std::collections::HashMap;

/// Storage for a single contract
#[derive(Debug, Clone, Default)]
pub struct Storage {
    /// Current values
    slots: HashMap<H256, U256>,
    /// Original values at start of transaction (for gas calculation)
    original: HashMap<H256, U256>,
    /// Transient storage (EIP-1153)
    transient: HashMap<H256, U256>,
}

impl Storage {
    /// Create new empty storage
    pub fn new() -> Self {
        Storage {
            slots: HashMap::new(),
            original: HashMap::new(),
            transient: HashMap::new(),
        }
    }

    /// Load value from storage (SLOAD)
    pub fn load(&self, key: &H256) -> U256 {
        self.slots.get(key).copied().unwrap_or(U256::ZERO)
    }

    /// Store value to storage (SSTORE)
    /// Returns the gas cost
    pub fn store(&mut self, key: H256, value: U256) -> u64 {
        let current = self.load(&key);
        let original = self.original.get(&key).copied().unwrap_or(U256::ZERO);

        // Calculate gas based on EIP-2200
        let gas = self.calculate_sstore_gas(original, current, value);

        // Record original if first write
        if !self.original.contains_key(&key) {
            self.original.insert(key, current);
        }

        // Update current value
        if value.is_zero() {
            self.slots.remove(&key);
        } else {
            self.slots.insert(key, value);
        }

        gas
    }

    /// Calculate SSTORE gas cost (EIP-2200 / EIP-3529)
    fn calculate_sstore_gas(&self, original: U256, current: U256, new: U256) -> u64 {
        // Simplified gas calculation
        if current == new {
            // No-op
            100
        } else if original == current {
            if original.is_zero() {
                // Creating new slot
                20000
            } else if new.is_zero() {
                // Deleting slot (refund handled separately)
                2900
            } else {
                // Modifying existing slot
                2900
            }
        } else {
            // Already modified in this transaction
            100
        }
    }

    /// Load from transient storage (TLOAD)
    pub fn tload(&self, key: &H256) -> U256 {
        self.transient.get(key).copied().unwrap_or(U256::ZERO)
    }

    /// Store to transient storage (TSTORE)
    pub fn tstore(&mut self, key: H256, value: U256) {
        if value.is_zero() {
            self.transient.remove(&key);
        } else {
            self.transient.insert(key, value);
        }
    }

    /// Clear transient storage (at end of transaction)
    pub fn clear_transient(&mut self) {
        self.transient.clear();
    }

    /// Reset original values (at start of transaction)
    pub fn reset_original(&mut self) {
        self.original.clear();
    }

    /// Get all storage slots
    pub fn slots(&self) -> &HashMap<H256, U256> {
        &self.slots
    }
}

/// Global state database (maps addresses to storage)
#[derive(Debug, Clone, Default)]
pub struct StateDB {
    /// Account storage
    storage: HashMap<Address, Storage>,
    /// Account balances
    balances: HashMap<Address, U256>,
    /// Account nonces
    nonces: HashMap<Address, u64>,
    /// Account code
    code: HashMap<Address, Vec<u8>>,
    /// Account code hashes
    code_hashes: HashMap<Address, H256>,
}

impl StateDB {
    /// Create new empty state
    pub fn new() -> Self {
        StateDB::default()
    }

    /// Get storage for an address
    pub fn storage(&self, address: &Address) -> &Storage {
        static EMPTY: std::sync::OnceLock<Storage> = std::sync::OnceLock::new();
        self.storage.get(address).unwrap_or_else(|| EMPTY.get_or_init(Storage::new))
    }

    /// Get mutable storage for an address
    pub fn storage_mut(&mut self, address: &Address) -> &mut Storage {
        self.storage.entry(*address).or_insert_with(Storage::new)
    }

    /// Get balance
    pub fn balance(&self, address: &Address) -> U256 {
        self.balances.get(address).copied().unwrap_or(U256::ZERO)
    }

    /// Set balance
    pub fn set_balance(&mut self, address: Address, balance: U256) {
        self.balances.insert(address, balance);
    }

    /// Get nonce
    pub fn nonce(&self, address: &Address) -> u64 {
        self.nonces.get(address).copied().unwrap_or(0)
    }

    /// Increment nonce
    pub fn increment_nonce(&mut self, address: &Address) {
        let nonce = self.nonce(address);
        self.nonces.insert(*address, nonce + 1);
    }

    /// Get code
    pub fn code(&self, address: &Address) -> &[u8] {
        self.code.get(address).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Set code
    pub fn set_code(&mut self, address: Address, code: Vec<u8>) {
        use eth_primitives::keccak256;
        let hash = keccak256(&code);
        self.code_hashes.insert(address, hash);
        self.code.insert(address, code);
    }

    /// Get code hash
    pub fn code_hash(&self, address: &Address) -> H256 {
        self.code_hashes.get(address).copied().unwrap_or_else(|| {
            // Empty code hash (keccak256 of empty)
            H256::from_hex("0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470").unwrap()
        })
    }

    /// Check if account exists
    pub fn exists(&self, address: &Address) -> bool {
        self.balances.contains_key(address)
            || self.nonces.contains_key(address)
            || self.code.contains_key(address)
    }

    /// Transfer value between accounts
    pub fn transfer(&mut self, from: &Address, to: &Address, value: U256) -> bool {
        let from_balance = self.balance(from);
        if from_balance < value {
            return false;
        }

        self.set_balance(*from, from_balance - value);
        let to_balance = self.balance(to);
        self.set_balance(*to, to_balance + value);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage() {
        let mut storage = Storage::new();
        let key = H256::from_hex("0x0000000000000000000000000000000000000000000000000000000000000001").unwrap();

        assert_eq!(storage.load(&key), U256::ZERO);

        storage.store(key, U256::from_u64(42));
        assert_eq!(storage.load(&key), U256::from_u64(42));
    }

    #[test]
    fn test_transient_storage() {
        let mut storage = Storage::new();
        let key = H256::zero();

        storage.tstore(key, U256::from_u64(100));
        assert_eq!(storage.tload(&key), U256::from_u64(100));

        storage.clear_transient();
        assert_eq!(storage.tload(&key), U256::ZERO);
    }

    #[test]
    fn test_state_db() {
        let mut state = StateDB::new();
        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        state.set_balance(alice, U256::from_u64(1000));
        assert!(state.transfer(&alice, &bob, U256::from_u64(300)));

        assert_eq!(state.balance(&alice), U256::from_u64(700));
        assert_eq!(state.balance(&bob), U256::from_u64(300));
    }
}
