use std::collections::HashMap;

// =============================================================================
// Vector Clocks
//
//   Problem: in a distributed system, events on different nodes have no
//   shared wall clock. How do you know if event A happened before event B?
//
//   Lamport timestamps give total ordering but can't detect CONCURRENCY.
//   Vector clocks solve this: they track causal relationships between events.
//
//   How it works:
//     Each node maintains a vector of counters, one per node.
//     vc[i] = "number of events I know about from node i"
//
//     On local event at node i:
//       vc[i] += 1
//
//     On send from node i:
//       vc[i] += 1
//       attach vc to the message
//
//     On receive at node j:
//       vc[j] += 1
//       for each k: vc[k] = max(vc[k], received_vc[k])
//
//   Comparing two vector clocks A and B:
//     A < B  (A happened-before B): all A[i] <= B[i], and at least one A[i] < B[i]
//     A > B  (B happened-before A): all A[i] >= B[i], and at least one A[i] > B[i]
//     A || B (concurrent):          neither A <= B nor B <= A
//
//   When events are concurrent → CONFLICT. Application must resolve
//   (e.g., last-writer-wins, merge function, or ask the user).
//
//   Used by: DynamoDB (variant), Riak, distributed databases for conflict detection
//
//   Limitation: vector size = O(number of nodes). For large clusters,
//   use dotted version vectors or interval tree clocks instead.
// =============================================================================

/// Vector clock: map of node_id → logical counter.
#[derive(Debug, Clone)]
struct VectorClock {
    clock: HashMap<String, u64>,
}

#[derive(Debug, PartialEq)]
enum Ordering {
    HappenedBefore, // A < B
    HappenedAfter,  // A > B
    Concurrent,     // A || B
    Equal,          // A == B
}

impl VectorClock {
    fn new() -> Self {
        Self {
            clock: HashMap::new(),
        }
    }

    /// Local event: increment my own counter.
    fn tick(&mut self, node_id: &str) {
        *self.clock.entry(node_id.to_string()).or_insert(0) += 1;
    }

    /// Merge with another clock (on message receive): element-wise max.
    fn merge(&mut self, other: &VectorClock) {
        for (node, &count) in &other.clock {
            let entry = self.clock.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(count);
        }
    }

    fn get(&self, node_id: &str) -> u64 {
        self.clock.get(node_id).copied().unwrap_or(0)
    }

    /// Compare two vector clocks.
    fn compare(&self, other: &VectorClock) -> Ordering {
        let all_keys: std::collections::HashSet<&String> =
            self.clock.keys().chain(other.clock.keys()).collect();

        let mut self_leq = true; // all self[k] <= other[k]?
        let mut other_leq = true; // all other[k] <= self[k]?

        for key in all_keys {
            let a = self.get(key);
            let b = other.get(key);
            if a > b {
                other_leq = false;
            }
            if b > a {
                self_leq = false;
            }
        }

        match (self_leq, other_leq) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::HappenedBefore, // self < other
            (false, true) => Ordering::HappenedAfter,  // self > other
            (false, false) => Ordering::Concurrent,    // conflict!
        }
    }
}

/// A node that uses vector clocks to track causality.
struct Node {
    id: String,
    vc: VectorClock,
    data: HashMap<String, (String, VectorClock)>, // key → (value, write_clock)
}

impl Node {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            vc: VectorClock::new(),
            data: HashMap::new(),
        }
    }

    /// Write a key: tick clock, store value with current clock.
    fn write(&mut self, key: &str, value: &str) -> VectorClock {
        self.vc.tick(&self.id);
        let vc = self.vc.clone();
        self.data
            .insert(key.to_string(), (value.to_string(), vc.clone()));
        vc
    }

    /// Send message: tick clock, return (data, clock) to attach to message.
    fn send(&mut self) -> VectorClock {
        self.vc.tick(&self.id);
        self.vc.clone()
    }

    /// Receive message: tick clock, merge with sender's clock.
    fn receive(&mut self, sender_vc: &VectorClock) {
        self.vc.tick(&self.id);
        self.vc.merge(sender_vc);
    }
}

pub fn demo() {
    println!("\n  ═══ Vector Clocks ═══\n");

    let mut alice = Node::new("A");
    let mut bob = Node::new("B");
    let mut charlie = Node::new("C");

    // ── Causal ordering ──
    //
    // Alice writes x=1, sends to Bob.
    // Bob receives, writes x=2 (causally after Alice).
    //

    println!("    ── Causal ordering ──\n");

    let vc1 = alice.write("x", "1");
    println!("      Alice writes x=1, vc={:?}", vc1.clock);

    let send_vc = alice.send();
    bob.receive(&send_vc);
    let vc2 = bob.write("x", "2");
    println!("      Alice sends to Bob");
    println!("      Bob writes x=2,   vc={:?}", vc2.clock);

    let order = vc1.compare(&vc2);
    println!("      Alice.x=1 vs Bob.x=2: {:?}", order);
    println!("      (Bob's write causally follows Alice's)\n");

    // ── Concurrent writes → conflict ──
    //
    // Alice writes x=10 (doesn't know about Bob).
    // Charlie writes x=20 (doesn't know about Alice).
    // These are concurrent: neither happened before the other.
    //

    println!("    ── Concurrent writes (conflict) ──\n");

    let vc_alice = alice.write("x", "10");
    let vc_charlie = charlie.write("x", "20");
    println!("      Alice writes x=10,   vc={:?}", vc_alice.clock);
    println!("      Charlie writes x=20, vc={:?}", vc_charlie.clock);

    let order = vc_alice.compare(&vc_charlie);
    println!("      Alice vs Charlie: {:?}", order);
    println!("      These are CONCURRENT — app must resolve conflict!");
    println!("      Options: last-writer-wins, merge function, or ask user\n");

    // ── Resolving conflict via merge ──
    //
    // Bob knows about both Alice and Charlie (received from both).
    // Bob's clock dominates both → his write resolves the conflict.
    //

    println!("    ── Conflict resolution via merge ──\n");

    bob.receive(&vc_alice);
    bob.receive(&vc_charlie);
    let vc_bob = bob.write("x", "resolved");
    println!("      Bob receives both, writes x=resolved");
    println!("      Bob's vc={:?}", vc_bob.clock);
    println!("      Bob > Alice: {:?}", vc_bob.compare(&vc_alice));
    println!("      Bob > Charlie: {:?}", vc_bob.compare(&vc_charlie));
    println!("      Bob's write causally follows both → conflict resolved\n");
}
