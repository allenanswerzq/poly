use std::time::Instant;

// =============================================================================
// Scatter-Gather (Cross-Shard Queries)
//
//   Problem: sharding puts user:1 on shard A and user:2 on shard B.
//   What if you need "all users with age > 25"? No single shard has all users.
//
//   Solution: SCATTER-GATHER
//     1. SCATTER: send the query to ALL shards in parallel
//     2. Each shard filters locally and returns partial results
//     3. GATHER: merge partial results at the coordinator
//
//         Client
//           │
//        Scatter
//        ┌──┼──┐
//        ▼  ▼  ▼
//       S0  S1  S2    ← each shard filters locally
//        │  │  │
//        Gather
//        └──┼──┘
//           ▼
//        Merged
//        Result
//
//   Performance: latency = slowest shard (P99 of N shards)
//     With 100 shards, you're waiting for the slowest one every time.
//     This is why scatter-gather doesn't scale well beyond ~10-20 shards.
//
//   Operations that need scatter-gather:
//     - COUNT(*) → sum counts from all shards
//     - ORDER BY + LIMIT → each shard returns top-K, coordinator merges
//     - JOIN across shard keys → must fetch from multiple shards
//     - Aggregations (SUM, AVG, MAX) → partial aggregates per shard
//
//   Design rule: choose your shard key so that 90%+ of queries
//   hit a SINGLE shard. Scatter-gather should be rare, not the norm.
// =============================================================================

/// A simulated shard with user data.
struct Shard {
    id: usize,
    users: Vec<User>,
}

#[derive(Debug, Clone)]
struct User {
    id: u64,
    name: String,
    age: u32,
    score: f64,
}

impl Shard {
    fn new(id: usize) -> Self {
        Self {
            id,
            users: Vec::new(),
        }
    }

    /// Local filter: only scans this shard's data.
    fn filter_by_age(&self, min_age: u32) -> Vec<&User> {
        self.users.iter().filter(|u| u.age >= min_age).collect()
    }

    /// Local aggregation: count users on this shard.
    fn count(&self) -> usize {
        self.users.len()
    }

    /// Local top-K by score.
    fn top_k_by_score(&self, k: usize) -> Vec<&User> {
        let mut sorted: Vec<&User> = self.users.iter().collect();
        sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        sorted.truncate(k);
        sorted
    }

    /// Local sum of scores.
    fn sum_scores(&self) -> f64 {
        self.users.iter().map(|u| u.score).sum()
    }
}

/// Coordinator that routes queries across shards.
struct ShardCluster {
    shards: Vec<Shard>,
}

impl ShardCluster {
    fn new(num_shards: usize) -> Self {
        let shards = (0..num_shards).map(Shard::new).collect();
        Self { shards }
    }

    /// Route user to shard by user_id.
    fn insert(&mut self, user: User) {
        let shard_id = (user.id as usize) % self.shards.len();
        self.shards[shard_id].users.push(user);
    }

    /// Single-shard query: get user by ID (O(1) routing, no scatter).
    fn get_by_id(&self, user_id: u64) -> Option<&User> {
        let shard_id = (user_id as usize) % self.shards.len();
        self.shards[shard_id].users.iter().find(|u| u.id == user_id)
    }

    /// SCATTER-GATHER: filter across all shards.
    fn scatter_filter_by_age(&self, min_age: u32) -> Vec<&User> {
        // Scatter: query each shard (in production: parallel RPCs)
        let mut results = Vec::new();
        for shard in &self.shards {
            results.extend(shard.filter_by_age(min_age));
        }
        // Gather: merge results (already done by extend)
        results
    }

    /// SCATTER-GATHER: count total users.
    fn scatter_count(&self) -> usize {
        // Each shard returns its count, coordinator sums
        self.shards.iter().map(|s| s.count()).sum()
    }

    /// SCATTER-GATHER: global top-K by score.
    fn scatter_top_k(&self, k: usize) -> Vec<&User> {
        // Each shard returns its local top-K
        let mut candidates: Vec<&User> = Vec::new();
        for shard in &self.shards {
            candidates.extend(shard.top_k_by_score(k));
        }
        // Coordinator merges and picks global top-K
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        candidates.truncate(k);
        candidates
    }

    /// SCATTER-GATHER: average score across all shards.
    fn scatter_avg_score(&self) -> f64 {
        let total_sum: f64 = self.shards.iter().map(|s| s.sum_scores()).sum();
        let total_count: usize = self.shards.iter().map(|s| s.count()).sum();
        if total_count > 0 {
            total_sum / total_count as f64
        } else {
            0.0
        }
    }
}

pub fn demo() {
    println!("\n  ═══ Scatter-Gather ═══\n");

    let mut cluster = ShardCluster::new(4);

    // Insert 20 users across 4 shards
    let names = [
        "Alice", "Bob", "Charlie", "Diana", "Eve", "Frank", "Grace", "Hank", "Ivy", "Jack", "Kate",
        "Leo", "Mia", "Noah", "Olivia", "Pat", "Quinn", "Rose", "Sam", "Tina",
    ];
    for (i, name) in names.iter().enumerate() {
        cluster.insert(User {
            id: i as u64,
            name: name.to_string(),
            age: 20 + (i as u32 % 15),
            score: (i as f64 * 7.3) % 100.0,
        });
    }

    println!("    20 users across 4 shards:\n");
    for shard in &cluster.shards {
        let names: Vec<&str> = shard.users.iter().map(|u| u.name.as_str()).collect();
        println!(
            "      Shard {}: {} users {:?}",
            shard.id,
            shard.users.len(),
            names
        );
    }

    // ── Single-shard query (fast) ──

    println!("\n    ── Single-shard query: GET user:5 ──\n");
    let start = Instant::now();
    let user = cluster.get_by_id(5);
    println!(
        "      Result: {:?} ({:?})",
        user.map(|u| &u.name),
        start.elapsed()
    );
    println!("      Only shard {} was queried (no scatter)", 5 % 4);

    // ── Scatter-gather: filter ──

    println!("\n    ── Scatter-gather: WHERE age >= 30 ──\n");
    let results = cluster.scatter_filter_by_age(30);
    println!("      Found {} users:", results.len());
    for u in &results {
        println!("        {} (age={}, shard={})", u.name, u.age, u.id % 4);
    }
    println!("      ALL 4 shards were queried");

    // ── Scatter-gather: count ──

    println!("\n    ── Scatter-gather: COUNT(*) ──\n");
    let count = cluster.scatter_count();
    println!(
        "      Total users: {} (sum of [{}])",
        count,
        cluster
            .shards
            .iter()
            .map(|s| s.count().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // ── Scatter-gather: top-K ──

    println!("\n    ── Scatter-gather: TOP 3 by score ──\n");
    let top = cluster.scatter_top_k(3);
    for (rank, u) in top.iter().enumerate() {
        println!(
            "      #{}: {} (score={:.1}, shard={})",
            rank + 1,
            u.name,
            u.score,
            u.id % 4
        );
    }
    println!("      Each shard returned its local top-3, coordinator merged");

    // ── Scatter-gather: average ──

    println!("\n    ── Scatter-gather: AVG(score) ──\n");
    let avg = cluster.scatter_avg_score();
    println!("      Average score: {:.1}", avg);
    println!("      = SUM(shard scores) / COUNT(all users)\n");

    println!("    Design rule: choose shard key so 90%+ of queries hit 1 shard.");
    println!("    Scatter-gather should be RARE, not the default.\n");
}
