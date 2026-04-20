# Cassandra Deep Dive

## Overview

Apache Cassandra is a **wide-column distributed database** designed for massive write throughput and linear horizontal scaling. It sacrifices strong consistency for availability (AP in CAP theorem). Choose it when you need to write millions of events per second across multiple data centers.

## History & Why It Exists

```
The problem (2007):
  Facebook needed an inbox search system. Requirements:
    - Handle 100M+ users writing messages simultaneously
    - Work across multiple data centers (no single point of failure)
    - Writes are MORE common than reads (opposite of most databases)

  Existing options were insufficient:
    MySQL: single-master writes, vertical scaling only
    Oracle/DB2: expensive, not horizontally scalable
    HBase: required HDFS + ZooKeeper, single-region only

  Avinash Lakshman (co-author of Amazon Dynamo) and Prashant Malik at
  Facebook designed Cassandra combining two seminal papers:
    Amazon Dynamo (2007): ring topology, consistent hashing, tunable consistency
    Google BigTable (2006): column-family data model, memtable + SSTable storage

  Result: Dynamo's distribution model + BigTable's storage engine.

Timeline:
  2007  Built at Facebook for inbox search
  2008  Open-sourced
  2010  Apache top-level project
  2011  DataStax founded (commercial Cassandra support)
  2015  Cassandra 3.0 (materialized views, better compaction)
  2021  Cassandra 4.0 (virtual tables, audit logging)

Key design philosophy:
  - Availability over consistency (AP in CAP theorem)
  - Masterless — every node is equal (no single point of failure)
  - Tunable consistency — choose per-query: ONE, QUORUM, ALL
  - Write-optimized — append-only log + memtable, no read-before-write
  - Linear scaling — add nodes, throughput grows linearly

Who uses it:
  Apple (400K+ nodes), Netflix, Discord, Instagram, Uber
  Apple's deployment is the largest known Cassandra cluster in the world.
```

## When to Choose Cassandra

| Use Case | Why Cassandra |
|----------|-------------|
| Time-series data | Write-optimized, partitioned by time |
| IoT sensor data | Millions of writes/sec |
| Chat messages | Partition by conversation, ordered by time |
| Activity feeds | Write per user, read latest N |
| Metrics/logging | Append-heavy, TTL for auto-expiry |

## Data Model

```
Not a relational database. Think of it as a distributed sorted map.

Partition key → determines which node stores the data
Clustering key → determines sort order within a partition

Example: Chat messages
┌─────────────────────────────────────────────────────────────┐
│ Partition Key: conversation_id                               │
│ Clustering Key: sent_at (DESC)                               │
│                                                              │
│ conversation_id=abc │ sent_at            │ sender │ text    │
│─────────────────────┼────────────────────┼────────┼─────────│
│ abc                 │ 2024-01-15 12:03   │ Alice  │ "Hey!"  │
│ abc                 │ 2024-01-15 12:02   │ Bob    │ "Hi"    │
│ abc                 │ 2024-01-15 12:01   │ Alice  │ "Hello" │
│                                                              │
│ All messages for conversation "abc" are stored together      │
│ on the same node, sorted by time. Reading last 50 = fast.   │
└─────────────────────────────────────────────────────────────┘
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                   Cassandra Node Internals                        │
│                                                                   │
│  Client write: INSERT INTO messages (conv_id, sent_at, text) ... │
│       │                                                           │
│       ▼                                                           │
│  ┌──────────────┐  ANY node can be coordinator (masterless!)     │
│  │ Coordinator   │  Routes request to replica nodes               │
│  │ Node          │  (based on partition key hash → token range)   │
│  └──────┬───────┘                                                │
│         │  forward to replica nodes (RF=3 → 3 nodes)             │
│         ▼                                                        │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │              On Each Replica Node                         │    │
│  │                                                           │    │
│  │  1. Commit Log (append-only, sequential write to disk)    │    │
│  │     └─► crash recovery: replay log after restart          │    │
│  │                                                           │    │
│  │  2. Memtable (in-memory sorted structure, per table)      │    │
│  │     └─► sorted by clustering key (e.g., sent_at)          │    │
│  │     └─► when full → flush to disk as SSTable              │    │
│  │                                                           │    │
│  │  3. SSTables (Sorted String Tables, immutable on disk)    │    │
│  │     ┌──────────┐ ┌──────────┐ ┌──────────┐              │    │
│  │     │SSTable 1  │ │SSTable 2  │ │SSTable 3  │              │    │
│  │     │(old data) │ │(newer)    │ │(newest)   │              │    │
│  │     └──────────┘ └──────────┘ └──────────┘              │    │
│  │     └─► immutable! never modified after written           │    │
│  │     └─► compaction merges multiple → fewer, larger SSTables│   │
│  │                                                           │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                   │
│  Write acknowledged when CL nodes confirm (QUORUM = 2 of 3)     │
└──────────────────────────────────────────────────────────────────┘

WRITE PATH (why writes are ~1ms):
  Client → Coordinator → Commit Log (sequential append) → Memtable (memory)
  No read-before-write. No locks. No random I/O. Just append + memwrite.
  That's why Cassandra writes are so fast.

READ PATH (more complex):
  Client → Coordinator → Replica Node(s)
  On each replica:
    1. Check Memtable (in-memory, instant)
    2. Check Bloom Filters for each SSTable
       └─► probabilistic: "definitely NOT here" or "maybe here"
       └─► avoids reading SSTables that can't contain the key
    3. Check Partition Index → find offset in SSTable
    4. Read data from SSTable on disk
    5. Merge results from Memtable + all matching SSTables
       └─► newest timestamp wins (last-write-wins conflict resolution)

  ┌─────────────────────────────────────────────────────────┐
  │              Read Path Detail                            │
  │                                                          │
  │  Query: SELECT * FROM messages WHERE conv_id = 'abc'     │
  │       │                                                  │
  │       ▼                                                  │
  │  Memtable ─────────────────────────────┐                │
  │       │                                │                │
  │       ▼                                │ merge by       │
  │  Bloom Filter (SSTable 3) → maybe? ──► │ timestamp      │
  │  Bloom Filter (SSTable 2) → no! skip   │ (newest wins)  │
  │  Bloom Filter (SSTable 1) → maybe? ──► │                │
  │                                        ▼                │
  │                                   Return result         │
  └─────────────────────────────────────────────────────────┘

COMPACTION (background maintenance):
  SSTables accumulate over time (each flush creates a new one).
  Compaction merges them: removes deleted data (tombstones),
  resolves duplicates, creates fewer/larger SSTables.

  Strategies:
    Size-Tiered (STCS):  merge SSTables of similar size. Good for writes.
    Leveled (LCS):       fixed-size SSTables in levels. Good for reads.
    Time-Window (TWCS):  group by time window. Best for time-series.
```

## Key Concepts for Interviews

### 1. Partition Key — THE Most Important Decision
```
Good partition key:                  Bad partition key:
  conversation_id                      date (all today's data → 1 node!)
  user_id                             country (USA gets 60% of traffic)
  sensor_id                           status ("active" → hot partition)

Rule: choose a key with high cardinality and even distribution.
```

### 2. Write Path (Why Writes Are Fast)
```
Client ──► Any Node (coordinator)
              │
              ├──► Commit Log (append-only, sequential write)
              ├──► Memtable (in-memory sorted structure)
              │
              └──► Eventually: flush to SSTable (sorted string table)
                   Compaction merges SSTables in background

No read-before-write. No locks. Just append.
Single partition write: ~1ms
```

### 3. Consistency Levels
```
Write: ONE / QUORUM / ALL
Read:  ONE / QUORUM / ALL

QUORUM = majority of replicas (2 out of 3)

CL=ONE:     Fastest, risk of stale reads
CL=QUORUM:  Balanced (most common choice)
CL=ALL:     Strongest, but one node down = query fails

Read QUORUM + Write QUORUM = strong consistency
(if RF=3: write to 2/3 + read from 2/3 = guaranteed overlap)
```

### 4. Denormalization Is Required
```
In Cassandra you model tables per query, not per entity.

Query: "Get user's recent orders"
  → Table: orders_by_user (partition: user_id, clustering: order_date DESC)

Query: "Get order by order_id"
  → Table: orders_by_id (partition: order_id)

Same data, two tables! This is normal and expected.
```

### 5. Ring Architecture
```
       Node A (token 0-25)
      /                    \
Node D (75-100)        Node B (25-50)
      \                    /
       Node C (50-75)

Partition key → hash → token → which node owns it
Replication factor 3: data stored on 3 consecutive nodes
Any node can be coordinator (no single point of failure)
```

## Cassandra vs Other Databases

| Aspect | Cassandra | PostgreSQL | DynamoDB |
|--------|-----------|-----------|----------|
| Write speed | Exceptional | Good | Good |
| Read patterns | Must match partition key | Flexible (any query) | Must match key |
| Consistency | Tunable (AP default) | Strong (CP) | Tunable |
| JOINs | None | Full | None |
| Scaling | Linear, multi-DC | Vertical + replicas | Auto-scaling |
| Operations | Complex (manage nodes) | Simpler | Zero (managed) |

## Limitations to Mention

- No JOINs, no subqueries, no aggregations (use Spark on top)
- Must design tables around queries (denormalize everything)
- Reads by non-partition key = full cluster scan
- Lightweight transactions (LWT) are slow — avoid if possible
- Tombstones from deletes can cause performance issues
- Operational complexity (compaction tuning, repair, etc.)

## Interview Sound Bite

> "For the messaging system, I'd use Cassandra because we need to handle millions of message writes per second across data centers. We'd partition by conversation_id with sent_at as the clustering key, so reading the last 50 messages is a single partition read. We'd use QUORUM consistency for writes and ONE for reads to balance durability and latency."
