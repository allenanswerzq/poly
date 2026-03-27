use std::collections::HashMap;

// =============================================================================
// Hash-Based Sharding
//
//   The simplest sharding strategy:
//     shard_id = hash(key) % num_shards
//
//   How it works:
//     1. Take the shard key (e.g., user_id)
//     2. Hash it to a large number
//     3. Modulo by number of shards → shard index
//
//   Properties:
//     + Even distribution (if hash is good)
//     + O(1) routing — no lookup table needed
//     + Stateless — any app server can compute the shard
//
//     - Adding/removing shards remaps ~(N-1)/N of all keys
//       (4 shards → 5 shards: ~80% of keys move!)
//     - No range queries across shards
//     - Hot keys still land on one shard (one viral user)
//
//   Fix for the remapping problem: consistent hashing (see 06-consistent-hashing)
//
//   Used by: Redis Cluster (CRC16 % 16384 slots), Memcached, Cassandra (Murmur3)
// =============================================================================

/// A simulated database shard — just an in-memory HashMap.
struct Shard {
    id: usize,
    data: HashMap<String, String>,
}

impl Shard {
    fn new(id: usize) -> Self {
        Self {
            id,
            data: HashMap::new(),
        }
    }
}

/// Hash-based shard router: hash(key) % num_shards.
struct HashRouter {
    shards: Vec<Shard>,
}

impl HashRouter {
    fn new(num_shards: usize) -> Self {
        let shards = (0..num_shards).map(Shard::new).collect();
        Self { shards }
    }

    /// FNV-1a hash — fast, good distribution.
    fn hash(key: &str) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for byte in key.bytes() {
            h ^= byte as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    /// Route a key to a shard: hash(key) % N
    fn shard_for(&self, key: &str) -> usize {
        (Self::hash(key) % self.shards.len() as u64) as usize
    }

    fn set(&mut self, key: &str, value: &str) {
        let shard_id = self.shard_for(key);
        self.shards[shard_id]
            .data
            .insert(key.to_string(), value.to_string());
    }

    fn get(&self, key: &str) -> Option<&String> {
        let shard_id = self.shard_for(key);
        self.shards[shard_id].data.get(key)
    }
}

pub fn demo() {
    println!("\n  ═══ Hash Sharding ═══\n");

    let mut router = HashRouter::new(4);

    // Insert users — they distribute across shards
    let users = vec![
        ("user:1", "Alice"),
        ("user:2", "Bob"),
        ("user:3", "Charlie"),
        ("user:4", "Diana"),
        ("user:5", "Eve"),
        ("user:6", "Frank"),
        ("user:7", "Grace"),
        ("user:8", "Hank"),
    ];

    println!("    Inserting 8 users into 4 shards:\n");
    for (key, name) in &users {
        let shard = router.shard_for(key);
        router.set(key, name);
        println!("      {} ({}) → shard {}", key, name, shard);
    }

    // Show distribution
    println!("\n    Distribution across shards:\n");
    for shard in &router.shards {
        let keys: Vec<&String> = shard.data.keys().collect();
        println!(
            "      Shard {}: {} keys {:?}",
            shard.id,
            shard.data.len(),
            keys
        );
    }

    // Read back
    println!("\n    Read: user:3 → {:?}", router.get("user:3"));
    println!("    Read: user:7 → {:?}", router.get("user:7"));

    // Show the problem: what happens when we add a shard?
    println!("\n    ── Problem: adding a 5th shard remaps keys ──\n");

    let _router5 = HashRouter::new(5);
    let mut moved = 0;
    for (key, _) in &users {
        let old_shard = HashRouter::hash(key) % 4;
        let new_shard = HashRouter::hash(key) % 5;
        if old_shard != new_shard {
            moved += 1;
        }
        println!(
            "      {} : shard {} → shard {} {}",
            key,
            old_shard,
            new_shard,
            if old_shard != new_shard {
                "← MOVED"
            } else {
                ""
            }
        );
    }
    println!(
        "\n      {}/{} keys moved ({:.0}%) — this is why hash sharding is bad for scaling\n",
        moved,
        users.len(),
        moved as f64 / users.len() as f64 * 100.0
    );
}
