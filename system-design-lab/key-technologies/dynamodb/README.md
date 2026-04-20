# DynamoDB Deep Dive

## Overview

DynamoDB is AWS's **fully managed key-value and document database**. Zero operational overhead — no servers, no patching, no capacity planning (with on-demand mode). Choose it when you want a scalable database without managing infrastructure.

## History & Why It Exists

```
The problem (2004-2007):
  Amazon.com had a critical incident: during a peak shopping event,
  their relational database couldn't scale. Cart data was lost.

  Amazon engineers asked: what if we build a database that:
    - NEVER goes down (availability above all else)
    - Scales horizontally without limits
    - Accepts simple key-value operations (no complex SQL needed)
    - Sacrifices consistency when necessary (AP in CAP)

  Result: Amazon Dynamo paper (2007) — one of the most influential
  distributed systems papers ever written.

  The Dynamo paper introduced:
    - Consistent hashing (partition data across nodes)
    - Vector clocks (detect conflicts without coordination)
    - Sloppy quorum + hinted handoff (always writable)
    - Gossip protocol (decentralized failure detection)
    - Tunable consistency (choose per-request)

  Dynamo (internal, 2007) → DynamoDB (AWS service, 2012)
  DynamoDB kept the scalability ideas but simplified the model:
    No vector clocks, no client-side conflict resolution.
    Fully managed — you never see a server.

Timeline:
  2007  Amazon publishes Dynamo paper
  2007  Dynamo used internally at Amazon
  2012  DynamoDB launched as AWS service (Werner Vogels announces at re:Invent)
  2013  Local secondary indexes added
  2014  Global secondary indexes added
  2018  On-demand mode (no capacity planning needed)
  2019  Global Tables (multi-region active-active replication)
  2023  Zero-ETL integration with Redshift

The Dynamo paper's influence:
  Cassandra = Dynamo's distribution + BigTable's storage
  Riak = faithful Dynamo implementation (vector clocks and all)
  Voldemort = LinkedIn's Dynamo-inspired key-value store
  DynamoDB = Amazon's managed evolution of the original idea
```

## When to Choose DynamoDB

| Use Case | Why DynamoDB |
|----------|-----------|
| Session storage | Single-digit ms latency, auto-TTL |
| Gaming leaderboards | Fast writes, GSI for rank queries |
| Shopping carts | Key-value per user, flexible item schema |
| IoT data | Auto-scaling writes, TTL for expiry |
| Serverless apps | Pairs with Lambda, no connection management |

## Data Model

```
┌─────────────────────────────────────────────────────────────┐
│ Table: Orders                                                │
│                                                              │
│ Partition Key (PK): user_id                                  │
│ Sort Key (SK): order_id                                      │
│                                                              │
│ PK=alice │ SK=order-001 │ total=29.99 │ status=shipped      │
│ PK=alice │ SK=order-002 │ total=9.99  │ status=delivered     │
│ PK=bob   │ SK=order-001 │ total=49.99 │ status=pending       │
│                                                              │
│ PK → which partition (node)                                  │
│ SK → sort order within a partition                           │
│                                                              │
│ Query: "Alice's orders" → PK=alice → returns both, sorted   │
│ Query: "All pending" → needs GSI (can't scan by status)      │
└─────────────────────────────────────────────────────────────┘
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│              DynamoDB Internal Architecture                       │
│              (based on Dynamo paper + AWS enhancements)           │
│                                                                   │
│  Client: PutItem(PK="alice", SK="order-001", total=29.99)       │
│       │                                                           │
│       ▼                                                           │
│  ┌──────────────┐                                                │
│  │ Request Router│  Stateless fleet of routers (auto-scaled)     │
│  │ (front door)  │  Authenticates, rate-limits, routes request    │
│  └──────┬───────┘                                                │
│         │  hash(PK) → partition → which storage nodes?           │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │ Partition Map │  Maps partition key hash → storage nodes      │
│  │ (metadata)    │  Updated when table splits/merges partitions  │
│  └──────┬───────┘                                                │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │           Storage Nodes (per partition)                    │    │
│  │                                                           │    │
│  │  Partition = unit of storage + throughput                  │    │
│  │                                                           │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐              │    │
│  │  │ Leader    │  │ Replica   │  │ Replica   │              │    │
│  │  │ (writes)  │──│ (reads OK)│──│ (reads OK)│              │    │
│  │  │           │  │           │  │           │              │    │
│  │  │ B-tree    │  │ B-tree    │  │ B-tree    │              │    │
│  │  │ + WAL     │  │ + WAL     │  │ + WAL     │              │    │
│  │  └──────────┘  └──────────┘  └──────────┘              │    │
│  │                                                           │    │
│  │  3 replicas across 3 AZs in one region.                   │    │
│  │  Leader handles writes. Any replica handles eventually-   │    │
│  │  consistent reads. Leader handles strongly-consistent reads│   │
│  │                                                           │    │
│  │  Paxos-based replication:                                 │    │
│  │    Write acknowledged when 2 of 3 replicas confirm.       │    │
│  └──────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘

WRITE PATH:
  Client
    → Request Router (auth + route)
      → Partition Leader (based on hash of PK)
        → Write to WAL (write-ahead log)
        → Write to B-tree (in-memory + disk)
        → Replicate to 2 followers (Paxos)
        → Ack to client when 2/3 confirm
  Latency: single-digit milliseconds (p99 < 10ms)

READ PATH:
  Eventually consistent (default, cheaper):
    Client → Request Router → ANY of 3 replicas → return
    May read slightly stale data (ms-level staleness).

  Strongly consistent:
    Client → Request Router → Leader replica → return
    Always latest data. Costs 2x the RCU.

AUTO-SCALING INTERNALS:
  ┌────────────────────────────────────────────────────────┐
  │  Each partition supports:                               │
  │    3000 RCU + 1000 WCU + 10GB storage                  │
  │                                                         │
  │  When partition gets hot → DynamoDB SPLITS it:          │
  │    Partition [A-Z] → Partition [A-M] + [N-Z]           │
  │    Each half gets its own throughput budget.             │
  │    Fully automatic, no downtime, invisible to you.      │
  │                                                         │
  │  Adaptive capacity: borrows unused throughput from       │
  │  cold partitions and gives it to hot ones.              │
  └────────────────────────────────────────────────────────┘

GLOBAL SECONDARY INDEX (GSI) — how it works internally:
  GSI = a separate hidden table with different PK/SK.
  When you write to the main table, DynamoDB ASYNCHRONOUSLY
  writes to each GSI. GSI reads are always eventually consistent.

  Main table write → DynamoDB Streams → GSI update
  (this is why GSI writes consume WCU from the GSI, not the table)
```

## Key Concepts for Interviews

### 1. Single-Table Design
```
Instead of multiple tables, store everything in ONE table with
different item types distinguished by PK/SK patterns.

PK              SK              Attributes
────────────────────────────────────────────────
USER#alice      PROFILE         name, email, ...
USER#alice      ORDER#001       total, status, ...
USER#alice      ORDER#002       total, status, ...
ORDER#001       ITEM#widget     qty, price, ...
ORDER#001       ITEM#gadget     qty, price, ...

One table. Query by prefix: SK begins_with("ORDER#") → all orders.
This is the DynamoDB way — denormalize and overload keys.
```

### 2. Global Secondary Index (GSI)
```
Main table: PK=user_id, SK=order_id
  → Can query: "all orders for user X"
  → Can NOT query: "all orders with status=pending"

GSI: PK=status, SK=created_at
  → Now can query: "all pending orders, newest first"

GSI = a full copy of the table with different PK/SK.
Costs extra storage + write capacity.
```

### 3. Capacity Modes
```
On-Demand:   Pay per request. Auto-scales. No planning.
             Good for: unpredictable traffic, new apps.

Provisioned: Reserve read/write capacity units (RCU/WCU).
             Cheaper for predictable workloads.
             Auto-scaling: adjusts within min/max bounds.

1 RCU = 1 strongly consistent read/sec (up to 4KB)
1 WCU = 1 write/sec (up to 1KB)
```

### 4. Streams + Event-Driven
```
DynamoDB Streams: CDC (change data capture) for every write

Table write ──► DynamoDB Stream ──► Lambda function
                                      │
                                      ├──► Update search index
                                      ├──► Send notification
                                      └──► Replicate to analytics DB
```

### 5. Transactions
```
TransactWriteItems: up to 100 items, all-or-nothing
TransactGetItems: consistent read across items

Use for: transfer money between accounts, place order + decrement stock
Cost: 2x the normal write cost
```

## DynamoDB vs Cassandra

| Aspect | DynamoDB | Cassandra |
|--------|---------|-----------|
| Operations | Zero (AWS managed) | Complex (you manage) |
| Cost model | Per request or provisioned | Per node (EC2 instances) |
| Multi-region | Global Tables (push-button) | Multi-DC (manual setup) |
| Query flexibility | PK/SK + GSI only | CQL (SQL-like, still limited) |
| Consistency | Strong or eventual | Tunable (QUORUM) |
| Vendor lock-in | AWS only | Open source, any cloud |
| Performance | Predictable (SLA-backed) | Depends on tuning |

## Limitations to Mention

- Item size limit: 400KB
- No JOINs, no aggregations
- GSI count limited (20 per table)
- Scan operations are expensive and slow
- Hot partition problem if key design is bad
- AWS vendor lock-in
- Transactions limited to 100 items, 2x cost

## Interview Sound Bite

> "DynamoDB is perfect here because we're on AWS and need zero operational overhead. We'd use single-table design with user_id as the partition key. For the leaderboard query, we'd add a GSI on score. On-demand mode handles traffic spikes without any capacity planning."
