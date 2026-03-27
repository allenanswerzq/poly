#![allow(dead_code, unused_variables, unused_imports, clippy::all)]
//! # Networking Essentials Demo
//!
//! Each protocol is in its own module:
//! - `tcp` — TCP echo server/client
//! - `http_server` — shared axum server
//! - `http1` — HTTP/1.1 keep-alive, HOL blocking, connection pooling
//! - `http2` — HTTP/2 multiplexing, streaming
//! - `quic` — HTTP/3 / QUIC over UDP
//! - `websocket` — WebSocket full-duplex
//! - `sse` — Server-Sent Events
//! - `grpc` — gRPC-style RPC over HTTP/2
//! - `tls` — TLS 1.3 handshake
//! - `pool` — Connection pooling concept

mod grpc;
mod http1;
mod http2;
mod http_server;
mod pool;
mod quic;
mod sse;
mod tcp;
mod tls;
mod websocket;

use std::net::TcpListener;
use std::thread;
use std::time::Duration;

fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    println!("╔══════════════════════════════════════════════════╗");
    println!("║       Networking Essentials — Full Demo          ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    // ── 1. TCP Echo ──────────────────────────────────────────────────────
    println!("━━━ 1. TCP Echo Server/Client ━━━");
    let echo_addr = "127.0.0.1:9001";
    let _echo_server = thread::spawn(move || tcp::run_echo_server(echo_addr).ok());
    thread::sleep(Duration::from_millis(50));
    tcp::run_echo_client(echo_addr, &["Hello", "World", "Goodbye"]).ok();
    println!();

    // ── 2. HTTP/1.1 ─────────────────────────────────────────────────────
    println!("━━━ 2. HTTP/1.1 — Real Server (axum) + Real Client (reqwest) ━━━");
    let http_addr = "127.0.0.1:9002";
    http_server::start_http_server(http_addr);
    thread::sleep(Duration::from_millis(200));
    let base_url = format!("http://{}", http_addr);
    http1::demo_keepalive(&base_url);
    http1::demo_hol_blocking(&base_url);

    // ── 3. HTTP/2 Multiplexing ──────────────────────────────────────────
    println!("━━━ 3. HTTP/2 — Real Multiplexing (10 concurrent requests) ━━━");
    http2::demo_multiplexing(&base_url);

    // ── 3b. HTTP/2 Streaming ────────────────────────────────────────────
    println!("━━━ 3b. HTTP/2 — Streaming Large Payloads ━━━");
    http2::demo_streaming(&base_url);

    // ── 4. HTTP/3 / QUIC ────────────────────────────────────────────────
    println!("━━━ 4. HTTP/3 / QUIC — Real QUIC over UDP (quinn) ━━━");
    quic::demo();

    // ── 5. WebSocket ────────────────────────────────────────────────────
    println!("━━━ 5. WebSocket — Real Full-Duplex (tokio-tungstenite) ━━━");
    websocket::demo();

    // ── 6. Server-Sent Events ───────────────────────────────────────────
    println!("━━━ 6. Server-Sent Events (SSE) — Server Push ━━━");
    sse::demo(&base_url);

    // ── 7. gRPC-style RPC ───────────────────────────────────────────────
    println!("━━━ 7. gRPC — RPC over HTTP/2 ━━━");
    grpc::demo(&base_url);

    // ── 8. TLS ──────────────────────────────────────────────────────────
    println!("━━━ 8. TLS — Real TLS 1.3 Handshake (rustls) ━━━");
    tls::demo();

    // ── 9. Connection Pool ──────────────────────────────────────────────
    println!("━━━ 9. Connection Pool ━━━");
    let pool_addr = "127.0.0.1:9006";
    let _pool_server = thread::spawn(move || {
        let listener = TcpListener::bind(pool_addr).unwrap();
        for stream in listener.incoming().take(3) {
            stream.ok();
        }
    });
    thread::sleep(Duration::from_millis(50));

    let mut conn_pool = pool::ConnectionPool::new(2);
    if let Ok(conn1) = conn_pool.get(pool_addr) {
        conn_pool.put(conn1);
    }
    if let Ok(_conn2) = conn_pool.get(pool_addr) {
        // Reused from pool
    }
    println!();

    // ── Protocol Comparison ─────────────────────────────────────────────
    println!("━━━ Protocol Comparison ━━━");
    println!(
        "
┌──────────────┬───────────┬──────────────┬──────────┬─────────────┐
│ Protocol     │ Transport │ Duplex       │ Overhead │ Best For    │
├──────────────┼───────────┼──────────────┼──────────┼─────────────┤
│ HTTP/1.1     │ TCP       │ Half (req →) │ Text hdr │ Simple APIs │
│ HTTP/2       │ TCP       │ Multiplexed  │ Binary   │ Modern web  │
│ HTTP/3       │ UDP/QUIC  │ Multiplexed  │ Binary   │ Mobile/lossy│
│ WebSocket    │ TCP       │ Full-duplex  │ 2-6 byte │ Real-time   │
│ SSE          │ TCP/HTTP  │ Server push  │ Text     │ Live feeds  │
│ gRPC         │ HTTP/2    │ Bi-stream    │ Protobuf │ Microsvcs   │
└──────────────┴───────────┴──────────────┴──────────┴─────────────┘

When to use what:
• REST API, CRUD ops          → HTTP/1.1 or HTTP/2
• Dashboard with live updates → SSE
• Chat, gaming, collaboration → WebSocket
• Mobile app, poor network    → HTTP/3 (QUIC)
• Internal microservices      → gRPC over HTTP/2
• Streaming large files       → HTTP/2 with flow control
"
    );

    // ── Latency Breakdown ───────────────────────────────────────────────
    println!("━━━ Latency Breakdown ━━━");
    println!(
        "
HTTP/1.0: New TCP conn per request
  DNS(50ms) + TCP(50ms) + TLS(100ms) + Request(50ms) = ~250ms EACH

HTTP/1.1: Keep-alive (reuse TCP conn)
  First:  DNS(50ms) + TCP(50ms) + TLS(100ms) + Request(50ms) = ~250ms
  Next:   Request(50ms) = ~50ms  ← connection reused!

HTTP/2: Multiplexed streams
  First:  DNS(50ms) + TCP(50ms) + TLS(100ms) + Request(50ms) = ~250ms
  Next:   All requests in parallel on same conn = ~50ms total

HTTP/3: QUIC (0-RTT possible)
  First:  DNS(50ms) + QUIC handshake(0-50ms) + Request(50ms) = ~100-150ms
  Resume: 0-RTT! Request(50ms) = ~50ms  ← no handshake at all!

WebSocket: After initial handshake
  Handshake: DNS + TCP + TLS + Upgrade = ~250ms (one time)
  Messages:  ~1ms per frame (no HTTP overhead, just 2-6 byte header)
"
    );

    thread::sleep(Duration::from_millis(500));
    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
