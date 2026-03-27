//! # Data Modeling Demo
//!
//! Demonstrates core data modeling concepts:
//! 1. Normalization vs Denormalization — same data, different tradeoffs
//! 2. SQL-style relational model with foreign keys
//! 3. NoSQL document store (flexible schema)
//! 4. Key-value store (fast lookups)
//! 5. Polymorphic associations (comments on different entity types)
//! 6. Hierarchical data (adjacency list + materialized path)
//! 7. Event sourcing (append-only log → derive state)
//! 8. Indexing effects on read/write performance

mod denormalized;
mod document_store;
mod event_sourcing;
mod hierarchical;
mod indexing;
mod key_value;
mod normalized;
mod polymorphic;

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║       Data Modeling — Core Concepts              ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    println!("━━━ 1. Normalized (SQL) Model ━━━");
    normalized::demo();

    println!("━━━ 2. Denormalized Model ━━━");
    denormalized::demo();

    println!("━━━ 3. Document Store (NoSQL) ━━━");
    document_store::demo();

    println!("━━━ 4. Key-Value Store ━━━");
    key_value::demo();

    println!("━━━ 5. Polymorphic Associations ━━━");
    polymorphic::demo();

    println!("━━━ 6. Hierarchical Data ━━━");
    hierarchical::demo();

    println!("━━━ 7. Event Sourcing ━━━");
    event_sourcing::demo();

    println!("━━━ 8. Indexing Effects ━━━");
    indexing::demo();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
