//! Core types for mini-eth

use eth_primitives::{Address, H256, U256};
use serde::{Deserialize, Serialize, Serializer, Deserializer};

/// Logs bloom filter (256 bytes)
#[derive(Debug, Clone)]
pub struct LogsBloom(pub [u8; 256]);

impl Default for LogsBloom {
    fn default() -> Self {
        LogsBloom([0u8; 256])
    }
}

impl Serialize for LogsBloom {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("0x{}", hex::encode(&self.0)))
    }
}

impl<'de> Deserialize<'de> for LogsBloom {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let s = s.strip_prefix("0x").unwrap_or(&s);
        let bytes = hex::decode(s).map_err(serde::de::Error::custom)?;
        if bytes.len() != 256 {
            return Err(serde::de::Error::custom("Invalid logs bloom length"));
        }
        let mut arr = [0u8; 256];
        arr.copy_from_slice(&bytes);
        Ok(LogsBloom(arr))
    }
}

/// Account state stored in the world state
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Account {
    /// Account nonce (number of transactions sent)
    pub nonce: u64,
    /// Account balance in wei
    pub balance: U256,
    /// Storage root hash (for contracts)
    pub storage_root: H256,
    /// Code hash (for contracts)
    pub code_hash: H256,
}

impl Account {
    /// Create a new empty account
    pub fn new() -> Self {
        Account {
            nonce: 0,
            balance: U256::zero(),
            storage_root: H256::zero(),
            code_hash: H256::zero(),
        }
    }

    /// Create an account with balance
    pub fn with_balance(balance: U256) -> Self {
        Account {
            balance,
            ..Default::default()
        }
    }

    /// Check if this is a contract account
    pub fn is_contract(&self) -> bool {
        self.code_hash != H256::zero()
    }

    /// Check if account is empty (EIP-161)
    pub fn is_empty(&self) -> bool {
        self.nonce == 0 && self.balance == U256::zero() && self.code_hash == H256::zero()
    }
}

/// Block header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    /// Parent block hash
    pub parent_hash: H256,
    /// Beneficiary (miner/validator address)
    pub beneficiary: Address,
    /// State root after executing block
    pub state_root: H256,
    /// Transactions root
    pub transactions_root: H256,
    /// Receipts root
    pub receipts_root: H256,
    /// Logs bloom filter
    pub logs_bloom: LogsBloom,
    /// Block number
    pub number: u64,
    /// Gas limit
    pub gas_limit: u64,
    /// Gas used
    pub gas_used: u64,
    /// Timestamp
    pub timestamp: u64,
    /// Extra data
    pub extra_data: Vec<u8>,
    /// Base fee per gas (EIP-1559)
    pub base_fee: u64,
}

impl BlockHeader {
    /// Compute block hash
    pub fn hash(&self) -> H256 {
        use eth_primitives::keccak256;

        let mut data = Vec::new();
        data.extend_from_slice(self.parent_hash.as_bytes());
        data.extend_from_slice(self.beneficiary.as_bytes());
        data.extend_from_slice(self.state_root.as_bytes());
        data.extend_from_slice(&self.number.to_be_bytes());
        data.extend_from_slice(&self.timestamp.to_be_bytes());

        keccak256(&data)
    }
}

/// Full block with transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Block header
    pub header: BlockHeader,
    /// Transactions in the block
    pub transactions: Vec<SignedTransaction>,
}

impl Block {
    /// Get block hash
    pub fn hash(&self) -> H256 {
        self.header.hash()
    }

    /// Get block number
    pub fn number(&self) -> u64 {
        self.header.number
    }
}

/// Signed transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTransaction {
    /// Sender address
    pub from: Address,
    /// Recipient address (None for contract creation)
    pub to: Option<Address>,
    /// Value in wei
    pub value: U256,
    /// Input data (calldata or constructor code)
    pub data: Vec<u8>,
    /// Nonce
    pub nonce: u64,
    /// Gas limit
    pub gas_limit: u64,
    /// Max fee per gas
    pub max_fee_per_gas: u64,
    /// Max priority fee per gas
    pub max_priority_fee_per_gas: u64,
    /// Transaction hash
    pub hash: H256,
    /// Signature (v, r, s encoded)
    pub signature: Vec<u8>,
}

impl SignedTransaction {
    /// Compute transaction hash
    pub fn compute_hash(&self) -> H256 {
        use eth_primitives::keccak256;

        let mut data = Vec::new();
        data.extend_from_slice(self.from.as_bytes());
        if let Some(to) = &self.to {
            data.extend_from_slice(to.as_bytes());
        }
        data.extend_from_slice(&self.nonce.to_be_bytes());
        data.extend_from_slice(&self.data);

        keccak256(&data)
    }

    /// Check if this is a contract creation
    pub fn is_contract_creation(&self) -> bool {
        self.to.is_none()
    }

    /// Get effective gas price
    pub fn effective_gas_price(&self, base_fee: u64) -> u64 {
        let priority_fee = self.max_priority_fee_per_gas.min(
            self.max_fee_per_gas.saturating_sub(base_fee)
        );
        base_fee + priority_fee
    }
}

/// Transaction receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    /// Transaction hash
    pub tx_hash: H256,
    /// Block hash
    pub block_hash: H256,
    /// Block number
    pub block_number: u64,
    /// Transaction index in block
    pub tx_index: u64,
    /// Sender address
    pub from: Address,
    /// Recipient address
    pub to: Option<Address>,
    /// Contract address (if created)
    pub contract_address: Option<Address>,
    /// Gas used
    pub gas_used: u64,
    /// Cumulative gas used in block
    pub cumulative_gas_used: u64,
    /// Success status
    pub status: bool,
    /// Logs emitted
    pub logs: Vec<Log>,
    /// Return data
    pub return_data: Vec<u8>,
}

/// Event log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    /// Contract address that emitted the log
    pub address: Address,
    /// Indexed topics
    pub topics: Vec<H256>,
    /// Non-indexed data
    pub data: Vec<u8>,
}

/// Node peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer ID
    pub id: String,
    /// Peer address
    pub address: String,
    /// Current block number
    pub block_number: u64,
    /// Is connected
    pub connected: bool,
}

/// Sync status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    /// Is syncing
    pub syncing: bool,
    /// Starting block
    pub starting_block: u64,
    /// Current block
    pub current_block: u64,
    /// Highest known block
    pub highest_block: u64,
}

/// Chain ID for the network
pub const CHAIN_ID: u64 = 1337; // Local testnet
