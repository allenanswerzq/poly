//! HTTP/2 demos: multiplexing (10 concurrent requests) and streaming large payloads.

use futures::StreamExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Fire 10 concurrent HTTP/2 requests on ONE connection and compare with HTTP/1.1.
pub fn demo_multiplexing(base_url: &str) {
    println!("\n  ═══ demo_http2_multiplexing ═══\n");

    let paths = vec![
        "/",
        "/health",
        "/api/users",
        "/slow",
        "/",
        "/health",
        "/api/users",
        "/slow",
        "/health",
        "/",
    ];

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
    println!(
        "  HTTP/1.1 total: {:?} (sequential — each waits for previous)\n",
        http1_total
    );

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

        let mut handles = Vec::new();
        for (i, path) in paths_owned.iter().enumerate() {
            let client = http2_client.clone();
            let url = format!("{}{}", base_url_owned, path);
            let path = path.to_string();
            handles.push(tokio::spawn(async move {
                let req_start = Instant::now();
                let resp = client.get(&url).send().await.unwrap();
                (
                    i + 1,
                    path,
                    resp.status(),
                    resp.version(),
                    req_start.elapsed(),
                )
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }
        results.sort_by_key(|r| r.0);
        results
    });

    for (i, path, status, version, elapsed) in &results {
        println!(
            "    req {:2} GET {:12} -> {} {:?} ({:?})",
            i, path, status, version, elapsed
        );
    }
    let http2_total = start.elapsed();
    println!(
        "  HTTP/2 total: {:?} (all 10 flew concurrently!)\n",
        http2_total
    );

    let speedup = http1_total.as_secs_f64() / http2_total.as_secs_f64();
    println!("  RESULTS:");
    println!(
        "  HTTP/1.1: {:?} (10 requests sequential on 1 connection)",
        http1_total
    );
    println!(
        "  HTTP/2:   {:?} (10 requests multiplexed on 1 connection)",
        http2_total
    );
    println!(
        "  Speedup:  {:.1}x faster with HTTP/2 multiplexing",
        speedup
    );
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

/// Demonstrates HTTP/2 streaming: 512KB received chunk by chunk while other requests complete.
pub fn demo_streaming(base_url: &str) {
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
                while let Some(result) = stream.next().await {
                    let chunk = result.unwrap();
                    total += chunk.len() as u64;
                    chunk_count += 1;
                    bytes_clone.store(total, Ordering::Relaxed);
                }
                (version, total, chunk_count, start.elapsed())
            },
            async {
                tokio::time::sleep(Duration::from_millis(2)).await;
                let start = Instant::now();
                let resp = client
                    .get(format!("{}/health", base_url_owned))
                    .send()
                    .await
                    .unwrap();
                let streamed_so_far = bytes_clone.load(Ordering::Relaxed);
                let body = resp.text().await.unwrap();
                ("/health", body, start.elapsed(), streamed_so_far)
            },
            async {
                tokio::time::sleep(Duration::from_millis(3)).await;
                let start = Instant::now();
                let resp = client
                    .get(format!("{}/", base_url_owned))
                    .send()
                    .await
                    .unwrap();
                let streamed_so_far = bytes_clone.load(Ordering::Relaxed);
                let body = resp.text().await.unwrap();
                ("/", body, start.elapsed(), streamed_so_far)
            },
            async {
                tokio::time::sleep(Duration::from_millis(4)).await;
                let start = Instant::now();
                let resp = client
                    .get(format!("{}/api/users", base_url_owned))
                    .send()
                    .await
                    .unwrap();
                let streamed_so_far = bytes_clone.load(Ordering::Relaxed);
                let body = resp.text().await.unwrap();
                ("/api/users", body, start.elapsed(), streamed_so_far)
            },
        );

        let total_time = overall_start.elapsed();
        for (path, body, elapsed, streamed) in [r1, r2, r3] {
            println!(
                "    GET {:12} -> {} ({:?}) ← stream had {}KB when this finished",
                path,
                body,
                elapsed,
                streamed / 1024
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
    println!("  Flow control prevents overwhelming the receiver:");
    println!("    • Each stream has a receive window (default 64KB)");
    println!("    • Sender pauses when window is full (WINDOW_UPDATE to resume)");
    println!("    • Pausing one stream doesn't affect others");
    println!();
}
