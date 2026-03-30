use std::collections::HashMap;

// =============================================================================
// Shard Rebalancing
//
//   When you add or remove a shard, data must move.
//   This is one of the hardest operational problems in distributed systems.
//
//   Strategies:
//
//   1. Full Reshuffle (naive hash sharding):
//      hash(key) % N → hash(key) % (N+1)
//      ~80% of keys move when going from 4→5 shards. Terrible.
//
//   2. Virtual Shards (slots):
//      Pre-partition into many slots (e.g., 16384 like Redis Cluster).
//      Each physical shard owns a range of slots.
//      Adding a shard = move some slots, not all keys.
//
//      Before: Shard A owns slots [0, 5461], B owns [5462, 10922], C owns [10923, 16383]
//      After:  Move slots [0, 4095] to new Shard D
//      Only ~25% of keys move, not 75%.
//
//   3. Split/Merge (range sharding):
//      Split a hot shard into 2 by dividing its key range.
//      Shard A [0, 1000) → Shard A [0, 500) + Shard A' [500, 1000)
//      Only the split shard's data moves.
//
//   Online rebalancing requirements:
//     - No downtime (reads/writes continue during move)
//     - Double-write: write to both old and new shard during transition
//     - Atomic cutover: switch routing after copy is complete
//     - Verify: check no data was lost
//
//   Used by: Redis Cluster (slot migration), Vitess (shard split), CockroachDB (range split)
// =============================================================================

const TOTAL_SLOTS: usize = 16; // simplified (Redis uses 16384)

/// A physical shard that owns a set of virtual slots.
/// Data is stored PER-SLOT, not in a flat map.
/// This way, migrating a slot = move its entire HashMap. O(keys_in_slot).
/// If we stored data per-shard, migrating would scan ALL keys and re-hash. O(all_keys).
struct Shard {
    id: usize,
    slot_data: HashMap<usize, HashMap<String, String>>, // slot → {key → value}
}

impl Shard {
    fn new(id: usize) -> Self {
        Self {
            id,
            slot_data: HashMap::new(),
        }
    }

    fn slots(&self) -> Vec<usize> {
        self.slot_data.keys().copied().collect()
    }

    fn num_slots(&self) -> usize {
        self.slot_data.len()
    }

    fn num_keys(&self) -> usize {
        self.slot_data.values().map(|m| m.len()).sum()
    }

    fn insert(&mut self, slot: usize, key: String, value: String) {
        self.slot_data.entry(slot).or_default().insert(key, value);
    }

    fn get(&self, slot: usize, key: &str) -> Option<&String> {
        self.slot_data.get(&slot)?.get(key)
    }

    /// Remove an entire slot's data — O(1), just take the HashMap.
    fn take_slot(&mut self, slot: usize) -> HashMap<String, String> {
        self.slot_data.remove(&slot).unwrap_or_default()
    }

    /// Receive an entire slot's data — O(1), just insert the HashMap.
    fn receive_slot(&mut self, slot: usize, data: HashMap<String, String>) {
        self.slot_data.insert(slot, data);
    }
}

/// Virtual slot-based shard router.
/// Keys map to slots (fixed), slots map to shards (changeable).
struct SlotRouter {
    shards: Vec<Shard>,
    slot_to_shard: [usize; TOTAL_SLOTS], // slot → shard_id
}

impl SlotRouter {
    fn new(num_shards: usize) -> Self {
        let mut shards: Vec<Shard> = (0..num_shards).map(Shard::new).collect();
        let mut slot_to_shard = [0usize; TOTAL_SLOTS];

        // Distribute slots evenly across shards
        for (slot, entry) in slot_to_shard.iter_mut().enumerate().take(TOTAL_SLOTS) {
            let shard_id = slot % num_shards;
            *entry = shard_id;
            shards[shard_id].slot_data.insert(slot, HashMap::new()); // initialize empty slot
        }

        Self {
            shards,
            slot_to_shard,
        }
    }

    fn key_to_slot(key: &str) -> usize {
        let mut h: u64 = 0xcbf29ce484222325;
        for byte in key.bytes() {
            h ^= byte as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        (h as usize) % TOTAL_SLOTS
    }

    fn shard_for(&self, key: &str) -> usize {
        let slot = Self::key_to_slot(key);
        self.slot_to_shard[slot]
    }

    fn set(&mut self, key: &str, value: &str) {
        let slot = Self::key_to_slot(key);
        let shard_id = self.slot_to_shard[slot];
        self.shards[shard_id].insert(slot, key.to_string(), value.to_string());
    }

    fn get(&self, key: &str) -> Option<&String> {
        let slot = Self::key_to_slot(key);
        let shard_id = self.slot_to_shard[slot];
        self.shards[shard_id].get(slot, key)
    }

    /// Add a new shard by migrating some slots from existing shards.
    /// Migration is O(keys_in_moved_slots) — NOT O(all_keys_on_shard).
    fn add_shard(&mut self) -> (usize, usize) {
        let new_id = self.shards.len();
        self.shards.push(Shard::new(new_id));

        let slots_per_shard = TOTAL_SLOTS / (new_id + 1);
        let mut moved_slots = 0;
        let mut moved_keys = 0;

        // Collect (shard_id, slot) pairs to move
        let mut slots_to_move: Vec<(usize, usize)> = Vec::new();
        for shard in &self.shards[..new_id] {
            let donate = shard.num_slots().saturating_sub(slots_per_shard);
            let slots: Vec<usize> = shard.slots();
            for &slot in slots.iter().rev().take(donate) {
                slots_to_move.push((shard.id, slot));
            }
        }

        // Move each slot: take entire HashMap from old shard, give to new shard
        for (old_shard_id, slot) in &slots_to_move {
            // O(1) take — just remove the slot's HashMap from old shard
            let slot_data = self.shards[*old_shard_id].take_slot(*slot);
            moved_keys += slot_data.len();

            // O(1) receive — just insert the HashMap into new shard
            self.shards[new_id].receive_slot(*slot, slot_data);

            // Update routing table
            self.slot_to_shard[*slot] = new_id;
            moved_slots += 1;
        }

        (moved_slots, moved_keys)
    }

    fn print_distribution(&self) {
        for shard in &self.shards {
            println!(
                "      Shard {}: {} slots, {} keys",
                shard.id,
                shard.num_slots(),
                shard.num_keys()
            );
        }
    }
}

pub fn demo() {
    println!("\n  ═══ Rebalancing ═══\n");

    let mut router = SlotRouter::new(3);

    // Insert data
    for i in 0..30 {
        router.set(&format!("key:{}", i), &format!("value-{}", i));
    }

    println!("    Initial: 3 shards, {} slots, 30 keys\n", TOTAL_SLOTS);
    router.print_distribution();

    // Verify reads work
    println!(
        "\n    Read key:5 → {:?} (shard {})",
        router.get("key:5"),
        router.shard_for("key:5")
    );

    // ── Add a 4th shard ──

    println!("\n    ── Adding shard 3 (rebalancing) ──\n");
    let (moved_slots, moved_keys) = router.add_shard();
    println!(
        "      Moved {} slots and {} keys to new shard\n",
        moved_slots, moved_keys
    );
    router.print_distribution();

    // Verify reads still work after rebalancing
    println!(
        "\n    Read key:5 → {:?} (shard {}) — still works!",
        router.get("key:5"),
        router.shard_for("key:5")
    );

    // Verify all data is still accessible
    let mut accessible = 0;
    for i in 0..30 {
        if router.get(&format!("key:{}", i)).is_some() {
            accessible += 1;
        }
    }
    println!("    All data accessible: {}/30 keys found\n", accessible);

    println!(
        "    Key insight: only {}/{} slots moved ({:.0}%)",
        moved_slots,
        TOTAL_SLOTS,
        moved_slots as f64 / TOTAL_SLOTS as f64 * 100.0
    );
    println!("    With hash(key)%N, ~75% of keys would move. Slots = much better.\n");
}
