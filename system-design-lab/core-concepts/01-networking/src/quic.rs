//! HTTP/3 / QUIC demo using quinn (production QUIC library).

use std::sync::Arc;
use std::time::{Duration, Instant};

/// Real QUIC demo: server + client over UDP with multiple independent streams.
pub fn demo() {
    println!("\n  ═══ demo_real_quic ═══\n");
    println!("  Using quinn (production QUIC library) for real QUIC over UDP.\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = rustls_pki_types::CertificateDer::from(cert.cert);
        let key_der = rustls_pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
        println!("  [Setup] Generated self-signed TLS 1.3 cert (QUIC mandates encryption)");

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

        let server = tokio::spawn(async move {
            let incoming = server_endpoint.accept().await.unwrap();
            let connection = incoming.await.unwrap();
            println!("  [QUIC Server] Client connected (conn_id={})", connection.stable_id());

            loop {
                match connection.accept_bi().await {
                    Ok((mut send, mut recv)) => {
                        tokio::spawn(async move {
                            let data = recv.read_to_end(4096).await.unwrap();
                            let request = String::from_utf8_lossy(&data);
                            if request.contains("slow") {
                                tokio::time::sleep(Duration::from_millis(300)).await;
                            }
                            let response = format!("QUIC OK: {}", request);
                            send.write_all(response.as_bytes()).await.unwrap();
                            send.finish().unwrap();
                        });
                    }
                    Err(_) => break,
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert_der).unwrap();
        let client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto).unwrap(),
        ));

        let mut client_endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        client_endpoint.set_default_client_config(client_config);

        let handshake_start = Instant::now();
        let connection = client_endpoint.connect(server_addr, "localhost").unwrap().await.unwrap();
        let handshake_time = handshake_start.elapsed();

        println!("  [QUIC Client] Connected! Handshake: {:?} (1 RTT, TLS 1.3 included!)", handshake_time);
        println!("  [QUIC Client] Transport: UDP (not TCP)");
        println!("  [QUIC Client] Connection ID: {} (survives IP changes!)\n", connection.stable_id());

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
        results.sort_by(|a, b| a.3.cmp(&b.3));

        for (name, req, resp, elapsed) in &results {
            println!("    {} | {} -> {} ({:?})", name, req, resp, elapsed);
        }

        let total = overall_start.elapsed();
        println!("\n  Total: {:?}", total);
        println!("  ^ /slow (300ms) did NOT block the fast streams!\n");

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
