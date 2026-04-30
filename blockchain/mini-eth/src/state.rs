//! World State - Manages all account state
//!
//! This provides:
//! - Account balance/nonce management
//! - Contract code storage
//! - Contract storage slots

use std::collections::HashMap;
use eth_primitives::{Address, H256, U256, keccak256};

use crate::types::Account;
use crate::error::{MiniEthError, Result};

/// World state database
#[derive(Clone)]
pub struct WorldState {
    /// Account cache for fast access
    accounts: HashMap<Address, Account>,

    /// Contract code storage (code_hash -> bytecode)
    code: HashMap<H256, Vec<u8>>,

    /// Contract storage (address -> (slot -> value))
    storage: HashMap<Address, HashMap<U256, H256>>,

    /// State root
    state_root: H256,
}

impl WorldState {
    /// Create a new empty world state
    pub fn new() -> Self {
        WorldState {
            accounts: HashMap::new(),
            code: HashMap::new(),
            storage: HashMap::new(),
            state_root: H256::zero(),
        }
    }

    /// Get the current state root
    pub fn state_root(&self) -> H256 {
        self.state_root
    }

    /// Get account (returns default if not exists)
    pub fn get_account(&self, address: Address) -> Result<Account> {
        Ok(self.accounts.get(&address).cloned().unwrap_or_default())
    }

    /// Check if account exists
    pub fn account_exists(&self, address: Address) -> bool {
        self.accounts.contains_key(&address)
    }

    /// Set account state
    pub fn set_account(&mut self, address: Address, account: Account) -> Result<()> {
        self.accounts.insert(address, account);
        self.update_state_root();
        Ok(())
    }

    /// Get account balance
    pub fn get_balance(&self, address: Address) -> Result<U256> {
        Ok(self.accounts.get(&address).map(|a| a.balance).unwrap_or_default())
    }

    /// Set account balance
    pub fn set_balance(&mut self, address: Address, balance: U256) -> Result<()> {
        let account = self.accounts.entry(address).or_insert_with(Account::new);
        account.balance = balance;
        self.update_state_root();
        Ok(())
    }

    /// Add to account balance
    pub fn add_balance(&mut self, address: Address, amount: U256) -> Result<()> {
        let account = self.accounts.entry(address).or_insert_with(Account::new);
        account.balance = account.balance.checked_add(amount)
            .map_err(|_| MiniEthError::State("Balance overflow".into()))?;
        self.update_state_root();
        Ok(())
    }

    /// Subtract from account balance
    pub fn sub_balance(&mut self, address: Address, amount: U256) -> Result<()> {
        let account = self.accounts.entry(address).or_insert_with(Account::new);
        if account.balance < amount {
            return Err(MiniEthError::InsufficientBalance {
                required: amount,
                available: account.balance,
            });
        }
        account.balance = account.balance.checked_sub(amount)
            .map_err(|_| MiniEthError::State("Balance underflow".into()))?;
        self.update_state_root();
        Ok(())
    }

    /// Get account nonce
    pub fn get_nonce(&self, address: Address) -> Result<u64> {
        Ok(self.accounts.get(&address).map(|a| a.nonce).unwrap_or(0))
    }

    /// Increment nonce
    pub fn increment_nonce(&mut self, address: Address) -> Result<()> {
        let account = self.accounts.entry(address).or_insert_with(Account::new);
        account.nonce += 1;
        self.update_state_root();
        Ok(())
    }

    /// Set account nonce
    pub fn set_nonce(&mut self, address: Address, nonce: u64) -> Result<()> {
        let account = self.accounts.entry(address).or_insert_with(Account::new);
        account.nonce = nonce;
        self.update_state_root();
        Ok(())
    }

    /// Store contract code
    pub fn set_code(&mut self, address: Address, code: Vec<u8>) -> Result<()> {
        let code_hash = keccak256(&code);

        // Store code by hash
        self.code.insert(code_hash, code);

        // Update account
        let account = self.accounts.entry(address).or_insert_with(Account::new);
        account.code_hash = code_hash;

        self.update_state_root();
        Ok(())
    }

    /// Get contract code
    pub fn get_code(&self, address: Address) -> Result<Vec<u8>> {
        let account = self.accounts.get(&address);
        match account {
            Some(a) if a.code_hash != H256::zero() => {
                Ok(self.code.get(&a.code_hash).cloned().unwrap_or_default())
            }
            _ => Ok(vec![]),
        }
    }

    /// Get contract code by hash
    pub fn get_code_by_hash(&self, code_hash: H256) -> Option<Vec<u8>> {
        self.code.get(&code_hash).cloned()
    }

    /// Set storage value
    pub fn set_storage(&mut self, address: Address, slot: U256, value: H256) -> Result<()> {
        let account_storage = self.storage.entry(address).or_insert_with(HashMap::new);

        if value == H256::zero() {
            account_storage.remove(&slot);
        } else {
            account_storage.insert(slot, value);
        }

        self.update_state_root();
        Ok(())
    }

    /// Get storage value
    pub fn get_storage(&self, address: Address, slot: U256) -> Result<H256> {
        Ok(self.storage
            .get(&address)
            .and_then(|s| s.get(&slot))
            .copied()
            .unwrap_or(H256::zero()))
    }

    /// Get all storage for an address
    pub fn get_all_storage(&self, address: Address) -> HashMap<U256, H256> {
        self.storage.get(&address).cloned().unwrap_or_default()
    }

    /// Compute contract creation address
    pub fn compute_contract_address(sender: Address, nonce: u64) -> Address {
        let mut data = Vec::new();
        data.extend_from_slice(sender.as_bytes());
        data.extend_from_slice(&nonce.to_be_bytes());

        let hash = keccak256(&data);
        let mut addr_bytes = [0u8; 20];
        addr_bytes.copy_from_slice(&hash.as_bytes()[12..32]);
        Address::from(addr_bytes)
    }

    /// Create a snapshot for reverting state
    pub fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            accounts: self.accounts.clone(),
            code: self.code.clone(),
            storage: self.storage.clone(),
            state_root: self.state_root,
        }
    }

    /// Revert to a snapshot
    pub fn revert(&mut self, snapshot: StateSnapshot) {
        self.accounts = snapshot.accounts;
        self.code = snapshot.code;
        self.storage = snapshot.storage;
        self.state_root = snapshot.state_root;
    }

    /// Commit state changes
    pub fn commit(&mut self) -> Result<()> {
        self.update_state_root();
        Ok(())
    }

    /// Update state root after modifications
    fn update_state_root(&mut self) {
        // Compute merkle root of all accounts
        let mut data = Vec::new();

        let mut sorted_addrs: Vec<_> = self.accounts.keys().collect();
        sorted_addrs.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

        for addr in sorted_addrs {
            if let Some(account) = self.accounts.get(addr) {
                data.extend_from_slice(addr.as_bytes());
                data.extend_from_slice(&account.nonce.to_be_bytes());
                // Add balance bytes
                let balance_bytes = account.balance.to_be_bytes();
                data.extend_from_slice(&balance_bytes);
                data.extend_from_slice(account.code_hash.as_bytes());
            }
        }

        self.state_root = keccak256(&data);
    }

    /// Get number of accounts
    pub fn account_count(&self) -> usize {
        self.accounts.len()
    }

    /// Get all accounts (for debugging)
    pub fn all_accounts(&self) -> HashMap<Address, Account> {
        self.accounts.clone()
    }
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new()
    }
}

/// State snapshot for reverting
#[derive(Clone)]
pub struct StateSnapshot {
    accounts: HashMap<Address, Account>,
    code: HashMap<H256, Vec<u8>>,
    storage: HashMap<Address, HashMap<U256, H256>>,
    state_root: H256,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_operations() {
        let mut state = WorldState::new();

        let addr = Address::zero();
        let balance = U256::from(1000u64);

        state.set_balance(addr, balance).unwrap();
        assert_eq!(state.get_balance(addr).unwrap(), balance);

        state.increment_nonce(addr).unwrap();
        assert_eq!(state.get_nonce(addr).unwrap(), 1);
    }

    #[test]
    fn test_contract_code() {
        let mut state = WorldState::new();

        let addr = Address::zero();
        let code = vec![0x60, 0x00, 0x60, 0x00, 0xf3]; // Simple return

        state.set_code(addr, code.clone()).unwrap();

        let retrieved = state.get_code(addr).unwrap();
        assert_eq!(retrieved, code);
    }

    #[test]
    fn test_storage() {
        let mut state = WorldState::new();

        let addr = Address::zero();
        let slot = U256::from(1u64);
        let value = H256::from([0xff; 32]);

        state.set_storage(addr, slot, value).unwrap();
        assert_eq!(state.get_storage(addr, slot).unwrap(), value);
    }

    #[test]
    fn test_snapshot_revert() {
        let mut state = WorldState::new();

        let addr = Address::zero();
        state.set_balance(addr, U256::from(100u64)).unwrap();

        let snapshot = state.snapshot();

        state.set_balance(addr, U256::from(200u64)).unwrap();
        assert_eq!(state.get_balance(addr).unwrap(), U256::from(200u64));

        state.revert(snapshot);
        assert_eq!(state.get_balance(addr).unwrap(), U256::from(100u64));
    }
}
