//! WebSocket demo using tokio-tungstenite (production WebSocket library).

use std::time::{Duration, Instant};

/// Real WebSocket demo: server + client with echo, ping/pong, and close.
pub fn demo() {
    println!("\n  ═══ demo_websocket ═══\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:9010")
            .await
            .unwrap();
        println!("  [WS Server] Listening on 127.0.0.1:9010");

        let server = tokio::spawn(async move {
            let (tcp_stream, peer) = listener.accept().await.unwrap();
            println!("  [WS Server] TCP connection from {}", peer);

            let ws_stream = tokio_tungstenite::accept_async(tcp_stream).await.unwrap();
            println!("  [WS Server] WebSocket handshake complete (HTTP → WebSocket)");

            let (mut write, mut read) = futures::StreamExt::split(ws_stream);

            let mut msg_count = 0u32;
            while let Some(Ok(msg)) = futures::StreamExt::next(&mut read).await {
                use tokio_tungstenite::tungstenite::Message;
                match msg {
                    Message::Text(text) => {
                        msg_count += 1;
                        println!("  [WS Server] Received #{}: \"{}\"", msg_count, text);
                        let echo = format!("echo: {}", text);
                        futures::SinkExt::send(&mut write, Message::Text(echo))
                            .await
                            .unwrap();
                    }
                    Message::Ping(data) => {
                        println!("  [WS Server] Ping received, sending Pong");
                        futures::SinkExt::send(&mut write, Message::Pong(data))
                            .await
                            .unwrap();
                    }
                    Message::Close(_) => {
                        println!("  [WS Server] Close frame received");
                        break;
                    }
                    _ => {}
                }
            }
            println!(
                "  [WS Server] Connection closed ({} messages exchanged)",
                msg_count
            );
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let handshake_start = Instant::now();
        let (ws_stream, response) = tokio_tungstenite::connect_async("ws://127.0.0.1:9010")
            .await
            .unwrap();
        let handshake_time = handshake_start.elapsed();

        println!("  [WS Client] Connected! Handshake: {:?}", handshake_time);
        println!("  [WS Client] HTTP response status: {}", response.status());
        println!(
            "  [WS Client] Upgrade: {}",
            response
                .headers()
                .get("upgrade")
                .map(|v| v.to_str().unwrap_or(""))
                .unwrap_or("none")
        );
        println!("  [WS Client] Protocol is now WebSocket (HTTP is gone)\n");

        let (mut write, mut read) = futures::StreamExt::split(ws_stream);
        use tokio_tungstenite::tungstenite::Message;

        let messages = ["Hello WebSocket!", "How are you?", "Real-time is cool"];
        for msg in &messages {
            let start = Instant::now();
            futures::SinkExt::send(&mut write, Message::Text((*msg).into()))
                .await
                .unwrap();
            if let Some(Ok(resp)) = futures::StreamExt::next(&mut read).await {
                println!(
                    "  [WS Client] Sent: \"{}\" → Got: \"{}\" ({:?})",
                    msg,
                    resp.into_text().unwrap_or_default(),
                    start.elapsed()
                );
            }
        }

        println!();
        let ping_start = Instant::now();
        futures::SinkExt::send(&mut write, Message::Ping(vec![1, 2, 3, 4]))
            .await
            .unwrap();
        if let Some(Ok(pong)) = futures::StreamExt::next(&mut read).await {
            println!(
                "  [WS Client] Ping → {:?} ({:?})",
                pong,
                ping_start.elapsed()
            );
        }

        futures::SinkExt::send(&mut write, Message::Close(None))
            .await
            .unwrap();
        println!("  [WS Client] Sent Close frame");
        server.await.ok();
    });

    println!();
    println!("  WEBSOCKET LIFECYCLE:");
    println!("  1. HTTP/1.1 Upgrade handshake (GET + 101 Switching Protocols)");
    println!("  2. TCP socket switches to WebSocket binary framing");
    println!("  3. Full-duplex: both sides send messages anytime (no req/resp)");
    println!("  4. Ping/Pong for keep-alive (detect dead connections)");
    println!("  5. Close frame for graceful shutdown");
    println!();
    println!("  Frame overhead: 2-6 bytes per message (vs ~800 bytes for HTTP headers)");
    println!("  Latency: <1ms per message (no HTTP overhead, just frame header)");
    println!();
}
