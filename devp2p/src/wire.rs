//! # Wire Protocol Messages
//!
//! Ethereum wire protocol (eth/68) messages for block/tx synchronization.

use eth_primitives::{H256, U256, Address};
use std::fmt;

/// Ethereum wire protocol message types
#[derive(Debug, Clone)]
pub enum EthMessage {
    // Status exchange
    Status(StatusMessage),

    // Block headers
    GetBlockHeaders(GetBlockHeadersRequest),
    BlockHeaders(Vec<BlockHeader>),

    // Block bodies
    GetBlockBodies(Vec<H256>),
    BlockBodies(Vec<BlockBody>),

    // Transaction pool
    NewPooledTransactionHashes(Vec<H256>),
    GetPooledTransactions(Vec<H256>),
    PooledTransactions(Vec<Transaction>),

    // Block announcements
    NewBlockHashes(Vec<BlockHashNumber>),
    NewBlock(Box<NewBlockMessage>),

    // Receipts
    GetReceipts(Vec<H256>),
    Receipts(Vec<Vec<Receipt>>),
}

impl EthMessage {
    /// Get message ID within eth protocol
    pub fn message_id(&self) -> u8 {
        match self {
            EthMessage::Status(_) => 0x00,
            EthMessage::NewBlockHashes(_) => 0x01,
            EthMessage::GetBlockHeaders(_) => 0x03,
            EthMessage::BlockHeaders(_) => 0x04,
            EthMessage::GetBlockBodies(_) => 0x05,
            EthMessage::BlockBodies(_) => 0x06,
            EthMessage::NewBlock(_) => 0x07,
            EthMessage::NewPooledTransactionHashes(_) => 0x08,
            EthMessage::GetPooledTransactions(_) => 0x09,
            EthMessage::PooledTransactions(_) => 0x0a,
            EthMessage::GetReceipts(_) => 0x0f,
            EthMessage::Receipts(_) => 0x10,
        }
    }
}

/// Status message exchanged at connection start
#[derive(Debug, Clone)]
pub struct StatusMessage {
    /// Protocol version
    pub version: u32,
    /// Network ID (1 = mainnet, 5 = goerli, etc)
    pub network_id: u64,
    /// Total difficulty
    pub total_difficulty: U256,
    /// Best block hash
    pub best_hash: H256,
    /// Genesis hash
    pub genesis: H256,
    /// Fork ID (EIP-2124)
    pub fork_id: ForkId,
}

impl StatusMessage {
    /// Create status for mainnet
    pub fn mainnet(best_hash: H256, total_difficulty: U256) -> Self {
        StatusMessage {
            version: 68,
            network_id: 1,
            total_difficulty,
            best_hash,
            genesis: H256::new([
                0xd4, 0xe5, 0x67, 0x40, 0xf8, 0x76, 0xae, 0xf8,
                0xc0, 0x10, 0xb8, 0x6a, 0x40, 0xd5, 0xf5, 0x67,
                0x45, 0xa1, 0x18, 0xd0, 0x90, 0x6a, 0x34, 0xe6,
                0x9a, 0xec, 0x8c, 0x0d, 0xb1, 0xcb, 0x8f, 0xa3,
            ]),
            fork_id: ForkId::default(),
        }
    }
}

/// Fork ID (EIP-2124)
#[derive(Debug, Clone, Default)]
pub struct ForkId {
    /// CRC32 checksum of genesis and fork hashes
    pub hash: [u8; 4],
    /// Next fork block number (0 if no scheduled forks)
    pub next: u64,
}

/// Block hash and number
#[derive(Debug, Clone)]
pub struct BlockHashNumber {
    pub hash: H256,
    pub number: u64,
}

/// Block header
#[derive(Debug, Clone)]
pub struct BlockHeader {
    /// Hash of the parent block - links blocks into a chain
    pub parent_hash: H256,
    /// Hash of the list of uncle/ommer block headers
    pub uncle_hash: H256,
    /// Address that receives block reward (miner pre-merge, fee recipient post-merge)
    pub coinbase: Address,
    /// Root hash of the state trie after executing this block
    pub state_root: H256,
    /// Root hash of the transactions trie for this block
    pub tx_root: H256,
    /// Root hash of the receipts trie for this block
    pub receipt_root: H256,
    /// Bloom filter for quick log searching (2048 bits)
    pub logs_bloom: [u8; 256],
    /// Block difficulty for PoW (always 0 post-merge)
    pub difficulty: U256,
    /// Block number (height in the chain, 0 = genesis)
    pub number: u64,
    /// Maximum gas allowed in this block
    pub gas_limit: u64,
    /// Total gas used by all transactions in this block
    pub gas_used: u64,
    /// Unix timestamp when block was created
    pub timestamp: u64,
    /// Arbitrary data field (max 32 bytes, often miner/client identifier)
    pub extra_data: Vec<u8>,
    /// PoW: hash mixed with nonce to prove work. PoS: prev RANDAO value
    pub mix_hash: H256,
    /// PoW nonce that satisfies difficulty (always 0 post-merge)
    pub nonce: [u8; 8],
    // Post-merge fields (EIP-1559+)
    /// Base fee per gas for this block (EIP-1559, London fork)
    pub base_fee: Option<u64>,
    /// Root hash of withdrawals trie (EIP-4895, Shanghai fork)
    pub withdrawals_root: Option<H256>,
    /// Total blob gas used in this block (EIP-4844, Dencun fork)
    pub blob_gas_used: Option<u64>,
    /// Running total of excess blob gas (EIP-4844, Dencun fork)
    pub excess_blob_gas: Option<u64>,
}

impl BlockHeader {
    /// Compute block hash
    pub fn hash(&self) -> H256 {
        // Simplified - real impl would RLP encode then hash
        use eth_primitives::keccak256;

        let mut data = Vec::new();
        data.extend_from_slice(self.parent_hash.as_bytes());
        data.extend_from_slice(&self.number.to_be_bytes());
        data.extend_from_slice(&self.timestamp.to_be_bytes());

        keccak256(&data)
    }
}

/// Block body (transactions + uncles)
#[derive(Debug, Clone)]
pub struct BlockBody {
    pub transactions: Vec<Transaction>,
    pub uncles: Vec<BlockHeader>,
    pub withdrawals: Option<Vec<Withdrawal>>,
}

/// Transaction (simplified)
#[derive(Debug, Clone)]
pub struct Transaction {
    /// Keccak256 hash of the RLP-encoded transaction (unique identifier)
    pub hash: H256,
    /// Sender's transaction count - prevents replay and orders txs from same sender
    pub nonce: u64,
    /// Price in wei per unit of gas (legacy tx) or max fee (EIP-1559)
    pub gas_price: u64,
    /// Maximum gas units this transaction can consume
    pub gas_limit: u64,
    /// Recipient address. None = contract creation
    pub to: Option<Address>,
    /// Amount of ETH (in wei) to transfer
    pub value: U256,
    /// Input data: constructor bytecode (creation) or function calldata (call)
    pub data: Vec<u8>,
    /// ECDSA recovery id + chain id (EIP-155 replay protection)
    pub v: u64,
    /// ECDSA signature r value (first 32 bytes of signature)
    pub r: U256,
    /// ECDSA signature s value (second 32 bytes of signature)
    pub s: U256,
}

/// Transaction receipt
#[derive(Debug, Clone)]
pub struct Receipt {
    pub status: bool,
    pub cumulative_gas_used: u64,
    pub logs_bloom: [u8; 256],
    pub logs: Vec<Log>,
}

/// Log entry
#[derive(Debug, Clone)]
pub struct Log {
    pub address: Address,
    pub topics: Vec<H256>,
    pub data: Vec<u8>,
}

/// Withdrawal (post-Shanghai)
#[derive(Debug, Clone)]
pub struct Withdrawal {
    pub index: u64,
    pub validator_index: u64,
    pub address: Address,
    pub amount: u64,
}

/// Get block headers request
#[derive(Debug, Clone)]
pub struct GetBlockHeadersRequest {
    /// Request ID
    pub request_id: u64,
    /// Start block (hash or number)
    pub start: BlockId,
    /// Maximum headers to return
    pub limit: u64,
    /// Skip headers
    pub skip: u64,
    /// Reverse order
    pub reverse: bool,
}

/// Block identifier
#[derive(Debug, Clone)]
pub enum BlockId {
    Hash(H256),
    Number(u64),
}

/// New block message
#[derive(Debug, Clone)]
pub struct NewBlockMessage {
    pub header: BlockHeader,
    pub body: BlockBody,
    pub total_difficulty: U256,
}

/// Generic wire message wrapper
#[derive(Debug, Clone)]
pub enum Message {
    /// RLPx base protocol
    Hello(crate::rlpx::Hello),
    Disconnect(crate::rlpx::DisconnectReason),
    Ping,
    Pong,
    /// Ethereum wire protocol
    Eth(EthMessage),
}

impl Message {
    /// Get message code (including subprotocol offset)
    pub fn code(&self, eth_offset: u8) -> u8 {
        match self {
            Message::Hello(_) => 0x00,
            Message::Disconnect(_) => 0x01,
            Message::Ping => 0x02,
            Message::Pong => 0x03,
            Message::Eth(eth) => eth_offset + eth.message_id(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_message() {
        let status = StatusMessage::mainnet(
            H256::default(),
            U256::from_u64(0),
        );

        assert_eq!(status.version, 68);
        assert_eq!(status.network_id, 1);
    }

    #[test]
    fn test_eth_message_ids() {
        let status = EthMessage::Status(StatusMessage::mainnet(
            H256::default(),
            U256::from_u64(0),
        ));
        assert_eq!(status.message_id(), 0x00);

        let hashes = EthMessage::NewBlockHashes(vec![]);
        assert_eq!(hashes.message_id(), 0x01);
    }

    #[test]
    fn test_block_id() {
        let by_hash = BlockId::Hash(H256::default());
        let by_number = BlockId::Number(12345);

        match by_hash {
            BlockId::Hash(_) => {}
            _ => panic!("Expected hash"),
        }

        match by_number {
            BlockId::Number(n) => assert_eq!(n, 12345),
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_get_headers_request() {
        let req = GetBlockHeadersRequest {
            request_id: 1,
            start: BlockId::Number(1000),
            limit: 100,
            skip: 0,
            reverse: false,
        };

        assert_eq!(req.request_id, 1);
        assert_eq!(req.limit, 100);
    }

    #[test]
    fn test_message_code() {
        let ping = Message::Ping;
        assert_eq!(ping.code(0x10), 0x02);

        let eth = Message::Eth(EthMessage::Status(StatusMessage::mainnet(
            H256::default(),
            U256::from_u64(0),
        )));
        // eth/status is 0x00, so with offset 0x10 it's 0x10
        assert_eq!(eth.code(0x10), 0x10);
    }
}
