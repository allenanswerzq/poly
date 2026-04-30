# PostgreSQL Deep Dive

## Overview

PostgreSQL is the **default choice for SQL databases** in system design. When the interviewer hears "relational database," they're likely thinking PostgreSQL. It's ACID-compliant, extensible, and handles complex queries well.

## History & Why It Exists

```
The problem (1986):
  Michael Stonebraker at UC Berkeley wanted to build a database that
  could handle COMPLEX DATA TYPES — not just integers and strings,
  but geographic data, arrays, custom types. Relational databases
  at the time (Ingres, Oracle) were rigid.

  He built POSTGRES (Post-Ingres) as a research project.
  "What if a database was EXTENSIBLE? Users define new types,
  operators, index methods, and the database treats them as first-class."

Timeline:
  1986  Michael Stonebraker starts POSTGRES at UC Berkeley
  1995  Postgres95 — SQL support added (replacing original QUEL language)
  1996  Renamed to PostgreSQL. Community-driven development begins.
  2005  PostgreSQL 8.0 (Windows support, savepoints, tablespaces)
  2010  PostgreSQL 9.0 (streaming replication — finally built-in!)
  2012  PostgreSQL 9.2 (JSON support)
  2016  PostgreSQL 10 (logical replication, partitioning, parallelism)
  2020  PostgreSQL 13 (incremental sort, deduplication)
  2023  PostgreSQL 16 (logical replication improvements, parallelism)
  2024  PostgreSQL 17 (incremental backup, new JSON functions)

Why PostgreSQL won:
  1. EXTENSIBILITY: PostGIS (geography), pgvector (embeddings),
     pg_trgm (fuzzy search), TimescaleDB (time-series).
     You can extend PostgreSQL for ALMOST any use case.
  2. CORRECTNESS: strictest SQL compliance. MVCC done right.
  3. OPEN SOURCE: no corporate owner. BSD license. Truly community-driven.
  4. "Use Postgres until it hurts, then keep using Postgres."

PostgreSQL vs MySQL:
  MySQL: simpler, faster for simple reads, easier replication
  PostgreSQL: more features, stricter correctness, better for complex queries
  2010s: PostgreSQL overtook MySQL in new projects.
  Today: PostgreSQL is the default recommendation for any new SQL workload.

Who uses it:
  Apple, Instagram (largest PG deployment), Reddit, Spotify,
  Twitch, UK Government, Supabase (PG as a service).
```

## When to Choose PostgreSQL

| Use Case | Why PostgreSQL |
|----------|---------------|
| User accounts & profiles | Structured data, relationships, ACID |
| E-commerce orders | Transactions (payment + inventory must be atomic) |
| Complex queries with JOINs | Powerful query planner |
| Geospatial data | PostGIS extension |
| Full-text search (moderate) | Built-in tsvector/tsquery |
| JSON + relational hybrid | JSONB column type |

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    PostgreSQL Architecture                       │
│                                                                  │
│  Client ──► Postmaster ──► Backend Process (1 per connection)   │
│                                                                  │
│  ┌──────────────────┐  ┌──────────────────┐                    │
│  │  Shared Buffers   │  │  WAL Buffers      │                    │
│  │  (page cache)     │  │  (write-ahead log) │                    │
│  └────────┬─────────┘  └────────┬─────────┘                    │
│           │                      │                               │
│           ▼                      ▼                               │
│  ┌──────────────────┐  ┌──────────────────┐                    │
│  │  Data Files       │  │  WAL Files        │                    │
│  │  (heap + index)   │  │  (crash recovery)  │                    │
│  └──────────────────┘  └──────────────────┘                    │
└─────────────────────────────────────────────────────────────────┘
```

## Key Concepts for Interviews

### 1. MVCC (Multi-Version Concurrency Control)

MVCC is the most important concurrency concept in databases. Without it,
databases have to choose between: (a) readers block writers, or (b) readers
see partially-written data. MVCC gives you neither problem.

**The core idea**: instead of updating a row in place, create a NEW VERSION
of the row. Old transactions keep reading the old version. New transactions
see the new version. Nobody blocks anybody.

```
THE PROBLEM MVCC SOLVES:

  Without MVCC (locking-based):

    Transaction A: SELECT * FROM accounts WHERE id = 1;  -- reads balance=1000
    Transaction B: UPDATE accounts SET balance = 500 WHERE id = 1;

    Two options, both bad:
      Option 1: B waits for A to finish (writer blocks on reader)
        → Terrible throughput. Reads are common, everything waits.

      Option 2: A sees B's half-written data (dirty read)
        → A reads balance=500 before B commits. B rolls back.
        → A made a decision based on data that NEVER existed. Corrupt.

  With MVCC:

    Transaction A (started at time T=10) reads balance → sees version at T=10 → 1000
    Transaction B (started at time T=12) updates balance to 500 → creates NEW version
    Transaction A reads balance AGAIN → still sees 1000 (its snapshot is T=10)
    Transaction B commits.
    Transaction C (starts at T=15) reads balance → sees 500 (new snapshot)

    A never blocked. B never waited. Nobody saw inconsistent data.
```

**How PostgreSQL implements MVCC — tuple versioning:**

```
Every row (tuple) in PostgreSQL has hidden system columns:

  ┌──────────────────────────────────────────────────────────────┐
  │ Row in the heap (data file):                                 │
  │                                                              │
  │  xmin  │  xmax  │  id  │  name   │  balance                │
  │  100   │  0     │  1   │ "alice" │  1000                    │
  │                                                              │
  │  xmin = transaction ID that CREATED this row version         │
  │  xmax = transaction ID that DELETED/UPDATED this version     │
  │         (0 means "still alive")                              │
  └──────────────────────────────────────────────────────────────┘

  Now Transaction 200 runs:  UPDATE accounts SET balance = 500 WHERE id = 1;

  PostgreSQL does NOT modify the existing row. Instead:

  1. Mark the OLD row as "deleted by txn 200" (set xmax = 200)
  2. Insert a NEW row with the updated data

  Heap now contains TWO physical rows for the same logical row:

  ┌──────────────────────────────────────────────────────────────┐
  │  xmin  │  xmax  │  id  │  name   │  balance                │
  │  100   │  200   │  1   │ "alice" │  1000     ← old version │
  │  200   │  0     │  1   │ "alice" │  500      ← new version │
  └──────────────────────────────────────────────────────────────┘

  Transaction 150 (started BEFORE txn 200):
    Sees xmin=100 (committed, visible) and xmax=200 (not committed yet from
    150's perspective) → reads balance = 1000. Correct.

  Transaction 250 (started AFTER txn 200 committed):
    Sees old row: xmax=200 (committed → this version is dead, skip it)
    Sees new row: xmin=200 (committed → visible) → reads balance = 500. Correct.

  Both transactions see consistent data. Neither blocked.
```

**Visibility rules — how a transaction decides which row version to see:**

```
  For a row version to be VISIBLE to transaction T:

    1. xmin must be committed AND xmin < T's snapshot
       (the row was created by a transaction that committed before my snapshot)

    2. xmax must be EITHER:
       a. 0 (row not deleted yet), OR
       b. NOT committed (deleting transaction rolled back), OR
       c. Committed but xmax > T's snapshot (deleted AFTER my snapshot)

  In pseudocode:
    visible = (xmin is committed AND xmin < my_snapshot)
              AND
              (xmax == 0 OR xmax is not committed OR xmax > my_snapshot)

  PostgreSQL tracks which transactions are committed vs. aborted in a
  structure called CLOG (commit log) — a bitmap where each txn ID maps
  to: committed, aborted, or in-progress.
```

**The cost of MVCC — dead tuples and VACUUM:**

```
  Every UPDATE creates a new row AND leaves the old row on disk.
  Every DELETE marks a row as dead but doesn't remove it.

  These dead rows are called "dead tuples." They:
    - Waste disk space
    - Slow down sequential scans (must skip over dead rows)
    - Bloat indexes (index still points to dead rows)

  VACUUM is the cleanup process:
    1. Scans the table for dead tuples (xmax is committed, no active
       transaction needs them anymore)
    2. Marks that space as reusable for future inserts
    3. Does NOT return space to the OS (the file stays the same size)

  VACUUM FULL: rewrites the entire table, DOES return space to OS.
    But it locks the table exclusively — nobody can read or write.
    Only use for emergency space recovery.

  Autovacuum: background daemon that runs VACUUM automatically.
    Triggers when dead tuples exceed a threshold.
    NEVER DISABLE AUTOVACUUM. If you do, the table bloats until
    queries slow to a crawl and disk fills up. This is the #1
    PostgreSQL operational mistake.

  ┌──────────────────────────────────────────────────────────────┐
  │  Timeline of a row's life:                                   │
  │                                                              │
  │  INSERT (txn 100)         → row created, xmin=100, xmax=0  │
  │  UPDATE (txn 200)         → old: xmax=200, new: xmin=200   │
  │  All txns > 200 committed → old row is DEAD (no one needs it)│
  │  VACUUM runs              → marks old row's space as free    │
  │  Next INSERT              → reuses that space                │
  └──────────────────────────────────────────────────────────────┘
```

**How other databases do MVCC differently:**

```
┌──────────────────┬──────────────────────────────────────────────┐
│ PostgreSQL       │ Store old and new versions IN THE SAME TABLE │
│                  │ (heap). Old versions = dead tuples. Need     │
│                  │ VACUUM to clean up.                          │
│                  │ Pros: simple, append-friendly.               │
│                  │ Cons: table bloat, VACUUM overhead.          │
├──────────────────┼──────────────────────────────────────────────┤
│ MySQL/InnoDB     │ Store current version in the B-tree (primary │
│                  │ key = clustered index). Old versions go to   │
│                  │ a separate UNDO LOG (rollback segment).      │
│                  │ Pros: table stays compact, no vacuum needed. │
│                  │ Cons: long transactions → huge undo log.     │
│                  │ Reads of old versions chase undo chain.      │
├──────────────────┼──────────────────────────────────────────────┤
│ Oracle           │ Same as MySQL — undo segments.               │
│                  │ "ORA-01555: snapshot too old" = undo was     │
│                  │ reclaimed before your long txn could read it.│
├──────────────────┼──────────────────────────────────────────────┤
│ etcd             │ Append-only revisions. Every write creates   │
│                  │ a new revision number. Compaction removes     │
│                  │ old revisions. Simpler (no concurrent txns). │
├──────────────────┼──────────────────────────────────────────────┤
│ FoundationDB     │ Storage servers keep multiple versions       │
│                  │ per key. Garbage collected when no active    │
│                  │ transaction has a read version that old.     │
│                  │ Read version from Sequencer = snapshot.      │
├──────────────────┼──────────────────────────────────────────────┤
│ CockroachDB      │ MVCC timestamps on each key-value pair      │
│                  │ in the LSM-tree (Pebble). GC removes old    │
│                  │ versions past a configurable threshold.      │
└──────────────────┴──────────────────────────────────────────────┘
```

**Isolation levels and MVCC — what snapshot do you see?**

```
  READ COMMITTED (PostgreSQL default):
    Each STATEMENT gets a FRESH snapshot.
    Within one transaction, two SELECTs can see different data
    if another transaction committed between them.

      BEGIN;
      SELECT balance FROM accounts WHERE id = 1;  -- snapshot at T=10 → 1000
      -- another txn commits: balance = 500
      SELECT balance FROM accounts WHERE id = 1;  -- snapshot at T=12 → 500 (!)
      COMMIT;

    This is fine for most applications. Each statement is consistent
    within itself.

  REPEATABLE READ (SNAPSHOT ISOLATION):
    The entire TRANSACTION gets ONE snapshot (at the first statement).
    All reads within the transaction see the same data, regardless
    of other commits.

      BEGIN ISOLATION LEVEL REPEATABLE READ;
      SELECT balance FROM accounts WHERE id = 1;  -- snapshot at T=10 → 1000
      -- another txn commits: balance = 500
      SELECT balance FROM accounts WHERE id = 1;  -- SAME snapshot → still 1000
      COMMIT;

  SERIALIZABLE:
    Same as REPEATABLE READ, plus PostgreSQL detects "serialization
    anomalies" — situations where the outcome would differ from
    some serial execution order. If detected, one transaction is aborted.

    This is the STRONGEST level. Used when correctness matters more
    than retry cost (financial systems, inventory).

  ┌──────────────────────────────────────────────────────────────┐
  │  Level              │ Snapshot          │ Anomalies blocked  │
  ├─────────────────────┼───────────────────┼────────────────────┤
  │  READ COMMITTED     │ Per statement     │ Dirty reads        │
  │  REPEATABLE READ    │ Per transaction   │ + Non-repeatable   │
  │                     │                   │   reads, phantom   │
  │  SERIALIZABLE       │ Per transaction   │ + All anomalies    │
  │                     │ + dependency check│                    │
  └──────────────────────────────────────────────────────────────┘
```

### 2. WAL (Write-Ahead Logging)
- Every change is written to WAL before data files
- Crash recovery: replay WAL to recover committed transactions
- Streaming replication: send WAL to replicas

### 3. Index Types
| Index Type | Use Case |
|-----------|----------|
| B-tree (default) | Equality, range, sorting |
| Hash | Equality only (rare) |
| GIN | Full-text search, JSONB, arrays |
| GiST | Geospatial, range types |
| BRIN | Very large tables with natural ordering (time-series) |

### 4. Connection Pooling
- PostgreSQL forks a process per connection (~10MB each)
- At 10K connections: 100GB RAM just for connections
- Solution: PgBouncer or built-in connection pooling (Supavisor)
- Typical: 100 connections with pooler vs 10K direct connections

### 5. Partitioning
```sql
-- Partition large tables by range (e.g., date)
CREATE TABLE events (
    id BIGSERIAL,
    created_at TIMESTAMP,
    data JSONB
) PARTITION BY RANGE (created_at);

CREATE TABLE events_2024 PARTITION OF events
    FOR VALUES FROM ('2024-01-01') TO ('2025-01-01');
```

### 6. JSONB — Best of Both Worlds
```sql
-- Store flexible data alongside structured data
CREATE TABLE products (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL,
    price DECIMAL NOT NULL,
    metadata JSONB  -- flexible attributes per product
);

-- Query into JSON
SELECT * FROM products WHERE metadata->>'color' = 'blue';
CREATE INDEX idx_color ON products USING GIN (metadata);
```

## Scaling PostgreSQL

```
Read scaling:
  Primary ──► Replica 1 (reads)
          ──► Replica 2 (reads)
          ──► Replica 3 (reads)

Write scaling (when single primary isn't enough):
  Option 1: Vertical scaling (bigger machine)
  Option 2: Partitioning (split table by range/hash)
  Option 3: Citus extension (distributed PostgreSQL)
  Option 4: Application-level sharding
```

## Limitations to Mention

- Single-node write bottleneck (no native multi-master)
- Connection overhead (use pooler)
- VACUUM overhead for write-heavy workloads
- Not ideal for: time-series at massive scale, graph traversals, simple key-value lookups

## Interview Sound Bite

> "I'd use PostgreSQL here because we need ACID transactions and complex queries with JOINs. For the read-heavy access pattern, we'd add read replicas. For the flexible product attributes, we can use JSONB columns so we get the best of SQL and document-store models."
