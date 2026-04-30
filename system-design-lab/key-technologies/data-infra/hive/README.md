# Hive Deep Dive

## Overview

Apache Hive is a **SQL-on-Hadoop** engine. It lets you write SQL queries against petabytes of data stored in HDFS (or S3). Hive compiles SQL into distributed jobs (MapReduce, Tez, or Spark) so analysts don't need to write Java MapReduce code.

## History & Why It Exists

```
The problem (2008):
  Facebook had petabytes of data in Hadoop/HDFS. Engineers wrote
  MapReduce jobs in Java to analyze it — hundreds of lines of
  boilerplate for what should be a simple GROUP BY query.

  Analysts couldn't use Hadoop at all. They knew SQL, not Java.

  Facebook engineers built Hive: write SQL → Hive compiles it
  to MapReduce → runs on Hadoop → returns results.
  "SQL for people who have data in Hadoop."

Timeline:
  2008  Facebook creates Hive internally
  2009  Open-sourced as Apache Hive
  2013  Hive on Tez (10x faster than MapReduce execution)
  2014  Hive on Spark (alternative execution engine)
  2016  Hive LLAP (Live Long And Process — in-memory caching layer)
  2019  Hive 3.0 (ACID transactions, materialized views)

The evolution of SQL-on-Hadoop:
  2008: Hive — SQL → MapReduce (minutes per query)
  2013: Hive + Tez — SQL → DAG execution (10x faster)
  2013: Impala — bypass MapReduce, direct HDFS reads (seconds)
  2013: Presto — Facebook's replacement for Hive (interactive SQL)
  2014: Spark SQL — SQL inside Spark (fast + programmable)
  2020s: Most new work uses Spark SQL or Presto/Trino

Hive's legacy:
  - Made big data accessible to SQL users
  - The Hive Metastore (table metadata catalog) is STILL the standard
    even for Spark, Presto, and Trino
  - HiveQL syntax influenced every SQL-on-Hadoop tool
```

## When to Choose Hive

| Use Case | Why Hive |
|----------|---------|
| Batch ETL on data lake | SQL against petabytes in HDFS/S3 |
| Existing Hadoop cluster | Already have HDFS + YARN |
| Analysts need SQL access | Don't want to write Spark code |
| Data warehouse (non-interactive) | Queries that take minutes are OK |

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        Hive Architecture                          │
│                                                                   │
│  SQL Query: "SELECT city, COUNT(*) FROM events GROUP BY city"    │
│       │                                                           │
│       ▼                                                           │
│  ┌──────────────┐                                                │
│  │  HiveServer2  │  Receives SQL, manages sessions                │
│  └──────┬───────┘                                                │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │   Compiler    │  Parse SQL → AST → logical plan → physical plan│
│  │              │  Consults Metastore for table schemas          │
│  └──────┬───────┘                                                │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │  Optimizer    │  Predicate pushdown, column pruning,          │
│  │              │  join reordering, partition pruning             │
│  └──────┬───────┘                                                │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │ Execution     │  Runs as MapReduce, Tez, or Spark job         │
│  │ Engine        │  on YARN cluster                               │
│  └──────┬───────┘                                                │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │  HDFS / S3    │  Reads data files (Parquet, ORC, CSV, etc.)   │
│  └──────────────┘                                                │
│                                                                   │
│  Separately:                                                      │
│  ┌──────────────┐                                                │
│  │  Metastore    │  Stores table schemas, partition info,        │
│  │  (MySQL/PG)   │  file locations. Shared by Spark, Presto too. │
│  └──────────────┘                                                │
└──────────────────────────────────────────────────────────────────┘
```

## Key Concepts for Interviews

### 1. Hive Metastore — The Lasting Contribution
```
The Metastore is a database (usually MySQL/PostgreSQL) that stores:
  - Table names, column names, types
  - Partition information (year=2024/month=01)
  - File locations in HDFS/S3
  - File format (Parquet, ORC, etc.)

It's the "catalog" that tells any engine WHERE data is and WHAT it looks like.

Even if you never use Hive itself:
  Spark reads from Hive Metastore
  Presto/Trino reads from Hive Metastore
  AWS Glue Data Catalog is a Hive-Metastore-compatible service

The Metastore outlived Hive's own query engine.
```

### 2. Partitioning — Skip Irrelevant Data
```sql
CREATE TABLE events (
    user_id BIGINT,
    event_type STRING,
    value DOUBLE
) PARTITIONED BY (event_date STRING)
STORED AS PARQUET;

-- Data layout on HDFS:
-- /warehouse/events/event_date=2024-01-01/data.parquet
-- /warehouse/events/event_date=2024-01-02/data.parquet
-- /warehouse/events/event_date=2024-01-03/data.parquet

-- Query with partition filter:
SELECT COUNT(*) FROM events WHERE event_date = '2024-01-15';
-- Only reads ONE partition directory. Skips all other dates.

-- Without partitioning: full table scan over ALL data. Slow.
```

### 3. File Formats — ORC & Parquet
```
Hive supports multiple storage formats:

┌────────────┬──────────────────────────────────────────────┐
│ Format     │ When to use                                   │
├────────────┼──────────────────────────────────────────────┤
│ Parquet    │ Default for most modern data lakes.           │
│            │ Columnar, compressed, used by Spark/Presto.   │
│ ORC        │ Hive-native columnar format. Better for Hive  │
│            │ specifically (predicate pushdown, ACID).       │
│ Text/CSV   │ Human-readable, no compression. Bad for perf. │
│ Avro       │ Row-based, good for schema evolution.          │
└────────────┴──────────────────────────────────────────────┘

Why columnar matters (same as ClickHouse concept):
  SELECT AVG(value) FROM events
    Parquet/ORC: reads only the "value" column. Fast.
    CSV:         reads ALL columns of every row. Slow.
```

### 4. Execution Engines — MapReduce vs Tez vs Spark
```
MapReduce (original):
  SQL → map → disk → shuffle → disk → reduce → disk
  Every intermediate result hits disk. Very slow.

Tez (Hive 0.13+):
  SQL → DAG of tasks → pipelining between stages
  Avoids unnecessary disk writes. 10x faster than MapReduce.
  Default engine for Hive on most clusters.

Spark (Hive on Spark):
  SQL → Spark RDD/DataFrame operations → in-memory processing
  Fastest option but more memory-hungry.

LLAP (Live Long And Process):
  Long-running daemons with in-memory cache.
  Pre-fetches and caches hot data. Sub-second for cached queries.
  Makes Hive "interactive" for frequently-accessed data.
```

### 5. Bucketing — Optimize Joins
```sql
-- Bucketing: hash-partition data within each partition
CREATE TABLE users (
    user_id BIGINT,
    name STRING
) CLUSTERED BY (user_id) INTO 256 BUCKETS
STORED AS ORC;

-- When you join two tables bucketed on the same key:
-- Hive can do a bucket-map-join: bucket 0 of table A joins
-- with bucket 0 of table B. No full shuffle needed.
-- Massive speedup for large joins.
```

## Hive vs Alternatives

| Aspect | Hive | Spark SQL | Presto/Trino | BigQuery |
|--------|------|----------|-------------|---------|
| Latency | Minutes (batch) | Seconds-minutes | Sub-second to seconds | Seconds |
| Use case | Batch ETL | Batch + interactive | Interactive queries | Serverless analytics |
| Compute model | MapReduce/Tez | In-memory RDDs | Pipelined in-memory | Managed |
| Metastore | Created it | Uses Hive Metastore | Uses Hive Metastore | Own catalog |
| Still used? | Legacy clusters | Most popular | Growing fast | Cloud-native |

## Limitations to Mention

- Slow for interactive queries (even with Tez, still seconds-to-minutes)
- Schema-on-read — data might not match schema (errors at query time)
- No real UPDATE/DELETE (mutations via ACID tables are slow)
- Operational complexity — Metastore, HiveServer2, YARN all need management
- Being replaced by Spark SQL and Presto/Trino for most new workloads
- The Metastore itself is the lasting piece — the query engine is legacy

## Interview Sound Bite

> "Hive made Hadoop accessible to SQL users — it compiles SQL to distributed jobs on HDFS. While the query engine is mostly replaced by Spark SQL and Trino, the Hive Metastore remains the de facto standard catalog for data lakes. For batch ETL on existing Hadoop clusters, Hive with Tez is still common, but for interactive analytics, I'd use Presto or Spark SQL."
