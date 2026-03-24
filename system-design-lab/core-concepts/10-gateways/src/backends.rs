use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub fn start_backend_services() {
    // User service on :9101
    thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let app = axum::Router::new()
                .route("/users/:id", axum::routing::get(|Path(id): Path<u32>| async move {
                    Json(json!({"service": "user-service", "user_id": id, "name": "Alice", "email": "alice@example.com"}))
                }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:9101").await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });

    // Order service on :9102
    thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let app = axum::Router::new()
                .route("/orders/:id", axum::routing::get(|Path(id): Path<u32>| async move {
                    Json(json!({"service": "order-service", "order_id": id, "items": ["Widget", "Gadget"], "total": 42.99}))
                }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:9102").await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });

    // Flaky service on :9103 (fails 50% of the time — for circuit breaker demo)
    thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let counter = Arc::new(AtomicU32::new(0));
        rt.block_on(async {
            let counter = counter.clone();
            let app = axum::Router::new()
                .route("/data", axum::routing::get(move || {
                    let count = counter.fetch_add(1, Ordering::Relaxed);
                    async move {
                        if count % 2 == 0 {
                            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "service down"})))
                        } else {
                            (StatusCode::OK, Json(json!({"service": "flaky-service", "data": "success"})))
                        }
                    }
                }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:9103").await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });

    thread::sleep(Duration::from_millis(200));
    println!("  [Backend] User service on :9101, Order service on :9102, Flaky service on :9103\n");
}
