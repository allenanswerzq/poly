# Presto / Trino Deep Dive

## Overview

Presto (now **Trino**) is a **distributed SQL query engine** for interactive analytics. It queries data where it lives — HDFS, S3, PostgreSQL, MySQL, Cassandra, Kafka — without moving it first. Sub-second to seconds latency on terabytes. Think "federation layer" that sits on top of all your data sources.

## History & Why It Exists

```
The problem (2012):
  Facebook had the world's largest Hive/Hadoop warehouse (300PB+).
  Hive compiled SQL to MapReduce — every query took minutes.
  Analysts needed INTERACTIVE queries (seconds, not minutes).
  They couldn't wait 10 minutes to check if their WHERE clause was right.

  Facebook built Presto: a new query engine that reads HDFS data
  directly, processes it in memory across a cluster, and returns
  results in seconds. No MapReduce, no disk spills (for most queries).

Timeline:
  2012  Facebook builds Presto internally
  2013  Open-sourced (prestodb.io)
  2019  Original creators leave Facebook, fork → Trino (trino.io)
        Two projects diverge:
          PrestoDB  — still at Facebook/Meta, maintained by them
          Trino     — community-driven, more active, the de facto successor
  2020s Trino becomes the standard. AWS Athena is Trino under the hood.

The naming confusion:
  "Presto" at Facebook (2013) → original project
  "PrestoSQL" (2019)          → the fork by original creators
  "Trino" (2021)              → renamed from PrestoSQL (trademark issues)
  "PrestoDB" (2019)           → Facebook's version, still maintained

  When people say "Presto" in interviews, they usually mean Trino.
  When AWS says "Athena," it's Trino under the hood.

What Presto replaced:
  Hive (minutes per query) → Presto (seconds per query)
  Same data in HDFS, 100x faster for interactive queries.
  But Hive is still better for massive multi-hour ETL jobs.
```

## When to Choose Presto/Trino

| Use Case | Why Presto/Trino |
|----------|----------------|
| Interactive analytics on data lake | Sub-second to seconds on S3/HDFS |
| Query federation | Join data across PostgreSQL + S3 + Kafka in one query |
| Ad-hoc exploration | Analysts run exploratory queries all day |
| AWS Athena workloads | Athena IS Trino — same concepts apply |
| Replace Hive for reads | Same data, 10-100x faster |

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                    Presto/Trino Architecture                      │
│                                                                   │
│  SQL Query                                                        │
│       │                                                           │
│       ▼                                                           │
│  ┌──────────────┐                                                │
│  │ Coordinator   │  Parses SQL, plans query, orchestrates workers │
│  │ (single node) │  Splits into stages/tasks, assigns to workers  │
│  └──────┬───────┘                                                │
│         │                                                        │
│    ┌────┴───────┬──────────────┐                                │
│    ▼            ▼              ▼                                  │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐                        │
│  │ Worker 1  │ │ Worker 2  │ │ Worker 3  │                        │
│  │           │ │           │ │           │                        │
│  │ Task A    │ │ Task B    │ │ Task C    │                        │
│  │ (scan +   │ │ (scan +   │ │ (aggregate │                        │
│  │  filter)  │ │  filter)  │ │  + output) │                        │
│  └─────┬────┘ └─────┬────┘ └──────────┘                        │
│        │            │                                             │
│        ▼            ▼                                             │
│  ┌────────────────────────────────────────┐                      │
│  │           Connectors (plugins)          │                      │
│  │                                         │                      │
│  │  Hive connector → HDFS / S3 (Parquet)  │                      │
│  │  PostgreSQL connector → PostgreSQL      │                      │
│  │  MySQL connector → MySQL                │                      │
│  │  Kafka connector → Kafka topics         │                      │
│  │  Cassandra connector → Cassandra        │                      │
│  │  Redis connector → Redis                │                      │
│  └────────────────────────────────────────┘                      │
│                                                                   │
│  Key: ALL processing is in-memory, pipelined.                    │
│  Data flows Worker→Worker without writing to disk.               │
│  No intermediate HDFS writes (unlike Hive/MapReduce).           │
└──────────────────────────────────────────────────────────────────┘
```

### How a Query Executes

```
Query: SELECT city, COUNT(*) FROM events WHERE year = 2024 GROUP BY city

1. Coordinator parses SQL → logical plan
2. Optimizer:
   - Partition pruning: only read year=2024 partition
   - Predicate pushdown: push "year=2024" into connector
   - Column pruning: only read "city" column from Parquet
3. Split into stages:
   Stage 0: Scan + partial aggregation (on each worker)
   Stage 1: Final aggregation + output (on one worker)
4. Workers execute in parallel:
   Worker 1: reads splits 1-100 → partial counts {NYC: 50, LA: 30, ...}
   Worker 2: reads splits 101-200 → partial counts {NYC: 45, SF: 20, ...}
   Worker 3: reads splits 201-300 → partial counts {NYC: 55, LA: 25, ...}
5. Stage 1 worker merges partial results:
   {NYC: 150, LA: 55, SF: 20, ...}
6. Return to client

Total time: 2-5 seconds over 1TB of Parquet in S3.
```

## Key Concepts for Interviews

### 1. Connector Architecture — Query Anything
```
Presto doesn't store data. It's a QUERY engine.
Connectors are plugins that know how to read from a specific source.

  SELECT u.name, o.total
  FROM postgresql.public.users u
  JOIN hive.warehouse.orders o ON u.id = o.user_id
  WHERE o.order_date > '2024-01-01'

This query:
  - Reads users from PostgreSQL (via PostgreSQL connector)
  - Reads orders from S3/HDFS (via Hive connector)
  - Joins them in Presto's memory
  - Returns the result

No ETL pipeline needed. No data copying. Query in place.
```

### 2. Pipelined Execution — No Intermediate Disk
```
Hive/MapReduce:
  Scan → write to HDFS → shuffle → write to HDFS → aggregate → output
  Disk I/O at every stage boundary. Slow.

Presto:
  Scan → stream to next stage → aggregate → output
  Data flows through memory (pages/buffers). No disk writes.

The tradeoff: if a query needs more memory than available,
Presto FAILS (or spills to disk slowly), while Hive completes.
Presto is built for queries that fit in cluster memory.
```

### 3. Predicate Pushdown & Column Pruning
```sql
SELECT city, COUNT(*) FROM events WHERE year = 2024 AND value > 100

Presto optimizer pushes as much work as possible into the connector:
  1. Partition pruning: tell Hive connector "only read year=2024 partition"
  2. Column pruning: "only read city and value columns" (skip the rest)
  3. Predicate pushdown: "value > 100" pushed into Parquet reader
     → skip row groups where max(value) <= 100 (Parquet statistics)

Result: read 1% of the actual data. The rest is never loaded.
```

### 4. Cost-Based Optimizer (CBO)
```
For joins, order matters hugely:

  Table A: 1 billion rows
  Table B: 1 million rows
  Table C: 1 thousand rows

  Bad plan:  A JOIN B JOIN C → shuffle 1B rows, then 1M rows
  Good plan: C JOIN B JOIN A → start small, grow gradually

  Presto's CBO uses table statistics (from Hive Metastore)
  to choose the optimal join order and join strategy:
    - Broadcast join: small table sent to all workers (fast)
    - Hash join: both tables shuffled by join key
    - Sorted merge join: pre-sorted data merged
```

### 5. Memory Management & Spilling
```
Presto runs entirely in memory. Each query gets a memory budget.

  - Query memory limit: e.g., 5GB per query
  - If exceeded: query killed (or spills to disk in newer versions)
  - This is the fundamental tradeoff vs Hive:
      Hive: always works (spills to disk), but slow
      Presto: fast (in-memory), but fails on huge queries

Solution for huge queries:
  - Use Spark for massive ETL jobs
  - Use Presto for interactive analytics (smaller result sets)
  - Tune memory limits per query type
```

## Presto/Trino vs Alternatives

| Aspect | Presto/Trino | Hive | Spark SQL | ClickHouse | BigQuery |
|--------|-------------|------|-----------|-----------|---------|
| Latency | Seconds | Minutes | Seconds-minutes | Sub-second | Seconds |
| Data location | Any (federation) | HDFS | HDFS/S3 | Own storage | Own storage |
| Best for | Interactive analytics | Batch ETL | Both batch + interactive | Pre-loaded analytics | Serverless analytics |
| Memory model | In-memory pipelined | Disk at every stage | In-memory + spill | Column scan | Managed |
| Federation? | Yes (many connectors) | No (HDFS only) | Limited | No | Some |

## Limitations to Mention

- No data storage — pure compute engine, needs data somewhere else
- Memory-bound — very large queries may fail or spill to disk slowly
- Coordinator is single point of failure (no HA coordinator yet in OSS)
- Not for heavy ETL/write workloads — read-only query engine
- Requires good data layout (partitioned Parquet) for best performance
- Join performance depends on data statistics being up-to-date

## Interview Sound Bite

> "For interactive analytics on our data lake, I'd use Trino (the open-source successor to Presto). It queries data in-place — S3, PostgreSQL, Kafka — without ETL pipelines. The connector architecture lets us federate across data sources in one SQL query. It's in-memory and pipelined, so we get sub-second queries on terabytes of Parquet, but for multi-hour ETL jobs we'd still use Spark."
