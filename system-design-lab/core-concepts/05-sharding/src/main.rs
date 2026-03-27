//! # Sharding — Runnable Demos
//!
//! Split data across multiple "database servers" (simulated in-memory).
//!
//! Demos:
//!   1. Hash Sharding — hash(key) % N
//!   2. Range Sharding — key ranges map to shards
//!   3. Directory Sharding — lookup table maps keys to shards
//!   4. Rebalancing — what happens when you add/remove shards
//!   5. Scatter-Gather — cross-shard queries

mod directory_sharding;
mod hash_sharding;
mod range_sharding;
mod rebalancing;
mod scatter_gather;

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║            Sharding — Demos                      ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    println!("━━━ 1. Hash Sharding ━━━");
    hash_sharding::demo();

    println!("━━━ 2. Range Sharding ━━━");
    range_sharding::demo();

    println!("━━━ 3. Directory Sharding ━━━");
    directory_sharding::demo();

    println!("━━━ 4. Rebalancing ━━━");
    rebalancing::demo();

    println!("━━━ 5. Scatter-Gather ━━━");
    scatter_gather::demo();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
