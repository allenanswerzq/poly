# ClickHouse Deep Dive

## Overview

ClickHouse is a **columnar OLAP database** built for analytics. It can scan billions of rows per second on a single node. Choose it when you need fast aggregation queries (COUNT, SUM, AVG) over massive datasets — dashboards, metrics, ad analytics.

## History & Why It Exists

```
The problem (2009):
  Yandex (Russia's Google) needed to analyze petabytes of web analytics
  data (Yandex.Metrica — their Google Analytics competitor).

  Existing options:
    MySQL: row-oriented, too slow for aggregation over billions of rows
    Hadoop/Hive: minutes per query, not interactive
    Vertica/Teradata: expensive commercial columnar DBs
    Google BigQuery: didn't exist yet (launched 2012)

  Yandex engineers built ClickHouse as an internal columnar database
  designed from scratch for one thing: scan billions of rows per second
  on commodity hardware.

Timeline:
  2009  Development started at Yandex for Metrica
  2016  Open-sourced (already processing 13 trillion rows per day)
  2021  ClickHouse Inc. founded (raised $250M, spun out of Yandex)
  2023  ClickHouse Cloud (managed service) generally available
  2024  Fastest-growing OLAP database by adoption

Key design philosophy:
  - Columnar storage: only read columns you need (skip the rest)
  - Vectorized execution: process data in batches of columns (SIMD-friendly)
  - Compression: same-type data in columns compresses 10-100x
  - Single node speed FIRST: one ClickHouse node outperforms most
    distributed systems. Scale out only when needed.
  - No transactions: not ACID, append-mostly, eventual merge

What makes ClickHouse special vs other columnar DBs:
  - Written in C++ with extreme low-level optimization
  - Vectorized execution (not Volcano/row-at-a-time model)
  - 100+ functions optimized per CPU architecture (SSE/AVX)
  - MergeTree engine family handles indexing, partitioning, TTL
  - Can ingest millions of rows/sec while serving queries simultaneously

Who uses it:
  Cloudflare (DNS analytics), Uber, eBay, Spotify, Gitlab, Bloomberg
```

## When to Choose ClickHouse

| Use Case | Why ClickHouse |
|----------|--------------|
| Analytics dashboards | Aggregate billions of rows in seconds |
| Ad click tracking | Count clicks by campaign/region/time |
| Log analysis | Fast search + aggregation on log data |
| Metrics monitoring | Time-series aggregation |
| A/B test analysis | Fast GROUP BY over experiment data |

## Why Columnar Storage Matters

```
Row-oriented (PostgreSQL):         Column-oriented (ClickHouse):
Store all columns of a row         Store each column separately
together on disk.                  on disk.

┌──────┬──────┬────────┐          ┌──────┬──────┬──────┬──────┐
│ id=1 │ NYC  │  29.99 │          │ id=1 │ id=2 │ id=3 │ id=4 │  ← id column
│ id=2 │ LA   │   9.99 │          └──────┴──────┴──────┴──────┘
│ id=3 │ NYC  │  49.99 │          ┌──────┬──────┬──────┬──────┐
│ id=4 │ SF   │  19.99 │          │ NYC  │  LA  │ NYC  │  SF  │  ← city column
└──────┴──────┴────────┘          └──────┴──────┴──────┴──────┘
                                  ┌──────┬──────┬──────┬──────┐
Query: SELECT AVG(price)          │29.99 │ 9.99 │49.99 │19.99 │  ← price column
  Row store: read ALL columns     └──────┴──────┴──────┴──────┘
  of ALL rows (wasted I/O)
                                  Query: SELECT AVG(price)
                                    Only read the price column!
                                    Skip id and city entirely.
                                    + Same-type data compresses 10-100x
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                  ClickHouse Server Architecture                   │
│                                                                   │
│  SQL Query: SELECT city, COUNT(*) FROM events                    │
│             WHERE date = '2024-01-15' GROUP BY city              │
│       │                                                           │
│       ▼                                                           │
│  ┌──────────────┐                                                │
│  │    Parser     │  SQL → AST                                    │
│  └──────┬───────┘                                                │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │   Analyzer    │  Resolve tables, columns, types                │
│  └──────┬───────┘                                                │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │  Query Planner│  Logical plan → physical plan                 │
│  │  + Optimizer  │  Partition pruning, column pruning,           │
│  │              │  predicate pushdown, projection pushdown       │
│  └──────┬───────┘                                                │
│         ▼                                                        │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │         Vectorized Execution Engine                       │    │
│  │                                                           │    │
│  │  Process data in COLUMNS, not rows.                       │    │
│  │  Each step processes a BLOCK of N values (e.g., 65536)    │    │
│  │  at once using SIMD instructions.                         │    │
│  │                                                           │    │
│  │  Pipeline:                                                │    │
│  │    ReadFromMergeTree → Filter → Aggregate → Sort → Output│    │
│  │    (each step processes 65K values at a time)             │    │
│  │                                                           │    │
│  │  vs row-at-a-time (Volcano model, PostgreSQL):            │    │
│  │    Process 1 row → pass up → process 1 row → pass up     │    │
│  │    Branch prediction misses, function call overhead.       │    │
│  │                                                           │    │
│  │  Vectorized: process 65K values → pass column up          │    │
│  │    CPU cache-friendly, SIMD-optimized, minimal overhead.  │    │
│  └──────────────────────────────────────────────────────────┘    │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │              MergeTree Storage Engine                      │    │
│  │                                                           │    │
│  │  Each table is split into PARTITIONS (e.g., by month)     │    │
│  │  Each partition contains PARTS (sorted data chunks)        │    │
│  │                                                           │    │
│  │  Table: events                                            │    │
│  │  ├── 202401/                  (January partition)          │    │
│  │  │   ├── part_1/                                          │    │
│  │  │   │   ├── city.bin         (city column, compressed)   │    │
│  │  │   │   ├── date.bin         (date column, compressed)   │    │
│  │  │   │   ├── value.bin        (value column, compressed)  │    │
│  │  │   │   ├── primary.idx      (sparse index)              │    │
│  │  │   │   └── count.txt        (row count)                 │    │
│  │  │   └── part_2/                                          │    │
│  │  └── 202402/                  (February partition)         │    │
│  │      └── part_1/                                          │    │
│  │                                                           │    │
│  │  Sparse index: NOT every row indexed.                     │    │
│  │  Index stores value every 8192 rows (granule).            │    │
│  │  Query: WHERE date = '2024-01-15'                         │    │
│  │    → skip February partition entirely (partition pruning)  │    │
│  │    → within January, check sparse index granules           │    │
│  │    → read only matching granules, skip the rest            │    │
│  └──────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘

Query: SELECT city FROM events WHERE date = '2024-01-15' AND user_id > 5000

Step 1: primary.idx (sparse index of KEY VALUES)
  Binary search: find granules where (date, user_id) could match
  → granules 10-15 might contain date='2024-01-15' AND user_id > 5000

Step 2: date.mrk2 (marks for date column)
  Look up granules 10-15:
    granule 10 → compressed block at byte offset 120400, decompressed offset 0
    granule 11 → compressed block at byte offset 120400, decompressed offset 32768
    ...

Step 3: Read + decompress those blocks from date.bin
  Scan decompressed date values → which ROWS match date='2024-01-15'?
  → rows 82000-82500 match

Step 4: city.mrk2 (marks for city column)
  Look up same granules for city column (different offsets!)
    granule 10 → compressed block at byte offset 45200, decompressed offset 0

Step 5: Read + decompress city.bin at those offsets
  Return city values for matching rows.

  ┌──────────────────────────────────────────────────────────────┐
  │  primary.idx: "which granules?"     (key values, one file)   │
  │  .mrk2 files: "where on disk?"     (per-column offsets)     │
  │  .bin files:  "the actual data"    (per-column, compressed) │
  │                                                               │
  │  primary.idx decides the granules.                           │
  │  .mrk2 translates granule → physical position per column.   │
  │  Each column has independent offsets because compression     │
  │  produces different block sizes per column.                  │
  └──────────────────────────────────────────────────────────────┘

WRITE PATH:
  INSERT → data sorted by ORDER BY key → written as new PART
  → background merge combines parts (like LSM-tree compaction)

  INSERT is BATCHED. Don't insert row-by-row!
  Recommended: batches of 10K-100K rows per INSERT.

  Each part is immutable once written. Merge creates
  new parts, deletes old ones. No in-place update.

DISTRIBUTED QUERY (multi-node cluster):
  ┌──────────────────────────────────────────────────┐
  │                                                    │
  │  Client → any node (coordinator)                  │
  │            │                                       │
  │            ├──► Shard 1: scan + partial aggregate  │
  │            ├──► Shard 2: scan + partial aggregate  │
  │            └──► Shard 3: scan + partial aggregate  │
  │                      │                             │
  │                      ▼                             │
  │            Coordinator merges partial results      │
  │            Returns final result to client          │
  │                                                    │
  │  Sharding by column (e.g., hash of user_id)       │
  │  Replication between shard replicas for HA         │
  └──────────────────────────────────────────────────┘
```

## Key Concepts for Interviews

### 1. MergeTree Engine (The Heart of ClickHouse)
```sql
CREATE TABLE events (
    event_date Date,
    user_id UInt64,
    event_type String,
    value Float64
) ENGINE = MergeTree()
ORDER BY (event_date, user_id)  -- sort key (like clustered index)
PARTITION BY toYYYYMM(event_date);  -- monthly partitions
```
- Data sorted by ORDER BY key → range queries are fast
- Partitions → only scan relevant months
- Background merges consolidate small files

### 2. Approximate Queries (Trade Accuracy for Speed)
```sql
-- Exact count distinct: slow on 1B rows
SELECT COUNT(DISTINCT user_id) FROM events;  -- 30 seconds

-- Approximate: uniqHLL12 (HyperLogLog, <1% error)
SELECT uniqHLL12(user_id) FROM events;       -- 0.5 seconds

-- Sample: scan only 10% of data
SELECT AVG(value) FROM events SAMPLE 0.1;    -- 10x faster
```

### 3. Materialized Views (Pre-Aggregate on Insert)
```sql
-- Raw events: 1 billion rows
CREATE TABLE raw_events (...) ENGINE = MergeTree();

-- Pre-aggregated: count per hour per event_type
CREATE MATERIALIZED VIEW hourly_counts
ENGINE = SummingMergeTree() ORDER BY (hour, event_type)
AS SELECT
    toStartOfHour(timestamp) AS hour,
    event_type,
    count() AS cnt
FROM raw_events
GROUP BY hour, event_type;

-- Query the materialized view: milliseconds instead of minutes
SELECT * FROM hourly_counts WHERE hour > now() - INTERVAL 24 HOUR;
```

### 4. Performance Numbers
```
Single node, commodity hardware:
  Scan speed:      1-2 billion rows/second
  Compression:     10-100x (same-type columns compress well)
  Insert speed:    1-2 million rows/second (batched)
  Storage cost:    ~10x less than row-oriented (compression)

Typical: 1TB of raw data → ~100GB in ClickHouse
```

## ClickHouse vs Other Solutions

| Aspect | ClickHouse | PostgreSQL | Elasticsearch | BigQuery |
|--------|-----------|-----------|--------------|---------|
| Query type | OLAP (aggregation) | OLTP + OLAP | Full-text search | OLAP |
| Scan speed | Billions/sec | Millions/sec | Millions/sec | Billions/sec |
| Real-time insert | Yes (batched) | Yes | Yes | No (batch load) |
| Cost | Self-hosted | Self-hosted | Self-hosted | Pay per query |
| JOINs | Limited | Full | None | Full |
| Best for | Analytics | Transactions | Search | Ad-hoc analytics |

## Limitations to Mention

- Not for OLTP (single-row updates are slow)
- No UPDATE/DELETE at row level (use ALTER TABLE + mutations)
- JOINs are limited (best to denormalize or use subqueries)
- INSERT must be batched (don't insert row-by-row)
- Eventual consistency on replicas

## Interview Sound Bite

> "For the analytics dashboard, I'd use ClickHouse because we're doing COUNT/SUM/AVG over billions of ad impressions. Columnar storage means we only read the columns we need, with 10-100x compression. We'd partition by date and use materialized views to pre-aggregate hourly metrics, giving us sub-second dashboard queries."
