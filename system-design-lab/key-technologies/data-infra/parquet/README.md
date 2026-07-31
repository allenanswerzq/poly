# Apache Parquet Deep Dive

## Overview

Apache Parquet is the **standard columnar file format** for big data. If Arrow is how data lives in RAM, Parquet is how data lives on disk. It's designed for analytics: read only the columns you need, skip irrelevant row groups, and compress each column with the best algorithm for its type. Every data lake, every Spark job, every analytics pipeline uses Parquet.

## History & Why It Exists

```
The problem (2012):
  Hadoop ecosystem stored data as:
    - CSV/TSV: no types, no compression, terrible for analytics
    - JSON: parse-heavy, bloated, no columnar access
    - SequenceFile: row-oriented binary, must read entire rows
    - Avro: row-oriented, good for streaming but bad for analytics

  Analytics queries typically touch 5 columns out of 200.
  Row formats force you to read ALL 200 columns to get 5.
  At petabyte scale, this wastes 95% of I/O bandwidth.

  Twitter + Cloudera realized: we need a columnar FILE format
  (not just an in-memory format) designed for:
    - Reading only the columns you need
    - Skipping entire sections of data that don't match your filter
    - Compressing each column independently (same-type = better compression)
    - Working with Hadoop, Spark, Hive, Impala, etc.

  Inspired by Google's Dremel paper (2010) — the internal format
  behind BigQuery. Parquet is the open-source implementation of
  Dremel's columnar storage ideas.

Timeline:
  2012  Twitter + Cloudera start Parquet (inspired by Dremel)
  2013  Open-sourced, Apache incubation
  2015  Apache top-level project. Spark adopts as default format.
  2017  Parquet 2.0 (logical types, page-level CRC)
  2020  Industry standard — every data tool supports it
  2024  Parquet is THE format for data lakes (Delta Lake, Iceberg, Hudi)

Who uses it:
  Spark, Hive, Presto/Trino, DuckDB, Polars, pandas, BigQuery,
  Snowflake, Redshift, Athena, Databricks, dbt, Delta Lake, Iceberg
```

---

## 2. File Structure — Row Groups, Column Chunks, Pages

```
A Parquet file is organized hierarchically:

  ┌──────────────────────────────────────────────────────────────────┐
  │                     Parquet File                                   │
  │                                                                    │
  │  ┌────────────────────────────────────────────────────────────┐  │
  │  │  ROW GROUP 0  (typically 128 MB of uncompressed data)       │  │
  │  │                                                             │  │
  │  │  ┌────────────────┐ ┌────────────────┐ ┌────────────────┐ │  │
  │  │  │ Column Chunk:  │ │ Column Chunk:  │ │ Column Chunk:  │ │  │
  │  │  │ "name"         │ │ "age"          │ │ "city"         │ │  │
  │  │  │                │ │                │ │                │ │  │
  │  │  │ ┌──────────┐  │ │ ┌──────────┐  │ │ ┌──────────┐  │ │  │
  │  │  │ │  Page 0  │  │ │ │  Page 0  │  │ │ │  Page 0  │  │ │  │
  │  │  │ │ (1MB)    │  │ │ │ (1MB)    │  │ │ │ (1MB)    │  │ │  │
  │  │  │ ├──────────┤  │ │ ├──────────┤  │ │ ├──────────┤  │ │  │
  │  │  │ │  Page 1  │  │ │ │  Page 1  │  │ │ │  Page 1  │  │ │  │
  │  │  │ │ (1MB)    │  │ │ │ (1MB)    │  │ │ │ (1MB)    │  │ │  │
  │  │  │ └──────────┘  │ │ └──────────┘  │ │ └──────────┘  │ │  │
  │  │  └────────────────┘ └────────────────┘ └────────────────┘ │  │
  │  └────────────────────────────────────────────────────────────┘  │
  │                                                                    │
  │  ┌────────────────────────────────────────────────────────────┐  │
  │  │  ROW GROUP 1                                                │  │
  │  │  (same structure, next batch of rows)                       │  │
  │  └────────────────────────────────────────────────────────────┘  │
  │                                                                    │
  │  ┌────────────────────────────────────────────────────────────┐  │
  │  │  ROW GROUP 2 ...                                            │  │
  │  └────────────────────────────────────────────────────────────┘  │
  │                                                                    │
  │  ┌────────────────────────────────────────────────────────────┐  │
  │  │  FOOTER (metadata)                                          │  │
  │  │    - Schema (column names, types, nested structure)         │  │
  │  │    - Row group metadata (offsets, sizes)                    │  │
  │  │    - Column chunk metadata (encodings, compression)         │  │
  │  │    - Statistics (min/max per column per row group)          │  │
  │  │    - Page index (min/max per page — for fine-grained skip) │  │
  │  └────────────────────────────────────────────────────────────┘  │
  │                                                                    │
  │  Magic: "PAR1" (4 bytes at start and end of file)                 │
  └──────────────────────────────────────────────────────────────────┘

Key terms:
  ROW GROUP:     a horizontal partition of rows (typically 128 MB).
                 The unit of parallelism — different row groups can
                 be processed by different threads/machines.

  COLUMN CHUNK:  all values of ONE column within one row group.
                 The unit of I/O — read only the columns you need.

  PAGE:          the smallest unit of encoding/compression (~1 MB).
                 Within a column chunk, data is divided into pages.
                 Each page is independently compressed and decodable.
```

---

## 3. Why Columnar Storage Is Fast for Analytics

```
Query: SELECT AVG(age) FROM users WHERE city = 'NYC'

ROW FORMAT (CSV, Avro):
  ┌──────────────────────────────────────────┐
  │ Row 0: [name="Alice", age=25, city="NYC",│  Must read
  │         email="...", bio="...", addr=".."]│  ALL columns
  │ Row 1: [name="Bob", age=30, city="SF",..]│  just to get
  │ Row 2: [name="Carol", age=28, city="NYC"]│  "age" and "city"
  │ ...                                       │
  └──────────────────────────────────────────┘
  Read: 200 columns × 1M rows = 200M cells. Used: 2 columns. Wasted: 99%.

PARQUET (columnar):
  Read only "age" column chunk + "city" column chunk.
  Skip "name", "email", "bio", "addr" entirely.
  Read: 2 columns × 1M rows = 2M cells. Wasted: 0%.

  ┌─────────────────────────────────────────────────────────────┐
  │  With 100 columns and you need 3:                           │
  │    Row format: read 100% of data, use 3%                    │
  │    Parquet:    read 3% of data, use 100% of what you read   │
  │    → 33x less I/O. At petabyte scale, this is HOURS saved.  │
  └─────────────────────────────────────────────────────────────┘
```

---

## 4. Predicate Pushdown & Statistics — Skipping Data Without Reading It

```
Parquet stores MIN/MAX statistics for each column in each row group.
Query engines use these to SKIP entire row groups without reading them.

  Query: SELECT * FROM orders WHERE amount > 1000

  Row Group 0: amount min=5, max=500       ← SKIP (no values > 1000)
  Row Group 1: amount min=200, max=3000    ← READ (might have matches)
  Row Group 2: amount min=1, max=100       ← SKIP
  Row Group 3: amount min=800, max=5000    ← READ

  Result: only read 2 of 4 row groups = 50% less I/O.

  Page-level statistics (Parquet 2.0+):
    Same min/max but per PAGE within a column chunk.
    Even finer granularity — skip individual 1MB pages.
    "This page has ages 20-30, but I need age > 50 → skip this page."

  This is why:
    - Sorting your data before writing to Parquet helps MASSIVELY.
      Sorted by "date" → all dates in a row group are similar
      → min/max are tight → more row groups can be skipped.
    - Random/unordered data → min=1, max=999999 in every row group
      → statistics are useless → can't skip anything.

  ┌──────────────────────────────────────────────────────────────┐
  │  PRO TIP: Always sort/partition Parquet files by your         │
  │  most common filter column (usually date or partition key).  │
  │  This makes predicate pushdown effective.                    │
  └──────────────────────────────────────────────────────────────┘
```

---

## 5. Encoding — Why Parquet Files Are Small

```
Each column is encoded independently using the BEST encoding for its type.

┌──────────────────┬────────────────────────────────────────────────────┐
│ Encoding         │ How it works                                       │
├──────────────────┼────────────────────────────────────────────────────┤
│ PLAIN            │ Raw values. No encoding. Baseline.                 │
│                  │                                                     │
│ DICTIONARY       │ Build a dictionary of unique values.               │
│                  │ Store indices instead of values.                   │
│                  │ "NYC","SF","NYC","NYC" → dict={0:NYC,1:SF}        │
│                  │ → stored as [0, 1, 0, 0] (much smaller!)          │
│                  │ Great for: low-cardinality strings (status, city)  │
│                  │                                                     │
│ RLE (Run-Length) │ Consecutive repeated values compressed.            │
│                  │ [1,1,1,1,1,2,2,2] → [(1,5),(2,3)]               │
│                  │ Great for: sorted columns, boolean flags           │
│                  │                                                     │
│ DELTA            │ Store differences between consecutive values.      │
│                  │ [100, 102, 105, 103] → [100, +2, +3, -2]         │
│                  │ Great for: timestamps, sequential IDs              │
│                  │                                                     │
│ BIT-PACKING     │ Use minimum bits needed for the range.             │
│                  │ Values 0-7 need only 3 bits (not 32).             │
│                  │ Pack 10 values into 30 bits instead of 320.       │
│                  │ Great for: dictionary indices, small integers       │
└──────────────────┴────────────────────────────────────────────────────┘

After encoding, COMPRESSION is applied per page:

  ┌───────────────┬──────────────────────────────────────────────────┐
  │ Compression   │ Tradeoff                                         │
  ├───────────────┼──────────────────────────────────────────────────┤
  │ Snappy        │ Fast compression/decompression. Moderate ratio.  │
  │ (default)     │ Used when read speed matters more than size.     │
  │               │                                                   │
  │ Zstd          │ Better ratio than Snappy, still fast.            │
  │               │ Increasingly used as the modern default.         │
  │               │                                                   │
  │ Gzip          │ Best ratio, slowest. Used for cold storage.      │
  │               │                                                   │
  │ LZ4           │ Fastest decompress. Used for hot/frequently-read │
  │               │ data where read speed is critical.               │
  │               │                                                   │
  │ None          │ No compression. Used with Arrow IPC (already fast)│
  └───────────────┴──────────────────────────────────────────────────┘

Combined effect:
  Raw CSV: 100 GB
  Parquet (dictionary + RLE + Snappy): ~10-15 GB (7-10x smaller)
  Parquet (dictionary + Zstd level 3): ~5-10 GB (10-20x smaller)
```

---

## 6. Nested Data — Dremel Encoding (Repetition & Definition Levels)

```
Parquet supports nested/repeated fields (like JSON objects/arrays).
This comes from Google's Dremel paper.

  Schema:
    message Document {
      required string name;
      repeated group links {
        optional string url;
        optional string text;
      }
    }

  Data:
    {"name": "doc1", "links": [{"url": "a.com", "text": "A"},
                               {"url": "b.com"}]}
    {"name": "doc2", "links": []}
    {"name": "doc3", "links": [{"url": "c.com", "text": "C"}]}

  Parquet stores this FLAT (no nested objects in memory):
    Column "links.url":  ["a.com", "b.com", null, "c.com"]
    With REPETITION LEVELS:  [0, 1, 0, 0]
      0 = new record, 1 = repeated within same record

    With DEFINITION LEVELS: [2, 2, 0, 2]
      Tracks which level of nesting is defined (vs null/empty).

  This allows nested data to be stored COLUMNAR and efficiently compressed,
  while preserving the full structure. No JSON overhead.
```

---

## 7. Reading Parquet — What Actually Happens

```
Query: SELECT name, age FROM users WHERE city = 'NYC' LIMIT 100

Step 1: Read FOOTER (last bytes of file)
  Contains: schema, row group metadata, column offsets, statistics.
  Just one small read (~KB) to know the file structure.

Step 2: Check row group statistics
  Row group 0: city min="Boston", max="NYC"  → might match, read
  Row group 1: city min="Portland", max="SF" → SKIP (no NYC possible)
  Row group 2: city min="LA", max="NYC"      → might match, read

Step 3: Read ONLY needed column chunks from matching row groups
  From row group 0: read "name" chunk, "age" chunk, "city" chunk
  From row group 2: same
  DON'T read: "email", "bio", "address" columns. Not needed.

Step 4: Decompress + decode each page
  Snappy decompress → dictionary decode → raw values
  Apply filter: city == 'NYC' → bitmap of matching rows
  Project: return only name + age for matching rows

Step 5: Return results (as Arrow RecordBatch in modern engines)

  ┌──────────────────────────────────────────────────────────────┐
  │  I/O pattern:                                                 │
  │    1 footer read (seek to end of file)                       │
  │    + N column chunk reads (seek to specific offsets)          │
  │                                                               │
  │  This is why Parquet works well on cloud storage (S3):       │
  │    S3 supports byte-range requests.                          │
  │    Read footer → know offsets → request ONLY needed chunks.  │
  │    Don't download the whole file. Pay for what you read.     │
  └──────────────────────────────────────────────────────────────┘
```

---

## 8. Writing Parquet — Best Practices

```
How data gets written:

  Writer accumulates rows until ROW GROUP is full (128 MB default):
    For each column in the row group:
      Encode values (dictionary, RLE, delta, etc.)
      Split into pages (~1 MB each)
      Compress each page
      Write column chunk to file
    Write row group metadata

  After all row groups: write FOOTER at end of file.

Best practices:

  1. ROW GROUP SIZE: 128 MB (default) is usually good.
     Smaller (64 MB): better for small queries (less to skip).
     Larger (256 MB): better for full-table scans (less overhead).

  2. SORT BEFORE WRITING:
     Sort by your most-filtered column (date, region, user_id).
     This makes statistics tight → predicate pushdown works.
     Unsorted: min=1, max=1B in every row group → useless stats.
     Sorted by date: each row group covers one day → easy to skip.

  3. PARTITION:
     For data lakes: partition files by date/region.
     /data/year=2024/month=07/part-00001.parquet
     Query for July → only read July's directory. Skip other months.

  4. COMPRESSION: use Zstd (best ratio + speed tradeoff for 2024+).
     Legacy: Snappy. Still fine but Zstd is better in every way now.

  5. FILE SIZE: target 128 MB - 1 GB per file.
     Too small (1 MB): too many files → metadata overhead, S3 listing slow.
     Too large (10 GB): one file can't be split for parallelism easily.
```

---

## 9. Parquet in the Data Lake Stack

```
Parquet is the storage layer. Table formats add transactions on top:

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                   │
  │  Query Engines (read/write Parquet):                             │
  │    Spark, Trino/Presto, DuckDB, Athena, BigQuery, Polars        │
  │                                                                   │
  │  Table Formats (add ACID, schema evolution, time-travel):        │
  │    Delta Lake, Apache Iceberg, Apache Hudi                       │
  │    These manage COLLECTIONS of Parquet files +                   │
  │    metadata/transaction logs on top.                              │
  │                                                                   │
  │  Storage:                                                         │
  │    S3, GCS, HDFS, Azure Blob (Parquet files live here)           │
  │                                                                   │
  └──────────────────────────────────────────────────────────────────┘

  Delta Lake / Iceberg add:
    - ACID transactions (concurrent writes don't corrupt)
    - Time travel (query data as of yesterday)
    - Schema evolution (add/rename columns safely)
    - Compaction (merge small Parquet files into big ones)
    - Partition evolution (change partitioning without rewriting)

  Under the hood: they're just managing Parquet files + a metadata log.
```

---

## 10. Parquet vs Other Formats

```
┌──────────────┬──────────────┬──────────────┬──────────────┬───────────┐
│              │ Parquet      │ ORC          │ Avro         │ CSV       │
├──────────────┼──────────────┼──────────────┼──────────────┼───────────┤
│ Layout       │ Columnar     │ Columnar     │ Row          │ Row (text)│
│ Compression  │ Snappy/Zstd  │ Zlib/Snappy  │ Snappy/null  │ None      │
│ Schema       │ Strong typed │ Strong typed │ Strong (JSON)│ None      │
│ Nested data  │ Yes (Dremel) │ Yes          │ Yes          │ No        │
│ Analytics    │ Excellent    │ Excellent    │ OK           │ Terrible  │
│ Streaming    │ Bad (batch)  │ Bad          │ Good         │ Good      │
│ Ecosystem    │ Everything   │ Hive-centric │ Kafka, Spark │ Everything│
│ Size (1 GB raw)│ ~100-200 MB │ ~100-200 MB │ ~500 MB      │ ~1 GB    │
│ Winner?      │ ✓ Analytics  │ Hive legacy  │ Streaming    │ Import    │
└──────────────┴──────────────┴──────────────┴──────────────┴───────────┘

When to use what:
  Analytics/data lake → Parquet (always)
  Kafka messages     → Avro (schema registry, row-oriented, streaming)
  Hive legacy        → ORC (fine, but Parquet has wider adoption)
  Quick import/export→ CSV (human-readable, universal, slow)
  In-memory processing → Arrow (read Parquet → Arrow in memory)
```

---

## 11. Key Numbers

```
Compression ratio (vs CSV):      5-20x smaller
Column pruning savings:          Up to 100x less I/O (100 cols, need 1)
Row group skip (predicate):      Varies, often 50-90% of data skipped
Read throughput (local SSD):     2-5 GB/s per thread (Zstd, modern CPU)
Read throughput (S3):            Limited by network (~1 Gbps per stream)
Write throughput:                500 MB - 2 GB/s (depends on compression)
Footer size:                     ~KB (schema + statistics)
Page size:                       ~1 MB (default)
Row group size:                  128 MB (default, configurable)
Max nested depth:                Unlimited (Dremel encoding)
Supported types:                 int32/64, float/double, bool, binary,
                                 string, decimal, date, timestamp, list,
                                 map, struct (all SQL types covered)
```
