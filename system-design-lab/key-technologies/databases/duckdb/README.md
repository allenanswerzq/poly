# DuckDB Deep Dive

## Overview

DuckDB is an **embedded analytical (OLAP) database** — the "SQLite for analytics." No server, runs in-process, but instead of optimizing for transactional row lookups (like SQLite), it's designed to crunch through millions of rows at GB/s speeds. It's fast because of **columnar storage**, **vectorized execution**, and **morsel-driven parallelism**.

## History & Why It Exists

```
The problem (2018):
  Researchers at CWI Amsterdam (Mark Raasveldt & Hannes Mühleisen)
  noticed a gap: data scientists needed to run analytical queries on
  local datasets (CSV, Parquet, JSON), but their options were:

  1. Load into PostgreSQL/MySQL → slow for analytics (row-oriented)
  2. Spin up Spark/Presto cluster → massive overhead for local work
  3. Use pandas → no SQL, memory-limited, single-threaded

  They thought: "What if we built an analytics engine that runs
  in-process like SQLite, but is designed from the ground up for
  OLAP workloads with modern CPU-optimized execution?"

  DuckDB was born: an embeddable columnar database with a
  state-of-the-art vectorized query engine.

Timeline:
  2018  Development starts at CWI Amsterdam
  2019  First public release (0.1.0)
  2020  Python/R bindings, Parquet support
  2021  DuckDB Labs founded (commercial entity)
  2022  Adoption explodes — "pandas killer" narrative
  2023  Extensions ecosystem (spatial, httpfs, iceberg)
  2024  v1.0 stable release, production-ready
  2025  Standard tool for local analytics, data engineering

Key design philosophy:
  - Embedded: no server, runs in your process (like SQLite)
  - OLAP-optimized: columnar storage, vectorized execution
  - Zero dependencies: single binary/library, no external config
  - Parallel by default: uses all CPU cores automatically
  - Reads anything: CSV, Parquet, JSON, Arrow, PostgreSQL, MySQL
  - SQL-first: full SQL support with extensions (ASOF JOIN, LIST types)

When DuckDB makes sense (and when it doesn't):
  ✓ Analytics on local files (CSV, Parquet, JSON)
  ✓ Data exploration and transformation (ETL)
  ✓ Replacing pandas for SQL-based analysis
  ✓ Embedded analytics in applications
  ✓ Single-machine datasets (fits in memory or local SSD)
  ✗ High-concurrency OLTP (many small transactions)
  ✗ Multi-node distributed queries (single machine only)
  ✗ Real-time ingestion with concurrent writes
```

## DuckDB vs SQLite — Why Two Embedded DBs?

```
┌──────────────────────┬──────────────────────────┬──────────────────────────┐
│                      │ SQLite                    │ DuckDB                    │
├──────────────────────┼──────────────────────────┼──────────────────────────┤
│ Workload             │ OLTP (transactions)       │ OLAP (analytics)          │
│ Storage layout       │ Row-oriented              │ Column-oriented           │
│ Best at              │ INSERT/UPDATE single row   │ SELECT across millions    │
│ Execution            │ Tuple-at-a-time (row VM)  │ Vectorized (batch of cols)│
│ Parallelism          │ Single-threaded            │ Multi-threaded by default │
│ Typical query        │ WHERE id = 42              │ GROUP BY city SUM(sales)  │
│ Read external files  │ No                        │ Yes (CSV, Parquet, JSON)  │
│ Concurrency model    │ File-level locking         │ MVCC                     │
└──────────────────────┴──────────────────────────┴──────────────────────────┘

Same idea (embedded, no server), opposite workloads.
```

## Architecture — Why It's Fast

### The Full Stack

```
┌──────────────────────────────────────────────────────────────────────────┐
│           DuckDB Internal Architecture                                   │
│                                                                          │
│  SQL: SELECT city, SUM(sales) FROM orders GROUP BY city                  │
│                                                                          │
│  ┌──────────────────┐                                                   │
│  │  Parser           │  SQL → AST (uses PostgreSQL's parser, libpg)     │
│  └──────┬───────────┘                                                   │
│         ▼                                                                │
│  ┌──────────────────┐                                                   │
│  │  Binder/Planner   │  Resolve names, types → logical plan             │
│  └──────┬───────────┘                                                   │
│         ▼                                                                │
│  ┌──────────────────┐                                                   │
│  │  Optimizer        │  Filter pushdown, join reordering, CTE           │
│  │                   │  materialization, cardinality estimation          │
│  └──────┬───────────┘                                                   │
│         ▼                                                                │
│  ┌──────────────────┐                                                   │
│  │  Physical Planner │  Logical plan → physical operators               │
│  │                   │  Choose hash join vs merge join, etc.            │
│  └──────┬───────────┘                                                   │
│         ▼                                                                │
│  ┌──────────────────┐  ◄── THIS is the key differentiator              │
│  │  Vectorized       │  Processes data in vectors of 2048 values        │
│  │  Execution Engine │  Column-at-a-time, not row-at-a-time             │
│  │  (push-based)     │  SIMD-friendly, cache-friendly                   │
│  └──────┬───────────┘                                                   │
│         ▼                                                                │
│  ┌──────────────────┐                                                   │
│  │  Morsel-Driven    │  Work divided into "morsels" (~10K rows)         │
│  │  Parallelism      │  Threads grab morsels dynamically                │
│  │                   │  No partitioning needed — automatic              │
│  └──────┬───────────┘                                                   │
│         ▼                                                                │
│  ┌──────────────────┐                                                   │
│  │  Storage Layer    │  Columnar pages, compression per column          │
│  │                   │  Or reads directly from Parquet/CSV/Arrow        │
│  └──────┬───────────┘                                                   │
│         ▼                                                                │
│      data.duckdb  OR  external files (Parquet, CSV, etc.)               │
└──────────────────────────────────────────────────────────────────────────┘
```

### 1. Columnar Storage — WHY it matters for analytics

```
Row-oriented (SQLite, PostgreSQL):

  Row 1: │ Alice │ NYC     │ 500  │ 2024-01 │
  Row 2: │ Bob   │ Chicago │ 300  │ 2024-01 │
  Row 3: │ Carol │ NYC     │ 700  │ 2024-02 │
  Row 4: │ Dave  │ Chicago │ 200  │ 2024-02 │

  To compute SUM(sales), you must read EVERY column of EVERY row,
  even though you only need the "sales" column. Wasted I/O.

Column-oriented (DuckDB):

  names:  │ Alice │ Bob   │ Carol │ Dave  │
  cities: │ NYC   │ Chi   │ NYC   │ Chi   │
  sales:  │ 500   │ 300   │ 700   │ 200   │  ← read ONLY this
  dates:  │ 01    │ 01    │ 02    │ 02    │

  SUM(sales) reads only the sales column → 4x less I/O.
  With 100 columns, that's 100x less I/O.

Why columnar is faster for analytics:
  1. Read only columns you need (massive I/O savings)
  2. Same-type values compress better (RLE, dictionary, bitpacking)
  3. Sequential memory access → CPU cache lines fully utilized
  4. SIMD: process 4/8/16 values in a single CPU instruction
```

┌──────────────────────┬──────────────────────┬──────────────────────┐
│                      │ DuckDB native         │ Parquet               │
├──────────────────────┼──────────────────────┼──────────────────────┤
│ Layout               │ Columnar (row groups) │ Columnar (row groups) │
│ Compression          │ Per-column (auto)     │ Per-column (configurable)│
│ Mutable?             │ Yes (UPDATE, DELETE)  │ No (immutable file)   │
│ Transactions         │ Yes (ACID, MVCC)      │ No                    │
│ Indexes              │ Yes (ART, zone maps)  │ No (min/max stats only)│
│ File = query-ready?  │ Yes (for DuckDB only) │ Yes (for any tool)    │
│ Cross-tool compat    │ No (DuckDB-specific)  │ Yes (universal)       │
│ Streaming writes     │ Yes (INSERT INTO)     │ No (write whole file) │
│ Typical use          │ Persistent analytics DB│ Data interchange     │
│                      │                       │ Data lake files        │
└──────────────────────┴──────────────────────┴──────────────────────┘

### 2. Vectorized Execution — The Core Engine

```
Traditional (Volcano / tuple-at-a-time):

  for each row:                    ← virtual function call per row
    get row from scan              ← pipeline stall
    apply filter (age > 25)        ← branch misprediction
    project columns                ← another virtual call
    aggregate into group           ← random memory access

  10 million rows = 10 million iterations with function call overhead.
  CPU spends most time on overhead, not actual computation.

Vectorized (DuckDB) push based, explicit pipeline graph:

  get 2048 values of "age" column    ← one array
  compare all 2048 with 25           ← SIMD: 8 comparisons per instruction
  get 2048 values of "sales" column  ← sequential read, cache-friendly
  sum the matching sales             ← tight loop, CPU loves this

  10 million rows = ~5000 batches. Function call overhead is amortized
  over 2048 rows. Inner loops are tight, SIMD-friendly, branch-free.

The vector size (2048) is chosen to fit in L1/L2 cache:
  2048 × 8 bytes (int64) = 16KB ≈ fits in L1 cache (32-64KB)

  ┌─────────────────────────────────────────────────────────┐
  │  Why 2048?                                               │
  │                                                          │
  │  Too small → function call overhead dominates            │
  │  Too large → doesn't fit in CPU cache, get cache misses  │
  │  2048 → sweet spot: amortizes overhead, fits in L1       │
  └─────────────────────────────────────────────────────────┘
```

### 3. Morsel-Driven Parallelism — Automatic Multi-threading

```
Traditional parallelism (partition-based):
  Pre-partition data into N chunks (one per thread).
  Problem: if partitions are uneven → some threads finish early, sit idle.

Morsel-driven (DuckDB):
  Divide data into small "morsels" (~10K rows).
  Threads dynamically grab the next available morsel.
  Like a work-stealing scheduler.

  Thread 1: ██████░░░░  grabs morsel 1, 4, 7 ...
  Thread 2: █████░░░░░  grabs morsel 2, 5, 8 ...
  Thread 3: ██████░░░░  grabs morsel 3, 6, 9 ...

  Automatic load balancing — no skew, no idle threads.
  No locks on the hot path (each thread has local state).
  Global state (hash tables for GROUP BY) uses lock-free merging.

  Pipeline model:
  ┌─────────┐    ┌─────────┐    ┌─────────┐
  │  Scan    │───►│ Filter  │───►│  Hash   │──► results
  │ (morsel) │    │         │    │  Agg    │
  └─────────┘    └─────────┘    └─────────┘
       ▲
       │ each thread grabs next morsel
       │ independently (no coordination)
```

### 4. Compression — Less I/O, More Speed

```
Each column is compressed independently using the best algorithm:

  Column Type     Compression           Ratio
  ─────────────────────────────────────────────
  status (enum)   Dictionary encoding   50-100x
  timestamps      Delta + bitpacking    10-20x
  prices          Bitpacking (FOR)      3-5x
  names           Dictionary + FSST     5-10x
  flags           Run-Length Encoding   100x+
  nulls           Validity bitmask      compact

  Dictionary encoding example:
    Raw:    │ "error" │ "ok" │ "ok" │ "error" │ "ok" │
    Dict:   { 0: "error", 1: "ok" }
    Stored: │ 0 │ 1 │ 1 │ 0 │ 1 │  ← 1 byte each instead of strings

  Lightweight decompression — fast enough to decompress during query.
  Often FASTER to read compressed data (less I/O) + decompress (cheap)
  than to read uncompressed data.
```

### 5. Data Ingestion — How Writes Work and Why They're Fast

```
DuckDB can ingest data at 1-10 GB/s depending on source format.
Three main paths: bulk load, streaming inserts, and direct file queries.

BULK LOAD (the primary ingestion path):

  CREATE TABLE events AS SELECT * FROM 'events.parquet';
  -- or:
  COPY events FROM 'events.csv' (FORMAT CSV, HEADER);
  -- or:
  INSERT INTO events SELECT * FROM read_parquet('s3://lake/*.parquet');

  What happens internally:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                               │
  │  Source (Parquet/CSV) → Pipeline:                            │
  │                                                               │
  │  1. SCAN: read source in batches (morsel = ~10K rows)       │
  │     For Parquet: decode column chunks → Arrow vectors         │
  │     For CSV: parallel CSV parser (one thread per file chunk) │
  │                                                               │
  │  2. TRANSFORM (in pipeline): cast types, evaluate defaults   │
  │                                                               │
  │  3. COMPRESS: pick best encoding per column per row group    │
  │     Dictionary? Bitpacking? RLE? FSST? Analyze the data.    │
  │                                                               │
  │  4. WRITE ROW GROUP: flush compressed columns to storage    │
  │     Append to .duckdb file (allocate new blocks).           │
  │                                                               │
  │  All steps are PIPELINED and PARALLEL:                       │
  │    Thread 1: scan batch 0 → compress → write                │
  │    Thread 2: scan batch 1 → compress → write                │
  │    Thread 3: scan batch 2 → compress → write                │
  │    ...                                                        │
  │                                                               │
  │  Throughput: bounded by the SLOWEST step:                    │
  │    Parquet source: ~2-5 GB/s (decode + decompress Zstd)     │
  │    CSV source: ~500 MB-2 GB/s (parsing is expensive)        │
  │    Compression: ~2-5 GB/s (lightweight encodings)            │
  │    Disk write: ~1-3 GB/s (SSD sequential write)             │
  │                                                               │
  │  Net: ~1-3 GB/s for Parquet→DuckDB. ~500 MB/s for CSV.     │
  └──────────────────────────────────────────────────────────────┘

  Parquet → DuckDB is fastest because:
    - Parquet is already COLUMNAR → no row-to-column conversion
    - Statistics carry over (min/max → zone maps)
    - Can read only needed columns (if CREATE TABLE AS SELECT subset)
    - Parquet decompression parallelizes well

  CSV → DuckDB is slower because:
    - Text parsing (detect types, handle quoting, escape chars)
    - Must sniff schema (scan first N rows to guess types)
    - Row-to-column conversion (CSV is row-oriented text)
    - BUT: DuckDB's parallel CSV reader splits file by byte offset
      and assigns chunks to threads. Much faster than single-threaded.


WHY IT'S FAST (vs PostgreSQL COPY):

  PostgreSQL COPY FROM 'file.csv':
    Parse CSV → build heap tuple → find page → write to table →
    update indexes → write WAL → fsync.
    Per-row overhead: tuple header, page management, WAL record.
    Speed: ~100-500 MB/s (limited by WAL, indexes, row format)

  DuckDB bulk load:
    Parse CSV → accumulate in columnar vectors → when row group full:
    compress per column → write one contiguous block → done.
    No per-row overhead. No WAL during bulk load. No indexes to update.
    Speed: ~500 MB-3 GB/s (limited by parsing or disk)

  ┌──────────────────────────────────────────────────────────────┐
  │  Why DuckDB bulk load is 5-10× faster than PostgreSQL:       │
  │                                                               │
  │  1. No WAL: bulk load doesn't write a write-ahead log       │
  │     (single-writer, can just write data directly).           │
  │  2. No per-row overhead: no 23-byte tuple header per row.   │
  │  3. No indexes: append compressed blocks, no B-tree updates.│
  │  4. Batch compression: compress 122K rows at once (efficient)│
  │     vs per-row processing.                                   │
  │  5. Parallel: multiple threads scan + compress simultaneously│
  │  6. Sequential writes: append blocks to end of file.         │
  │     No random I/O, no page management.                      │
  └──────────────────────────────────────────────────────────────┘


STREAMING INSERTS (row-by-row or small batches):

  INSERT INTO events VALUES (1, 'click', '2024-07-15', 42);
  INSERT INTO events VALUES (2, 'view',  '2024-07-15', 10);
  ...

  This is SLOW compared to bulk load. Why:
    - Each INSERT is a full transaction (WAL write + commit).
    - Small batches don't fill a row group → poor compression.
    - Single-writer: each commit blocks until fsync.
    - Per-row overhead dominates (WAL record per small insert).

  Speed: ~10K-100K rows/sec (vs millions for bulk load).

  FIX: batch inserts into larger transactions:

    BEGIN;
    INSERT INTO events SELECT * FROM ...;  -- 100K rows at once
    COMMIT;

    Or use COPY for file-based ingestion. Or use appender API:

    # Python appender API (fastest streaming insert):
    appender = con.appender("events")
    for row in rows:
        appender.append_row(row)
    appender.close()  # one commit for all rows

  ┌──────────────────────────────────────────────────────────────┐
  │  Rule: NEVER insert row-by-row in a loop.                    │
  │  Always batch (COPY, INSERT...SELECT, or appender API).     │
  │  1 INSERT of 100K rows >> 100K INSERTs of 1 row.           │
  └──────────────────────────────────────────────────────────────┘


DIRECT QUERY (no ingestion at all):

  SELECT * FROM 'data.parquet' WHERE date = '2024-07-15';

  DuckDB doesn't copy the data into its own storage.
  It reads Parquet (or CSV, JSON) directly during the query.
  Zero ingestion time. Zero storage duplication.

  Tradeoff: slower for REPEATED queries (no DuckDB indexes/compression).
  For ad-hoc exploration: perfect.
  For repeated dashboards: bulk load once, query many times.


INGESTION SPEED COMPARISON:

  ┌────────────────────────────┬──────────────────────────────────┐
  │ Source → DuckDB             │ Approximate speed                │
  ├────────────────────────────┼──────────────────────────────────┤
  │ Parquet (local SSD)        │ 2-5 GB/s                         │
  │ Parquet (S3)               │ limited by network (~1 Gbps)    │
  │ CSV (local SSD)            │ 500 MB - 2 GB/s                  │
  │ CSV (with type detection)  │ 200-500 MB/s                     │
  │ JSON (newline-delimited)   │ 200-500 MB/s                     │
  │ PostgreSQL (over network)  │ 50-200 MB/s (network + serialize)│
  │ Row-by-row INSERT          │ 10K-100K rows/s (don't do this!) │
  │ Appender API (batched)     │ 500K-2M rows/s                   │
  └────────────────────────────┴──────────────────────────────────┘

  For perspective: 1 GB CSV file:
    DuckDB COPY: ~1-2 seconds
    PostgreSQL COPY: ~5-10 seconds
    pandas read_csv: ~10-30 seconds
```

### 6. Direct File Reading — Zero-Copy When Possible

```
Traditional approach:
  CSV file ──► LOAD into database ──► query database
  (slow ETL step, doubles storage)

DuckDB:
  SELECT * FROM 'sales_2024.parquet' WHERE region = 'US';
  SELECT * FROM read_csv('data.csv', header=true);
  SELECT * FROM 'https://example.com/data.parquet';

  Parquet is already columnar → DuckDB reads only needed columns.
  Predicate pushdown → skip entire row groups that don't match.
  No import step. No copy. Query files directly.

  ┌──────────┐     ┌──────────────┐
  │ Parquet  │────►│ DuckDB only  │
  │ file     │     │ reads the    │
  │          │     │ "sales" col  │
  │ col: id  │     │ from disk    │
  │ col: name│     │              │
  │ col:sales│◄────│              │
  │ col: date│     │ + skips row  │
  │          │     │ groups where │
  │          │     │ min(sales)>X │
  └──────────┘     └──────────────┘
```

## Why It's So Fast — Summary

```
┌────────────────────────────────────────────────────────────────────────┐
│  Technique              │ What it does              │ Speedup         │
├─────────────────────────┼───────────────────────────┼─────────────────┤
│ Columnar storage        │ Read only needed columns  │ 10-100x less IO │
│ Vectorized execution    │ Batch process 2048 values │ 5-10x CPU       │
│ SIMD instructions       │ 8+ ops per CPU cycle      │ 4-8x compute    │
│ Morsel parallelism      │ Auto multi-thread         │ Nx (N = cores)  │
│ Compression             │ Less I/O, fits in cache   │ 3-10x I/O       │
│ Late materialization    │ Don't build rows until end│ 2-5x less mem   │
│ Predicate pushdown      │ Skip data early           │ variable        │
│ ART indexes             │ Adaptive Radix Tree       │ fast point look │
│ Zero-copy Parquet       │ No import/ETL step        │ ∞ (no load)     │
└─────────────────────────┴───────────────────────────┴─────────────────┘

Combined effect on a typical analytical query:
  PostgreSQL (row, tuple-at-a-time): 30 seconds
  DuckDB (columnar, vectorized):    0.3 seconds  ← 100x faster
```

## Concurrency Model

```
DuckDB uses MVCC (Multi-Version Concurrency Control):
  - Multiple readers + one writer (ACID compliant)
  - Readers never block writers, writers never block readers
  - Each transaction sees a consistent snapshot

  This is different from SQLite's file-level locking.
  More concurrent, but still single-writer (embedded, remember).

  HyPer-style MVCC:
  - Stores "undo buffers" for in-flight transactions
  - Main storage always has latest version (optimized for reads)
  - Old versions reconstructed from undo log when needed
```

## Buffer Manager — Larger-Than-Memory Queries

```
DuckDB can query datasets larger than RAM:

  ┌──────────────────────────────────────────────┐
  │  Buffer Manager                               │
  │                                                │
  │  Memory limit: 4 GB                           │
  │  Dataset: 50 GB Parquet                       │
  │                                                │
  │  1. Load blocks into memory as needed         │
  │  2. When full, evict LRU blocks to temp file  │
  │  3. Hash tables spill to disk (grace hashing) │
  │  4. Sorts use external merge sort             │
  │                                                │
  │  Result: slower than in-memory, but works.    │
  │  Streaming operators (filter, project) don't  │
  │  need extra memory at all.                    │
  └──────────────────────────────────────────────┘
```

## Extension System

```
DuckDB has a modular extension system (shared libraries loaded at runtime):

  INSTALL httpfs;         -- read from S3/HTTP
  INSTALL spatial;        -- PostGIS-like spatial queries
  INSTALL iceberg;        -- read Iceberg tables
  INSTALL postgres;       -- attach PostgreSQL as data source
  INSTALL sqlite;         -- attach SQLite databases
  INSTALL json;           -- JSON parsing and extraction
  INSTALL fts;            -- full-text search

  Extensions can add:
  - New table functions (read_parquet, read_csv)
  - New scalar/aggregate functions
  - New data types
  - New file systems (S3, GCS, HTTP)
  - Foreign data wrappers (PostgreSQL, MySQL, SQLite)
```

## Common Patterns

### Replace pandas

```python
# pandas: single-threaded, memory-hungry
df = pd.read_csv("big.csv")
result = df.groupby("city")["sales"].sum()

# DuckDB: multi-threaded, streaming, SQL
import duckdb
result = duckdb.sql("""
    SELECT city, SUM(sales)
    FROM 'big.csv'
    GROUP BY city
""").fetchdf()

# DuckDB can query pandas DataFrames directly too:
result = duckdb.sql("SELECT * FROM df WHERE sales > 1000")
```

### Query Parquet on S3

```sql
INSTALL httpfs;
LOAD httpfs;
SET s3_region = 'us-east-1';

SELECT date_trunc('month', ts) AS month, COUNT(*)
FROM 's3://my-bucket/events/*.parquet'
WHERE event_type = 'purchase'
GROUP BY month
ORDER BY month;
```

## When to Use What

```
Need OLTP (transactions, single-row ops)?  → SQLite / PostgreSQL
Need OLAP (analytics, aggregations)?       → DuckDB
Need local file analysis (CSV/Parquet)?    → DuckDB
Need distributed analytics (TB+)?         → ClickHouse / Spark / Presto
Need concurrent writes from many users?    → PostgreSQL / MySQL
Need embedded analytics in your app?       → DuckDB
```


## Key Internals to Know

| Component | Implementation | Why |
|-----------|---------------|-----|
| Parser | PostgreSQL's libpg_query | Battle-tested SQL parser, full compatibility |
| Optimizer | Cost-based (Cascades-style) | Cardinality estimation, join reordering |
| Execution | Push-based vectorized | Better than pull (Volcano) for OLAP |
| Vectors | 2048 values per vector | Fits L1 cache, amortizes overhead |
| Parallelism | Morsel-driven | Dynamic load balancing, no partition skew |
| Storage | Row groups + columnar pages | Compression per column, zone maps |
| Indexes | ART (Adaptive Radix Tree) | Memory-efficient, fast point lookups |
| Strings | FST + overflow pages | Short strings inline, long strings separate |
| MVCC | HyPer-style (undo buffers) | Optimistic for read-heavy OLAP |

## How UPDATE and DELETE Work in a Columnar Database

```
Columnar files (Parquet) are IMMUTABLE. You can't change a value in place
because columns are compressed and packed together. So how does DuckDB
support UPDATE and DELETE in its native .duckdb format?

DuckDB uses a combination of:
  1. Row groups (similar to Parquet row groups, but mutable metadata)
  2. An UNDO LOG for MVCC
  3. Mark-and-rewrite for committed changes

┌──────────────────────────────────────────────────────────────────────┐
│  DuckDB Native Storage Layout:                                       │
│                                                                       │
│  Row Group 0 (e.g., 122,880 rows):                                  │
│    ┌──────────────┐ ┌──────────────┐ ┌──────────────┐              │
│    │ city column   │ │ sales column │ │ status column│              │
│    │ (compressed)  │ │ (compressed) │ │ (compressed) │              │
│    └──────────────┘ └──────────────┘ └──────────────┘              │
│    + validity masks (which rows are "alive" vs deleted)             │
│    + version pointers (for in-flight transactions)                  │
│                                                                       │
│  Row Group 1: ...                                                    │
│  Row Group 2: ...                                                    │
└──────────────────────────────────────────────────────────────────────┘


DELETE — mark rows as deleted, don't physically remove:

  DELETE FROM orders WHERE order_id = 42;

  Step 1: Find which row group and row index contains order_id=42.
          (Scan or use zone maps / ART index to locate.)

  Step 2: Set a DELETION MARKER for that row.
          DuckDB maintains a validity bitmask per row group:
            row 0: alive
            row 1: alive
            row 2: DELETED  ← just flip this bit
            row 3: alive

          The compressed column data is NOT rewritten.
          Only the bitmask changes (tiny metadata update).

  Step 3: Future reads skip deleted rows using the bitmask.
          Scans check: if bit is 0 → skip this row.

  Step 4: Eventually, VACUUM/compaction rewrites the row group
          WITHOUT the deleted rows → reclaims space.


UPDATE — internally a DELETE + INSERT:

  UPDATE orders SET status = 'shipped' WHERE order_id = 42;

  Step 1: Mark the old row as DELETED (same as above).

  Step 2: INSERT a new row with the updated values.
          The new row goes into a WRITE-AHEAD structure
          (in-memory buffer or new row group segment).

  Step 3: Both the delete marker and the new row are part of
          the SAME transaction. Either both are visible or neither.
          MVCC (undo buffers) ensures this.

  When does the new row reach permanent row-group storage?

    COMMIT time:
      The update is durable IMMEDIATELY — written to the WAL file.
      If crash after commit → replay WAL on restart → update survives.
      But: the row group on disk is NOT yet rewritten. The new row
      lives in the WAL + in-memory buffer.

    CHECKPOINT time (periodic or explicit):
      DuckDB merges buffered changes into proper row groups on disk.
      The update buffer's rows are compressed into new row groups.
      The old row group is rewritten without the deleted row.
      WAL entries for committed changes are discarded.
      After checkpoint: everything is in columnar storage. Clean state.

    Timeline:
      COMMIT ───────────────────── CHECKPOINT ────────────────────►
        │                              │
        │ durable via WAL              │ merged into row group
        │ new row in memory buffer     │ buffer flushed
        │ reads merge buffer + disk    │ WAL truncated
        │                              │ reads hit only row groups
        │                              │
      crash? → replay WAL           crash? → clean, nothing to replay

    The memory buffer does NOT grow unbounded:
      - CHECKPOINT fires automatically when WAL exceeds a size threshold
        (or ~200K rows buffered, configurable via wal_autocheckpoint).
      - Explicit: CHECKPOINT; or PRAGMA force_checkpoint;
      - On clean shutdown: automatic checkpoint.
      - Between checkpoints: buffer holds at most a few hundred MB.

  ┌──────────────────────────────────────────────────────────────┐
  │  Before UPDATE:                                               │
  │                                                               │
  │  Row Group 0 (compressed):                                   │
  │    row 42: {order_id:42, status:"pending", city:"NYC"}       │
  │                                                               │
  │  After UPDATE (same row group, NOT rewritten):               │
  │                                                               │
  │  Row Group 0 (compressed, unchanged!):                       │
  │    row 42: {order_id:42, status:"pending", city:"NYC"}       │
  │            ↑ MARKED DELETED (validity bit = 0)               │
  │                                                               │
  │  New segment / update buffer:                                 │
  │    new row: {order_id:42, status:"shipped", city:"NYC"}      │
  │             ↑ this is the current version                    │
  │                                                               │
  │  After CHECKPOINT:                                           │
  │  Row Group 0' (compressed, rewritten):                       │
  │    row 42: {order_id:42, status:"shipped", city:"NYC"}       │
  │            ↑ permanent. Buffer and WAL cleared.              │
  │                                                               │
  │  Read path merges: skip deleted row 42, return new version.  │
  └──────────────────────────────────────────────────────────────┘


MVCC — how concurrent reads see consistent state:

  DuckDB uses HyPer-style MVCC with UNDO BUFFERS:

  Main storage always has the LATEST committed version.
  If a transaction needs to see an OLDER version, DuckDB reconstructs
  it from undo buffers (reverse the latest change).

  Transaction T1 (started before the UPDATE):
    Reads row 42 → checks undo buffer → sees OLD version: status="pending"

  Transaction T2 (started after the UPDATE committed):
    Reads row 42 → sees MAIN storage: status="shipped"

  This is the OPPOSITE of PostgreSQL's MVCC:
    PostgreSQL: main storage has OLD version, new version in heap.
    DuckDB:     main storage has NEW version, old version in undo buffer.

  Why? DuckDB is read-heavy/OLAP. Most readers want the LATEST data.
  Keeping latest in main storage means most reads never touch the undo log.


Why Parquet can't do this:

  Parquet files are sealed after writing. There is no:
    - Validity bitmask to mark deletions
    - Undo buffer for MVCC
    - In-place metadata update mechanism
    - Transaction coordination between files

  To "UPDATE" a Parquet file, you must:
    1. Read the entire file
    2. Modify the rows in memory
    3. Write an entirely NEW Parquet file
    4. Delete the old file

  Or use Iceberg/Delta Lake which manage delete files + new data files
  alongside the original Parquet (Merge-on-Read or Copy-on-Write).

  DuckDB's native format avoids this by building mutability into the
  row group metadata layer, while keeping column data compressed.


Summary:

  ┌──────────────────┬──────────────────────┬──────────────────────┐
  │ Operation        │ DuckDB native         │ Parquet file          │
  ├──────────────────┼──────────────────────┼──────────────────────┤
  │ DELETE           │ Flip validity bit     │ Rewrite entire file  │
  │                  │ (no data rewrite)     │ (or use Iceberg)     │
  │ UPDATE           │ Mark deleted + insert │ Rewrite entire file  │
  │                  │ new version           │ (or use Iceberg)     │
  │ MVCC             │ Undo buffers (latest  │ N/A (no transactions)│
  │                  │ in main storage)      │                      │
  │ Space reclaim    │ Background compaction │ Write new file       │
  │ Concurrency      │ Multiple readers +    │ N/A                  │
  │                  │ single writer          │                      │
  └──────────────────┴──────────────────────┴──────────────────────┘
```

## The .duckdb File Format — Single File, Everything Inside

```
DuckDB stores ALL data in ONE file (like SQLite). No separate directories,
no loose SSTables, no WAL segments scattered on disk.

  my_analytics.duckdb
  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                   │
  │  HEADER (magic, version, flags)                                  │
  │                                                                   │
  │  METADATA BLOCKS:                                                │
  │    - Table catalog (table names, schemas, column types)          │
  │    - Index catalog (ART index metadata)                          │
  │    - Block allocation map (which blocks are free/used)           │
  │                                                                   │
  │  DATA BLOCKS (row groups for each table):                        │
  │    - Table "orders" row group 0:                                 │
  │        [order_id column, compressed]                             │
  │        [city column, compressed]                                 │
  │        [amount column, compressed]                               │
  │    - Table "orders" row group 1: ...                             │
  │    - Table "users" row group 0: ...                              │
  │                                                                   │
  │  INDEX BLOCKS:                                                   │
  │    - ART index data for orders.order_id                          │
  │                                                                   │
  │  FREE BLOCKS (recycled space from deletes/updates):              │
  │    - Available for reuse by new writes                           │
  │                                                                   │
  └──────────────────────────────────────────────────────────────────┘

  Plus a temporary WAL file during operation:
    my_analytics.duckdb.wal  (deleted after clean shutdown/checkpoint)
```

### Block Allocator — Managing Space Inside One File

```
The file is divided into fixed-size BLOCKS (typically 256 KB):

  Block 0:  [header + metadata]
  Block 1:  [table catalog]
  Block 2:  [orders.order_id column, row group 0]
  Block 3:  [orders.city column, row group 0]
  Block 4:  [orders.amount column, row group 0]
  Block 5:  [users.name column, row group 0]
  Block 6:  [FREE — previously deleted data]
  Block 7:  [orders.order_id column, row group 1]
  Block 8:  [ART index block]
  ...

  INSERT new data → allocate from free blocks → write compressed columns.
  DELETE data → mark blocks as free (reusable later).
  CHECKPOINT → flush WAL into proper blocks, update metadata atomically.
```

### The WAL (Temporary Second File)

```
During operation, two files exist:

  my_analytics.duckdb      ← main storage (row groups, metadata)
  my_analytics.duckdb.wal  ← write-ahead log (uncommitted/recent changes)

  WAL lifecycle:
    INSERT/UPDATE/DELETE → appended to WAL first (fast sequential write).
    CHECKPOINT → WAL changes merged into main file → WAL deleted.
    Clean shutdown → automatic checkpoint → WAL disappears.
    Crash with WAL → replay WAL on next open → consistent state restored.

  After clean shutdown: truly a SINGLE file on disk.
```

### How Big Can It Get?

```
┌────────────────────────────┬────────────────────────────────────────┐
│ Limit                      │ Value                                  │
├────────────────────────────┼────────────────────────────────────────┤
│ DuckDB internal limit      │ None (no built-in max)                 │
│ ext4 filesystem            │ 16 TB max file size                    │
│ XFS filesystem             │ 8 EB (effectively unlimited)           │
│ NTFS (Windows)             │ 16 TB                                  │
│ APFS (macOS)               │ 8 EB                                   │
├────────────────────────────┼────────────────────────────────────────┤
│ Practical sweet spot       │ 1 GB – 500 GB                          │
│ "Works but getting big"    │ 500 GB – 2 TB                          │
│ "Use something distributed"│ > 2 TB                                 │
└────────────────────────────┴────────────────────────────────────────┘

What limits you in practice (not the file format):
  - CHECKPOINT time: larger file → more dirty blocks to flush.
    1 TB DB → checkpoint could take 10+ seconds.
  - VACUUM: rewrites entire file to reclaim deleted space.
    500 GB → reads + writes 500 GB. Can take 30+ minutes.
  - BACKUP: cp file /backup/ copies the whole thing.
  - RAM: some operations (hash joins, sorts) work better with
    RAM ≈ 10-25% of DB size. DuckDB spills to disk but slower.
```

### Why Single File Works for DuckDB

```
Advantages:
  + Simple: one file to copy, backup, move, delete.
  + Atomic: checkpoint updates metadata block → all-or-nothing.
  + No loose files, no orphans, no directory management.
  + Works on any filesystem.
  + Easy embedding: open one file, no configuration.

Disadvantages:
  - Can't span multiple disks.
  - Internal fragmentation (holes from deletes) → need VACUUM.
  - Multi-process access needs file locking.
  - Large files: backup/vacuum operates on full file size.

For DuckDB's target (embedded analytics, single machine, GBs to low TBs),
single file is perfect. Same design choice as SQLite.
```

### DuckDB Native vs Parquet — Storage Comparison

```
┌──────────────────────┬──────────────────────────┬──────────────────────────┐
│                      │ DuckDB native (.duckdb)   │ Parquet (.parquet)        │
├──────────────────────┼──────────────────────────┼──────────────────────────┤
│ Files                │ 1 file (all tables)       │ 1 file per table/partition│
│ Mutable              │ Yes (UPDATE, DELETE)      │ No (immutable)            │
│ Transactions (ACID)  │ Yes                       │ No                        │
│ Compression          │ FSST, ALP, bitpack, dict │ Dict, RLE, bitpack + Zstd│
│ Compression ratio    │ ~6-8× vs raw CSV         │ ~8-10× vs raw (with Zstd)│
│ Decode speed         │ Faster (no Zstd in path) │ Slower (Zstd decompress)  │
│ Cross-tool compat    │ DuckDB only              │ Universal (any tool)      │
│ Best for             │ Persistent local DB       │ Data interchange/lake     │
│ Max practical size   │ ~500 GB – 2 TB           │ ~128 MB – 1 GB per file   │
│                      │                           │ (unlimited total via many)│
└──────────────────────┴──────────────────────────┴──────────────────────────┘

Most DuckDB users never use the native format:
  - Query Parquet files directly: SELECT * FROM '*.parquet'
  - Results in memory or exported as Parquet
  - Native .duckdb only when you need UPDATE/DELETE or persistence
```

## Checkpoint — How WAL Becomes Permanent Storage

```
Checkpoint merges everything from the WAL + memory buffer into the
main .duckdb file, making it self-contained again.

BEFORE CHECKPOINT:

  my_analytics.duckdb (main file):
    Row Groups with old compressed data (some rows marked deleted).
    Metadata points to these row groups.

  my_analytics.duckdb.wal (WAL file):
    [INSERT 1000 rows], [DELETE row 42], [UPDATE row 99], ...
    Accumulated since last checkpoint.

  Memory buffer:
    New rows (not yet compressed into row groups).
    Delete markers. Updated row versions.

WHAT CHECKPOINT DOES:

  Step 1: Write new/modified row groups to main file.
    - Buffered INSERTs → compress columns → write as NEW row group.
    - Row groups with deletions → rewrite WITHOUT deleted rows.
    - Updated rows → new version placed in proper row group.
    - All written as new blocks in the .duckdb file.

  Step 2: Update metadata in main file (ATOMIC commit point).
    - Table catalog: row counts, row group pointers.
    - Block allocation: mark old blocks as free, new as used.
    - This metadata write is the point of no return.

  Step 3: fsync the main file.
    - All new data + metadata flushed to stable storage.
    - Main file is now self-contained and consistent.

  Step 4: Delete/truncate the WAL file.
    - No longer needed. Changes are in the main file.

  Step 5: Clear memory buffers.
    - Update buffer emptied. Undo log entries discarded.
    - Memory usage drops.

AFTER CHECKPOINT:

  my_analytics.duckdb: complete, self-contained, all data present.
  my_analytics.duckdb.wal: GONE (or empty).
  Memory buffer: empty, fresh start.
```

### When Checkpoint Fires

```
AUTOMATIC:
  wal_autocheckpoint = 16 MB (default, configurable).
  When WAL exceeds this size → trigger checkpoint.

  Small threshold (16 MB):
    Checkpoints often. WAL stays small. Crash recovery fast.
    Slightly more I/O overhead.

  Large threshold (1 GB):
    Checkpoints rarely. Batches more work.
    Crash recovery replays more. Memory buffer larger.

EXPLICIT:
  CHECKPOINT;                -- manual trigger
  PRAGMA force_checkpoint;   -- force even if WAL is small

ON SHUTDOWN:
  Automatic checkpoint on clean close.
  After shutdown: only .duckdb exists. No WAL file.

READ-ONLY (no writes at all):
  Querying Parquet files directly → no WAL → no checkpoint needed.
```

### Crash Recovery (WAL Replay)

```
CRASH BEFORE CHECKPOINT:
  .duckdb has old (consistent) data. .wal has recent changes.
  On restart:
    1. Open .duckdb (valid, just missing recent writes).
    2. Detect .wal → replay all committed transactions from it.
    3. Checkpoint → merge into main file → delete WAL.
    4. Ready. All committed data preserved.

CRASH DURING CHECKPOINT:
  Main file metadata NOT yet updated (old row group pointers still valid).
  Partially-written new blocks are orphaned (leaked space).
  On restart:
    1. Open .duckdb → metadata points to old row groups (safe).
    2. .wal still exists → replay it.
    3. Checkpoint again (properly this time).
    4. Orphaned blocks reclaimed.

CRASH AFTER CHECKPOINT:
  .duckdb is complete. .wal was deleted.
  On restart: open file, ready immediately. Nothing to replay.

Safety guarantee:
  .duckdb metadata ALWAYS points to valid, complete row groups.
  Never points to partially-written data.
  If .wal exists → replay it. If not → main file is authoritative.
```

### Checkpoint Cost

```
Checkpoint is not free, but is fast for typical DuckDB workloads:

  ┌──────────────────────────────────────────────────────────────┐
  │ Work                    │ Cost                                │
  ├─────────────────────────┼─────────────────────────────────────┤
  │ Compress buffered rows  │ CPU (lightweight encodings, fast)   │
  │ Write new row groups    │ Sequential I/O (SSD: ~1-3 GB/s)    │
  │ Rewrite deleted rows    │ Read + recompress + write (heavier) │
  │ fsync main file         │ ~1-10 ms per SSD flush             │
  │ Delete WAL              │ Negligible                          │
  └─────────────────────────┴─────────────────────────────────────┘

  16 MB WAL threshold (default):
    Checkpoint takes ~10-100 ms. Barely noticeable.

  Large buffer (100K+ rows accumulated):
    Checkpoint takes ~100 ms - 1 second.
    Single writer is busy during this time (no concurrent writes).
    Reads still work (MVCC — consistent snapshot).

  This is why frequent small checkpoints are preferred:
    Quick, minimal disruption, fast crash recovery.
```

### Checkpoint vs PostgreSQL

```
Same concept, different model:

  PostgreSQL:
    Dirty pages in shared_buffers → flush to data files.
    WAL recycled up to checkpoint position.
    WAL is a CONTINUOUS stream; checkpoint marks a position.
    Happens every 5 min or at WAL size threshold.

  DuckDB:
    Memory buffer + WAL → merge into .duckdb row groups.
    WAL DELETED entirely after checkpoint.
    WAL is a TEMPORARY file; checkpoint eliminates it.
    Happens every 16 MB or on shutdown.

  Key difference:
    PostgreSQL: WAL is permanent infrastructure (streaming replication uses it).
    DuckDB: WAL is a scratch file (no replication, embedded use only).
    DuckDB's model is simpler because it's single-writer + single-node.
```
