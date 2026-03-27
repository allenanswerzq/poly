// =============================================================================
// Paxos Consensus Protocol
//
//   The ORIGINAL consensus protocol (Lamport, 1989). Raft is basically
//   "Paxos made understandable" — same guarantees, friendlier decomposition.
//
//   Problem: N nodes must agree on a SINGLE value, even if some crash.
//   (In Multi-Paxos / replicated state machine: agree on a SEQUENCE of values.)
//
//   Three roles (a node can play multiple roles):
//     Proposer   — proposes a value, drives the protocol
//     Acceptor   — votes on proposals, stores accepted values
//     Learner    — learns the decided value (reads the result)
//
//   Two phases (for a single value):
//
//   Phase 1 — PREPARE (establish leadership):
//     Proposer picks a unique, monotonically increasing proposal number N.
//     Proposer → all Acceptors: "Prepare(N)"
//     Each Acceptor:
//       If N > highest_seen:
//         Promise: "I won't accept any proposal < N"
//         Reply with (highest_accepted_N, highest_accepted_value) if any
//       Else: reject (already promised to a higher N)
//
//   Phase 2 — ACCEPT (propose the value):
//     If Proposer got promises from MAJORITY:
//       If any acceptor already accepted a value → must use THAT value
//         (this is how Paxos preserves previously decided values)
//       Otherwise → propose its own value
//       Proposer → all Acceptors: "Accept(N, value)"
//     Each Acceptor:
//       If N >= highest_promised: accept it, store (N, value)
//       Else: reject
//
//   Decision: when MAJORITY of acceptors accept the same (N, value),
//   that value is DECIDED. It can never be changed.
//
//   Why it's hard:
//     - Multiple proposers can compete (dueling proposers)
//     - A proposer must adopt a previously accepted value
//     - Livelock: proposer A prepares N=1, proposer B prepares N=2,
//       A's accept rejected, A prepares N=3, B's accept rejected...
//       Fix: randomized backoff, or elect a distinguished proposer (leader)
//
//   Multi-Paxos: run Paxos for each slot in a log. The leader skips
//   Phase 1 for subsequent slots → effectively just Phase 2 = fast.
//   This is where Raft gets its "leader sends AppendEntries" from.
//
//   Paxos vs Raft:
//     Paxos: any node can propose, more flexible, harder to implement
//     Raft: strong leader, simpler mental model, easier to implement
//     Same safety guarantees. Raft is Paxos with training wheels.
//
//   Used by: Google Chubby, Google Spanner (variant), Apache ZooKeeper (ZAB ≈ Paxos)
// =============================================================================

/// A proposal is (proposal_number, value).
#[derive(Debug, Clone)]
struct Proposal {
    number: u64,
    value: String,
}

/// An Acceptor stores its promise and accepted state.
struct Acceptor {
    id: usize,
    highest_promised: u64,      // won't accept proposals below this
    accepted: Option<Proposal>, // highest accepted (number, value)
}

impl Acceptor {
    fn new(id: usize) -> Self {
        Self {
            id,
            highest_promised: 0,
            accepted: None,
        }
    }

    /// Phase 1: handle Prepare(n).
    /// Returns Ok((accepted_n, accepted_val)) if promise granted, Err if rejected.
    fn prepare(&mut self, proposal_n: u64) -> Result<Option<Proposal>, u64> {
        if proposal_n > self.highest_promised {
            self.highest_promised = proposal_n;
            Ok(self.accepted.clone()) // promise + return any accepted value
        } else {
            Err(self.highest_promised) // already promised to higher N
        }
    }

    /// Phase 2: handle Accept(n, value).
    /// Returns true if accepted, false if rejected.
    fn accept(&mut self, proposal: &Proposal) -> bool {
        if proposal.number >= self.highest_promised {
            self.highest_promised = proposal.number;
            self.accepted = Some(proposal.clone());
            true
        } else {
            false // promised to higher N
        }
    }
}

/// A Proposer drives Phase 1 and Phase 2.
struct Proposer {
    id: usize,
    proposal_counter: u64,
    num_proposers: u64, // for generating unique proposal numbers
}

impl Proposer {
    fn new(id: usize, num_proposers: u64) -> Self {
        Self {
            id,
            proposal_counter: 0,
            num_proposers,
        }
    }

    /// Generate a unique, monotonically increasing proposal number.
    /// We interleave: proposer 0 uses 0,3,6,... proposer 1 uses 1,4,7,...
    /// This guarantees uniqueness across proposers.
    fn next_proposal_number(&mut self) -> u64 {
        self.proposal_counter += 1;
        self.proposal_counter * self.num_proposers + self.id as u64
    }
}

pub fn demo() {
    println!("\n  ═══ Paxos ═══\n");

    let num_acceptors = 5;
    let majority = num_acceptors / 2 + 1;
    let mut acceptors: Vec<Acceptor> = (0..num_acceptors).map(Acceptor::new).collect();

    // ── Single-decree Paxos: agree on one value ──

    println!("    ── Single-decree: Proposer 0 proposes \"X\" ──\n");

    let mut proposer = Proposer::new(0, 2);
    let n = proposer.next_proposal_number();

    // Phase 1: Prepare
    println!("      Phase 1 — Prepare(n={}):", n);
    let mut promises = 0;
    let mut highest_accepted: Option<Proposal> = None;

    for acc in acceptors.iter_mut() {
        match acc.prepare(n) {
            Ok(prev) => {
                promises += 1;
                println!(
                    "        Acceptor {} → PROMISE (prev={:?})",
                    acc.id,
                    prev.as_ref().map(|p| format!("({},{})", p.number, p.value))
                );
                // Track highest previously accepted value
                if let Some(ref p) = prev {
                    if highest_accepted
                        .as_ref()
                        .is_none_or(|h| p.number > h.number)
                    {
                        highest_accepted = Some(p.clone());
                    }
                }
            }
            Err(higher) => {
                println!(
                    "        Acceptor {} → REJECT (promised to {})",
                    acc.id, higher
                );
            }
        }
    }

    if promises < majority {
        println!("      Failed to get majority promises!");
        return;
    }

    // Phase 2: Accept — MUST use previously accepted value if any
    let value = match highest_accepted {
        Some(ref prev) => {
            println!(
                "\n      Must adopt previously accepted value: {:?}",
                prev.value
            );
            prev.value.clone()
        }
        None => {
            println!("\n      No prior accepted value → propose my own");
            "X".to_string()
        }
    };

    let proposal = Proposal {
        number: n,
        value: value.clone(),
    };
    println!("      Phase 2 — Accept(n={}, value={:?}):", n, value);
    let mut accepts = 0;
    for acc in acceptors.iter_mut() {
        let ok = acc.accept(&proposal);
        if ok {
            accepts += 1;
        }
        println!(
            "        Acceptor {} → {}",
            acc.id,
            if ok { "ACCEPTED" } else { "REJECTED" }
        );
    }

    if accepts >= majority {
        println!(
            "\n      DECIDED: {:?} (majority={}/{})\n",
            value, accepts, num_acceptors
        );
    }

    // ── Dueling proposers: two proposers compete ──

    println!("    ── Dueling proposers: P0 proposes \"A\", P1 proposes \"B\" ──\n");

    let mut acceptors2: Vec<Acceptor> = (0..num_acceptors).map(Acceptor::new).collect();
    let mut p0 = Proposer::new(0, 2);
    let mut p1 = Proposer::new(1, 2);

    // P0 does Phase 1 with n=2
    let n0 = p0.next_proposal_number();
    println!("      P0: Prepare(n={})", n0);
    let mut promises0 = 0;
    for acc in acceptors2.iter_mut() {
        if acc.prepare(n0).is_ok() {
            promises0 += 1;
        }
    }
    println!("      P0: got {}/{} promises", promises0, num_acceptors);

    // P1 does Phase 1 with n=3 (HIGHER, overwrites P0's promises)
    let n1 = p1.next_proposal_number();
    println!("      P1: Prepare(n={}) — HIGHER than P0!", n1);
    let mut promises1 = 0;
    for acc in acceptors2.iter_mut() {
        if acc.prepare(n1).is_ok() {
            promises1 += 1;
        }
    }
    println!("      P1: got {}/{} promises", promises1, num_acceptors);

    // P0 tries Phase 2 with n=2 — REJECTED because acceptors promised n=3
    let prop0 = Proposal {
        number: n0,
        value: "A".to_string(),
    };
    let mut accepts0 = 0;
    for acc in acceptors2.iter_mut() {
        if acc.accept(&prop0) {
            accepts0 += 1;
        }
    }
    println!(
        "      P0: Accept(n={}, A) → {}/{} accepted (REJECTED — promised n={})",
        n0, accepts0, num_acceptors, n1
    );

    // P1 does Phase 2 with n=3 — SUCCEEDS
    let prop1 = Proposal {
        number: n1,
        value: "B".to_string(),
    };
    let mut accepts1 = 0;
    for acc in acceptors2.iter_mut() {
        if acc.accept(&prop1) {
            accepts1 += 1;
        }
    }
    println!(
        "      P1: Accept(n={}, B) → {}/{} accepted → DECIDED: \"B\"",
        n1, accepts1, num_acceptors
    );

    // P0 must retry with higher N — and will adopt "B"
    let n0_retry = p0.next_proposal_number();
    println!("\n      P0 retries: Prepare(n={})", n0_retry);
    let mut prev_accepted = None;
    let mut promises_retry = 0;
    for acc in acceptors2.iter_mut() {
        if let Ok(prev) = acc.prepare(n0_retry) {
            promises_retry += 1;
            if let Some(ref p) = prev {
                if prev_accepted
                    .as_ref()
                    .is_none_or(|h: &Proposal| p.number > h.number)
                {
                    prev_accepted = Some(p.clone());
                }
            }
        }
    }
    println!(
        "      P0: got {}/{} promises",
        promises_retry, num_acceptors
    );
    println!(
        "      P0 sees prev accepted: {:?} → must adopt it!",
        prev_accepted.as_ref().map(|p| &p.value)
    );
    println!("      P0 proposes \"B\" (not \"A\") — Paxos preserves the decision\n");
}
