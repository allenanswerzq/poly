use dashmap::DashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// Token Bucket
// =============================================================================

// Token bucket rate limiter.
// Each client (identified by key) gets its own bucket of tokens.
// Requests consume tokens; tokens refill over time at a fixed rate.
//
// Bucket for "client-A" (max 5 tokens, refill 2/sec):
//
//   Time 0:  [●●●●●]  5 tokens (full)
//   Req 1:   [●●●● ]  4 tokens
//   Req 2:   [●●●  ]  3 tokens
//   Req 3:   [●●   ]  2 tokens
//   Req 4:   [●    ]  1 token
//   Req 5:   [     ]  0 tokens → bucket empty
//   Req 6:   REJECTED (429 Too Many Requests)
//
//   ...1 second passes, 2 tokens refill...
//
//   Req 7:   [●    ]  1 token left → allowed
//   Req 8:   [     ]  0 tokens → rejected again
struct RateLimiter {
    // Concurrent hashmap: "client-key" → (remaining_tokens, last_refill_unix_secs)
    // DashMap allows multiple threads to read/write without a global lock
    buckets: DashMap<String, (u32, u64)>,
    max_tokens: u32,    // bucket capacity (also the max burst size)
    refill_rate: u32,   // how many tokens are added per second
}

impl RateLimiter {
    fn new(max_tokens: u32, refill_rate: u32) -> Self {
        Self {
            buckets: DashMap::new(),
            max_tokens,
            refill_rate,
        }
    }

    // Returns true if the request is allowed, false if rate-limited (429).
    fn allow(&self, key: &str) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        let mut entry = self.buckets.entry(key.to_string()).or_insert((self.max_tokens, now));
        let (tokens, last_refill) = entry.value_mut();

        // Step 1: Refill — add tokens based on how much time has passed
        let elapsed = now - *last_refill;
        if elapsed > 0 {
            *tokens = (*tokens + (elapsed as u32 * self.refill_rate)).min(self.max_tokens);
            *last_refill = now;
        }

        // Step 2: Try to consume one token
        if *tokens > 0 {
            *tokens -= 1;
            true
        } else {
            false
        }
    }
}

pub fn demo_rate_limiting() {
    println!("\n  ═══ demo_rate_limiting ═══\n");
    println!("  Token bucket rate limiter at the gateway (5 tokens, 2/sec refill):\n");

    let limiter = RateLimiter::new(5, 2);

    for i in 1..=8 {
        let allowed = limiter.allow("client-A");
        println!("    Request #{}: {} {}",
            i,
            if allowed { "✓ ALLOWED" } else { "✗ REJECTED (429)" },
            if i == 5 { "← bucket empty after this" } else { "" }
        );
    }

    println!("\n    Token bucket: start with 5 tokens, consume 1 per request.");
    println!("    When empty → 429 Too Many Requests.");
    println!("    Tokens refill at 2/sec → sustained rate = 2 req/sec.\n");

    println!("    Different clients have separate buckets:");
    let a = limiter.allow("client-B");
    let b = limiter.allow("client-C");
    println!("    client-B: {} (fresh bucket)", if a { "✓" } else { "✗" });
    println!("    client-C: {} (fresh bucket)\n", if b { "✓" } else { "✗" });
}

// =============================================================================
// Fixed Window Counter
// =============================================================================

//   Window 1 [0s─1s]: ●●●●●   5/5 (ok)
//   Window 2 [1s─2s]: ●●●●●   5/5 (ok)
//                   ↑ but 10 requests in 0.2s if clustered at boundary!
struct FixedWindowLimiter {
    windows: DashMap<String, (u32, u64)>,
    max_requests: u32,
    window_secs: u64,
}

impl FixedWindowLimiter {
    fn new(max_requests: u32, window_secs: u64) -> Self {
        Self { windows: DashMap::new(), max_requests, window_secs }
    }

    fn allow(&self, key: &str, now: u64) -> bool {
        let window_start = now / self.window_secs * self.window_secs;
        let mut entry = self.windows.entry(key.to_string()).or_insert((0, window_start));
        let (count, entry_window) = entry.value_mut();

        if *entry_window != window_start {
            *count = 0;
            *entry_window = window_start;
        }

        if *count < self.max_requests {
            *count += 1;
            true
        } else {
            false
        }
    }
}

// =============================================================================
// Sliding Window Log
// =============================================================================

//   Timestamps: [0.1, 0.3, 0.5, 0.8, 0.9, 1.1, 1.2]
//   Window = last 1s from now=1.2:  count timestamps >= 0.2 → 6 requests
//   In production at 10K req/s: 10K timestamps per client per second = memory bomb
struct SlidingWindowLogLimiter {
    logs: DashMap<String, Vec<u64>>,
    max_requests: u32,
    window_ms: u64,
}

impl SlidingWindowLogLimiter {
    fn new(max_requests: u32, window_ms: u64) -> Self {
        Self { logs: DashMap::new(), max_requests, window_ms }
    }

    fn allow(&self, key: &str, now_ms: u64) -> bool {
        let mut entry = self.logs.entry(key.to_string()).or_insert_with(Vec::new);
        let timestamps = entry.value_mut();

        let cutoff = now_ms.saturating_sub(self.window_ms);
        timestamps.retain(|&t| t > cutoff);

        if (timestamps.len() as u32) < self.max_requests {
            timestamps.push(now_ms);
            true
        } else {
            false
        }
    }
}

// =============================================================================
// Leaky Bucket
// =============================================================================

//   ┌─────────────┐
//   │ ● ● ● ● ●   │  queue (capacity 5)
//   └─────┬───────┘
//         ▼ drip at 2/sec (fixed, always)
struct LeakyBucketLimiter {
    buckets: DashMap<String, (f64, u64)>,
    capacity: f64,
    leak_rate: f64,
}

impl LeakyBucketLimiter {
    fn new(capacity: f64, leak_rate: f64) -> Self {
        Self { buckets: DashMap::new(), capacity, leak_rate }
    }

    fn allow(&self, key: &str, now_ms: u64) -> bool {
        let mut entry = self.buckets.entry(key.to_string()).or_insert((0.0, now_ms));
        let (water, last_leak) = entry.value_mut();

        let elapsed_secs = (now_ms - *last_leak) as f64 / 1000.0;
        *water = (*water - elapsed_secs * self.leak_rate).max(0.0);
        *last_leak = now_ms;

        if *water + 1.0 <= self.capacity {
            *water += 1.0;
            true
        } else {
            false
        }
    }
}

// =============================================================================
// Comparison Demo
// =============================================================================

pub fn demo_rate_limiting_comparison() {
    println!("\n  ═══ demo_rate_limiting_comparison ═══\n");
    println!("  Comparing 4 rate limiting algorithms side by side:\n");

    // --- Fixed Window ---
    println!("  ── 1. Fixed Window Counter (5 req per 1s window) ──\n");
    let fw = FixedWindowLimiter::new(5, 1);
    for i in 1..=7 {
        let allowed = fw.allow("client", 0);
        println!("    Req #{} at t=0.{}s: {}", i, i, if allowed { "✓" } else { "✗ REJECTED" });
    }
    println!("\n    Boundary problem:");
    let fw2 = FixedWindowLimiter::new(5, 10);
    for i in 1..=5 {
        fw2.allow("client", 9);
        if i == 5 { println!("    5 requests at t=9s  (end of window 1)   → all ✓"); }
    }
    for i in 1..=5 {
        fw2.allow("client", 10);
        if i == 5 { println!("    5 requests at t=10s (start of window 2) → all ✓"); }
    }
    println!("    → 10 requests in 1 second! Limit was 5/10s. That's the bug.\n");

    // --- Sliding Window Log ---
    println!("  ── 2. Sliding Window Log (5 req per 1000ms) ──\n");
    let sw = SlidingWindowLogLimiter::new(5, 1000);
    let times = [100, 200, 400, 600, 800, 900, 950];
    for (i, &t) in times.iter().enumerate() {
        let allowed = sw.allow("client", t);
        println!("    Req #{} at t={}ms: {}", i + 1, t, if allowed { "✓" } else { "✗ REJECTED" });
    }
    let log_size = sw.logs.get("client").map(|v| v.len()).unwrap_or(0);
    println!("\n    Stored {} timestamps in memory (grows with every request!)", log_size);
    println!("    At 10K req/s → 10,000 timestamps per client per second.\n");

    // --- Leaky Bucket ---
    println!("  ── 3. Leaky Bucket (capacity 5, leak 2/sec) ──\n");
    let lb = LeakyBucketLimiter::new(5.0, 2.0);
    for i in 1..=7 {
        let allowed = lb.allow("client", 0);
        println!("    Req #{} at t=0ms:    {}{}", i,
            if allowed { "✓" } else { "✗ REJECTED" },
            if i == 5 { " ← bucket full" } else { "" });
    }
    println!("\n    ...1 second passes (2 units leak out)...\n");
    for i in 1..=3 {
        let allowed = lb.allow("client", 1000);
        println!("    Req #{} at t=1000ms: {}{}", i + 7,
            if allowed { "✓" } else { "✗ REJECTED" },
            if !allowed { " ← only 2 leaked, bucket full again" } else { "" });
    }
    println!("\n    Leaky bucket: strict fixed output rate. No bursts.\n");

    // --- Summary ---
    println!("  ── Summary ──\n");
    println!("    ┌──────────────────┬─────────┬──────────┬────────┬───────────┐");
    println!("    │ Algorithm        │ Bursts? │ Accurate │ Memory │ Latency   │");
    println!("    ├──────────────────┼─────────┼──────────┼────────┼───────────┤");
    println!("    │ Token Bucket     │ ✓ Yes   │ ✓ Yes    │ O(1)   │ None      │ ← best");
    println!("    │ Fixed Window     │ ✗ Boundary│ ✗ No   │ O(1)   │ None      │");
    println!("    │ Sliding Log      │ ✗ No    │ ✓ Yes    │ O(n)   │ None      │");
    println!("    │ Leaky Bucket     │ ✗ No    │ ✓ Yes    │ O(1)   │ Queue wait│");
    println!("    └──────────────────┴─────────┴──────────┴────────┴───────────┘\n");
}
