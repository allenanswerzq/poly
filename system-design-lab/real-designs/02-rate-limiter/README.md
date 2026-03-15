# Rate Limiter Design

## Problem Statement

Design a rate limiter that:
- Limits requests per user/IP
- Works in a distributed environment
- Handles millions of requests/second
- Returns appropriate error responses

## Requirements

### Functional
- Limit N requests per time window per client
- Support different limits for different endpoints
- Support burst traffic

### Non-Functional
- Low latency (< 1ms decision)
- Highly available
- Distributed (works across multiple servers)

## Rate Limiting Algorithms

### 1. Token Bucket ⭐ (Most Common)

```
┌─────────────────────────────────────────────────────────┐
│                     Token Bucket                        │
│                                                         │
│   Tokens refill at rate R          ┌─────────────────┐  │
│            │                       │  ○ ○ ○ ○ ○ ○ ○  │  │
│            ↓                       │     Bucket      │  │
│   ┌──────────────┐                │   (capacity B)  │  │
│   │   Refiller   │───────────────→│                 │  │
│   └──────────────┘                └────────┬────────┘  │
│                                            │           │
│   Request consumes 1 token                 ↓           │
│                                    ┌──────────────┐    │
│   If tokens available → Allow      │   Requests   │    │
│   If no tokens → Reject            └──────────────┘    │
│                                                         │
│   Pros: Allows bursts up to bucket size                │
│   Cons: Memory per user (2 values: tokens, last_time)  │
└─────────────────────────────────────────────────────────┘
```

### 2. Leaky Bucket

```
┌─────────────────────────────────────────────────────────┐
│                     Leaky Bucket                        │
│                                                         │
│   Requests enter bucket            ┌─────────────────┐  │
│            │                       │  ● ● ● ● ● ● ●  │  │
│            ↓                       │     Bucket      │  │
│   ┌──────────────┐                │   (queue size)  │  │
│   │  Incoming    │───────────────→│                 │  │
│   └──────────────┘                └────────┬────────┘  │
│                                            │           │
│   Processed at constant rate               ↓           │
│                                    ┌──────────────┐    │
│   If queue full → Reject           │  Processor   │    │
│   Smooth output rate               └──────────────┘    │
│                                                         │
│   Pros: Smooth traffic, useful for APIs with limits   │
│   Cons: No burst handling, can introduce latency      │
└─────────────────────────────────────────────────────────┘
```

### 3. Fixed Window Counter

```
┌─────────────────────────────────────────────────────────┐
│                 Fixed Window Counter                    │
│                                                         │
│   Window 1        Window 2        Window 3              │
│   [0s - 60s]      [60s - 120s]    [120s - 180s]        │
│                                                         │
│   ┌──────────┐    ┌──────────┐    ┌──────────┐         │
│   │ count=97 │    │ count=45 │    │ count=0  │         │
│   │ limit=100│    │ limit=100│    │ limit=100│         │
│   └──────────┘    └──────────┘    └──────────┘         │
│                                                         │
│   Pros: Simple, memory efficient (1 counter per window)│
│   Cons: Burst at window boundaries (2x limit possible) │
│                                                         │
│   Edge case: 100 requests at :59, 100 at :01           │
│   = 200 requests in 2 seconds!                         │
└─────────────────────────────────────────────────────────┘
```

### 4. Sliding Window Log

```
┌─────────────────────────────────────────────────────────┐
│                 Sliding Window Log                      │
│                                                         │
│   Track timestamp of each request                       │
│                                                         │
│   Timestamps: [1:00:00, 1:00:15, 1:00:30, 1:00:45]     │
│                                                         │
│   On new request at 1:01:10:                           │
│   1. Remove timestamps older than 1 minute             │
│      [1:00:15, 1:00:30, 1:00:45]                       │
│   2. Count remaining (3)                               │
│   3. If count < limit, allow and add timestamp         │
│                                                         │
│   Pros: Accurate, no edge cases                         │
│   Cons: Memory intensive (store all timestamps)        │
└─────────────────────────────────────────────────────────┘
```

### 5. Sliding Window Counter ⭐ (Best Balance)

```
┌─────────────────────────────────────────────────────────┐
│              Sliding Window Counter                     │
│                                                         │
│   Combines fixed window with weighted average           │
│                                                         │
│   Previous Window    Current Window                     │
│   [count = 70]       [count = 30]                      │
│                                                         │
│   Current time: 25% into current window                 │
│   Weighted count = 70 * 0.75 + 30 = 82.5               │
│                                                         │
│   Limit = 100, so request allowed                       │
│                                                         │
│   Pros: Memory efficient, reasonably accurate          │
│   Cons: Approximate (good enough for rate limiting)    │
└─────────────────────────────────────────────────────────┘
```

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        API Gateway Layer                            │
│   ┌─────────────────────────────────────────────────────────────┐   │
│   │                      Rate Limiter                            │   │
│   │                                                              │   │
│   │   ┌──────────────┐        ┌──────────────────────────────┐  │   │
│   │   │  Local Cache │◄──────►│      Redis Cluster           │  │   │
│   │   │  (fast path) │        │  (distributed state)         │  │   │
│   │   └──────────────┘        └──────────────────────────────┘  │   │
│   │                                                              │   │
│   │   Rules Engine: /api/* → 1000/min, /search → 100/min       │   │
│   └─────────────────────────────────────────────────────────────┘   │
│                                   │                                  │
│                                   ↓                                  │
│   ┌─────────────────────────────────────────────────────────────┐   │
│   │                    Backend Services                          │   │
│   └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

## Implementation Decisions

| Decision | Options | Choice |
|----------|---------|--------|
| Algorithm | Token Bucket vs Sliding Window | Token Bucket (allows bursts) |
| Storage | Redis vs Local | Redis (distributed) + Local cache |
| Key | IP vs User ID vs API Key | Flexible (configurable) |
| Response | 429 + Retry-After header | Standard HTTP |

## Redis Implementation (Lua Script)

```lua
-- Token bucket in Redis
local key = KEYS[1]
local capacity = tonumber(ARGV[1])
local rate = tonumber(ARGV[2])
local now = tonumber(ARGV[3])
local requested = tonumber(ARGV[4])

local data = redis.call('HMGET', key, 'tokens', 'last_update')
local tokens = tonumber(data[1]) or capacity
local last_update = tonumber(data[2]) or now

-- Refill tokens
local elapsed = now - last_update
local refill = elapsed * rate
tokens = math.min(capacity, tokens + refill)

-- Check and consume
local allowed = tokens >= requested
if allowed then
    tokens = tokens - requested
end

-- Update state
redis.call('HMSET', key, 'tokens', tokens, 'last_update', now)
redis.call('EXPIRE', key, 3600)

return {allowed and 1 or 0, tokens}
```

## Interview Tips

1. **Start with requirements**: What's being rate limited? Per user? Per IP? Per API key?
2. **Discuss tradeoffs**: Token bucket vs sliding window
3. **Think distributed**: How do multiple servers share state?
4. **Handle edge cases**: Clock skew, Redis failures
5. **Consider UX**: Return Retry-After header, graceful degradation

Run the demo:
```bash
cargo run --bin rate-limiter
```
