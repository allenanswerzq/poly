# MongoDB Deep Dive

## Overview

MongoDB is the **most popular document database**. Instead of rows and tables, it stores flexible JSON-like documents in collections. Choose it when your data schema varies per record or changes frequently.

## History & Why It Exists

```
The problem (2007):
  Web applications were exploding. Developers kept running into friction
  with relational databases:
    - Schema changes required painful ALTER TABLE migrations
    - Every object needed flattening into rows and columns
    - Horizontal scaling was an afterthought (sharding MySQL = pain)
    - Agile development wanted flexible, evolving schemas

  The NoSQL movement emerged: "Not Only SQL" — different data models
  for different problems. Document databases fit the web app model:
  store objects (JSON documents) directly, no ORM translation layer.

  Dwight Merriman and Eliot Horowitz (founders of DoubleClick, the
  ad server) knew the pain of scaling relational databases. They
  built MongoDB: store BSON (binary JSON) documents, flexible schema,
  built-in horizontal scaling.

Timeline:
  2007  Development begins at 10gen (now MongoDB Inc.)
  2009  Open-sourced (v1.0)
  2012  Replica sets and sharding mature
  2014  WiredTiger storage engine (replaced MMAPv1 — huge perf boost)
  2017  MongoDB 3.6 (change streams — real-time CDC)
  2018  MongoDB Atlas (managed cloud service) grows rapidly
  2018  Multi-document ACID transactions added (v4.0)
  2020  MongoDB 5.0 (time-series collections, live resharding)
  2023  MongoDB 7.0 (queryable encryption, metadata improvements)

Key design philosophy:
  - Document model: store what your application uses natively (JSON)
  - Flexible schema: different documents in same collection can have
    different fields. Schema-on-read, not schema-on-write.
  - Horizontal scaling built-in: automatic sharding by shard key
  - Developer experience: easy to get started, feels natural

The controversy:
  Early MongoDB (pre-2018) had no multi-document transactions,
  defaulted to unsafe write settings, and lost data in some failures.
  This created the "MongoDB loses data" reputation.
  Modern MongoDB (4.0+) has ACID transactions, durable writes by default,
  and is battle-tested at scale. The reputation is outdated.

Who uses it:
  EA (gaming), Toyota, Forbes, Cisco, Coinbase, thousands of startups.
```

## When to Choose MongoDB

| Use Case | Why MongoDB |
|----------|-----------|
| Product catalogs | Different products have different attributes |
| Content management | Articles, pages with varying structure |
| User profiles | Varying fields per user type |
| Prototyping | Schema can evolve without migrations |
| Embedded 1:N data | Reviews inside products, comments inside posts |

## Data Model

```
SQL table (rigid):                 MongoDB document (flexible):
┌────┬───────┬───────────┐        {
│ id │ name  │ email     │          "_id": ObjectId("..."),
├────┼───────┼───────────┤          "name": "Alice",
│  1 │ Alice │ a@b.com   │          "email": "a@b.com",
│  2 │ Bob   │ b@b.com   │          "addresses": [           ← embedded array
└────┴───────┴───────────┘            {"city": "NYC", "zip": "10001"},
                                      {"city": "LA", "zip": "90001"}
Every row must have                 ],
the same columns.                   "preferences": {          ← nested object
                                      "theme": "dark",
                                      "notifications": true
                                    }
                                  }

                                  Different docs can have
                                  completely different fields.
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                  MongoDB Architecture                            │
│                                                                   │
│  Single Node (Replica Set Member):                                │
│                                                                   │
│  Client (driver)                                                  │
│       │                                                           │
│       ▼                                                           │
│  ┌──────────────┐                                                │
│  │ Query Engine  │  Parse → plan → optimize → execute             │
│  │              │  Uses plan cache (skip re-planning hot queries) │
│  └──────┬───────┘                                                │
│         ▼                                                        │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │         WiredTiger Storage Engine (default since 3.2)     │    │
│  │                                                           │    │
│  │  ┌────────────────┐  Documents stored as BSON (binary JSON) │    │
│  │  │  Cache (RAM)    │  Hot data pages kept in memory         │    │
│  │  │  ~50% of RAM   │  LRU eviction for cold pages            │    │
│  │  └────────┬───────┘                                         │    │
│  │           │                                                │    │
│  │           ▼                                                │    │
│  │  ┌────────────────┐  ┌────────────────┐                 │    │
│  │  │ B-tree indexes  │  │ Data files      │  on disk        │    │
│  │  │ (_id, custom)  │  │ (BSON docs)     │                 │    │
│  │  └────────────────┘  └────────────────┘                 │    │
│  │                                                           │    │
│  │  ┌────────────────┐                                         │    │
│  │  │ Journal (WAL)   │  Write-ahead log for crash recovery     │    │
│  │  │ (every 50ms)   │  Replayed on restart if unclean shutdown│    │
│  │  └────────────────┘                                         │    │
│  └──────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘

REPLICA SET (high availability):
  ┌───────────────────────────────────────────────────────┐
  │                                                        │
  │  ┌──────────┐    ┌───────────┐    ┌───────────┐   │
  │  │ Primary   │ ─► │ Secondary │ ─► │ Secondary │   │
  │  │ (writes)  │    │ (reads OK)│    │ (reads OK)│   │
  │  └──────────┘    └───────────┘    └───────────┘   │
  │       │              │                                │
  │       └─── oplog ────┘                                │
  │    (operation log — replication stream)                 │
  │                                                        │
  │  Primary fails → election → secondary promoted          │
  │  Automatic failover in ~10-30 seconds.                  │
  └───────────────────────────────────────────────────────┘

SHARDED CLUSTER (horizontal scaling):
  ┌───────────────────────────────────────────────────────┐
  │  Client (driver)                                       │
  │       │                                                │
  │       ▼                                                │
  │  ┌──────────────┐                                       │
  │  │    mongos     │  Router — directs queries to shards  │
  │  │  (stateless)  │  Uses config servers for shard map   │
  │  └──────┬───────┘                                       │
  │         │                                               │
  │    ┌────┼───────┬─────────┐                            │
  │    ▼         ▼         ▼                            │
  │  Shard 1   Shard 2   Shard 3   (each is a replica set)│
  │  [A-H]     [I-P]     [Q-Z]     (by shard key range)   │
  │                                                        │
  │  Config Servers (3-node replica set)                    │
  │  Stores: which shard has which key ranges               │
  └───────────────────────────────────────────────────────┘

WRITE PATH:
  Client → mongos (router) → correct shard (by shard key)
    → Primary of that shard's replica set
      → Journal (WAL, fsync every 50ms)
      → Apply to WiredTiger B-tree
      → Replicate via oplog to secondaries
      → Ack to client (w:majority waits for 2/3 to confirm)

READ PATH:
  Client → mongos → route to correct shard
  If query includes shard key: targeted query (one shard).
  If not: scatter-gather (query ALL shards, merge results). Slow!
  Always include shard key in queries for best performance.
```

## Key Concepts for Interviews

### 1. Embedding vs Referencing
```
Embedding (denormalized):          Referencing (normalized):
┌──────────────────────┐          ┌──────────────┐  ┌──────────────┐
│ order: {             │          │ order: {     │  │ user: {      │
│   user_name: "Alice" │          │   user_id: 1 │──│   name: Alice│
│   user_email: "..."  │          │   items: [...]│  │   email: ... │
│   items: [...]       │          │ }            │  │ }            │
│ }                    │          └──────────────┘  └──────────────┘
└──────────────────────┘
Fast reads (1 query)              Consistent data (no duplication)
Duplicated data                    Requires $lookup (slow JOIN)
```

**Rule of thumb**: Embed data that's read together. Reference data that's updated independently.

### 2. Sharding (Horizontal Scaling)
```
Shard key: user_id

Shard 1 (user_id 1-1000)     Shard 2 (user_id 1001-2000)
┌─────────────────────┐     ┌─────────────────────────┐
│ Alice's docs        │     │ Charlie's docs          │
│ Bob's docs          │     │ Dave's docs             │
└─────────────────────┘     └─────────────────────────┘

Config servers (metadata)    Mongos routers (query routing)
```

### 3. Indexes
- B-tree indexes (default, same as SQL)
- Compound indexes: `{user_id: 1, created_at: -1}`
- Text indexes for full-text search
- Geospatial indexes (2dsphere)
- **If you don't index, every query is a collection scan** (same as SQL)

### 4. Aggregation Pipeline
```javascript
// SQL: SELECT status, COUNT(*) FROM orders GROUP BY status
db.orders.aggregate([
  { $group: { _id: "$status", count: { $sum: 1 } } },
  { $sort: { count: -1 } }
])
```

### 5. Replica Sets
```
Primary ──► Secondary 1 (auto-failover)
        ──► Secondary 2
        ──► Arbiter (votes only, no data)

- Automatic failover (election in ~10 seconds)
- Read from secondaries for read scaling
- Write concern: "majority" = wait for 2/3 nodes
```

## MongoDB vs PostgreSQL JSONB

| Aspect | MongoDB | PostgreSQL JSONB |
|--------|---------|-----------------|
| Schema flexibility | Native, first-class | Bolt-on (JSONB column) |
| JOINs | Weak ($lookup) | Native, optimized |
| Transactions | Multi-doc since v4.0 | ACID from day one |
| Scaling | Native sharding | Extensions (Citus) |
| Tooling | Atlas, Compass | pgAdmin, psql |
| Best for | Document-centric apps | Mixed relational + JSON |

## Limitations to Mention

- No true JOINs (only $lookup, expensive)
- Multi-document transactions have overhead (added in v4.0, not free)
- Denormalization means update anomalies
- Large documents (>16MB limit) need GridFS
- Schema validation is optional (discipline required)

## Interview Sound Bite

> "I'd use MongoDB for the product catalog because each product type has different attributes — a phone has 'screen_size' and 'battery', while a shirt has 'size' and 'color'. With MongoDB we embed these varying attributes directly without schema migrations. For the order system that needs ACID transactions, I'd pair it with PostgreSQL."
