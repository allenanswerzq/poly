use std::collections::HashMap;

// =============================================================================
// Range-Based Sharding
//
//   Map contiguous ranges of the shard key to specific shards.
//
//     key range [0, 1000)     → Shard 0
//     key range [1000, 2000)  → Shard 1
//     key range [2000, 3000)  → Shard 2
//
//   Properties:
//     + Range queries are efficient (scan only the relevant shards)
//     + Easy to split a shard: just move half its range to a new shard
//     + Data locality — nearby keys on the same shard (good for time-series)
//
//     - Hotspots: if data is skewed towards a range, one shard gets hammered
//       e.g., date-based sharding + all recent writes → last shard is hot
//     - Need a config that maps ranges to shards (metadata to maintain)
//
//   Real-world usage:
//     - HBase: row key ranges map to regions (a region is a shard)
//     - MongoDB: range-based sharding on the shard key
//     - Time-series DBs: partition by time range (hourly/daily/monthly)
//     - Google Bigtable: tablet ≈ contiguous row range on one server
// =============================================================================

/// A range shard owns keys in [range_start, range_end).
struct RangeShard {
    id: usize,
    range_start: u64,
    range_end: u64,
    data: HashMap<u64, String>,
}

impl RangeShard {
    fn new(id: usize, start: u64, end: u64) -> Self {
        Self {
            id,
            range_start: start,
            range_end: end,
            data: HashMap::new(),
        }
    }

    fn owns(&self, key: u64) -> bool {
        key >= self.range_start && key < self.range_end
    }
}

/// Range-based shard router: find which shard owns the key range.
struct RangeRouter {
    shards: Vec<RangeShard>,
}

impl RangeRouter {
    fn new(ranges: &[(u64, u64)]) -> Self {
        let shards = ranges
            .iter()
            .enumerate()
            .map(|(id, &(start, end))| RangeShard::new(id, start, end))
            .collect();
        Self { shards }
    }

    fn shard_for(&self, key: u64) -> Option<usize> {
        self.shards.iter().position(|s| s.owns(key))
    }

    fn set(&mut self, key: u64, value: &str) {
        if let Some(idx) = self.shard_for(key) {
            self.shards[idx].data.insert(key, value.to_string());
        }
    }

    #[allow(dead_code)]
    fn get(&self, key: u64) -> Option<&String> {
        let idx = self.shard_for(key)?;
        self.shards[idx].data.get(&key)
    }

    /// Range query: find all values where key is in [start, end).
    /// Only scans shards whose range overlaps with the query range.
    fn range_query(&self, start: u64, end: u64) -> Vec<(u64, &String)> {
        let mut results = Vec::new();
        for shard in &self.shards {
            // Skip shards with no overlap
            if shard.range_end <= start || shard.range_start >= end {
                continue;
            }
            for (&k, v) in &shard.data {
                if k >= start && k < end {
                    results.push((k, v));
                }
            }
        }
        results.sort_by_key(|&(k, _)| k);
        results
    }
}

pub fn demo() {
    println!("\n  ═══ Range Sharding ═══\n");

    // 3 shards: [0,1000), [1000,2000), [2000,3000)
    let mut router = RangeRouter::new(&[(0, 1000), (1000, 2000), (2000, 3000)]);

    println!("    Shard layout:");
    for s in &router.shards {
        println!(
            "      Shard {}: keys [{}, {})",
            s.id, s.range_start, s.range_end
        );
    }

    // Insert users by numeric ID
    let users = vec![
        (100, "Alice"),
        (500, "Bob"),
        (999, "Charlie"),
        (1001, "Diana"),
        (1500, "Eve"),
        (2001, "Frank"),
        (2500, "Grace"),
    ];

    println!("\n    Inserting users:\n");
    for &(id, name) in &users {
        let shard = router.shard_for(id).unwrap();
        router.set(id, name);
        println!("      user:{} ({}) → shard {}", id, name, shard);
    }

    // Distribution
    println!("\n    Distribution:");
    for s in &router.shards {
        println!(
            "      Shard {} [{},{}): {} keys",
            s.id,
            s.range_start,
            s.range_end,
            s.data.len()
        );
    }

    // Range query: only scans relevant shards
    println!("\n    ── Range query: users with ID in [400, 1500) ──\n");
    let results = router.range_query(400, 1500);
    println!("      Results ({} found):", results.len());
    for (k, v) in &results {
        println!("        user:{} → {}", k, v);
    }
    println!("      Only shards 0 and 1 were scanned (shard 2 skipped)");

    // Show the hotspot problem
    println!("\n    ── Hotspot problem ──\n");
    println!("      If we shard by date and all recent writes go to the last shard:");
    let mut date_router = RangeRouter::new(&[
        (20240101, 20240401), // Q1
        (20240401, 20240701), // Q2
        (20240701, 20241001), // Q3
        (20241001, 20250101), // Q4
    ]);
    // All recent writes hit Q4
    for day in 20241001..=20241231 {
        date_router.set(day, "event");
    }
    for s in &date_router.shards {
        println!(
            "      Shard {} ({}–{}): {} records {}",
            s.id,
            s.range_start,
            s.range_end,
            s.data.len(),
            if s.data.len() > 50 { "← HOT!" } else { "" }
        );
    }
    println!("      Fix: compound key (user_id + date) to spread writes\n");
}
