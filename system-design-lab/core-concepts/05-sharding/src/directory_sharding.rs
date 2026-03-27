use std::collections::HashMap;

// =============================================================================
// Directory-Based Sharding
//
//   A lookup table (directory) maps each key to its shard.
//
//   Instead of computing shard from the key (hash/range), we STORE the mapping.
//
//     Directory:
//       "user:1" → shard_A
//       "user:2" → shard_B
//       "user:3" → shard_A
//
//   Properties:
//     + Maximum flexibility — any key can be on any shard
//     + Easy to move a key: just update the directory entry
//     + No hash remapping problem, no range hotspot problem
//     + Can rebalance individual keys (move hot user to a less loaded shard)
//
//     - Extra hop: every read/write must first query the directory
//     - Directory is a SPOF — if it goes down, nothing can route
//     - Directory must be fast (usually cached in memory or Redis)
//     - Directory itself must be replicated for availability
//
//   Used by:
//     - AWS Aurora: directory maps tables → storage volumes
//     - Google Vitess: VSchema maps keys → shards via vindexes
//     - Many multi-tenant SaaS: directory maps tenant_id → shard
// =============================================================================

/// The directory: maps keys to shard names.
struct ShardDirectory {
    mapping: HashMap<String, String>, // key → shard_name
}

impl ShardDirectory {
    fn new() -> Self {
        Self {
            mapping: HashMap::new(),
        }
    }

    fn assign(&mut self, key: &str, shard: &str) {
        self.mapping.insert(key.to_string(), shard.to_string());
    }

    fn lookup(&self, key: &str) -> Option<&String> {
        self.mapping.get(key)
    }

    /// Move a key to a different shard (rebalancing).
    fn move_key(&mut self, key: &str, new_shard: &str) {
        self.mapping.insert(key.to_string(), new_shard.to_string());
    }
}

/// A database shard.
struct Shard {
    #[allow(dead_code)]
    name: String,
    data: HashMap<String, String>,
}

impl Shard {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            data: HashMap::new(),
        }
    }
}

/// Directory-based shard router.
struct DirectoryRouter {
    directory: ShardDirectory,
    shards: HashMap<String, Shard>,
}

impl DirectoryRouter {
    fn new(shard_names: &[&str]) -> Self {
        let mut shards = HashMap::new();
        for &name in shard_names {
            shards.insert(name.to_string(), Shard::new(name));
        }
        Self {
            directory: ShardDirectory::new(),
            shards,
        }
    }

    /// Assign a key to a shard and write the value.
    fn set(&mut self, key: &str, value: &str, shard_name: &str) {
        self.directory.assign(key, shard_name);
        if let Some(shard) = self.shards.get_mut(shard_name) {
            shard.data.insert(key.to_string(), value.to_string());
        }
    }

    /// Read: directory lookup → route to shard → get value.
    fn get(&self, key: &str) -> Option<&String> {
        let shard_name = self.directory.lookup(key)?;
        let shard = self.shards.get(shard_name)?;
        shard.data.get(key)
    }

    /// Move a key from one shard to another (online rebalancing).
    fn move_key(&mut self, key: &str, new_shard: &str) {
        // 1. Read value from old shard
        let old_shard_name = self.directory.lookup(key).cloned();
        let value = old_shard_name
            .as_ref()
            .and_then(|name| self.shards.get(name).and_then(|s| s.data.get(key).cloned()));

        if let Some(value) = value {
            // 2. Write to new shard
            if let Some(shard) = self.shards.get_mut(new_shard) {
                shard.data.insert(key.to_string(), value);
            }
            // 3. Update directory (atomic switch)
            self.directory.move_key(key, new_shard);
            // 4. Delete from old shard
            if let Some(old_name) = old_shard_name {
                if let Some(shard) = self.shards.get_mut(&old_name) {
                    shard.data.remove(key);
                }
            }
        }
    }
}

pub fn demo() {
    println!("\n  ═══ Directory Sharding ═══\n");

    let mut router = DirectoryRouter::new(&["shard_A", "shard_B", "shard_C"]);

    // Assign users to specific shards
    // In production: tenant_id → shard mapping
    println!("    Assigning users to shards:\n");
    router.set(
        "user:1",
        r#"{"name":"Alice","plan":"enterprise"}"#,
        "shard_A",
    );
    router.set("user:2", r#"{"name":"Bob","plan":"free"}"#, "shard_B");
    router.set(
        "user:3",
        r#"{"name":"Charlie","plan":"enterprise"}"#,
        "shard_A",
    );
    router.set("user:4", r#"{"name":"Diana","plan":"free"}"#, "shard_C");
    router.set("user:5", r#"{"name":"Eve","plan":"pro"}"#, "shard_B");

    for key in &["user:1", "user:2", "user:3", "user:4", "user:5"] {
        let shard = router.directory.lookup(key).unwrap();
        println!("      {} → {}", key, shard);
    }

    // Read
    println!("\n    Read: user:3 → {:?}", router.get("user:3"));

    // Distribution
    println!("\n    Distribution:");
    for (name, shard) in &router.shards {
        println!("      {}: {} keys", name, shard.data.len());
    }

    // ── Move a hot user to a less loaded shard ──
    println!("\n    ── Moving user:1 from shard_A to shard_C ──\n");

    let before = router.directory.lookup("user:1").unwrap().clone();
    router.move_key("user:1", "shard_C");
    let after = router.directory.lookup("user:1").unwrap().clone();
    println!("      user:1: {} → {}", before, after);
    println!("      Data still accessible: {:?}", router.get("user:1"));

    println!("\n    Distribution after move:");
    for (name, shard) in &router.shards {
        let keys: Vec<&String> = shard.data.keys().collect();
        println!("      {}: {} keys {:?}", name, shard.data.len(), keys);
    }
    println!("\n      Directory-based sharding lets you move ANY key with zero downtime\n");
}
