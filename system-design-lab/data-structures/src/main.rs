#![allow(dead_code, unused_variables, unused_imports)]
//! # Data Structures
//!
//! Common data structures implemented from scratch in Rust.
//! Covers basic, thread-safe, lock-free, and advanced variants.

mod arena;
mod bloom_filter;
mod bst;
mod concurrent;
mod graph;
mod hash_map;
mod heap;
mod linked_list;
mod lock_free;
mod lru_lfu;
mod segment_tree;
mod skip_list;
mod stack_queue;
mod trie;
mod union_find;

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                  DATA STRUCTURES IN RUST                    ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // === Basic ===
    section("LINKED LIST");
    linked_list::demo();

    section("STACK / QUEUE / DEQUE");
    stack_queue::demo();

    section("HASH MAP");
    hash_map::demo();

    section("BINARY SEARCH TREE / AVL TREE");
    bst::demo();

    section("HEAP / PRIORITY QUEUE");
    heap::demo();

    section("TRIE");
    trie::demo();

    section("GRAPH");
    graph::demo();

    // === Thread-Safe ===
    section("THREAD-SAFE (Concurrent HashMap, Blocking Queue, Sorted Set)");
    concurrent::demo();

    // === Lock-Free ===
    section("LOCK-FREE (Treiber Stack, Atomic Counter, SPSC Queue)");
    lock_free::demo();

    // === Advanced ===
    section("LRU / LFU CACHE");
    lru_lfu::demo();

    section("BLOOM FILTER");
    bloom_filter::demo();

    section("SKIP LIST");
    skip_list::demo();

    section("UNION-FIND");
    union_find::demo();

    section("SEGMENT TREE / FENWICK TREE");
    segment_tree::demo();

    section("ARENA / OBJECT POOL / SLAB");
    arena::demo();

    println!("\n✓ All demos complete!");
}

fn section(name: &str) {
    let sep = "=".repeat(64);
    println!("\n{sep}");
    println!("  {name}");
    println!("{sep}\n");
}
