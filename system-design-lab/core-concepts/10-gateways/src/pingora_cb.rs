use axum::http::StatusCode;
use axum::response::Json;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use pingora::upstreams::peer::HttpPeer;
use pingora::http::ResponseHeader;
use bytes::Bytes;
use serde_json::json;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Shared circuit breaker state — tracks failures per backend
struct PingoraCircuitBreaker {
    failure_count: AtomicU32,
    threshold: u32,
    last_failure: AtomicU64,
    cooldown_secs: u64,
    is_open: std::sync::atomic::AtomicBool,
}

impl PingoraCircuitBreaker {
    fn new(threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            threshold,
            last_failure: AtomicU64::new(0),
            cooldown_secs,
            is_open: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn check_open(&self) -> bool {
        if !self.is_open.load(Ordering::Relaxed) {
            return false;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let last = self.last_failure.load(Ordering::Relaxed);
        if now - last >= self.cooldown_secs {
            self.is_open.store(false, Ordering::Relaxed);
            self.failure_count.store(0, Ordering::Relaxed);
            false
        } else {
            true
        }
    }

    fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        self.last_failure.store(now, Ordering::Relaxed);
        if count >= self.threshold {
            self.is_open.store(true, Ordering::Relaxed);
        }
    }

    fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.is_open.store(false, Ordering::Relaxed);
    }
}

pub fn demo_pingora_circuit_breaker() {
    println!("\n  ═══ demo_pingora_circuit_breaker ═══\n");
    println!("  Pingora proxy with circuit breaker — stops forwarding to failing backends.\n");

    // Start a backend that ALWAYS fails
    thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let app = axum::Router::new()
                .route("/data", axum::routing::get(|| async {
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "always down"})))
                }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:9105").await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });
    thread::sleep(Duration::from_millis(100));

    let cb: Arc<PingoraCircuitBreaker> = Arc::new(PingoraCircuitBreaker::new(2, 3));

    struct CBProxy {
        cb: Arc<PingoraCircuitBreaker>,
    }

    #[async_trait::async_trait]
    impl ProxyHttp for CBProxy {
        type CTX = ();
        fn new_ctx(&self) -> Self::CTX {}

        async fn request_filter(
            &self,
            session: &mut Session,
            _ctx: &mut Self::CTX,
        ) -> Result<bool> {
            if self.cb.check_open() {
                let body = b"{\"error\":\"circuit open\",\"message\":\"backend unavailable, try later\"}";
                let mut resp = ResponseHeader::build(503, Some(2))?;
                resp.insert_header("Content-Type", "application/json")?;
                resp.insert_header("Content-Length", body.len().to_string())?;
                session.write_response_header(Box::new(resp), false).await?;
                session.write_response_body(Some(Bytes::from_static(body)), true).await?;
                return Ok(true);
            }
            Ok(false)
        }

        async fn upstream_peer(
            &self,
            _session: &mut Session,
            _ctx: &mut Self::CTX,
        ) -> Result<Box<HttpPeer>> {
            let peer = HttpPeer::new("127.0.0.1:9105", false, String::new());
            Ok(Box::new(peer))
        }

        async fn response_filter(
            &self,
            _session: &mut Session,
            upstream_response: &mut ResponseHeader,
            _ctx: &mut Self::CTX,
        ) -> Result<()> {
            if upstream_response.status.as_u16() >= 500 {
                self.cb.record_failure();
            } else {
                self.cb.record_success();
            }
            Ok(())
        }

        fn fail_to_connect(
            &self,
            _session: &mut Session,
            _peer: &HttpPeer,
            _ctx: &mut Self::CTX,
            e: Box<pingora::Error>,
        ) -> Box<pingora::Error> {
            self.cb.record_failure();
            e
        }
    }

    let cb_clone = cb.clone();

    thread::spawn(move || {
        let mut server = Server::new(None).unwrap();
        server.bootstrap();

        let proxy = CBProxy { cb: cb_clone };

        let mut svc = pingora::proxy::http_proxy_service(&server.configuration, proxy);
        svc.add_tcp("127.0.0.1:6190");

        server.add_service(svc);
        server.run(pingora::server::RunArgs::default());
    });

    thread::sleep(Duration::from_secs(2));

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    println!("    Pingora CB proxy on :6190 → always-failing backend on :9105");
    println!("    Circuit opens after 2 failures, cooldown 3s\n");

    // Phase 1: CLOSED → failures pile up → circuit OPENS
    println!("    Phase 1: CLOSED → send requests to failing backend\n");
    for i in 1..=4 {
        let _is_open = cb.check_open();
        match client.get("http://127.0.0.1:6190/data").send() {
            Ok(resp) => {
                let status = resp.status();
                let state = if status == 503 { "OPEN→503 (didn't hit backend)" }
                    else if status.as_u16() >= 500 { "CLOSED→fail (backend returned 500)" }
                    else { "CLOSED→ok" };
                println!("    Req #{}: {} [{}]", i, status, state);
            }
            Err(e) => println!("    Req #{}: ERROR: {}", i, e),
        }
        thread::sleep(Duration::from_millis(100));
    }

    // Phase 2: Wait for cooldown → circuit transitions to HALF-OPEN
    println!("\n    Phase 2: Waiting 4s for cooldown (circuit → HALF-OPEN)...\n");
    thread::sleep(Duration::from_secs(4));

    // Phase 3: HALF-OPEN → try one request (backend still fails → re-opens)
    for i in 5..=7 {
        let is_open = cb.check_open();
        match client.get("http://127.0.0.1:6190/data").send() {
            Ok(resp) => {
                let status = resp.status();
                let state = if is_open && status == 503 { "OPEN→503" }
                    else if status.as_u16() >= 500 { "HALF-OPEN→fail (re-opens circuit)" }
                    else { "HALF-OPEN→ok (circuit closes)" };
                println!("    Req #{}: {} [{}]", i, status, state);
            }
            Err(e) => println!("    Req #{}: ERROR: {}", i, e),
        }
        thread::sleep(Duration::from_millis(100));
    }

    println!("\n    Pingora lifecycle:");
    println!("    request_filter()  → check circuit state, return 503 if OPEN");
    println!("    upstream_peer()   → route to backend (only if CLOSED)");
    println!("    response_filter() → track 5xx failures, open circuit if threshold hit");
    println!("    fail_to_connect() → also counts as failure\n");
    println!("    States: CLOSED (forward) → OPEN (reject 503) → HALF-OPEN (try one)\n");
}
