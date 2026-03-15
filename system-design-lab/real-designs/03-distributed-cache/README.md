# Distributed Cache Design

## Problem Statement

Design a distributed cache that:
- Stores key-value pairs in memory
- Scales horizontally across machines
- Handles node failures gracefully
- Provides fast lookups (< 1ms)

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       Distributed Cache Architecture                        │
│                                                                              │
│  ┌──────────────┐                                                           │
│  │    Client    │                                                           │
│  └──────┬───────┘                                                           │
│         │                                                                    │
│         ▼                                                                    │
│  ┌──────────────┐    Consistent Hashing                                     │
│  │ Cache Client │    key → hash → node                                      │
│  │   Library    │                                                           │
│  └──────┬───────┘                                                           │
│         │                                                                    │
│    ┌────┴────────────────┬─────────────────────┐                           │
│    ▼                     ▼                     ▼                           │
│  ┌──────────┐        ┌──────────┐        ┌──────────┐                      │
│  │  Node 1  │◄──────►│  Node 2  │◄──────►│  Node 3  │                      │
│  │  Primary │        │  Primary │        │  Primary │                      │
│  └────┬─────┘        └────┬─────┘        └────┬─────┘                      │
│       │                   │                   │                             │
│       ▼                   ▼                   ▼                             │
│  ┌──────────┐        ┌──────────┐        ┌──────────┐                      │
│  │ Replica  │        │ Replica  │        │ Replica  │                      │
│  │  Node 2  │        │  Node 3  │        │  Node 1  │                      │
│  └──────────┘        └──────────┘        └──────────┘                      │
│                                                                              │
│  Replication Factor = 2 (data on 2 nodes)                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

### 1. Partitioning: Consistent Hashing
```
Key → hash(key) → position on ring → find next node clockwise

Benefits:
- Add/remove nodes: only K/N keys move
- Even distribution with virtual nodes
- Predictable key routing
```

### 2. Replication
```
Each key stored on N consecutive nodes (replication factor)
- Read: Any replica can serve
- Write: Primary + sync to replicas
```

### 3. Consistency Models

| Model | Description | Latency |
|-------|-------------|---------|
| Strong | All replicas consistent before ACK | High |
| Eventual | ACK after primary, async replicas | Low |
| Read-your-writes | Reader sees own writes | Medium |

### 4. Eviction Policies
```
LRU  - Least Recently Used (most common)
LFU  - Least Frequently Used
FIFO - First In First Out
TTL  - Time-based expiration
```

## Caching Patterns

### Cache-Aside (Lazy Loading)
```rust
fn get(key):
    value = cache.get(key)
    if value is None:
        value = database.get(key)
        cache.set(key, value)
    return value
```

### Write-Through
```rust
fn set(key, value):
    cache.set(key, value)
    database.set(key, value)  // Sync
```

### Write-Behind (Write-Back)
```rust
fn set(key, value):
    cache.set(key, value)
    queue.add(key, value)  // Async write to DB
```

## Implementation

Our implementation demonstrates:
1. Consistent hashing for key distribution
2. Replication across nodes
3. Fault tolerance (node removal)
4. TTL expiration
5. Cache-aside pattern

Run the demo:
```bash
cargo run --bin distributed-cache
```
