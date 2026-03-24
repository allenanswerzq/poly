# Cassandra Deep Dive

## Overview

Apache Cassandra is a **wide-column distributed database** designed for massive write throughput and linear horizontal scaling. It sacrifices strong consistency for availability (AP in CAP theorem). Choose it when you need to write millions of events per second across multiple data centers.

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
