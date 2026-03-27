use pingora::lb::{selection::RoundRobin, LoadBalancer};
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use pingora::upstreams::peer::HttpPeer;
use serde_json::{json, Value};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub fn demo_pingora_proxy() {
    println!("\n  ═══ demo_pingora_proxy ═══\n");
    println!("  Running a REAL reverse proxy using Cloudflare's Pingora framework.\n");

    struct RoutingProxy {
        user_upstream: Arc<LoadBalancer<RoundRobin>>,
        order_upstream: Arc<LoadBalancer<RoundRobin>>,
    }

    #[async_trait::async_trait]
    impl ProxyHttp for RoutingProxy {
        type CTX = ();
        fn new_ctx(&self) -> Self::CTX {}

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
            _ctx: &mut Self::CTX,
        ) -> Result<()> {
            let path = upstream_request.uri.path().to_string();
            if let Some(rest) = path.strip_prefix("/api") {
                let new_uri = rest.parse().unwrap();
                upstream_request.set_uri(new_uri);
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

        let proxy = RoutingProxy {
            user_upstream: Arc::new(users),
            order_upstream: Arc::new(orders),
        };

        let mut svc = pingora::proxy::http_proxy_service(&server.configuration, proxy);
        svc.add_tcp("127.0.0.1:6188");

        server.add_service(svc);
        server.run(pingora::server::RunArgs::default());
    });

    thread::sleep(Duration::from_secs(2));

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    println!("    Pingora proxy on :6188 → backends on :9101, :9102\n");

    match client.get("http://127.0.0.1:6188/api/users/1").send() {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().unwrap_or_default();
            println!(
                "    GET /api/users/1   → {} service={:?}",
                status,
                body.get("service").unwrap_or(&json!("?"))
            );
        }
        Err(e) => println!("    GET /api/users/1   → ERROR: {}", e),
    }

    match client.get("http://127.0.0.1:6188/api/orders/42").send() {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().unwrap_or_default();
            println!(
                "    GET /api/orders/42  → {} service={:?}",
                status,
                body.get("service").unwrap_or(&json!("?"))
            );
        }
        Err(e) => println!("    GET /api/orders/42  → ERROR: {}", e),
    }

    println!("\n    This is Cloudflare's Pingora — the same framework powering their edge.");
    println!("    Path routing, load balancing, connection pooling, all built-in.\n");
}
