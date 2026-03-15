//! # Load Balancer Implementation
//!
//! Demonstrates various load balancing algorithms:
//! - Round Robin
//! - Weighted Round Robin
//! - Least Connections
//! - Consistent Hashing (sticky sessions)
//! - Random
//! - IP Hash

use dashmap::DashMap;
use parking_lot::RwLock;
use rand::Rng;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

// =============================================================================
// Server Types
// =============================================================================

#[derive(Debug, Clone)]
pub struct Server {
    pub id: String,
    pub address: String,
    pub weight: u32,
    pub healthy: bool,
}

impl Server {
    pub fn new(id: &str, address: &str) -> Self {
        Self {
            id: id.to_string(),
            address: address.to_string(),
            weight: 1,
            healthy: true,
        }
    }

    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }
}

#[derive(Debug)]
pub struct ServerStats {
    pub active_connections: AtomicU32,
    pub total_requests: AtomicU32,
    pub last_health_check: RwLock<Instant>,
}

impl Default for ServerStats {
    fn default() -> Self {
        Self {
            active_connections: AtomicU32::new(0),
            total_requests: AtomicU32::new(0),
            last_health_check: RwLock::new(Instant::now()),
        }
    }
}

// =============================================================================
// Load Balancing Algorithms
// =============================================================================

pub trait LoadBalancer: Send + Sync {
    fn select(&self, servers: &[Server], client_info: Option<&str>) -> Option<usize>;
    fn name(&self) -> &str;
}

/// Round Robin - Simple rotation through servers
pub struct RoundRobin {
    counter: AtomicUsize,
}

impl RoundRobin {
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl LoadBalancer for RoundRobin {
    fn select(&self, servers: &[Server], _client_info: Option<&str>) -> Option<usize> {
        let healthy: Vec<_> = servers.iter().enumerate()
            .filter(|(_, s)| s.healthy)
            .collect();

        if healthy.is_empty() {
            return None;
        }

        let idx = self.counter.fetch_add(1, Ordering::SeqCst);
        Some(healthy[idx % healthy.len()].0)
    }

    fn name(&self) -> &str {
        "Round Robin"
    }
}

/// Weighted Round Robin - Servers with higher weight get more requests
pub struct WeightedRoundRobin {
    counter: AtomicUsize,
}

impl WeightedRoundRobin {
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl LoadBalancer for WeightedRoundRobin {
    fn select(&self, servers: &[Server], _client_info: Option<&str>) -> Option<usize> {
        // Build expanded list based on weights
        let mut weighted: Vec<usize> = Vec::new();
        for (idx, server) in servers.iter().enumerate() {
            if server.healthy {
                for _ in 0..server.weight {
                    weighted.push(idx);
                }
            }
        }

        if weighted.is_empty() {
            return None;
        }

        let idx = self.counter.fetch_add(1, Ordering::SeqCst);
        Some(weighted[idx % weighted.len()])
    }

    fn name(&self) -> &str {
        "Weighted Round Robin"
    }
}

/// Least Connections - Select server with fewest active connections
pub struct LeastConnections {
    stats: Arc<DashMap<String, ServerStats>>,
}

impl LeastConnections {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(DashMap::new()),
        }
    }

    pub fn connect(&self, server_id: &str) {
        self.stats
            .entry(server_id.to_string())
            .or_default()
            .active_connections
            .fetch_add(1, Ordering::SeqCst);
    }

    pub fn disconnect(&self, server_id: &str) {
        if let Some(stats) = self.stats.get(server_id) {
            stats.active_connections.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub fn get_connections(&self, server_id: &str) -> u32 {
        self.stats
            .get(server_id)
            .map(|s| s.active_connections.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}

impl LoadBalancer for LeastConnections {
    fn select(&self, servers: &[Server], _client_info: Option<&str>) -> Option<usize> {
        servers.iter()
            .enumerate()
            .filter(|(_, s)| s.healthy)
            .min_by_key(|(_, s)| self.get_connections(&s.id))
            .map(|(idx, _)| idx)
    }

    fn name(&self) -> &str {
        "Least Connections"
    }
}

/// IP Hash - Same client always goes to same server (sticky sessions)
pub struct IpHash;

impl IpHash {
    pub fn new() -> Self {
        Self
    }

    fn hash(key: &str) -> usize {
        let mut hash: usize = 5381;
        for byte in key.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as usize);
        }
        hash
    }
}

impl LoadBalancer for IpHash {
    fn select(&self, servers: &[Server], client_info: Option<&str>) -> Option<usize> {
        let healthy: Vec<_> = servers.iter().enumerate()
            .filter(|(_, s)| s.healthy)
            .collect();

        if healthy.is_empty() {
            return None;
        }

        let key = client_info.unwrap_or("default");
        let hash = Self::hash(key);
        Some(healthy[hash % healthy.len()].0)
    }

    fn name(&self) -> &str {
        "IP Hash"
    }
}

/// Random selection
pub struct RandomSelection;

impl RandomSelection {
    pub fn new() -> Self {
        Self
    }
}

impl LoadBalancer for RandomSelection {
    fn select(&self, servers: &[Server], _client_info: Option<&str>) -> Option<usize> {
        let healthy: Vec<_> = servers.iter().enumerate()
            .filter(|(_, s)| s.healthy)
            .collect();

        if healthy.is_empty() {
            return None;
        }

        let idx = rand::thread_rng().gen_range(0..healthy.len());
        Some(healthy[idx].0)
    }

    fn name(&self) -> &str {
        "Random"
    }
}

// =============================================================================
// Load Balancer Service
// =============================================================================

/// Complete load balancer with health checking
pub struct LoadBalancerService {
    servers: RwLock<Vec<Server>>,
    algorithm: Box<dyn LoadBalancer>,
    stats: DashMap<String, ServerStats>,
}

impl LoadBalancerService {
    pub fn new(algorithm: Box<dyn LoadBalancer>) -> Self {
        Self {
            servers: RwLock::new(Vec::new()),
            algorithm,
            stats: DashMap::new(),
        }
    }

    pub fn add_server(&self, server: Server) {
        self.stats.insert(server.id.clone(), ServerStats::default());
        self.servers.write().push(server);
    }

    pub fn remove_server(&self, server_id: &str) {
        self.servers.write().retain(|s| s.id != server_id);
        self.stats.remove(server_id);
    }

    pub fn set_server_health(&self, server_id: &str, healthy: bool) {
        if let Some(server) = self.servers.write().iter_mut().find(|s| s.id == server_id) {
            server.healthy = healthy;
            println!("[Health] {} is now {}", server_id, if healthy { "UP" } else { "DOWN" });
        }
    }

    /// Route a request to a server
    pub fn route(&self, client_ip: Option<&str>) -> Option<Server> {
        let servers = self.servers.read();
        let idx = self.algorithm.select(&servers, client_ip)?;
        let server = servers[idx].clone();

        // Track stats
        if let Some(stats) = self.stats.get(&server.id) {
            stats.total_requests.fetch_add(1, Ordering::SeqCst);
        }

        Some(server)
    }

    /// Get current server list
    pub fn get_servers(&self) -> Vec<Server> {
        self.servers.read().clone()
    }

    /// Get request distribution stats
    pub fn get_stats(&self) -> HashMap<String, u32> {
        let mut result = HashMap::new();
        for entry in self.stats.iter() {
            result.insert(
                entry.key().clone(),
                entry.value().total_requests.load(Ordering::SeqCst),
            );
        }
        result
    }

    pub fn algorithm_name(&self) -> &str {
        self.algorithm.name()
    }
}

// =============================================================================
// Demonstration
// =============================================================================

fn main() {
    println!("=== Load Balancer Demo ===\n");

    let servers = vec![
        Server::new("server-1", "10.0.0.1:8080").with_weight(1),
        Server::new("server-2", "10.0.0.2:8080").with_weight(2),
        Server::new("server-3", "10.0.0.3:8080").with_weight(1),
    ];

    // Demo 1: Round Robin
    println!("--- Round Robin ---");
    let lb = LoadBalancerService::new(Box::new(RoundRobin::new()));
    for server in &servers {
        lb.add_server(server.clone());
    }

    print!("Requests: ");
    for _ in 0..10 {
        if let Some(server) = lb.route(None) {
            print!("{} ", server.id);
        }
    }
    println!("\n");

    // Demo 2: Weighted Round Robin
    println!("--- Weighted Round Robin (server-2 has weight 2) ---");
    let lb = LoadBalancerService::new(Box::new(WeightedRoundRobin::new()));
    for server in &servers {
        lb.add_server(server.clone());
    }

    print!("Requests: ");
    for _ in 0..12 {
        if let Some(server) = lb.route(None) {
            print!("{} ", server.id);
        }
    }
    println!("\n");

    // Demo 3: IP Hash (sticky sessions)
    println!("--- IP Hash (Sticky Sessions) ---");
    let lb = LoadBalancerService::new(Box::new(IpHash::new()));
    for server in &servers {
        lb.add_server(server.clone());
    }

    let clients = ["192.168.1.1", "192.168.1.2", "192.168.1.3"];
    for client in &clients {
        print!("Client {} -> ", client);
        for _ in 0..3 {
            if let Some(server) = lb.route(Some(client)) {
                print!("{} ", server.id);
            }
        }
        println!("(same server each time)");
    }
    println!();

    // Demo 4: Least Connections (shared for connection tracking)
    println!("--- Least Connections ---");
    let lc = Arc::new(LeastConnections::new());
    let lb = LoadBalancerService::new(Box::new(LeastConnections::new()));
    for server in &servers {
        lb.add_server(server.clone());
    }

    // Simulate some active connections
    lc.connect("server-1");
    lc.connect("server-1");
    lc.connect("server-2");

    println!("Active connections: server-1=2, server-2=1, server-3=0");
    print!("Next 5 requests go to: ");
    for _ in 0..5 {
        if let Some(server) = lb.route(None) {
            print!("{} ", server.id);
            lc.connect(&server.id);
        }
    }
    println!("\n");

    // Demo 5: Health Checks
    println!("--- Health Check Simulation ---");
    let lb = LoadBalancerService::new(Box::new(RoundRobin::new()));
    for server in &servers {
        lb.add_server(server.clone());
    }

    println!("All servers healthy:");
    print!("  Requests: ");
    for _ in 0..6 {
        if let Some(server) = lb.route(None) {
            print!("{} ", server.id);
        }
    }
    println!();

    // Mark server-2 as unhealthy
    lb.set_server_health("server-2", false);
    print!("  Requests: ");
    for _ in 0..6 {
        if let Some(server) = lb.route(None) {
            print!("{} ", server.id);
        }
    }
    println!();

    // Restore server-2
    lb.set_server_health("server-2", true);
    print!("  Requests: ");
    for _ in 0..6 {
        if let Some(server) = lb.route(None) {
            print!("{} ", server.id);
        }
    }
    println!();

    // Demo 6: Statistics
    println!("\n--- Request Statistics ---");
    let lb = LoadBalancerService::new(Box::new(WeightedRoundRobin::new()));
    for server in &servers {
        lb.add_server(server.clone());
    }

    for _ in 0..100 {
        lb.route(None);
    }

    let stats = lb.get_stats();
    println!("Distribution over 100 requests:");
    for (server_id, count) in &stats {
        let percentage = (*count as f64 / 100.0) * 100.0;
        println!("  {}: {} requests ({:.0}%)", server_id, count, percentage);
    }

    println!("\n=== Demo Complete ===");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_servers() -> Vec<Server> {
        vec![
            Server::new("s1", "10.0.0.1:80"),
            Server::new("s2", "10.0.0.2:80"),
            Server::new("s3", "10.0.0.3:80"),
        ]
    }

    #[test]
    fn test_round_robin() {
        let rr = RoundRobin::new();
        let servers = test_servers();

        let idx1 = rr.select(&servers, None);
        let idx2 = rr.select(&servers, None);
        let idx3 = rr.select(&servers, None);
        let idx4 = rr.select(&servers, None);

        assert_eq!(idx1, Some(0));
        assert_eq!(idx2, Some(1));
        assert_eq!(idx3, Some(2));
        assert_eq!(idx4, Some(0));  // Wraps around
    }

    #[test]
    fn test_ip_hash_sticky() {
        let ih = IpHash::new();
        let servers = test_servers();

        // Same client should always go to same server
        let idx1 = ih.select(&servers, Some("192.168.1.1"));
        let idx2 = ih.select(&servers, Some("192.168.1.1"));
        let idx3 = ih.select(&servers, Some("192.168.1.1"));

        assert_eq!(idx1, idx2);
        assert_eq!(idx2, idx3);
    }

    #[test]
    fn test_unhealthy_server_skipped() {
        let rr = RoundRobin::new();
        let mut servers = test_servers();
        servers[1].healthy = false;

        let selections: Vec<_> = (0..6)
            .filter_map(|_| rr.select(&servers, None))
            .collect();

        // Should never select server 1
        assert!(selections.iter().all(|&idx| idx != 1));
    }

    #[test]
    fn test_weighted_distribution() {
        let wrr = WeightedRoundRobin::new();
        let servers = vec![
            Server::new("s1", "10.0.0.1:80").with_weight(1),
            Server::new("s2", "10.0.0.2:80").with_weight(2),
        ];

        let mut counts = [0, 0];
        for _ in 0..300 {
            if let Some(idx) = wrr.select(&servers, None) {
                counts[idx] += 1;
            }
        }

        // s2 should get roughly 2x the requests
        assert!(counts[1] > counts[0] * 3 / 2);  // At least 1.5x
    }
}
