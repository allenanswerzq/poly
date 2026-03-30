#![allow(dead_code, unused_variables, unused_imports)]
//! # Mini Hadoop
//!
//! Simulates core Hadoop concepts:
//! 1. **HDFS** — Distributed file system with block splitting, replication, NameNode/DataNode
//! 2. **MapReduce** — Distributed computation: Map → Shuffle → Reduce
//! 3. **YARN** — Resource management (simplified)
//!
//! Hadoop = HDFS (storage) + MapReduce (compute) + YARN (scheduling)
//!
//! Key ideas:
//! - Data is split into 128MB blocks, replicated 3x across DataNodes
//! - Computation moves TO the data (not the other way around)
//! - MapReduce: split input → map each split → shuffle by key → reduce per key

use std::collections::HashMap;

// =============================================================================
// HDFS — Hadoop Distributed File System
// =============================================================================
//
// Architecture:
//   NameNode (1 master): metadata — which blocks are where
//   DataNodes (many workers): store actual data blocks
//
//   Client writes "big_file.csv" (400MB):
//     → NameNode splits into 4 blocks (128MB each)
//     → Each block replicated 3x across different DataNodes
//     → NameNode records: block_0 → [DN1, DN3, DN5]
//
//   Client reads "big_file.csv":
//     → Ask NameNode: "where are the blocks?"
//     → Read blocks from nearest DataNode

const BLOCK_SIZE: usize = 128; // simulated block size (128 "units" instead of 128MB)
const REPLICATION_FACTOR: usize = 3;

#[derive(Debug, Clone)]
struct Block {
    id: String,
    data: Vec<u8>,
}

#[derive(Debug)]
struct DataNode {
    name: String,
    blocks: Vec<Block>,
    used_space: usize,
}

impl DataNode {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            blocks: Vec::new(),
            used_space: 0,
        }
    }

    fn store_block(&mut self, block: Block) {
        self.used_space += block.data.len();
        self.blocks.push(block);
    }
}

/// NameNode: metadata server that tracks block → DataNode mapping.
struct NameNode {
    // file_name → list of block_ids (ordered)
    file_map: HashMap<String, Vec<String>>,
    // block_id → list of DataNode names that hold this block
    block_locations: HashMap<String, Vec<String>>,
}

impl NameNode {
    fn new() -> Self {
        Self {
            file_map: HashMap::new(),
            block_locations: HashMap::new(),
        }
    }

    fn register_file(&mut self, filename: &str, block_ids: Vec<String>) {
        self.file_map.insert(filename.to_string(), block_ids);
    }

    fn register_block(&mut self, block_id: &str, datanode: &str) {
        self.block_locations
            .entry(block_id.to_string())
            .or_default()
            .push(datanode.to_string());
    }

    fn get_block_locations(&self, filename: &str) -> Vec<(&str, &[String])> {
        let block_ids = match self.file_map.get(filename) {
            Some(ids) => ids,
            None => return vec![],
        };
        block_ids
            .iter()
            .filter_map(|bid| {
                self.block_locations
                    .get(bid)
                    .map(|locs| (bid.as_str(), locs.as_slice()))
            })
            .collect()
    }
}

/// Simulated HDFS cluster.
struct Hdfs {
    namenode: NameNode,
    datanodes: Vec<DataNode>,
}

impl Hdfs {
    fn new(num_datanodes: usize) -> Self {
        let datanodes = (0..num_datanodes)
            .map(|i| DataNode::new(&format!("datanode-{i}")))
            .collect();
        Self {
            namenode: NameNode::new(),
            datanodes,
        }
    }

    /// Write a file: split into blocks, replicate across DataNodes.
    fn write_file(&mut self, filename: &str, data: &[u8]) {
        let blocks: Vec<_> = data
            .chunks(BLOCK_SIZE)
            .enumerate()
            .map(|(i, chunk)| {
                let block_id = format!("{filename}_block_{i}");
                Block {
                    id: block_id,
                    data: chunk.to_vec(),
                }
            })
            .collect();

        let block_ids: Vec<String> = blocks.iter().map(|b| b.id.clone()).collect();
        self.namenode.register_file(filename, block_ids);

        let num_dn = self.datanodes.len();
        for (i, block) in blocks.iter().enumerate() {
            // Place replicas on different DataNodes (round-robin spread)
            for r in 0..REPLICATION_FACTOR.min(num_dn) {
                let dn_idx = (i + r) % num_dn;
                self.namenode
                    .register_block(&block.id, &self.datanodes[dn_idx].name);
                self.datanodes[dn_idx].store_block(block.clone());
            }
        }
    }

    /// Read a file: ask NameNode for locations, fetch blocks.
    fn read_file(&self, filename: &str) -> Vec<u8> {
        let locations = self.namenode.get_block_locations(filename);
        let mut result = Vec::new();

        for (block_id, datanode_names) in &locations {
            // In real HDFS, pick the nearest DataNode. Here just pick first.
            let dn_name = &datanode_names[0];
            let dn = self.datanodes.iter().find(|d| &d.name == dn_name).unwrap();
            let block = dn.blocks.iter().find(|b| b.id == *block_id).unwrap();
            result.extend_from_slice(&block.data);
        }
        result
    }
}

// =============================================================================
// MapReduce
// =============================================================================
//
//   Input: ["hello world", "hello hadoop", "world hello"]
//
//   Map phase (per split):
//     "hello world"  → [("hello",1), ("world",1)]
//     "hello hadoop" → [("hello",1), ("hadoop",1)]
//     "world hello"  → [("world",1), ("hello",1)]
//
//   Shuffle phase (group by key):
//     "hello"  → [1, 1, 1]
//     "world"  → [1, 1]
//     "hadoop" → [1]
//
//   Reduce phase (aggregate):
//     "hello"  → 3
//     "world"  → 2
//     "hadoop" → 1

type MapFn = fn(&str) -> Vec<(String, i64)>;
type ReduceFn = fn(&str, &[i64]) -> i64;

struct MapReduceJob {
    input_splits: Vec<String>,
    map_fn: MapFn,
    reduce_fn: ReduceFn,
}

impl MapReduceJob {
    fn new(input_splits: Vec<String>, map_fn: MapFn, reduce_fn: ReduceFn) -> Self {
        Self {
            input_splits,
            map_fn,
            reduce_fn,
        }
    }

    fn run(&self) -> HashMap<String, i64> {
        // Map phase: each split produces key-value pairs
        println!("  [MapReduce] Map phase: processing {} splits", self.input_splits.len());
        let mut intermediate: Vec<(String, i64)> = Vec::new();
        for split in &self.input_splits {
            let pairs = (self.map_fn)(split);
            println!("    map(\"{split}\") → {} pairs", pairs.len());
            intermediate.extend(pairs);
        }

        // Shuffle phase: group by key
        println!("  [MapReduce] Shuffle phase: grouping by key");
        let mut grouped: HashMap<String, Vec<i64>> = HashMap::new();
        for (key, value) in intermediate {
            grouped.entry(key).or_default().push(value);
        }
        println!("    {} unique keys", grouped.len());

        // Reduce phase: aggregate each key
        println!("  [MapReduce] Reduce phase:");
        let mut results = HashMap::new();
        for (key, values) in &grouped {
            let result = (self.reduce_fn)(key, values);
            println!("    reduce(\"{key}\", {:?}) → {result}", values);
            results.insert(key.clone(), result);
        }

        results
    }
}

// Built-in map/reduce functions
fn word_count_map(line: &str) -> Vec<(String, i64)> {
    line.split_whitespace()
        .map(|word| (word.to_lowercase(), 1))
        .collect()
}

fn sum_reduce(_key: &str, values: &[i64]) -> i64 {
    values.iter().sum()
}

fn max_reduce(_key: &str, values: &[i64]) -> i64 {
    *values.iter().max().unwrap_or(&0)
}

// =============================================================================
// YARN — Yet Another Resource Negotiator (simplified)
// =============================================================================

#[derive(Debug, Clone)]
struct Container {
    id: String,
    cpu_cores: u32,
    memory_mb: u32,
    node: String,
}

struct YarnResourceManager {
    nodes: Vec<(String, u32, u32)>, // (name, total_cpu, total_mem)
    allocated: Vec<Container>,
}

impl YarnResourceManager {
    fn new(nodes: Vec<(&str, u32, u32)>) -> Self {
        Self {
            nodes: nodes
                .into_iter()
                .map(|(n, c, m)| (n.to_string(), c, m))
                .collect(),
            allocated: Vec::new(),
        }
    }

    fn request_container(&mut self, app_id: &str, cpu: u32, mem: u32) -> Option<Container> {
        // Find node with enough resources (simplified — no tracking used resources)
        for (name, total_cpu, total_mem) in &self.nodes {
            if *total_cpu >= cpu && *total_mem >= mem {
                let container = Container {
                    id: format!("{app_id}_container_{}", self.allocated.len()),
                    cpu_cores: cpu,
                    memory_mb: mem,
                    node: name.clone(),
                };
                self.allocated.push(container.clone());
                return Some(container);
            }
        }
        None
    }
}

// =============================================================================
// Demo
// =============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║          Mini Hadoop — HDFS + MapReduce          ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    // === HDFS Demo ===
    println!("━━━ 1. HDFS — Distributed File System ━━━\n");

    let mut hdfs = Hdfs::new(5);

    // Write a file (400 bytes → split into blocks)
    let data = vec![42u8; 400];
    hdfs.write_file("sales_2024.csv", &data);

    println!("Wrote 'sales_2024.csv' ({} bytes)", data.len());
    println!("Block size: {BLOCK_SIZE} bytes, Replication: {REPLICATION_FACTOR}x\n");

    let locations = hdfs.namenode.get_block_locations("sales_2024.csv");
    for (block_id, nodes) in &locations {
        println!("  {block_id}");
        println!("    replicas on: {:?}", nodes);
    }

    // Read it back
    let read_data = hdfs.read_file("sales_2024.csv");
    println!("\nRead back {} bytes, matches: {}", read_data.len(), read_data == data);

    // DataNode usage
    println!("\nDataNode usage:");
    for dn in &hdfs.datanodes {
        println!("  {}: {} bytes, {} blocks", dn.name, dn.used_space, dn.blocks.len());
    }

    // === MapReduce Demo: Word Count ===
    println!("\n━━━ 2. MapReduce — Word Count ━━━\n");

    let job = MapReduceJob::new(
        vec![
            "hello world hello".to_string(),
            "hadoop mapreduce hello".to_string(),
            "world data hadoop".to_string(),
        ],
        word_count_map,
        sum_reduce,
    );

    let results = job.run();
    println!("\n  Final word counts:");
    let mut sorted: Vec<_> = results.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (word, count) in &sorted {
        println!("    {word}: {count}");
    }

    // === MapReduce Demo: Max Temperature ===
    println!("\n━━━ 3. MapReduce — Max Temperature per City ━━━\n");

    fn temp_map(line: &str) -> Vec<(String, i64)> {
        // Format: "city,temp"
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() == 2 {
            let city = parts[0].to_string();
            let temp: i64 = parts[1].parse().unwrap_or(0);
            vec![(city, temp)]
        } else {
            vec![]
        }
    }

    let temp_job = MapReduceJob::new(
        vec![
            "NYC,85".to_string(),
            "SF,68".to_string(),
            "NYC,90".to_string(),
            "SF,72".to_string(),
            "NYC,78".to_string(),
            "LA,95".to_string(),
        ],
        temp_map,
        max_reduce,
    );

    let temp_results = temp_job.run();
    println!("\n  Max temperatures:");
    for (city, temp) in &temp_results {
        println!("    {city}: {temp}°F");
    }

    // === YARN Demo ===
    println!("\n━━━ 4. YARN — Resource Manager ━━━\n");

    let mut yarn = YarnResourceManager::new(vec![
        ("node-1", 16, 65536),
        ("node-2", 16, 65536),
        ("node-3", 32, 131072),
    ]);

    for i in 0..4 {
        let app = format!("wordcount_job_{i}");
        match yarn.request_container(&app, 4, 8192) {
            Some(c) => println!("  Allocated: {} → {} ({}cpu, {}MB)", c.id, c.node, c.cpu_cores, c.memory_mb),
            None => println!("  FAILED: No resources for {app}"),
        }
    }

    // === Summary ===
    println!("\n━━━ Hadoop Ecosystem Summary ━━━\n");
    println!("  ┌──────────────┬───────────────────────────────────────────┐");
    println!("  │ Component    │ Purpose                                   │");
    println!("  ├──────────────┼───────────────────────────────────────────┤");
    println!("  │ HDFS         │ Distributed storage (blocks, replication) │");
    println!("  │ MapReduce    │ Batch compute (map → shuffle → reduce)    │");
    println!("  │ YARN         │ Resource management & scheduling          │");
    println!("  │ Hive         │ SQL-on-Hadoop (translates SQL → MR jobs)  │");
    println!("  │ HBase        │ Column-family NoSQL on HDFS               │");
    println!("  │ ZooKeeper    │ Coordination (leader election, locks)     │");
    println!("  └──────────────┴───────────────────────────────────────────┘");
}
