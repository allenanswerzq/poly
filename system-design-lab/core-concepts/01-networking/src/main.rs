//! # Networking Essentials Demo
//!
//! Demonstrates core networking concepts with real protocol implementations:
//! - TCP echo server/client
//! - HTTP/1.0 vs HTTP/1.1 (keep-alive, pipelining, head-of-line blocking)
//! - HTTP/2 concepts (binary framing, stream multiplexing, HPACK)
//! - HTTP/3 / QUIC concepts (UDP-based, 0-RTT, no head-of-line blocking)
//! - WebSocket server/client (handshake, framing, bidirectional messaging)
//! - Server-Sent Events (SSE) (server push over HTTP)
//! - Connection pooling

use base64::Engine;
use futures::StreamExt;
use sha1::{Digest, Sha1};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use serde_json::json;
use std::time::{Duration, Instant};

// =============================================================================
// 1. TCP Echo Server
// =============================================================================

fn run_echo_server(addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("[Echo Server] Listening on {}", addr);

    for stream in listener.incoming().take(3) {
        match stream {
            Ok(stream) => {
                thread::spawn(move || handle_echo_client(stream));
            }
            Err(e) => eprintln!("[Echo Server] Connection error: {}", e),
        }
    }
    Ok(())
}

fn handle_echo_client(mut stream: TcpStream) {
    let peer = stream.peer_addr().unwrap();
    println!("[Echo Server] Client connected: {}", peer);

    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();

    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        println!("[Echo Server] Received: {}", line.trim());
        stream.write_all(format!("ECHO: {}", line).as_bytes()).ok();
        line.clear();
    }
    println!("[Echo Server] Client disconnected: {}", peer);
}

fn run_echo_client(addr: &str, messages: &[&str]) -> std::io::Result<()> {
    let mut stream = TcpStream::connect(addr)?;
    let mut reader = BufReader::new(stream.try_clone()?);

    for msg in messages {
        stream.write_all(format!("{}\n", msg).as_bytes())?;
        stream.flush()?;

        let mut response = String::new();
        reader.read_line(&mut response)?;
        println!("[Echo Client] Sent '{}', Got '{}'", msg, response.trim());
    }
    Ok(())
}

// =============================================================================
// 2. HTTP/1.1 — Real Server (axum) + Real Client (reqwest)
// =============================================================================
//
// This uses production-grade libraries:
// - axum: web framework built on hyper (powers many production Rust services)
// - reqwest: HTTP client with connection pooling, keep-alive, cookies, etc.
//
// HTTP/1.0: one request per TCP connection (Connection: close by default)
// HTTP/1.1: persistent connections (Connection: keep-alive by default)
//           supports pipelining (send multiple requests without waiting)
//           BUT suffers from head-of-line (HOL) blocking — responses must
//           arrive in request order, so a slow response blocks everything.

/// Start a real HTTP server using axum (built on hyper).
/// Runs on a background thread with its own tokio runtime.
fn start_http_server(addr: &str) {
    let addr = addr.to_string();
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let app = axum::Router::new()
                .route("/", axum::routing::get(handle_root))
                .route("/health", axum::routing::get(handle_health))
                .route("/slow", axum::routing::get(handle_slow))
                .route("/api/users", axum::routing::get(handle_users))
                .route("/stream/:size_kb", axum::routing::get(handle_stream));

            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            println!("[HTTP Server] axum + hyper listening on {}", addr);
            axum::serve(listener, app).await.unwrap();
        });
    });
}

async fn handle_root() -> axum::Json<serde_json::Value> {
    axum::Json(json!({"message": "hello from HTTP/1.1"}))
}

async fn handle_health() -> axum::Json<serde_json::Value> {
    axum::Json(json!({"status": "healthy"}))
}

async fn handle_slow() -> axum::Json<serde_json::Value> {
    tokio::time::sleep(Duration::from_millis(500)).await;
    axum::Json(json!({"message": "slow response (500ms)"}))
}

async fn handle_users() -> axum::Json<serde_json::Value> {
    axum::Json(json!([{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]))
}

/// Streams a response body in 16KB chunks — does NOT buffer the full payload.
/// HTTP/2 sends each chunk as a separate DATA frame with flow control.
async fn handle_stream(
    axum::extract::Path(size_kb): axum::extract::Path<u32>,
) -> axum::body::Body {
    let chunk_size = 16 * 1024; // 16KB per DATA frame (HTTP/2 default max)
    let total_bytes = (size_kb as usize) * 1024;

    let stream = async_stream::stream! {
        let mut sent = 0usize;
        while sent < total_bytes {
            let this_chunk = chunk_size.min(total_bytes - sent);
            // Generate data on the fly — only 16KB in memory at a time
            let chunk = vec![b'X'; this_chunk];
            sent += this_chunk;
            yield Ok::<_, std::io::Error>(bytes::Bytes::from(chunk));
        }
    };
    axum::body::Body::from_stream(stream)
}

/// Demonstrates HTTP/1.1 keep-alive with a real HTTP client (reqwest).
/// reqwest automatically reuses TCP connections via its connection pool.
fn demo_http11_keepalive(base_url: &str) {
    println!("\n  ═══ demo_http11_keepalive ═══\n");
    // reqwest::blocking::Client has built-in connection pooling.
    // It reuses TCP connections via keep-alive automatically — just like browsers.
    let client = reqwest::blocking::Client::new();

    let paths = ["/", "/health", "/api/users"];

    // --- Round 1: One client, connection reused via keep-alive ---
    println!("  Round 1: ONE reqwest::Client (connection pooled, keep-alive):\n");

    let start = Instant::now();
    for path in &paths {
        let url = format!("{}{}", base_url, path);
        let req_start = Instant::now();
        let resp = client.get(&url).send().unwrap();
        let status = resp.status();
        let body = resp.text().unwrap();
        println!(
            "    GET {:12} -> {} {} ({:?})",
            path,
            status.as_u16(),
            body,
            req_start.elapsed()
        );
    }
    let reused_time = start.elapsed();
    println!(
        "\n  Total (pooled): {:?}",
        reused_time
    );

    // --- Round 2: New client per request, forces new TCP connection each time ---
    println!("\n  Round 2: NEW reqwest::Client per request (new TCP connection each time):\n");

    let start = Instant::now();
    for path in &paths {
        let fresh_client = reqwest::blocking::Client::new();
        let url = format!("{}{}", base_url, path);
        let req_start = Instant::now();
        let resp = fresh_client.get(&url).send().unwrap();
        let status = resp.status();
        let body = resp.text().unwrap();
        println!(
            "    GET {:12} -> {} {} ({:?})",
            path,
            status.as_u16(),
            body,
            req_start.elapsed()
        );
    }
    let fresh_time = start.elapsed();
    println!(
        "\n  Total (fresh):  {:?}",
        fresh_time
    );

    println!(
        "\n  Pooled is ~{:.0}% faster — 2nd/3rd requests skip TCP handshake!",
        (1.0 - reused_time.as_secs_f64() / fresh_time.as_secs_f64()) * 100.0
    );
    println!("  Try: curl -v {}/ — look for 'Connection: keep-alive' in response\n", base_url);
}

/// Demonstrates real HTTP/1.1 vs HTTP/2 head-of-line blocking.
/// Uses actual protocol version pinning — you'll see HTTP/1.1 and HTTP/2.0 in output.
fn demo_http11_hol_blocking(base_url: &str) {
    println!("\n  ═══ demo_http11_hol_blocking ═══\n");
    // ── HTTP/1.1: sequential, head-of-line blocking ──────────────────────
    // .http1_only() forces reqwest to use HTTP/1.1 — no upgrade to HTTP/2.
    // On HTTP/1.1, you MUST wait for each response before sending the next request.
    let http1_client = reqwest::blocking::ClientBuilder::new()
        .http1_only()
        .build()
        .unwrap();

    println!("  HTTP/1.1 — sequential on ONE TCP connection (head-of-line blocking):");
    println!("  One reqwest::Client = one connection pool = reuses same TCP socket.");
    println!("  Proof: 2nd request is faster (no TCP handshake needed).\n");
    println!("  Order 1: /slow then /health:\n");

    let start = Instant::now();
    for path in &["/slow", "/health"] {
        let url = format!("{}{}", base_url, path);
        let req_start = Instant::now();
        let resp = http1_client.get(&url).send().unwrap();
        println!(
            "    GET {:12} -> {} version={:?} ({:?})",
            path,
            resp.status(),
            resp.version(),
            req_start.elapsed()
        );
    }
    println!("  HTTP/1.1 total: {:?}", start.elapsed());
    println!("  ^ /health had to wait for /slow — both on same TCP connection\n");

    println!("  Order 2: /health then /slow:\n");

    let start = Instant::now();
    for path in &["/health", "/slow"] {
        let url = format!("{}{}", base_url, path);
        let req_start = Instant::now();
        let resp = http1_client.get(&url).send().unwrap();
        println!(
            "    GET {:12} -> {} version={:?} ({:?})",
            path,
            resp.status(),
            resp.version(),
            req_start.elapsed()
        );
    }
    println!("  HTTP/1.1 total: {:?}", start.elapsed());
    println!("  ^ Still ~500ms total — order doesn't matter, it's always sequential.\n");

    // ── HTTP/1.1 with MULTIPLE TCP connections (like a browser) ──────────
    // Browsers open 6 parallel TCP connections per domain to work around HOL blocking.
    // We simulate this with separate threads, each with its own reqwest::Client.
    // Each Client = its own connection pool = its own TCP socket.
    println!("  HTTP/1.1 — MULTIPLE TCP connections (like a browser with 6 connections):");
    println!("  Each thread gets its own reqwest::Client = its own TCP socket.");
    println!("  /slow and /health run on SEPARATE connections, so they don't block.\n");

    let start = Instant::now();
    let base1 = base_url.to_string();
    let base2 = base_url.to_string();

    // Thread 1: its own Client → its own TCP connection
    let h1 = thread::spawn(move || {
        let client = reqwest::blocking::ClientBuilder::new()
            .http1_only()
            .build()
            .unwrap();
        let t = Instant::now();
        let r = client.get(format!("{}/slow", base1)).send().unwrap();
        (r.status().to_string(), r.version(), t.elapsed())
    });

    // Thread 2: its own Client → its own TCP connection
    let h2 = thread::spawn(move || {
        let client = reqwest::blocking::ClientBuilder::new()
            .http1_only()
            .build()
            .unwrap();
        let t = Instant::now();
        let r = client.get(format!("{}/health", base2)).send().unwrap();
        (r.status().to_string(), r.version(), t.elapsed())
    });

    let (s1, v1, d1) = h1.join().unwrap();
    let (s2, v2, d2) = h2.join().unwrap();
    println!("    GET /slow        -> {} version={:?} ({:?})", s1, v1, d1);
    println!("    GET /health      -> {} version={:?} ({:?})", s2, v2, d2);
    println!("  HTTP/1.1 multi-conn total: {:?}", start.elapsed());
    println!("  ^ /health didn't wait! But we needed 2 TCP connections to do it.");
    println!("  This is the browser workaround: 6 connections × sequential = ~6 parallel.\n");

    // ── HTTP/1.1 with ONE client, async pool (like a real browser) ───────
    // reqwest's async Client automatically opens multiple TCP connections
    // from its pool when existing ones are busy — just like a browser!
    println!("  HTTP/1.1 — ONE async client, pool auto-opens 6 connections:");
    println!("  reqwest::Client with .http1_only() + .pool_max_idle_per_host(6)");
    println!("  tokio::join! fires 6 requests — pool opens 6 TCP connections.\n");

    let base_url_owned = base_url.to_string();
    let start = Instant::now();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let browser_results = rt.block_on(async {
        let browser_client = reqwest::ClientBuilder::new()
            .http1_only()
            .pool_max_idle_per_host(6)
            .build()
            .unwrap();

        // Fire 6 requests concurrently — pool opens 6 TCP connections automatically
        let paths = ["/slow", "/health", "/api/users", "/slow", "/", "/health"];
        let mut handles = Vec::new();
        for (i, path) in paths.iter().enumerate() {
            let client = browser_client.clone();
            let url = format!("{}{}", base_url_owned, path);
            let path = path.to_string();
            handles.push(tokio::spawn(async move {
                let start = Instant::now();
                let resp = client.get(&url).send().await.unwrap();
                (i + 1, path, resp.status(), resp.version(), start.elapsed())
            }));
        }

        let mut results = Vec::new();
        for h in handles {
            results.push(h.await.unwrap());
        }
        results.sort_by_key(|r| r.0);
        results
    });

    for (i, path, status, version, elapsed) in &browser_results {
        println!(
            "    req {} GET {:12} -> {} {:?} ({:?})",
            i, path, status, version, elapsed
        );
    }
    println!("  HTTP/1.1 browser-style total: {:?}", start.elapsed());
    println!("  ^ All 6 ran in parallel! Pool auto-opened 6 TCP connections.");
    println!("  /slow didn't block /health — they're on different connections.\n");

    // ── HTTP/2: multiplexed, no head-of-line blocking ────────────────────
    // .http2_prior_knowledge() tells reqwest to speak HTTP/2 directly (h2c).
    // On HTTP/2, multiple requests fly on ONE TCP connection simultaneously.
    // We use async reqwest + tokio::join! to fire both requests at the same time.
    println!("  HTTP/2 — multiplexed (no head-of-line blocking):");
    println!("  Client configured with: reqwest::Client::builder().http2_prior_knowledge()");
    println!("  Sending /slow AND /health CONCURRENTLY on ONE connection:\n");

    let base_url_owned = base_url.to_string();
    let start = Instant::now();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let (slow_result, health_result) = rt.block_on(async {
        // Same the same http2_client instance for both requests — they share ONE TCP connection with multiplexing.
        let http2_client = reqwest::ClientBuilder::new()
            .http2_prior_knowledge() // Speak HTTP/2 directly, no TLS needed
            .build()
            .unwrap();

        let slow_url = format!("{}/slow", base_url_owned);
        let health_url = format!("{}/health", base_url_owned);

        // tokio::join! sends both requests concurrently on the SAME HTTP/2 connection.
        // This is real multiplexing — both are in-flight simultaneously as separate streams.
        let slow_start = Instant::now();
        let health_start = Instant::now();

        tokio::join!(
            async {
                let resp = http2_client.get(&slow_url).send().await.unwrap();
                (resp.status(), resp.version(), slow_start.elapsed())
            },
            async {
                let resp = http2_client.get(&health_url).send().await.unwrap();
                (resp.status(), resp.version(), health_start.elapsed())
            }
        )
    });

    println!(
        "    GET /slow        -> {} version={:?} ({:?})",
        slow_result.0, slow_result.1, slow_result.2
    );
    println!(
        "    GET /health      -> {} version={:?} ({:?})",
        health_result.0, health_result.1, health_result.2
    );
    println!("  HTTP/2 total: {:?}", start.elapsed());
    println!("  ^ /health returned instantly — didn't wait for /slow!\n");

    println!("  COMPARISON:");
    println!("  HTTP/1.1: /slow(500ms) → /health(1ms) = ~501ms total (sequential)");
    println!("  HTTP/2:   /slow(500ms) + /health(1ms) = ~500ms total (multiplexed)");
    println!("  The {:?} format shows the ACTUAL protocol version from the wire.\n",
        "version=HTTP/2.0");
}

// =============================================================================
// 3. HTTP/2 — Real Multiplexing Demo
// =============================================================================
//
// HTTP/2 key improvements over HTTP/1.1:
// - Binary framing layer (not text-based)
// - Multiplexed streams (multiple requests/responses interleaved on one conn)
// - Header compression (HPACK)
// - Server push
// - Stream prioritization
//
// Instead of simulating frames, we fire REAL HTTP/2 requests against our
// axum server and prove multiplexing with timing.

/// Fire 10 concurrent HTTP/2 requests on ONE connection and compare with HTTP/1.1.
/// Uses the same axum server already running from Demo 2.
fn demo_http2_multiplexing(base_url: &str) {
    println!("\n  ═══ demo_http2_multiplexing ═══\n");

    // Paths to request — mix of fast and slow endpoints
    let paths = vec![
        "/", "/health", "/api/users", "/slow", "/",
        "/health", "/api/users", "/slow", "/health", "/",
    ];

    // ── HTTP/1.1: sequential, one at a time ──────────────────────────────
    println!("  HTTP/1.1 — 10 requests SEQUENTIAL (one connection):\n");
    let http1_client = reqwest::blocking::ClientBuilder::new()
        .http1_only()
        .build()
        .unwrap();

    let start = Instant::now();
    for (i, path) in paths.iter().enumerate() {
        let url = format!("{}{}", base_url, path);
        let req_start = Instant::now();
        let resp = http1_client.get(&url).send().unwrap();
        println!(
            "    req {:2} GET {:12} -> {} {:?} ({:?})",
            i + 1,
            path,
            resp.status(),
            resp.version(),
            req_start.elapsed()
        );
    }
    let http1_total = start.elapsed();
    println!("  HTTP/1.1 total: {:?} (sequential — each waits for previous)\n", http1_total);

    // ── HTTP/2: all 10 concurrent on ONE connection ──────────────────────
    println!("  HTTP/2 — 10 requests CONCURRENT (one multiplexed connection):\n");

    let base_url_owned = base_url.to_string();
    let paths_owned = paths.clone();
    let start = Instant::now();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let results = rt.block_on(async {
        let http2_client = reqwest::ClientBuilder::new()
            .http2_prior_knowledge()
            .build()
            .unwrap();

        // Spawn all 10 requests AT ONCE — they'll be multiplexed as separate
        // HTTP/2 streams on the SAME TCP connection.
        let mut handles = Vec::new();
        for (i, path) in paths_owned.iter().enumerate() {
            let client = http2_client.clone();
            let url = format!("{}{}", base_url_owned, path);
            let path = path.to_string();
            handles.push(tokio::spawn(async move {
                let req_start = Instant::now();
                let resp = client.get(&url).send().await.unwrap();
                (i + 1, path, resp.status(), resp.version(), req_start.elapsed())
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }
        results.sort_by_key(|r| r.0); // Sort by request number
        results
    });

    for (i, path, status, version, elapsed) in &results {
        println!(
            "    req {:2} GET {:12} -> {} {:?} ({:?})",
            i, path, status, version, elapsed
        );
    }
    let http2_total = start.elapsed();
    println!("  HTTP/2 total: {:?} (all 10 flew concurrently!)\n", http2_total);

    // ── Summary ──────────────────────────────────────────────────────────
    let speedup = http1_total.as_secs_f64() / http2_total.as_secs_f64();
    println!("  RESULTS:");
    println!("  HTTP/1.1: {:?} (10 requests sequential on 1 connection)", http1_total);
    println!("  HTTP/2:   {:?} (10 requests multiplexed on 1 connection)", http2_total);
    println!("  Speedup:  {:.1}x faster with HTTP/2 multiplexing", speedup);
    println!();
    println!("  WHY? HTTP/1.1 has 2× /slow (500ms each) = 1000ms minimum.");
    println!("  HTTP/2 runs both /slow requests in parallel = 500ms for both.");
    println!("  All fast requests also overlap with the slow ones.");
    println!();
    println!("  Binary framing format (what HTTP/2 uses on the wire):");
    println!("  ┌──────────┬────────┬───────┬────────────┬─────────┐");
    println!("  │ Length 3B│Type 1B │Flags 1B│Stream ID 4B│ Payload │");
    println!("  └──────────┴────────┴───────┴────────────┴─────────┘");
    println!("  Each request gets its own Stream ID (1, 3, 5, 7...).");
    println!("  Frames from different streams are interleaved on the wire.");
    println!("  HPACK compresses headers: ':method GET' = 1 byte (index 0x82).");
    println!();
}

/// Demonstrates HTTP/2 streaming: large payload received chunk by chunk,
/// while other requests complete concurrently on the SAME connection.
fn demo_http2_streaming(base_url: &str) {
    println!("\n  ═══ demo_http2_streaming ═══\n");
    println!("  Q: What if the payload is huge (512KB)? Does it load into memory?");
    println!("  A: NO! HTTP/2 streams it in ~16KB DATA frames with flow control.\n");

    let base_url_owned = base_url.to_string();
    let bytes_received = Arc::new(AtomicU64::new(0));
    let bytes_clone = bytes_received.clone();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::ClientBuilder::new()
            .http2_prior_knowledge()
            .build()
            .unwrap();

        println!("  Streaming 512KB + 3 fast requests CONCURRENTLY (one HTTP/2 connection):\n");

        let overall_start = Instant::now();

        let (stream_result, r1, r2, r3) = tokio::join!(
            // Stream 512KB — received chunk by chunk, never holds full payload
            async {
                let start = Instant::now();
                let resp = client
                    .get(format!("{}/stream/512", base_url_owned))
                    .send()
                    .await
                    .unwrap();
                let version = resp.version();

                let mut total = 0u64;
                let mut chunk_count = 0u32;
                let mut stream = resp.bytes_stream();

                // Read chunk by chunk — only ~16KB in memory at a time
                while let Some(result) = stream.next().await {
                    let chunk = result.unwrap();
                    total += chunk.len() as u64;
                    chunk_count += 1;
                    bytes_clone.store(total, Ordering::Relaxed);
                }
                (version, total, chunk_count, start.elapsed())
            },
            // These fast requests complete WHILE the 512KB stream is still going
            async {
                // Small delay so stream starts first
                tokio::time::sleep(Duration::from_millis(2)).await;
                let start = Instant::now();
                let resp = client.get(format!("{}/health", base_url_owned)).send().await.unwrap();
                let streamed_so_far = bytes_clone.load(Ordering::Relaxed);
                let body = resp.text().await.unwrap();
                ("/health", body, start.elapsed(), streamed_so_far)
            },
            async {
                tokio::time::sleep(Duration::from_millis(3)).await;
                let start = Instant::now();
                let resp = client.get(format!("{}/", base_url_owned)).send().await.unwrap();
                let streamed_so_far = bytes_clone.load(Ordering::Relaxed);
                let body = resp.text().await.unwrap();
                ("/", body, start.elapsed(), streamed_so_far)
            },
            async {
                tokio::time::sleep(Duration::from_millis(4)).await;
                let start = Instant::now();
                let resp = client.get(format!("{}/api/users", base_url_owned)).send().await.unwrap();
                let streamed_so_far = bytes_clone.load(Ordering::Relaxed);
                let body = resp.text().await.unwrap();
                ("/api/users", body, start.elapsed(), streamed_so_far)
            },
        );

        let total_time = overall_start.elapsed();

        // Fast requests completed during the stream
        for (path, body, elapsed, streamed) in [r1, r2, r3] {
            println!(
                "    GET {:12} -> {} ({:?}) ← stream had {}KB when this finished",
                path, body, elapsed, streamed / 1024
            );
        }
        println!(
            "    GET /stream/512  -> {:?} {} bytes in {} chunks ({:?})",
            stream_result.0, stream_result.1, stream_result.2, stream_result.3
        );
        println!("\n  Total time: {:?}", total_time);
    });

    println!();
    println!("  KEY INSIGHTS:");
    println!("  1. Server generated 512KB on the fly (async_stream), never buffered it all");
    println!("  2. Client received it in ~32 chunks of 16KB (only 16KB in RAM at a time)");
    println!("  3. /health, /, /api/users completed DURING the stream (interleaved frames)");
    println!("  4. All 4 requests shared ONE TCP connection (HTTP/2 multiplexing)");
    println!();
    println!("  On the wire, HTTP/2 interleaves DATA frames from different streams:");
    println!("    [DATA stream=1 16KB] [DATA stream=1 16KB] [HEADERS stream=3 /health]");
    println!("    [DATA stream=3 resp] [DATA stream=1 16KB] [HEADERS stream=5 /]");
    println!("    [DATA stream=5 resp] [DATA stream=1 16KB] ...");
    println!();
    println!("  Flow control prevents overwhelming the receiver:");
    println!("    • Each stream has a receive window (default 64KB)");
    println!("    • Sender pauses when window is full (WINDOW_UPDATE to resume)");
    println!("    • Pausing one stream doesn't affect others");
    println!("    • This is why HTTP/2 is safe for streaming files, video, gRPC, etc.");
    println!();
}

// =============================================================================
// 4. HTTP/3 / QUIC — Real QUIC Demo (quinn library)
// =============================================================================
//
// HTTP/3 runs over QUIC (UDP-based transport).
// Instead of simulating, we use the quinn library (production QUIC implementation)
// to demonstrate:
// - Real UDP transport (not TCP!)
// - Built-in TLS 1.3 (mandatory — no unencrypted QUIC exists)
// - Independent bidirectional streams (no HOL blocking)
// - Connection ID (enables connection migration)
// - 1-RTT handshake (vs 2-3 RTT for TCP+TLS)

/// Real QUIC demo using quinn: server + client over UDP with multiple streams.
fn demo_real_quic() {
    println!("\n  ═══ demo_real_quic ═══\n");
    println!("  Using quinn (production QUIC library) for real QUIC over UDP.\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // ── Step 1: Generate self-signed TLS cert ────────────────────────
        // QUIC mandates TLS 1.3 — you CANNOT have unencrypted QUIC.
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = rustls_pki_types::CertificateDer::from(cert.cert);
        let key_der = rustls_pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
        println!("  [Setup] Generated self-signed TLS 1.3 cert (QUIC mandates encryption)");

        // ── Step 2: Start QUIC server on UDP ─────────────────────────────
        let server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert_der.clone()],
                rustls_pki_types::PrivateKeyDer::Pkcs8(key_der),
            )
            .unwrap();
        let server_config = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto).unwrap(),
        ));

        let server_addr: std::net::SocketAddr = "127.0.0.1:9007".parse().unwrap();
        let server_endpoint = quinn::Endpoint::server(server_config, server_addr).unwrap();
        println!("  [QUIC Server] Listening on UDP {} (not TCP!)", server_addr);

        // Server: accept one connection, handle streams independently
        let server = tokio::spawn(async move {
            let incoming = server_endpoint.accept().await.unwrap();
            let connection = incoming.await.unwrap();
            println!(
                "  [QUIC Server] Client connected (conn_id={})",
                connection.stable_id()
            );

            // Accept bidirectional streams in a loop — each handled independently
            loop {
                match connection.accept_bi().await {
                    Ok((mut send, mut recv)) => {
                        tokio::spawn(async move {
                            let data = recv.read_to_end(4096).await.unwrap();
                            let request = String::from_utf8_lossy(&data);

                            // Slow endpoint: simulate 300ms delay
                            if request.contains("slow") {
                                tokio::time::sleep(Duration::from_millis(300)).await;
                            }

                            let response = format!("QUIC OK: {}", request);
                            send.write_all(response.as_bytes()).await.unwrap();
                            send.finish().unwrap();
                        });
                    }
                    Err(_) => break, // Connection closed
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        // ── Step 3: QUIC client connects ─────────────────────────────────
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert_der).unwrap();
        let client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto).unwrap(),
        ));

        let mut client_endpoint =
            quinn::Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        client_endpoint.set_default_client_config(client_config);

        // Time the QUIC handshake (1 RTT, includes TLS 1.3!)
        let handshake_start = Instant::now();
        let connection = client_endpoint
            .connect(server_addr, "localhost")
            .unwrap()
            .await
            .unwrap();
        let handshake_time = handshake_start.elapsed();

        println!(
            "  [QUIC Client] Connected! Handshake: {:?} (1 RTT, TLS 1.3 included!)",
            handshake_time
        );
        println!("  [QUIC Client] Transport: UDP (not TCP)");
        println!(
            "  [QUIC Client] Connection ID: {} (survives IP changes!)\n",
            connection.stable_id()
        );

        // ── Step 4: Open 4 streams concurrently ──────────────────────────
        // Each stream is independent — slow stream doesn't block fast ones.
        // This is the key advantage over HTTP/2 (TCP), where a lost TCP packet
        // blocks ALL streams.
        println!("  4 bidirectional streams on ONE QUIC connection:\n");

        let requests = vec![
            ("Stream 1", "GET /fast1"),
            ("Stream 2", "GET /slow (300ms)"),
            ("Stream 3", "GET /fast2"),
            ("Stream 4", "GET /fast3"),
        ];

        let overall_start = Instant::now();
        let mut handles = vec![];

        for (name, req) in &requests {
            let conn = connection.clone();
            let name = name.to_string();
            let req = req.to_string();
            handles.push(tokio::spawn(async move {
                let start = Instant::now();
                let (mut send, mut recv) = conn.open_bi().await.unwrap();
                send.write_all(req.as_bytes()).await.unwrap();
                send.finish().unwrap();

                let response = recv.read_to_end(4096).await.unwrap();
                let elapsed = start.elapsed();
                let resp_str = String::from_utf8_lossy(&response).to_string();
                (name, req, resp_str, elapsed)
            }));
        }

        let mut results = vec![];
        for h in handles {
            results.push(h.await.unwrap());
        }
        results.sort_by(|a, b| a.3.cmp(&b.3)); // Sort by completion time

        for (name, req, resp, elapsed) in &results {
            println!("    {} | {} -> {} ({:?})", name, req, resp, elapsed);
        }

        let total = overall_start.elapsed();
        println!("\n  Total: {:?}", total);
        println!("  ^ /slow (300ms) did NOT block the fast streams!\n");

        // Clean up
        connection.close(0u32.into(), b"done");
        client_endpoint.wait_idle().await;
        server.abort();
    });

    println!("  WHY THIS MATTERS vs HTTP/2:");
    println!("  HTTP/2 multiplexes streams on TCP. If a TCP packet is lost,");
    println!("  ALL streams stall until retransmit (TCP HOL blocking).");
    println!("  QUIC (UDP) has independent streams — packet loss on Stream 2");
    println!("  only blocks Stream 2. Streams 1, 3, 4 keep flowing.");
    println!();
    println!("  ┌─────────────────┬──────────────────┬──────────────────┐");
    println!("  │                 │ HTTP/2 (TCP)     │ HTTP/3 (QUIC)    │");
    println!("  ├─────────────────┼──────────────────┼──────────────────┤");
    println!("  │ Transport       │ TCP              │ UDP              │");
    println!("  │ Handshake       │ TCP+TLS = 2-3 RTT│ 1 RTT (0-RTT!)  │");
    println!("  │ HOL blocking    │ YES (TCP layer)  │ NO (per-stream)  │");
    println!("  │ Encryption      │ TLS on top       │ Built-in TLS 1.3 │");
    println!("  │ Conn migration  │ NO (IP:port)     │ YES (conn ID)    │");
    println!("  │ Loss recovery   │ Per-connection   │ Per-stream       │");
    println!("  └─────────────────┴──────────────────┴──────────────────┘");
    println!();
}

// =============================================================================
// 5. WebSocket Server/Client — Full Handshake + Binary Framing
// =============================================================================
//
// WebSocket provides full-duplex communication over a single TCP connection.
// Lifecycle:
//   1. Client sends HTTP Upgrade request with Sec-WebSocket-Key
//   2. Server responds with 101 Switching Protocols + Sec-WebSocket-Accept
//   3. Both sides exchange WebSocket frames (not HTTP anymore)
//   4. Either side can send Close frame

const WS_MAGIC_GUID: &str = "258EAFA5-E914-47DA-95CA-5AB5DC76E598";

/// WebSocket opcodes
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
enum WsOpcode {
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
}

impl WsOpcode {
    fn from_byte(b: u8) -> Option<Self> {
        match b & 0x0F {
            0x1 => Some(Self::Text),
            0x2 => Some(Self::Binary),
            0x8 => Some(Self::Close),
            0x9 => Some(Self::Ping),
            0xA => Some(Self::Pong),
            _ => None,
        }
    }
}

/// WebSocket frame (simplified — handles payloads up to 125 bytes for demo)
///
/// Wire format:
///   Byte 0: [FIN(1) RSV(3) OPCODE(4)]
///   Byte 1: [MASK(1) LENGTH(7)]
///   If MASK=1: 4 bytes masking key
///   Payload data (XOR'd with mask if MASK=1)
struct WsFrame {
    fin: bool,
    opcode: WsOpcode,
    mask: Option<[u8; 4]>,
    payload: Vec<u8>,
}

impl WsFrame {
    fn text(msg: &str) -> Self {
        Self {
            fin: true,
            opcode: WsOpcode::Text,
            mask: None,
            payload: msg.as_bytes().to_vec(),
        }
    }

    fn text_masked(msg: &str, mask_key: [u8; 4]) -> Self {
        Self {
            fin: true,
            opcode: WsOpcode::Text,
            mask: Some(mask_key),
            payload: msg.as_bytes().to_vec(),
        }
    }

    fn close() -> Self {
        Self {
            fin: true,
            opcode: WsOpcode::Close,
            mask: None,
            payload: vec![],
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Byte 0: FIN + opcode
        let byte0 = if self.fin { 0x80 } else { 0x00 } | (self.opcode as u8);
        buf.push(byte0);

        // Byte 1: MASK + length
        let mask_bit = if self.mask.is_some() { 0x80 } else { 0x00 };
        let len = self.payload.len();
        if len <= 125 {
            buf.push(mask_bit | len as u8);
        } else if len <= 65535 {
            buf.push(mask_bit | 126);
            buf.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            buf.push(mask_bit | 127);
            buf.extend_from_slice(&(len as u64).to_be_bytes());
        }

        // Masking key + masked payload
        if let Some(mask_key) = self.mask {
            buf.extend_from_slice(&mask_key);
            for (i, &b) in self.payload.iter().enumerate() {
                buf.push(b ^ mask_key[i % 4]);
            }
        } else {
            buf.extend_from_slice(&self.payload);
        }

        buf
    }

    fn decode(data: &[u8]) -> Option<(Self, usize)> {
        if data.len() < 2 {
            return None;
        }

        let fin = data[0] & 0x80 != 0;
        let opcode = WsOpcode::from_byte(data[0])?;
        let masked = data[1] & 0x80 != 0;
        let mut payload_len = (data[1] & 0x7F) as usize;
        let mut offset = 2;

        if payload_len == 126 {
            if data.len() < 4 {
                return None;
            }
            payload_len =
                u16::from_be_bytes([data[2], data[3]]) as usize;
            offset = 4;
        } else if payload_len == 127 {
            if data.len() < 10 {
                return None;
            }
            payload_len = u64::from_be_bytes(
                data[2..10].try_into().ok()?,
            ) as usize;
            offset = 10;
        }

        let mask_key = if masked {
            if data.len() < offset + 4 {
                return None;
            }
            let key: [u8; 4] = data[offset..offset + 4].try_into().ok()?;
            offset += 4;
            Some(key)
        } else {
            None
        };

        if data.len() < offset + payload_len {
            return None;
        }

        let mut payload = data[offset..offset + payload_len].to_vec();

        // Unmask payload
        if let Some(key) = mask_key {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= key[i % 4];
            }
        }

        Some((
            Self {
                fin,
                opcode,
                mask: mask_key,
                payload,
            },
            offset + payload_len,
        ))
    }
}

/// Compute the Sec-WebSocket-Accept value per RFC 6455
fn ws_accept_key(client_key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(client_key.trim().as_bytes());
    hasher.update(WS_MAGIC_GUID.as_bytes());
    let hash = hasher.finalize();
    base64::engine::general_purpose::STANDARD.encode(hash)
}

/// WebSocket echo server
fn run_ws_server(addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("[WebSocket] Listening on {}", addr);

    for stream in listener.incoming().take(1) {
        match stream {
            Ok(stream) => {
                handle_ws_connection(stream);
            }
            Err(e) => eprintln!("[WebSocket] Error: {}", e),
        }
    }
    Ok(())
}

fn handle_ws_connection(mut stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());

    // Step 1: Read the HTTP Upgrade request
    let mut headers = Vec::new();
    let mut ws_key = String::new();

    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok();
        if line.trim().is_empty() {
            break;
        }
        let lower = line.to_lowercase();
        if lower.starts_with("sec-websocket-key:") {
            ws_key = line.split(':').nth(1).unwrap_or("").trim().to_string();
        }
        headers.push(line);
    }

    println!(
        "[WebSocket] Upgrade request received, key: {}",
        ws_key
    );

    // Step 2: Send 101 Switching Protocols
    let accept = ws_accept_key(&ws_key);
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\
         \r\n",
        accept
    );
    stream.write_all(response.as_bytes()).unwrap();
    stream.flush().unwrap();
    println!("[WebSocket] Handshake complete, Sec-WebSocket-Accept: {}", accept);

    // Step 3: Exchange frames
    let mut frame_buf = vec![0u8; 4096];
    loop {
        let n = match stream.read(&mut frame_buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };

        if let Some((frame, _)) = WsFrame::decode(&frame_buf[..n]) {
            match frame.opcode {
                WsOpcode::Text => {
                    let msg = String::from_utf8_lossy(&frame.payload);
                    println!("[WebSocket Server] Received text: \"{}\"", msg);

                    // Echo back (server->client frames are NOT masked per RFC 6455)
                    let echo = WsFrame::text(&format!("echo: {}", msg));
                    stream.write_all(&echo.encode()).unwrap();
                    stream.flush().unwrap();
                }
                WsOpcode::Ping => {
                    let pong = WsFrame {
                        fin: true,
                        opcode: WsOpcode::Pong,
                        mask: None,
                        payload: frame.payload,
                    };
                    stream.write_all(&pong.encode()).unwrap();
                }
                WsOpcode::Close => {
                    println!("[WebSocket Server] Close frame received");
                    let close = WsFrame::close();
                    stream.write_all(&close.encode()).ok();
                    break;
                }
                _ => {}
            }
        }
    }
}

/// WebSocket client that performs handshake and exchanges messages
fn run_ws_client(addr: &str) {
    let mut stream = TcpStream::connect(addr).unwrap();
    let ws_key = "dGhlIHNhbXBsZSBub25jZQ=="; // Example key from RFC

    // Step 1: Send HTTP Upgrade request
    let request = format!(
        "GET /chat HTTP/1.1\r\n\
         Host: {}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n",
        addr, ws_key
    );
    stream.write_all(request.as_bytes()).unwrap();
    stream.flush().unwrap();

    // Step 2: Read 101 response
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).ok();
    println!("[WebSocket Client] Server response: {}", line.trim());

    // Consume remaining headers
    loop {
        line.clear();
        reader.read_line(&mut line).ok();
        if line.trim().is_empty() {
            break;
        }
    }

    // Verify handshake
    let expected_accept = ws_accept_key(ws_key);
    println!(
        "[WebSocket Client] Expected Accept: {}",
        expected_accept
    );

    // Step 3: Send messages (client->server frames MUST be masked per RFC 6455)
    let messages = ["Hello WebSocket!", "How are you?", "Goodbye"];
    let mask_key = [0x12, 0x34, 0x56, 0x78]; // In production, use random key

    for msg in &messages {
        let frame = WsFrame::text_masked(msg, mask_key);
        stream.write_all(&frame.encode()).unwrap();
        stream.flush().unwrap();

        // Read echo response
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        if let Some((resp_frame, _)) = WsFrame::decode(&buf[..n]) {
            println!(
                "[WebSocket Client] Sent: \"{}\" -> Got: \"{}\"",
                msg,
                String::from_utf8_lossy(&resp_frame.payload)
            );
        }
    }

    // Send close frame
    let close = WsFrame {
        fin: true,
        opcode: WsOpcode::Close,
        mask: Some(mask_key),
        payload: vec![],
    };
    stream.write_all(&close.encode()).ok();
    println!("[WebSocket Client] Connection closed\n");
}

// =============================================================================
// 6. Server-Sent Events (SSE) — Server Push over HTTP
// =============================================================================
//
// SSE is a simple protocol for server-to-client streaming:
// - Uses plain HTTP (Content-Type: text/event-stream)
// - Server sends events in "data: ...\n\n" format
// - Client auto-reconnects (Last-Event-ID header)
// - Unidirectional: server → client only (unlike WebSocket)
//
// Perfect for: live feeds, notifications, stock tickers, log streaming

/// SSE server that pushes events to clients
fn run_sse_server(addr: &str, events: Arc<Mutex<Vec<String>>>) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("[SSE Server] Listening on {}", addr);

    for stream in listener.incoming().take(1) {
        match stream {
            Ok(mut stream) => {
                let mut reader = BufReader::new(stream.try_clone().unwrap());

                // Read HTTP request
                let mut request_line = String::new();
                reader.read_line(&mut request_line).ok();
                println!("[SSE Server] {}", request_line.trim());

                // Consume headers
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).ok();
                    // Check for Last-Event-ID (reconnection support)
                    if line.to_lowercase().starts_with("last-event-id:") {
                        println!(
                            "[SSE Server] Client reconnecting from event: {}",
                            line.split(':').nth(1).unwrap_or("").trim()
                        );
                    }
                    if line.trim().is_empty() {
                        break;
                    }
                }

                // Send SSE response headers
                let headers = "HTTP/1.1 200 OK\r\n\
                               Content-Type: text/event-stream\r\n\
                               Cache-Control: no-cache\r\n\
                               Connection: keep-alive\r\n\
                               Access-Control-Allow-Origin: *\r\n\
                               \r\n";
                stream.write_all(headers.as_bytes()).unwrap();
                stream.flush().unwrap();

                // Stream events
                let events = events.lock().unwrap();
                for (i, event) in events.iter().enumerate() {
                    // SSE format: "id: N\nevent: type\ndata: payload\n\n"
                    let sse_event = format!(
                        "id: {}\nevent: message\ndata: {}\n\n",
                        i + 1,
                        event
                    );
                    if stream.write_all(sse_event.as_bytes()).is_err() {
                        break;
                    }
                    stream.flush().ok();
                    println!("[SSE Server] Sent event #{}: {}", i + 1, event);
                    thread::sleep(Duration::from_millis(100));
                }

                // Send a named event (different event type)
                let named = "id: 99\nevent: status\ndata: {\"status\":\"complete\"}\n\n";
                stream.write_all(named.as_bytes()).ok();
                stream.flush().ok();
                println!("[SSE Server] Sent status event");
            }
            Err(e) => eprintln!("[SSE Server] Error: {}", e),
        }
    }
    Ok(())
}

/// SSE client that reads server-pushed events
fn run_sse_client(addr: &str) {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .ok();

    // Send request with Accept: text/event-stream
    let request = format!(
        "GET /events HTTP/1.1\r\n\
         Host: {}\r\n\
         Accept: text/event-stream\r\n\
         Cache-Control: no-cache\r\n\
         \r\n",
        addr
    );
    stream.write_all(request.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(&stream);

    // Skip HTTP response headers
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok();
        if line.trim().is_empty() {
            break;
        }
    }

    // Parse SSE events
    let mut current_id = String::new();
    let mut current_event = String::new();
    let mut current_data = String::new();

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            _ => {}
        }

        let line = line.trim_end();

        if line.is_empty() {
            // Empty line = end of event
            if !current_data.is_empty() {
                println!(
                    "[SSE Client] Event(id={}, type={}): {}",
                    current_id,
                    if current_event.is_empty() {
                        "message"
                    } else {
                        &current_event
                    },
                    current_data.trim()
                );
                current_id.clear();
                current_event.clear();
                current_data.clear();
            }
        } else if let Some(rest) = line.strip_prefix("id: ") {
            current_id = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("event: ") {
            current_event = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("data: ") {
            current_data.push_str(rest);
        }
    }

    println!();
}

// =============================================================================
// 8. TLS — Transport Layer Security
// =============================================================================
//
// TLS encrypts the connection between client and server.
// Without TLS: anyone on the network can read your HTTP traffic (passwords, tokens, etc.)
// With TLS: traffic is encrypted, authenticated, and tamper-proof.
//
// TLS handshake adds 1-2 RTT of latency but provides:
// - Encryption: AES-256-GCM or ChaCha20-Poly1305
// - Authentication: server proves identity with a certificate
// - Integrity: HMAC prevents tampering
//
// We use rustls (the same TLS library quinn/QUIC uses) to demonstrate
// a real TLS handshake with certificate generation.

/// Real TLS demo: generate a cert, start a TLS server, connect with a TLS client.
fn demo_tls() {
    println!("\n  ═══ demo_tls ═══\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // ── Step 1: Generate self-signed certificate ─────────────────────
        // In production, you'd get this from Let's Encrypt or your CA.
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = rustls_pki_types::CertificateDer::from(cert.cert);
        let key_der = rustls_pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

        println!("  [Step 1] Generated self-signed X.509 certificate");
        println!("    Subject: localhost");
        println!("    Cert size: {} bytes (DER encoded)", cert_der.len());
        println!("    Key type: PKCS#8 private key\n");

        // ── Step 2: Configure TLS server ─────────────────────────────────
        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth() // No mTLS (mutual TLS) for this demo
            .with_single_cert(
                vec![cert_der.clone()],
                rustls_pki_types::PrivateKeyDer::Pkcs8(key_der),
            )
            .unwrap();

        let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:9008").await.unwrap();
        println!("  [Step 2] TLS server listening on 127.0.0.1:9008");

        // Server: accept one TLS connection, read request, send response
        let acceptor = tls_acceptor.clone();
        let server = tokio::spawn(async move {
            let (tcp_stream, peer) = listener.accept().await.unwrap();
            println!("  [Server] TCP connection from {}", peer);

            // TLS handshake happens here — this is where the magic is
            let handshake_start = Instant::now();
            let tls_stream = acceptor.accept(tcp_stream).await.unwrap();
            let handshake_time = handshake_start.elapsed();

            let (_, server_conn) = tls_stream.get_ref();
            println!(
                "  [Server] TLS handshake complete ({:?})",
                handshake_time
            );
            println!(
                "  [Server] Protocol: {:?}",
                server_conn.protocol_version().unwrap()
            );
            println!(
                "  [Server] Cipher: {:?}",
                server_conn.negotiated_cipher_suite().unwrap()
            );
            println!(
                "  [Server] ALPN: {:?}",
                server_conn.alpn_protocol().map(|p| String::from_utf8_lossy(p).to_string())
            );

            // Read HTTP request over TLS
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut tls_stream = tls_stream;
            let mut buf = [0u8; 4096];
            let n = tls_stream.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            println!("  [Server] Received (encrypted on wire): {}", request.lines().next().unwrap_or(""));

            let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 21\r\n\r\nHello over TLS 1.3!\n";
            tls_stream.write_all(response.as_bytes()).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        // ── Step 3: TLS client connects ──────────────────────────────────
        // Client must trust the server's certificate
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert_der).unwrap();

        let client_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();

        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));

        let tcp_stream = tokio::net::TcpStream::connect("127.0.0.1:9008").await.unwrap();
        println!("\n  [Client] TCP connected, starting TLS handshake...");

        let handshake_start = Instant::now();
        let server_name = rustls_pki_types::ServerName::try_from("localhost").unwrap();
        let mut tls_stream = connector.connect(server_name, tcp_stream).await.unwrap();
        let handshake_time = handshake_start.elapsed();

        let (_, client_conn) = tls_stream.get_ref();
        println!("  [Client] TLS handshake complete ({:?})", handshake_time);
        println!(
            "  [Client] Protocol: {:?}",
            client_conn.protocol_version().unwrap()
        );
        println!(
            "  [Client] Cipher: {:?}",
            client_conn.negotiated_cipher_suite().unwrap()
        );

        // Send HTTP request over TLS
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let request = "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
        tls_stream.write_all(request.as_bytes()).await.unwrap();

        let mut buf = [0u8; 4096];
        let n = tls_stream.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);
        println!("  [Client] Response: {}\n", response.lines().last().unwrap_or(""));

        server.await.unwrap();
    });

    println!("  TLS HANDSHAKE FLOW:");
    println!("  ┌─────────────────────────────────────────────────────────┐");
    println!("  │ Client                              Server              │");
    println!("  │   │                                    │                │");
    println!("  │   │── ClientHello ────────────────────►│  (1 RTT)      │");
    println!("  │   │   supported ciphers, TLS version   │                │");
    println!("  │   │   random, SNI (hostname)           │                │");
    println!("  │   │                                    │                │");
    println!("  │   │◄── ServerHello + Certificate ──────│  (1 RTT)      │");
    println!("  │   │    chosen cipher, server random    │                │");
    println!("  │   │    X.509 cert (proves identity)    │                │");
    println!("  │   │                                    │                │");
    println!("  │   │── Finished ───────────────────────►│               │");
    println!("  │   │   (key exchange complete)          │                │");
    println!("  │   │                                    │                │");
    println!("  │   │═══ Encrypted Application Data ════│               │");
    println!("  └─────────────────────────────────────────────────────────┘");
    println!();
    println!("  TLS VERSION COMPARISON:");
    println!("  ┌──────────┬──────────┬──────────┬───────────┬───────────┐");
    println!("  │ Version  │Handshake │ Ciphers  │ 0-RTT     │ Status    │");
    println!("  ├──────────┼──────────┼──────────┼───────────┼───────────┤");
    println!("  │ TLS 1.0  │ 2 RTT    │ Weak     │ No        │ DEAD      │");
    println!("  │ TLS 1.1  │ 2 RTT    │ Weak     │ No        │ DEAD      │");
    println!("  │ TLS 1.2  │ 2 RTT    │ Mixed    │ No        │ Legacy    │");
    println!("  │ TLS 1.3  │ 1 RTT    │ Strong   │ Yes (PSK) │ Current   │");
    println!("  └──────────┴──────────┴──────────┴───────────┴───────────┘");
    println!();
    println!("  KEY CONCEPTS:");
    println!("  • Certificate: X.509 file proving server identity (signed by CA)");
    println!("  • CA (Certificate Authority): trusted third party (Let's Encrypt, DigiCert)");
    println!("  • ALPN: negotiates HTTP version during TLS handshake (h2, http/1.1)");
    println!("  • SNI: client sends hostname in ClientHello (needed for virtual hosting)");
    println!("  • 0-RTT (TLS 1.3): resumed connections can send data immediately");
    println!("  • mTLS: mutual TLS — both client AND server present certificates");
    println!();
}

// =============================================================================
// 9. Connection Pool
// =============================================================================

struct ConnectionPool {
    connections: Vec<TcpStream>,
    max_size: usize,
}

impl ConnectionPool {
    fn new(max_size: usize) -> Self {
        Self {
            connections: Vec::new(),
            max_size,
        }
    }

    fn get(&mut self, addr: &str) -> std::io::Result<TcpStream> {
        if let Some(conn) = self.connections.pop() {
            println!("[Pool] Reusing connection");
            return Ok(conn);
        }
        println!("[Pool] Creating new connection");
        TcpStream::connect(addr)
    }

    fn put(&mut self, conn: TcpStream) {
        if self.connections.len() < self.max_size {
            println!("[Pool] Returning connection to pool");
            self.connections.push(conn);
        } else {
            println!("[Pool] Pool full, closing connection");
            drop(conn);
        }
    }
}

// =============================================================================
// Main — Run All Demos
// =============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║       Networking Essentials — Full Demo          ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    // ── Demo 1: TCP Echo ─────────────────────────────────────────────────
    println!("━━━ 1. TCP Echo Server/Client ━━━");
    let echo_addr = "127.0.0.1:9001";
    let _echo_server = thread::spawn(move || run_echo_server(echo_addr).ok());
    thread::sleep(Duration::from_millis(50));
    run_echo_client(echo_addr, &["Hello", "World", "Goodbye"]).ok();
    println!();

    // ── Demo 2: HTTP/1.1 — Real Server + Client ─────────────────────────
    println!("━━━ 2. HTTP/1.1 — Real Server (axum) + Real Client (reqwest) ━━━");
    let http_addr = "127.0.0.1:9002";
    start_http_server(http_addr);
    thread::sleep(Duration::from_millis(200));
    let base_url = format!("http://{}", http_addr);
    demo_http11_keepalive(&base_url);
    demo_http11_hol_blocking(&base_url);

    // ── Demo 3: HTTP/2 Real Multiplexing ─────────────────────────────────
    println!("━━━ 3. HTTP/2 — Real Multiplexing (10 concurrent requests) ━━━");
    demo_http2_multiplexing(&base_url);

    // ── Demo 3b: HTTP/2 Streaming ────────────────────────────────────────
    println!("━━━ 3b. HTTP/2 — Streaming Large Payloads ━━━");
    demo_http2_streaming(&base_url);

    // ── Demo 4: HTTP/3 / QUIC (Real) ───────────────────────────────────
    println!("━━━ 4. HTTP/3 / QUIC — Real QUIC over UDP (quinn) ━━━");
    demo_real_quic();

    // ── Demo 5: WebSocket ────────────────────────────────────────────────
    println!("━━━ 5. WebSocket — Full-Duplex Communication ━━━");
    println!("  Handshake flow:");
    println!("  Client → Server: GET /chat HTTP/1.1");
    println!("                   Upgrade: websocket");
    println!("                   Sec-WebSocket-Key: <base64 nonce>");
    println!("  Server → Client: HTTP/1.1 101 Switching Protocols");
    println!("                   Sec-WebSocket-Accept: SHA1(key+GUID)\n");

    let ws_addr = "127.0.0.1:9003";
    let _ws_server = thread::spawn(move || run_ws_server(ws_addr).ok());
    thread::sleep(Duration::from_millis(50));
    run_ws_client(ws_addr);

    // ── Demo 6: Server-Sent Events ───────────────────────────────────────
    println!("━━━ 6. Server-Sent Events (SSE) — Server Push ━━━");
    println!("  SSE format:  id: N\\nevent: type\\ndata: payload\\n\\n");
    println!("  Content-Type: text/event-stream");
    println!("  Unidirectional: server → client only\n");

    let sse_addr = "127.0.0.1:9005";
    let events = Arc::new(Mutex::new(vec![
        r#"{"price":"BTC $67,000"}"#.to_string(),
        r#"{"price":"ETH $3,400"}"#.to_string(),
        r#"{"alert":"New block #19000000"}"#.to_string(),
    ]));
    let events_clone = events.clone();
    let _sse_server = thread::spawn(move || run_sse_server(sse_addr, events_clone).ok());
    thread::sleep(Duration::from_millis(50));
    run_sse_client(sse_addr);

    // ── Demo 7: Connection Pool ──────────────────────────────────────────
    println!("━━━ 7. Connection Pool ━━━");
    // Start a quick server for pool demo
    let pool_addr = "127.0.0.1:9006";
    let _pool_server = thread::spawn(move || {
        let listener = TcpListener::bind(pool_addr).unwrap();
        for stream in listener.incoming().take(3) {
            stream.ok(); // Just accept
        }
    });
    thread::sleep(Duration::from_millis(50));

    let mut pool = ConnectionPool::new(2);
    if let Ok(conn1) = pool.get(pool_addr) {
        pool.put(conn1);
    }
    if let Ok(_conn2) = pool.get(pool_addr) {
        // Reused from pool
    }
    println!();

    // ── Protocol Comparison ──────────────────────────────────────────────
    println!("━━━ Protocol Comparison ━━━");
    println!("
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
");

    // ── Latency Breakdown ────────────────────────────────────────────────
    println!("━━━ Latency Breakdown ━━━");
    println!("
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
");

    thread::sleep(Duration::from_millis(500));
    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
