# Redis Deep Dive

## Overview

Redis (Remote Dictionary Server) is an in-memory data structure store used as cache, database, message broker, and session store. Understanding Redis internals is essential for staff+ level interviews.

## What You Must Master

| Concept | Why It Matters |
|---------|----------------|
| Data structures | Choose right structure for use case |
| Persistence (RDB/AOF) | Understand durability trade-offs |
| Replication | Master-replica for HA |
| Cluster mode | Sharding for scale |
| Pub/Sub | Real-time messaging |
| Lua scripts | Atomic operations |

## Architecture Diagram

```mermaid
graph TB
    subgraph "Redis Architecture"
        subgraph "Single Instance"
            CLIENT[Clients] --> REDIS[Redis Server]
            REDIS --> MEM[(In-Memory<br/>Data)]
            REDIS --> |"Async"| DISK[(RDB/AOF<br/>Persistence)]
        end

        subgraph "Replication"
            M[Master] --> |"Async Replication"| R1[Replica 1]
            M --> |"Async Replication"| R2[Replica 2]
        end

        subgraph "Cluster Mode"
            S1[Shard 1<br/>slots 0-5460]
            S2[Shard 2<br/>slots 5461-10922]
            S3[Shard 3<br/>slots 10923-16383]
        end
    end
```

## Why Redis is Fast

```
┌─────────────────────────────────────────────────────────────────┐
│                    Why Redis is Fast                             │
│                                                                  │
│  1. In-Memory Storage                                           │
│     └── No disk I/O for reads                                   │
│                                                                  │
│  2. Single-Threaded Event Loop                                  │
│     └── No lock contention, context switches                    │
│                                                                  │
│  3. Optimized Data Structures                                   │
│     └── Ziplist, intset, quicklist for small data              │
│                                                                  │
│  4. IO Multiplexing (epoll/kqueue)                             │
│     └── Handle thousands of connections efficiently             │
│                                                                  │
│  Typical: 100,000+ operations/second on single node            │
└─────────────────────────────────────────────────────────────────┘
```

## Data Structures

### 1. Strings
```redis
SET user:1:name "Alice"
GET user:1:name           # "Alice"
INCR page:views           # Atomic increment
SETEX session:abc 3600 "data"  # Expires in 1 hour
```

### 2. Hashes
```redis
HSET user:1 name "Alice" age 30 city "NYC"
HGET user:1 name          # "Alice"
HGETALL user:1            # name, Alice, age, 30, city, NYC
```

### 3. Lists (Linked List)
```redis
RPUSH queue:jobs "job1" "job2"  # Push right
LPOP queue:jobs                  # Pop left (FIFO queue)
BRPOP queue:jobs 0               # Blocking pop (for workers)
```

### 4. Sets
```redis
SADD user:1:friends 2 3 4
SADD user:2:friends 3 4 5
SINTER user:1:friends user:2:friends  # {3, 4}
```

### 5. Sorted Sets
```redis
ZADD leaderboard 100 "alice" 200 "bob" 150 "charlie"
ZRANGE leaderboard 0 -1 WITHSCORES   # alice, bob, charlie
ZRANK leaderboard "bob"              # 2 (0-indexed)
```

## Use Cases

### 1. Caching
```
┌─────────────────────────────────────────────────────────────────┐
│  App → Check Redis → Cache Hit? → Return                        │
│                    → Cache Miss → Query DB → Store in Redis     │
│                                                                  │
│  SET user:123 "{json}" EX 3600   # Cache for 1 hour            │
└─────────────────────────────────────────────────────────────────┘
```

### 2. Rate Limiting
```redis
# Sliding window rate limiter
INCR requests:user:123:minute
EXPIRE requests:user:123:minute 60
# If > limit, reject
```

### 3. Session Storage
```redis
SET session:abc123 "{user_id: 1, ...}" EX 86400
```

### 4. Pub/Sub
```redis
SUBSCRIBE channel:news
PUBLISH channel:news "Breaking news!"
```

### 5. Distributed Lock
```redis
SET lock:resource1 "owner1" NX PX 30000
# NX = only if not exists
# PX = expire in 30 seconds
```

### 6. Leaderboards
```redis
ZADD game:scores 1000 "player1" 850 "player2"
ZREVRANGE game:scores 0 9  # Top 10 players
```

## Persistence Options

| Mode | Description | Trade-offs |
|------|-------------|------------|
| **RDB** | Point-in-time snapshots | Fast restart, may lose recent data |
| **AOF** | Append log of every write | More durable, larger files |
| **Both** | Combined approach | Best durability |

## Redis Cluster

```
┌─────────────────────────────────────────────────────────────────┐
│                      Redis Cluster                               │
│                                                                  │
│  Hash Slots: 0-16383 distributed across masters                 │
│                                                                  │
│  ┌─────────┐     ┌─────────┐     ┌─────────┐                   │
│  │Master 1 │     │Master 2 │     │Master 3 │                   │
│  │Slots    │     │Slots    │     │Slots    │                   │
│  │0-5460   │     │5461-10922│    │10923-16383│                 │
│  └────┬────┘     └────┬────┘     └────┬────┘                   │
│       │              │              │                           │
│       ▼              ▼              ▼                           │
│  ┌─────────┐     ┌─────────┐     ┌─────────┐                   │
│  │Replica 1│     │Replica 2│     │Replica 3│                   │
│  └─────────┘     └─────────┘     └─────────┘                   │
│                                                                  │
│  Key → CRC16(key) % 16384 → slot → master                       │
└─────────────────────────────────────────────────────────────────┘
```

## Implementation

Our mini Redis demonstrates:
1. String commands (GET, SET, DEL)
2. Hash commands (HSET, HGET, HGETALL)
3. List commands (LPUSH, RPUSH, LPOP, RPOP)
4. TTL/Expiration
5. Basic pub/sub

Run the demo:
```bash
cargo run --bin mini-redis
```
