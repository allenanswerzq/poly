//! Full Node implementation
//!
//! Orchestrates all components: state, txpool, executor, consensus, network, rpc

use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use eth_primitives::{Address, H256, U256};
use async_trait::async_trait;

use crate::config::NodeConfig;
use crate::genesis::GenesisConfig;
use crate::state::WorldState;
use crate::txpool::TransactionPool;
use crate::executor::Executor;
use crate::consensus::Consensus;
use crate::network::{Network, NetworkConfig, NetworkMessage};
use crate::rpc::{RpcServer, RpcHandler, TransactionCall};
use crate::types::{Block, BlockHeader, SignedTransaction, Receipt, SyncStatus, LogsBloom};
use crate::error::{MiniEthError, Result};

/// Node status
#[derive(Debug, Clone, PartialEq)]
pub enum NodeStatus {
    /// Node is starting
    Starting,
    /// Node is syncing
    Syncing,
    /// Node is running normally
    Running,
    /// Node is stopping
    Stopping,
    /// Node is stopped
    Stopped,
}

/// Full Ethereum node
pub struct Node {
    /// Node configuration
    config: NodeConfig,

    /// World state
    state: Arc<RwLock<WorldState>>,

    /// Transaction pool
    txpool: Arc<RwLock<TransactionPool>>,

    /// Block executor
    executor: Arc<Executor>,

    /// Consensus engine
    consensus: Arc<RwLock<Consensus>>,

    /// P2P Network
    network: Arc<RwLock<Network>>,

    /// Chain (block storage)
    chain: Arc<RwLock<Chain>>,

    /// Node status
    status: Arc<RwLock<NodeStatus>>,

    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
}

/// Block chain storage
pub struct Chain {
    /// Blocks by hash
    blocks: HashMap<H256, Block>,

    /// Block hash by number
    block_numbers: HashMap<u64, H256>,

    /// Transaction receipts
    receipts: HashMap<H256, Receipt>,

    /// Transaction locations (block hash, index)
    tx_locations: HashMap<H256, (H256, usize)>,

    /// Current head
    head: Option<H256>,

    /// Chain ID
    chain_id: u64,

    /// Genesis hash
    genesis_hash: H256,
}

impl Chain {
    /// Create a new chain
    pub fn new(chain_id: u64) -> Self {
        Chain {
            blocks: HashMap::new(),
            block_numbers: HashMap::new(),
            receipts: HashMap::new(),
            tx_locations: HashMap::new(),
            head: None,
            chain_id,
            genesis_hash: H256::zero(),
        }
    }

    /// Initialize with genesis block
    pub fn init_genesis(&mut self, genesis: Block) {
        let hash = genesis.hash();
        self.genesis_hash = hash;
        self.blocks.insert(hash, genesis);
        self.block_numbers.insert(0, hash);
        self.head = Some(hash);
    }

    /// Insert a block
    pub fn insert_block(&mut self, block: Block, receipts: Vec<Receipt>) -> Result<()> {
        let hash = block.hash();
        let number = block.number();

        // Verify parent exists
        if number > 0 {
            let parent_hash = block.header.parent_hash;
            if !self.blocks.contains_key(&parent_hash) {
                return Err(MiniEthError::Consensus("Parent block not found".into()));
            }
        }

        // Store transactions locations
        for (i, tx) in block.transactions.iter().enumerate() {
            self.tx_locations.insert(tx.hash, (hash, i));
        }

        // Store receipts
        for receipt in receipts {
            self.receipts.insert(receipt.tx_hash, receipt);
        }

        // Store block
        self.blocks.insert(hash, block);
        self.block_numbers.insert(number, hash);
        self.head = Some(hash);

        Ok(())
    }

    /// Get block by hash
    pub fn get_block(&self, hash: H256) -> Option<&Block> {
        self.blocks.get(&hash)
    }

    /// Get block by number
    pub fn get_block_by_number(&self, number: u64) -> Option<&Block> {
        self.block_numbers.get(&number).and_then(|h| self.blocks.get(h))
    }

    /// Get head block
    pub fn head_block(&self) -> Option<&Block> {
        self.head.and_then(|h| self.blocks.get(&h))
    }

    /// Get head block number
    pub fn head_number(&self) -> u64 {
        self.head_block().map(|b| b.number()).unwrap_or(0)
    }

    /// Get transaction by hash
    pub fn get_transaction(&self, hash: H256) -> Option<&SignedTransaction> {
        self.tx_locations.get(&hash).and_then(|(block_hash, index)| {
            self.blocks.get(block_hash).and_then(|b| b.transactions.get(*index))
        })
    }

    /// Get receipt by transaction hash
    pub fn get_receipt(&self, hash: H256) -> Option<&Receipt> {
        self.receipts.get(&hash)
    }
}

impl Node {
    /// Create a new node
    pub fn new(config: NodeConfig) -> Self {
        let chain_id = config.chain_id;
        let state = Arc::new(RwLock::new(WorldState::new()));
        let txpool = Arc::new(RwLock::new(TransactionPool::new(Default::default())));
        let executor = Arc::new(Executor::new(chain_id));
        let consensus = Arc::new(RwLock::new(Consensus::new(Default::default())));

        let network = Arc::new(RwLock::new(Network::new(NetworkConfig {
            listen_addr: config.network.listen_addr.clone(),
            listen_port: config.network.port,
            bootnodes: config.network.bootnodes.clone(),
            max_peers: config.network.max_peers,
            node_id: format!("{}-{}", config.name, rand::random::<u32>()),
        })));

        let chain = Arc::new(RwLock::new(Chain::new(chain_id)));

        Node {
            config,
            state,
            txpool,
            executor,
            consensus,
            network,
            chain,
            status: Arc::new(RwLock::new(NodeStatus::Stopped)),
            shutdown_tx: None,
        }
    }

    /// Initialize the node
    pub async fn init(&mut self) -> Result<()> {
        *self.status.write() = NodeStatus::Starting;

        tracing::info!("Initializing node: {}", self.config.name);

        // Ensure data directory exists
        self.config.ensure_data_dir()
            .map_err(|e| MiniEthError::State(e.to_string()))?;

        // Initialize genesis
        if let Some(ref genesis_config) = self.config.genesis {
            self.init_genesis(genesis_config.clone())?;
        }

        // Set up validators if mining is enabled
        if self.config.mining.enabled {
            let coinbase = self.config.mining.coinbase;
            self.consensus.write().add_validator(coinbase);
            tracing::info!("Mining enabled for address: {:?}", coinbase);
        }

        Ok(())
    }

    /// Initialize from genesis
    fn init_genesis(&mut self, genesis_config: GenesisConfig) -> Result<()> {
        // Initialize state
        genesis_config.init_state(&mut self.state.write())?;

        // Create genesis block
        let genesis_block = genesis_config.genesis_block();
        self.chain.write().init_genesis(genesis_block.clone());

        // Add validators from genesis
        for validator in &genesis_config.validators {
            self.consensus.write().add_validator(*validator);
        }

        tracing::info!(
            "Genesis initialized: block 0, hash {:?}",
            genesis_block.hash()
        );

        Ok(())
    }

    /// Start the node
    pub async fn start(&mut self) -> Result<()> {
        if *self.status.read() == NodeStatus::Running {
            return Ok(());
        }

        tracing::info!("Starting node: {}", self.config.name);

        // Initialize if not already done
        if *self.status.read() == NodeStatus::Stopped {
            self.init().await?;
        }

        // Start network
        if self.config.network.enabled {
            self.network.write().start().await?;
        }

        // Start block production if mining
        if self.config.mining.enabled {
            self.start_block_production().await?;
        }

        *self.status.write() = NodeStatus::Running;
        tracing::info!("Node started: {}", self.config.name);

        Ok(())
    }

    /// Start block production loop
    async fn start_block_production(&self) -> Result<()> {
        let interval = self.config.mining.block_interval;
        let coinbase = self.config.mining.coinbase;
        let gas_limit = self.config.mining.gas_limit;

        let state = Arc::clone(&self.state);
        let txpool = Arc::clone(&self.txpool);
        let consensus = Arc::clone(&self.consensus);
        let chain = Arc::clone(&self.chain);
        let network = Arc::clone(&self.network);
        let executor = Arc::clone(&self.executor);

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(
                tokio::time::Duration::from_secs(interval)
            );

            loop {
                interval_timer.tick().await;

                // Get pending transactions
                let pending_txs: Vec<SignedTransaction> = txpool
                    .read()
                    .get_pending(100);

                if pending_txs.is_empty() {
                    continue;
                }

                // Get parent block
                let parent = chain.read().head_block().cloned();
                let parent = match parent {
                    Some(p) => p,
                    None => continue,
                };

                // Create block
                let mut state_guard = state.write();
                let result = consensus.write().create_block(
                    &parent,
                    pending_txs.clone(),
                    coinbase,
                    &mut *state_guard,
                );

                match result {
                    Ok(block) => {
                        let block_number = block.number();
                        let block_hash = block.hash();
                        let tx_count = block.transactions.len();

                        // Insert block (no receipts returned from create_block)
                        if let Err(e) = chain.write().insert_block(block.clone(), vec![]) {
                            tracing::error!("Failed to insert block: {}", e);
                            continue;
                        }

                        // Remove included transactions from pool
                        for tx in &pending_txs {
                            txpool.write().remove_tx(&tx.hash);
                        }

                        // Broadcast block
                        network.read().broadcast_block(block);

                        tracing::info!(
                            "Produced block #{} ({:?}) with {} txs",
                            block_number,
                            block_hash,
                            tx_count
                        );
                    }
                    Err(e) => {
                        tracing::error!("Failed to create block: {}", e);
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop the node
    pub async fn stop(&mut self) -> Result<()> {
        *self.status.write() = NodeStatus::Stopping;
        tracing::info!("Stopping node: {}", self.config.name);

        // Stop network
        self.network.write().stop().await;

        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }

        *self.status.write() = NodeStatus::Stopped;
        tracing::info!("Node stopped: {}", self.config.name);

        Ok(())
    }

    /// Submit a transaction
    pub fn submit_transaction(&self, tx: SignedTransaction) -> Result<H256> {
        let hash = tx.hash;

        // Validate transaction
        self.validate_transaction(&tx)?;

        // Add to pool
        self.txpool.write().add_tx(tx.clone(), &self.state.read(), false)?;

        // Broadcast to network
        self.network.read().broadcast_tx(tx);

        tracing::debug!("Transaction submitted: {:?}", hash);

        Ok(hash)
    }

    /// Validate a transaction
    fn validate_transaction(&self, tx: &SignedTransaction) -> Result<()> {
        let state = self.state.read();

        // Check sender balance
        let balance = state.get_balance(tx.from)?;
        let cost = tx.value + U256::from(tx.gas_limit) * U256::from(tx.max_fee_per_gas);

        if balance < cost {
            return Err(MiniEthError::InsufficientBalance {
                required: cost,
                available: balance,
            });
        }

        // Check nonce
        let expected_nonce = state.get_nonce(tx.from)?;
        if tx.nonce < expected_nonce {
            return Err(MiniEthError::NonceTooLow {
                expected: expected_nonce,
                got: tx.nonce,
            });
        }

        Ok(())
    }

    /// Get node status
    pub fn status(&self) -> NodeStatus {
        self.status.read().clone()
    }

    /// Get chain ID
    pub fn chain_id(&self) -> u64 {
        self.config.chain_id
    }

    /// Get current block number
    pub fn block_number(&self) -> u64 {
        self.chain.read().head_number()
    }

    /// Get block by number
    pub fn get_block_by_number(&self, number: u64) -> Option<Block> {
        self.chain.read().get_block_by_number(number).cloned()
    }

    /// Get block by hash
    pub fn get_block_by_hash(&self, hash: H256) -> Option<Block> {
        self.chain.read().get_block(hash).cloned()
    }

    /// Get balance
    pub fn get_balance(&self, address: Address) -> Result<U256> {
        self.state.read().get_balance(address)
    }

    /// Get nonce
    pub fn get_nonce(&self, address: Address) -> Result<u64> {
        self.state.read().get_nonce(address)
    }

    /// Get code
    pub fn get_code(&self, address: Address) -> Result<Vec<u8>> {
        self.state.read().get_code(address)
    }

    /// Get storage
    pub fn get_storage(&self, address: Address, key: U256) -> Result<H256> {
        self.state.read().get_storage(address, key)
    }

    /// Call (simulate transaction)
    pub fn call(&self, from: Option<Address>, to: Option<Address>, value: U256, data: Vec<u8>) -> Result<Vec<u8>> {
        let from = from.unwrap_or(Address::zero());
        let mut state = self.state.read().clone();

        self.executor.call(from, to, value, data, &mut state)
    }

    /// Get transaction receipt
    pub fn get_receipt(&self, hash: H256) -> Option<Receipt> {
        self.chain.read().get_receipt(hash).cloned()
    }

    /// Get transaction by hash
    pub fn get_transaction(&self, hash: H256) -> Option<SignedTransaction> {
        self.chain.read().get_transaction(hash).cloned()
    }

    /// Get sync status
    pub fn sync_status(&self) -> SyncStatus {
        let current = self.block_number();
        self.network.read().sync_status(current)
    }

    /// Get peer count
    pub fn peer_count(&self) -> usize {
        self.network.read().peer_count()
    }

    /// Connect to peer
    pub async fn connect_peer(&self, address: String) -> Result<()> {
        self.network.read().connect(address).await
    }
}

/// RPC handler implementation for Node
#[async_trait]
impl RpcHandler for Node {
    fn eth_chain_id(&self) -> Result<u64> {
        Ok(self.chain_id())
    }

    fn eth_block_number(&self) -> Result<u64> {
        Ok(self.block_number())
    }

    fn eth_get_balance(&self, address: Address, _block: Option<String>) -> Result<U256> {
        self.get_balance(address)
    }

    fn eth_get_transaction_count(&self, address: Address, _block: Option<String>) -> Result<u64> {
        self.get_nonce(address)
    }

    fn eth_get_code(&self, address: Address, _block: Option<String>) -> Result<Vec<u8>> {
        self.get_code(address)
    }

    fn eth_get_storage_at(&self, address: Address, position: U256, _block: Option<String>) -> Result<H256> {
        self.get_storage(address, position)
    }

    async fn eth_send_raw_transaction(&self, _raw_tx: Vec<u8>) -> Result<H256> {
        // In real impl, would decode and submit
        Err(MiniEthError::Rpc("Raw tx decoding not implemented".into()))
    }

    fn eth_call(&self, tx: TransactionCall, _block: Option<String>) -> Result<Vec<u8>> {
        self.call(tx.from, tx.to, tx.value.unwrap_or_default(), tx.data.unwrap_or_default())
    }

    fn eth_estimate_gas(&self, tx: TransactionCall) -> Result<u64> {
        // Simple estimation - in real impl would trace execution
        let base = 21000u64;
        let data_cost = tx.data.as_ref().map(|d| {
            d.iter().map(|b| if *b == 0 { 4u64 } else { 16u64 }).sum::<u64>()
        }).unwrap_or(0);
        Ok(base + data_cost)
    }

    fn eth_get_block_by_number(&self, number: u64, _full_txs: bool) -> Result<Option<Block>> {
        Ok(self.get_block_by_number(number))
    }

    fn eth_get_block_by_hash(&self, hash: H256, _full_txs: bool) -> Result<Option<Block>> {
        Ok(self.get_block_by_hash(hash))
    }

    fn eth_get_transaction_by_hash(&self, hash: H256) -> Result<Option<SignedTransaction>> {
        Ok(self.get_transaction(hash))
    }

    fn eth_get_transaction_receipt(&self, hash: H256) -> Result<Option<Receipt>> {
        Ok(self.get_receipt(hash))
    }

    fn eth_gas_price(&self) -> Result<u64> {
        // Return base fee from latest block
        let chain = self.chain.read();
        Ok(chain.head_block().map(|b| b.header.base_fee).unwrap_or(1_000_000_000))
    }

    fn eth_syncing(&self) -> Result<SyncStatus> {
        Ok(self.sync_status())
    }

    fn net_version(&self) -> Result<String> {
        Ok(self.chain_id().to_string())
    }

    fn net_peer_count(&self) -> Result<usize> {
        Ok(self.peer_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_node_creation() {
        let config = NodeConfig::dev();
        let node = Node::new(config);

        assert_eq!(node.status(), NodeStatus::Stopped);
    }

    #[tokio::test]
    async fn test_node_init() {
        let config = NodeConfig::dev();
        let mut node = Node::new(config);

        node.init().await.unwrap();

        assert_eq!(node.block_number(), 0);
    }

    #[test]
    fn test_chain() {
        let mut chain = Chain::new(1337);

        let genesis = Block {
            header: BlockHeader {
                parent_hash: H256::zero(),
                beneficiary: Address::zero(),
                state_root: H256::zero(),
                transactions_root: H256::zero(),
                receipts_root: H256::zero(),
                logs_bloom: LogsBloom::default(),
                number: 0,
                gas_limit: 30_000_000,
                gas_used: 0,
                timestamp: 0,
                extra_data: vec![],
                base_fee: 1_000_000_000,
            },
            transactions: vec![],
        };

        chain.init_genesis(genesis);

        assert_eq!(chain.head_number(), 0);
        assert!(chain.head_block().is_some());
    }
}
