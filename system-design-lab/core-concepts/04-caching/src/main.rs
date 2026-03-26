//! # Caching Demo
//!
//! Demonstrates caching patterns using moka (production-grade cache, like Java Caffeine):
//! 1. Cache-Aside (lazy loading)
//! 2. Write-Through
//! 3. Write-Behind (write-back)
//! 4. Read-Through
//! 5. TTL + LRU Eviction
//! 6. Cache Stampede + prevention
//! 7. Cache Penetration + bloom filter
//! 8. Multi-Layer Caching (L1/L2)
//! 9. Cache Warming
//! 10. Hot Key problem
//! 11. Cache-Aside Race Condition + fix

mod cache_aside;
mod write_through;
mod write_behind;
mod read_through;
mod eviction;
mod stampede;
mod penetration;
mod multi_layer;
mod warming;
mod hot_key;
mod race_condition;

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║          Caching — Core Strategies               ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    println!("━━━ 1. Cache-Aside (Lazy Loading) ━━━");
    cache_aside::demo();

    println!("━━━ 2. Write-Through ━━━");
    write_through::demo();

    println!("━━━ 3. Write-Behind (Write-Back) ━━━");
    write_behind::demo();

    println!("━━━ 4. Read-Through ━━━");
    read_through::demo();

    println!("━━━ 5. TTL + LRU Eviction ━━━");
    eviction::demo();

    println!("━━━ 6. Cache Stampede (Thundering Herd) ━━━");
    stampede::demo();

    println!("━━━ 7. Cache Penetration ━━━");
    penetration::demo();

    println!("━━━ 8. Multi-Layer Caching (L1/L2) ━━━");
    multi_layer::demo();

    println!("━━━ 9. Cache Warming ━━━");
    warming::demo();

    println!("━━━ 10. Hot Key Problem ━━━");
    hot_key::demo();

    println!("━━━ 11. Cache-Aside Race Condition ━━━");
    race_condition::demo();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
