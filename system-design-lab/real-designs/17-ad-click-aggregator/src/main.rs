//! # Ad Click Aggregator - Mini Implementation
//!
//! Demonstrates:
//! - Lambda architecture (batch + real-time)
//! - Time-windowed aggregation
//! - Click deduplication
//! - MapReduce-style processing
//!
//! Run: cargo run -p ad-click-aggregator

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

fn instant_now() -> Instant {
    Instant::now()
}

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClickEvent {
    event_id: String,
    ad_id: String,
    campaign_id: String,
    user_id: String,
    #[serde(skip, default = "instant_now")]
    timestamp: Instant,
    country: String,
    device_type: String,
}

#[derive(Debug, Clone, Default)]
struct AggregatedStats {
    total_clicks: u64,
    unique_users: usize,
    by_country: HashMap<String, u64>,
    by_device: HashMap<String, u64>,
}

// =============================================================================
// Click Deduplicator (Bloom Filter based)
// =============================================================================

struct ClickDeduplicator {
    seen_events: DashMap<String, Instant>, // event_id -> first_seen_time
    ttl: Duration,
    duplicates_detected: AtomicU64,
}

impl ClickDeduplicator {
    fn new(ttl: Duration) -> Self {
        Self {
            seen_events: DashMap::new(),
            ttl,
            duplicates_detected: AtomicU64::new(0),
        }
    }

    fn is_duplicate(&self, event_id: &str) -> bool {
        if self.seen_events.contains_key(event_id) {
            self.duplicates_detected.fetch_add(1, Ordering::SeqCst);
            return true;
        }

        self.seen_events.insert(event_id.to_string(), Instant::now());
        false
    }

    fn cleanup(&self) {
        let now = Instant::now();
        self.seen_events
            .retain(|_, ts| now.duration_since(*ts) < self.ttl);
    }
}

// =============================================================================
// Real-time Aggregator (Speed Layer)
// =============================================================================

struct SpeedLayer {
    // Time buckets for windowed aggregation
    windows: DashMap<String, AggregationWindow>, // campaign_id -> window
    window_duration: Duration,
}

struct AggregationWindow {
    buckets: RwLock<VecDeque<TimeBucket>>,
    bucket_duration: Duration,
}

struct TimeBucket {
    start_time: Instant,
    clicks: u64,
    users: HashSet<String>,
    by_country: HashMap<String, u64>,
    by_device: HashMap<String, u64>,
}

impl AggregationWindow {
    fn new(window_duration: Duration, bucket_count: usize) -> Self {
        Self {
            buckets: RwLock::new(VecDeque::new()),
            bucket_duration: window_duration / bucket_count as u32,
        }
    }

    fn add(&self, event: &ClickEvent) {
        let now = Instant::now();
        let mut buckets = self.buckets.write();

        // Create new bucket if needed
        let should_create = buckets
            .back()
            .map(|b| now.duration_since(b.start_time) >= self.bucket_duration)
            .unwrap_or(true);

        if should_create {
            buckets.push_back(TimeBucket {
                start_time: now,
                clicks: 0,
                users: HashSet::new(),
                by_country: HashMap::new(),
                by_device: HashMap::new(),
            });
        }

        // Update current bucket
        if let Some(bucket) = buckets.back_mut() {
            bucket.clicks += 1;
            bucket.users.insert(event.user_id.clone());
            *bucket.by_country.entry(event.country.clone()).or_default() += 1;
            *bucket.by_device.entry(event.device_type.clone()).or_default() += 1;
        }
    }

    fn get_stats(&self, max_age: Duration) -> AggregatedStats {
        let now = Instant::now();
        let buckets = self.buckets.read();

        let mut stats = AggregatedStats::default();
        let mut all_users: HashSet<String> = HashSet::new();

        for bucket in buckets.iter() {
            if now.duration_since(bucket.start_time) <= max_age {
                stats.total_clicks += bucket.clicks;
                all_users.extend(bucket.users.iter().cloned());

                for (country, count) in &bucket.by_country {
                    *stats.by_country.entry(country.clone()).or_default() += count;
                }
                for (device, count) in &bucket.by_device {
                    *stats.by_device.entry(device.clone()).or_default() += count;
                }
            }
        }

        stats.unique_users = all_users.len();
        stats
    }

    fn cleanup(&self, max_age: Duration) {
        let now = Instant::now();
        let mut buckets = self.buckets.write();
        while let Some(bucket) = buckets.front() {
            if now.duration_since(bucket.start_time) > max_age {
                buckets.pop_front();
            } else {
                break;
            }
        }
    }
}

impl SpeedLayer {
    fn new(window_duration: Duration) -> Self {
        Self {
            windows: DashMap::new(),
            window_duration,
        }
    }

    fn process(&self, event: &ClickEvent) {
        self.windows
            .entry(event.campaign_id.clone())
            .or_insert_with(|| AggregationWindow::new(self.window_duration, 60))
            .add(event);
    }

    fn get_stats(&self, campaign_id: &str, window: Duration) -> AggregatedStats {
        self.windows
            .get(campaign_id)
            .map(|w| w.get_stats(window))
            .unwrap_or_default()
    }
}

// =============================================================================
// Batch Layer (Simulated)
// =============================================================================

struct BatchLayer {
    // Stores finalized hourly aggregates
    hourly_stats: DashMap<String, HashMap<u64, AggregatedStats>>, // campaign_id -> hour -> stats
}

impl BatchLayer {
    fn new() -> Self {
        Self {
            hourly_stats: DashMap::new(),
        }
    }

    fn store_hourly(&self, campaign_id: &str, hour: u64, stats: AggregatedStats) {
        self.hourly_stats
            .entry(campaign_id.to_string())
            .or_default()
            .insert(hour, stats);
    }

    fn get_range(&self, campaign_id: &str, start_hour: u64, end_hour: u64) -> AggregatedStats {
        let mut combined = AggregatedStats::default();

        if let Some(hours) = self.hourly_stats.get(campaign_id) {
            for hour in start_hour..=end_hour {
                if let Some(stats) = hours.get(&hour) {
                    combined.total_clicks += stats.total_clicks;
                    // Note: unique_users would need HyperLogLog for accurate merging

                    for (country, count) in &stats.by_country {
                        *combined.by_country.entry(country.clone()).or_default() += count;
                    }
                    for (device, count) in &stats.by_device {
                        *combined.by_device.entry(device.clone()).or_default() += count;
                    }
                }
            }
        }

        combined
    }
}

// =============================================================================
// MapReduce Aggregator
// =============================================================================

struct MapReduceAggregator {
    // Simulated distributed processing
    mapper_outputs: Mutex<Vec<(String, (String, u64))>>, // (key, (dimension, count))
}

impl MapReduceAggregator {
    fn new() -> Self {
        Self {
            mapper_outputs: Mutex::new(Vec::new()),
        }
    }

    fn map(&self, events: &[ClickEvent]) {
        let mut outputs = self.mapper_outputs.lock();

        for event in events {
            // Emit multiple key-value pairs per event
            outputs.push((
                format!("campaign:{}", event.campaign_id),
                ("clicks".to_string(), 1),
            ));
            outputs.push((
                format!("campaign:{}:country:{}", event.campaign_id, event.country),
                ("clicks".to_string(), 1),
            ));
            outputs.push((
                format!("campaign:{}:device:{}", event.campaign_id, event.device_type),
                ("clicks".to_string(), 1),
            ));
        }
    }

    fn reduce(&self) -> HashMap<String, u64> {
        let outputs = self.mapper_outputs.lock();
        let mut results: HashMap<String, u64> = HashMap::new();

        for (key, (_, count)) in outputs.iter() {
            *results.entry(key.clone()).or_default() += count;
        }

        results
    }

    fn clear(&self) {
        self.mapper_outputs.lock().clear();
    }
}

// =============================================================================
// Ad Click Service (Lambda Architecture)
// =============================================================================

struct AdClickService {
    deduplicator: ClickDeduplicator,
    speed_layer: SpeedLayer,
    batch_layer: BatchLayer,
    map_reduce: MapReduceAggregator,
    total_events: AtomicU64,
}

impl AdClickService {
    fn new() -> Self {
        Self {
            deduplicator: ClickDeduplicator::new(Duration::from_secs(3600)),
            speed_layer: SpeedLayer::new(Duration::from_secs(3600)),
            batch_layer: BatchLayer::new(),
            map_reduce: MapReduceAggregator::new(),
            total_events: AtomicU64::new(0),
        }
    }

    fn process_click(&self, event: ClickEvent) -> bool {
        // Deduplication
        if self.deduplicator.is_duplicate(&event.event_id) {
            return false;
        }

        // Real-time aggregation
        self.speed_layer.process(&event);

        // Queue for batch processing
        self.map_reduce.map(&[event]);

        self.total_events.fetch_add(1, Ordering::SeqCst);
        true
    }

    fn get_realtime_stats(&self, campaign_id: &str, window: Duration) -> AggregatedStats {
        self.speed_layer.get_stats(campaign_id, window)
    }

    fn run_batch_job(&self) -> HashMap<String, u64> {
        let results = self.map_reduce.reduce();
        self.map_reduce.clear();
        results
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== Ad Click Aggregator Demo ===\n");

    let service = AdClickService::new();

    // Generate click events
    println!("--- Processing Click Events ---");

    let campaigns = vec!["camp_summer", "camp_holiday", "camp_flash"];
    let countries = vec!["US", "UK", "DE", "FR", "JP"];
    let devices = vec!["mobile", "desktop", "tablet"];

    let mut rng = rand::thread_rng();
    let mut events_processed = 0;
    let mut duplicates = 0;

    for i in 0..100 {
        let event = ClickEvent {
            event_id: format!("evt_{}", i % 90), // Some duplicates
            ad_id: format!("ad_{}", rng.gen_range(1..10)),
            campaign_id: campaigns[rng.gen_range(0..campaigns.len())].to_string(),
            user_id: format!("user_{}", rng.gen_range(1..50)),
            timestamp: Instant::now(),
            country: countries[rng.gen_range(0..countries.len())].to_string(),
            device_type: devices[rng.gen_range(0..devices.len())].to_string(),
        };

        if service.process_click(event) {
            events_processed += 1;
        } else {
            duplicates += 1;
        }
    }

    println!(
        "Processed: {} events, Duplicates blocked: {}",
        events_processed, duplicates
    );
    println!(
        "Total unique events: {}",
        service.total_events.load(Ordering::SeqCst)
    );
    println!();

    // Real-time stats
    println!("--- Real-time Stats (Speed Layer) ---");
    for campaign in &campaigns {
        let stats = service.get_realtime_stats(campaign, Duration::from_secs(60));
        if stats.total_clicks > 0 {
            println!("{}:", campaign);
            println!(
                "  Clicks: {}, Unique users: {}",
                stats.total_clicks, stats.unique_users
            );
            println!("  By country: {:?}", stats.by_country);
            println!("  By device: {:?}", stats.by_device);
        }
    }
    println!();

    // Batch processing
    println!("--- Batch Processing (MapReduce) ---");
    let batch_results = service.run_batch_job();
    println!("MapReduce results:");
    let mut sorted: Vec<_> = batch_results.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (key, count) in sorted.iter().take(10) {
        println!("  {}: {}", key, count);
    }
    println!();

    // Demonstrate windowed aggregation
    println!("--- Windowed Aggregation ---");
    let window = AggregationWindow::new(Duration::from_secs(60), 6);

    // Add some events
    for i in 0..50 {
        let event = ClickEvent {
            event_id: format!("win_evt_{}", i),
            ad_id: "ad_1".to_string(),
            campaign_id: "test".to_string(),
            user_id: format!("user_{}", i % 10),
            timestamp: Instant::now(),
            country: if i % 2 == 0 { "US" } else { "UK" }.to_string(),
            device_type: "mobile".to_string(),
        };
        window.add(&event);
    }

    let stats = window.get_stats(Duration::from_secs(60));
    println!("Last 60 seconds:");
    println!("  Total clicks: {}", stats.total_clicks);
    println!("  Unique users: {}", stats.unique_users);
    println!("  US vs UK: {:?}", stats.by_country);

    println!("\n=== Key Concepts ===");
    println!("1. Deduplication: Filter duplicate click events");
    println!("2. Speed Layer: Real-time windowed aggregation");
    println!("3. Batch Layer: Historical hourly aggregates");
    println!("4. MapReduce: Distributed aggregation pattern");
    println!("5. Lambda Architecture: Speed + Batch for accuracy + low latency");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplication() {
        let dedup = ClickDeduplicator::new(Duration::from_secs(60));

        assert!(!dedup.is_duplicate("evt_1"));
        assert!(dedup.is_duplicate("evt_1")); // Duplicate
        assert!(!dedup.is_duplicate("evt_2"));
    }

    #[test]
    fn test_windowed_aggregation() {
        let window = AggregationWindow::new(Duration::from_secs(60), 6);

        for i in 0..10 {
            let event = ClickEvent {
                event_id: format!("evt_{}", i),
                ad_id: "ad_1".to_string(),
                campaign_id: "test".to_string(),
                user_id: format!("user_{}", i % 5),
                timestamp: Instant::now(),
                country: "US".to_string(),
                device_type: "mobile".to_string(),
            };
            window.add(&event);
        }

        let stats = window.get_stats(Duration::from_secs(60));
        assert_eq!(stats.total_clicks, 10);
        assert_eq!(stats.unique_users, 5);
    }

    #[test]
    fn test_map_reduce() {
        let mr = MapReduceAggregator::new();

        let events = vec![
            ClickEvent {
                event_id: "1".to_string(),
                ad_id: "ad".to_string(),
                campaign_id: "c1".to_string(),
                user_id: "u1".to_string(),
                timestamp: Instant::now(),
                country: "US".to_string(),
                device_type: "mobile".to_string(),
            },
            ClickEvent {
                event_id: "2".to_string(),
                ad_id: "ad".to_string(),
                campaign_id: "c1".to_string(),
                user_id: "u2".to_string(),
                timestamp: Instant::now(),
                country: "US".to_string(),
                device_type: "desktop".to_string(),
            },
        ];

        mr.map(&events);
        let results = mr.reduce();

        assert_eq!(results.get("campaign:c1"), Some(&2));
    }
}
