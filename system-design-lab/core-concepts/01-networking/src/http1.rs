//! HTTP/1.1 demos: keep-alive, head-of-line blocking, browser-style connection pooling.

use std::thread;
use std::time::Instant;

/// Demonstrates HTTP/1.1 keep-alive: connection reuse vs fresh connections.
pub fn demo_keepalive(base_url: &str) {
    println!("\n  ═══ demo_http11_keepalive ═══\n");
    let client = reqwest::blocking::Client::new();
    let paths = ["/", "/health", "/api/users"];

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
    println!("\n  Total (pooled): {:?}", reused_time);

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
    println!("\n  Total (fresh):  {:?}", fresh_time);

    println!(
        "\n  Pooled is ~{:.0}% faster — 2nd/3rd requests skip TCP handshake!",
        (1.0 - reused_time.as_secs_f64() / fresh_time.as_secs_f64()) * 100.0
    );
    println!(
        "  Try: curl -v {}/ — look for 'Connection: keep-alive' in response\n",
        base_url
    );
}

/// Demonstrates HOL blocking: HTTP/1.1 sequential vs HTTP/1.1 multi-conn vs HTTP/2 multiplexed.
pub fn demo_hol_blocking(base_url: &str) {
    println!("\n  ═══ demo_http11_hol_blocking ═══\n");
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

    // ── Multiple TCP connections (like a browser) ────────────────────────
    println!("  HTTP/1.1 — MULTIPLE TCP connections (like a browser with 6 connections):");
    println!("  Each thread gets its own reqwest::Client = its own TCP socket.");
    println!("  /slow and /health run on SEPARATE connections, so they don't block.\n");

    let start = Instant::now();
    let base1 = base_url.to_string();
    let base2 = base_url.to_string();

    let h1 = thread::spawn(move || {
        let client = reqwest::blocking::ClientBuilder::new()
            .http1_only()
            .build()
            .unwrap();
        let t = Instant::now();
        let r = client.get(format!("{}/slow", base1)).send().unwrap();
        (r.status().to_string(), r.version(), t.elapsed())
    });
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

    // ── Browser-style async pool ─────────────────────────────────────────
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

    // ── HTTP/2 multiplexed ───────────────────────────────────────────────
    println!("  HTTP/2 — multiplexed (no head-of-line blocking):");
    println!("  Client configured with: reqwest::Client::builder().http2_prior_knowledge()");
    println!("  Sending /slow AND /health CONCURRENTLY on ONE connection:\n");

    let base_url_owned = base_url.to_string();
    let start = Instant::now();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let (slow_result, health_result) = rt.block_on(async {
        let http2_client = reqwest::ClientBuilder::new()
            .http2_prior_knowledge()
            .build()
            .unwrap();

        let slow_url = format!("{}/slow", base_url_owned);
        let health_url = format!("{}/health", base_url_owned);
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
    println!(
        "  The {:?} format shows the ACTUAL protocol version from the wire.\n",
        "version=HTTP/2.0"
    );
}
