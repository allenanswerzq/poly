use rand::Rng;
use std::collections::{HashMap, HashSet};

// =============================================================================
// Gossip Protocol (Epidemic Protocol)
//
//   Problem: N nodes need to share information (membership, state, etc.)
//   without a central coordinator. Must tolerate node failures.
//
//   Key idea: each node periodically picks a RANDOM peer and exchanges info.
//   Information spreads like a virus — exponentially fast.
//
//   How it works:
//     Every T seconds, each node:
//       1. Pick a random peer
//       2. Send my state (or delta) to that peer
//       3. Peer merges with its own state
//
//   Convergence speed:
//     With N nodes, after O(log N) rounds, ALL nodes have the information.
//     100 nodes → ~7 rounds. 1 million nodes → ~20 rounds.
//     This is provably optimal for epidemic spread.
//
//   Variants:
//     Push: I send my state to a random peer.
//     Pull: I ask a random peer for their state.
//     Push-Pull: both directions in one exchange (faster convergence).
//
//   Failure detection (SWIM-style):
//     Each node maintains a membership list with heartbeat counters.
//     On gossip: exchange heartbeats. If a node's heartbeat hasn't
//     increased for T_fail seconds → mark it as SUSPECTED → then DEAD.
//
//   Properties:
//     - Scalable: O(log N) convergence, O(1) per-node message load
//     - Fault-tolerant: works even with many node failures
//     - Eventually consistent: all nodes converge, but NOT instantly
//     - No single point of failure
//
//   Used by: Cassandra (membership), DynamoDB (ring state), Consul (Serf),
//            Bitcoin (transaction propagation), Redis Cluster (gossip bus)
// =============================================================================

/// A node in the gossip network. Knows some key-value pairs.
struct GossipNode {
    id: usize,
    data: HashMap<String, (String, u64)>, // key → (value, version)
    known_peers: HashSet<usize>,
}

impl GossipNode {
    fn new(id: usize, peers: &[usize]) -> Self {
        Self {
            id,
            data: HashMap::new(),
            known_peers: peers.iter().copied().collect(),
        }
    }

    /// Write a key-value pair locally.
    fn write(&mut self, key: &str, value: &str, version: u64) {
        self.data
            .insert(key.to_string(), (value.to_string(), version));
    }

    /// Pick a random peer to gossip with.
    fn pick_random_peer(&self, rng: &mut impl Rng) -> Option<usize> {
        let peers: Vec<usize> = self.known_peers.iter().copied().collect();
        if peers.is_empty() {
            return None;
        }
        Some(peers[rng.gen_range(0..peers.len())])
    }

    /// Prepare state to send (push gossip).
    fn get_state(&self) -> HashMap<String, (String, u64)> {
        self.data.clone()
    }

    /// Merge received state: keep the HIGHER version for each key.
    fn merge(&mut self, other_state: &HashMap<String, (String, u64)>) -> u32 {
        let mut updates = 0;
        for (key, (value, version)) in other_state {
            let dominated = match self.data.get(key) {
                Some((_, my_ver)) => *version > *my_ver,
                None => true,
            };
            if dominated {
                self.data.insert(key.clone(), (value.clone(), *version));
                updates += 1;
            }
        }
        updates
    }
}

pub fn demo() {
    println!("\n  ═══ Gossip Protocol ═══\n");

    let n = 10;
    let all_peers: Vec<usize> = (0..n).collect();

    // Create N nodes, each knows all others
    let mut nodes: Vec<GossipNode> = (0..n)
        .map(|id| {
            let peers: Vec<usize> = all_peers.iter().copied().filter(|&p| p != id).collect();
            GossipNode::new(id, &peers)
        })
        .collect();

    // ── Inject information at just 1 node ──

    nodes[0].write("config:version", "v2.5.0", 1);
    nodes[0].write("leader", "node-7", 1);
    nodes[0].write("feature:flag-X", "enabled", 1);

    println!("    Node 0 has 3 keys. Others have 0.\n");

    // ── Run gossip rounds ──
    //
    // Each round: every node picks 1 random peer, pushes its state.
    //

    let mut rng = rand::thread_rng();

    for round in 1..=8 {
        // Collect all push-pull pairs for this round
        let mut exchanges: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            if let Some(peer) = nodes[i].pick_random_peer(&mut rng) {
                exchanges.push((i, peer));
            }
        }

        // Execute exchanges (push-pull: both sides merge)
        let mut round_updates = 0;
        for (sender, receiver) in &exchanges {
            let sender_state = nodes[*sender].get_state();
            let receiver_state = nodes[*receiver].get_state();
            round_updates += nodes[*receiver].merge(&sender_state);
            round_updates += nodes[*sender].merge(&receiver_state);
        }

        // Count how many nodes have all 3 keys
        let informed: usize = nodes.iter().filter(|node| node.data.len() == 3).count();

        println!(
            "      Round {}: {}/{} nodes informed, {} updates",
            round, informed, n, round_updates
        );

        if informed == n {
            println!(
                "      All nodes converged in {} rounds! (O(log {}) = {:.1})\n",
                round,
                n,
                (n as f64).ln() / 2.0_f64.ln()
            );
            break;
        }
    }

    // ── Verify convergence ──

    println!("    Final state across all nodes:\n");
    for node in &nodes {
        let keys: Vec<&String> = node.data.keys().collect();
        println!(
            "      Node {}: {} keys {:?}",
            node.id,
            node.data.len(),
            keys.iter().map(|k| k.as_str()).collect::<Vec<_>>()
        );
    }

    // Check all nodes have identical state
    let reference = &nodes[0].data;
    let all_match = nodes.iter().all(|n| n.data == *reference);
    println!("\n      All nodes identical: {}", all_match);

    // ── Demonstrate update propagation ──

    println!("\n    ── Update propagation ──\n");
    println!("      Node 5 writes config:version=v3.0.0 (version 2)");
    nodes[5].write("config:version", "v3.0.0", 2);

    for round in 1..=6 {
        let mut exchanges: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            if let Some(peer) = nodes[i].pick_random_peer(&mut rng) {
                exchanges.push((i, peer));
            }
        }
        for (sender, receiver) in &exchanges {
            let sender_state = nodes[*sender].get_state();
            let receiver_state = nodes[*receiver].get_state();
            nodes[*receiver].merge(&sender_state);
            nodes[*sender].merge(&receiver_state);
        }

        let updated: usize = nodes
            .iter()
            .filter(|node| {
                node.data
                    .get("config:version")
                    .is_some_and(|(v, _)| v == "v3.0.0")
            })
            .count();

        println!("      Round {}: {}/{} nodes see v3.0.0", round, updated, n);
        if updated == n {
            break;
        }
    }
    println!();
}
