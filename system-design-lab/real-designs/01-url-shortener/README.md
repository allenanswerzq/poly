# URL Shortener (Bit.ly)

## Problem Statement

Design a URL shortening service like bit.ly that:
- Converts long URLs to short URLs
- Redirects short URLs to original URLs
- Handles billions of URLs
- Provides analytics (optional)

## Requirements

### Functional
- Shorten URL: `POST /shorten` → returns short URL
- Redirect: `GET /{shortCode}` → 301/302 redirect
- Custom aliases (optional)
- Analytics: click count, referrers

### Non-Functional
- 100M URLs/day created
- 10B redirects/day (100:1 read/write ratio)
- Low latency redirects (< 100ms)
- High availability (99.9%)
- URLs never expire (unless specified)

## Capacity Estimation

```rust
// Writes
let urls_per_day = 100_000_000;           // 100M
let urls_per_second = urls_per_day / 86400; // ~1,157 URLs/sec

// Reads (100:1 ratio)
let reads_per_second = urls_per_second * 100; // ~115,700 reads/sec

// Storage (5 years)
let url_size_bytes = 500;                    // Average URL + metadata
let urls_5_years = urls_per_day * 365 * 5;   // ~182.5 billion URLs
let storage_5_years = urls_5_years * url_size_bytes; // ~91 TB

// Short code length
// Base62: [a-zA-Z0-9] = 62 characters
// 7 characters: 62^7 = 3.5 trillion combinations ✓
```

## Short Code Generation

### Option 1: Hash-based
```
MD5(longUrl) → take first 7 chars → base62 encode
Pros: Deterministic, same URL → same code
Cons: Collisions possible
```

### Option 2: Counter-based
```
Distributed counter → base62(counter)
Pros: No collisions, predictable
Cons: Predictable URLs (security concern)
```

### Option 3: Random + Collision Check ⭐
```
Generate random 7-char string → check DB for collision
Pros: Simple, unpredictable
Cons: Extra DB lookup (cached)
```

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              URL Shortener Architecture                      │
│                                                                              │
│   ┌──────────┐         ┌─────────────────────────────────────────────────┐  │
│   │  Client  │────────►│              Load Balancer                      │  │
│   └──────────┘         │         (Nginx / AWS ALB)                       │  │
│                        └────────────────────┬────────────────────────────┘  │
│                                             │                                │
│                        ┌────────────────────┼────────────────────────┐      │
│                        ▼                    ▼                        ▼      │
│                 ┌────────────┐       ┌────────────┐           ┌────────────┐│
│                 │   API      │       │   API      │           │   API      ││
│                 │  Server 1  │       │  Server 2  │    ...    │  Server N  ││
│                 └─────┬──────┘       └─────┬──────┘           └─────┬──────┘│
│                       │                    │                        │       │
│                       ▼                    ▼                        ▼       │
│                 ┌─────────────────────────────────────────────────────────┐ │
│                 │                    Redis Cache                          │ │
│                 │            (hot URLs, rate limiting)                    │ │
│                 └────────────────────────┬────────────────────────────────┘ │
│                                          │                                   │
│            ┌─────────────────────────────┼─────────────────────────────┐    │
│            ▼                             ▼                             ▼    │
│     ┌─────────────┐              ┌─────────────┐              ┌─────────────┐
│     │  DB Shard 1 │              │  DB Shard 2 │              │  DB Shard N ││
│     │   (a-f)     │              │   (g-m)     │              │   (n-z)     ││
│     └─────────────┘              └─────────────┘              └─────────────┘│
│                                                                              │
│   ┌─────────────────────────────────────────────────────────────────────────┐
│   │                        Analytics Pipeline                                │
│   │  Clicks → Kafka → Flink → Analytics DB (ClickHouse/Druid)               │
│   └─────────────────────────────────────────────────────────────────────────┘
└─────────────────────────────────────────────────────────────────────────────┘
```

## Database Schema

```sql
-- Main URL table (sharded by short_code)
CREATE TABLE urls (
    short_code VARCHAR(10) PRIMARY KEY,
    long_url TEXT NOT NULL,
    user_id BIGINT,
    created_at TIMESTAMP DEFAULT NOW(),
    expires_at TIMESTAMP,
    click_count BIGINT DEFAULT 0
);

-- Index for reverse lookup (optional)
CREATE INDEX idx_long_url ON urls(long_url);

-- Analytics table (separate database)
CREATE TABLE clicks (
    id BIGINT AUTO_INCREMENT,
    short_code VARCHAR(10),
    clicked_at TIMESTAMP,
    user_agent TEXT,
    referrer TEXT,
    ip_address VARCHAR(45),
    country VARCHAR(2)
);
```

## Key Design Decisions

| Decision | Options | Choice | Rationale |
|----------|---------|--------|-----------|
| ID Generation | Counter vs Hash vs Random | Random | Unpredictable, simple |
| Database | SQL vs NoSQL | SQL (Postgres) | ACID, mature |
| Caching | Redis | Redis | Fast reads, TTL support |
| Sharding | Hash vs Range | Hash on short_code | Even distribution |
| Redirect | 301 vs 302 | 301 (permanent) | Better UX, cacheable |

## API Design

```
POST /api/v1/shorten
{
    "url": "https://example.com/very/long/url",
    "custom_alias": "my-link",  // optional
    "expires_at": "2025-12-31"  // optional
}
→ 201 Created
{
    "short_url": "https://short.ly/abc1234",
    "expires_at": "2025-12-31T00:00:00Z"
}

GET /abc1234
→ 301 Redirect (Location: https://example.com/very/long/url)

GET /api/v1/stats/abc1234
→ 200 OK
{
    "clicks": 1234,
    "created_at": "2024-01-01T00:00:00Z"
}
```

## Scaling Considerations

1. **Cache hot URLs**: 80-20 rule, cache popular URLs in Redis
2. **Database sharding**: Shard by first char of short code
3. **Async analytics**: Use Kafka to decouple click tracking
4. **CDN**: Cache redirects at edge for popular URLs
5. **Rate limiting**: Prevent abuse

## Interview Tips

1. Start with requirements and capacity estimation
2. Draw high-level architecture first
3. Discuss ID generation tradeoffs
4. Mention caching strategy
5. Don't forget analytics and monitoring

Run the demo:
```bash
cargo run --bin url-shortener
```
