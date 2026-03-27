#![allow(clippy::needless_range_loop, clippy::cloned_ref_to_slice_refs)]
//! # Distributed Protocols — Runnable Demos
//!
//! Core consensus / coordination protocols, implemented from scratch.
//!
//! Demos:
//!   1. Raft — leader election + log replication
//!   2. Paxos — the original consensus protocol
//!   3. Two-Phase Commit (2PC) — atomic distributed transactions
//!   4. Saga Pattern — distributed transactions without 2PC
//!   5. Lamport Clocks — logical time, total ordering
//!   6. Vector Clocks — causal ordering, concurrency detection
//!   7. CRDTs — conflict-free replicated data types
//!   8. Gossip Protocol — epidemic information dissemination
//!   9. Chain Replication — strong consistency with high throughput
//!  10. Leases & Fencing Tokens — safe distributed locking

mod chain_replication;
mod crdt;
mod gossip;
mod lamport_clock;
mod lease;
mod paxos;
mod raft;
mod saga;
mod two_phase_commit;
mod vector_clock;

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║       Distributed Protocols — Demos              ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    println!("━━━ 1. Raft (Leader Election + Log Replication) ━━━");
    raft::demo();

    println!("━━━ 2. Paxos (Single-Decree + Dueling Proposers) ━━━");
    paxos::demo();

    println!("━━━ 3. Two-Phase Commit (2PC) ━━━");
    two_phase_commit::demo();

    println!("━━━ 4. Saga Pattern ━━━");
    saga::demo();

    println!("━━━ 5. Lamport Clocks ━━━");
    lamport_clock::demo();

    println!("━━━ 6. Vector Clocks ━━━");
    vector_clock::demo();

    println!("━━━ 7. CRDTs ━━━");
    crdt::demo();

    println!("━━━ 8. Gossip Protocol ━━━");
    gossip::demo();

    println!("━━━ 9. Chain Replication ━━━");
    chain_replication::demo();

    println!("━━━ 10. Leases & Fencing Tokens ━━━");
    lease::demo();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
