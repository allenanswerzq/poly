//! # Gateways Demo
//!
//! Demonstrates core gateway patterns with real implementations:
//! 1. Reverse proxy with path-based routing
//! 2. Rate limiting middleware (token bucket)
//! 3. Authentication middleware (API key validation)
//! 4. Circuit breaker (detect and isolate failing backends)
//! 5. Request aggregation (fan-out + combine)

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use dashmap::DashMap;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// =============================================================================
// Backend Services (simulated microservices)
// =============================================================================

fn start_backend_services() {
    // User service on :9101
    thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let app = axum::Router::new()
                .route("/users/:id", axum::routing::get(|Path(id): Path<u32>| async move {
                    Json(json!({"service": "user-service", "user_id": id, "name": "Alice", "email": "alice@example.com"}))
                }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:9101").await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });

    // Order service on :9102
    thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let app = axum::Router::new()
                .route("/orders/:id", axum::routing::get(|Path(id): Path<u32>| async move {
                    Json(json!({"service": "order-service", "order_id": id, "items": ["Widget", "Gadget"], "total": 42.99}))
                }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:9102").await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });

    // Flaky service on :9103 (fails 50% of the time — for circuit breaker demo)
    thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let counter = Arc::new(AtomicU32::new(0));
        rt.block_on(async {
            let counter = counter.clone();
            let app = axum::Router::new()
                .route("/data", axum::routing::get(move || {
                    let count = counter.fetch_add(1, Ordering::Relaxed);
                    async move {
                        if count % 2 == 0 {
                            // Simulate failure
                            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "service down"})))
                        } else {
                            (StatusCode::OK, Json(json!({"service": "flaky-service", "data": "success"})))
                        }
                    }
                }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:9103").await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });

    thread::sleep(Duration::from_millis(200));
    println!("  [Backend] User service on :9101, Order service on :9102, Flaky service on :9103\n");
}

// =============================================================================
// 1. Reverse Proxy with Path-Based Routing
// =============================================================================

fn demo_reverse_proxy() {
    println!("\n  ═══ demo_reverse_proxy ═══\n");
    println!("  Gateway routes requests to different backends based on URL path:\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // Start gateway with route table
        let routes: Arc<Vec<(&str, &str)>> = Arc::new(vec![
            ("/api/users", "http://127.0.0.1:9101/users"),
            ("/api/orders", "http://127.0.0.1:9102/orders"),
        ]);

        let gateway = axum::Router::new()
            .route("/api/users/:id", axum::routing::get({
                move |Path(id): Path<u32>| async move {
                    let client = reqwest::Client::new();
                    let resp = client.get(format!("http://127.0.0.1:9101/users/{}", id))
                        .send().await.unwrap();
                    let body: Value = resp.json().await.unwrap();
                    Json(body)
                }
            }))
            .route("/api/orders/:id", axum::routing::get({
                move |Path(id): Path<u32>| async move {
                    let client = reqwest::Client::new();
                    let resp = client.get(format!("http://127.0.0.1:9102/orders/{}", id))
                        .send().await.unwrap();
                    let body: Value = resp.json().await.unwrap();
                    Json(body)
                }
            }));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:9100").await.unwrap();
        let server = tokio::spawn(async { axum::serve(listener, gateway).await.unwrap() });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();

        // Client talks to ONE gateway address — doesn't know about backend services
        for (path, desc) in [("/api/users/1", "User Service"), ("/api/orders/42", "Order Service")] {
            let start = Instant::now();
            let resp = client.get(format!("http://127.0.0.1:9100{}", path))
                .send().await.unwrap();
            let body: Value = resp.json().await.unwrap();
            println!("    GET {} → {} → {:?} ({:?})",
                path, desc, body.get("service").unwrap(), start.elapsed());
        }

        println!("\n    Client sees ONE endpoint (gateway:9100).");
        println!("    Gateway routes /api/users → :9101, /api/orders → :9102.");
        println!("    Backend topology is hidden from the client.\n");
        server.abort();
    });
}

// =============================================================================
// 1b. Real Proxy — Pingora (Cloudflare's production proxy framework)
// =============================================================================

fn demo_pingora_proxy() {
    use pingora::prelude::*;
    use pingora::proxy::{ProxyHttp, Session};
    use pingora::upstreams::peer::HttpPeer;
    use pingora::lb::{selection::RoundRobin, LoadBalancer};

    println!("\n  ═══ demo_pingora_proxy ═══\n");
    println!("  Running a REAL reverse proxy using Cloudflare's Pingora framework.\n");

    struct RoutingProxy {
        user_upstream: Arc<LoadBalancer<RoundRobin>>,
        order_upstream: Arc<LoadBalancer<RoundRobin>>,
    }

    #[async_trait::async_trait]
    impl ProxyHttp for RoutingProxy {
        type CTX = ();
        fn new_ctx(&self) -> Self::CTX {}

        async fn upstream_peer(
            &self,
            session: &mut Session,
            _ctx: &mut Self::CTX,
        ) -> Result<Box<HttpPeer>> {
            let path = session.req_header().uri.path();

            let upstream = if path.starts_with("/api/users") {
                self.user_upstream.select(b"", 256)
            } else if path.starts_with("/api/orders") {
                self.order_upstream.select(b"", 256)
            } else {
                None
            };

            let upstream =
                upstream.ok_or_else(|| pingora::Error::new_str("no route matched"))?;

            let peer = HttpPeer::new(upstream, false, String::new());
            Ok(Box::new(peer))
        }

        async fn upstream_request_filter(
            &self,
            _session: &mut Session,
            upstream_request: &mut pingora::http::RequestHeader,
            _ctx: &mut Self::CTX,
        ) -> Result<()> {
            // Rewrite: /api/users/1 → /users/1 (strip /api prefix)
            let path = upstream_request.uri.path().to_string();
            if let Some(rest) = path.strip_prefix("/api") {
                let new_uri = rest.parse().unwrap();
                upstream_request.set_uri(new_uri);
            }
            upstream_request.insert_header("X-Forwarded-By", "pingora-gateway")?;
            Ok(())
        }
    }

    // Start Pingora in a background thread (run_forever blocks + calls exit)
    thread::spawn(|| {
        let mut server = Server::new(None).unwrap();
        server.bootstrap();

        let users = LoadBalancer::try_from_iter(["127.0.0.1:9101"]).unwrap();
        let orders = LoadBalancer::try_from_iter(["127.0.0.1:9102"]).unwrap();

        let proxy = RoutingProxy {
            user_upstream: Arc::new(users),
            order_upstream: Arc::new(orders),
        };

        let mut svc = pingora::proxy::http_proxy_service(&server.configuration, proxy);
        svc.add_tcp("127.0.0.1:6188");

        server.add_service(svc);
        server.run(pingora::server::RunArgs::default());
    });

    // Wait for Pingora to bind the port
    thread::sleep(Duration::from_secs(2));

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    println!("    Pingora proxy on :6188 → backends on :9101, :9102\n");

    // Test: /api/users/1
    match client.get("http://127.0.0.1:6188/api/users/1").send() {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().unwrap_or_default();
            println!("    GET /api/users/1   → {} service={:?}",
                status, body.get("service").unwrap_or(&json!("?")));
        }
        Err(e) => println!("    GET /api/users/1   → ERROR: {}", e),
    }

    // Test: /api/orders/42
    match client.get("http://127.0.0.1:6188/api/orders/42").send() {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().unwrap_or_default();
            println!("    GET /api/orders/42  → {} service={:?}",
                status, body.get("service").unwrap_or(&json!("?")));
        }
        Err(e) => println!("    GET /api/orders/42  → ERROR: {}", e),
    }

    // Test: unknown route
    match client.get("http://127.0.0.1:6188/api/unknown").send() {
        Ok(resp) => println!("    GET /api/unknown   → {} (no route)", resp.status()),
        Err(e) => println!("    GET /api/unknown   → ERROR: {}", e),
    }

    println!("\n    This is Cloudflare's Pingora — the same framework powering their edge.");
    println!("    Path routing, load balancing, connection pooling, all built-in.\n");
}

// =============================================================================
// 2. Rate Limiting (Token Bucket)
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

        // Look up this client's bucket, or create a new one starting full
        let mut entry = self.buckets.entry(key.to_string()).or_insert((self.max_tokens, now));
        let (tokens, last_refill) = entry.value_mut();

        // Step 1: Refill — add tokens based on how much time has passed
        // e.g., 3 seconds elapsed × 2 tokens/sec = 6 new tokens
        let elapsed = now - *last_refill;
        if elapsed > 0 {
            *tokens = (*tokens + (elapsed as u32 * self.refill_rate)).min(self.max_tokens);
            //                                                       ^^^^ never exceed capacity
            *last_refill = now;
        }

        // Step 2: Try to consume one token
        if *tokens > 0 {
            *tokens -= 1;  // spend a token
            true           // request allowed
        } else {
            false          // no tokens left → 429 Too Many Requests
        }
    }
}

fn demo_rate_limiting() {
    println!("\n  ═══ demo_rate_limiting ═══\n");
    println!("  Token bucket rate limiter at the gateway (5 tokens, 2/sec refill):\n");

    let limiter = RateLimiter::new(5, 2);

    // Burst of 8 requests from same client
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

    // Different clients have separate buckets
    println!("    Different clients have separate buckets:");
    let a = limiter.allow("client-B");
    let b = limiter.allow("client-C");
    println!("    client-B: {} (fresh bucket)", if a { "✓" } else { "✗" });
    println!("    client-C: {} (fresh bucket)\n", if b { "✓" } else { "✗" });
}

// =============================================================================
// 2b. Rate Limiting Algorithm Comparison
// =============================================================================

// ── Fixed Window Counter ──
// Divide time into fixed windows (e.g., 1-second windows).
// Count requests per window. Reject if count exceeds limit.
//
// Problem: boundary burst. 5 requests at 0.9s + 5 at 1.1s = 10 in 0.2s
//
//   Window 1 [0s─1s]: ●●●●●   5/5 (ok)
//   Window 2 [1s─2s]: ●●●●●   5/5 (ok)
//                   ↑ but 10 requests in 0.2s if clustered at boundary!
struct FixedWindowLimiter {
    // key → (count_in_current_window, window_start_timestamp)
    windows: DashMap<String, (u32, u64)>,
    max_requests: u32,    // max requests per window
    window_secs: u64,     // window duration in seconds
}

impl FixedWindowLimiter {
    fn new(max_requests: u32, window_secs: u64) -> Self {
        Self { windows: DashMap::new(), max_requests, window_secs }
    }

    fn allow(&self, key: &str, now: u64) -> bool {
        let window_start = now / self.window_secs * self.window_secs;

        let mut entry = self.windows.entry(key.to_string()).or_insert((0, window_start));
        let (count, entry_window) = entry.value_mut();

        // New window? Reset counter.
        if *entry_window != window_start {
            *count = 0;
            *entry_window = window_start;
        }

        if *count < self.max_requests {
            *count += 1;
            true
        } else {
            false   // window limit exceeded → 429
        }
    }
}

// ── Sliding Window Log ──
// Store the timestamp of every request. Count those within the last N seconds.
// Most accurate, but uses O(n) memory per client.
//
//   Timestamps: [0.1, 0.3, 0.5, 0.8, 0.9, 1.1, 1.2]
//   Window = last 1s from now=1.2:  count timestamps >= 0.2 → 6 requests
//
// In production at 10K req/s: 10K timestamps per client per second = memory bomb
struct SlidingWindowLogLimiter {
    // key → vec of request timestamps (milliseconds)
    logs: DashMap<String, Vec<u64>>,
    max_requests: u32,
    window_ms: u64,       // window size in milliseconds
}

impl SlidingWindowLogLimiter {
    fn new(max_requests: u32, window_ms: u64) -> Self {
        Self { logs: DashMap::new(), max_requests, window_ms }
    }

    fn allow(&self, key: &str, now_ms: u64) -> bool {
        let mut entry = self.logs.entry(key.to_string()).or_insert_with(Vec::new);
        let timestamps = entry.value_mut();

        // Remove expired timestamps (outside the window)
        let cutoff = now_ms.saturating_sub(self.window_ms);
        timestamps.retain(|&t| t > cutoff);

        if (timestamps.len() as u32) < self.max_requests {
            timestamps.push(now_ms);  // Record this request
            true
        } else {
            false   // too many requests in window → 429
        }
        // Note: timestamps vec keeps growing until cleanup — this is the memory problem
    }
}

// ── Leaky Bucket ──
// Requests enter a queue that "leaks" (processes) at a fixed rate.
// If the queue is full, reject. No bursts allowed — strict fixed rate.
//
//   ┌─────────────┐
//   │ ● ● ● ● ●   │  queue (capacity 5)
//   └─────┬───────┘
//         ▼ drip at 2/sec (fixed, always)
//
// Unlike token bucket: even if queue is empty, output rate is still fixed.
// This means requests experience DELAY (queuing), not just accept/reject.
// For simplicity we implement the "reject when full" variant (no actual queue).
struct LeakyBucketLimiter {
    // key → (water_level, last_leak_timestamp_ms)
    buckets: DashMap<String, (f64, u64)>,
    capacity: f64,        // max water level (queue size)
    leak_rate: f64,       // how many units leak per second
}

impl LeakyBucketLimiter {
    fn new(capacity: f64, leak_rate: f64) -> Self {
        Self { buckets: DashMap::new(), capacity, leak_rate }
    }

    fn allow(&self, key: &str, now_ms: u64) -> bool {
        let mut entry = self.buckets.entry(key.to_string()).or_insert((0.0, now_ms));
        let (water, last_leak) = entry.value_mut();

        // Leak water based on elapsed time
        let elapsed_secs = (now_ms - *last_leak) as f64 / 1000.0;
        *water = (*water - elapsed_secs * self.leak_rate).max(0.0);
        *last_leak = now_ms;

        // Try to add 1 unit of water (1 request)
        if *water + 1.0 <= self.capacity {
            *water += 1.0;
            true    // fits in the bucket
        } else {
            false   // bucket overflowing → 429
        }
    }
}

fn demo_rate_limiting_comparison() {
    println!("\n  ═══ demo_rate_limiting_comparison ═══\n");
    println!("  Comparing 4 rate limiting algorithms side by side:\n");

    // --- Fixed Window ---
    println!("  ── 1. Fixed Window Counter (5 req per 1s window) ──\n");
    let fw = FixedWindowLimiter::new(5, 1);
    // All requests in "same second" (window 0-1s)
    for i in 1..=7 {
        let allowed = fw.allow("client", 0); // all at timestamp 0 (same window)
        println!("    Req #{} at t=0.{}s: {}", i, i, if allowed { "✓" } else { "✗ REJECTED" });
    }
    // Now show the boundary problem: requests at end of window 1 + start of window 2
    println!("\n    Boundary problem:");
    let fw2 = FixedWindowLimiter::new(5, 10); // 5 req per 10s window
    // 5 requests at t=9 (end of window [0-10))
    for i in 1..=5 {
        fw2.allow("client", 9);
        if i == 5 { println!("    5 requests at t=9s  (end of window 1)   → all ✓"); }
    }
    // 5 more at t=10 (start of new window [10-20)) — counter resets!
    for i in 1..=5 {
        fw2.allow("client", 10);
        if i == 5 { println!("    5 requests at t=10s (start of window 2) → all ✓"); }
    }
    println!("    → 10 requests in 1 second! Limit was 5/10s. That's the bug.\n");

    // --- Sliding Window Log ---
    println!("  ── 2. Sliding Window Log (5 req per 1000ms) ──\n");
    let sw = SlidingWindowLogLimiter::new(5, 1000);
    // Spread over time
    let times = [100, 200, 400, 600, 800, 900, 950];
    for (i, &t) in times.iter().enumerate() {
        let allowed = sw.allow("client", t);
        println!("    Req #{} at t={}ms: {}", i + 1, t, if allowed { "✓" } else { "✗ REJECTED" });
    }
    // Show memory usage
    let log_size = sw.logs.get("client").map(|v| v.len()).unwrap_or(0);
    println!("\n    Stored {} timestamps in memory (grows with every request!)", log_size);
    println!("    At 10K req/s → 10,000 timestamps per client per second.\n");

    // --- Leaky Bucket ---
    println!("  ── 3. Leaky Bucket (capacity 5, leak 2/sec) ──\n");
    let lb = LeakyBucketLimiter::new(5.0, 2.0);
    // Burst: 7 requests at once (t=0)
    for i in 1..=7 {
        let allowed = lb.allow("client", 0);
        println!("    Req #{} at t=0ms:    {}{}", i,
            if allowed { "✓" } else { "✗ REJECTED" },
            if i == 5 { " ← bucket full" } else { "" });
    }
    // Wait 1 second → 2 units leak out
    println!("\n    ...1 second passes (2 units leak out)...\n");
    for i in 1..=3 {
        let allowed = lb.allow("client", 1000);
        println!("    Req #{} at t=1000ms: {}{}", i + 7,
            if allowed { "✓" } else { "✗ REJECTED" },
            if !allowed { " ← only 2 leaked, bucket full again" } else { "" });
    }
    println!("\n    Leaky bucket: strict fixed output rate. No bursts.\n");

    // --- Side by side summary ---
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

// =============================================================================
// 3. Authentication Middleware
// =============================================================================

fn demo_auth_middleware() {
    println!("\n  ═══ demo_auth_middleware ═══\n");
    println!("  Gateway validates API keys before forwarding to backend:\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // Valid API keys (in production: database or Redis lookup)
        let valid_keys: Arc<DashMap<String, String>> = Arc::new(DashMap::new());
        valid_keys.insert("sk-valid-key-123".into(), "user-42".into());
        valid_keys.insert("sk-admin-key-456".into(), "admin-1".into());

        let keys = valid_keys.clone();
        let auth_gateway = axum::Router::new()
            .route("/api/users/:id", axum::routing::get(move |
                Path(id): Path<u32>,
                headers: HeaderMap,
            | {
                let keys = keys.clone();
                async move {
                    // Check API key
                    let api_key = headers.get("x-api-key")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");

                    match keys.get(api_key) {
                        Some(user_id) => {
                            // Forward to backend with authenticated user ID
                            let client = reqwest::Client::new();
                            let resp = client.get(format!("http://127.0.0.1:9101/users/{}", id))
                                .header("X-Authenticated-User", user_id.value().clone())
                                .send().await.unwrap();
                            let body: Value = resp.json().await.unwrap();
                            (StatusCode::OK, Json(json!({
                                "authenticated_as": user_id.value(),
                                "data": body
                            })))
                        }
                        None => {
                            (StatusCode::UNAUTHORIZED, Json(json!({
                                "error": "Invalid API key",
                                "hint": "Set X-Api-Key header"
                            })))
                        }
                    }
                }
            }));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:9104").await.unwrap();
        let server = tokio::spawn(async { axum::serve(listener, auth_gateway).await.unwrap() });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();

        // No API key → 401
        let resp = client.get("http://127.0.0.1:9104/api/users/1").send().await.unwrap();
        println!("    No API key:      {} {}", resp.status(), resp.text().await.unwrap());

        // Invalid key → 401
        let resp = client.get("http://127.0.0.1:9104/api/users/1")
            .header("X-Api-Key", "sk-invalid")
            .send().await.unwrap();
        println!("    Invalid key:     {} {}", resp.status(), resp.text().await.unwrap());

        // Valid key → 200 + forwarded
        let resp = client.get("http://127.0.0.1:9104/api/users/1")
            .header("X-Api-Key", "sk-valid-key-123")
            .send().await.unwrap();
        println!("    Valid key:       {} (forwarded to backend)", resp.status());

        println!("\n    Gateway validates auth ONCE. Backend trusts X-Authenticated-User header.\n");
        server.abort();
    });
}

// =============================================================================
// 4. Circuit Breaker
// =============================================================================

struct CircuitBreaker {
    failure_count: AtomicU32,
    threshold: u32,
    last_failure: AtomicU64,
    cooldown_secs: u64,
}

impl CircuitBreaker {
    fn new(threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            threshold,
            last_failure: AtomicU64::new(0),
            cooldown_secs,
        }
    }

    fn is_open(&self) -> bool {
        let failures = self.failure_count.load(Ordering::Relaxed);
        if failures < self.threshold {
            return false;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let last = self.last_failure.load(Ordering::Relaxed);
        now - last < self.cooldown_secs
    }

    fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
    }

    fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        self.last_failure.store(now, Ordering::Relaxed);
    }
}

fn demo_circuit_breaker() {
    println!("\n  ═══ demo_circuit_breaker ═══\n");
    println!("  Circuit breaker protects against cascading failures:\n");

    let cb = CircuitBreaker::new(3, 5); // Open after 3 failures, cooldown 5s
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap();

    for i in 1..=8 {
        if cb.is_open() {
            println!("    Request #{}: ⚡ CIRCUIT OPEN — skipped (returning 503)", i);
            continue;
        }

        match client.get("http://127.0.0.1:9103/data").send() {
            Ok(resp) if resp.status().is_success() => {
                cb.record_success();
                println!("    Request #{}: ✓ Success (circuit closed)", i);
            }
            _ => {
                cb.record_failure();
                let failures = cb.failure_count.load(Ordering::Relaxed);
                println!("    Request #{}: ✗ Failure (failures: {}/{}{})",
                    i, failures, cb.threshold,
                    if failures >= cb.threshold { " → CIRCUIT OPENS!" } else { "" }
                );
            }
        }
    }

    println!("\n    States: CLOSED (normal) → OPEN (stop sending) → HALF-OPEN (try one)");
    println!("    Prevents: cascading failures, resource exhaustion, thundering herd\n");
}

// =============================================================================
// 5. Request Aggregation (BFF Pattern)
// =============================================================================

fn demo_request_aggregation() {
    println!("\n  ═══ demo_request_aggregation ═══\n");
    println!("  Gateway fans out to multiple services, combines responses:\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();
        let user_id = 1;
        let order_id = 42;

        // Without gateway: 2 sequential requests from client
        println!("    WITHOUT gateway (client makes 2 requests):");
        let start = Instant::now();
        let user = client.get(format!("http://127.0.0.1:9101/users/{}", user_id))
            .send().await.unwrap().json::<Value>().await.unwrap();
        let order = client.get(format!("http://127.0.0.1:9102/orders/{}", order_id))
            .send().await.unwrap().json::<Value>().await.unwrap();
        println!("      Request 1: user → {:?}", user.get("name"));
        println!("      Request 2: order → {:?}", order.get("total"));
        println!("      Total: {:?} (sequential)\n", start.elapsed());

        // With gateway aggregation: 1 request, gateway fans out in parallel
        println!("    WITH gateway aggregation (1 request, parallel fan-out):");
        let start = Instant::now();
        let (user, order) = tokio::join!(
            async {
                client.get(format!("http://127.0.0.1:9101/users/{}", user_id))
                    .send().await.unwrap().json::<Value>().await.unwrap()
            },
            async {
                client.get(format!("http://127.0.0.1:9102/orders/{}", order_id))
                    .send().await.unwrap().json::<Value>().await.unwrap()
            },
        );
        let combined = json!({
            "user": user,
            "order": order,
        });
        println!("      Combined response: user={}, order={}",
            user.get("name").unwrap(), order.get("total").unwrap());
        println!("      Total: {:?} (parallel fan-out)\n", start.elapsed());

        println!("    On mobile (100ms RTT): 2 requests = 200ms, 1 aggregated = 100ms");
        println!("    This is the BFF (Backend for Frontend) pattern.\n");
    });
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║       Gateways — API Gateway Deep Dive           ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    // Start backend services
    println!("━━━ Starting Backend Services ━━━");
    start_backend_services();

    // Demo 1: Reverse Proxy
    println!("━━━ 1. Reverse Proxy — Path-Based Routing ━━━");
    demo_reverse_proxy();

    // Demo 1b: Real Proxy — Pingora
    println!("━━━ 1b. Real Proxy — Cloudflare Pingora ━━━");
    demo_pingora_proxy();

    // Demo 2: Rate Limiting
    println!("━━━ 2. Rate Limiting — Token Bucket ━━━");
    demo_rate_limiting();

    // Demo 2b: All Rate Limiting Algorithms Compared
    println!("━━━ 2b. Rate Limiting — Algorithm Comparison ━━━");
    demo_rate_limiting_comparison();

    // Demo 3: Auth Middleware
    println!("━━━ 3. Authentication — API Key Validation ━━━");
    demo_auth_middleware();

    // Demo 4: Circuit Breaker
    println!("━━━ 4. Circuit Breaker — Failure Isolation ━━━");
    demo_circuit_breaker();

    // Demo 5: Request Aggregation
    println!("━━━ 5. Request Aggregation — BFF Pattern ━━━");
    demo_request_aggregation();

    // Summary
    println!("━━━ Gateway Summary ━━━");
    println!("
┌──────────────────┬────────────────────────────────────────────┐
│ Pattern          │ What it does                               │
├──────────────────┼────────────────────────────────────────────┤
│ Reverse Proxy    │ Hide backends, route by path/host          │
│ Rate Limiting    │ Prevent abuse (token bucket per client)    │
│ Authentication   │ Validate tokens once, forward user ID      │
│ Circuit Breaker  │ Stop sending to failing backends           │
│ Aggregation      │ Fan-out + combine (reduce client RTTs)     │
│ SSL Termination  │ HTTPS outside, HTTP inside                 │
│ Caching          │ Cache responses at the edge                │
│ Load Balancing   │ Distribute across backend replicas         │
└──────────────────┴────────────────────────────────────────────┘

Real-world: nginx (reverse proxy) → Kong/Envoy (API gateway) → Istio (service mesh)
");

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
