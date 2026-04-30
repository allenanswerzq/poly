//! Network Simulation Module
//!
//! This module simulates the P2P network layer of Ethereum, including:
//! - Message passing between nodes (builders, proposers, relays)
//! - Network latency simulation
//! - Gossip protocol basics
//! - Network partitions and faults
//!
//! In real Ethereum, the network layer uses devp2p (RLPx) for block/tx gossip
//! and libp2p for consensus messages. This simulation abstracts those details
//! while maintaining the essential behaviors.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};
use crate::chain::Block;

/// Unique identifier for network nodes
pub type NodeId = u64;

/// Types of messages that can be sent over the network
#[derive(Debug, Clone)]
pub enum NetworkMessage {
    /// A new block announcement
    NewBlock(Block),

    /// Request for a specific block by hash
    GetBlock { hash: [u8; 32] },

    /// Response with requested block
    BlockResponse { block: Option<Block> },

    /// New transaction announcement
    NewTransaction { hash: [u8; 32], data: Vec<u8> },

    /// Builder submitting block to relay
    BuilderSubmission {
        slot: u64,
        block: Block,
        bid: u128,
    },

    /// Proposer requesting best header from relay
    GetHeader { slot: u64, proposer: NodeId },

    /// Relay's response with blinded header
    HeaderResponse {
        slot: u64,
        header_hash: [u8; 32],
        value: u128,
    },

    /// Proposer committing to a header
    SignedCommitment {
        slot: u64,
        header_hash: [u8; 32],
        signature: [u8; 64],
    },

    /// Relay revealing full block after commitment
    BlockReveal { block: Block },

    /// Heartbeat/keepalive message
    Ping { nonce: u64 },

    /// Response to heartbeat
    Pong { nonce: u64 },
}

/// An envelope containing a message and routing info
#[derive(Debug, Clone)]
pub struct MessageEnvelope {
    /// Unique message ID
    pub id: u64,

    /// Sender node
    pub from: NodeId,

    /// Recipient node (None = broadcast)
    pub to: Option<NodeId>,

    /// The actual message
    pub message: NetworkMessage,

    /// When the message was sent
    pub sent_at: Instant,

    /// When the message should be delivered (after latency)
    pub deliver_at: Instant,

    /// Number of hops (for gossip)
    pub hops: u32,
}

/// Network node types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeType {
    /// Full node (verifies everything)
    FullNode,

    /// Light client (header verification only)
    LightClient,

    /// Block builder
    Builder,

    /// MEV relay
    Relay,

    /// Validator/Proposer
    Validator,

    /// Bootnode for discovery
    Bootnode,
}

/// Represents a node in the network
#[derive(Debug, Clone)]
pub struct NetworkNode {
    /// Unique identifier
    pub id: NodeId,

    /// Node type
    pub node_type: NodeType,

    /// Connected peers
    pub peers: HashSet<NodeId>,

    /// Maximum peer connections
    pub max_peers: usize,

    /// Is the node online?
    pub online: bool,

    /// Simulated latency to this node (milliseconds)
    pub latency_ms: u32,

    /// Inbox of pending messages
    pub inbox: VecDeque<MessageEnvelope>,

    /// Messages seen (for deduplication)
    pub seen_messages: HashSet<u64>,
}

impl NetworkNode {
    /// Create a new network node
    pub fn new(id: NodeId, node_type: NodeType) -> Self {
        NetworkNode {
            id,
            node_type,
            peers: HashSet::new(),
            max_peers: 50,
            online: true,
            latency_ms: 50, // Default 50ms latency
            inbox: VecDeque::new(),
            seen_messages: HashSet::new(),
        }
    }

    /// Check if node can accept more peers
    pub fn can_accept_peer(&self) -> bool {
        self.online && self.peers.len() < self.max_peers
    }

    /// Add a peer connection
    pub fn add_peer(&mut self, peer_id: NodeId) -> bool {
        if self.can_accept_peer() && peer_id != self.id {
            self.peers.insert(peer_id);
            true
        } else {
            false
        }
    }

    /// Remove a peer connection
    pub fn remove_peer(&mut self, peer_id: NodeId) {
        self.peers.remove(&peer_id);
    }

    /// Receive a message
    pub fn receive(&mut self, envelope: MessageEnvelope) -> bool {
        // Deduplicate
        if self.seen_messages.contains(&envelope.id) {
            return false;
        }

        self.seen_messages.insert(envelope.id);
        self.inbox.push_back(envelope);
        true
    }

    /// Get next message from inbox
    pub fn pop_message(&mut self) -> Option<MessageEnvelope> {
        self.inbox.pop_front()
    }
}

/// Latency model for network simulation
#[derive(Debug, Clone)]
pub enum LatencyModel {
    /// Fixed latency for all messages
    Fixed(Duration),

    /// Uniform random latency within range
    UniformRandom { min_ms: u32, max_ms: u32 },

    /// Geographic-based latency (simplified)
    Geographic {
        /// Latency matrix between regions
        regions: HashMap<(u32, u32), Duration>,
    },
}

impl LatencyModel {
    /// Calculate latency between two nodes
    pub fn latency(&self, _from: &NetworkNode, to: &NetworkNode) -> Duration {
        match self {
            LatencyModel::Fixed(d) => *d,
            LatencyModel::UniformRandom { min_ms, max_ms } => {
                // Simple deterministic "random" based on node latency
                let range = max_ms - min_ms;
                let offset = to.latency_ms % range;
                Duration::from_millis((*min_ms + offset) as u64)
            }
            LatencyModel::Geographic { .. } => {
                // Simplified: use node's configured latency
                Duration::from_millis(to.latency_ms as u64)
            }
        }
    }
}

/// Network statistics
#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    /// Total messages sent
    pub messages_sent: u64,

    /// Total messages delivered
    pub messages_delivered: u64,

    /// Messages dropped (offline nodes, etc.)
    pub messages_dropped: u64,

    /// Average delivery latency
    pub avg_latency_ms: f64,

    /// Number of active connections
    pub total_connections: u64,
}

/// The main network simulator
///
/// Manages all nodes and message passing with simulated latency
pub struct NetworkSimulator {
    /// All nodes in the network
    nodes: HashMap<NodeId, NetworkNode>,

    /// Messages in flight (not yet delivered)
    in_flight: VecDeque<MessageEnvelope>,

    /// Next message ID
    next_message_id: u64,

    /// Next node ID
    next_node_id: NodeId,

    /// Latency model
    latency_model: LatencyModel,

    /// Network statistics
    stats: NetworkStats,

    /// Simulation time
    current_time: Instant,

    /// Message handlers by node type
    /// In real impl, each node would have its own handler
    gossip_factor: u32,
}

impl NetworkSimulator {
    /// Create a new network simulator
    pub fn new(latency_model: LatencyModel) -> Self {
        NetworkSimulator {
            nodes: HashMap::new(),
            in_flight: VecDeque::new(),
            next_message_id: 1,
            next_node_id: 1,
            latency_model,
            stats: NetworkStats::default(),
            current_time: Instant::now(),
            gossip_factor: 8, // Forward to sqrt(peers) or 8, whichever is smaller
        }
    }

    /// Create a network with default settings
    pub fn default_network() -> Self {
        Self::new(LatencyModel::UniformRandom {
            min_ms: 20,
            max_ms: 200,
        })
    }

    /// Add a node to the network
    pub fn add_node(&mut self, node_type: NodeType) -> NodeId {
        let id = self.next_node_id;
        self.next_node_id += 1;

        let node = NetworkNode::new(id, node_type);
        self.nodes.insert(id, node);

        id
    }

    /// Add a node with custom latency
    pub fn add_node_with_latency(&mut self, node_type: NodeType, latency_ms: u32) -> NodeId {
        let id = self.add_node(node_type);
        if let Some(node) = self.nodes.get_mut(&id) {
            node.latency_ms = latency_ms;
        }
        id
    }

    /// Connect two nodes bidirectionally
    pub fn connect(&mut self, node1: NodeId, node2: NodeId) -> bool {
        if node1 == node2 {
            return false;
        }

        let can_connect = {
            let n1 = self.nodes.get(&node1);
            let n2 = self.nodes.get(&node2);

            match (n1, n2) {
                (Some(a), Some(b)) => a.can_accept_peer() && b.can_accept_peer(),
                _ => false,
            }
        };

        if can_connect {
            if let Some(n1) = self.nodes.get_mut(&node1) {
                n1.add_peer(node2);
            }
            if let Some(n2) = self.nodes.get_mut(&node2) {
                n2.add_peer(node1);
            }
            self.stats.total_connections += 1;
            true
        } else {
            false
        }
    }

    /// Disconnect two nodes
    pub fn disconnect(&mut self, node1: NodeId, node2: NodeId) {
        if let Some(n1) = self.nodes.get_mut(&node1) {
            n1.remove_peer(node2);
        }
        if let Some(n2) = self.nodes.get_mut(&node2) {
            n2.remove_peer(node1);
        }
        self.stats.total_connections = self.stats.total_connections.saturating_sub(1);
    }

    /// Take a node offline
    pub fn set_offline(&mut self, node_id: NodeId) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.online = false;
        }
    }

    /// Bring a node back online
    pub fn set_online(&mut self, node_id: NodeId) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.online = true;
        }
    }

    /// Send a direct message to a specific node
    pub fn send(&mut self, from: NodeId, to: NodeId, message: NetworkMessage) -> Option<u64> {
        let (from_node, to_node) = {
            let f = self.nodes.get(&from)?;
            let t = self.nodes.get(&to)?;
            (f.clone(), t.clone())
        };

        if !from_node.online || !to_node.online {
            self.stats.messages_dropped += 1;
            return None;
        }

        let latency = self.latency_model.latency(&from_node, &to_node);
        let msg_id = self.next_message_id;
        self.next_message_id += 1;

        let envelope = MessageEnvelope {
            id: msg_id,
            from,
            to: Some(to),
            message,
            sent_at: self.current_time,
            deliver_at: self.current_time + latency,
            hops: 0,
        };

        self.in_flight.push_back(envelope);
        self.stats.messages_sent += 1;

        Some(msg_id)
    }

    /// Broadcast a message to all peers of a node
    pub fn broadcast(&mut self, from: NodeId, message: NetworkMessage) -> Vec<u64> {
        let peers: Vec<NodeId> = {
            match self.nodes.get(&from) {
                Some(node) if node.online => node.peers.iter().copied().collect(),
                _ => return vec![],
            }
        };

        let mut msg_ids = Vec::new();

        for peer in peers {
            if let Some(id) = self.send(from, peer, message.clone()) {
                msg_ids.push(id);
            }
        }

        msg_ids
    }

    /// Gossip a message (broadcast with forwarding)
    pub fn gossip(&mut self, from: NodeId, message: NetworkMessage) -> Vec<u64> {
        // Initial broadcast from originator
        self.broadcast(from, message)
    }

    /// Process messages and advance simulation time
    pub fn tick(&mut self, duration: Duration) {
        self.current_time += duration;

        // Deliver messages that have reached their delivery time
        let mut to_deliver = Vec::new();
        let mut remaining = VecDeque::new();

        while let Some(envelope) = self.in_flight.pop_front() {
            if envelope.deliver_at <= self.current_time {
                to_deliver.push(envelope);
            } else {
                remaining.push_back(envelope);
            }
        }

        self.in_flight = remaining;

        // Deliver messages
        for envelope in to_deliver {
            if let Some(to_id) = envelope.to {
                if let Some(node) = self.nodes.get_mut(&to_id) {
                    if node.online {
                        let latency = envelope.deliver_at.duration_since(envelope.sent_at);
                        self.stats.avg_latency_ms =
                            (self.stats.avg_latency_ms * self.stats.messages_delivered as f64
                             + latency.as_millis() as f64)
                            / (self.stats.messages_delivered + 1) as f64;

                        node.receive(envelope.clone());
                        self.stats.messages_delivered += 1;

                        // Gossip forwarding
                        if envelope.hops < 3 {
                            self.forward_gossip(to_id, envelope);
                        }
                    } else {
                        self.stats.messages_dropped += 1;
                    }
                }
            }
        }
    }

    /// Forward a gossip message to some peers
    fn forward_gossip(&mut self, node_id: NodeId, mut envelope: MessageEnvelope) {
        envelope.hops += 1;

        let peers_to_forward: Vec<NodeId> = {
            match self.nodes.get(&node_id) {
                Some(node) => {
                    node.peers
                        .iter()
                        .filter(|&&p| p != envelope.from)
                        .take(self.gossip_factor as usize)
                        .copied()
                        .collect()
                }
                None => return,
            }
        };

        for peer in peers_to_forward {
            let _ = self.send(node_id, peer, envelope.message.clone());
        }
    }

    /// Get a node by ID
    pub fn get_node(&self, id: NodeId) -> Option<&NetworkNode> {
        self.nodes.get(&id)
    }

    /// Get mutable node by ID
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut NetworkNode> {
        self.nodes.get_mut(&id)
    }

    /// Get all nodes of a specific type
    pub fn get_nodes_by_type(&self, node_type: NodeType) -> Vec<&NetworkNode> {
        self.nodes
            .values()
            .filter(|n| n.node_type == node_type)
            .collect()
    }

    /// Get network statistics
    pub fn stats(&self) -> &NetworkStats {
        &self.stats
    }

    /// Create a partitioned network (for testing consensus)
    pub fn partition(&mut self, group_a: &[NodeId], group_b: &[NodeId]) {
        // Disconnect all links between the two groups
        for &a in group_a {
            for &b in group_b {
                self.disconnect(a, b);
            }
        }
    }

    /// Heal a network partition
    pub fn heal_partition(&mut self, group_a: &[NodeId], group_b: &[NodeId]) {
        // Reconnect groups (simplified: connect all)
        for &a in group_a {
            for &b in group_b {
                self.connect(a, b);
            }
        }
    }

    /// Get number of messages in flight
    pub fn pending_messages(&self) -> usize {
        self.in_flight.len()
    }

    /// Build a simple test network
    pub fn build_test_network(
        num_validators: u32,
        num_builders: u32,
        num_relays: u32,
    ) -> Self {
        let mut network = Self::default_network();

        // Add relays (central hub)
        let mut relay_ids = Vec::new();
        for _ in 0..num_relays {
            let id = network.add_node_with_latency(NodeType::Relay, 10);
            relay_ids.push(id);
        }

        // Add validators
        let mut validator_ids = Vec::new();
        for _ in 0..num_validators {
            let id = network.add_node_with_latency(NodeType::Validator, 50);
            validator_ids.push(id);

            // Connect to all relays
            for &relay in &relay_ids {
                network.connect(id, relay);
            }
        }

        // Add builders
        let mut builder_ids = Vec::new();
        for _ in 0..num_builders {
            let id = network.add_node_with_latency(NodeType::Builder, 30);
            builder_ids.push(id);

            // Connect to all relays
            for &relay in &relay_ids {
                network.connect(id, relay);
            }
        }

        // Connect validators to each other (mesh topology)
        for i in 0..validator_ids.len() {
            for j in (i + 1)..validator_ids.len() {
                network.connect(validator_ids[i], validator_ids[j]);
            }
        }

        network
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::Block;

    #[test]
    fn test_basic_network() {
        let mut network = NetworkSimulator::default_network();

        let node1 = network.add_node(NodeType::FullNode);
        let node2 = network.add_node(NodeType::FullNode);

        assert!(network.connect(node1, node2));

        let n1 = network.get_node(node1).unwrap();
        assert!(n1.peers.contains(&node2));
    }

    #[test]
    fn test_message_delivery() {
        let mut network = NetworkSimulator::new(LatencyModel::Fixed(Duration::from_millis(100)));

        let node1 = network.add_node(NodeType::FullNode);
        let node2 = network.add_node(NodeType::FullNode);
        network.connect(node1, node2);

        // Send a message
        let block = Block::genesis();
        network.send(node1, node2, NetworkMessage::NewBlock(block));

        // Message should be in flight
        assert_eq!(network.pending_messages(), 1);

        // Advance time past delivery
        network.tick(Duration::from_millis(150));

        // Message should be delivered
        assert_eq!(network.pending_messages(), 0);
        assert_eq!(network.stats().messages_delivered, 1);
    }

    #[test]
    fn test_offline_node() {
        let mut network = NetworkSimulator::default_network();

        let node1 = network.add_node(NodeType::FullNode);
        let node2 = network.add_node(NodeType::FullNode);
        network.connect(node1, node2);

        // Take node2 offline
        network.set_offline(node2);

        // Try to send a message
        let block = Block::genesis();
        let result = network.send(node1, node2, NetworkMessage::NewBlock(block));

        // Message should be dropped
        assert!(result.is_none());
        assert_eq!(network.stats().messages_dropped, 1);
    }

    #[test]
    fn test_broadcast() {
        let mut network = NetworkSimulator::default_network();

        let center = network.add_node(NodeType::FullNode);
        let peer1 = network.add_node(NodeType::FullNode);
        let peer2 = network.add_node(NodeType::FullNode);
        let peer3 = network.add_node(NodeType::FullNode);

        network.connect(center, peer1);
        network.connect(center, peer2);
        network.connect(center, peer3);

        // Broadcast from center
        let block = Block::genesis();
        let msg_ids = network.broadcast(center, NetworkMessage::NewBlock(block));

        assert_eq!(msg_ids.len(), 3);
        assert_eq!(network.stats().messages_sent, 3);
    }

    #[test]
    fn test_network_partition() {
        let mut network = NetworkSimulator::default_network();

        // Create two groups
        let group_a: Vec<_> = (0..3).map(|_| network.add_node(NodeType::Validator)).collect();
        let group_b: Vec<_> = (0..3).map(|_| network.add_node(NodeType::Validator)).collect();

        // Fully connect everyone
        for &a in &group_a {
            for &b in &group_b {
                network.connect(a, b);
            }
        }

        // Verify connections exist
        let n = network.get_node(group_a[0]).unwrap();
        assert!(n.peers.contains(&group_b[0]));

        // Create partition
        network.partition(&group_a, &group_b);

        // Verify connections are broken
        let n = network.get_node(group_a[0]).unwrap();
        assert!(!n.peers.contains(&group_b[0]));
    }

    #[test]
    fn test_mev_boost_topology() {
        let network = NetworkSimulator::build_test_network(4, 2, 1);

        // Check we have the right number of nodes
        assert_eq!(network.get_nodes_by_type(NodeType::Validator).len(), 4);
        assert_eq!(network.get_nodes_by_type(NodeType::Builder).len(), 2);
        assert_eq!(network.get_nodes_by_type(NodeType::Relay).len(), 1);
    }
}
