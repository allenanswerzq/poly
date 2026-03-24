# PostgreSQL Deep Dive

## Overview

PostgreSQL is the **default choice for SQL databases** in system design. When the interviewer hears "relational database," they're likely thinking PostgreSQL. It's ACID-compliant, extensible, and handles complex queries well.

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
- Readers never block writers, writers never block readers
- Each transaction sees a snapshot of the database
- Dead tuples accumulate → need VACUUM to reclaim space

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
