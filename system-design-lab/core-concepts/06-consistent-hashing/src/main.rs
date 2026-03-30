#![allow(dead_code, unused_variables, unused_imports)]
//! # Hashing — Comprehensive Guide
//!
//! Everything hash-related for system design and interviews:
//! - Hash function fundamentals (FNV, DJB2, MurmurHash, xxHash)
//! - Consistent hash ring (virtual nodes, weighted, replication)
//! - Rendezvous hashing (Highest Random Weight)
//! - Jump consistent hash (Google, O(1) memory)
//! - Maglev hashing (Google load balancer, O(1) lookup)
//! - Probabilistic structures (HyperLogLog, Count-Min Sketch, MinHash)
//! - Geohashing (spatial indexing)
//! - Merkle trees & content-addressable storage

mod consistent_ring;
mod geohash;
mod hash_functions;
mod jump_hash;
mod maglev;
mod merkle;
mod probabilistic;
mod rendezvous;

/// Demonstrates the consistent hashing behavior
fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║              HASHING — COMPREHENSIVE GUIDE                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // === 1. Hash Function Fundamentals ===
    section("1. HASH FUNCTION FUNDAMENTALS");
    hash_functions::demo();

    // === 2. Consistent Hash Ring ===
    section("2. CONSISTENT HASH RING");
    consistent_ring::demo();

    // === 3. Rendezvous Hashing ===
    section("3. RENDEZVOUS HASHING (HRW)");
    rendezvous::demo();

    // === 4. Jump Consistent Hash ===
    section("4. JUMP CONSISTENT HASH (Google)");
    jump_hash::demo();

    // === 5. Maglev Hashing ===
    section("5. MAGLEV HASHING (Google Load Balancer)");
    maglev::demo();

    // === 6. Probabilistic Hash Structures ===
    section("6. PROBABILISTIC (HyperLogLog, Count-Min Sketch, MinHash)");
    probabilistic::demo();

    // === 7. Geohashing ===
    section("7. GEOHASHING (Spatial Indexing)");
    geohash::demo();

    // === 8. Merkle Trees ===
    section("8. MERKLE TREE & CONTENT-ADDRESSABLE STORAGE");
    merkle::demo();

    println!("\n✓ All hashing demos complete!");
}

fn section(name: &str) {
    let sep = "=".repeat(64);
    println!("\n{sep}");
    println!("  {name}");
    println!("{sep}\n");
}


