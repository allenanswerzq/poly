use axum::extract::Path;
use axum::response::Json;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn demo_reverse_proxy() {
    println!("\n  ═══ demo_reverse_proxy ═══\n");
    println!("  Gateway routes requests to different backends based on URL path:\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let _routes: Arc<Vec<(&str, &str)>> = Arc::new(vec![
            ("/api/users", "http://127.0.0.1:9101/users"),
            ("/api/orders", "http://127.0.0.1:9102/orders"),
        ]);

        let gateway = axum::Router::new()
            .route("/api/users/:id", axum::routing::get({
                move |Path(id): Path<u32>| async move {
                    let client = reqwest::Client::new();
                    let resp = client.get(format!("http://127.0.0.1:9101/users/{}", id))
                        .send().await.unwrap();
                    let body: Value = resp.json().await.unwrap();
                    Json(body)
                }
            }))
            .route("/api/orders/:id", axum::routing::get({
                move |Path(id): Path<u32>| async move {
                    let client = reqwest::Client::new();
                    let resp = client.get(format!("http://127.0.0.1:9102/orders/{}", id))
                        .send().await.unwrap();
                    let body: Value = resp.json().await.unwrap();
                    Json(body)
                }
            }));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:9100").await.unwrap();
        let server = tokio::spawn(async { axum::serve(listener, gateway).await.unwrap() });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();

        for (path, desc) in [("/api/users/1", "User Service"), ("/api/orders/42", "Order Service")] {
            let start = Instant::now();
            let resp = client.get(format!("http://127.0.0.1:9100{}", path))
                .send().await.unwrap();
            let body: Value = resp.json().await.unwrap();
            println!("    GET {} → {} → {:?} ({:?})",
                path, desc, body.get("service").unwrap(), start.elapsed());
        }

        println!("\n    Client sees ONE endpoint (gateway:9100).");
        println!("    Gateway routes /api/users → :9101, /api/orders → :9102.");
        println!("    Backend topology is hidden from the client.\n");
        server.abort();
    });
}
