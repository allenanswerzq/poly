//! # Metrics Monitoring System - Mini Implementation
//!
//! Demonstrates:
//! - Time-series storage
//! - Metric aggregation (counter, gauge, histogram)
//! - Alerting rules
//! - Downsampling for long-term storage
//!
//! Run: cargo run -p metrics-monitoring

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone)]
struct DataPoint {
    timestamp: u64,
    value: f64,
    labels: HashMap<String, String>,
}

#[derive(Debug, Clone)]
enum MetricType {
    Counter,   // Only increases
    Gauge,     // Can go up or down
    Histogram, // Distribution of values
}

#[derive(Debug, Clone)]
struct MetricDefinition {
    name: String,
    metric_type: MetricType,
    description: String,
    labels: Vec<String>,
}

// =============================================================================
// Time Series Storage
// =============================================================================

struct TimeSeriesChunk {
    start_time: u64,
    data: RwLock<Vec<(u64, f64)>>, // (timestamp, value)
}

impl TimeSeriesChunk {
    fn new(start_time: u64) -> Self {
        Self {
            start_time,
            data: RwLock::new(Vec::new()),
        }
    }

    fn append(&self, timestamp: u64, value: f64) {
        self.data.write().push((timestamp, value));
    }

    fn range(&self, start: u64, end: u64) -> Vec<(u64, f64)> {
        self.data
            .read()
            .iter()
            .filter(|(ts, _)| *ts >= start && *ts <= end)
            .cloned()
            .collect()
    }
}

struct TimeSeries {
    name: String,
    labels: HashMap<String, String>,
    chunks: RwLock<BTreeMap<u64, TimeSeriesChunk>>,
    chunk_duration: Duration,
}

impl TimeSeries {
    fn new(name: &str, labels: HashMap<String, String>, chunk_duration: Duration) -> Self {
        Self {
            name: name.to_string(),
            labels,
            chunks: RwLock::new(BTreeMap::new()),
            chunk_duration,
        }
    }

    fn append(&self, timestamp: u64, value: f64) {
        let chunk_start = (timestamp / self.chunk_duration.as_millis() as u64)
            * self.chunk_duration.as_millis() as u64;

        let mut chunks = self.chunks.write();
        let chunk = chunks
            .entry(chunk_start)
            .or_insert_with(|| TimeSeriesChunk::new(chunk_start));

        chunk.append(timestamp, value);
    }

    fn query(&self, start: u64, end: u64) -> Vec<(u64, f64)> {
        let chunks = self.chunks.read();
        let mut results = Vec::new();

        for (_, chunk) in chunks.range(start..) {
            if chunk.start_time > end {
                break;
            }
            results.extend(chunk.range(start, end));
        }

        results.sort_by_key(|(ts, _)| *ts);
        results
    }
}

// =============================================================================
// Metrics Collectors
// =============================================================================

struct Counter {
    value: AtomicU64,
    series: TimeSeries,
}

impl Counter {
    fn new(name: &str, labels: HashMap<String, String>) -> Self {
        Self {
            value: AtomicU64::new(0),
            series: TimeSeries::new(name, labels, Duration::from_secs(60)),
        }
    }

    fn inc(&self) {
        self.inc_by(1);
    }

    fn inc_by(&self, n: u64) {
        let new_val = self.value.fetch_add(n, Ordering::SeqCst) + n;
        let timestamp = Instant::now().elapsed().as_millis() as u64;
        self.series.append(timestamp, new_val as f64);
    }

    fn get(&self) -> u64 {
        self.value.load(Ordering::SeqCst)
    }
}

struct Gauge {
    value: RwLock<f64>,
    series: TimeSeries,
}

impl Gauge {
    fn new(name: &str, labels: HashMap<String, String>) -> Self {
        Self {
            value: RwLock::new(0.0),
            series: TimeSeries::new(name, labels, Duration::from_secs(60)),
        }
    }

    fn set(&self, value: f64) {
        *self.value.write() = value;
        let timestamp = Instant::now().elapsed().as_millis() as u64;
        self.series.append(timestamp, value);
    }

    fn inc(&self) {
        self.add(1.0);
    }

    fn dec(&self) {
        self.add(-1.0);
    }

    fn add(&self, delta: f64) {
        let mut v = self.value.write();
        *v += delta;
        let timestamp = Instant::now().elapsed().as_millis() as u64;
        self.series.append(timestamp, *v);
    }

    fn get(&self) -> f64 {
        *self.value.read()
    }
}

struct Histogram {
    buckets: Vec<f64>,
    counts: Vec<AtomicU64>,
    sum: AtomicU64, // Stored as bits
    count: AtomicU64,
}

impl Histogram {
    fn new(buckets: Vec<f64>) -> Self {
        let counts = buckets.iter().map(|_| AtomicU64::new(0)).collect();
        Self {
            buckets,
            counts,
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    fn observe(&self, value: f64) {
        // Update bucket counts
        for (i, bucket) in self.buckets.iter().enumerate() {
            if value <= *bucket {
                self.counts[i].fetch_add(1, Ordering::SeqCst);
            }
        }

        // Update sum and count
        self.sum.fetch_add(value.to_bits(), Ordering::SeqCst);
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    fn percentile(&self, p: f64) -> f64 {
        let total = self.count.load(Ordering::SeqCst) as f64;
        if total == 0.0 {
            return 0.0;
        }

        let target = total * p;
        for (i, bucket) in self.buckets.iter().enumerate() {
            let count = self.counts[i].load(Ordering::SeqCst) as f64;
            if count >= target {
                return *bucket;
            }
        }

        *self.buckets.last().unwrap_or(&0.0)
    }

    fn mean(&self) -> f64 {
        let count = self.count.load(Ordering::SeqCst);
        if count == 0 {
            return 0.0;
        }

        let sum_bits = self.sum.load(Ordering::SeqCst);
        f64::from_bits(sum_bits) / count as f64
    }
}

// =============================================================================
// Alerting
// =============================================================================

#[derive(Debug, Clone)]
struct AlertRule {
    name: String,
    metric: String,
    condition: AlertCondition,
    duration: Duration,
    severity: AlertSeverity,
}

#[derive(Debug, Clone)]
enum AlertCondition {
    GreaterThan(f64),
    LessThan(f64),
    Absent(Duration),
}

#[derive(Debug, Clone, Copy)]
enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
struct Alert {
    rule_name: String,
    metric: String,
    value: f64,
    severity: AlertSeverity,
    fired_at: Instant,
    resolved_at: Option<Instant>,
}

struct AlertManager {
    rules: RwLock<Vec<AlertRule>>,
    active_alerts: DashMap<String, Alert>,
    alert_history: Mutex<VecDeque<Alert>>,
}

impl AlertManager {
    fn new() -> Self {
        Self {
            rules: RwLock::new(Vec::new()),
            active_alerts: DashMap::new(),
            alert_history: Mutex::new(VecDeque::new()),
        }
    }

    fn add_rule(&self, rule: AlertRule) {
        self.rules.write().push(rule);
    }

    fn check(&self, metric: &str, value: f64) -> Vec<Alert> {
        let mut fired = Vec::new();
        let rules = self.rules.read();

        for rule in rules.iter() {
            if rule.metric != metric {
                continue;
            }

            let should_fire = match rule.condition {
                AlertCondition::GreaterThan(threshold) => value > threshold,
                AlertCondition::LessThan(threshold) => value < threshold,
                AlertCondition::Absent(_) => false, // Checked separately
            };

            if should_fire {
                let alert = Alert {
                    rule_name: rule.name.clone(),
                    metric: metric.to_string(),
                    value,
                    severity: rule.severity,
                    fired_at: Instant::now(),
                    resolved_at: None,
                };

                self.active_alerts.insert(rule.name.clone(), alert.clone());
                fired.push(alert);
            } else if self.active_alerts.contains_key(&rule.name) {
                // Resolve alert
                if let Some(mut alert) = self.active_alerts.get_mut(&rule.name) {
                    alert.resolved_at = Some(Instant::now());
                    self.alert_history.lock().push_back(alert.clone());
                }
                self.active_alerts.remove(&rule.name);
            }
        }

        fired
    }

    fn get_active(&self) -> Vec<Alert> {
        self.active_alerts.iter().map(|e| e.value().clone()).collect()
    }
}

// =============================================================================
// Downsampling
// =============================================================================

fn downsample(data: &[(u64, f64)], target_interval: u64) -> Vec<(u64, f64, f64, f64)> {
    // Returns (timestamp, min, max, avg)
    let mut result = Vec::new();

    if data.is_empty() {
        return result;
    }

    let mut bucket_start = (data[0].0 / target_interval) * target_interval;
    let mut bucket_values: Vec<f64> = Vec::new();

    for (ts, value) in data {
        let value_bucket = (*ts / target_interval) * target_interval;

        if value_bucket != bucket_start {
            // Emit previous bucket
            if !bucket_values.is_empty() {
                let min = bucket_values.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = bucket_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let avg: f64 = bucket_values.iter().sum::<f64>() / bucket_values.len() as f64;
                result.push((bucket_start, min, max, avg));
            }

            bucket_start = value_bucket;
            bucket_values.clear();
        }

        bucket_values.push(*value);
    }

    // Don't forget last bucket
    if !bucket_values.is_empty() {
        let min = bucket_values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = bucket_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let avg: f64 = bucket_values.iter().sum::<f64>() / bucket_values.len() as f64;
        result.push((bucket_start, min, max, avg));
    }

    result
}

// =============================================================================
// Metrics Registry
// =============================================================================

struct MetricsRegistry {
    counters: DashMap<String, Counter>,
    gauges: DashMap<String, Gauge>,
    histograms: DashMap<String, Histogram>,
    alert_manager: AlertManager,
}

impl MetricsRegistry {
    fn new() -> Self {
        Self {
            counters: DashMap::new(),
            gauges: DashMap::new(),
            histograms: DashMap::new(),
            alert_manager: AlertManager::new(),
        }
    }

    fn counter(&self, name: &str, labels: HashMap<String, String>) -> &Counter {
        let key = format!("{}_{:?}", name, labels);
        self.counters
            .entry(key)
            .or_insert_with(|| Counter::new(name, labels));

        // Return reference (safe because DashMap entries are stable)
        unsafe {
            &*(self.counters.get(&format!("{}_{:?}", name, HashMap::<String, String>::new()))
                .map(|e| &*e as *const Counter)
                .unwrap_or(std::ptr::null()))
        }
    }

    fn gauge(&self, name: &str) {
        let key = name.to_string();
        self.gauges
            .entry(key)
            .or_insert_with(|| Gauge::new(name, HashMap::new()));
    }

    fn histogram(&self, name: &str, buckets: Vec<f64>) {
        let key = name.to_string();
        self.histograms
            .entry(key)
            .or_insert_with(|| Histogram::new(buckets));
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== Metrics Monitoring System Demo ===\n");

    // Create metrics
    println!("--- Creating Metrics ---");

    let http_requests = Counter::new("http_requests_total", HashMap::new());
    let active_connections = Gauge::new("active_connections", HashMap::new());
    let request_duration = Histogram::new(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]);

    // Simulate metrics
    println!("Simulating traffic...\n");

    let mut rng = rand::thread_rng();

    // HTTP requests
    for _ in 0..1000 {
        http_requests.inc();

        // Observe request duration
        let duration = rng.gen_range(0.01..0.5);
        request_duration.observe(duration);
    }

    // Active connections (fluctuating)
    for i in 0..50 {
        let connections = 100.0 + (i as f64).sin() * 20.0;
        active_connections.set(connections);
    }

    // Show metrics
    println!("--- Metric Values ---");
    println!("http_requests_total: {}", http_requests.get());
    println!("active_connections: {:.0}", active_connections.get());
    println!();

    println!("--- Histogram Percentiles ---");
    println!("request_duration p50: {:.3}s", request_duration.percentile(0.5));
    println!("request_duration p90: {:.3}s", request_duration.percentile(0.9));
    println!("request_duration p99: {:.3}s", request_duration.percentile(0.99));
    println!();

    // Alerting
    println!("--- Alerting ---");
    let alerts = AlertManager::new();

    alerts.add_rule(AlertRule {
        name: "HighRequestRate".to_string(),
        metric: "http_requests_total".to_string(),
        condition: AlertCondition::GreaterThan(500.0),
        duration: Duration::from_secs(60),
        severity: AlertSeverity::Warning,
    });

    alerts.add_rule(AlertRule {
        name: "LowConnections".to_string(),
        metric: "active_connections".to_string(),
        condition: AlertCondition::LessThan(10.0),
        duration: Duration::from_secs(60),
        severity: AlertSeverity::Critical,
    });

    // Check alerts
    let fired = alerts.check("http_requests_total", http_requests.get() as f64);
    for alert in &fired {
        println!("🚨 Alert fired: {} ({:?})", alert.rule_name, alert.severity);
        println!("   {} = {}", alert.metric, alert.value);
    }

    // No alert for normal connections
    alerts.check("active_connections", active_connections.get());

    println!("\nActive alerts: {}", alerts.get_active().len());
    println!();

    // Downsampling demo
    println!("--- Downsampling ---");
    let raw_data: Vec<(u64, f64)> = (0..100)
        .map(|i| (i * 1000, 100.0 + rng.gen_range(-10.0..10.0)))
        .collect();

    println!("Raw data points: {}", raw_data.len());

    let downsampled = downsample(&raw_data, 10000); // 10-second buckets
    println!("Downsampled to {} points (10s buckets)", downsampled.len());

    for (ts, min, max, avg) in downsampled.iter().take(3) {
        println!("  ts={}: min={:.1}, max={:.1}, avg={:.1}", ts, min, max, avg);
    }
    println!();

    // Time series query
    println!("--- Time Series Query ---");
    let ts = TimeSeries::new("test_metric", HashMap::new(), Duration::from_secs(60));

    for i in 0..100 {
        ts.append(i * 1000, 50.0 + rng.gen_range(-5.0..5.0));
    }

    let results = ts.query(20000, 30000);
    println!(
        "Query [20s-30s]: {} data points",
        results.len()
    );

    println!("\n=== Key Concepts ===");
    println!("1. Metric Types: Counter (inc only), Gauge (any), Histogram (distribution)");
    println!("2. Time Series: Chunked storage for efficient range queries");
    println!("3. Alerting: Rules with conditions, severity, and duration");
    println!("4. Downsampling: Aggregate old data to save storage (min/max/avg)");
    println!("5. Labels: Dimensional metrics for filtering");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let counter = Counter::new("test", HashMap::new());
        counter.inc();
        counter.inc_by(5);
        assert_eq!(counter.get(), 6);
    }

    #[test]
    fn test_gauge() {
        let gauge = Gauge::new("test", HashMap::new());
        gauge.set(10.0);
        assert_eq!(gauge.get(), 10.0);

        gauge.add(5.0);
        assert_eq!(gauge.get(), 15.0);

        gauge.dec();
        assert_eq!(gauge.get(), 14.0);
    }

    #[test]
    fn test_histogram() {
        let hist = Histogram::new(vec![1.0, 5.0, 10.0]);

        hist.observe(0.5);
        hist.observe(2.0);
        hist.observe(7.0);

        assert!(hist.percentile(0.5) <= 5.0);
    }

    #[test]
    fn test_downsampling() {
        let data = vec![
            (0, 10.0),
            (1, 20.0),
            (2, 30.0),
            (10, 15.0),
            (11, 25.0),
        ];

        let result = downsample(&data, 10);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].3, 20.0); // avg of 10, 20, 30
    }
}
