#![allow(dead_code, unused_variables, unused_imports)]
//! # Load Balancing
//!
//! Load balancing distributes incoming requests across multiple backend servers
//! to increase throughput, reduce latency, and provide fault tolerance.
//!
//! **Rate limiting vs Load balancing:**
//! - Rate limiting = "how many requests can this CLIENT send?" (protects servers)
//! - Load balancing = "which SERVER should handle this request?" (distributes load)
//!
//! ┌──────────────┐         ┌──────────────┐
//! │   Client A   │────┐    │  Backend 1   │ 10% traffic
//! └──────────────┘    │    └──────────────┘
//! ┌──────────────┐    ▼    ┌──────────────┐
//! │   Client B   │──►[LB]──│  Backend 2   │ 90% traffic
//! └──────────────┘    ▲    └──────────────┘
//! ┌──────────────┐    │    ┌──────────────┐
//! │   Client C   │────┘    │  Backend 3   │  0% (draining)
//! └──────────────┘         └──────────────┘

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

// =============================================================================
// Backend representation
// =============================================================================

#[derive(Clone, Debug)]
struct Backend {
    name: String,
    weight: u32,      // relative weight (higher = more traffic)
    healthy: bool,
    active_conns: u32, // current active connections
}

impl Backend {
    fn new(name: &str, weight: u32) -> Self {
        Self {
            name: name.to_string(),
            weight,
            healthy: true,
            active_conns: 0,
        }
    }
}

// =============================================================================
// 1. Round Robin
// =============================================================================
// Simplest algorithm. Cycles through backends in order.
// Equal distribution. Does NOT account for backend capacity or load.
//
// Request 1 → Backend 0
// Request 2 → Backend 1
// Request 3 → Backend 2
// Request 4 → Backend 0   (wraps around)

struct RoundRobin {
    backends: Vec<Backend>,
    counter: usize,
}

impl RoundRobin {
    fn new(backends: Vec<Backend>) -> Self {
        Self {
            backends,
            counter: 0,
        }
    }

    fn next(&mut self) -> &str {
        let healthy: Vec<usize> = self
            .backends
            .iter()
            .enumerate()
            .filter(|(_, b)| b.healthy)
            .map(|(i, _)| i)
            .collect();

        if healthy.is_empty() {
            return "NO_HEALTHY_BACKEND";
        }

        let idx = healthy[self.counter % healthy.len()];
        self.counter += 1;
        &self.backends[idx].name
    }
}

// =============================================================================
// 2. Weighted Round Robin
// =============================================================================
// Like round robin, but backends with higher weight get proportionally more
// requests. Used for heterogeneous server fleets or traffic shifting.
//
// Backend A (weight=1) → 10% traffic
// Backend B (weight=9) → 90% traffic
//
// Implementation: smooth weighted round robin (nginx algorithm).
// Each round, add weight to current_weight, pick highest, subtract total.

struct WeightedRoundRobin {
    backends: Vec<Backend>,
    current_weights: Vec<i32>,
}

impl WeightedRoundRobin {
    fn new(backends: Vec<Backend>) -> Self {
        let n = backends.len();
        Self {
            backends,
            current_weights: vec![0i32; n],
        }
    }

    /// Smooth weighted round robin (nginx style).
    /// Produces a well-distributed sequence even with large weight differences.
    fn next(&mut self) -> &str {
        let total: i32 = self
            .backends
            .iter()
            .filter(|b| b.healthy)
            .map(|b| b.weight as i32)
            .sum();

        if total == 0 {
            return "NO_HEALTHY_BACKEND";
        }

        // Add effective weight to current weight
        for i in 0..self.backends.len() {
            if self.backends[i].healthy {
                self.current_weights[i] += self.backends[i].weight as i32;
            }
        }

        // Pick the backend with the highest current weight
        let mut best = 0;
        for i in 1..self.backends.len() {
            if self.backends[i].healthy && self.current_weights[i] > self.current_weights[best] {
                best = i;
            }
        }

        // Subtract total weight from the chosen backend
        self.current_weights[best] -= total;

        &self.backends[best].name
    }
}

// =============================================================================
// 3. Least Connections
// =============================================================================
// Sends to the backend with the fewest active connections.
// Best when requests have varying processing times.
//
// Backend A: 15 active connections ←
// Backend B: 3 active connections  ← PICK THIS
// Backend C: 10 active connections

struct LeastConnections {
    backends: Vec<Backend>,
}

impl LeastConnections {
    fn new(backends: Vec<Backend>) -> Self {
        Self { backends }
    }

    fn next(&mut self) -> &str {
        let best = self
            .backends
            .iter()
            .filter(|b| b.healthy)
            .min_by_key(|b| b.active_conns)
            .map(|b| b.name.as_str())
            .unwrap_or("NO_HEALTHY_BACKEND");
        best
    }

    fn connect(&mut self, name: &str) {
        if let Some(b) = self.backends.iter_mut().find(|b| b.name == name) {
            b.active_conns += 1;
        }
    }

    fn disconnect(&mut self, name: &str) {
        if let Some(b) = self.backends.iter_mut().find(|b| b.name == name) {
            b.active_conns = b.active_conns.saturating_sub(1);
        }
    }
}

// =============================================================================
// 4. Weighted Least Connections
// =============================================================================
// Combines weight and connection count: score = active_conns / weight.
// Lower score = better candidate.

struct WeightedLeastConnections {
    backends: Vec<Backend>,
}

impl WeightedLeastConnections {
    fn new(backends: Vec<Backend>) -> Self {
        Self { backends }
    }

    fn next(&self) -> &str {
        self.backends
            .iter()
            .filter(|b| b.healthy && b.weight > 0)
            .min_by(|a, b| {
                let score_a = a.active_conns as f64 / a.weight as f64;
                let score_b = b.active_conns as f64 / b.weight as f64;
                score_a.partial_cmp(&score_b).unwrap()
            })
            .map(|b| b.name.as_str())
            .unwrap_or("NO_HEALTHY_BACKEND")
    }

    fn connect(&mut self, name: &str) {
        if let Some(b) = self.backends.iter_mut().find(|b| b.name == name) {
            b.active_conns += 1;
        }
    }

    fn disconnect(&mut self, name: &str) {
        if let Some(b) = self.backends.iter_mut().find(|b| b.name == name) {
            b.active_conns = b.active_conns.saturating_sub(1);
        }
    }
}

// =============================================================================
// 5. IP Hash (Sticky Sessions)
// =============================================================================
// Hash the client IP to deterministically pick a backend.
// Same client always hits the same server → useful for session state.
//
// hash("10.0.0.1") % 3 = 1 → Backend 1  (always)
// hash("10.0.0.2") % 3 = 0 → Backend 0  (always)

struct IpHash {
    backends: Vec<Backend>,
}

impl IpHash {
    fn new(backends: Vec<Backend>) -> Self {
        Self { backends }
    }

    fn next(&self, client_ip: &str) -> &str {
        let healthy: Vec<&Backend> = self.backends.iter().filter(|b| b.healthy).collect();
        if healthy.is_empty() {
            return "NO_HEALTHY_BACKEND";
        }

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        client_ip.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % healthy.len();
        &healthy[idx].name
    }
}

// =============================================================================
// 6. Power of Two Choices (P2C)
// =============================================================================
// Pick 2 random backends, send to the one with fewer connections.
// Provably better than random, nearly as good as least-connections,
// but O(1) instead of O(n).
//
// Used in: Envoy proxy, gRPC, Twitter's Finagle.

struct PowerOfTwoChoices {
    backends: Vec<Backend>,
}

impl PowerOfTwoChoices {
    fn new(backends: Vec<Backend>) -> Self {
        Self { backends }
    }

    fn next(&self) -> &str {
        let healthy: Vec<&Backend> = self.backends.iter().filter(|b| b.healthy).collect();
        if healthy.is_empty() {
            return "NO_HEALTHY_BACKEND";
        }
        if healthy.len() == 1 {
            return &healthy[0].name;
        }

        let a = rand::random::<usize>() % healthy.len();
        let mut b = rand::random::<usize>() % healthy.len();
        while b == a {
            b = rand::random::<usize>() % healthy.len();
        }

        if healthy[a].active_conns <= healthy[b].active_conns {
            &healthy[a].name
        } else {
            &healthy[b].name
        }
    }

    fn connect(&mut self, name: &str) {
        if let Some(b) = self.backends.iter_mut().find(|b| b.name == name) {
            b.active_conns += 1;
        }
    }
}

// =============================================================================
// 7. Traffic Shifting / Gradual Migration
// =============================================================================
// Dynamically change weights to shift traffic between backends.
// Used for: deployments, migrations, draining servers.
//
// Step 1: old=100%, new=0%    (before deploy)
// Step 2: old=90%,  new=10%   (canary)
// Step 3: old=50%,  new=50%   (half traffic)
// Step 4: old=10%,  new=90%   (almost done)
// Step 5: old=0%,   new=100%  (complete)

struct TrafficSplitter {
    backends: Vec<(String, u32)>, // (name, weight out of 100)
}

impl TrafficSplitter {
    fn new(backends: Vec<(&str, u32)>) -> Self {
        Self {
            backends: backends
                .into_iter()
                .map(|(n, w)| (n.to_string(), w))
                .collect(),
        }
    }

    fn set_weight(&mut self, name: &str, weight: u32) {
        if let Some(b) = self.backends.iter_mut().find(|(n, _)| n == name) {
            b.1 = weight;
        }
    }

    fn route(&self) -> &str {
        let total: u32 = self.backends.iter().map(|(_, w)| w).sum();
        if total == 0 {
            return "NO_BACKEND";
        }
        let r = rand::random::<u32>() % total;
        let mut cumulative = 0;
        for (name, weight) in &self.backends {
            cumulative += weight;
            if r < cumulative {
                return name;
            }
        }
        &self.backends[0].0
    }

    fn distribution(&self, num_requests: usize) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for _ in 0..num_requests {
            let backend = self.route();
            *counts.entry(backend.to_string()).or_insert(0) += 1;
        }
        counts
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Load Balancing — Algorithms & Traffic Shifting ═══\n");

    let make_backends = || {
        vec![
            Backend::new("node-1", 1),
            Backend::new("node-2", 1),
            Backend::new("node-3", 1),
        ]
    };

    // --- 1. Round Robin ---
    println!("  ── 1. Round Robin ──");
    println!("  Cycles through backends in order. Simple, equal distribution.\n");
    let mut rr = RoundRobin::new(make_backends());
    let mut rr_dist = HashMap::new();
    for _ in 0..9 {
        let b = rr.next();
        print!("  {b} → ");
        *rr_dist.entry(b.to_string()).or_insert(0) += 1;
    }
    println!("\n");

    // --- 2. Weighted Round Robin (10%/90% split) ---
    println!("  ── 2. Weighted Round Robin (traffic split) ──");
    println!("  node-1 weight=1 (10%), node-2 weight=9 (90%)\n");
    let mut wrr = WeightedRoundRobin::new(vec![
        Backend::new("node-1", 1),
        Backend::new("node-2", 9),
    ]);

    let mut wrr_dist = HashMap::new();
    let n = 10_000;
    for _ in 0..n {
        let b = wrr.next();
        *wrr_dist.entry(b.to_string()).or_insert(0) += 1;
    }
    print_dist("  ", &wrr_dist, n);

    // Show the smooth distribution in first 10 picks
    println!("\n  First 20 picks (smooth interleaving, not bursty):");
    let mut wrr2 = WeightedRoundRobin::new(vec![
        Backend::new("A", 1),
        Backend::new("B", 9),
    ]);
    print!("  ");
    for _ in 0..20 {
        print!("{} ", wrr2.next());
    }
    println!("\n");

    // --- 3. Least Connections ---
    println!("  ── 3. Least Connections ──");
    println!("  Sends to backend with fewest active connections.\n");
    let mut lc = LeastConnections::new(make_backends());
    // Simulate: node-1 has 5 conns, node-2 has 2, node-3 has 0
    for _ in 0..5 {
        lc.connect("node-1");
    }
    for _ in 0..2 {
        lc.connect("node-2");
    }
    println!(
        "  Active: node-1={}, node-2={}, node-3={}",
        lc.backends[0].active_conns, lc.backends[1].active_conns, lc.backends[2].active_conns
    );
    println!("  Next request → {} (least loaded)\n", lc.next());

    // --- 4. IP Hash (sticky sessions) ---
    println!("  ── 4. IP Hash (Sticky Sessions) ──");
    println!("  Same client IP always hits the same backend.\n");
    let ip_hash = IpHash::new(make_backends());
    let test_ips = ["10.0.0.1", "10.0.0.2", "10.0.0.3", "192.168.1.100", "10.0.0.1"];
    for ip in &test_ips {
        println!("  {ip:>16} → {}", ip_hash.next(ip));
    }
    println!("  (note: 10.0.0.1 always maps to the same backend)\n");

    // --- 5. Power of Two Choices ---
    println!("  ── 5. Power of Two Choices (P2C) ──");
    println!("  Pick 2 random, choose the one with fewer connections.");
    println!("  O(1) per request, near-optimal distribution.\n");
    let mut p2c = PowerOfTwoChoices::new(vec![
        Backend::new("node-1", 1),
        Backend::new("node-2", 1),
        Backend::new("node-3", 1),
    ]);
    // Set uneven connections
    p2c.backends[0].active_conns = 100;
    p2c.backends[1].active_conns = 50;
    p2c.backends[2].active_conns = 10;
    let mut p2c_dist = HashMap::new();
    for _ in 0..3000 {
        let b = p2c.next();
        *p2c_dist.entry(b.to_string()).or_insert(0) += 1;
    }
    println!("  Connections: node-1=100, node-2=50, node-3=10");
    println!("  3000 requests routed by P2C (favors least-loaded):");
    print_dist("  ", &p2c_dist, 3000);
    println!();

    // --- 6. Traffic Shifting ---
    println!("  ── 6. Traffic Shifting (Deployment) ──");
    println!("  Gradually shift traffic from old to new version.\n");

    let steps: Vec<(&str, Vec<(&str, u32)>)> = vec![
        ("Before deploy",        vec![("old-v1", 100), ("new-v2", 0)]),
        ("Canary (10%)",         vec![("old-v1", 90),  ("new-v2", 10)]),
        ("Half traffic",         vec![("old-v1", 50),  ("new-v2", 50)]),
        ("Almost migrated",      vec![("old-v1", 10),  ("new-v2", 90)]),
        ("Complete",             vec![("old-v1", 0),   ("new-v2", 100)]),
    ];

    println!(
        "  {:<22} {:>10} {:>10}",
        "Stage", "old-v1", "new-v2"
    );
    println!("  {}", "-".repeat(44));

    for (label, weights) in &steps {
        let splitter = TrafficSplitter::new(weights.clone());
        let dist = splitter.distribution(10_000);
        let old_pct = dist.get("old-v1").unwrap_or(&0) * 100 / 10_000.max(1);
        let new_pct = dist.get("new-v2").unwrap_or(&0) * 100 / 10_000.max(1);
        println!(
            "  {:<22} {:>9}% {:>9}%",
            label, old_pct, new_pct,
        );
    }

    // --- 7. Multi-node traffic split ---
    println!("\n  ── 7. Multi-Node Traffic Split ──");
    println!("  Route traffic to 4 nodes with custom percentages.\n");

    let splitter = TrafficSplitter::new(vec![
        ("node-1", 10),  // 10%
        ("node-2", 20),  // 20%
        ("node-3", 30),  // 30%
        ("node-4", 40),  // 40%
    ]);

    let dist = splitter.distribution(100_000);
    let mut sorted: Vec<_> = dist.iter().collect();
    sorted.sort_by_key(|(k, _)| k.clone());
    for (name, count) in &sorted {
        let pct = **count as f64 / 1000.0;
        let bar = "█".repeat(pct as usize);
        println!("  {name}: {pct:5.1}%  {bar}");
    }

    // --- 8. Handling failures ---
    println!("\n  ── 8. Health-Aware Load Balancing ──");
    println!("  Unhealthy backends are automatically skipped.\n");
    let mut backends = make_backends();
    backends[1].healthy = false; // node-2 is down
    let mut rr_healthy = RoundRobin::new(backends);
    let mut healthy_dist = HashMap::new();
    for _ in 0..6 {
        let b = rr_healthy.next();
        *healthy_dist.entry(b.to_string()).or_insert(0) += 1;
    }
    println!("  node-2 is DOWN. 6 requests distributed:");
    for (name, count) in &healthy_dist {
        println!("    {name}: {count} requests");
    }

    // --- Algorithm comparison ---
    println!("\n  ── Algorithm Comparison ──\n");
    println!(
        "  {:<26} {:<12} {:<12} {:<15} {:<10}",
        "Algorithm", "Complexity", "Sticky?", "Weight-aware?", "Best for"
    );
    println!("  {}", "-".repeat(75));
    let rows = [
        ("Round Robin",            "O(1)",  "No",  "No",  "Equal servers"),
        ("Weighted Round Robin",   "O(1)",  "No",  "Yes", "Mixed capacity"),
        ("Least Connections",      "O(n)",  "No",  "No",  "Varied latency"),
        ("Weighted Least Conns",   "O(n)",  "No",  "Yes", "Mixed + varied"),
        ("IP Hash",                "O(1)",  "Yes", "No",  "Session state"),
        ("Power of Two Choices",   "O(1)",  "No",  "No",  "Large scale"),
        ("Consistent Hashing",     "O(logN)","Yes","Yes", "Cache clusters"),
    ];
    for (name, complexity, sticky, weighted, best) in &rows {
        println!(
            "  {:<26} {:<12} {:<12} {:<15} {:<10}",
            name, complexity, sticky, weighted, best
        );
    }
    println!();
}

fn print_dist(prefix: &str, dist: &HashMap<String, usize>, total: usize) {
    let mut sorted: Vec<_> = dist.iter().collect();
    sorted.sort_by_key(|(k, _)| k.clone());
    for (name, count) in &sorted {
        let pct = **count as f64 / total as f64 * 100.0;
        println!("{prefix}{name}: {count} ({pct:.1}%)");
    }
}
