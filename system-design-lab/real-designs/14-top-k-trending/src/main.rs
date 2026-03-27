#![allow(dead_code, unused_variables, unused_imports)]
//! # Top-K / Trending System (YouTube Top K) - Mini Implementation
//!
//! Demonstrates:
//! - Count-Min Sketch for approximate frequency
//! - Heap-based Top-K tracking
//! - Time-windowed trending
//! - Heavy hitters detection
//!
//! Run: cargo run -p top-k-trending

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use rand::Rng;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// =============================================================================
// Count-Min Sketch (Approximate Frequency Counter)
// =============================================================================

struct CountMinSketch {
    width: usize,
    depth: usize,
    table: Vec<Vec<AtomicU64>>,
    seeds: Vec<u64>,
}

impl CountMinSketch {
    fn new(width: usize, depth: usize) -> Self {
        let mut rng = rand::thread_rng();
        let seeds: Vec<u64> = (0..depth).map(|_| rng.gen()).collect();

        let table = (0..depth)
            .map(|_| (0..width).map(|_| AtomicU64::new(0)).collect())
            .collect();

        Self {
            width,
            depth,
            table,
            seeds,
        }
    }

    fn hash(&self, item: &str, seed: u64) -> usize {
        let mut hash: u64 = seed;
        for byte in item.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        (hash as usize) % self.width
    }

    fn increment(&self, item: &str, count: u64) {
        for (i, seed) in self.seeds.iter().enumerate() {
            let idx = self.hash(item, *seed);
            self.table[i][idx].fetch_add(count, Ordering::SeqCst);
        }
    }

    fn estimate(&self, item: &str) -> u64 {
        self.seeds
            .iter()
            .enumerate()
            .map(|(i, seed)| {
                let idx = self.hash(item, *seed);
                self.table[i][idx].load(Ordering::SeqCst)
            })
            .min()
            .unwrap_or(0)
    }
}

// =============================================================================
// Min-Heap for Top-K
// =============================================================================

#[derive(Debug, Clone, Eq, PartialEq)]
struct HeapItem {
    id: String,
    count: u64,
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Min-heap: smaller count = higher priority (will be evicted first)
        other.count.cmp(&self.count)
    }
}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// =============================================================================
// Top-K Tracker
// =============================================================================

struct TopKTracker {
    k: usize,
    sketch: CountMinSketch,
    heap: Mutex<BinaryHeap<HeapItem>>,
    in_top_k: DashMap<String, u64>, // item_id -> count
    min_count: AtomicU64,
}

impl TopKTracker {
    fn new(k: usize, sketch_width: usize, sketch_depth: usize) -> Self {
        Self {
            k,
            sketch: CountMinSketch::new(sketch_width, sketch_depth),
            heap: Mutex::new(BinaryHeap::new()),
            in_top_k: DashMap::new(),
            min_count: AtomicU64::new(0),
        }
    }

    fn record(&self, item_id: &str) {
        // Update sketch
        self.sketch.increment(item_id, 1);
        let count = self.sketch.estimate(item_id);

        // Check if item should be in top-k
        let mut heap = self.heap.lock();

        if self.in_top_k.contains_key(item_id) {
            // Already in top-k, update count
            self.in_top_k.insert(item_id.to_string(), count);
            return;
        }

        if heap.len() < self.k {
            // Room in top-k
            heap.push(HeapItem {
                id: item_id.to_string(),
                count,
            });
            self.in_top_k.insert(item_id.to_string(), count);
        } else if count > self.min_count.load(Ordering::SeqCst) {
            // Better than minimum in top-k
            if let Some(evicted) = heap.pop() {
                self.in_top_k.remove(&evicted.id);
            }
            heap.push(HeapItem {
                id: item_id.to_string(),
                count,
            });
            self.in_top_k.insert(item_id.to_string(), count);

            // Update min
            if let Some(min_item) = heap.peek() {
                self.min_count.store(min_item.count, Ordering::SeqCst);
            }
        }
    }

    fn get_top_k(&self) -> Vec<(String, u64)> {
        let mut results: Vec<(String, u64)> = self
            .in_top_k
            .iter()
            .map(|e| (e.key().clone(), self.sketch.estimate(e.key())))
            .collect();

        results.sort_by(|a, b| b.1.cmp(&a.1));
        results
    }
}

// =============================================================================
// Time-Windowed Trending
// =============================================================================

struct TimeWindow {
    duration: Duration,
    buckets: RwLock<VecDeque<(Instant, HashMap<String, u64>)>>,
    bucket_duration: Duration,
}

impl TimeWindow {
    fn new(duration: Duration, bucket_count: usize) -> Self {
        Self {
            duration,
            buckets: RwLock::new(VecDeque::new()),
            bucket_duration: duration / bucket_count as u32,
        }
    }

    fn record(&self, item_id: &str) {
        let now = Instant::now();
        let mut buckets = self.buckets.write();

        // Get or create current bucket
        let should_create = buckets
            .back()
            .map(|(ts, _)| now.duration_since(*ts) >= self.bucket_duration)
            .unwrap_or(true);

        if should_create {
            buckets.push_back((now, HashMap::new()));
        }

        if let Some((_, counts)) = buckets.back_mut() {
            *counts.entry(item_id.to_string()).or_default() += 1;
        }

        // Evict old buckets
        while let Some((ts, _)) = buckets.front() {
            if now.duration_since(*ts) > self.duration {
                buckets.pop_front();
            } else {
                break;
            }
        }
    }

    fn get_counts(&self) -> HashMap<String, u64> {
        let buckets = self.buckets.read();
        let mut totals: HashMap<String, u64> = HashMap::new();

        for (_, counts) in buckets.iter() {
            for (item, count) in counts {
                *totals.entry(item.clone()).or_default() += count;
            }
        }

        totals
    }

    fn get_trending(&self, k: usize) -> Vec<(String, u64)> {
        let mut counts: Vec<(String, u64)> = self.get_counts().into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        counts.truncate(k);
        counts
    }
}

// =============================================================================
// Trending with Velocity
// =============================================================================

struct TrendingWithVelocity {
    recent: TimeWindow,   // Last 5 minutes
    baseline: TimeWindow, // Last 1 hour
}

impl TrendingWithVelocity {
    fn new() -> Self {
        Self {
            recent: TimeWindow::new(Duration::from_secs(300), 30), // 5 min, 10s buckets
            baseline: TimeWindow::new(Duration::from_secs(3600), 60), // 1 hour, 1 min buckets
        }
    }

    fn record(&self, item_id: &str) {
        self.recent.record(item_id);
        self.baseline.record(item_id);
    }

    fn get_trending(&self, k: usize) -> Vec<(String, f64, u64)> {
        let recent_counts = self.recent.get_counts();
        let baseline_counts = self.baseline.get_counts();

        let mut items: Vec<(String, f64, u64)> = recent_counts
            .iter()
            .map(|(item, recent_count)| {
                let baseline = baseline_counts.get(item).copied().unwrap_or(1);
                // Velocity: how much faster than baseline
                let velocity = (*recent_count as f64 * 12.0) / (baseline as f64).max(1.0);
                (item.clone(), velocity, *recent_count)
            })
            .collect();

        // Sort by velocity (trending = higher velocity)
        items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        items.truncate(k);
        items
    }
}

// =============================================================================
// Heavy Hitters (Space-Saving Algorithm Simplified)
// =============================================================================

struct HeavyHitters {
    counters: DashMap<String, u64>,
    max_counters: usize,
    min_count: AtomicU64,
}

impl HeavyHitters {
    fn new(max_counters: usize) -> Self {
        Self {
            counters: DashMap::new(),
            max_counters,
            min_count: AtomicU64::new(0),
        }
    }

    fn record(&self, item_id: &str) {
        if let Some(mut count) = self.counters.get_mut(item_id) {
            *count += 1;
            return;
        }

        if self.counters.len() < self.max_counters {
            self.counters.insert(item_id.to_string(), 1);
        } else {
            // Replace minimum
            let min_key = self
                .counters
                .iter()
                .min_by_key(|e| *e.value())
                .map(|e| e.key().clone());

            if let Some(key) = min_key {
                if let Some((_, old_count)) = self.counters.remove(&key) {
                    // New item inherits count + 1 (over-estimation)
                    self.counters.insert(item_id.to_string(), old_count + 1);
                }
            }
        }
    }

    fn get_heavy_hitters(&self, threshold_pct: f64, total: u64) -> Vec<(String, u64)> {
        let min_count = (total as f64 * threshold_pct / 100.0) as u64;

        let mut results: Vec<(String, u64)> = self
            .counters
            .iter()
            .filter(|e| *e.value() >= min_count)
            .map(|e| (e.key().clone(), *e.value()))
            .collect();

        results.sort_by(|a, b| b.1.cmp(&a.1));
        results
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== Top-K / Trending System Demo ===\n");

    // Demo 1: Count-Min Sketch
    println!("\n  ═══ Count-Min Sketch ═══");
    let sketch = CountMinSketch::new(1000, 5);

    // Simulate views
    for _ in 0..1000 {
        sketch.increment("video_popular", 1);
    }
    for _ in 0..100 {
        sketch.increment("video_medium", 1);
    }
    for _ in 0..10 {
        sketch.increment("video_rare", 1);
    }

    println!(
        "video_popular estimate: {} (actual: 1000)",
        sketch.estimate("video_popular")
    );
    println!(
        "video_medium estimate: {} (actual: 100)",
        sketch.estimate("video_medium")
    );
    println!(
        "video_rare estimate: {} (actual: 10)",
        sketch.estimate("video_rare")
    );
    println!();

    // Demo 2: Top-K Tracker
    println!("\n  ═══ Top-K Tracker ═══");
    let topk = TopKTracker::new(5, 1000, 5);

    // Simulate video views
    let videos = vec![
        ("vid_viral", 5000),
        ("vid_trending", 2000),
        ("vid_popular", 1000),
        ("vid_good", 500),
        ("vid_decent", 200),
        ("vid_meh", 50),
        ("vid_boring", 10),
    ];

    for (vid, views) in &videos {
        for _ in 0..*views {
            topk.record(vid);
        }
    }

    println!("Top 5 videos:");
    for (i, (vid, count)) in topk.get_top_k().iter().enumerate() {
        println!("  {}. {} ({} views)", i + 1, vid, count);
    }
    println!();

    // Demo 3: Time-Windowed Trending
    println!("\n  ═══ Time-Windowed Trending (Simulated) ═══");
    let window = TimeWindow::new(Duration::from_secs(60), 6);

    // Simulate events in window
    for _ in 0..100 {
        window.record("breaking_news");
    }
    for _ in 0..50 {
        window.record("sports_update");
    }
    for _ in 0..20 {
        window.record("tech_news");
    }

    println!("Trending in last minute:");
    for (i, (item, count)) in window.get_trending(5).iter().enumerate() {
        println!("  {}. {} ({} events)", i + 1, item, count);
    }
    println!();

    // Demo 4: Trending with Velocity
    println!("\n  ═══ Trending with Velocity ═══");
    let trending = TrendingWithVelocity::new();

    // Simulate baseline (historical)
    for _ in 0..100 {
        trending.baseline.record("steady_content");
    }

    // Simulate recent spike
    for _ in 0..100 {
        trending.record("viral_content"); // Recent only
    }
    for _ in 0..20 {
        trending.record("steady_content"); // Both
    }

    println!("Trending (by velocity):");
    for (i, (item, velocity, count)) in trending.get_trending(5).iter().enumerate() {
        println!(
            "  {}. {} ({} recent, velocity: {:.1}x)",
            i + 1,
            item,
            count,
            velocity
        );
    }
    println!();

    // Demo 5: Heavy Hitters
    println!("\n  ═══ Heavy Hitters Detection ═══");
    let hh = HeavyHitters::new(10);

    let mut total = 0u64;
    for (item, count) in &[
        ("item_A", 500),
        ("item_B", 300),
        ("item_C", 100),
        ("item_D", 50),
        ("item_E", 30),
        ("item_F", 10),
        ("item_G", 5),
        ("item_H", 3),
        ("item_I", 1),
        ("item_J", 1),
    ] {
        for _ in 0..*count {
            hh.record(item);
        }
        total += count;
    }

    println!("Heavy hitters (>5% of traffic, total={}):", total);
    for (item, count) in hh.get_heavy_hitters(5.0, total) {
        println!("  {} ({}%)", item, count * 100 / total);
    }

    println!("\n=== Key Concepts ===");
    println!("1. Count-Min Sketch: O(1) space approximate counting");
    println!("2. Top-K: Min-heap maintains only top K items");
    println!("3. Time Windows: Sliding window with bucket aggregation");
    println!("4. Velocity: (recent / baseline) ratio for trend detection");
    println!("5. Heavy Hitters: Space-saving algorithm for frequent items");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_min_sketch() {
        let sketch = CountMinSketch::new(100, 3);

        for _ in 0..100 {
            sketch.increment("item1", 1);
        }

        let estimate = sketch.estimate("item1");
        // Should be close to 100 (with some over-estimation possible)
        assert!(estimate >= 100);
        assert!(estimate <= 110); // Allow small error
    }

    #[test]
    fn test_top_k() {
        let topk = TopKTracker::new(2, 100, 3);

        for _ in 0..100 {
            topk.record("a");
        }
        for _ in 0..50 {
            topk.record("b");
        }
        for _ in 0..10 {
            topk.record("c");
        }

        let top = topk.get_top_k();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "a");
        assert_eq!(top[1].0, "b");
    }
}
