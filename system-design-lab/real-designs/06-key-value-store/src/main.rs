//! # Key-Value Store Implementation
//!
//! A mini LSM-tree based key-value store demonstrating:
//! - MemTable (in-memory sorted storage)
//! - Write-Ahead Log (WAL) for durability
//! - SSTable (sorted string table)
//! - Bloom filter for fast negative lookups
//! - Basic compaction

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

// =============================================================================
// Error Types
// =============================================================================

#[derive(Error, Debug)]
pub enum KvError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Key not found")]
    NotFound,
}

type Result<T> = std::result::Result<T, KvError>;

// =============================================================================
// Bloom Filter
// =============================================================================

/// A simple bloom filter for probabilistic key existence checks
pub struct BloomFilter {
    bits: Vec<bool>,
    num_hashes: usize,
}

impl BloomFilter {
    pub fn new(expected_elements: usize, false_positive_rate: f64) -> Self {
        // Calculate optimal size and number of hash functions
        let bits_count = Self::optimal_bits(expected_elements, false_positive_rate);
        let num_hashes = Self::optimal_hashes(bits_count, expected_elements);

        Self {
            bits: vec![false; bits_count],
            num_hashes,
        }
    }

    fn optimal_bits(n: usize, p: f64) -> usize {
        let ln2_squared = std::f64::consts::LN_2.powi(2);
        (-(n as f64) * p.ln() / ln2_squared).ceil() as usize
    }

    fn optimal_hashes(m: usize, n: usize) -> usize {
        ((m as f64 / n as f64) * std::f64::consts::LN_2).ceil() as usize
    }

    /// Hash functions using double hashing technique
    fn get_hashes(&self, key: &str) -> Vec<usize> {
        let h1 = Self::hash1(key);
        let h2 = Self::hash2(key);
        let m = self.bits.len();

        (0..self.num_hashes)
            .map(|i| ((h1.wrapping_add(i as u64).wrapping_mul(h2)) % m as u64) as usize)
            .collect()
    }

    fn hash1(key: &str) -> u64 {
        // Simple hash
        let mut hash: u64 = 5381;
        for byte in key.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
        }
        hash
    }

    fn hash2(key: &str) -> u64 {
        // Different hash
        let mut hash: u64 = 0;
        for byte in key.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash
    }

    pub fn insert(&mut self, key: &str) {
        for idx in self.get_hashes(key) {
            self.bits[idx] = true;
        }
    }

    /// Returns true if key might exist, false if definitely doesn't
    pub fn might_contain(&self, key: &str) -> bool {
        self.get_hashes(key).iter().all(|&idx| self.bits[idx])
    }
}

// =============================================================================
// MemTable (In-Memory Storage)
// =============================================================================

/// In-memory sorted storage using BTreeMap
pub struct MemTable {
    data: BTreeMap<String, Option<Vec<u8>>>,  // None = tombstone (deleted)
    size_bytes: usize,
    max_size: usize,
}

impl MemTable {
    pub fn new(max_size: usize) -> Self {
        Self {
            data: BTreeMap::new(),
            size_bytes: 0,
            max_size,
        }
    }

    pub fn put(&mut self, key: String, value: Vec<u8>) {
        self.size_bytes += key.len() + value.len();
        self.data.insert(key, Some(value));
    }

    pub fn get(&self, key: &str) -> Option<&Option<Vec<u8>>> {
        self.data.get(key)
    }

    pub fn delete(&mut self, key: String) {
        self.size_bytes += key.len();
        self.data.insert(key, None);  // Tombstone
    }

    pub fn is_full(&self) -> bool {
        self.size_bytes >= self.max_size
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Option<Vec<u8>>)> {
        self.data.iter()
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.size_bytes = 0;
    }
}

// =============================================================================
// Write-Ahead Log
// =============================================================================

#[derive(Serialize, Deserialize, Debug)]
enum WalEntry {
    Put { key: String, value: Vec<u8> },
    Delete { key: String },
}

/// Write-Ahead Log for durability
pub struct Wal {
    file: BufWriter<File>,
    path: PathBuf,
}

impl Wal {
    pub fn new(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        Ok(Self {
            file: BufWriter::new(file),
            path: path.to_path_buf(),
        })
    }

    pub fn append_put(&mut self, key: &str, value: &[u8]) -> Result<()> {
        let entry = WalEntry::Put {
            key: key.to_string(),
            value: value.to_vec(),
        };
        self.append_entry(&entry)
    }

    pub fn append_delete(&mut self, key: &str) -> Result<()> {
        let entry = WalEntry::Delete {
            key: key.to_string(),
        };
        self.append_entry(&entry)
    }

    fn append_entry(&mut self, entry: &WalEntry) -> Result<()> {
        let json = serde_json::to_string(entry)?;
        writeln!(self.file, "{}", json)?;
        self.file.flush()?;
        Ok(())
    }

    /// Replay WAL entries to rebuild MemTable
    pub fn replay(path: &Path) -> Result<Vec<WalEntry>> {
        if !path.exists() {
            return Ok(vec![]);
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut entries = vec![];

        for line in reader.lines() {
            let line = line?;
            if !line.is_empty() {
                let entry: WalEntry = serde_json::from_str(&line)?;
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    pub fn clear(&mut self) -> Result<()> {
        // Truncate the file
        let file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.path)?;
        self.file = BufWriter::new(file);
        Ok(())
    }
}

// =============================================================================
// SSTable (Sorted String Table)
// =============================================================================

#[derive(Serialize, Deserialize)]
struct SSTableEntry {
    key: String,
    value: Option<Vec<u8>>,  // None = deleted
}

/// Immutable sorted file on disk
pub struct SSTable {
    path: PathBuf,
    bloom_filter: BloomFilter,
    index: Vec<(String, u64)>,  // (first_key_in_block, offset)
}

impl SSTable {
    /// Create SSTable from MemTable
    pub fn write_from_memtable(path: &Path, memtable: &MemTable) -> Result<Self> {
        let mut bloom = BloomFilter::new(memtable.len(), 0.01);
        let mut index = vec![];

        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        let mut offset = 0u64;
        let mut first_key_in_block = None;
        let block_size = 4096;

        for (key, value) in memtable.iter() {
            // Add to bloom filter
            bloom.insert(key);

            // Track block boundaries
            if first_key_in_block.is_none() {
                first_key_in_block = Some(key.clone());
                index.push((key.clone(), offset));
            }

            // Write entry
            let entry = SSTableEntry {
                key: key.clone(),
                value: value.clone(),
            };
            let json = serde_json::to_string(&entry)?;
            writeln!(writer, "{}", json)?;
            offset += json.len() as u64 + 1;

            // Start new block if needed
            if offset % block_size == 0 {
                first_key_in_block = None;
            }
        }

        writer.flush()?;

        Ok(Self {
            path: path.to_path_buf(),
            bloom_filter: bloom,
            index,
        })
    }

    /// Check if key might exist
    pub fn might_contain(&self, key: &str) -> bool {
        self.bloom_filter.might_contain(key)
    }

    /// Get value for key from disk
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        // Fast path: bloom filter says no
        if !self.might_contain(key) {
            return Ok(None);
        }

        // Find the block that might contain this key
        let _block_offset = self.find_block(key);

        // Linear scan through file (simplified - real impl would use index)
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            let entry: SSTableEntry = serde_json::from_str(&line)?;

            if entry.key == key {
                return Ok(entry.value);
            }

            // Since sorted, we can stop early
            if entry.key > key.to_string() {
                break;
            }
        }

        Ok(None)
    }

    fn find_block(&self, key: &str) -> u64 {
        // Binary search to find the right block
        let pos = self.index.binary_search_by(|(k, _)| k.as_str().cmp(key));

        match pos {
            Ok(idx) => self.index[idx].1,
            Err(0) => 0,
            Err(idx) => self.index[idx - 1].1,
        }
    }
}

// =============================================================================
// KV Store (Full Implementation)
// =============================================================================

/// The main key-value store
pub struct KvStore {
    memtable: RwLock<MemTable>,
    immutable_memtable: RwLock<Option<MemTable>>,
    wal: RwLock<Wal>,
    sstables: RwLock<Vec<SSTable>>,
    data_dir: PathBuf,
    sstable_counter: AtomicU64,
}

impl KvStore {
    pub fn open(path: &Path) -> Result<Self> {
        fs::create_dir_all(path)?;

        let wal_path = path.join("wal.log");

        // Replay WAL to rebuild memtable
        let mut memtable = MemTable::new(64 * 1024);  // 64KB memtable
        for entry in Wal::replay(&wal_path)? {
            match entry {
                WalEntry::Put { key, value } => memtable.put(key, value),
                WalEntry::Delete { key } => memtable.delete(key),
            }
        }

        let wal = Wal::new(&wal_path)?;

        Ok(Self {
            memtable: RwLock::new(memtable),
            immutable_memtable: RwLock::new(None),
            wal: RwLock::new(wal),
            sstables: RwLock::new(vec![]),
            data_dir: path.to_path_buf(),
            sstable_counter: AtomicU64::new(0),
        })
    }

    /// Put a key-value pair
    pub fn put(&self, key: &str, value: &[u8]) -> Result<()> {
        // 1. Append to WAL first (durability)
        self.wal.write().append_put(key, value)?;

        // 2. Write to memtable
        let mut memtable = self.memtable.write();
        memtable.put(key.to_string(), value.to_vec());

        // 3. Check if memtable needs flushing
        if memtable.is_full() {
            drop(memtable);  // Release lock before flush
            self.flush_memtable()?;
        }

        Ok(())
    }

    /// Get a value by key
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        // 1. Check memtable first
        if let Some(value) = self.memtable.read().get(key) {
            return Ok(value.clone());
        }

        // 2. Check immutable memtable
        if let Some(ref imm) = *self.immutable_memtable.read() {
            if let Some(value) = imm.get(key) {
                return Ok(value.clone());
            }
        }

        // 3. Check SSTables (newest to oldest)
        let sstables = self.sstables.read();
        for sstable in sstables.iter().rev() {
            if let Some(value) = sstable.get(key)? {
                return Ok(Some(value));
            }
        }

        Ok(None)
    }

    /// Delete a key
    pub fn delete(&self, key: &str) -> Result<()> {
        // Write tombstone
        self.wal.write().append_delete(key)?;
        self.memtable.write().delete(key.to_string());
        Ok(())
    }

    /// Flush memtable to disk as SSTable
    fn flush_memtable(&self) -> Result<()> {
        // Swap memtable with empty one
        let mut memtable = self.memtable.write();
        let old_memtable = std::mem::replace(&mut *memtable, MemTable::new(64 * 1024));
        drop(memtable);

        // Store as immutable while flushing
        *self.immutable_memtable.write() = Some(old_memtable);

        // Write SSTable
        let counter = self.sstable_counter.fetch_add(1, Ordering::SeqCst);
        let sstable_path = self.data_dir.join(format!("sstable_{}.dat", counter));

        let imm = self.immutable_memtable.read();
        if let Some(ref memtable) = *imm {
            let sstable = SSTable::write_from_memtable(&sstable_path, memtable)?;
            drop(imm);

            self.sstables.write().push(sstable);
            *self.immutable_memtable.write() = None;

            // Clear WAL after successful flush
            self.wal.write().clear()?;
        }

        Ok(())
    }

    /// Force flush (for testing/shutdown)
    pub fn force_flush(&self) -> Result<()> {
        if !self.memtable.read().is_empty() {
            self.flush_memtable()?;
        }
        Ok(())
    }
}

// =============================================================================
// Demonstration
// =============================================================================

fn main() -> Result<()> {
    println!("=== Key-Value Store Demo ===\n");

    // Create temp directory for demo
    let temp_dir = std::env::temp_dir().join("kv_store_demo");
    let _ = fs::remove_dir_all(&temp_dir);  // Clean up any existing

    // Demo 1: Bloom Filter
    println!("--- Bloom Filter Demo ---");
    let mut bloom = BloomFilter::new(1000, 0.01);

    let keys = ["apple", "banana", "cherry", "date", "elderberry"];
    for key in &keys {
        bloom.insert(key);
    }

    println!("Inserted: {:?}", keys);
    println!("Contains 'apple': {}", bloom.might_contain("apple"));
    println!("Contains 'grape': {} (should be false, but may have false positive)",
             bloom.might_contain("grape"));
    println!("Contains 'mango': {}", bloom.might_contain("mango"));

    // Demo 2: MemTable
    println!("\n--- MemTable Demo ---");
    let mut memtable = MemTable::new(1024);

    memtable.put("key1".to_string(), b"value1".to_vec());
    memtable.put("key2".to_string(), b"value2".to_vec());
    memtable.put("key3".to_string(), b"value3".to_vec());

    println!("MemTable entries: {}", memtable.len());
    println!("Get 'key2': {:?}", memtable.get("key2").map(|v| v.as_ref().map(|b| String::from_utf8_lossy(b).to_string())));

    memtable.delete("key2".to_string());
    println!("After delete 'key2': {:?}", memtable.get("key2"));

    // Demo 3: Full KV Store
    println!("\n--- KV Store Demo ---");

    let store = KvStore::open(&temp_dir)?;

    // Write some data
    println!("Writing data...");
    store.put("user:1", b"Alice")?;
    store.put("user:2", b"Bob")?;
    store.put("user:3", b"Charlie")?;

    // Read back
    println!("Reading data...");
    for id in 1..=4 {
        let key = format!("user:{}", id);
        let value = store.get(&key)?;
        match value {
            Some(v) => println!("  {} = {}", key, String::from_utf8_lossy(&v)),
            None => println!("  {} = NOT FOUND", key),
        }
    }

    // Update
    println!("\nUpdating user:2...");
    store.put("user:2", b"Bobby")?;
    println!("  user:2 = {:?}", store.get("user:2")?.map(|v| String::from_utf8_lossy(&v).to_string()));

    // Delete
    println!("\nDeleting user:3...");
    store.delete("user:3")?;
    println!("  user:3 = {:?}", store.get("user:3")?);

    // Write enough to trigger flush
    println!("\nWriting more data to trigger flush...");
    for i in 0..1000 {
        store.put(&format!("item:{}", i), format!("value_{}", i).as_bytes())?;
    }
    store.force_flush()?;
    println!("Flush complete");

    // Verify data after flush
    println!("\nReading after flush...");
    println!("  item:0 = {:?}", store.get("item:0")?.map(|v| String::from_utf8_lossy(&v).to_string()));
    println!("  item:500 = {:?}", store.get("item:500")?.map(|v| String::from_utf8_lossy(&v).to_string()));
    println!("  item:999 = {:?}", store.get("item:999")?.map(|v| String::from_utf8_lossy(&v).to_string()));

    // Demo 4: Recovery from WAL
    println!("\n--- WAL Recovery Demo ---");
    drop(store);  // Close store

    let store2 = KvStore::open(&temp_dir)?;
    println!("Reopened store, checking data...");
    println!("  user:1 = {:?}", store2.get("user:1")?.map(|v| String::from_utf8_lossy(&v).to_string()));
    println!("  item:100 = {:?}", store2.get("item:100")?.map(|v| String::from_utf8_lossy(&v).to_string()));

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);

    println!("\n=== Demo Complete ===");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_filter() {
        let mut bloom = BloomFilter::new(100, 0.01);
        bloom.insert("test");

        assert!(bloom.might_contain("test"));
        // Note: might_contain can have false positives
    }

    #[test]
    fn test_memtable() {
        let mut mt = MemTable::new(1024);
        mt.put("key".to_string(), b"value".to_vec());

        assert_eq!(mt.get("key"), Some(&Some(b"value".to_vec())));
        assert_eq!(mt.get("nonexistent"), None);
    }

    #[test]
    fn test_kv_store() -> Result<()> {
        let temp = std::env::temp_dir().join("test_kv");
        let _ = fs::remove_dir_all(&temp);

        let store = KvStore::open(&temp)?;

        store.put("key", b"value")?;
        assert_eq!(store.get("key")?, Some(b"value".to_vec()));

        store.delete("key")?;
        assert_eq!(store.get("key")?, None);

        let _ = fs::remove_dir_all(&temp);
        Ok(())
    }
}
