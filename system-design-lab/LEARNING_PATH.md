# System Design Interview Lab 🎯

> **Goal**: Staff+ level system design interview preparation with hands-on Rust implementations

## How to Use This Lab

Each folder contains:
1. **README.md** - Comprehensive explanation with Mermaid diagrams
2. **What You Must Master** - Key concepts to understand deeply
3. **Interview Checklist** - What to cover in interviews
4. **src/main.rs** - Working Rust implementation

**Study approach:**
1. Read the README first
2. Run the code: `cargo run --bin <project-name>`
3. Modify and experiment
4. Practice explaining aloud

```
system-design-lab/
├── core-concepts/       # Fundamentals you MUST know
├── real-designs/        # Classic interview questions with implementations
├── key-technologies/    # Deep dives into Redis, Kafka, ZooKeeper, etc.
├── patterns/            # Scalability patterns with code
└── diagrams/            # Index to all architecture diagrams
```

---

## 📚 Learning Path (8-12 weeks)

### Phase 1: Core Concepts (Week 1-3)

| # | Topic | What to Master | Difficulty |
|---|-------|----------------|------------|
| 1 | [Networking](core-concepts/01-networking/) | TCP vs UDP, HTTP/2, WebSockets, TLS | ⭐⭐ |
| 2 | [API Design](core-concepts/02-api-design/) | REST conventions, pagination, versioning | ⭐⭐ |
| 3 | [Data Modeling](core-concepts/03-data-modeling/) | SQL vs NoSQL decision, denormalization | ⭐⭐⭐ |
| 4 | [Caching](core-concepts/04-caching/) | Cache-aside, write-through, eviction | ⭐⭐ |
| 5 | [Sharding](core-concepts/05-sharding/) | Shard key selection, cross-shard queries | ⭐⭐⭐ |
| 6 | [Consistent Hashing](core-concepts/06-consistent-hashing/) | Hash ring, virtual nodes, rebalancing | ⭐⭐⭐ |
| 7 | [CAP Theorem](core-concepts/07-cap-theorem/) | CP vs AP trade-offs, consistency models | ⭐⭐⭐ |
| 8 | [Database Indexing](core-concepts/08-database-indexing/) | B-tree, LSM tree, when to index | ⭐⭐⭐ |
| 9 | [Numbers to Know](core-concepts/09-numbers-to-know/) | Latency, throughput, capacity estimation | ⭐⭐ |

### Phase 2: Real System Designs (Week 4-8)

| # | System | Key Interview Points | Difficulty |
|---|--------|---------------------|------------|
| 1 | [URL Shortener](real-designs/01-url-shortener/) | ID generation, hash collisions, caching | ⭐⭐ |
| 2 | [Rate Limiter](real-designs/02-rate-limiter/) | Token bucket, sliding window, distributed | ⭐⭐ |
| 3 | [Distributed Cache](real-designs/03-distributed-cache/) | Consistent hash, replication, eviction | ⭐⭐⭐ |
| 4 | [Message Queue](real-designs/04-message-queue/) | Partitions, ordering, at-least-once | ⭐⭐⭐ |
| 5 | [Web Crawler](real-designs/05-web-crawler/) | Politeness, URL frontier, deduplication | ⭐⭐⭐ |
| 6 | [Key-Value Store](real-designs/06-key-value-store/) | LSM tree, WAL, compaction, bloom filter | ⭐⭐⭐⭐ |
| 7 | [Load Balancer](real-designs/07-load-balancer/) | L4 vs L7, algorithms, health checks | ⭐⭐⭐ |
| 8 | [Chat System](real-designs/08-chat-system/) | WebSockets, presence, message delivery | ⭐⭐⭐⭐ |
| 9 | [News Feed](real-designs/09-news-feed/) | Push vs pull fanout, ranking, celebrities | ⭐⭐⭐⭐ |
| 10 | [Ticket Booking](real-designs/10-ticket-booking/) | Inventory lock, overselling, flash sale | ⭐⭐⭐⭐ |

### Phase 3: Key Technologies (Week 9-10)

| Technology | Why You Must Know It | Key Concepts |
|------------|---------------------|--------------|
| [Redis](key-technologies/redis/) | Used in almost every system | Data structures, persistence, cluster |
| [Kafka](key-technologies/kafka/) | Event streaming backbone | Partitions, consumer groups, ordering |
| [ZooKeeper](key-technologies/zookeeper/) | Distributed coordination | Leader election, distributed locks |
| [Elasticsearch](key-technologies/elasticsearch/) | Full-text search | Inverted index, shards, analyzers |
| [ML Models Timeline](key-technologies/ml-models/) | Understand model evolution and architecture tradeoffs | CNNs, RNNs, Transformers, diffusion, MoE, multimodal models |
| [RL Frameworks](key-technologies/ml-training/rl-frameworks/) | Understand reinforcement learning training stacks | Gymnasium, SB3, CleanRL, RLlib, Brax, TRL, OpenRLHF, verl |
| [MLSys 2026 Guide](key-technologies/mlsys-conferences/mlsys-2026/) | Learn current ML systems research from the conference | LLM serving, training, agents, compression, compilers, benchmarks |

### Phase 4: Scalability Patterns (Week 11-12)

| Pattern | Problem It Solves | Example |
|---------|-------------------|---------|
| Read Replicas | Read-heavy workloads | Database replicas + cache |
| Sharding | Write scaling | Partition by user_id |
| CQRS | Different read/write patterns | Separate read models |
| Event Sourcing | Audit, replay | Store events, derive state |

---

## 🎯 Interview Framework

### 1. Requirements (3-5 min)
```
Functional: What does the system DO?
Non-functional: Scale, latency, availability, consistency
```

### 2. Capacity Estimation (3-5 min)
```rust
// Example: URL Shortener
let daily_urls = 100_000_000;  // 100M URLs/day
let read_write_ratio = 100;     // 100:1 read heavy
let writes_per_sec = daily_urls / 86400;  // ~1,157 writes/sec
let reads_per_sec = writes_per_sec * read_write_ratio;  // ~115,700 reads/sec
let storage_per_url = 500;  // bytes
let yearly_storage = daily_urls * 365 * storage_per_url;  // ~18TB/year
```

### 3. High-Level Design (10-15 min)
- Draw boxes: clients, load balancers, services, databases
- Explain data flow
- Identify bottlenecks

### 4. Deep Dive (15-20 min)
- Database schema
- API design
- Scaling strategies
- Failure handling

### 5. Wrap-up (3-5 min)
- Tradeoffs made
- Future improvements
- Monitoring/alerting

---

## 🔢 Numbers Every Engineer Should Know

```
L1 cache reference                           0.5 ns
Branch mispredict                            5   ns
L2 cache reference                           7   ns
Mutex lock/unlock                           25   ns
Main memory reference                      100   ns
Compress 1K bytes with Zippy             3,000   ns
Send 1K bytes over 1 Gbps network       10,000   ns
Read 4K randomly from SSD              150,000   ns
Read 1 MB sequentially from memory     250,000   ns
Round trip within same datacenter      500,000   ns
Read 1 MB sequentially from SSD      1,000,000   ns
Disk seek                           10,000,000   ns
Read 1 MB sequentially from disk    20,000,000   ns
Send packet CA->Netherlands->CA    150,000,000   ns
```

### Quick Capacity Math
```
1 day = 86,400 seconds ≈ 100,000 seconds
1 million requests/day ≈ 12 requests/second
1 GB = 1 billion bytes
1 TB = 1000 GB = 1 trillion bytes
```

---

## 🏗️ How to Use This Lab

### For Each Topic:
1. **Read** the README.md for theory
2. **Run** the code examples
3. **Modify** to understand edge cases
4. **Draw** the architecture diagram
5. **Explain** out loud (interview practice)

### Running the Code:
```bash
cd system-design-lab
cargo build --release

# Run specific example
cargo run --bin consistent-hashing
cargo run --bin rate-limiter
cargo run --bin mini-redis
```

### Practice Schedule:
- **Daily**: 1 core concept OR 1 design deep-dive
- **Weekly**: 2 mock interviews (45 min each)
- **Before Interview**: Review numbers, common patterns

---

## 📋 Self-Assessment Checklist

### Core Concepts
- [ ] Can explain CAP theorem with real examples
- [ ] Can implement consistent hashing from scratch
- [ ] Know when to use SQL vs NoSQL
- [ ] Understand caching strategies and invalidation
- [ ] Can estimate capacity for any system

### System Design
- [ ] Can design URL shortener in 30 minutes
- [ ] Can explain rate limiter algorithms
- [ ] Understand database sharding strategies
- [ ] Can design real-time chat system
- [ ] Know how to handle hot keys/partitions

### Communication
- [ ] Drive the interview, ask clarifying questions
- [ ] Clearly articulate tradeoffs
- [ ] Draw clean architecture diagrams
- [ ] Explain technical concepts simply

---

## 🚀 Getting Started

Start with the first core concept:

```bash
cd core-concepts/01-networking
cat README.md
cargo run
```

Good luck! 🎯
