//! gRPC-style RPC over HTTP/2 with real gRPC 5-byte message framing.

use std::time::Instant;

/// Demonstrates gRPC-style RPC: unary and concurrent calls over HTTP/2.
pub fn demo(base_url: &str) {
    println!("\n  ═══ demo_grpc_style_rpc ═══\n");
    println!("  gRPC = HTTP/2 + protobuf + 5-byte message prefix.");
    println!("  We use the real gRPC wire format with JSON (same framing, easier to read).\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    let base = base_url.to_string();

    rt.block_on(async {
        let client = reqwest::ClientBuilder::new()
            .http2_prior_knowledge()
            .build()
            .unwrap();

        println!("  1) Unary RPC (like a normal function call):");
        println!("     POST /rpc/get_user with gRPC-framed body\n");

        let request_body = r#"{"user_id": 42}"#;
        let grpc_frame = encode(request_body.as_bytes());

        let start = Instant::now();
        let resp = client
            .post(format!("{}/api/users", base))
            .header("content-type", "application/grpc+json")
            .header("grpc-encoding", "identity")
            .body(grpc_frame.clone())
            .send()
            .await
            .unwrap();

        println!("     Request:  {} ({} bytes on wire: 5-byte prefix + {} payload)",
            request_body, grpc_frame.len(), request_body.len());
        println!("     Response: {} version={:?} ({:?})",
            resp.status(), resp.version(), start.elapsed());
        println!("     ^ Uses HTTP/2 POST to /service/method path (like gRPC)\n");

        println!("  gRPC message framing (5-byte prefix):");
        println!("  ┌──────────┬──────────────┬─────────────────────┐");
        println!("  │Compress 1B│ Length 4B BE │ Protobuf/JSON body  │");
        println!("  └──────────┴──────────────┴─────────────────────┘");
        println!("  Example: [0x00][0x00 0x00 0x00 0x0F][{{\"user_id\":42}}]");
        println!("           no compression  15 bytes    the message\n");

        println!("  2) Concurrent RPCs (multiplexed on one HTTP/2 connection):");
        println!("     Fire 5 \"RPCs\" at once — all on same connection\n");

        let rpcs = vec![
            ("GetUser",   "/api/users", r#"{"id":1}"#),
            ("GetHealth", "/health",    r#"{"check":"liveness"}"#),
            ("SlowQuery", "/slow",      r#"{"query":"SELECT * FROM big_table"}"#),
            ("GetUser2",  "/api/users", r#"{"id":2}"#),
            ("GetRoot",   "/",          r#"{"ping":true}"#),
        ];

        let start = Instant::now();
        let mut handles = vec![];

        for (name, path, body) in &rpcs {
            let client = client.clone();
            let url = format!("{}{}", base, path);
            let name = name.to_string();
            let frame = encode(body.as_bytes());

            handles.push(tokio::spawn(async move {
                let t = Instant::now();
                let resp = client
                    .post(&url)
                    .header("content-type", "application/grpc+json")
                    .body(frame)
                    .send()
                    .await
                    .unwrap();
                (name, resp.status().to_string(), resp.version(), t.elapsed())
            }));
        }

        let mut results = vec![];
        for h in handles {
            results.push(h.await.unwrap());
        }
        results.sort_by(|a, b| a.3.cmp(&b.3));

        for (name, status, version, elapsed) in &results {
            println!("     {:12} -> {} {:?} ({:?})", name, status, version, elapsed);
        }
        println!("     Total: {:?}", start.elapsed());
        println!("     ^ SlowQuery took 500ms but didn't block other RPCs!");
        println!("     All 5 RPCs multiplexed on ONE HTTP/2 connection.\n");
    });

    println!("  gRPC STREAMING MODES:");
    println!("  ┌────────────────────┬──────────────────────────────────────┐");
    println!("  │ Unary              │ req → resp (like normal HTTP)        │");
    println!("  │ Server streaming   │ req → resp1, resp2, resp3...         │");
    println!("  │ Client streaming   │ req1, req2, req3... → resp           │");
    println!("  │ Bidi streaming     │ msgs flowing both ways (like WS)     │");
    println!("  └────────────────────┴──────────────────────────────────────┘");
    println!();
    println!("  WHY gRPC OVER REST:");
    println!("  • Protobuf is 2-10x smaller than JSON (binary, no field names)");
    println!("  • HTTP/2 multiplexing = many RPCs on one connection");
    println!("  • Codegen from .proto file = typed client/server stubs");
    println!("  • Built-in streaming (REST needs SSE or WebSocket hacks)");
    println!("  • Deadlines, cancellation, metadata propagation built-in");
    println!();
}

/// Encode a message in gRPC wire format: [compressed: 0] [length: 4 bytes BE] [message]
fn encode(msg: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(5 + msg.len());
    frame.push(0);
    frame.extend_from_slice(&(msg.len() as u32).to_be_bytes());
    frame.extend_from_slice(msg);
    frame
}
