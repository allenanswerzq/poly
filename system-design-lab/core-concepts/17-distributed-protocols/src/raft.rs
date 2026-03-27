use std::collections::HashMap;

// =============================================================================
// Raft Consensus Protocol
//
//   Problem: N servers must agree on a sequence of commands, even if some crash.
//
//   Key idea: elect a LEADER, all writes go through leader, leader replicates
//   to followers. If leader dies, elect a new one.
//
//   Three roles:
//     Follower  — passive, responds to leader/candidate RPCs
//     Candidate — trying to become leader (requests votes)
//     Leader    — handles all client requests, replicates log
//
//   Leader Election:
//     1. Followers have an election timeout (randomized 150-300ms)
//     2. If timeout fires without hearing from leader → become Candidate
//     3. Candidate increments term, votes for self, sends RequestVote to all
//     4. If majority votes → become Leader
//     5. Leader sends periodic heartbeats to prevent new elections
//
//     Election timeout is RANDOMIZED to avoid split votes:
//       If 2 candidates start simultaneously → neither gets majority → retry
//       Random timeouts make this unlikely (one almost always starts first)
//
//   Log Replication:
//     1. Client sends command to Leader
//     2. Leader appends to its log, sends AppendEntries to all followers
//     3. When MAJORITY acknowledges → entry is COMMITTED
//     4. Leader applies committed entry to state machine, responds to client
//
//     Commit rule: entry is committed when stored on majority of servers.
//     Even if leader crashes, the entry survives because majority has it.
//
//   Terms:
//     Monotonically increasing logical clock.
//     Each election increments the term.
//     If a server sees a higher term → it steps down to Follower.
//     Prevents stale leaders from making progress.
//
//   Safety properties:
//     - Election Safety: at most 1 leader per term
//     - Leader Append-Only: leader never overwrites its log
//     - Log Matching: if 2 logs have same index+term → identical up to that point
//     - Leader Completeness: committed entry appears in all future leaders' logs
//
//   Compared to Paxos:
//     - Raft is designed to be UNDERSTANDABLE (Paxos is notoriously hard)
//     - Raft decomposes into: leader election, log replication, safety
//     - Same theoretical guarantees, much easier to implement correctly
//
//   Used by: etcd, CockroachDB, TiKV, Consul, RethinkDB
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug, Clone)]
struct LogEntry {
    term: u64,
    command: String,
}

/// A single Raft node. In production these communicate over RPC.
/// Here they share a simulated "network" (HashMap of mailboxes).
struct RaftNode {
    id: usize,
    role: Role,
    current_term: u64,
    voted_for: Option<usize>, // who I voted for in current term
    log: Vec<LogEntry>,
    commit_index: usize,            // highest committed entry
    state: HashMap<String, String>, // applied state machine
}

impl RaftNode {
    fn new(id: usize) -> Self {
        Self {
            id,
            role: Role::Follower,
            current_term: 0,
            voted_for: None,
            log: Vec::new(),
            commit_index: 0,
            state: HashMap::new(),
        }
    }

    /// RequestVote RPC handler.
    /// Grant vote if: candidate's term >= mine AND I haven't voted yet in this term
    /// AND candidate's log is at least as up-to-date as mine.
    fn handle_request_vote(
        &mut self,
        candidate_id: usize,
        candidate_term: u64,
        candidate_last_log_term: u64,
        candidate_last_log_idx: usize,
    ) -> bool {
        // Step down if seeing higher term
        if candidate_term > self.current_term {
            self.current_term = candidate_term;
            self.role = Role::Follower;
            self.voted_for = None;
        }

        if candidate_term < self.current_term {
            return false; // stale candidate
        }

        // Already voted for someone else this term?
        if let Some(voted) = self.voted_for {
            if voted != candidate_id {
                return false;
            }
        }

        // Log up-to-date check (Section 5.4.1 of Raft paper):
        // Candidate's log must be at least as up-to-date as mine.
        let my_last_term = self.log.last().map_or(0, |e| e.term);
        let my_last_idx = self.log.len();
        let candidate_log_ok = candidate_last_log_term > my_last_term
            || (candidate_last_log_term == my_last_term && candidate_last_log_idx >= my_last_idx);

        if !candidate_log_ok {
            return false; // my log is more up-to-date
        }

        self.voted_for = Some(candidate_id);
        true
    }

    /// AppendEntries RPC handler (simplified).
    /// Returns true if entries were accepted.
    fn handle_append_entries(
        &mut self,
        leader_term: u64,
        entries: &[LogEntry],
        leader_commit: usize,
    ) -> bool {
        if leader_term < self.current_term {
            return false; // reject stale leader
        }

        // Recognize new leader
        self.current_term = leader_term;
        self.role = Role::Follower;
        self.voted_for = None;

        // Append new entries
        for entry in entries {
            self.log.push(entry.clone());
        }

        // Advance commit index
        if leader_commit > self.commit_index {
            self.commit_index = leader_commit.min(self.log.len());
            self.apply_committed();
        }
        true
    }

    /// Apply committed log entries to the state machine.
    fn apply_committed(&mut self) {
        for i in 0..self.commit_index {
            if let Some(entry) = self.log.get(i) {
                // Parse "SET key value" commands
                let parts: Vec<&str> = entry.command.split_whitespace().collect();
                if parts.len() == 3 && parts[0] == "SET" {
                    self.state
                        .insert(parts[1].to_string(), parts[2].to_string());
                }
            }
        }
    }
}

pub fn demo() {
    println!("\n  ═══ Raft ═══\n");

    let cluster_size = 5;
    let mut nodes: Vec<RaftNode> = (0..cluster_size).map(RaftNode::new).collect();

    // ── Leader Election ──
    //
    // Node 0's election timeout fires first → becomes Candidate.
    // It requests votes from all other nodes.
    //

    println!("    ── Leader Election ──\n");
    let candidate_id = 0;
    nodes[candidate_id].role = Role::Candidate;
    nodes[candidate_id].current_term += 1;
    nodes[candidate_id].voted_for = Some(candidate_id);
    let term = nodes[candidate_id].current_term;

    let mut votes = 1; // voted for self
    let candidate_last_term = nodes[candidate_id].log.last().map_or(0, |e| e.term);
    let candidate_last_idx = nodes[candidate_id].log.len();

    for i in 1..cluster_size {
        let granted = nodes[i].handle_request_vote(
            candidate_id,
            term,
            candidate_last_term,
            candidate_last_idx,
        );
        if granted {
            votes += 1;
        }
        println!(
            "      Node {} → vote for Node {}: {} (term={})",
            i,
            candidate_id,
            if granted { "YES" } else { "NO" },
            term
        );
    }

    let majority = cluster_size / 2 + 1;
    if votes >= majority {
        nodes[candidate_id].role = Role::Leader;
        println!(
            "\n      Node {} elected Leader (votes={}/{}, majority={})\n",
            candidate_id, votes, cluster_size, majority
        );
    }

    // ── Log Replication ──
    //
    // Leader receives client commands → appends to log → replicates to followers.
    // When majority acknowledges → committed → applied to state machine.
    //

    println!("    ── Log Replication ──\n");
    let commands = vec!["SET x 1", "SET y 2", "SET z 3"];

    for cmd in &commands {
        // Leader appends to its own log
        let entry = LogEntry {
            term,
            command: cmd.to_string(),
        };
        nodes[0].log.push(entry.clone());

        // Replicate to followers (in production: parallel AppendEntries RPCs)
        let mut acks = 1; // leader counts as 1
        let entries = &[entry.clone()];
        for i in 1..cluster_size {
            let ok = nodes[i].handle_append_entries(term, entries, 0);
            if ok {
                acks += 1;
            }
        }

        // Committed when majority acknowledges
        if acks >= majority {
            nodes[0].commit_index = nodes[0].log.len();
            nodes[0].apply_committed();
            println!(
                "      Replicate {:?}: acks={}/{} → COMMITTED",
                cmd, acks, cluster_size
            );
        }

        // Tell followers about new commit index
        let leader_commit = nodes[0].commit_index;
        for i in 1..cluster_size {
            nodes[i].handle_append_entries(term, &[], leader_commit);
        }
    }

    // Verify all nodes have the same state
    println!("\n      State after replication:");
    for node in &nodes {
        println!(
            "        Node {} ({:?}): log={} entries, committed={}, state={:?}",
            node.id,
            node.role,
            node.log.len(),
            node.commit_index,
            node.state
        );
    }

    // ── Leader Failure + Re-election ──
    //
    // Leader crashes → followers detect timeout → new election.
    //

    println!("\n    ── Leader Failure ──\n");
    println!("      Node 0 (Leader) crashes!");
    nodes[0].role = Role::Follower; // simulate crash

    // Node 2's timeout fires → becomes candidate for term 2
    let new_candidate = 2;
    nodes[new_candidate].current_term += 1;
    nodes[new_candidate].role = Role::Candidate;
    nodes[new_candidate].voted_for = Some(new_candidate);
    let new_term = nodes[new_candidate].current_term;
    let last_term = nodes[new_candidate].log.last().map_or(0, |e| e.term);
    let last_idx = nodes[new_candidate].log.len();

    let mut votes = 1;
    // Node 0 is crashed (doesn't respond), but we still have 4 alive nodes
    for i in [1, 3, 4] {
        let granted = nodes[i].handle_request_vote(new_candidate, new_term, last_term, last_idx);
        if granted {
            votes += 1;
        }
        println!(
            "      Node {} → vote for Node {}: {} (term={})",
            i,
            new_candidate,
            if granted { "YES" } else { "NO" },
            new_term
        );
    }
    if votes >= majority {
        nodes[new_candidate].role = Role::Leader;
        println!(
            "\n      Node {} elected new Leader (votes={}/{}, term={})",
            new_candidate,
            votes,
            cluster_size - 1,
            new_term
        );
        println!("      Cluster survived leader failure! No data lost.\n");
    }
}
