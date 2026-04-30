//! # State Database
//!
//! L2 state storage using MPT for state roots

use eth_primitives::{Address, H256, keccak256};
use mpt_trie::PatriciaTrie;
use std::collections::HashMap;

/// Account state
#[derive(Debug, Clone)]
pub struct Account {
    /// Account balance in wei
    pub balance: u64,
    /// Transaction nonce
    pub nonce: u64,
    /// Code hash (for contracts)
    pub code_hash: H256,
    /// Storage root (for contracts)
    pub storage_root: H256,
}

impl Account {
    /// Create new empty account
    pub fn new() -> Self {
        Account {
            balance: 0,
            nonce: 0,
            code_hash: keccak256(&[]), // Empty code hash
            storage_root: H256::default(), // Empty storage
        }
    }

    /// Create account with balance
    pub fn with_balance(balance: u64) -> Self {
        Account {
            balance,
            nonce: 0,
            code_hash: keccak256(&[]),
            storage_root: H256::default(),
        }
    }

    /// Serialize account for storage
    pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(48);
        data.extend_from_slice(&self.balance.to_le_bytes());
        data.extend_from_slice(&self.nonce.to_le_bytes());
        data.extend_from_slice(self.code_hash.as_bytes());
        // Skip storage_root for simplicity
        data
    }

    /// Deserialize account from storage
    pub fn deserialize(data: &[u8]) -> Option<Self> {
        if data.len() < 48 {
            return None;
        }
        let balance = u64::from_le_bytes(data[0..8].try_into().ok()?);
        let nonce = u64::from_le_bytes(data[8..16].try_into().ok()?);
        let mut code_bytes = [0u8; 32];
        code_bytes.copy_from_slice(&data[16..48]);
        Some(Account {
            balance,
            nonce,
            code_hash: H256::new(code_bytes),
            storage_root: H256::default(),
        })
    }
}

impl Default for Account {
    fn default() -> Self {
        Self::new()
    }
}

/// State database with MPT backing
pub struct StateDB {
    /// Account states in MPT
    trie: PatriciaTrie<mpt_trie::trie::MemoryDB>,
    /// Cache for frequently accessed accounts
    cache: HashMap<Address, Account>,
    /// Dirty accounts (modified since last commit)
    dirty: HashMap<Address, Account>,
}

impl StateDB {
    /// Create new empty state
    pub fn new() -> Self {
        StateDB {
            trie: PatriciaTrie::new_memory(),
            cache: HashMap::new(),
            dirty: HashMap::new(),
        }
    }

    /// Get current state root
    pub fn state_root(&self) -> H256 {
        self.trie.root_hash()
    }

    /// Get account (returns default if not exists)
    pub fn get_account(&self, address: &Address) -> Account {
        // Check dirty first
        if let Some(account) = self.dirty.get(address) {
            return account.clone();
        }

        // Check cache
        if let Some(account) = self.cache.get(address) {
            return account.clone();
        }

        // Check trie
        let key = address.as_bytes();
        if let Some(data) = self.trie.get(key) {
            if let Some(account) = Account::deserialize(&data) {
                return account;
            }
        }

        Account::new()
    }

    /// Check if account exists
    pub fn account_exists(&self, address: &Address) -> bool {
        self.dirty.contains_key(address) ||
        self.cache.contains_key(address) ||
        self.trie.get(address.as_bytes()).is_some()
    }

    /// Get balance
    pub fn get_balance(&self, address: &Address) -> u64 {
        self.get_account(address).balance
    }

    /// Get nonce
    pub fn get_nonce(&self, address: &Address) -> u64 {
        self.get_account(address).nonce
    }

    /// Set account (marks as dirty)
    pub fn set_account(&mut self, address: Address, account: Account) {
        self.dirty.insert(address, account);
    }

    /// Set balance
    pub fn set_balance(&mut self, address: Address, balance: u64) {
        let mut account = self.get_account(&address);
        account.balance = balance;
        self.set_account(address, account);
    }

    /// Increment nonce
    pub fn increment_nonce(&mut self, address: &Address) {
        let mut account = self.get_account(address);
        account.nonce += 1;
        self.set_account(*address, account);
    }

    /// Add to balance
    pub fn add_balance(&mut self, address: Address, amount: u64) {
        let mut account = self.get_account(&address);
        account.balance = account.balance.saturating_add(amount);
        self.set_account(address, account);
    }

    /// Subtract from balance
    pub fn sub_balance(&mut self, address: &Address, amount: u64) -> bool {
        let mut account = self.get_account(address);
        if account.balance < amount {
            return false;
        }
        account.balance -= amount;
        self.set_account(*address, account);
        true
    }

    /// Commit dirty accounts to trie
    pub fn commit(&mut self) -> H256 {
        for (address, account) in self.dirty.drain() {
            let key = address.as_bytes();
            let value = account.serialize();
            self.trie.insert(key, value);
            self.cache.insert(address, account);
        }

        self.state_root()
    }

    /// Revert uncommitted changes
    pub fn revert(&mut self) {
        self.dirty.clear();
    }

    /// Create checkpoint for nested transactions
    pub fn checkpoint(&self) -> StateCheckpoint {
        StateCheckpoint {
            dirty: self.dirty.clone(),
        }
    }

    /// Restore from checkpoint
    pub fn restore(&mut self, checkpoint: StateCheckpoint) {
        self.dirty = checkpoint.dirty;
    }
}

impl Default for StateDB {
    fn default() -> Self {
        Self::new()
    }
}

/// State checkpoint for reverting changes
#[derive(Debug, Clone)]
pub struct StateCheckpoint {
    dirty: HashMap<Address, Account>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_serialization() {
        let account = Account::with_balance(1_000_000);
        let data = account.serialize();
        let restored = Account::deserialize(&data).unwrap();

        assert_eq!(account.balance, restored.balance);
        assert_eq!(account.nonce, restored.nonce);
    }

    #[test]
    fn test_state_db() {
        let mut state = StateDB::new();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        // Set initial balances
        state.set_balance(alice, 1_000_000);
        state.set_balance(bob, 500_000);

        assert_eq!(state.get_balance(&alice), 1_000_000);
        assert_eq!(state.get_balance(&bob), 500_000);
    }

    #[test]
    fn test_transfer() {
        let mut state = StateDB::new();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        state.set_balance(alice, 1_000_000);

        // Transfer 100_000 from alice to bob
        assert!(state.sub_balance(&alice, 100_000));
        state.add_balance(bob, 100_000);

        assert_eq!(state.get_balance(&alice), 900_000);
        assert_eq!(state.get_balance(&bob), 100_000);
    }

    #[test]
    fn test_commit() {
        let mut state = StateDB::new();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();

        let root1 = state.state_root();

        state.set_balance(alice, 1_000_000);
        let root2 = state.commit();

        assert_ne!(root1, root2);

        // Balance persisted in trie
        state.cache.clear();
        assert_eq!(state.get_balance(&alice), 1_000_000);
    }

    #[test]
    fn test_revert() {
        let mut state = StateDB::new();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();

        state.set_balance(alice, 1_000_000);
        state.commit();

        // Make changes
        state.set_balance(alice, 500_000);
        assert_eq!(state.get_balance(&alice), 500_000);

        // Revert
        state.revert();
        assert_eq!(state.get_balance(&alice), 1_000_000);
    }

    #[test]
    fn test_nonce() {
        let mut state = StateDB::new();

        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();

        assert_eq!(state.get_nonce(&alice), 0);

        state.increment_nonce(&alice);
        assert_eq!(state.get_nonce(&alice), 1);

        state.increment_nonce(&alice);
        assert_eq!(state.get_nonce(&alice), 2);
    }
}
