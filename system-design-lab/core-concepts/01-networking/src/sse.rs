//! Server-Sent Events (SSE) — real implementation using axum SSE + reqwest streaming.
//!
//! SSE is a simple protocol for server-to-client streaming:
//! - Uses plain HTTP with Content-Type: text/event-stream
//! - Server sends events in "id: N\nevent: type\ndata: payload\n\n" format
//! - Client auto-reconnects (Last-Event-ID header)
//! - Unidirectional: server → client only (unlike WebSocket)
use futures::StreamExt;
use std::time::{Duration, Instant};

/// Demonstrates real SSE using axum's Sse extractor on the server
/// and reqwest streaming on the client.
pub fn demo(_base_url: &str) {
    println!("\n  ═══ demo_sse ═══\n");

    // The axum server is already running (started in demo 2).
    // We add an SSE endpoint to it. Since we can't add routes dynamically,
    // we start a separate small SSE server here.

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // ── Start SSE server using axum's real Sse support ───────────────
        let sse_app = axum::Router::new()
            .route("/events", axum::routing::get(sse_handler));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:9011").await.unwrap();
        println!("  [SSE Server] axum SSE listening on 127.0.0.1:9011/events");

        let server = tokio::spawn(async move {
            axum::serve(listener, sse_app).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        // ── SSE client using reqwest streaming ───────────────────────────
        println!("  [SSE Client] Connecting to http://127.0.0.1:9011/events\n");

        let client = reqwest::ClientBuilder::new().build().unwrap();
        let start = Instant::now();

        let resp = client
            .get("http://127.0.0.1:9011/events")
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .send()
            .await
            .unwrap();

        println!("  [SSE Client] Connected! Status: {}", resp.status());
        println!(
            "  [SSE Client] Content-Type: {}",
            resp.headers()
                .get("content-type")
                .map(|v| v.to_str().unwrap_or(""))
                .unwrap_or("none")
        );
        println!("  [SSE Client] version={:?}\n", resp.version());

        // Read the SSE stream chunk by chunk
        let mut stream = resp.bytes_stream();
        let mut event_count = 0u32;
        let mut buffer = String::new();

        while let Some(Ok(chunk)) = stream.next().await {
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // SSE events are separated by double newlines
            while let Some(pos) = buffer.find("\n\n") {
                let event_text = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                // Parse the SSE event
                let mut id = String::new();
                let mut event_type = String::new();
                let mut data = String::new();

                for line in event_text.lines() {
                    if let Some(rest) = line.strip_prefix("id: ") {
                        id = rest.to_string();
                    } else if let Some(rest) = line.strip_prefix("event: ") {
                        event_type = rest.to_string();
                    } else if let Some(rest) = line.strip_prefix("data: ") {
                        data = rest.to_string();
                    }
                }

                if !data.is_empty() {
                    event_count += 1;
                    println!(
                        "  [SSE Client] Event #{} (id={}, type={}): {}",
                        event_count,
                        id,
                        if event_type.is_empty() { "message" } else { &event_type },
                        data
                    );
                }
            }
        }

        println!("\n  [SSE Client] Stream ended after {} events ({:?})", event_count, start.elapsed());
        server.abort();
    });

    println!();
    println!("  SSE PROTOCOL:");
    println!("  • Content-Type: text/event-stream (plain HTTP, long-lived response)");
    println!("  • Format: id: N\\nevent: type\\ndata: payload\\n\\n");
    println!("  • Server keeps connection open and writes events");
    println!("  • Client reads line by line (no binary framing, just text)");
    println!("  • Auto-reconnect: client sends Last-Event-ID on reconnect");
    println!("  • Unidirectional: server → client only");
    println!();
    println!("  SSE vs WebSocket:");
    println!("  ┌────────────┬──────────────────────┬──────────────────────┐");
    println!("  │            │ SSE                  │ WebSocket            │");
    println!("  ├────────────┼──────────────────────┼──────────────────────┤");
    println!("  │ Direction  │ Server → Client      │ Both ways            │");
    println!("  │ Transport  │ HTTP/1.1 (text)      │ WS frames (binary)   │");
    println!("  │ Reconnect  │ Automatic            │ Manual               │");
    println!("  │ Overhead   │ ~10 bytes/event      │ 2-6 bytes/frame      │");
    println!("  │ Use case   │ Live feeds, notifs   │ Chat, gaming, collab │");
    println!("  └────────────┴──────────────────────┴──────────────────────┘");
    println!();
}

/// axum SSE handler — streams events using axum's built-in Sse support.
async fn sse_handler() -> axum::response::sse::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    let events = vec![
        (1, "price", r#"{"symbol":"BTC","price":"$67,000"}"#),
        (2, "price", r#"{"symbol":"ETH","price":"$3,400"}"#),
        (3, "alert", r#"{"msg":"New block #19000000"}"#),
        (4, "price", r#"{"symbol":"BTC","price":"$67,100"}"#),
        (5, "status", r#"{"status":"stream_complete"}"#),
    ];

    let stream = async_stream::stream! {
        for (id, event_type, data) in events {
            println!("  [SSE Server] Sending event #{}: {} {}", id, event_type, data);
            let event = axum::response::sse::Event::default()
                .id(id.to_string())
                .event(event_type)
                .data(data);
            yield Ok::<_, std::convert::Infallible>(event);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };

    axum::response::sse::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
}
