// =============================================================================
// Lamport Clocks (Logical Clocks)
//
//   Problem: in a distributed system, nodes don't share a wall clock.
//   How to establish a TOTAL ORDER of events?
//
//   Lamport's key insight (1978):
//     "Time is defined by the order of events, not the other way around."
//
//   Rules:
//     1. Each process has a counter C, initialized to 0.
//     2. Before each local event: C += 1
//     3. On send: C += 1, attach C to the message.
//     4. On receive: C = max(C, received_C) + 1
//
//   Properties:
//     If A happened-before B → C(A) < C(B)        ← GUARANTEED
//     If C(A) < C(B) → A happened-before B?       ← NOT guaranteed!
//
//   This means: Lamport clocks give you a TOTAL ORDER that is CONSISTENT
//   with causality, but they can't DETECT concurrent events.
//   If C(A) < C(B), events could still be concurrent.
//
//   For detecting concurrency → use Vector Clocks (see vector_clock.rs).
//
//   Use cases:
//     - Generating globally unique, monotonically increasing IDs
//     - Total ordering of events for a replicated state machine
//     - Timestamp-based conflict resolution (simple but lossy)
//
//   Lamport clocks are the FOUNDATION of all distributed time concepts.
//   Vector clocks extend them. HLC (Hybrid Logical Clocks) combine them
//   with physical time for the best of both worlds.
// =============================================================================

/// A Lamport clock: a single monotonically increasing counter.
struct LamportClock {
    time: u64,
    #[allow(dead_code)]
    node_id: String,
}

impl LamportClock {
    fn new(node_id: &str) -> Self {
        Self {
            time: 0,
            node_id: node_id.to_string(),
        }
    }

    /// Local event: increment before each event.
    fn tick(&mut self) -> u64 {
        self.time += 1;
        self.time
    }

    /// Send a message: increment, return timestamp to attach.
    fn send(&mut self) -> u64 {
        self.time += 1;
        self.time
    }

    /// Receive a message: C = max(C, received) + 1
    fn receive(&mut self, received_time: u64) -> u64 {
        self.time = self.time.max(received_time) + 1;
        self.time
    }
}

/// An event with a Lamport timestamp.
#[derive(Debug)]
struct Event {
    node: String,
    description: String,
    lamport_time: u64,
}

pub fn demo() {
    println!("\n  ═══ Lamport Clocks ═══\n");

    let mut alice = LamportClock::new("Alice");
    let mut bob = LamportClock::new("Bob");
    let mut charlie = LamportClock::new("Charlie");
    let mut events: Vec<Event> = Vec::new();

    // ── Causal chain: Alice → Bob → Charlie ──

    println!("    ── Causal chain ──\n");

    // Alice does some local work
    let t = alice.tick();
    events.push(Event {
        node: "Alice".into(),
        description: "local work".into(),
        lamport_time: t,
    });
    println!("      Alice: local work          t={}", t);

    // Alice sends to Bob
    let send_t = alice.send();
    events.push(Event {
        node: "Alice".into(),
        description: "send to Bob".into(),
        lamport_time: send_t,
    });
    println!("      Alice: send to Bob         t={}", send_t);

    // Bob receives from Alice
    let recv_t = bob.receive(send_t);
    events.push(Event {
        node: "Bob".into(),
        description: "recv from Alice".into(),
        lamport_time: recv_t,
    });
    println!(
        "      Bob:   recv from Alice     t={} (max({}, {}) + 1)",
        recv_t, 0, send_t
    );

    // Bob does local work
    let t = bob.tick();
    events.push(Event {
        node: "Bob".into(),
        description: "local work".into(),
        lamport_time: t,
    });
    println!("      Bob:   local work          t={}", t);

    // Bob sends to Charlie
    let send_t = bob.send();
    events.push(Event {
        node: "Bob".into(),
        description: "send to Charlie".into(),
        lamport_time: send_t,
    });
    println!("      Bob:   send to Charlie     t={}", send_t);

    // Charlie receives from Bob
    let recv_t = charlie.receive(send_t);
    events.push(Event {
        node: "Charlie".into(),
        description: "recv from Bob".into(),
        lamport_time: recv_t,
    });
    println!("      Charlie: recv from Bob     t={}", recv_t);

    println!("\n      Total order: events sorted by Lamport time:");
    events.sort_by_key(|e| e.lamport_time);
    for e in &events {
        println!(
            "        t={}: {} — {}",
            e.lamport_time, e.node, e.description
        );
    }

    // ── Limitation: can't detect concurrency ──

    println!("\n    ── Limitation: false causality ──\n");

    let mut alice2 = LamportClock::new("Alice");
    let mut bob2 = LamportClock::new("Bob");

    // Alice and Bob both do local work independently (concurrent)
    let ta = alice2.tick(); // Alice: t=1
    let tb1 = bob2.tick(); // Bob: t=1
    let tb2 = bob2.tick(); // Bob: t=2

    println!("      Alice: local work  t={}", ta);
    println!("      Bob:   local work  t={}", tb1);
    println!("      Bob:   local work  t={}", tb2);
    println!();
    println!("      Alice t={} < Bob t={}", ta, tb2);
    println!("      Does this mean Alice happened-before Bob's second event?");
    println!("      NO! They are CONCURRENT (no message between them).");
    println!("      Lamport clocks can't tell the difference.");
    println!("      → Use Vector Clocks to detect concurrency.\n");
}
