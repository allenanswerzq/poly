use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use dashmap::DashMap;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

pub fn demo_auth_middleware() {
    println!("\n  ═══ demo_auth_middleware ═══\n");
    println!("  Gateway validates API keys before forwarding to backend:\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let valid_keys: Arc<DashMap<String, String>> = Arc::new(DashMap::new());
        valid_keys.insert("sk-valid-key-123".into(), "user-42".into());
        valid_keys.insert("sk-admin-key-456".into(), "admin-1".into());

        let keys = valid_keys.clone();
        let auth_gateway = axum::Router::new()
            .route("/api/users/:id", axum::routing::get(move |
                Path(id): Path<u32>,
                headers: HeaderMap,
            | {
                let keys = keys.clone();
                async move {
                    let api_key = headers.get("x-api-key")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");

                    match keys.get(api_key) {
                        Some(user_id) => {
                            let client = reqwest::Client::new();
                            let resp = client.get(format!("http://127.0.0.1:9101/users/{}", id))
                                .header("X-Authenticated-User", user_id.value().clone())
                                .send().await.unwrap();
                            let body: Value = resp.json().await.unwrap();
                            (StatusCode::OK, Json(json!({
                                "authenticated_as": user_id.value(),
                                "data": body
                            })))
                        }
                        None => {
                            (StatusCode::UNAUTHORIZED, Json(json!({
                                "error": "Invalid API key",
                                "hint": "Set X-Api-Key header"
                            })))
                        }
                    }
                }
            }));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:9104").await.unwrap();
        let server = tokio::spawn(async { axum::serve(listener, auth_gateway).await.unwrap() });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();

        let resp = client.get("http://127.0.0.1:9104/api/users/1").send().await.unwrap();
        println!("    No API key:      {} {}", resp.status(), resp.text().await.unwrap());

        let resp = client.get("http://127.0.0.1:9104/api/users/1")
            .header("X-Api-Key", "sk-invalid")
            .send().await.unwrap();
        println!("    Invalid key:     {} {}", resp.status(), resp.text().await.unwrap());

        let resp = client.get("http://127.0.0.1:9104/api/users/1")
            .header("X-Api-Key", "sk-valid-key-123")
            .send().await.unwrap();
        println!("    Valid key:       {} (forwarded to backend)", resp.status());

        println!("\n    Gateway validates auth ONCE. Backend trusts X-Authenticated-User header.\n");
        server.abort();
    });
}
