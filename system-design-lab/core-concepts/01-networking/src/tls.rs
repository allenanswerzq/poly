//! TLS demo — real TLS 1.3 handshake using rustls + tokio-rustls.

use std::sync::Arc;
use std::time::{Duration, Instant};

/// Real TLS demo: generate a cert, start a TLS server, connect with a TLS client.
pub fn demo() {
    println!("\n  ═══ demo_tls ═══\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = rustls_pki_types::CertificateDer::from(cert.cert);
        let key_der = rustls_pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

        println!("  [Step 1] Generated self-signed X.509 certificate");
        println!("    Subject: localhost");
        println!("    Cert size: {} bytes (DER encoded)", cert_der.len());
        println!("    Key type: PKCS#8 private key\n");

        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert_der.clone()],
                rustls_pki_types::PrivateKeyDer::Pkcs8(key_der),
            )
            .unwrap();

        let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:9008").await.unwrap();
        println!("  [Step 2] TLS server listening on 127.0.0.1:9008");

        let acceptor = tls_acceptor.clone();
        let server = tokio::spawn(async move {
            let (tcp_stream, peer) = listener.accept().await.unwrap();
            println!("  [Server] TCP connection from {}", peer);

            let handshake_start = Instant::now();
            let tls_stream = acceptor.accept(tcp_stream).await.unwrap();
            let handshake_time = handshake_start.elapsed();

            let (_, server_conn) = tls_stream.get_ref();
            println!("  [Server] TLS handshake complete ({:?})", handshake_time);
            println!("  [Server] Protocol: {:?}", server_conn.protocol_version().unwrap());
            println!("  [Server] Cipher: {:?}", server_conn.negotiated_cipher_suite().unwrap());
            println!("  [Server] ALPN: {:?}",
                server_conn.alpn_protocol().map(|p| String::from_utf8_lossy(p).to_string()));

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
        println!("  [Client] Protocol: {:?}", client_conn.protocol_version().unwrap());
        println!("  [Client] Cipher: {:?}", client_conn.negotiated_cipher_suite().unwrap());

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
    println!("  │   │── ClientHello ────────────────────►│  (1 RTT)      │");
    println!("  │   │◄── ServerHello + Certificate ──────│  (1 RTT)      │");
    println!("  │   │── Finished ───────────────────────►│               │");
    println!("  │   │═══ Encrypted Application Data ════│               │");
    println!("  └─────────────────────────────────────────────────────────┘");
    println!();
    println!("  TLS 1.3: 1 RTT handshake, strong ciphers only, 0-RTT on resume.");
    println!("  TLS 1.2: 2 RTT, mixed ciphers. TLS 1.0/1.1: DEAD.");
    println!();
}
