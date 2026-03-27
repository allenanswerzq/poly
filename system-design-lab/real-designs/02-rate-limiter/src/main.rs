#![allow(dead_code, unused_variables, unused_imports)]
//! # Rate Limiter Implementations
//!
//! This module implements several rate limiting algorithms:
//! 1. Token Bucket - Best for allowing bursts
//! 2. Sliding Window Counter - Good balance of accuracy and memory
//! 3. Leaky Bucket - Smooth output rate
//! 4. Fixed Window - Simplest but has edge cases

use dashmap::DashMap;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// =============================================================================
// Token Bucket Rate Limiter
// =============================================================================

/// Token Bucket algorithm implementation
///
/// Tokens are added at a constant rate up to a maximum capacity.
/// Each request consumes one token. Requests are rejected when no tokens available.
#[derive(Debug)]
pub struct TokenBucket {
    capacity: f64,        // Maximum tokens
    refill_rate: f64,     // Tokens per second
    tokens: f64,          // Current tokens (can be fractional)
    last_update: Instant, // Last token refill time
}

impl TokenBucket {
    pub fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            capacity,
            refill_rate,
            tokens: capacity, // Start full
            last_update: Instant::now(),
        }
    }

    /// Try to consume tokens. Returns true if allowed, false if rate limited.
    pub fn try_acquire(&mut self, tokens: f64) -> bool {
        self.refill();

        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();

        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_update = now;
    }

    /// Get current token count (for monitoring)
    pub fn available_tokens(&mut self) -> f64 {
        self.refill();
        self.tokens
    }
}

// =============================================================================
// Distributed Token Bucket (Thread-Safe)
// =============================================================================

/// Thread-safe token bucket for multiple clients
pub struct DistributedTokenBucket {
    buckets: DashMap<String, Mutex<TokenBucket>>,
    capacity: f64,
    refill_rate: f64,
}

impl DistributedTokenBucket {
    pub fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            buckets: DashMap::new(),
            capacity,
            refill_rate,
        }
    }

    /// Try to acquire tokens for a specific client
    pub fn try_acquire(&self, client_id: &str, tokens: f64) -> bool {
        let bucket = self
            .buckets
            .entry(client_id.to_string())
            .or_insert_with(|| Mutex::new(TokenBucket::new(self.capacity, self.refill_rate)));

        let result = bucket.lock().try_acquire(tokens);
        result
    }

    /// Get remaining tokens for a client
    pub fn get_remaining(&self, client_id: &str) -> f64 {
        self.buckets
            .get(client_id)
            .map(|b| b.lock().available_tokens())
            .unwrap_or(self.capacity)
    }
}

// =============================================================================
// Sliding Window Counter
// =============================================================================

/// Sliding window counter - approximates true sliding window
/// using weighted average of current and previous fixed windows
pub struct SlidingWindowCounter {
    window_size_ms: u64,
    max_requests: u64,
    counters: DashMap<String, WindowState>,
}

struct WindowState {
    prev_count: u64,
    curr_count: u64,
    curr_window_start: u64,
}

impl SlidingWindowCounter {
    pub fn new(window_size: Duration, max_requests: u64) -> Self {
        Self {
            window_size_ms: window_size.as_millis() as u64,
            max_requests,
            counters: DashMap::new(),
        }
    }

    fn current_time_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Try to record a request. Returns true if allowed.
    pub fn try_acquire(&self, client_id: &str) -> bool {
        let now = Self::current_time_ms();
        let current_window = now / self.window_size_ms;

        let mut entry = self
            .counters
            .entry(client_id.to_string())
            .or_insert(WindowState {
                prev_count: 0,
                curr_count: 0,
                curr_window_start: current_window,
            });

        let state = entry.value_mut();

        // Check if we've moved to a new window
        if current_window > state.curr_window_start {
            if current_window == state.curr_window_start + 1 {
                state.prev_count = state.curr_count;
            } else {
                state.prev_count = 0;
            }
            state.curr_count = 0;
            state.curr_window_start = current_window;
        }

        // Calculate weighted count
        let elapsed_in_window = now % self.window_size_ms;
        let weight = elapsed_in_window as f64 / self.window_size_ms as f64;

        let weighted_count = state.prev_count as f64 * (1.0 - weight) + state.curr_count as f64;

        if weighted_count < self.max_requests as f64 {
            state.curr_count += 1;
            true
        } else {
            false
        }
    }

    /// Get approximate remaining requests
    pub fn get_remaining(&self, client_id: &str) -> u64 {
        let now = Self::current_time_ms();
        let current_window = now / self.window_size_ms;

        self.counters
            .get(client_id)
            .map(|state| {
                if current_window == state.curr_window_start {
                    let elapsed_in_window = now % self.window_size_ms;
                    let weight = elapsed_in_window as f64 / self.window_size_ms as f64;
                    let weighted_count =
                        state.prev_count as f64 * (1.0 - weight) + state.curr_count as f64;
                    (self.max_requests as f64 - weighted_count).max(0.0) as u64
                } else {
                    self.max_requests
                }
            })
            .unwrap_or(self.max_requests)
    }
}

// =============================================================================
// Fixed Window Counter (Simple but has edge cases)
// =============================================================================

/// Simple fixed window counter
/// Warning: Can allow 2x traffic at window boundaries
pub struct FixedWindowCounter {
    window_size_ms: u64,
    max_requests: u64,
    counters: DashMap<String, (u64, AtomicU64)>, // (window_id, count)
}

impl FixedWindowCounter {
    pub fn new(window_size: Duration, max_requests: u64) -> Self {
        Self {
            window_size_ms: window_size.as_millis() as u64,
            max_requests,
            counters: DashMap::new(),
        }
    }

    fn current_window(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            / self.window_size_ms
    }

    pub fn try_acquire(&self, client_id: &str) -> bool {
        let current_window = self.current_window();

        let mut entry = self
            .counters
            .entry(client_id.to_string())
            .or_insert((current_window, AtomicU64::new(0)));

        // Reset counter if new window
        if entry.0 != current_window {
            *entry.value_mut() = (current_window, AtomicU64::new(0));
        }

        let count = entry.1.fetch_add(1, Ordering::SeqCst);
        count < self.max_requests
    }
}

// =============================================================================
// Leaky Bucket (Queue-based)
// =============================================================================

/// Leaky bucket - processes requests at a constant rate
pub struct LeakyBucket {
    capacity: usize,
    leak_rate: f64, // Requests processed per second
    queue: Mutex<Vec<Instant>>,
    last_leak: Mutex<Instant>,
}

impl LeakyBucket {
    pub fn new(capacity: usize, leak_rate: f64) -> Self {
        Self {
            capacity,
            leak_rate,
            queue: Mutex::new(Vec::new()),
            last_leak: Mutex::new(Instant::now()),
        }
    }

    pub fn try_acquire(&self) -> bool {
        self.leak();

        let mut queue = self.queue.lock();
        if queue.len() < self.capacity {
            queue.push(Instant::now());
            true
        } else {
            false
        }
    }

    fn leak(&self) {
        let now = Instant::now();
        let mut last_leak = self.last_leak.lock();
        let elapsed = now.duration_since(*last_leak).as_secs_f64();
        let to_leak = (elapsed * self.leak_rate) as usize;

        if to_leak > 0 {
            let mut queue = self.queue.lock();
            let drain_count = to_leak.min(queue.len());
            queue.drain(0..drain_count);
            *last_leak = now;
        }
    }

    pub fn queue_size(&self) -> usize {
        self.leak();
        self.queue.lock().len()
    }
}

// =============================================================================
// Rate Limiter with Rules
// =============================================================================

/// A rate limiter that supports different rules for different endpoints
pub struct RateLimiterWithRules {
    rules: HashMap<String, (u64, Duration)>, // endpoint -> (max_requests, window)
    limiters: DashMap<String, SlidingWindowCounter>,
}

impl Default for RateLimiterWithRules {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiterWithRules {
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
            limiters: DashMap::new(),
        }
    }

    pub fn add_rule(&mut self, endpoint: &str, max_requests: u64, window: Duration) {
        self.rules
            .insert(endpoint.to_string(), (max_requests, window));
    }

    pub fn check(&self, endpoint: &str, client_id: &str) -> RateLimitResult {
        let key = format!("{}:{}", endpoint, client_id);

        let (max_requests, window) = self
            .rules
            .get(endpoint)
            .copied()
            .unwrap_or((1000, Duration::from_secs(60))); // Default rule

        let limiter = self
            .limiters
            .entry(endpoint.to_string())
            .or_insert_with(|| SlidingWindowCounter::new(window, max_requests));

        if limiter.try_acquire(&key) {
            RateLimitResult::Allowed {
                remaining: limiter.get_remaining(&key),
            }
        } else {
            RateLimitResult::Limited {
                retry_after: window,
            }
        }
    }
}

#[derive(Debug)]
pub enum RateLimitResult {
    Allowed { remaining: u64 },
    Limited { retry_after: Duration },
}

// =============================================================================
// Demonstration
// =============================================================================

fn main() {
    println!("=== Rate Limiter Implementations Demo ===\n");

    // Demo 1: Token Bucket
    println!("\n  ═══ Token Bucket (10 tokens, 2/sec refill) ═══");
    let mut bucket = TokenBucket::new(10.0, 2.0);

    println!("Initial tokens: {:.1}", bucket.available_tokens());

    // Burst of 8 requests
    for i in 1..=12 {
        let allowed = bucket.try_acquire(1.0);
        println!(
            "  Request {}: {} (remaining: {:.1})",
            i,
            if allowed { "✓" } else { "✗" },
            bucket.available_tokens()
        );
    }

    println!("\nWaiting 2 seconds for refill...");
    std::thread::sleep(Duration::from_secs(2));
    println!("Tokens after wait: {:.1}\n", bucket.available_tokens());

    // Demo 2: Sliding Window Counter
    println!("\n  ═══ Sliding Window Counter (5 requests per 100ms) ═══");
    let sliding = SlidingWindowCounter::new(Duration::from_millis(100), 5);

    for i in 1..=8 {
        let allowed = sliding.try_acquire("user-1");
        println!(
            "  Request {}: {} (remaining: {})",
            i,
            if allowed { "✓" } else { "✗" },
            sliding.get_remaining("user-1")
        );
    }

    println!("\nWaiting 100ms for new window...");
    std::thread::sleep(Duration::from_millis(100));

    let allowed = sliding.try_acquire("user-1");
    println!(
        "  Request after wait: {} (remaining: {})\n",
        if allowed { "✓" } else { "✗" },
        sliding.get_remaining("user-1")
    );

    // Demo 3: Different clients
    println!("\n  ═══ Distributed Rate Limiting (per-client) ═══");
    let distributed = DistributedTokenBucket::new(3.0, 1.0);

    println!("Client A requests:");
    for _ in 0..4 {
        let allowed = distributed.try_acquire("client-a", 1.0);
        print!("{} ", if allowed { "✓" } else { "✗" });
    }

    println!("\nClient B requests (separate limit):");
    for _ in 0..4 {
        let allowed = distributed.try_acquire("client-b", 1.0);
        print!("{} ", if allowed { "✓" } else { "✗" });
    }
    println!();

    // Demo 4: Rules-based rate limiter
    println!("\n--- Rules-Based Rate Limiter ---");
    let mut limiter = RateLimiterWithRules::new();
    limiter.add_rule("/api/search", 10, Duration::from_secs(60));
    limiter.add_rule("/api/login", 5, Duration::from_secs(60));

    println!("Testing /api/search (limit: 10/min):");
    for i in 1..=12 {
        match limiter.check("/api/search", "user-123") {
            RateLimitResult::Allowed { remaining } => {
                println!("  Request {}: ✓ ({} remaining)", i, remaining);
            }
            RateLimitResult::Limited { retry_after } => {
                println!("  Request {}: ✗ (retry after {:?})", i, retry_after);
            }
        }
    }

    // Demo 5: Leaky Bucket
    println!("\n--- Leaky Bucket (capacity: 3, leak: 2/sec) ---");
    let leaky = LeakyBucket::new(3, 2.0);

    println!("Rapid requests:");
    for i in 1..=5 {
        let allowed = leaky.try_acquire();
        println!(
            "  Request {}: {} (queue: {})",
            i,
            if allowed { "✓" } else { "✗" },
            leaky.queue_size()
        );
    }

    println!("\nWaiting 1 second (2 should leak)...");
    std::thread::sleep(Duration::from_secs(1));
    println!("Queue after leak: {}", leaky.queue_size());

    // Demo 6: Show fixed window edge case
    println!("\n--- Fixed Window Edge Case Demo ---");
    println!("Fixed window can allow 2x traffic at boundaries!");
    println!("Example: 100 requests at :59, 100 at :01 = 200 in 2 seconds");
    println!("Use sliding window counter to avoid this.\n");

    println!("=== Demo Complete ===");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(5.0, 1.0);

        // Should allow first 5 requests
        for _ in 0..5 {
            assert!(bucket.try_acquire(1.0));
        }

        // 6th should fail
        assert!(!bucket.try_acquire(1.0));
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(2.0, 10.0); // Fast refill

        // Drain bucket
        assert!(bucket.try_acquire(2.0));
        assert!(!bucket.try_acquire(1.0));

        // Wait for refill
        std::thread::sleep(Duration::from_millis(200));

        // Should have tokens now
        assert!(bucket.try_acquire(1.0));
    }

    #[test]
    fn test_sliding_window() {
        let counter = SlidingWindowCounter::new(Duration::from_millis(100), 3);

        // Should allow first 3
        assert!(counter.try_acquire("test"));
        assert!(counter.try_acquire("test"));
        assert!(counter.try_acquire("test"));

        // 4th should fail
        assert!(!counter.try_acquire("test"));
    }

    #[test]
    fn test_different_clients() {
        let limiter = DistributedTokenBucket::new(2.0, 1.0);

        // Each client has their own limit
        assert!(limiter.try_acquire("client-a", 1.0));
        assert!(limiter.try_acquire("client-a", 1.0));
        assert!(!limiter.try_acquire("client-a", 1.0)); // Limited

        // Client B still has full quota
        assert!(limiter.try_acquire("client-b", 1.0));
        assert!(limiter.try_acquire("client-b", 1.0));
    }
}
