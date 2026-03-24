# SQLite Deep Dive

## Overview

SQLite is an **embedded database** — the entire database is a single file. No server, no configuration, no network. It's the most deployed database in the world (every phone, every browser, every desktop app). In system design, know it for edge/embedded scenarios and as a learning tool for how SQL databases work internally.

## When to Choose SQLite

| Use Case | Why SQLite |
|----------|----------|
| Mobile apps | No server needed, just a file |
| Desktop apps | Electron, VS Code settings, browsers |
| Edge/IoT devices | Tiny footprint (~600KB) |
| Testing | In-memory mode, no setup |
| Small websites | <100K requests/day is fine |
| Data files | Replace CSV/JSON with queryable format |

## Architecture (Simplicity Is the Feature)

```
PostgreSQL:                          SQLite:
  Client ──network──► Server          Application
  Server ──process──► Storage           │
  Connection pool                       │ direct function call
  WAL management                        │ (no network, no IPC)
  Vacuum daemon                         ▼
  Replication                        ┌──────────┐
  Auth & permissions                 │ SQLite    │
  ...much complexity                 │ Library   │
                                     └────┬─────┘
                                          │
                                     ┌────▼─────┐
                                     │ data.db  │ ← single file
                                     └──────────┘
```

## Key Concepts for Interviews

### 1. When SQLite Actually Scales
```
Litestream: replicate SQLite to S3 (disaster recovery)
LiteFS:     distributed SQLite at the edge (Fly.io)
Turso:      libSQL — SQLite fork with embedded replicas

Modern trend: run SQLite per-user or per-tenant at the edge.
  1000 users → 1000 SQLite databases
  Each user's data on the closest edge node
  No shared database bottleneck
```

### 2. WAL Mode (Write-Ahead Logging)
```sql
PRAGMA journal_mode=WAL;  -- enable WAL mode

Default mode: readers block writers
WAL mode:     readers and writers can work concurrently!

WAL = append changes to a log file → readers see snapshot
Same concept as PostgreSQL's WAL, just simpler.
```

### 3. Limitations That Matter
```
Concurrency:  1 writer at a time (readers are concurrent)
Network:      No client-server (file must be local)
Size:         Practical limit ~1TB (works but gets slow)
Auth:         No users/permissions (file permissions only)
Replication:  None built-in (use Litestream/LiteFS)
```

### 4. SQLite as a Learning Tool
SQLite implements the same concepts as PostgreSQL:
- B-tree indexes
- Query planner (EXPLAIN QUERY PLAN)
- Transactions (BEGIN, COMMIT, ROLLBACK)
- WAL for crash recovery
- VACUUM for space reclamation

Understanding SQLite = understanding 80% of how any SQL database works.

## SQLite vs Server Databases

| Aspect | SQLite | PostgreSQL |
|--------|--------|-----------|
| Deployment | Single file | Server process |
| Concurrency | 1 writer | Many writers |
| Network | Local only | Client-server |
| Setup | Zero config | Install + configure |
| Size | ~600KB | ~100MB |
| Best for | Embedded, edge | Shared, multi-user |

## Interview Sound Bite

> "For the mobile app's local data, I'd use SQLite — it gives us real SQL queries without running a server. For the edge caching layer, we could run SQLite-per-user with Turso for replication, so each user's data is on the nearest edge node with sub-millisecond reads."
