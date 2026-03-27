use std::collections::HashMap;

// =============================================================================
// Two-Phase Commit (2PC)
//
//   Problem: N databases must ALL commit or ALL abort a transaction.
//            e.g., transfer $100: bank_A -= 100, bank_B += 100.
//            If bank_A commits but bank_B aborts → money vanishes.
//
//   Solution: a COORDINATOR asks all PARTICIPANTS to prepare, then commit.
//
//   Phase 1 — PREPARE (voting):
//     Coordinator → each Participant: "Can you commit tx 42?"
//     Each Participant:
//       - Acquires locks, writes to WAL (write-ahead log)
//       - Responds YES (I can commit) or NO (I must abort)
//
//   Phase 2 — COMMIT/ABORT (decision):
//     If ALL voted YES:
//       Coordinator → each Participant: "COMMIT tx 42"
//       Participants apply changes, release locks.
//     If ANY voted NO:
//       Coordinator → each Participant: "ABORT tx 42"
//       Participants roll back, release locks.
//
//   The coordinator's decision is DURABLE (written to its own log before sending).
//
//   Failure scenarios:
//     1. Participant crashes before voting → Coordinator times out → ABORT
//     2. Participant crashes after YES → On recovery, checks coordinator for decision
//     3. Coordinator crashes after collecting votes → BLOCKING PROBLEM:
//        Participants who voted YES are STUCK holding locks, waiting.
//        This is the fundamental weakness of 2PC.
//
//   The blocking problem:
//     If coordinator crashes between Phase 1 and Phase 2, participants
//     can't decide on their own (they don't know how others voted).
//     They must wait for coordinator to recover. Locks held the entire time.
//     Fix: 3PC (Three-Phase Commit) adds a PRE-COMMIT phase, but it's
//     rarely used in practice. Instead, use timeouts + manual intervention.
//
//   Used by: MySQL XA transactions, distributed databases, Spanner (variant)
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
enum Vote {
    Yes,
    No,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Decision {
    Commit,
    Abort,
}

/// A participant in a 2PC transaction (e.g., a database shard).
struct Participant {
    name: String,
    data: HashMap<String, i64>,
    prepared: bool,  // did I vote YES?
    will_fail: bool, // simulate failure
}

impl Participant {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            data: HashMap::new(),
            prepared: false,
            will_fail: false,
        }
    }

    /// Phase 1: Coordinator asks "can you prepare this transaction?"
    /// Participant checks constraints, acquires locks, writes to WAL.
    fn prepare(&mut self, key: &str, delta: i64) -> Vote {
        if self.will_fail {
            println!("      {} → VOTE NO (simulated failure)", self.name);
            return Vote::No;
        }

        let current = self.data.get(key).copied().unwrap_or(0);
        if current + delta < 0 {
            println!(
                "      {} → VOTE NO (insufficient balance: {})",
                self.name, current
            );
            return Vote::No; // constraint violation
        }

        self.prepared = true;
        println!(
            "      {} → VOTE YES (balance {} → {})",
            self.name,
            current,
            current + delta
        );
        Vote::Yes
    }

    /// Phase 2: Apply or rollback based on coordinator's decision.
    fn decide(&mut self, key: &str, delta: i64, decision: Decision) {
        match decision {
            Decision::Commit => {
                if self.prepared {
                    let balance = self.data.entry(key.to_string()).or_insert(0);
                    *balance += delta;
                    println!("      {} → COMMITTED (balance={})", self.name, balance);
                }
            }
            Decision::Abort => {
                self.prepared = false;
                println!("      {} → ABORTED (rolled back)", self.name);
            }
        }
    }
}

/// The coordinator drives the 2PC protocol.
struct Coordinator {
    participants: Vec<Participant>,
}

impl Coordinator {
    fn new(participants: Vec<Participant>) -> Self {
        Self { participants }
    }

    /// Execute a distributed transaction across all participants.
    /// Each participant applies (key, delta) — all commit or all abort.
    fn execute(&mut self, operations: &[(&str, &str, i64)]) -> Decision {
        // Phase 1: collect votes
        println!("\n      Phase 1 — PREPARE:");
        let mut votes = Vec::new();
        for (participant_name, key, delta) in operations {
            let p = self
                .participants
                .iter_mut()
                .find(|p| p.name == *participant_name)
                .unwrap();
            votes.push(p.prepare(key, *delta));
        }

        // Decision: commit only if ALL voted YES
        let decision = if votes.iter().all(|v| *v == Vote::Yes) {
            Decision::Commit
        } else {
            Decision::Abort
        };
        println!("\n      Decision: {:?}", decision);

        // Phase 2: apply decision
        println!("\n      Phase 2 — {:?}:", decision);
        for (participant_name, key, delta) in operations {
            let p = self
                .participants
                .iter_mut()
                .find(|p| p.name == *participant_name)
                .unwrap();
            p.decide(key, *delta, decision);
        }

        decision
    }
}

pub fn demo() {
    println!("\n  ═══ Two-Phase Commit ═══\n");

    // ── Successful transaction: transfer $100 from A to B ──

    println!("    ── Transaction 1: Transfer $100 (A → B) ──");

    let mut bank_a = Participant::new("Bank_A");
    let mut bank_b = Participant::new("Bank_B");
    bank_a.data.insert("balance".to_string(), 500);
    bank_b.data.insert("balance".to_string(), 200);

    println!(
        "      Initial: A={}, B={}",
        bank_a.data["balance"], bank_b.data["balance"]
    );

    let mut coord = Coordinator::new(vec![bank_a, bank_b]);
    let result = coord.execute(&[
        ("Bank_A", "balance", -100), // A loses 100
        ("Bank_B", "balance", 100),  // B gains 100
    ]);

    println!(
        "      Result: {:?}, A={}, B={}\n",
        result, coord.participants[0].data["balance"], coord.participants[1].data["balance"]
    );

    // ── Failed transaction: B crashes → all abort ──

    println!("    ── Transaction 2: B crashes → ABORT ──");

    let mut bank_a2 = Participant::new("Bank_A");
    let mut bank_b2 = Participant::new("Bank_B");
    bank_a2.data.insert("balance".to_string(), 500);
    bank_b2.data.insert("balance".to_string(), 200);
    bank_b2.will_fail = true; // simulate crash

    println!(
        "      Initial: A={}, B={} (B will crash)",
        bank_a2.data["balance"], bank_b2.data["balance"]
    );

    let mut coord2 = Coordinator::new(vec![bank_a2, bank_b2]);
    let result2 = coord2.execute(&[("Bank_A", "balance", -100), ("Bank_B", "balance", 100)]);

    println!(
        "      Result: {:?}, A={}, B={} (no money lost!)\n",
        result2, coord2.participants[0].data["balance"], coord2.participants[1].data["balance"]
    );

    // ── Failed transaction: insufficient funds → constraint violation ──

    println!("    ── Transaction 3: Insufficient funds → ABORT ──");

    let mut bank_a3 = Participant::new("Bank_A");
    let mut bank_b3 = Participant::new("Bank_B");
    bank_a3.data.insert("balance".to_string(), 50); // only $50
    bank_b3.data.insert("balance".to_string(), 200);

    println!(
        "      Initial: A={}, B={} (A can't afford -100)",
        bank_a3.data["balance"], bank_b3.data["balance"]
    );

    let mut coord3 = Coordinator::new(vec![bank_a3, bank_b3]);
    let result3 = coord3.execute(&[("Bank_A", "balance", -100), ("Bank_B", "balance", 100)]);

    println!(
        "      Result: {:?}, A={}, B={}\n",
        result3, coord3.participants[0].data["balance"], coord3.participants[1].data["balance"]
    );
}
