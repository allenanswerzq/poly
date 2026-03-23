//! # API Design Demo
//!
//! Demonstrates API design patterns every staff engineer should know:
//! 1. REST API — CRUD, proper status codes, resource design
//! 2. Pagination — offset vs cursor-based
//! 3. Idempotency — safe retries with idempotency keys
//! 4. API Versioning — URL, header, and query param strategies
//! 5. Bulk/Batch APIs — efficient multi-resource operations
//! 6. Long-running operations — async job pattern
//! 7. GraphQL-style API — single endpoint, field selection

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

// =============================================================================
// Shared state for the API server
// =============================================================================

#[derive(Clone)]
struct AppState {
    users: Arc<RwLock<Vec<User>>>,
    idempotency_store: Arc<RwLock<HashMap<String, Value>>>,
    jobs: Arc<RwLock<HashMap<String, Job>>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct User {
    id: String,
    name: String,
    email: String,
    created_at: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Job {
    id: String,
    status: String, // "pending", "running", "completed", "failed"
    result: Option<Value>,
}

fn new_state() -> AppState {
    let users = vec![
        User { id: "u1".into(), name: "Alice".into(), email: "alice@example.com".into(), created_at: "2024-01-01T00:00:00Z".into() },
        User { id: "u2".into(), name: "Bob".into(), email: "bob@example.com".into(), created_at: "2024-01-02T00:00:00Z".into() },
        User { id: "u3".into(), name: "Carol".into(), email: "carol@example.com".into(), created_at: "2024-01-03T00:00:00Z".into() },
        User { id: "u4".into(), name: "Dave".into(), email: "dave@example.com".into(), created_at: "2024-01-04T00:00:00Z".into() },
        User { id: "u5".into(), name: "Eve".into(), email: "eve@example.com".into(), created_at: "2024-01-05T00:00:00Z".into() },
        User { id: "u6".into(), name: "Frank".into(), email: "frank@example.com".into(), created_at: "2024-01-06T00:00:00Z".into() },
        User { id: "u7".into(), name: "Grace".into(), email: "grace@example.com".into(), created_at: "2024-01-07T00:00:00Z".into() },
    ];
    AppState {
        users: Arc::new(RwLock::new(users)),
        idempotency_store: Arc::new(RwLock::new(HashMap::new())),
        jobs: Arc::new(RwLock::new(HashMap::new())),
    }
}

// =============================================================================
// API Server — all patterns in one axum server
// =============================================================================

fn start_api_server() {
    let state = new_state();
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let app = axum::Router::new()
                // REST CRUD
                .route("/api/v1/users", axum::routing::get(list_users).post(create_user))
                .route("/api/v1/users/:id", axum::routing::get(get_user).put(update_user).delete(delete_user))
                // Pagination
                .route("/api/v1/users-paginated", axum::routing::get(list_users_paginated))
                // Idempotent create
                .route("/api/v1/payments", axum::routing::post(create_payment))
                // Versioning: v2 endpoint
                .route("/api/v2/users", axum::routing::get(list_users_v2))
                // Batch
                .route("/api/v1/users/batch", axum::routing::post(batch_get_users))
                // Long-running async job
                .route("/api/v1/reports", axum::routing::post(start_report))
                .route("/api/v1/reports/:id", axum::routing::get(get_report))
                // GraphQL-style field selection
                .route("/api/v1/users-fields", axum::routing::get(list_users_fields))
                .with_state(state);

            let listener = tokio::net::TcpListener::bind("127.0.0.1:9200").await.unwrap();
            println!("[API Server] Listening on 127.0.0.1:9200\n");
            axum::serve(listener, app).await.unwrap();
        });
    });
    thread::sleep(Duration::from_millis(200));
}

// ── REST CRUD handlers ───────────────────────────────────────────────────────

async fn list_users(State(state): State<AppState>) -> Json<Value> {
    let users = state.users.read().unwrap();
    Json(json!({ "data": *users, "count": users.len() }))
}

async fn get_user(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let users = state.users.read().unwrap();
    users.iter().find(|u| u.id == id)
        .map(|u| Json(json!(u)))
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct CreateUser { name: String, email: String }

async fn create_user(State(state): State<AppState>, Json(body): Json<CreateUser>) -> (StatusCode, Json<Value>) {
    let user = User {
        id: format!("u{}", Uuid::new_v4().to_string().split('-').next().unwrap()),
        name: body.name,
        email: body.email,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let resp = json!(user);
    state.users.write().unwrap().push(user);
    (StatusCode::CREATED, Json(resp))
}

async fn update_user(State(state): State<AppState>, Path(id): Path<String>, Json(body): Json<CreateUser>) -> Result<Json<Value>, StatusCode> {
    let mut users = state.users.write().unwrap();
    if let Some(user) = users.iter_mut().find(|u| u.id == id) {
        user.name = body.name;
        user.email = body.email;
        Ok(Json(json!(user.clone())))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn delete_user(State(state): State<AppState>, Path(id): Path<String>) -> StatusCode {
    let mut users = state.users.write().unwrap();
    let len_before = users.len();
    users.retain(|u| u.id != id);
    if users.len() < len_before { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }
}

// ── Pagination handler ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PaginationParams {
    cursor: Option<String>,
    limit: Option<usize>,
}

async fn list_users_paginated(State(state): State<AppState>, Query(params): Query<PaginationParams>) -> Json<Value> {
    let users = state.users.read().unwrap();
    let limit = params.limit.unwrap_or(3);

    let start = match &params.cursor {
        Some(cursor) => users.iter().position(|u| u.id == *cursor).map(|p| p + 1).unwrap_or(0),
        None => 0,
    };

    let page: Vec<&User> = users.iter().skip(start).take(limit).collect();
    let next_cursor = if start + limit < users.len() {
        page.last().map(|u| u.id.clone())
    } else {
        None
    };

    Json(json!({
        "data": page,
        "next_cursor": next_cursor,
        "has_more": next_cursor.is_some(),
    }))
}

// ── Idempotency handler ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PaymentRequest { amount: f64, to: String }

async fn create_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PaymentRequest>,
) -> (StatusCode, Json<Value>) {
    let idempotency_key = headers.get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // Check if we already processed this key
    if !idempotency_key.is_empty() {
        let store = state.idempotency_store.read().unwrap();
        if let Some(cached) = store.get(&idempotency_key) {
            return (StatusCode::OK, Json(json!({
                "payment": cached,
                "_idempotent": true,
                "_note": "Returned cached result (same Idempotency-Key)"
            })));
        }
    }

    // Process payment
    let payment = json!({
        "id": format!("pay_{}", Uuid::new_v4().to_string().split('-').next().unwrap()),
        "amount": body.amount,
        "to": body.to,
        "status": "completed",
    });

    // Cache the result
    if !idempotency_key.is_empty() {
        state.idempotency_store.write().unwrap().insert(idempotency_key, payment.clone());
    }

    (StatusCode::CREATED, Json(json!({ "payment": payment })))
}

// ── Versioning: v2 returns different format ──────────────────────────────────

async fn list_users_v2(State(state): State<AppState>) -> Json<Value> {
    let users = state.users.read().unwrap();
    let slim: Vec<Value> = users.iter().map(|u| json!({
        "id": u.id,
        "display_name": format!("{} <{}>", u.name, u.email),
    })).collect();
    Json(json!({ "version": "v2", "users": slim }))
}

// ── Batch handler ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct BatchRequest { ids: Vec<String> }

async fn batch_get_users(State(state): State<AppState>, Json(body): Json<BatchRequest>) -> Json<Value> {
    let users = state.users.read().unwrap();
    let results: Vec<Value> = body.ids.iter().map(|id| {
        match users.iter().find(|u| &u.id == id) {
            Some(u) => json!({"id": id, "found": true, "user": u}),
            None => json!({"id": id, "found": false}),
        }
    }).collect();
    Json(json!({ "results": results }))
}

// ── Long-running async job ───────────────────────────────────────────────────

async fn start_report(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let job_id = format!("job_{}", Uuid::new_v4().to_string().split('-').next().unwrap());
    let job = Job { id: job_id.clone(), status: "pending".into(), result: None };
    state.jobs.write().unwrap().insert(job_id.clone(), job);

    // Simulate background processing
    let jobs = state.jobs.clone();
    let jid = job_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if let Some(job) = jobs.write().unwrap().get_mut(&jid) {
            job.status = "completed".into();
            job.result = Some(json!({"rows": 1234, "file": "report.csv"}));
        }
    });

    (StatusCode::ACCEPTED, Json(json!({
        "job_id": job_id,
        "status": "pending",
        "poll_url": format!("/api/v1/reports/{}", job_id),
    })))
}

async fn get_report(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let jobs = state.jobs.read().unwrap();
    jobs.get(&id).map(|j| Json(json!(j))).ok_or(StatusCode::NOT_FOUND)
}

// ── Field selection (GraphQL-style) ──────────────────────────────────────────

#[derive(Deserialize)]
struct FieldParams { fields: Option<String> }

async fn list_users_fields(State(state): State<AppState>, Query(params): Query<FieldParams>) -> Json<Value> {
    let users = state.users.read().unwrap();
    let fields: Vec<&str> = params.fields.as_deref().unwrap_or("id,name,email").split(',').collect();

    let filtered: Vec<Value> = users.iter().map(|u| {
        let full = json!(u);
        let mut obj = serde_json::Map::new();
        for f in &fields {
            let f = f.trim();
            if let Some(v) = full.get(f) {
                obj.insert(f.to_string(), v.clone());
            }
        }
        Value::Object(obj)
    }).collect();

    Json(json!({ "fields": fields, "data": filtered }))
}

// =============================================================================
// Demo functions
// =============================================================================

fn demo_rest_crud() {
    println!("\n  ═══ demo_rest_crud ═══\n");

    let client = reqwest::blocking::Client::new();
    let base = "http://127.0.0.1:9200/api/v1";

    // GET all users
    let resp = client.get(format!("{}/users", base)).send().unwrap();
    let body: Value = resp.json().unwrap();
    println!("  GET /users → 200 ({} users)", body["count"]);

    // GET single user
    let resp = client.get(format!("{}/users/u1", base)).send().unwrap();
    println!("  GET /users/u1 → {} ({})", resp.status(), resp.json::<Value>().unwrap()["name"]);

    // GET non-existent → 404
    let resp = client.get(format!("{}/users/u999", base)).send().unwrap();
    println!("  GET /users/u999 → {}", resp.status());

    // POST create user → 201
    let resp = client.post(format!("{}/users", base))
        .json(&json!({"name": "Zara", "email": "zara@example.com"}))
        .send().unwrap();
    let status = resp.status();
    let body: Value = resp.json().unwrap();
    println!("  POST /users → {} (id={})", status, body["id"]);

    // PUT update → 200
    let resp = client.put(format!("{}/users/u1", base))
        .json(&json!({"name": "Alice Updated", "email": "alice2@example.com"}))
        .send().unwrap();
    println!("  PUT /users/u1 → {} (full replace)", resp.status());

    // DELETE → 204
    let resp = client.delete(format!("{}/users/u2", base)).send().unwrap();
    println!("  DELETE /users/u2 → {} (no content)", resp.status());

    // DELETE again → 404
    let resp = client.delete(format!("{}/users/u2", base)).send().unwrap();
    println!("  DELETE /users/u2 → {} (already gone)", resp.status());

    println!();
}

fn demo_pagination() {
    println!("\n  ═══ demo_pagination ═══\n");
    println!("  Cursor-based pagination (3 per page):\n");

    let client = reqwest::blocking::Client::new();
    let base = "http://127.0.0.1:9200/api/v1";
    let mut cursor: Option<String> = None;
    let mut page = 0;

    loop {
        page += 1;
        let mut url = format!("{}/users-paginated?limit=3", base);
        if let Some(c) = &cursor {
            url.push_str(&format!("&cursor={}", c));
        }

        let resp: Value = client.get(&url).send().unwrap().json().unwrap();
        let users: Vec<&str> = resp["data"].as_array().unwrap()
            .iter().map(|u| u["name"].as_str().unwrap()).collect();
        let has_more = resp["has_more"].as_bool().unwrap_or(false);

        println!("  Page {}: {:?} (cursor={:?}, has_more={})",
            page, users, resp["next_cursor"], has_more);

        if !has_more { break; }
        cursor = resp["next_cursor"].as_str().map(|s| s.to_string());
    }

    println!("\n  Cursor-based: no duplicates/skips when data changes.");
    println!("  Each page uses the last item's ID as the cursor.\n");
}

fn demo_idempotency() {
    println!("\n  ═══ demo_idempotency ═══\n");
    println!("  Same Idempotency-Key → same result (safe to retry):\n");

    let client = reqwest::blocking::Client::new();
    let base = "http://127.0.0.1:9200/api/v1";
    let key = format!("idem_{}", Uuid::new_v4());

    // First request → creates payment
    let resp: Value = client.post(format!("{}/payments", base))
        .header("Idempotency-Key", &key)
        .json(&json!({"amount": 99.99, "to": "merchant-123"}))
        .send().unwrap().json().unwrap();
    println!("  1st request: id={} (created)", resp["payment"]["id"]);

    // Retry with same key → returns cached result
    let resp2: Value = client.post(format!("{}/payments", base))
        .header("Idempotency-Key", &key)
        .json(&json!({"amount": 99.99, "to": "merchant-123"}))
        .send().unwrap().json().unwrap();
    println!("  2nd request: id={} idempotent={} (cached!)",
        resp2["payment"]["id"], resp2["_idempotent"]);

    // Different key → new payment
    let resp3: Value = client.post(format!("{}/payments", base))
        .header("Idempotency-Key", format!("idem_{}", Uuid::new_v4()))
        .json(&json!({"amount": 99.99, "to": "merchant-123"}))
        .send().unwrap().json().unwrap();
    println!("  3rd request: id={} (new key → new payment)", resp3["payment"]["id"]);

    println!("\n  Use case: payment retries, order creation, any non-idempotent POST.");
    println!("  Server caches (key → response) so retries return same result.\n");
}

fn demo_versioning() {
    println!("\n  ═══ demo_versioning ═══\n");

    let client = reqwest::blocking::Client::new();

    // v1: full user objects
    let v1: Value = client.get("http://127.0.0.1:9200/api/v1/users")
        .send().unwrap().json().unwrap();
    let first = &v1["data"][0];
    println!("  GET /api/v1/users → fields: id, name, email, created_at");
    println!("    Example: {}", first);

    // v2: slimmed down format
    let v2: Value = client.get("http://127.0.0.1:9200/api/v2/users")
        .send().unwrap().json().unwrap();
    let first = &v2["users"][0];
    println!("\n  GET /api/v2/users → fields: id, display_name (combined)");
    println!("    Example: {}", first);

    println!("\n  v1 and v2 coexist. Old clients keep using v1.");
    println!("  New clients migrate to v2 at their own pace.\n");
}

fn demo_batch() {
    println!("\n  ═══ demo_batch ═══\n");

    let client = reqwest::blocking::Client::new();

    // Without batch: N requests
    println!("  Without batch: 3 separate requests");
    let start = Instant::now();
    for id in ["u1", "u3", "u5"] {
        client.get(format!("http://127.0.0.1:9200/api/v1/users/{}", id))
            .send().unwrap();
    }
    println!("    3 requests: {:?}", start.elapsed());

    // With batch: 1 request
    println!("\n  With batch: 1 request for all 3");
    let start = Instant::now();
    let resp: Value = client.post("http://127.0.0.1:9200/api/v1/users/batch")
        .json(&json!({"ids": ["u1", "u3", "u5", "u999"]}))
        .send().unwrap().json().unwrap();
    println!("    1 request: {:?}", start.elapsed());

    for r in resp["results"].as_array().unwrap() {
        println!("    {} → found={}", r["id"], r["found"]);
    }
    println!("\n  Batch reduces round trips. Essential for mobile/high-latency.\n");
}

fn demo_async_job() {
    println!("\n  ═══ demo_async_job ═══\n");
    println!("  Long-running operation (report generation):\n");

    let client = reqwest::blocking::Client::new();
    let base = "http://127.0.0.1:9200/api/v1";

    // Start job → 202 Accepted
    let resp: Value = client.post(format!("{}/reports", base))
        .send().unwrap().json().unwrap();
    let job_id = resp["job_id"].as_str().unwrap();
    println!("  POST /reports → 202 Accepted (job_id={})", job_id);
    println!("  Client polls {}", resp["poll_url"]);

    // Poll → pending
    let resp: Value = client.get(format!("{}/reports/{}", base, job_id))
        .send().unwrap().json().unwrap();
    println!("  Poll 1: status={}", resp["status"]);

    // Wait and poll again → completed
    thread::sleep(Duration::from_millis(600));
    let resp: Value = client.get(format!("{}/reports/{}", base, job_id))
        .send().unwrap().json().unwrap();
    println!("  Poll 2: status={} result={}", resp["status"], resp["result"]);

    println!("\n  Pattern: POST → 202 + job_id → poll GET until done.");
    println!("  Used for: report generation, video transcoding, data export.\n");
}

fn demo_field_selection() {
    println!("\n  ═══ demo_field_selection ═══\n");
    println!("  GraphQL-style field selection (reduce payload):\n");

    let client = reqwest::blocking::Client::new();
    let base = "http://127.0.0.1:9200/api/v1";

    // All fields
    let resp: Value = client.get(format!("{}/users-fields", base))
        .send().unwrap().json().unwrap();
    println!("  GET /users-fields → all fields:");
    println!("    {}", resp["data"][0]);

    // Only id,name
    let resp: Value = client.get(format!("{}/users-fields?fields=id,name", base))
        .send().unwrap().json().unwrap();
    println!("\n  GET /users-fields?fields=id,name → selected fields:");
    println!("    {}", resp["data"][0]);

    // Only email
    let resp: Value = client.get(format!("{}/users-fields?fields=email", base))
        .send().unwrap().json().unwrap();
    println!("\n  GET /users-fields?fields=email → minimal:");
    println!("    {}", resp["data"][0]);

    println!("\n  Reduces bandwidth for mobile. GraphQL does this natively.");
    println!("  REST equivalent: ?fields=id,name or ?include=orders,payments\n");
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║       API Design Patterns — Full Demo            ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    println!("━━━ Starting API Server ━━━");
    start_api_server();

    println!("━━━ 1. REST CRUD — Proper Status Codes ━━━");
    demo_rest_crud();

    println!("━━━ 2. Cursor-Based Pagination ━━━");
    demo_pagination();

    println!("━━━ 3. Idempotency Keys — Safe Retries ━━━");
    demo_idempotency();

    println!("━━━ 4. API Versioning (v1 vs v2) ━━━");
    demo_versioning();

    println!("━━━ 5. Batch API — Reduce Round Trips ━━━");
    demo_batch();

    println!("━━━ 6. Async Jobs — Long-Running Operations ━━━");
    demo_async_job();

    println!("━━━ 7. Field Selection — GraphQL-Style ━━━");
    demo_field_selection();

    println!("━━━ API Design Summary ━━━");
    println!("
┌──────────────────────┬──────────────────────────────────────────┐
│ Pattern              │ When to use                              │
├──────────────────────┼──────────────────────────────────────────┤
│ REST CRUD            │ Standard resource operations             │
│ Cursor pagination    │ Large datasets, infinite scroll          │
│ Idempotency keys     │ Payments, order creation (safe retries)  │
│ URL versioning       │ Breaking changes in public APIs          │
│ Batch API            │ Mobile apps, high-latency clients        │
│ Async jobs (202)     │ Reports, transcoding, data export        │
│ Field selection      │ Mobile bandwidth optimization            │
│ GraphQL              │ Complex queries, multiple resource types  │
│ gRPC                 │ Internal microservices, streaming         │
│ Webhooks             │ Event notifications to external systems   │
└──────────────────────┴──────────────────────────────────────────┘
");

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
