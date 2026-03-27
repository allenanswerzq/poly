// =============================================================================
// Chain Replication
//
//   Problem: replicate data across N nodes with STRONG consistency
//   and HIGH throughput. Raft/Paxos are bottlenecked on the leader.
//
//   Key idea: arrange nodes in a CHAIN.
//     Head → Node_1 → Node_2 → ... → Tail
//
//   Write path: client → HEAD → propagates down the chain → TAIL acknowledges
//   Read path:  client → TAIL (always has the latest committed data)
//
//         ┌────────┐   ┌────────┐   ┌────────┐   ┌────────┐
//   W────►│  Head  ├──►│ Node 1 ├──►│ Node 2 ├──►│  Tail  │◄────R
//         └────────┘   └────────┘   └────────┘   └────────┘
//           writes                                  reads
//           propagate →→→→→→→→→→→→→→→→→→→→→→→→→→   ack
//
//   Why it's great:
//     - Reads are ALWAYS strongly consistent (tail = latest committed state)
//     - Read load is handled by tail alone → no leader bottleneck for reads
//     - Write throughput: pipeline effect — while entry N propagates down,
//       entry N+1 starts at the head. Overlapping in-flight writes.
//     - Simple failure handling: external master detects crashes,
//       removes the failed node from the chain, adjusts head/tail pointers.
//
//   Failure handling:
//     Head crashes  → next node becomes head
//     Tail crashes  → previous node becomes tail
//     Middle crashes → predecessor sends to successor (skip the dead node)
//     In all cases: a configuration manager (like ZooKeeper) updates the chain.
//
//   Trade-off vs Raft/Paxos:
//     - Latency: write must traverse ALL nodes (longer tail latency)
//     - Throughput: higher because writes pipeline through the chain
//     - Reads: strongly consistent without quorum (just read from tail)
//     - Requires external failure detector (can't self-heal like Raft)
//
//   Variants:
//     CRAQ (Chain Replication with Apportioned Queries):
//       ANY node can serve reads if its version is committed.
//       If node has uncommitted write → forward read to tail.
//       Distributes read load across all nodes, not just tail.
//
//   Used by: HDFS NameNode, Azure Storage, Facebook f4, MongoDB (internal)
// =============================================================================

use std::collections::HashMap;

/// A node in the chain. Each node knows its successor.
struct ChainNode {
    id: usize,
    role: ChainRole,
    store: HashMap<String, (String, u64)>, // key → (value, version)
    pending: Vec<WriteOp>,                 // writes not yet acknowledged by tail
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ChainRole {
    Head,
    Middle,
    Tail,
}

#[derive(Debug, Clone)]
struct WriteOp {
    key: String,
    value: String,
    version: u64,
}

impl ChainNode {
    fn new(id: usize, role: ChainRole) -> Self {
        Self {
            id,
            role,
            store: HashMap::new(),
            pending: Vec::new(),
        }
    }

    /// Process a write: store locally + return it for propagation to next node.
    fn process_write(&mut self, op: &WriteOp) -> bool {
        self.store
            .insert(op.key.clone(), (op.value.clone(), op.version));

        if self.role == ChainRole::Tail {
            // Tail = end of chain → write is now COMMITTED
            true
        } else {
            // Not tail → write is pending until tail confirms
            self.pending.push(op.clone());
            false
        }
    }

    /// Read: only tail serves reads (strongly consistent).
    fn read(&self, key: &str) -> Option<&(String, u64)> {
        assert_eq!(self.role, ChainRole::Tail, "Only tail serves reads!");
        self.store.get(key)
    }

    /// Mark a write as committed (confirmed by tail).
    fn commit(&mut self, version: u64) {
        self.pending.retain(|op| op.version > version);
    }
}

/// The chain: an ordered list of nodes.
struct Chain {
    nodes: Vec<ChainNode>,
    next_version: u64,
}

impl Chain {
    fn new(size: usize) -> Self {
        let mut nodes = Vec::new();
        for i in 0..size {
            let role = if i == 0 {
                ChainRole::Head
            } else if i == size - 1 {
                ChainRole::Tail
            } else {
                ChainRole::Middle
            };
            nodes.push(ChainNode::new(i, role));
        }
        Self {
            nodes,
            next_version: 0,
        }
    }

    /// Write: start at head, propagate through chain until tail commits.
    fn write(&mut self, key: &str, value: &str) -> u64 {
        self.next_version += 1;
        let op = WriteOp {
            key: key.to_string(),
            value: value.to_string(),
            version: self.next_version,
        };

        // Propagate through chain: head → ... → tail
        let mut committed = false;
        for i in 0..self.nodes.len() {
            committed = self.nodes[i].process_write(&op);
        }

        // If tail committed, notify all nodes upstream
        if committed {
            for node in self.nodes.iter_mut() {
                node.commit(self.next_version);
            }
        }
        self.next_version
    }

    /// Read: always from tail (strongly consistent).
    fn read(&self, key: &str) -> Option<&(String, u64)> {
        self.nodes.last().unwrap().read(key)
    }
}

pub fn demo() {
    println!("\n  ═══ Chain Replication ═══\n");

    let mut chain = Chain::new(4); // Head → N1 → N2 → Tail

    // ── Basic write + read ──

    println!("    ── Write path: Head → ... → Tail ──\n");

    let v = chain.write("user:1", r#"{"name":"Alice"}"#);
    println!(
        "      write user:1 → propagated through 4 nodes, version={}",
        v
    );

    let v = chain.write("user:2", r#"{"name":"Bob"}"#);
    println!(
        "      write user:2 → propagated through 4 nodes, version={}",
        v
    );

    println!("\n    ── Read path: always from Tail (strongly consistent) ──\n");

    if let Some((val, ver)) = chain.read("user:1") {
        println!("      read user:1 → {} (version={})", val, ver);
    }
    if let Some((val, ver)) = chain.read("user:2") {
        println!("      read user:2 → {} (version={})", val, ver);
    }

    // ── Show all nodes have the data ──

    println!("\n    ── All nodes store the data (for fault tolerance) ──\n");
    for node in &chain.nodes {
        println!(
            "      Node {} ({:?}): {} keys, pending={}",
            node.id,
            node.role,
            node.store.len(),
            node.pending.len()
        );
    }

    // ── Update: write same key, new value ──

    println!("\n    ── Update: write propagates new version ──\n");
    chain.write("user:1", r#"{"name":"Alicia","age":30}"#);
    let (val, ver) = chain.read("user:1").unwrap();
    println!("      updated user:1 → {} (version={})", val, ver);

    // ── Simulate tail failure ──

    println!("\n    ── Failure: tail crashes → previous node becomes tail ──\n");
    chain.nodes.pop(); // remove tail
    let new_tail = chain.nodes.last_mut().unwrap();
    new_tail.role = ChainRole::Tail; // promote
    println!("      Old tail (Node 3) removed");
    println!(
        "      Node {} promoted to Tail ({:?})",
        new_tail.id, new_tail.role
    );

    // Reads still work from new tail — data was already replicated
    if let Some((val, ver)) = chain.nodes.last().unwrap().store.get("user:1") {
        println!("      Read from new tail: user:1 → {} (v={})", val, ver);
    }
    println!("      No data lost — all nodes had the data\n");
}
