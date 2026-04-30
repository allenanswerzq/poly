//! # FDB-Style Deterministic Simulation Testing Demo
//!
//! This demo shows the core ideas behind FoundationDB's simulation testing:
//!
//! 1. **Abstraction layer**: All I/O goes through traits (INetwork, IDisk)
//! 2. **Two implementations**: Real (production) vs Simulated (testing)
//! 3. **Deterministic seed**: Same seed → same execution → reproducible bugs
//! 4. **BUGGIFY**: Fault injection points inside the actual code
//! 5. **Single-threaded event loop**: Discrete event simulation
//!
//! We simulate a tiny KV store with Raft-like replication across 3 nodes,
//! injecting crashes, network partitions, and disk corruption — all
//! deterministically controlled by a single seed number.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::{BTreeMap, HashMap, VecDeque};

// ============================================================================
// 1. THE ABSTRACTION LAYER — Traits that both real and sim implement
// ============================================================================

/// Abstraction over network — like FDB's INetwork
trait Network {
    fn send(&mut self, from: NodeId, to: NodeId, msg: Message);
    fn receive(&mut self, node: NodeId) -> Vec<(NodeId, Message)>;
}

/// Abstraction over disk — like FDB's IAsyncFile
trait Disk {
    fn write(&mut self, node: NodeId, key: &str, value: &str) -> DiskResult;
    fn read(&self, node: NodeId, key: &str) -> Option<String>;
    fn fsync(&mut self, node: NodeId) -> DiskResult;
}

type NodeId = usize;

#[derive(Debug, Clone)]
enum DiskResult {
    Ok,
    Corrupted, // BUGGIFY: partial write
    Failed,    // BUGGIFY: disk failure
}

#[derive(Debug, Clone)]
enum Message {
    /// Leader → Follower: replicate this entry
    AppendEntry { key: String, value: String, index: u64 },
    /// Follower → Leader: I have it
    Ack { index: u64 },
    /// Client → Leader: write request
    ClientWrite { key: String, value: String },
    /// Leader → Client: write result
    ClientResponse { success: bool, index: u64 },
}

// ============================================================================
// 2. SIMULATED IMPLEMENTATIONS — Controllable, deterministic fakes
// ============================================================================

/// Simulated network: in-memory message queues with fault injection
struct SimNetwork {
    /// Per-node inbox: (from, message)
    inboxes: HashMap<NodeId, VecDeque<(NodeId, Message)>>,
    /// Network partitions: (a, b) means a can't reach b
    partitions: Vec<(NodeId, NodeId)>,
    /// Delayed messages (deliver later)
    delayed: Vec<(u64, NodeId, NodeId, Message)>, // (deliver_at_tick, from, to, msg)
    rng: StdRng,
    tick: u64,
}

impl SimNetwork {
    fn new(nodes: &[NodeId], rng: StdRng) -> Self {
        let mut inboxes = HashMap::new();
        for &n in nodes {
            inboxes.insert(n, VecDeque::new());
        }
        Self {
            inboxes,
            partitions: Vec::new(),
            delayed: Vec::new(),
            rng,
            tick: 0,
        }
    }

    fn is_partitioned(&self, from: NodeId, to: NodeId) -> bool {
        self.partitions
            .iter()
            .any(|&(a, b)| (a == from && b == to) || (a == to && b == from))
    }

    fn add_partition(&mut self, a: NodeId, b: NodeId) {
        println!("    💥 PARTITION: node {} <-> node {} (can't communicate)", a, b);
        self.partitions.push((a, b));
    }

    fn heal_partition(&mut self, a: NodeId, b: NodeId) {
        println!("    ✅ HEAL: node {} <-> node {} (reconnected)", a, b);
        self.partitions.retain(|&(x, y)| !(x == a && y == b) && !(x == b && y == a));
    }

    fn advance_tick(&mut self) {
        self.tick += 1;
        // Deliver any delayed messages whose time has come
        let tick = self.tick;
        let ready: Vec<_> = self
            .delayed
            .iter()
            .filter(|(deliver_at, _, _, _)| *deliver_at <= tick)
            .cloned()
            .collect();
        self.delayed.retain(|(deliver_at, _, _, _)| *deliver_at > tick);
        for (_, from, to, msg) in ready {
            if !self.is_partitioned(from, to) {
                if let Some(inbox) = self.inboxes.get_mut(&to) {
                    inbox.push_back((from, msg));
                }
            }
        }
    }
}

impl Network for SimNetwork {
    fn send(&mut self, from: NodeId, to: NodeId, msg: Message) {
        // BUGGIFY: 10% chance of delaying the message
        if self.rng.gen_bool(0.1) {
            let delay = self.rng.gen_range(1..5);
            println!(
                "    🐌 BUGGIFY: delaying message from {} to {} by {} ticks",
                from, to, delay
            );
            self.delayed
                .push((self.tick + delay, from, to, msg));
            return;
        }

        // BUGGIFY: 5% chance of dropping the message entirely
        if self.rng.gen_bool(0.05) {
            println!(
                "    🗑️  BUGGIFY: dropped message from {} to {}",
                from, to
            );
            return;
        }

        if self.is_partitioned(from, to) {
            println!(
                "    🚫 PARTITIONED: message from {} to {} lost",
                from, to
            );
            return;
        }

        if let Some(inbox) = self.inboxes.get_mut(&to) {
            inbox.push_back((from, msg));
        }
    }

    fn receive(&mut self, node: NodeId) -> Vec<(NodeId, Message)> {
        self.inboxes
            .get_mut(&node)
            .map(|q| q.drain(..).collect())
            .unwrap_or_default()
    }
}

/// Simulated disk: in-memory storage with fault injection
struct SimDisk {
    /// Per-node storage
    storage: HashMap<NodeId, BTreeMap<String, String>>,
    /// Per-node WAL (append-only)
    wal: HashMap<NodeId, Vec<(String, String)>>,
    /// Crashed nodes (disk inaccessible)
    crashed: Vec<NodeId>,
    rng: StdRng,
}

impl SimDisk {
    fn new(nodes: &[NodeId], rng: StdRng) -> Self {
        let mut storage = HashMap::new();
        let mut wal = HashMap::new();
        for &n in nodes {
            storage.insert(n, BTreeMap::new());
            wal.insert(n, Vec::new());
        }
        Self {
            storage,
            wal,
            crashed: Vec::new(),
            rng,
        }
    }

    fn crash_node(&mut self, node: NodeId) {
        println!("    💀 CRASH: node {} disk is now inaccessible", node);
        self.crashed.push(node);
    }

    fn recover_node(&mut self, node: NodeId) {
        println!("    🔄 RECOVER: node {} replaying WAL...", node);
        self.crashed.retain(|&n| n != node);
        // Replay WAL to rebuild state (this is what real recovery does)
        if let Some(wal_entries) = self.wal.get(&node).cloned() {
            if let Some(store) = self.storage.get_mut(&node) {
                store.clear();
                for (k, v) in &wal_entries {
                    store.insert(k.clone(), v.clone());
                }
            }
            println!(
                "    🔄 RECOVER: node {} rebuilt {} keys from WAL",
                node,
                wal_entries.len()
            );
        }
    }
}

impl Disk for SimDisk {
    fn write(&mut self, node: NodeId, key: &str, value: &str) -> DiskResult {
        if self.crashed.contains(&node) {
            return DiskResult::Failed;
        }
        // BUGGIFY: 3% chance of corrupted write
        if self.rng.gen_bool(0.03) {
            println!(
                "    ⚡ BUGGIFY: corrupted write on node {} key={}",
                node, key
            );
            return DiskResult::Corrupted;
        }
        if let Some(store) = self.storage.get_mut(&node) {
            store.insert(key.to_string(), value.to_string());
        }
        DiskResult::Ok
    }

    fn read(&self, node: NodeId, key: &str) -> Option<String> {
        if self.crashed.contains(&node) {
            return None;
        }
        self.storage.get(&node)?.get(key).cloned()
    }

    fn fsync(&mut self, node: NodeId) -> DiskResult {
        if self.crashed.contains(&node) {
            return DiskResult::Failed;
        }
        // BUGGIFY: 2% chance fsync is slow (we just report it, no actual delay in sim)
        if self.rng.gen_bool(0.02) {
            println!("    🐢 BUGGIFY: slow fsync on node {}", node);
        }
        // Persist current state to WAL
        if let Some(store) = self.storage.get(&node) {
            let entries: Vec<_> = store.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            self.wal.insert(node, entries);
        }
        DiskResult::Ok
    }
}

// ============================================================================
// 3. THE NODE — A tiny Raft-like replicated KV store
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
enum Role {
    Leader,
    Follower,
}

struct Node {
    id: NodeId,
    role: Role,
    peers: Vec<NodeId>,
    /// The replicated log
    log: Vec<(String, String)>,
    /// How many nodes have acked each index
    ack_count: HashMap<u64, usize>,
    /// Commit index (majority have this)
    commit_index: u64,
    /// Applied state
    applied: BTreeMap<String, String>,
}

impl Node {
    fn new(id: NodeId, role: Role, peers: Vec<NodeId>) -> Self {
        Self {
            id,
            role,
            peers,
            log: Vec::new(),
            ack_count: HashMap::new(),
            commit_index: 0,
            applied: BTreeMap::new(),
        }
    }

    /// Process one tick: handle incoming messages, produce outgoing ones
    fn tick(
        &mut self,
        net: &mut dyn Network,
        disk: &mut dyn Disk,
    ) {
        let messages = net.receive(self.id);

        for (from, msg) in messages {
            match (&self.role, msg) {
                // Leader receives a client write
                (Role::Leader, Message::ClientWrite { key, value }) => {
                    let index = self.log.len() as u64 + 1;
                    println!(
                        "  [Node {}] Leader: received write {}={} → log index {}",
                        self.id, key, value, index
                    );

                    // Step 1: Append to our log
                    self.log.push((key.clone(), value.clone()));

                    // Step 2: Write to WAL + fsync
                    let wal_result = disk.write(self.id, &key, &value);
                    let sync_result = disk.fsync(self.id);
                    match (&wal_result, &sync_result) {
                        (DiskResult::Ok, DiskResult::Ok) => {}
                        _ => {
                            println!(
                                "  [Node {}] Leader: WAL write failed! wal={:?} sync={:?}",
                                self.id, wal_result, sync_result
                            );
                            net.send(
                                self.id,
                                from,
                                Message::ClientResponse { success: false, index },
                            );
                            continue;
                        }
                    }

                    // Step 3: Count our own ack
                    self.ack_count.insert(index, 1);

                    // Step 4: Send AppendEntry to all followers
                    for &peer in &self.peers {
                        net.send(
                            self.id,
                            peer,
                            Message::AppendEntry {
                                key: key.clone(),
                                value: value.clone(),
                                index,
                            },
                        );
                    }
                }

                // Leader receives ACK from follower
                (Role::Leader, Message::Ack { index }) => {
                    let count = self.ack_count.entry(index).or_insert(0);
                    *count += 1;
                    let total_nodes = self.peers.len() + 1;
                    let majority = total_nodes / 2 + 1;

                    if *count >= majority && index > self.commit_index {
                        println!(
                            "  [Node {}] Leader: index {} COMMITTED ({}/{} acks, majority={})",
                            self.id, index, *count, total_nodes, majority
                        );
                        self.commit_index = index;

                        // Apply to state machine
                        if let Some((k, v)) = self.log.get(index as usize - 1) {
                            self.applied.insert(k.clone(), v.clone());
                            println!(
                                "  [Node {}] Leader: APPLIED {}={} to state machine",
                                self.id, k, v
                            );
                        }

                        // Respond to client
                        net.send(
                            self.id,
                            0, // client = node 0 (convention)
                            Message::ClientResponse {
                                success: true,
                                index,
                            },
                        );
                    }
                }

                // Follower receives AppendEntry from leader
                (Role::Follower, Message::AppendEntry { key, value, index }) => {
                    println!(
                        "  [Node {}] Follower: received entry {} {}={}",
                        self.id, index, key, value
                    );

                    // Write to our WAL + fsync
                    let wal_result = disk.write(self.id, &key, &value);
                    let sync_result = disk.fsync(self.id);

                    match (&wal_result, &sync_result) {
                        (DiskResult::Ok, DiskResult::Ok) => {
                            self.log.push((key, value));
                            // Send ACK
                            net.send(self.id, from, Message::Ack { index });
                        }
                        _ => {
                            println!(
                                "  [Node {}] Follower: WAL write FAILED, not acking",
                                self.id
                            );
                        }
                    }
                }

                // Ignore misrouted messages
                _ => {}
            }
        }
    }
}

// ============================================================================
// 4. THE SIMULATOR — Single-threaded deterministic event loop
// ============================================================================

struct Simulator {
    seed: u64,
    nodes: Vec<Node>,
    net: SimNetwork,
    disk: SimDisk,
    tick: u64,
    client_responses: Vec<(bool, u64)>,
}

impl Simulator {
    fn new(seed: u64) -> Self {
        // Everything derives from the single seed
        let mut master_rng = StdRng::seed_from_u64(seed);
        let net_rng = StdRng::seed_from_u64(master_rng.gen());
        let disk_rng = StdRng::seed_from_u64(master_rng.gen());

        let node_ids = vec![1, 2, 3]; // 3-node cluster

        let nodes = vec![
            Node::new(1, Role::Leader, vec![2, 3]),
            Node::new(2, Role::Follower, vec![1, 3]),
            Node::new(3, Role::Follower, vec![1, 2]),
        ];

        let net = SimNetwork::new(&node_ids, net_rng);
        let disk = SimDisk::new(&node_ids, disk_rng);

        Self {
            seed,
            nodes,
            net,
            disk,
            tick: 0,
            client_responses: Vec::new(),
        }
    }

    /// Submit a client write to the leader
    fn client_write(&mut self, key: &str, value: &str) {
        println!("\n📝 CLIENT: write {}={}", key, value);
        self.net.send(
            0, // from client (node 0)
            1, // to leader (node 1)
            Message::ClientWrite {
                key: key.to_string(),
                value: value.to_string(),
            },
        );
    }

    /// Run one tick of the simulation
    fn run_tick(&mut self) {
        self.tick += 1;
        self.net.advance_tick();

        // Process each node (deterministic order: always 1, 2, 3)
        // We can't borrow self.nodes mutably while also using net/disk,
        // so we temporarily take ownership
        let mut nodes = std::mem::take(&mut self.nodes);
        for node in &mut nodes {
            node.tick(&mut self.net, &mut self.disk);
        }
        self.nodes = nodes;

        // Collect client responses
        let responses = self.net.receive(0); // client inbox
        for (_, msg) in responses {
            if let Message::ClientResponse { success, index } = msg {
                println!("📬 CLIENT: got response success={} index={}", success, index);
                self.client_responses.push((success, index));
            }
        }
    }

    /// Inject a network partition (BUGGIFY-style)
    fn inject_partition(&mut self, a: NodeId, b: NodeId) {
        self.net.add_partition(a, b);
    }

    fn heal_partition(&mut self, a: NodeId, b: NodeId) {
        self.net.heal_partition(a, b);
    }

    /// Crash a node's disk
    fn crash_node(&mut self, node: NodeId) {
        self.disk.crash_node(node);
    }

    /// Recover a node from WAL
    fn recover_node(&mut self, node: NodeId) {
        self.disk.recover_node(node);
    }

    /// Check invariants (like FDB's test harness)
    fn check_invariants(&self) -> bool {
        println!("\n🔍 CHECKING INVARIANTS...");
        let mut ok = true;

        // Invariant 1: All committed data must exist on the leader
        let leader = &self.nodes[0]; // node 1 = index 0
        for (success, index) in &self.client_responses {
            if *success {
                if *index as usize > leader.log.len() {
                    println!("  ❌ INVARIANT VIOLATION: committed index {} not in leader log", index);
                    ok = false;
                }
            }
        }

        // Invariant 2: Leader's applied state must be consistent with its log
        for (key, value) in &leader.applied {
            let in_log = leader.log.iter().any(|(k, v)| k == key && v == value);
            if !in_log {
                println!(
                    "  ❌ INVARIANT VIOLATION: applied {}={} not in log",
                    key, value
                );
                ok = false;
            }
        }

        // Invariant 3: No follower's commit_index should exceed leader's
        for node in &self.nodes[1..] {
            if node.commit_index > leader.commit_index {
                println!(
                    "  ❌ INVARIANT VIOLATION: follower {} commit_index {} > leader {}",
                    node.id, node.commit_index, leader.commit_index
                );
                ok = false;
            }
        }

        if ok {
            println!("  ✅ All invariants passed!");
        }
        ok
    }

    fn print_state(&self) {
        println!("\n📊 CLUSTER STATE at tick {}:", self.tick);
        for node in &self.nodes {
            println!(
                "  Node {} ({:?}): log={} committed={} applied={:?}",
                node.id,
                node.role,
                node.log.len(),
                node.commit_index,
                node.applied
            );
        }
    }
}

// ============================================================================
// 5. RUN THE SIMULATION
// ============================================================================

fn run_scenario(seed: u64) -> bool {
    println!("{}", "=".repeat(70));
    println!("🎲 SIMULATION SEED: {}", seed);
    println!("   (same seed = same execution = same bugs = reproducible)");
    println!("{}", "=".repeat(70));

    let mut sim = Simulator::new(seed);

    // --- Scenario: normal writes, then chaos ---

    // Phase 1: Normal writes
    println!("\n--- Phase 1: Normal writes ---");
    sim.client_write("alice_balance", "100");
    for _ in 0..5 {
        sim.run_tick();
    }

    sim.client_write("bob_balance", "200");
    for _ in 0..5 {
        sim.run_tick();
    }

    sim.print_state();

    // Phase 2: Network partition — isolate follower 3
    println!("\n--- Phase 2: Network partition (isolate node 3) ---");
    sim.inject_partition(1, 3);
    sim.inject_partition(2, 3);

    sim.client_write("alice_balance", "50"); // should still commit (2/3 majority)
    for _ in 0..5 {
        sim.run_tick();
    }

    sim.print_state();

    // Phase 3: Heal partition
    println!("\n--- Phase 3: Heal partition ---");
    sim.heal_partition(1, 3);
    sim.heal_partition(2, 3);
    for _ in 0..5 {
        sim.run_tick();
    }

    // Phase 4: Crash and recover a node
    println!("\n--- Phase 4: Crash node 2, write, then recover ---");
    sim.crash_node(2);

    sim.client_write("charlie_balance", "300");
    for _ in 0..5 {
        sim.run_tick();
    }

    sim.recover_node(2);
    for _ in 0..5 {
        sim.run_tick();
    }

    sim.print_state();

    // Check invariants
    sim.check_invariants()
}

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║  FDB-Style Deterministic Simulation Testing Demo               ║");
    println!("║                                                                ║");
    println!("║  Key ideas demonstrated:                                       ║");
    println!("║  • All I/O through trait abstractions (INetwork, IDisk)        ║");
    println!("║  • Simulated implementations with BUGGIFY fault injection      ║");
    println!("║  • Single-threaded event loop (deterministic)                  ║");
    println!("║  • Same seed = same execution = reproducible bugs              ║");
    println!("║  • Invariant checking after chaos                              ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");

    // Run with multiple seeds — like FDB's CI running millions of seeds
    let seeds = [42, 123, 7777, 99999, 314159];
    let mut passed = 0;
    let mut failed = 0;

    for &seed in &seeds {
        if run_scenario(seed) {
            passed += 1;
        } else {
            failed += 1;
            println!("🐛 BUG FOUND with seed {}! Re-run with this seed to reproduce.", seed);
        }
        println!();
    }

    println!("{}", "=".repeat(70));
    println!("📊 RESULTS: {}/{} scenarios passed", passed, seeds.len());
    if failed > 0 {
        println!(
            "🐛 {} failures — re-run with the failing seed to debug deterministically",
            failed
        );
    } else {
        println!("✅ All scenarios passed with injected chaos!");
    }
    println!("{}", "=".repeat(70));

    // Demonstrate reproducibility: run same seed twice
    println!("\n\n🔁 REPRODUCIBILITY TEST: running seed 42 twice...");
    println!("   (output should be IDENTICAL both times)\n");
    run_scenario(42);
    println!("\n--- Second run with same seed ---\n");
    run_scenario(42);
}
