//! # Node Discovery (discv4)
//!
//! Simplified implementation of Ethereum's node discovery protocol v4.
//! Based on Kademlia DHT with some modifications.
//!
//! Messages:
//! - Ping: Check if node is alive
//! - Pong: Response to ping
//! - FindNode: Request nodes close to target
//! - Neighbors: Response with list of nodes

use eth_primitives::{H256, keccak256};
use crate::node::{NodeId, NodeRecord, Endpoint};
use crate::error::{P2pError, Result};
use std::collections::HashMap;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

/// Discovery message types
#[derive(Debug, Clone)]
pub enum DiscoveryMessage {
    Ping {
        version: u32,
        from: Endpoint,
        to: Endpoint,
        expiration: u64,
    },
    Pong {
        to: Endpoint,
        ping_hash: H256,
        expiration: u64,
    },
    FindNode {
        target: NodeId,
        expiration: u64,
    },
    Neighbors {
        nodes: Vec<NodeRecord>,
        expiration: u64,
    },
}

impl DiscoveryMessage {
    /// Get message type byte
    pub fn type_byte(&self) -> u8 {
        match self {
            DiscoveryMessage::Ping { .. } => 0x01,
            DiscoveryMessage::Pong { .. } => 0x02,
            DiscoveryMessage::FindNode { .. } => 0x03,
            DiscoveryMessage::Neighbors { .. } => 0x04,
        }
    }
}

/// Kademlia bucket for routing table
#[derive(Debug)]
pub struct KBucket {
    /// Nodes in this bucket (max 16 per Ethereum spec)
    pub nodes: Vec<NodeRecord>,
    /// Maximum bucket size
    pub max_size: usize,
}

impl KBucket {
    pub fn new(max_size: usize) -> Self {
        KBucket {
            nodes: Vec::new(),
            max_size,
        }
    }

    /// Add node to bucket
    pub fn add(&mut self, node: NodeRecord) -> bool {
        // Check if already exists
        if let Some(pos) = self.nodes.iter().position(|n| n.id == node.id) {
            // Move to end (most recently seen)
            self.nodes.remove(pos);
            self.nodes.push(node);
            return true;
        }

        if self.nodes.len() < self.max_size {
            self.nodes.push(node);
            true
        } else {
            // Bucket full - in real implementation, would ping oldest
            false
        }
    }

    /// Remove node from bucket
    pub fn remove(&mut self, id: &NodeId) -> bool {
        if let Some(pos) = self.nodes.iter().position(|n| &n.id == id) {
            self.nodes.remove(pos);
            true
        } else {
            false
        }
    }

    /// Get closest nodes to target
    pub fn closest(&self, target: &NodeId, count: usize) -> Vec<&NodeRecord> {
        let mut nodes: Vec<_> = self.nodes.iter().collect();
        nodes.sort_by_key(|n| n.id.log_distance(target));
        nodes.truncate(count);
        nodes
    }
}

/// Routing table (256 k-buckets)
#[derive(Debug)]
pub struct RoutingTable {
    /// Our node ID
    pub local_id: NodeId,
    /// K-buckets (one per bit of distance)
    pub buckets: Vec<KBucket>,
    /// Bucket size (k = 16 in Ethereum)
    pub bucket_size: usize,
}

impl RoutingTable {
    pub fn new(local_id: NodeId, bucket_size: usize) -> Self {
        let mut buckets = Vec::with_capacity(256);
        for _ in 0..256 {
            buckets.push(KBucket::new(bucket_size));
        }

        RoutingTable {
            local_id,
            buckets,
            bucket_size,
        }
    }

    /// Get bucket index for a node
    fn bucket_index(&self, id: &NodeId) -> usize {
        let dist = self.local_id.log_distance(id);
        if dist == 0 {
            0
        } else {
            dist - 1
        }
    }

    /// Add node to routing table
    pub fn add(&mut self, node: NodeRecord) -> bool {
        if node.id == self.local_id {
            return false;
        }

        let idx = self.bucket_index(&node.id);
        self.buckets[idx].add(node)
    }

    /// Remove node from routing table
    pub fn remove(&mut self, id: &NodeId) -> bool {
        let idx = self.bucket_index(id);
        self.buckets[idx].remove(id)
    }

    /// Find closest nodes to target
    pub fn closest(&self, target: &NodeId, count: usize) -> Vec<NodeRecord> {
        let mut all_nodes: Vec<_> = self.buckets.iter()
            .flat_map(|b| b.nodes.iter())
            .collect();

        all_nodes.sort_by_key(|n| n.id.log_distance(target));
        all_nodes.truncate(count);
        all_nodes.into_iter().cloned().collect()
    }

    /// Get total node count
    pub fn node_count(&self) -> usize {
        self.buckets.iter().map(|b| b.nodes.len()).sum()
    }
}

/// Discovery service state
pub struct Discovery {
    /// Our node ID and key
    pub local_id: NodeId,
    /// Routing table
    pub routing_table: RoutingTable,
    /// Pending ping responses
    pub pending_pings: HashMap<H256, (NodeId, Instant)>,
    /// Bootstrap nodes
    pub bootstrap_nodes: Vec<NodeRecord>,
}

impl Discovery {
    /// Create new discovery service
    pub fn new(local_id: NodeId) -> Self {
        Discovery {
            local_id: local_id.clone(),
            routing_table: RoutingTable::new(local_id, 16),
            pending_pings: HashMap::new(),
            bootstrap_nodes: Vec::new(),
        }
    }

    /// Add bootstrap node
    pub fn add_bootstrap(&mut self, node: NodeRecord) {
        self.bootstrap_nodes.push(node.clone());
        self.routing_table.add(node);
    }

    /// Create ping message
    pub fn create_ping(&mut self, to: &Endpoint) -> DiscoveryMessage {
        let from = Endpoint::new(
            IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            30303,
            30303
        );

        DiscoveryMessage::Ping {
            version: 4,
            from,
            to: to.clone(),
            expiration: Self::expiration(),
        }
    }

    /// Create pong message
    pub fn create_pong(&self, to: &Endpoint, ping_hash: H256) -> DiscoveryMessage {
        DiscoveryMessage::Pong {
            to: to.clone(),
            ping_hash,
            expiration: Self::expiration(),
        }
    }

    /// Create find_node message
    pub fn create_find_node(&self, target: &NodeId) -> DiscoveryMessage {
        DiscoveryMessage::FindNode {
            target: target.clone(),
            expiration: Self::expiration(),
        }
    }

    /// Create neighbors response
    pub fn create_neighbors(&self, target: &NodeId) -> DiscoveryMessage {
        let nodes = self.routing_table.closest(target, 16);
        DiscoveryMessage::Neighbors {
            nodes,
            expiration: Self::expiration(),
        }
    }

    /// Handle incoming message
    pub fn handle_message(&mut self, from: &NodeRecord, msg: DiscoveryMessage) -> Option<DiscoveryMessage> {
        // Add sender to routing table
        self.routing_table.add(from.clone());

        match msg {
            DiscoveryMessage::Ping { to, .. } => {
                // Respond with pong
                let ping_hash = keccak256(b"ping"); // Simplified
                Some(self.create_pong(&to, ping_hash))
            }
            DiscoveryMessage::Pong { ping_hash, .. } => {
                // Remove from pending
                self.pending_pings.remove(&ping_hash);
                None
            }
            DiscoveryMessage::FindNode { target, .. } => {
                // Respond with neighbors
                Some(self.create_neighbors(&target))
            }
            DiscoveryMessage::Neighbors { nodes, .. } => {
                // Add nodes to routing table
                for node in nodes {
                    self.routing_table.add(node);
                }
                None
            }
        }
    }

    /// Get expiration timestamp (now + 20 seconds)
    fn expiration() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now + 20
    }

    /// Find nodes close to target (Kademlia lookup)
    pub fn find_node(&self, target: &NodeId) -> Vec<NodeRecord> {
        self.routing_table.closest(target, 16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn create_test_node() -> NodeRecord {
        let (id, _) = NodeId::random();
        let endpoint = Endpoint::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            30303,
            30303
        );
        NodeRecord::new(id, endpoint)
    }

    #[test]
    fn test_kbucket() {
        let mut bucket = KBucket::new(16);

        for i in 0..20 {
            let node = create_test_node();
            let added = bucket.add(node);
            if i < 16 {
                assert!(added);
            }
        }

        assert_eq!(bucket.nodes.len(), 16);
    }

    #[test]
    fn test_routing_table() {
        let (local_id, _) = NodeId::random();
        let mut table = RoutingTable::new(local_id, 16);

        for _ in 0..100 {
            let node = create_test_node();
            table.add(node);
        }

        assert!(table.node_count() > 0);
    }

    #[test]
    fn test_discovery() {
        let (local_id, _) = NodeId::random();
        let mut discovery = Discovery::new(local_id.clone());

        // Add some bootstrap nodes
        for _ in 0..5 {
            let node = create_test_node();
            discovery.add_bootstrap(node);
        }

        assert_eq!(discovery.routing_table.node_count(), 5);

        // Find nodes close to target
        let (target, _) = NodeId::random();
        let closest = discovery.find_node(&target);
        assert!(closest.len() <= 16);
    }

    #[test]
    fn test_ping_pong() {
        let (local_id, _) = NodeId::random();
        let mut discovery = Discovery::new(local_id);

        let endpoint = Endpoint::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            30303,
            30303
        );

        let ping = discovery.create_ping(&endpoint);
        assert_eq!(ping.type_byte(), 0x01);

        let from_node = create_test_node();
        let response = discovery.handle_message(&from_node, ping);

        // Should respond with pong
        assert!(matches!(response, Some(DiscoveryMessage::Pong { .. })));
    }

    #[test]
    fn test_find_node_message() {
        let (local_id, _) = NodeId::random();
        let mut discovery = Discovery::new(local_id);

        // Add nodes
        for _ in 0..10 {
            discovery.routing_table.add(create_test_node());
        }

        let (target, _) = NodeId::random();
        let find_node = DiscoveryMessage::FindNode {
            target: target.clone(),
            expiration: Discovery::expiration(),
        };

        let from_node = create_test_node();
        let response = discovery.handle_message(&from_node, find_node);

        // Should respond with neighbors
        assert!(matches!(response, Some(DiscoveryMessage::Neighbors { .. })));
    }
}
