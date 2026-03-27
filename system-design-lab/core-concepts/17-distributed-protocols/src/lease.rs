// =============================================================================
// Leases and Fencing Tokens
//
//   Problem: distributed locks are DANGEROUS without fencing.
//
//   Naive distributed lock:
//     1. Client A acquires lock (SETNX in Redis, TTL 30s)
//     2. Client A does work... but pauses (GC, network delay)
//     3. Lock expires (30s TTL)
//     4. Client B acquires same lock, starts writing
//     5. Client A wakes up, ALSO writes → SPLIT BRAIN
//
//       Time: ──────────────────────────────────────────►
//       A:    [acquire lock]......[GC pause]......[writes!] ← STALE
//       B:              [lock expires] [acquire] [writes!]  ← VALID
//                                                 BOTH WRITE = CORRUPT
//
//   A LEASE is a time-bounded lock:
//     "You hold the lock for at most T seconds."
//     After T seconds, the lock is automatically released.
//     But this alone doesn't prevent the race above.
//
//   FENCING TOKEN: the real fix.
//     Each lease acquisition returns a monotonically increasing token.
//     The storage system REJECTS any write with a token ≤ the highest seen.
//
//     1. A acquires lease → token=33
//     2. A pauses (GC)
//     3. Lease expires, B acquires → token=34
//     4. B writes with token=34 → accepted
//     5. A wakes up, writes with token=33 → REJECTED (33 < 34)
//
//   This requires the STORAGE SYSTEM to enforce fencing:
//     if request.token < max_seen_token: reject
//     This is the key: the lock service alone can't prevent split-brain.
//     The resource being protected must also participate.
//
//   Implementation:
//     Lock service (ZooKeeper, etcd): returns incrementing fencing token
//     Storage (DB, file system): checks token before accepting writes
//
//   Used by: Google Chubby (leases), ZooKeeper (sequential znodes as tokens),
//            etcd (revision numbers), HBase (region server leases)
//
//   Martin Kleppmann's warning: Redis-based locks (Redlock) do NOT provide
//   fencing tokens. They're unsafe for correctness-critical operations.
//   Use ZooKeeper or etcd for fencing-safe distributed locks.
// =============================================================================

/// A lock service that issues leases with fencing tokens.
struct LeaseService {
    current_token: u64,
    holder: Option<String>, // who holds the lease
    lease_expiry: u64,      // simulated time
}

impl LeaseService {
    fn new() -> Self {
        Self {
            current_token: 0,
            holder: None,
            lease_expiry: 0,
        }
    }

    /// Acquire a lease. Returns fencing token if successful.
    fn acquire(&mut self, client: &str, current_time: u64) -> Option<u64> {
        // Check if current lease has expired
        if self.holder.is_some() && current_time < self.lease_expiry {
            return None; // someone else holds it
        }
        // Grant new lease with incremented token
        self.current_token += 1;
        self.holder = Some(client.to_string());
        self.lease_expiry = current_time + 30; // 30-second lease
        Some(self.current_token)
    }

    #[allow(dead_code)]
    fn release(&mut self) {
        self.holder = None;
    }
}

/// A storage system that enforces fencing tokens.
struct FencedStorage {
    data: std::collections::HashMap<String, String>,
    max_token: u64,                              // highest token seen
    write_log: Vec<(String, String, u64, bool)>, // (client, key, token, accepted)
}

impl FencedStorage {
    fn new() -> Self {
        Self {
            data: std::collections::HashMap::new(),
            max_token: 0,
            write_log: Vec::new(),
        }
    }

    /// Write with fencing: reject if token is stale.
    fn write(&mut self, client: &str, key: &str, value: &str, token: u64) -> bool {
        if token < self.max_token {
            // FENCED: stale token, reject the write
            self.write_log
                .push((client.to_string(), key.to_string(), token, false));
            false
        } else {
            self.max_token = token;
            self.data.insert(key.to_string(), value.to_string());
            self.write_log
                .push((client.to_string(), key.to_string(), token, true));
            true
        }
    }
}

pub fn demo() {
    println!("\n  ═══ Leases & Fencing Tokens ═══\n");

    // ── WITHOUT fencing: split-brain ──

    println!("    ── Without fencing: SPLIT-BRAIN ──\n");
    {
        let mut lock = LeaseService::new();
        let mut storage = std::collections::HashMap::<String, String>::new();

        // A acquires lock at time=0
        let token_a = lock.acquire("A", 0).unwrap();
        println!("      t=0:  A acquires lock (token={})", token_a);

        // A pauses (GC), lock expires at t=30
        println!("      t=10: A pauses (GC)...");

        // B acquires lock at t=35 (A's lease expired)
        let token_b = lock.acquire("B", 35).unwrap();
        println!(
            "      t=35: A's lease expired. B acquires lock (token={})",
            token_b
        );

        // B writes
        storage.insert("account".to_string(), "B-value".to_string());
        println!("      t=36: B writes account=B-value ✓");

        // A wakes up, still thinks it holds the lock, writes
        storage.insert("account".to_string(), "A-value-STALE".to_string());
        println!("      t=40: A wakes up, writes account=A-value-STALE ← SPLIT-BRAIN!");
        println!(
            "      Final: account={:?} (WRONG — B's write was overwritten)\n",
            storage.get("account").unwrap()
        );
    }

    // ── WITH fencing: stale write rejected ──

    println!("    ── With fencing: stale write REJECTED ──\n");
    {
        let mut lock = LeaseService::new();
        let mut storage = FencedStorage::new();

        // A acquires lock at time=0
        let token_a = lock.acquire("A", 0).unwrap();
        println!("      t=0:  A acquires lock (token={})", token_a);

        // A pauses (GC), lock expires at t=30
        println!("      t=10: A pauses (GC)...");

        // B acquires lock at t=35
        let token_b = lock.acquire("B", 35).unwrap();
        println!(
            "      t=35: A's lease expired. B acquires lock (token={})",
            token_b
        );

        // B writes with token=2
        let ok = storage.write("B", "account", "B-value", token_b);
        println!(
            "      t=36: B writes account=B-value (token={}) → {}",
            token_b,
            if ok { "ACCEPTED ✓" } else { "REJECTED" }
        );

        // A wakes up, writes with token=1 — FENCED!
        let ok = storage.write("A", "account", "A-value-STALE", token_a);
        println!(
            "      t=40: A writes account=A-value-STALE (token={}) → {}",
            token_a,
            if ok {
                "ACCEPTED"
            } else {
                "REJECTED (fenced!) ✓"
            }
        );

        println!(
            "      Final: account={:?} (CORRECT — B's write preserved)",
            storage.data.get("account").unwrap()
        );

        println!("\n      Write log:");
        for (client, key, token, accepted) in &storage.write_log {
            println!(
                "        {} wrote {} (token={}) → {}",
                client,
                key,
                token,
                if *accepted { "accepted" } else { "FENCED" }
            );
        }
    }
    println!();
}
