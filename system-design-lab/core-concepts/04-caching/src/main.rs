//! # Caching Demo — All patterns using REAL Redis + SQLite
//!
//! Redis is compiled from source by build.rs (first build takes ~30s).
//! Each demo starts its own Redis server on a unique port and shuts it down after.
//!
//! Demos:
//!   1. Cache-Aside (lazy loading)
//!   2. Write-Through (DB first, cache second)
//!   3. Write-Behind (cache only, async DB flush)
//!   4. TTL + Eviction (Redis EXPIRE, maxmemory)
//!   5. Cache Stampede + SETNX lock fix
//!   6. Cache Penetration + negative caching
//!   7. Cache Warming (pipeline pre-load)
//!   8. Hot Key + L1 local cache

mod cache_aside;
mod eviction;
mod hot_key;
mod penetration;
mod redis_server;
mod stampede;
mod store;
mod warming;
mod write_behind;
mod write_through;

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║    Caching — Real Redis + SQLite Demos           ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    println!("━━━ 1. Cache-Aside ━━━");
    cache_aside::demo();

    println!("━━━ 2. Write-Through ━━━");
    write_through::demo();

    println!("━━━ 3. Write-Behind ━━━");
    write_behind::demo();

    println!("━━━ 4. TTL + Eviction ━━━");
    eviction::demo();

    println!("━━━ 5. Cache Stampede ━━━");
    stampede::demo();

    println!("━━━ 6. Cache Penetration ━━━");
    penetration::demo();

    println!("━━━ 7. Cache Warming ━━━");
    warming::demo();

    println!("━━━ 8. Hot Key ━━━");
    hot_key::demo();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
