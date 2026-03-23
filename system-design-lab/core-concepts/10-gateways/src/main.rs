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
// 2. Rate Limiting (Token Bucket)
// =============================================================================

struct RateLimiter {
    buckets: DashMap<String, (u32, u64)>, // key → (tokens, last_refill_timestamp)
    max_tokens: u32,
    refill_rate: u32, // tokens per second
}

impl RateLimiter {
    fn new(max_tokens: u32, refill_rate: u32) -> Self {
        Self {
            buckets: DashMap::new(),
            max_tokens,
            refill_rate,
        }
    }

    fn allow(&self, key: &str) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let mut entry = self.buckets.entry(key.to_string()).or_insert((self.max_tokens, now));
        let (tokens, last_refill) = entry.value_mut();

        // Refill tokens based on time elapsed
        let elapsed = now - *last_refill;
        if elapsed > 0 {
            *tokens = (*tokens + (elapsed as u32 * self.refill_rate)).min(self.max_tokens);
            *last_refill = now;
        }

        if *tokens > 0 {
            *tokens -= 1;
            true
        } else {
            false
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

    // Demo 2: Rate Limiting
    println!("━━━ 2. Rate Limiting — Token Bucket ━━━");
    demo_rate_limiting();

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
