use bytes::Bytes;
use pingora::http::ResponseHeader;
use pingora::lb::{selection::RoundRobin, LoadBalancer};
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use pingora::upstreams::peer::HttpPeer;
use serde_json::{json, Value};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub fn demo_pingora_auth() {
    println!("\n  ═══ demo_pingora_auth ═══\n");
    println!("  Pingora proxy with API key authentication middleware.\n");

    static API_KEYS: &[(&str, &str)] = &[
        ("sk-valid-key-123", "user-42"),
        ("sk-admin-key-456", "admin-1"),
    ];

    struct AuthProxy {
        user_upstream: Arc<LoadBalancer<RoundRobin>>,
        order_upstream: Arc<LoadBalancer<RoundRobin>>,
    }

    #[async_trait::async_trait]
    impl ProxyHttp for AuthProxy {
        type CTX = Option<String>;
        fn new_ctx(&self) -> Self::CTX {
            None
        }

        /// Auth gate — runs BEFORE routing. Rejects unauthenticated requests.
        async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
            let api_key = session
                .req_header()
                .headers
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            match API_KEYS.iter().find(|(k, _)| *k == api_key) {
                Some((_, user_id)) => {
                    *ctx = Some(user_id.to_string());
                    Ok(false)
                }
                None => {
                    let body = b"{\"error\":\"unauthorized\",\"hint\":\"set X-Api-Key header\"}";
                    let mut resp = ResponseHeader::build(401, Some(2))?;
                    resp.insert_header("Content-Type", "application/json")?;
                    resp.insert_header("Content-Length", body.len().to_string())?;
                    session.write_response_header(Box::new(resp), false).await?;
                    session
                        .write_response_body(Some(Bytes::from_static(body)), true)
                        .await?;
                    Ok(true)
                }
            }
        }

        async fn upstream_peer(
            &self,
            session: &mut Session,
            _ctx: &mut Self::CTX,
        ) -> Result<Box<HttpPeer>> {
            let path = session.req_header().uri.path();

            let upstream = if path.starts_with("/api/users") {
                self.user_upstream.select(b"", 256)
            } else if path.starts_with("/api/orders") {
                self.order_upstream.select(b"", 256)
            } else {
                None
            };

            let upstream = upstream.ok_or_else(|| pingora::Error::new_str("no route matched"))?;

            let peer = HttpPeer::new(upstream, false, String::new());
            Ok(Box::new(peer))
        }

        async fn upstream_request_filter(
            &self,
            _session: &mut Session,
            upstream_request: &mut pingora::http::RequestHeader,
            ctx: &mut Self::CTX,
        ) -> Result<()> {
            let path = upstream_request.uri.path().to_string();
            if let Some(rest) = path.strip_prefix("/api") {
                let new_uri = rest.parse().unwrap();
                upstream_request.set_uri(new_uri);
            }

            if let Some(user_id) = ctx {
                upstream_request.insert_header("X-Authenticated-User", user_id.as_str())?;
            }

            upstream_request.insert_header("X-Forwarded-By", "pingora-gateway")?;
            Ok(())
        }
    }

    thread::spawn(|| {
        let mut server = Server::new(None).unwrap();
        server.bootstrap();

        let users = LoadBalancer::try_from_iter(["127.0.0.1:9101"]).unwrap();
        let orders = LoadBalancer::try_from_iter(["127.0.0.1:9102"]).unwrap();

        let proxy = AuthProxy {
            user_upstream: Arc::new(users),
            order_upstream: Arc::new(orders),
        };

        let mut svc = pingora::proxy::http_proxy_service(&server.configuration, proxy);
        svc.add_tcp("127.0.0.1:6189");

        server.add_service(svc);
        server.run(pingora::server::RunArgs::default());
    });

    thread::sleep(Duration::from_secs(2));

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    println!("    Pingora auth proxy on :6189\n");

    match client.get("http://127.0.0.1:6189/api/users/1").send() {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            println!("    No key:      GET /api/users/1    → {} {}", status, body);
        }
        Err(e) => println!("    No key:      GET /api/users/1    → ERROR: {}", e),
    }

    match client
        .get("http://127.0.0.1:6189/api/users/1")
        .header("X-Api-Key", "sk-wrong")
        .send()
    {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            println!("    Bad key:     GET /api/users/1    → {} {}", status, body);
        }
        Err(e) => println!("    Bad key:     GET /api/users/1    → ERROR: {}", e),
    }

    match client
        .get("http://127.0.0.1:6189/api/users/1")
        .header("X-Api-Key", "sk-valid-key-123")
        .send()
    {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().unwrap_or_default();
            println!(
                "    Valid key:   GET /api/users/1    → {} service={:?}",
                status,
                body.get("service").unwrap_or(&json!("?"))
            );
        }
        Err(e) => println!("    Valid key:   GET /api/users/1    → ERROR: {}", e),
    }

    match client
        .get("http://127.0.0.1:6189/api/orders/42")
        .header("X-Api-Key", "sk-admin-key-456")
        .send()
    {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().unwrap_or_default();
            println!(
                "    Admin key:   GET /api/orders/42  → {} service={:?}",
                status,
                body.get("service").unwrap_or(&json!("?"))
            );
        }
        Err(e) => println!("    Admin key:   GET /api/orders/42  → ERROR: {}", e),
    }

    println!("\n    Pingora lifecycle for each request:");
    println!("    request_filter() → validate API key, reject 401 if invalid");
    println!("    upstream_peer()  → pick backend based on path");
    println!("    upstream_request_filter() → rewrite path, add X-Authenticated-User");
    println!("    Backend never sees unauthenticated traffic.\n");
}
