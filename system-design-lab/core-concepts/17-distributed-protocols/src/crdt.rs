use std::collections::{HashMap, HashSet};

// =============================================================================
// CRDTs — Conflict-Free Replicated Data Types
//
//   Problem: N replicas of the same data. Writes happen on any replica
//   without coordination. How to merge without conflicts?
//
//   CRDTs guarantee: if all replicas eventually receive all updates,
//   they converge to the SAME state — no matter the order of delivery.
//   No coordination, no locking, no consensus protocol needed.
//
//   Two families:
//     State-based (CvRDT): replicas send full state, merge with a function.
//       merge(a, b) must be commutative, associative, idempotent.
//     Op-based (CmRDT): replicas send operations, apply in any order.
//       Operations must be commutative (order doesn't matter).
//
//   Common CRDTs:
//
//     G-Counter (Grow-only counter):
//       Each node has its own slot. Increment = increment my slot.
//       Value = sum of all slots. Merge = element-wise max.
//       Can only grow. For decrement, use PN-Counter.
//
//     PN-Counter (Positive-Negative counter):
//       Two G-Counters: P (positive) and N (negative).
//       Increment = P.increment(). Decrement = N.increment().
//       Value = P.value() - N.value().
//
//     G-Set (Grow-only set):
//       Add items, never remove. Merge = union.
//
//     OR-Set (Observed-Remove set):
//       Each add gets a unique tag. Remove = remove specific tags.
//       If add and remove are concurrent, add wins (add-wins semantics).
//
//     LWW-Register (Last-Writer-Wins register):
//       Each write has a timestamp. Merge = keep highest timestamp.
//       Simple but lossy — concurrent writes, one silently dropped.
//
//   Used by: Redis CRDT module, Riak, Automerge, Figma, Apple Notes
// =============================================================================

// ─── G-Counter ──────────────────────────────────────────────────────────────

/// G-Counter: grow-only distributed counter.
/// Each node increments its own slot. Value = sum of all.
struct GCounter {
    counts: HashMap<String, u64>,
}

impl GCounter {
    fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    fn increment(&mut self, node_id: &str) {
        *self.counts.entry(node_id.to_string()).or_insert(0) += 1;
    }

    fn value(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Merge: element-wise max. Commutative + associative + idempotent.
    fn merge(&mut self, other: &GCounter) {
        for (node, &count) in &other.counts {
            let entry = self.counts.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(count);
        }
    }
}

// ─── PN-Counter ─────────────────────────────────────────────────────────────

/// PN-Counter: supports both increment and decrement.
/// Internally: P (positive G-Counter) - N (negative G-Counter).
struct PNCounter {
    p: GCounter, // positive increments
    n: GCounter, // negative increments (decrements)
}

impl PNCounter {
    fn new() -> Self {
        Self {
            p: GCounter::new(),
            n: GCounter::new(),
        }
    }

    fn increment(&mut self, node_id: &str) {
        self.p.increment(node_id);
    }

    fn decrement(&mut self, node_id: &str) {
        self.n.increment(node_id);
    }

    fn value(&self) -> i64 {
        self.p.value() as i64 - self.n.value() as i64
    }

    fn merge(&mut self, other: &PNCounter) {
        self.p.merge(&other.p);
        self.n.merge(&other.n);
    }
}

// ─── LWW-Register ───────────────────────────────────────────────────────────

/// LWW-Register: last writer wins, based on timestamp.
/// Simple but lossy: concurrent writes → one is silently dropped.
struct LWWRegister {
    value: String,
    timestamp: u64,
}

impl LWWRegister {
    fn new() -> Self {
        Self {
            value: String::new(),
            timestamp: 0,
        }
    }

    fn set(&mut self, value: &str, timestamp: u64) {
        if timestamp > self.timestamp {
            self.value = value.to_string();
            self.timestamp = timestamp;
        }
    }

    fn merge(&mut self, other: &LWWRegister) {
        if other.timestamp > self.timestamp {
            self.value = other.value.clone();
            self.timestamp = other.timestamp;
        }
    }
}

// ─── G-Set ──────────────────────────────────────────────────────────────────

/// G-Set: grow-only set. Add items, never remove. Merge = union.
struct GSet {
    items: HashSet<String>,
}

impl GSet {
    fn new() -> Self {
        Self {
            items: HashSet::new(),
        }
    }

    fn add(&mut self, item: &str) {
        self.items.insert(item.to_string());
    }

    fn merge(&mut self, other: &GSet) {
        for item in &other.items {
            self.items.insert(item.clone());
        }
    }

    fn len(&self) -> usize {
        self.items.len()
    }
}

pub fn demo() {
    println!("\n  ═══ CRDTs ═══\n");

    // ── G-Counter ──

    println!("    ── G-Counter (grow-only) ──\n");

    let mut counter_a = GCounter::new();
    let mut counter_b = GCounter::new();

    // Node A increments 3 times
    counter_a.increment("A");
    counter_a.increment("A");
    counter_a.increment("A");

    // Node B increments 2 times
    counter_b.increment("B");
    counter_b.increment("B");

    println!(
        "      Replica A: value={} (counts={:?})",
        counter_a.value(),
        counter_a.counts
    );
    println!(
        "      Replica B: value={} (counts={:?})",
        counter_b.value(),
        counter_b.counts
    );

    // Merge: both replicas converge
    counter_a.merge(&counter_b);
    counter_b.merge(&counter_a);
    println!("      After merge:");
    println!(
        "      Replica A: value={} (counts={:?})",
        counter_a.value(),
        counter_a.counts
    );
    println!(
        "      Replica B: value={} (counts={:?})",
        counter_b.value(),
        counter_b.counts
    );
    println!(
        "      Converged: {}\n",
        counter_a.value() == counter_b.value()
    );

    // ── PN-Counter ──

    println!("    ── PN-Counter (increment + decrement) ──\n");

    let mut pn_a = PNCounter::new();
    let mut pn_b = PNCounter::new();

    pn_a.increment("A");
    pn_a.increment("A");
    pn_a.increment("A"); // +3
    pn_b.decrement("B"); // -1

    println!("      Replica A: value={} (A did +3)", pn_a.value());
    println!("      Replica B: value={} (B did -1)", pn_b.value());

    pn_a.merge(&pn_b);
    pn_b.merge(&pn_a);
    println!(
        "      After merge: A={}, B={}, converged={}\n",
        pn_a.value(),
        pn_b.value(),
        pn_a.value() == pn_b.value()
    );

    // ── LWW-Register ──

    println!("    ── LWW-Register (last writer wins) ──\n");

    let mut reg_a = LWWRegister::new();
    let mut reg_b = LWWRegister::new();

    // Concurrent writes with different timestamps
    reg_a.set("alice", 100); // A writes at t=100
    reg_b.set("bob", 200); // B writes at t=200 (later wins)

    println!(
        "      Replica A: value={:?} (t={})",
        reg_a.value, reg_a.timestamp
    );
    println!(
        "      Replica B: value={:?} (t={})",
        reg_b.value, reg_b.timestamp
    );

    reg_a.merge(&reg_b);
    reg_b.merge(&reg_a);
    println!(
        "      After merge: A={:?}, B={:?} (highest timestamp wins)",
        reg_a.value, reg_b.value
    );
    println!("      Warning: Alice's write is silently lost!\n");

    // ── G-Set ──

    println!("    ── G-Set (grow-only set) ──\n");

    let mut set_a = GSet::new();
    let mut set_b = GSet::new();

    set_a.add("apple");
    set_a.add("banana");
    set_b.add("banana");
    set_b.add("cherry");

    println!("      Replica A: {:?}", set_a.items);
    println!("      Replica B: {:?}", set_b.items);

    set_a.merge(&set_b);
    set_b.merge(&set_a);
    println!("      After merge: A={:?} (union)", set_a.items);
    println!(
        "      Converged: {}, size={}\n",
        set_a.items == set_b.items,
        set_a.len()
    );
}
