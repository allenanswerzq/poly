//! # Caching Demo
//!
//! Demonstrates caching patterns using moka (production-grade cache, like Java Caffeine):
//! 1. Cache-Aside (lazy loading) — check cache, miss → DB, populate cache
//! 2. Write-Through — write to cache + DB synchronously
//! 3. Write-Behind — write to cache, async flush to DB
//! 4. TTL + Eviction — automatic expiry, LRU eviction
//! 5. Cache Stampede — thundering herd problem + prevention
//! 6. Cache Penetration — queries for non-existent data

mod cache_aside;
mod write_through;
mod write_behind;
mod eviction;
mod stampede;
mod penetration;

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

    println!("━━━ 4. TTL + LRU Eviction ━━━");
    eviction::demo();

    println!("━━━ 5. Cache Stampede (Thundering Herd) ━━━");
    stampede::demo();

    println!("━━━ 6. Cache Penetration ━━━");
    penetration::demo();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
