//! TLS demos:
//! 1. Simple: HTTPS request with reqwest (TLS happens automatically)
//! 2. Under the hood: raw rustls server + client showing the handshake details

use std::sync::Arc;
use std::time::Instant;

/// Simple TLS demo: make real HTTPS requests and inspect the TLS details.
/// Then show the raw handshake with rustls for understanding.
pub fn demo() {
    println!("\n  ═══ demo_tls ═══\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // ── Part 1: HTTPS with reqwest (TLS is automatic) ────────────────
        println!("  PART 1: HTTPS with reqwest (TLS happens automatically)\n");

        let client = reqwest::Client::new();

        // Make a real HTTPS request — TLS handshake happens under the hood
        let start = Instant::now();
        let resp = client
            .get("https://httpbin.org/get")
            .send()
            .await
            .unwrap();

        println!("  [HTTPS] GET https://httpbin.org/get");
        println!("  [HTTPS] Status: {}", resp.status());
        println!("  [HTTPS] Version: {:?}", resp.version());
        println!("  [HTTPS] Time: {:?} (includes DNS + TCP + TLS + request)", start.elapsed());
        println!();

        // Second request — TLS session reused (much faster)
        let start = Instant::now();
        let resp2 = client
            .get("https://httpbin.org/headers")
            .send()
            .await
            .unwrap();

        println!("  [HTTPS] GET https://httpbin.org/headers (same client)");
        println!("  [HTTPS] Status: {}", resp2.status());
        println!("  [HTTPS] Time: {:?} (TLS session reused!)", start.elapsed());
        println!();

        println!("  That's it! reqwest handles TLS automatically:");
        println!("  • Verifies server certificate against system CA store");
        println!("  • Negotiates TLS 1.3 (or 1.2 fallback)");
        println!("  • Uses ALPN to negotiate HTTP/2 if server supports it");
        println!("  • Connection pool reuses TLS sessions\n");
    });

    // ── Part 2: Under the hood with rustls ───────────────────────────────
    println!("  PART 2: Under the hood — raw TLS handshake with rustls\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // Generate self-signed cert (in production: Let's Encrypt)
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = rustls_pki_types::CertificateDer::from(cert.cert);
        let key_der = rustls_pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

        println!("  [Setup] Generated self-signed X.509 cert ({} bytes)", cert_der.len());

        // Server config
        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert_der.clone()],
                rustls_pki_types::PrivateKeyDer::Pkcs8(key_der),
            )
            .unwrap();

        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:9008").await.unwrap();

        // Server task
        let acceptor_clone = acceptor.clone();
        let server = tokio::spawn(async move {
            let (tcp, peer) = listener.accept().await.unwrap();
            let handshake_start = Instant::now();
            let tls = acceptor_clone.accept(tcp).await.unwrap();
            let (_, conn) = tls.get_ref();

            println!("  [Server] TLS handshake with {} ({:?})", peer, handshake_start.elapsed());
            println!("  [Server] Protocol: {:?}", conn.protocol_version().unwrap());
            println!("  [Server] Cipher: {:?}", conn.negotiated_cipher_suite().unwrap());

            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut tls = tls;
            let mut buf = [0u8; 4096];
            let n = tls.read(&mut buf).await.unwrap();
            println!("  [Server] Request: {}", String::from_utf8_lossy(&buf[..n]).lines().next().unwrap_or(""));

            let resp = "HTTP/1.1 200 OK\r\nContent-Length: 21\r\n\r\nHello over TLS 1.3!\n";
            tls.write_all(resp.as_bytes()).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Client
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert_der).unwrap();
        let client_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();

        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
        let tcp = tokio::net::TcpStream::connect("127.0.0.1:9008").await.unwrap();

        let handshake_start = Instant::now();
        let server_name = rustls_pki_types::ServerName::try_from("localhost").unwrap();
        let mut tls = connector.connect(server_name, tcp).await.unwrap();
        let (_, conn) = tls.get_ref();

        println!("\n  [Client] TLS handshake: {:?}", handshake_start.elapsed());
        println!("  [Client] Protocol: {:?}", conn.protocol_version().unwrap());
        println!("  [Client] Cipher: {:?}", conn.negotiated_cipher_suite().unwrap());

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        tls.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").await.unwrap();
        let mut buf = [0u8; 4096];
        let n = tls.read(&mut buf).await.unwrap();
        println!("  [Client] Response: {}", String::from_utf8_lossy(&buf[..n]).lines().last().unwrap_or(""));

        server.await.ok();
    });

    println!();
    println!("  TLS 1.3: 1 RTT handshake, strong ciphers only, 0-RTT on resume.");
    println!("  In practice: just use reqwest/axum — they handle TLS for you.");
    println!("  Raw rustls is for understanding what happens under the hood.");
    println!();
}
