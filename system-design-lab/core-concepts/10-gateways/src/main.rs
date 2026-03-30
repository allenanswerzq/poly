//! # Gateways Demo
//!
//! Demonstrates core gateway patterns with real implementations:
//! 1. Reverse proxy with path-based routing
//! 2. Pingora reverse proxy (Cloudflare's production framework)
//! 3. Pingora with authentication middleware
//! 4. Pingora with circuit breaker
//! 5. Rate limiting (token bucket + algorithm comparison)
//! 6. Authentication middleware (axum)
//! 7. Circuit breaker
//! 8. Request aggregation (BFF pattern)

mod aggregation;
mod auth_middleware;
mod backends;
mod canary;
mod circuit_breaker;
mod load_balancing;
mod pingora_auth;
mod pingora_cb;
mod pingora_proxy;
mod rate_limiting;
mod reverse_proxy;

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║       Gateways — API Gateway Deep Dive           ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    // Start backend services
    println!("━━━ Starting Backend Services ━━━");
    backends::start_backend_services();

    // Demo 1: Reverse Proxy
    println!("━━━ 1. Reverse Proxy — Path-Based Routing ━━━");
    reverse_proxy::demo_reverse_proxy();

    // Demo 1b: Real Proxy — Pingora
    println!("━━━ 1b. Real Proxy — Cloudflare Pingora ━━━");
    pingora_proxy::demo_pingora_proxy();

    // Demo 1c: Pingora with Auth
    println!("━━━ 1c. Pingora Auth — API Key Middleware ━━━");
    pingora_auth::demo_pingora_auth();

    // Demo 1d: Pingora with Circuit Breaker
    println!("━━━ 1d. Pingora Circuit Breaker ━━━");
    pingora_cb::demo_pingora_circuit_breaker();

    // Demo 2: Rate Limiting
    println!("━━━ 2. Rate Limiting — Token Bucket ━━━");
    rate_limiting::demo_rate_limiting();

    // Demo 2b: All Rate Limiting Algorithms Compared
    println!("━━━ 2b. Rate Limiting — Algorithm Comparison ━━━");
    rate_limiting::demo_rate_limiting_comparison();

    // Demo 3: Auth Middleware
    println!("━━━ 3. Authentication — API Key Validation ━━━");
    auth_middleware::demo_auth_middleware();

    // Demo 4: Circuit Breaker
    println!("━━━ 4. Circuit Breaker — Failure Isolation ━━━");
    circuit_breaker::demo_circuit_breaker();

    // Demo 5: Request Aggregation
    println!("━━━ 5. Request Aggregation — BFF Pattern ━━━");
    aggregation::demo_request_aggregation();

    // Demo 6: Load Balancing
    println!("━━━ 6. Load Balancing — Algorithms & Traffic Shifting ━━━");
    load_balancing::demo();

    // Demo 7: Weighted Routing / Canary
    println!("━━━ 7. Weighted Routing — Canary Deployment ━━━");
    canary::demo();

    // Summary
    println!("━━━ Gateway Summary ━━━");
    println!(
        "
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
│ Load Balancing   │ Distribute across backend replicas (WRR,   │
│                  │ least-conns, P2C, IP hash, etc.)           │
└──────────────────┴────────────────────────────────────────────┘

Real-world: nginx (reverse proxy) → Kong/Envoy (API gateway) → Istio (service mesh)
"
    );

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
