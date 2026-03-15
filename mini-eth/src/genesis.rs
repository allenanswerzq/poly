//! Genesis block and initial state configuration

use std::collections::HashMap;
use eth_primitives::{Address, H256, U256};
use serde::{Deserialize, Serialize};

use crate::types::{Block, BlockHeader, Account, LogsBloom};
use crate::state::WorldState;
use crate::error::Result;

/// Genesis configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisConfig {
    /// Chain ID
    pub chain_id: u64,

    /// Genesis timestamp
    pub timestamp: u64,

    /// Initial gas limit
    pub gas_limit: u64,

    /// Initial difficulty
    pub difficulty: u64,

    /// Extra data
    #[serde(default)]
    pub extra_data: Vec<u8>,

    /// Coinbase (beneficiary)
    pub coinbase: Address,

    /// Initial allocations (address -> balance)
    #[serde(default)]
    pub alloc: HashMap<Address, GenesisAlloc>,

    /// Initial validators (for PoA)
    #[serde(default)]
    pub validators: Vec<Address>,

    /// EIP-1559 base fee
    #[serde(default)]
    pub base_fee_per_gas: Option<u64>,
}

/// Genesis allocation for an account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisAlloc {
    /// Balance
    pub balance: U256,

    /// Contract code (optional)
    #[serde(default)]
    pub code: Option<Vec<u8>>,

    /// Storage (optional)
    #[serde(default)]
    pub storage: Option<HashMap<U256, H256>>,

    /// Nonce (optional)
    #[serde(default)]
    pub nonce: Option<u64>,
}

impl Default for GenesisConfig {
    fn default() -> Self {
        // Default development chain configuration
        let mut alloc = HashMap::new();

        // Dev accounts with initial balance
        let dev_accounts = [
            "0x0000000000000000000000000000000000000001",
            "0x0000000000000000000000000000000000000002",
            "0x0000000000000000000000000000000000000003",
        ];

        for addr in dev_accounts {
            let bytes = hex::decode(addr.strip_prefix("0x").unwrap()).unwrap();
            let mut arr = [0u8; 20];
            arr.copy_from_slice(&bytes);
            alloc.insert(
                Address::from(arr),
                GenesisAlloc {
                    balance: U256::from(1_000_000_000_000_000_000_000u128), // 1000 ETH
                    code: None,
                    storage: None,
                    nonce: None,
                },
            );
        }

        GenesisConfig {
            chain_id: 1337,
            timestamp: 0,
            gas_limit: 30_000_000,
            difficulty: 1,
            extra_data: b"mini-eth genesis".to_vec(),
            coinbase: Address::zero(),
            alloc,
            validators: vec![],
            base_fee_per_gas: Some(1_000_000_000), // 1 gwei
        }
    }
}

impl GenesisConfig {
    /// Create a new genesis config for development
    pub fn dev() -> Self {
        Self::default()
    }

    /// Create a mainnet-like genesis (empty)
    pub fn mainnet() -> Self {
        GenesisConfig {
            chain_id: 1,
            timestamp: 0,
            gas_limit: 30_000_000,
            difficulty: 1,
            extra_data: vec![],
            coinbase: Address::zero(),
            alloc: HashMap::new(),
            validators: vec![],
            base_fee_per_gas: Some(1_000_000_000),
        }
    }

    /// Add an allocation
    pub fn with_alloc(mut self, address: Address, balance: U256) -> Self {
        self.alloc.insert(address, GenesisAlloc {
            balance,
            code: None,
            storage: None,
            nonce: None,
        });
        self
    }

    /// Add a contract allocation
    pub fn with_contract(mut self, address: Address, balance: U256, code: Vec<u8>) -> Self {
        self.alloc.insert(address, GenesisAlloc {
            balance,
            code: Some(code),
            storage: None,
            nonce: None,
        });
        self
    }

    /// Add a validator
    pub fn with_validator(mut self, address: Address) -> Self {
        self.validators.push(address);
        self
    }

    /// Set chain ID
    pub fn with_chain_id(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }

    /// Generate the genesis block
    pub fn genesis_block(&self) -> Block {
        // Calculate state root from allocations
        let state_root = self.calculate_state_root();

        let header = BlockHeader {
            parent_hash: H256::zero(),
            beneficiary: self.coinbase,
            state_root,
            transactions_root: H256::zero(), // Empty trie root
            receipts_root: H256::zero(),
            logs_bloom: LogsBloom::default(),
            number: 0,
            gas_limit: self.gas_limit,
            gas_used: 0,
            timestamp: self.timestamp,
            extra_data: self.extra_data.clone(),
            base_fee: self.base_fee_per_gas.unwrap_or(1_000_000_000),
        };

        Block {
            header,
            transactions: vec![],
        }
    }

    /// Calculate state root from allocations
    fn calculate_state_root(&self) -> H256 {
        // In a full implementation, this would use MPT to calculate the root
        // For now, return a deterministic hash based on config
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        self.chain_id.hash(&mut hasher);
        self.alloc.len().hash(&mut hasher);
        let hash_value = hasher.finish();

        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(&hash_value.to_be_bytes());
        H256::from(bytes)
    }

    /// Initialize world state from genesis
    pub fn init_state(&self, state: &mut WorldState) -> Result<()> {
        for (address, alloc) in &self.alloc {
            // Set balance
            state.set_balance(*address, alloc.balance)?;

            // Set nonce
            if let Some(nonce) = alloc.nonce {
                state.set_nonce(*address, nonce)?;
            }

            // Set code
            if let Some(ref code) = alloc.code {
                state.set_code(*address, code.clone())?;
            }

            // Set storage
            if let Some(ref storage) = alloc.storage {
                for (key, value) in storage {
                    state.set_storage(*address, *key, *value)?;
                }
            }
        }

        // Commit the genesis state
        state.commit()?;

        Ok(())
    }

    /// Compute genesis hash
    pub fn genesis_hash(&self) -> H256 {
        self.genesis_block().hash()
    }
}

/// Genesis builder for fluent API
pub struct GenesisBuilder {
    config: GenesisConfig,
}

impl GenesisBuilder {
    /// Create a new genesis builder
    pub fn new() -> Self {
        GenesisBuilder {
            config: GenesisConfig::default(),
        }
    }

    /// Set chain ID
    pub fn chain_id(mut self, id: u64) -> Self {
        self.config.chain_id = id;
        self
    }

    /// Set timestamp
    pub fn timestamp(mut self, ts: u64) -> Self {
        self.config.timestamp = ts;
        self
    }

    /// Set gas limit
    pub fn gas_limit(mut self, limit: u64) -> Self {
        self.config.gas_limit = limit;
        self
    }

    /// Set base fee
    pub fn base_fee(mut self, fee: u64) -> Self {
        self.config.base_fee_per_gas = Some(fee);
        self
    }

    /// Add an account with balance
    pub fn account(mut self, address: Address, balance: U256) -> Self {
        self.config.alloc.insert(address, GenesisAlloc {
            balance,
            code: None,
            storage: None,
            nonce: None,
        });
        self
    }

    /// Add a contract
    pub fn contract(mut self, address: Address, balance: U256, code: Vec<u8>) -> Self {
        self.config.alloc.insert(address, GenesisAlloc {
            balance,
            code: Some(code),
            storage: None,
            nonce: None,
        });
        self
    }

    /// Add a validator
    pub fn validator(mut self, address: Address) -> Self {
        self.config.validators.push(address);
        self
    }

    /// Build the genesis config
    pub fn build(self) -> GenesisConfig {
        self.config
    }
}

impl Default for GenesisBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_genesis() {
        let genesis = GenesisConfig::default();
        assert_eq!(genesis.chain_id, 1337);
        assert!(!genesis.alloc.is_empty());
    }

    #[test]
    fn test_genesis_block() {
        let genesis = GenesisConfig::default();
        let block = genesis.genesis_block();

        assert_eq!(block.number(), 0);
        assert_eq!(block.header.parent_hash, H256::zero());
    }

    #[test]
    fn test_genesis_builder() {
        let genesis = GenesisBuilder::new()
            .chain_id(42)
            .gas_limit(10_000_000)
            .base_fee(1_000_000)
            .build();

        assert_eq!(genesis.chain_id, 42);
        assert_eq!(genesis.gas_limit, 10_000_000);
        assert_eq!(genesis.base_fee_per_gas, Some(1_000_000));
    }

    #[test]
    fn test_init_state() {
        let genesis = GenesisConfig::default();
        let mut state = WorldState::new();

        genesis.init_state(&mut state).unwrap();

        // Check that allocations were applied
        for (address, alloc) in &genesis.alloc {
            let balance = state.get_balance(*address).unwrap();
            assert_eq!(balance, alloc.balance);
        }
    }
}
