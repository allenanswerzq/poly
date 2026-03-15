# Key-Value Store Design

## Problem Statement

Design a distributed key-value store like DynamoDB or Redis that:
- Handles millions of key-value pairs
- Provides fast reads and writes
- Supports persistence
- Is horizontally scalable

## Key Concepts

### Write Path (LSM Tree)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          LSM Tree Write Path                                 │
│                                                                              │
│   Write Request                                                              │
│       │                                                                      │
│       ▼                                                                      │
│   ┌──────────────┐                                                          │
│   │  Write-Ahead │  1. Append to WAL for durability                         │
│   │     Log      │                                                          │
│   └──────┬───────┘                                                          │
│          │                                                                   │
│          ▼                                                                   │
│   ┌──────────────┐                                                          │
│   │   MemTable   │  2. Write to in-memory sorted tree (Red-Black/Skip List) │
│   │  (in memory) │                                                          │
│   └──────┬───────┘                                                          │
│          │ When full                                                         │
│          ▼                                                                   │
│   ┌──────────────┐                                                          │
│   │   SSTable    │  3. Flush to immutable sorted file on disk               │
│   │   Level 0    │                                                          │
│   └──────┬───────┘                                                          │
│          │ Compaction                                                        │
│          ▼                                                                   │
│   ┌──────────────┐                                                          │
│   │   SSTable    │  4. Merge into larger sorted files                       │
│   │  Level 1-N   │                                                          │
│   └──────────────┘                                                          │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Read Path

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Read Path                                           │
│                                                                              │
│   Read Request (key)                                                         │
│       │                                                                      │
│       ▼                                                                      │
│   ┌──────────────┐                                                          │
│   │   MemTable   │  1. Check in-memory table first (O(log N))               │
│   └──────┬───────┘                                                          │
│          │ Not found                                                         │
│          ▼                                                                   │
│   ┌──────────────┐                                                          │
│   │ Bloom Filter │  2. Check if key MIGHT exist in SSTable                  │
│   └──────┬───────┘                                                          │
│          │ Might exist                                                       │
│          ▼                                                                   │
│   ┌──────────────┐                                                          │
│   │   SSTable    │  3. Binary search in sorted file                         │
│   │    Index     │                                                          │
│   └──────┬───────┘                                                          │
│          │ Found                                                             │
│          ▼                                                                   │
│   ┌──────────────┐                                                          │
│   │   Data       │  4. Read actual record                                    │
│   │   Block      │                                                          │
│   └──────────────┘                                                          │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Bloom Filter

Fast probabilistic data structure to check if key might exist:
- False positives possible (says yes, but actually no)
- False negatives impossible (if says no, definitely no)
- Saves disk reads for non-existent keys

```rust
// Bloom filter check
if !bloom_filter.might_contain(key) {
    return None;  // Definitely not in SSTable
}
// Might be in SSTable, need to check
```

## Write-Ahead Log (WAL)

Guarantees durability even if process crashes:

```
┌─────────────────────────────────────────────────────────────────┐
│                      WAL Structure                               │
│                                                                  │
│   ┌────────┬────────┬────────┬────────┬────────┐               │
│   │ Entry1 │ Entry2 │ Entry3 │ Entry4 │ Entry5 │ ...           │
│   └────────┴────────┴────────┴────────┴────────┘               │
│                                                                  │
│   Each entry: [Length][CRC][Timestamp][Type][Key][Value]        │
│                                                                  │
│   Recovery: Replay WAL on startup to rebuild MemTable           │
└─────────────────────────────────────────────────────────────────┘
```

## SSTable Format

```
┌─────────────────────────────────────────────────────────────────┐
│                      SSTable File                                │
│                                                                  │
│   ┌────────────────────────────────────────────────────────┐    │
│   │                     Data Blocks                         │    │
│   │   ┌─────────────────────────────────────────────────┐  │    │
│   │   │ key1:value1 | key2:value2 | key3:value3 | ...   │  │    │
│   │   └─────────────────────────────────────────────────┘  │    │
│   │   ┌─────────────────────────────────────────────────┐  │    │
│   │   │ key100:value100 | key101:value101 | ...         │  │    │
│   │   └─────────────────────────────────────────────────┘  │    │
│   └────────────────────────────────────────────────────────┘    │
│                                                                  │
│   ┌────────────────────────────────────────────────────────┐    │
│   │                     Index Block                         │    │
│   │   Block 0: starts at key1, offset 0                     │    │
│   │   Block 1: starts at key100, offset 4096                │    │
│   └────────────────────────────────────────────────────────┘    │
│                                                                  │
│   ┌────────────────────────────────────────────────────────┐    │
│   │                    Bloom Filter                         │    │
│   └────────────────────────────────────────────────────────┘    │
│                                                                  │
│   ┌────────────────────────────────────────────────────────┐    │
│   │                      Footer                             │    │
│   │   Index offset | Bloom filter offset | Magic number    │    │
│   └────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

## Compaction Strategies

### Size-Tiered Compaction
- Merge SSTables of similar size
- Good for write-heavy workloads
- Higher space amplification

### Leveled Compaction
- Each level is 10x larger than previous
- Better read performance
- Higher write amplification

## Implementation

Our implementation includes:
1. In-memory MemTable with sorted map
2. Simple WAL for durability
3. SSTable read/write
4. Basic compaction

Run the demo:
```bash
cargo run --bin key-value-store
```
