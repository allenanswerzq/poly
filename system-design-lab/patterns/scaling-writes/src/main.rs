#![allow(dead_code, unused_variables, unused_imports, clippy::all)]
//! # Scaling Writes Pattern Demos
//!
//! This module demonstrates common patterns for scaling write-heavy workloads:
//! 1. Sharding (Horizontal Partitioning)
//! 2. Write Batching
//! 3. Async Write Queue
//! 4. Write-Ahead Log (WAL)

use dashmap::DashMap;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// =============================================================================
// Pattern 1: Sharding
// =============================================================================
// Distribute writes across multiple shards to increase throughput
// Each shard handles 1/N of the total load

struct Shard {
    id: usize,
    data: Mutex<HashMap<String, String>>,
    write_count: AtomicU64,
}

impl Shard {
    fn new(id: usize) -> Self {
        Self {
            id,
            data: Mutex::new(HashMap::new()),
            write_count: AtomicU64::new(0),
        }
    }

    fn write(&self, key: String, value: String) {
        self.write_count.fetch_add(1, Ordering::SeqCst);
        self.data.lock().insert(key, value);
    }

    fn read(&self, key: &str) -> Option<String> {
        self.data.lock().get(key).cloned()
    }
}

struct ShardedDatabase {
    shards: Vec<Arc<Shard>>,
}

impl ShardedDatabase {
    fn new(num_shards: usize) -> Self {
        let shards = (0..num_shards).map(|i| Arc::new(Shard::new(i))).collect();
        Self { shards }
    }

    /// Hash-based shard selection
    fn get_shard(&self, key: &str) -> &Arc<Shard> {
        // Simple hash: sum of bytes mod num_shards
        let hash: usize = key.bytes().map(|b| b as usize).sum();
        &self.shards[hash % self.shards.len()]
    }

    fn write(&self, key: String, value: String) {
        let shard = self.get_shard(&key);
        shard.write(key, value);
    }

    fn read(&self, key: &str) -> Option<String> {
        let shard = self.get_shard(key);
        shard.read(key)
    }

    fn stats(&self) -> Vec<(usize, u64)> {
        self.shards
            .iter()
            .map(|s| (s.id, s.write_count.load(Ordering::SeqCst)))
            .collect()
    }
}

// =============================================================================
// Pattern 2: Write Batching
// =============================================================================
// Collect multiple writes and flush them together
// Trades latency for throughput

struct BatchedWriter {
    buffer: Mutex<Vec<(String, String)>>,
    storage: DashMap<String, String>,
    max_batch_size: usize,
    flush_interval: Duration,
    last_flush: Mutex<Instant>,
    batch_count: AtomicU64,
    write_count: AtomicU64,
}

impl BatchedWriter {
    fn new(max_batch_size: usize, flush_interval: Duration) -> Self {
        Self {
            buffer: Mutex::new(Vec::new()),
            storage: DashMap::new(),
            max_batch_size,
            flush_interval,
            last_flush: Mutex::new(Instant::now()),
            batch_count: AtomicU64::new(0),
            write_count: AtomicU64::new(0),
        }
    }

    fn write(&self, key: String, value: String) {
        self.write_count.fetch_add(1, Ordering::SeqCst);

        let mut buffer = self.buffer.lock();
        buffer.push((key, value));

        let should_flush = buffer.len() >= self.max_batch_size
            || self.last_flush.lock().elapsed() >= self.flush_interval;

        if should_flush {
            self.flush_internal(&mut buffer);
        }
    }

    fn flush_internal(&self, buffer: &mut Vec<(String, String)>) {
        if buffer.is_empty() {
            return;
        }

        self.batch_count.fetch_add(1, Ordering::SeqCst);

        // Simulate batch insert (much faster than individual inserts)
        // In real DB: INSERT INTO table VALUES (k1,v1), (k2,v2), ...
        for (k, v) in buffer.drain(..) {
            self.storage.insert(k, v);
        }

        *self.last_flush.lock() = Instant::now();
    }

    fn flush(&self) {
        let mut buffer = self.buffer.lock();
        self.flush_internal(&mut buffer);
    }

    fn read(&self, key: &str) -> Option<String> {
        // Check buffer first (uncommitted writes)
        let buffer = self.buffer.lock();
        for (k, v) in buffer.iter().rev() {
            if k == key {
                return Some(v.clone());
            }
        }
        drop(buffer);

        // Then check committed storage
        self.storage.get(key).map(|v| v.clone())
    }

    fn stats(&self) -> (u64, u64) {
        (
            self.write_count.load(Ordering::SeqCst),
            self.batch_count.load(Ordering::SeqCst),
        )
    }
}

// =============================================================================
// Pattern 3: Async Write Queue
// =============================================================================
// Accept writes immediately, process in background
// Returns fast, processes eventually

struct WriteRequest {
    key: String,
    value: String,
    timestamp: Instant,
}

struct AsyncWriteQueue {
    queue: Mutex<VecDeque<WriteRequest>>,
    storage: DashMap<String, String>,
    queued_count: AtomicU64,
    processed_count: AtomicU64,
}

impl AsyncWriteQueue {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::new()),
            storage: DashMap::new(),
            queued_count: AtomicU64::new(0),
            processed_count: AtomicU64::new(0),
        })
    }

    /// Enqueue write - returns immediately
    fn write_async(&self, key: String, value: String) {
        self.queued_count.fetch_add(1, Ordering::SeqCst);
        self.queue.lock().push_back(WriteRequest {
            key,
            value,
            timestamp: Instant::now(),
        });
    }

    /// Process one item from queue (called by worker)
    fn process_one(&self) -> Option<Duration> {
        let request = self.queue.lock().pop_front()?;
        self.processed_count.fetch_add(1, Ordering::SeqCst);

        // Simulate slow write
        std::thread::sleep(Duration::from_millis(1));
        self.storage.insert(request.key, request.value);

        Some(request.timestamp.elapsed())
    }

    fn queue_depth(&self) -> usize {
        self.queue.lock().len()
    }

    fn stats(&self) -> (u64, u64) {
        (
            self.queued_count.load(Ordering::SeqCst),
            self.processed_count.load(Ordering::SeqCst),
        )
    }
}

// =============================================================================
// Pattern 4: Write-Ahead Log (WAL)
// =============================================================================
// Append to sequential log first (fast), then update data structure (slow)
// Provides durability with good performance

#[derive(Clone)]
struct WalEntry {
    sequence: u64,
    key: String,
    value: String,
}

struct WriteAheadLog {
    log: Mutex<Vec<WalEntry>>,
    data: Mutex<HashMap<String, String>>,
    sequence: AtomicU64,
    log_writes: AtomicU64,
    data_writes: AtomicU64,
}

impl WriteAheadLog {
    fn new() -> Self {
        Self {
            log: Mutex::new(Vec::new()),
            data: Mutex::new(HashMap::new()),
            sequence: AtomicU64::new(0),
            log_writes: AtomicU64::new(0),
            data_writes: AtomicU64::new(0),
        }
    }

    /// Write with WAL - fast sequential append
    fn write(&self, key: String, value: String) {
        // Step 1: Append to WAL (sequential write - fast)
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        self.log_writes.fetch_add(1, Ordering::SeqCst);

        // Simulating sequential disk append (~0.1ms)
        self.log.lock().push(WalEntry {
            sequence: seq,
            key: key.clone(),
            value: value.clone(),
        });

        // Step 2: Update in-memory data structure
        // (In real systems, this might be batched/async)
        self.data_writes.fetch_add(1, Ordering::SeqCst);
        self.data.lock().insert(key, value);
    }

    fn read(&self, key: &str) -> Option<String> {
        self.data.lock().get(key).cloned()
    }

    /// Replay WAL to recover state (after crash)
    fn replay(&self) -> usize {
        let log = self.log.lock();
        let mut data = self.data.lock();
        data.clear();

        for entry in log.iter() {
            data.insert(entry.key.clone(), entry.value.clone());
        }

        log.len()
    }

    /// Compact WAL by keeping only latest value per key
    fn compact(&self) -> usize {
        let mut log = self.log.lock();
        let mut latest: HashMap<String, WalEntry> = HashMap::new();

        for entry in log.drain(..) {
            latest.insert(entry.key.clone(), entry);
        }

        let compacted: Vec<_> = latest.into_values().collect();
        let count = compacted.len();
        *log = compacted;
        count
    }

    fn stats(&self) -> (u64, u64, usize) {
        (
            self.log_writes.load(Ordering::SeqCst),
            self.data_writes.load(Ordering::SeqCst),
            self.log.lock().len(),
        )
    }
}

// =============================================================================
// Demo
// =============================================================================

fn main() {
    println!("=== Scaling Writes Pattern Demos ===\n");

    // Demo 1: Sharding
    println!("\n  ═══ Pattern 1: Sharding ═══");
    let db = ShardedDatabase::new(4); // 4 shards

    // Write 1000 items
    for i in 0..1000 {
        db.write(format!("key:{}", i), format!("value:{}", i));
    }

    println!("Distribution across 4 shards:");
    for (id, count) in db.stats() {
        println!("  Shard {}: {} writes", id, count);
    }

    // Verify reads work
    assert_eq!(db.read("key:42"), Some("value:42".to_string()));
    println!("Read key:42 = {:?}", db.read("key:42"));
    println!();

    // Demo 2: Write Batching
    println!("\n  ═══ Pattern 2: Write Batching ═══");
    let batcher = BatchedWriter::new(100, Duration::from_millis(50));

    let start = Instant::now();
    for i in 0..500 {
        batcher.write(format!("k{}", i), format!("v{}", i));
    }
    batcher.flush();
    println!("500 writes completed in {:?}", start.elapsed());

    let (writes, batches) = batcher.stats();
    println!("Total writes: {}, Batches: {}", writes, batches);
    println!("Average batch size: {:.1}", writes as f64 / batches as f64);
    println!();

    // Demo 3: Async Write Queue
    println!("\n  ═══ Pattern 3: Async Write Queue ═══");
    let queue = AsyncWriteQueue::new();
    let queue_clone = Arc::clone(&queue);

    // Start worker thread
    let worker = std::thread::spawn(move || {
        let mut total_latency = Duration::ZERO;
        let mut count = 0;
        while let Some(latency) = queue_clone.process_one() {
            total_latency += latency;
            count += 1;
        }
        (count, total_latency)
    });

    // Enqueue writes (fast!)
    let start = Instant::now();
    for i in 0..100 {
        queue.write_async(format!("async_key:{}", i), format!("async_val:{}", i));
    }
    println!("Enqueued 100 writes in {:?}", start.elapsed());
    println!("Queue depth: {}", queue.queue_depth());

    // Wait for processing
    std::thread::sleep(Duration::from_millis(200));

    let (queued, processed) = queue.stats();
    println!("Queued: {}, Processed: {}", queued, processed);

    drop(queue); // Allow worker to finish
    if let Ok((count, total)) = worker.join() {
        if count > 0 {
            println!(
                "Average write latency: {:?}",
                total / count.try_into().unwrap_or(1)
            );
        }
    }
    println!();

    // Demo 4: Write-Ahead Log
    println!("\n  ═══ Pattern 4: Write-Ahead Log ═══");
    let wal = WriteAheadLog::new();

    // Write some data
    for i in 0..100 {
        wal.write(format!("wal_key:{}", i), format!("wal_val:{}", i));
    }

    // Overwrite some keys (creates multiple log entries)
    for i in 0..50 {
        wal.write(format!("wal_key:{}", i), format!("wal_val_updated:{}", i));
    }

    let (log_writes, data_writes, log_size) = wal.stats();
    println!(
        "Log writes: {}, Data writes: {}, Log size: {}",
        log_writes, data_writes, log_size
    );

    // Compact
    let after_compact = wal.compact();
    println!(
        "After compaction: {} entries (was {})",
        after_compact, log_size
    );

    // Simulate crash recovery
    let recovered = wal.replay();
    println!("Recovered {} entries from WAL", recovered);

    // Verify data
    println!("Read wal_key:25 = {:?}", wal.read("wal_key:25"));

    println!("\n=== Key Takeaways ===");
    println!("1. Sharding: Distribute writes across nodes (N nodes = Nx throughput)");
    println!("2. Batching: Trade latency for throughput (fewer round trips)");
    println!("3. Async Queue: Fast ack, eventual persistence (good for non-critical)");
    println!("4. WAL: Sequential writes fast, random writes slow (used by all DBs)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sharding_distribution() {
        let db = ShardedDatabase::new(4);
        for i in 0..100 {
            db.write(format!("key{}", i), format!("val{}", i));
        }

        let total: u64 = db.stats().iter().map(|(_, c)| c).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_batching() {
        let batcher = BatchedWriter::new(10, Duration::from_secs(10));

        for i in 0..25 {
            batcher.write(format!("k{}", i), format!("v{}", i));
        }
        batcher.flush();

        let (writes, batches) = batcher.stats();
        assert_eq!(writes, 25);
        assert!(batches >= 2); // At least 2 batches for 25 items with batch size 10
    }

    #[test]
    fn test_wal_recovery() {
        let wal = WriteAheadLog::new();
        wal.write("key1".to_string(), "value1".to_string());
        wal.write("key2".to_string(), "value2".to_string());

        // Simulate crash - clear data
        wal.data.lock().clear();

        // Recovery from WAL
        let recovered = wal.replay();
        assert_eq!(recovered, 2);
        assert_eq!(wal.read("key1"), Some("value1".to_string()));
    }
}
