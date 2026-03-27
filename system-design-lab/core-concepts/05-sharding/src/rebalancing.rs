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
struct Shard {
    id: usize,
    slots: Vec<usize>,
    data: HashMap<String, String>,
}

impl Shard {
    fn new(id: usize) -> Self {
        Self {
            id,
            slots: Vec::new(),
            data: HashMap::new(),
        }
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
            shards[shard_id].slots.push(slot);
        }

        Self {
            shards,
            slot_to_shard,
        }
    }

    fn key_to_slot(key: &str) -> usize {
        // Simple hash to slot
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
        let shard_id = self.shard_for(key);
        self.shards[shard_id]
            .data
            .insert(key.to_string(), value.to_string());
    }

    fn get(&self, key: &str) -> Option<&String> {
        let shard_id = self.shard_for(key);
        self.shards[shard_id].data.get(key)
    }

    /// Add a new shard by migrating some slots from existing shards.
    /// Returns how many keys were moved.
    fn add_shard(&mut self) -> (usize, usize) {
        let new_id = self.shards.len();
        self.shards.push(Shard::new(new_id));

        // Steal ~1/N slots from each existing shard
        let slots_per_shard = TOTAL_SLOTS / (new_id + 1);
        let mut moved_slots = 0;
        let mut moved_keys = 0;

        // Collect slots to move
        let mut slots_to_move = Vec::new();
        for shard in &self.shards[..new_id] {
            // Each existing shard donates a few slots
            let donate = shard.slots.len().saturating_sub(slots_per_shard);
            for &slot in shard.slots.iter().rev().take(donate) {
                slots_to_move.push(slot);
            }
        }

        // Move slots and their data to new shard
        for slot in &slots_to_move {
            let old_shard_id = self.slot_to_shard[*slot];
            self.slot_to_shard[*slot] = new_id;

            // Move data belonging to this slot
            let old_data: Vec<(String, String)> = self.shards[old_shard_id]
                .data
                .iter()
                .filter(|(k, _)| Self::key_to_slot(k) == *slot)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            for (k, v) in &old_data {
                self.shards[old_shard_id].data.remove(k);
                self.shards[new_id].data.insert(k.clone(), v.clone());
                moved_keys += 1;
            }

            // Update slot ownership
            self.shards[old_shard_id].slots.retain(|s| s != slot);
            self.shards[new_id].slots.push(*slot);
            moved_slots += 1;
        }

        (moved_slots, moved_keys)
    }

    fn print_distribution(&self) {
        for shard in &self.shards {
            println!(
                "      Shard {}: {} slots, {} keys",
                shard.id,
                shard.slots.len(),
                shard.data.len()
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
