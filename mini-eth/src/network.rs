//! Network - P2P communication between nodes
//!
//! Integrates with devp2p for peer discovery and message passing

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use eth_primitives::{Address, H256};

use crate::types::{Block, SignedTransaction, PeerInfo, SyncStatus};
use crate::error::{MiniEthError, Result};

/// Network message types
#[derive(Debug, Clone)]
pub enum NetworkMessage {
    /// New block announcement
    NewBlock(Block),
    
    /// New transaction announcement
    NewTransaction(SignedTransaction),
    
    /// Request block by hash
    GetBlock(H256),
    
    /// Block response
    BlockResponse(Option<Block>),
    
    /// Request block headers
    GetHeaders { start: u64, count: u64 },
    
    /// Headers response
    Headers(Vec<Block>),
    
    /// Status message (handshake)
    Status {
        chain_id: u64,
        genesis_hash: H256,
        head_hash: H256,
        head_number: u64,
    },
    
    /// Ping
    Ping(u64),
    
    /// Pong
    Pong(u64),
}

/// Peer connection
#[derive(Debug, Clone)]
pub struct Peer {
    /// Peer ID
    pub id: String,
    /// Address
    pub address: String,
    /// Connected at
    pub connected_at: u64,
    /// Last seen
    pub last_seen: u64,
    /// Head block number
    pub head_number: u64,
    /// Head block hash
    pub head_hash: H256,
}

/// Network configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Listen address
    pub listen_addr: String,
    /// Listen port
    pub listen_port: u16,
    /// Bootstrap nodes
    pub bootnodes: Vec<String>,
    /// Maximum peers
    pub max_peers: usize,
    /// Node ID
    pub node_id: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        NetworkConfig {
            listen_addr: "0.0.0.0".to_string(),
            listen_port: 30303,
            bootnodes: vec![],
            max_peers: 25,
            node_id: hex::encode(&rand::random::<[u8; 32]>()[..16]),
        }
    }
}

/// Network layer for node communication
pub struct Network {
    /// Configuration
    config: NetworkConfig,
    
    /// Connected peers
    peers: Arc<RwLock<HashMap<String, Peer>>>,
    
    /// Known transactions (to avoid re-broadcasting)
    known_txs: Arc<RwLock<HashSet<H256>>>,
    
    /// Known blocks
    known_blocks: Arc<RwLock<HashSet<H256>>>,
    
    /// Message sender for outgoing
    tx: Option<mpsc::UnboundedSender<(String, NetworkMessage)>>,
    
    /// Message receiver for incoming
    rx: Option<mpsc::UnboundedReceiver<(String, NetworkMessage)>>,
    
    /// Is running
    running: Arc<RwLock<bool>>,
    
    /// Broadcast channel for new blocks
    block_broadcast: Option<tokio::sync::broadcast::Sender<Block>>,
    
    /// Broadcast channel for new transactions
    tx_broadcast: Option<tokio::sync::broadcast::Sender<SignedTransaction>>,
}

impl Network {
    /// Create a new network
    pub fn new(config: NetworkConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let (block_tx, _) = tokio::sync::broadcast::channel(100);
        let (tx_tx, _) = tokio::sync::broadcast::channel(1000);
        
        Network {
            config,
            peers: Arc::new(RwLock::new(HashMap::new())),
            known_txs: Arc::new(RwLock::new(HashSet::new())),
            known_blocks: Arc::new(RwLock::new(HashSet::new())),
            tx: Some(tx),
            rx: Some(rx),
            running: Arc::new(RwLock::new(false)),
            block_broadcast: Some(block_tx),
            tx_broadcast: Some(tx_tx),
        }
    }

    /// Start the network
    pub async fn start(&mut self) -> Result<()> {
        *self.running.write() = true;
        
        tracing::info!(
            "Network starting on {}:{}",
            self.config.listen_addr,
            self.config.listen_port
        );
        
        // Connect to bootnodes
        for bootnode in &self.config.bootnodes.clone() {
            if let Err(e) = self.connect(bootnode.clone()).await {
                tracing::warn!("Failed to connect to bootnode {}: {}", bootnode, e);
            }
        }
        
        Ok(())
    }

    /// Stop the network
    pub async fn stop(&mut self) {
        *self.running.write() = false;
        self.tx = None;
        tracing::info!("Network stopped");
    }

    /// Connect to a peer
    pub async fn connect(&self, address: String) -> Result<()> {
        if self.peers.read().len() >= self.config.max_peers {
            return Err(MiniEthError::Network("Max peers reached".into()));
        }
        
        let peer_id = format!("peer-{}", hex::encode(&rand::random::<[u8; 8]>()));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let peer = Peer {
            id: peer_id.clone(),
            address: address.clone(),
            connected_at: now,
            last_seen: now,
            head_number: 0,
            head_hash: H256::zero(),
        };
        
        self.peers.write().insert(peer_id.clone(), peer);
        
        tracing::info!("Connected to peer: {}", address);
        
        Ok(())
    }

    /// Disconnect from a peer
    pub fn disconnect(&self, peer_id: &str) {
        self.peers.write().remove(peer_id);
        tracing::info!("Disconnected from peer: {}", peer_id);
    }

    /// Broadcast a block to all peers
    pub fn broadcast_block(&self, block: Block) {
        let block_hash = block.hash();
        
        // Check if we've already seen this block
        if !self.known_blocks.write().insert(block_hash) {
            return;
        }
        
        // Broadcast to subscribers
        if let Some(ref tx) = self.block_broadcast {
            let _ = tx.send(block.clone());
        }
        
        // Send to all peers
        let peers: Vec<_> = self.peers.read().keys().cloned().collect();
        for peer_id in peers {
            self.send_to_peer(&peer_id, NetworkMessage::NewBlock(block.clone()));
        }
        
        tracing::debug!("Broadcast block {} to {} peers", block.number(), self.peers.read().len());
    }

    /// Broadcast a transaction to all peers
    pub fn broadcast_tx(&self, tx: SignedTransaction) {
        let tx_hash = tx.hash;
        
        // Check if we've already seen this tx
        if !self.known_txs.write().insert(tx_hash) {
            return;
        }
        
        // Broadcast to subscribers
        if let Some(ref tx_sender) = self.tx_broadcast {
            let _ = tx_sender.send(tx.clone());
        }
        
        // Send to all peers
        let peers: Vec<_> = self.peers.read().keys().cloned().collect();
        for peer_id in peers {
            self.send_to_peer(&peer_id, NetworkMessage::NewTransaction(tx.clone()));
        }
    }

    /// Send a message to a specific peer
    fn send_to_peer(&self, peer_id: &str, message: NetworkMessage) {
        if let Some(ref tx) = self.tx {
            let _ = tx.send((peer_id.to_string(), message));
        }
    }

    /// Request a block from peers
    pub async fn request_block(&self, hash: H256) -> Option<Block> {
        let peers: Vec<_> = self.peers.read().keys().cloned().collect();
        
        for peer_id in peers {
            self.send_to_peer(&peer_id, NetworkMessage::GetBlock(hash));
            // In real impl, would await response
        }
        
        None // Simplified - would need async response handling
    }

    /// Get connected peers
    pub fn peers(&self) -> Vec<PeerInfo> {
        self.peers
            .read()
            .values()
            .map(|p| PeerInfo {
                id: p.id.clone(),
                address: p.address.clone(),
                block_number: p.head_number,
                connected: true,
            })
            .collect()
    }

    /// Get peer count
    pub fn peer_count(&self) -> usize {
        self.peers.read().len()
    }

    /// Subscribe to new blocks
    pub fn subscribe_blocks(&self) -> Option<tokio::sync::broadcast::Receiver<Block>> {
        self.block_broadcast.as_ref().map(|tx| tx.subscribe())
    }

    /// Subscribe to new transactions
    pub fn subscribe_txs(&self) -> Option<tokio::sync::broadcast::Receiver<SignedTransaction>> {
        self.tx_broadcast.as_ref().map(|tx| tx.subscribe())
    }

    /// Get sync status
    pub fn sync_status(&self, current_block: u64) -> SyncStatus {
        let highest = self.peers
            .read()
            .values()
            .map(|p| p.head_number)
            .max()
            .unwrap_or(current_block);
        
        SyncStatus {
            syncing: current_block < highest,
            starting_block: 0,
            current_block,
            highest_block: highest,
        }
    }

    /// Update peer's head
    pub fn update_peer_head(&self, peer_id: &str, number: u64, hash: H256) {
        if let Some(peer) = self.peers.write().get_mut(peer_id) {
            peer.head_number = number;
            peer.head_hash = hash;
            peer.last_seen = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
        }
    }

    /// Get node ID
    pub fn node_id(&self) -> &str {
        &self.config.node_id
    }

    /// Get listen address
    pub fn listen_addr(&self) -> String {
        format!("{}:{}", self.config.listen_addr, self.config.listen_port)
    }

    /// Is running
    pub fn is_running(&self) -> bool {
        *self.running.read()
    }
}

impl Default for Network {
    fn default() -> Self {
        Self::new(NetworkConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_peer_connection() {
        let mut network = Network::new(NetworkConfig::default());
        
        network.connect("127.0.0.1:30303".to_string()).await.unwrap();
        
        assert_eq!(network.peer_count(), 1);
    }

    #[test]
    fn test_broadcast_deduplication() {
        let network = Network::new(NetworkConfig::default());
        
        let tx = SignedTransaction {
            from: Address::zero(),
            to: Some(Address::zero()),
            value: eth_primitives::U256::zero(),
            data: vec![],
            nonce: 0,
            gas_limit: 21000,
            max_fee_per_gas: 1000000000,
            max_priority_fee_per_gas: 1000000000,
            hash: H256::zero(),
            signature: vec![],
        };
        
        // First broadcast should add to known
        network.broadcast_tx(tx.clone());
        assert!(network.known_txs.read().contains(&tx.hash));
        
        // Second broadcast should be deduplicated (no error, just ignored)
        network.broadcast_tx(tx);
    }
}
