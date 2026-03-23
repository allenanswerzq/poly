//! Shared HTTP server (axum) used by HTTP/1.1, HTTP/2, and gRPC demos.

use serde_json::json;
use std::thread;
use std::time::Duration;

/// Start a real HTTP server using axum (built on hyper).
/// Supports both GET and POST on all routes (POST needed for gRPC demo).
/// Runs on a background thread with its own tokio runtime.
pub fn start_http_server(addr: &str) {
    let addr = addr.to_string();
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let app = axum::Router::new()
                .route("/", axum::routing::get(handle_root).post(handle_root))
                .route("/health", axum::routing::get(handle_health).post(handle_health))
                .route("/slow", axum::routing::get(handle_slow).post(handle_slow))
                .route("/api/users", axum::routing::get(handle_users).post(handle_users))
                .route("/stream/:size_kb", axum::routing::get(handle_stream));

            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            println!("[HTTP Server] axum + hyper listening on {}", addr);
            axum::serve(listener, app).await.unwrap();
        });
    });
}

async fn handle_root() -> axum::Json<serde_json::Value> {
    axum::Json(json!({"message": "hello from HTTP/1.1"}))
}

async fn handle_health() -> axum::Json<serde_json::Value> {
    axum::Json(json!({"status": "healthy"}))
}

async fn handle_slow() -> axum::Json<serde_json::Value> {
    tokio::time::sleep(Duration::from_millis(500)).await;
    axum::Json(json!({"message": "slow response (500ms)"}))
}

async fn handle_users() -> axum::Json<serde_json::Value> {
    axum::Json(json!([{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]))
}

/// Streams a response body in 16KB chunks — does NOT buffer the full payload.
async fn handle_stream(
    axum::extract::Path(size_kb): axum::extract::Path<u32>,
) -> axum::body::Body {
    let chunk_size = 16 * 1024;
    let total_bytes = (size_kb as usize) * 1024;

    let stream = async_stream::stream! {
        let mut sent = 0usize;
        while sent < total_bytes {
            let this_chunk = chunk_size.min(total_bytes - sent);
            let chunk = vec![b'X'; this_chunk];
            sent += this_chunk;
            yield Ok::<_, std::io::Error>(bytes::Bytes::from(chunk));
        }
    };
    axum::body::Body::from_stream(stream)
}
