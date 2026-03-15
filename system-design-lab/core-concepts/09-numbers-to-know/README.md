# Numbers Every Engineer Should Know

## Latency Numbers (2024)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Latency Numbers You Should Know                           │
│                                                                              │
│  Operation                                          Time                     │
│  ─────────────────────────────────────────────────────────────────────────  │
│  L1 cache reference                                 0.5 ns                   │
│  Branch mispredict                                  5 ns                     │
│  L2 cache reference                                 7 ns                     │
│  Mutex lock/unlock                                  25 ns                    │
│  Main memory reference                              100 ns                   │
│  Compress 1K bytes with Snappy                      3,000 ns    (3 μs)      │
│  Send 1K bytes over 1 Gbps network                  10,000 ns   (10 μs)     │
│  Read 4K randomly from SSD                          150,000 ns  (150 μs)    │
│  Read 1 MB sequentially from memory                 250,000 ns  (250 μs)    │
│  Round trip within same datacenter                  500,000 ns  (0.5 ms)    │
│  Read 1 MB sequentially from SSD                    1,000,000 ns (1 ms)     │
│  Disk seek (HDD)                                    10,000,000 ns (10 ms)   │
│  Read 1 MB sequentially from HDD                    20,000,000 ns (20 ms)   │
│  Send packet CA→Netherlands→CA                      150,000,000 ns (150 ms) │
│                                                                              │
│  Visual scale:                                                               │
│  1 ns  [·]                                                                   │
│  10 ns [··········]                                                          │
│  100 ns [············································]                       │
│  1 μs  [·····························································...]   │
│  ...                                                                         │
│  150 ms [███████████████████████████████████████████████████████████████...] │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Quick Mental Math

### Time Conversions
```
1 second         = 1,000 milliseconds (ms)
1 millisecond    = 1,000 microseconds (μs)
1 microsecond    = 1,000 nanoseconds (ns)

1 day            = 86,400 seconds ≈ 100,000 seconds
1 month          ≈ 2.5 million seconds
1 year           ≈ 31.5 million seconds ≈ π × 10^7 seconds
```

### Data Size Conversions
```
1 bit (b)
8 bits = 1 Byte (B)

1 KB  = 1,000 B      (10^3)   - A short text file
1 MB  = 1,000 KB     (10^6)   - A high-res photo
1 GB  = 1,000 MB     (10^9)   - A movie
1 TB  = 1,000 GB     (10^12)  - All movies you'll watch
1 PB  = 1,000 TB     (10^15)  - Large data warehouse

Binary (exact):
1 KiB = 1,024 B
1 MiB = 1,024 KiB
1 GiB = 1,024 MiB
```

### Request Rate Conversions
```
1 million requests/day    ≈ 12 requests/second
1 billion requests/day    ≈ 12,000 requests/second

Twitter scale:  ~600 million tweets/day ≈ 7,000 tweets/second
YouTube scale:  ~5 billion videos/day   ≈ 60,000 videos/second
```

## Capacity Estimation Template

### Example: URL Shortener
```rust
// 1. Identify read/write ratio
let read_write_ratio = 100;  // Read-heavy

// 2. Estimate traffic
let writes_per_month = 100_000_000;  // 100M new URLs
let writes_per_second = writes_per_month / (30 * 86400);  // ~38 writes/sec
let reads_per_second = writes_per_second * read_write_ratio;  // ~3,800 reads/sec

// 3. Estimate storage (5 years)
let url_record_size = 500;  // bytes (URL + metadata)
let urls_in_5_years = writes_per_month * 12 * 5;  // 6 billion URLs
let total_storage = urls_in_5_years * url_record_size;  // ~3 TB

// 4. Bandwidth
let incoming_bandwidth = writes_per_second * 500;  // ~19 KB/s
let outgoing_bandwidth = reads_per_second * 500;   // ~1.9 MB/s
```

### Example: YouTube
```rust
// DAU and content
let dau = 800_000_000;                    // 800M daily users
let videos_watched_per_user = 5;          // Per day
let total_video_views = dau * 5;          // 4 billion/day
let views_per_second = total_video_views / 86400;  // ~46,000/sec

// Video upload
let uploads_per_day = 720_000;            // 720K videos/day
let avg_video_size_mb = 300;              // 300 MB average
let storage_per_day_tb = uploads_per_day * avg_video_size_mb / 1_000_000;
// ~216 TB/day new storage!

// Bandwidth
let avg_video_bitrate_mbps = 5;           // 5 Mbps
let concurrent_viewers = 10_000_000;       // At any moment
let bandwidth_tbps = concurrent_viewers * 5 / 1_000_000;
// ~50 Tbps!
```

## Database Numbers

```
┌───────────────────────────────────────────────────────────────────┐
│                    Database Performance                           │
│                                                                   │
│  Single PostgreSQL instance (well configured):                    │
│  • Read QPS:     10,000 - 50,000                                 │
│  • Write QPS:    1,000 - 10,000                                  │
│  • Storage:      Up to a few TB                                  │
│                                                                   │
│  Single Redis instance:                                           │
│  • Read QPS:     100,000+                                        │
│  • Write QPS:    100,000+                                        │
│  • Storage:      Limited by RAM (typically < 100 GB)             │
│                                                                   │
│  Single Cassandra node:                                           │
│  • Read QPS:     10,000 - 30,000                                 │
│  • Write QPS:    10,000 - 50,000                                 │
│  • Storage:      1-2 TB per node                                 │
│                                                                   │
│  Connection limits (order of magnitude):                          │
│  • PostgreSQL:   ~1,000 connections                              │
│  • MySQL:        ~5,000 connections                              │
│  • MongoDB:      ~10,000 connections                             │
└───────────────────────────────────────────────────────────────────┘
```

## Network Numbers

```
┌───────────────────────────────────────────────────────────────────┐
│                    Network Numbers                                 │
│                                                                   │
│  TCP connection setup:     1.5 RTT (SYN, SYN-ACK, ACK)           │
│  TLS handshake:            1-2 RTT                               │
│  DNS lookup (uncached):    20-200 ms                             │
│  DNS lookup (cached):      < 1 ms                                │
│                                                                   │
│  RTT (Round Trip Time):                                          │
│  • Same datacenter:        0.5 ms                                │
│  • Same region:            1-5 ms                                │
│  • Cross-region:           50-200 ms                             │
│  • Intercontinental:       100-300 ms                            │
│                                                                   │
│  Bandwidth:                                                       │
│  • 1 Gbps network:         125 MB/s theoretical                  │
│  • 10 Gbps network:        1.25 GB/s theoretical                 │
│  • Typical cloud egress:   ~$0.10/GB                             │
└───────────────────────────────────────────────────────────────────┘
```

## Instance Sizing Reference

```
┌───────────────────────────────────────────────────────────────────┐
│                    AWS Instance Reference                         │
│                                                                   │
│  Type         vCPU    Memory    Good For                         │
│  ─────────────────────────────────────────────────────────────── │
│  t3.micro     2       1 GB     Development                       │
│  t3.medium    2       4 GB     Small apps                        │
│  m5.large     2       8 GB     Web servers                       │
│  m5.xlarge    4       16 GB    Medium apps                       │
│  m5.4xlarge   16      64 GB    Large apps                        │
│  r5.4xlarge   16      128 GB   In-memory/cache                   │
│  c5.4xlarge   16      32 GB    Compute-heavy                     │
│                                                                   │
│  Rough cost (on-demand, us-east-1):                              │
│  • m5.large:   ~$70/month                                        │
│  • m5.xlarge:  ~$140/month                                       │
│  • Reserved:   ~50% off on-demand                                │
└───────────────────────────────────────────────────────────────────┘
```

## Rules of Thumb

```
✓ If in doubt, estimate high for storage, low for QPS
✓ Plan for 10x growth over current needs
✓ 80-20 rule: 20% of data gets 80% of traffic
✓ Cache hit rate should be > 90% for most caches
✓ Database query should be < 10ms for real-time apps
✓ API response time < 200ms for good UX
✓ 99th percentile matters more than average

Common bottlenecks (in order):
1. Database (usually first)
2. Network (bandwidth, latency)
3. Compute (CPU, memory)
4. Disk I/O
```

## Interview Tips

When doing capacity estimation:
1. **State assumptions clearly**: "Assuming 100M DAU..."
2. **Round numbers**: Use powers of 10, easier math
3. **Show reasoning**: "100M users × 10 requests × 1KB = 1TB/day"
4. **Identify bottlenecks**: What hits limits first?
5. **Propose scaling**: "At 10x, we'd need to shard..."
