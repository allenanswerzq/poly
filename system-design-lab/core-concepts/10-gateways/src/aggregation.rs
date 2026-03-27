use serde_json::{json, Value};
use std::time::Instant;

pub fn demo_request_aggregation() {
    println!("\n  ═══ demo_request_aggregation ═══\n");
    println!("  Gateway fans out to multiple services, combines responses:\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();
        let user_id = 1;
        let order_id = 42;

        // Without gateway: 2 sequential requests from client
        println!("    WITHOUT gateway (client makes 2 requests):");
        let start = Instant::now();
        let user = client
            .get(format!("http://127.0.0.1:9101/users/{}", user_id))
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        let order = client
            .get(format!("http://127.0.0.1:9102/orders/{}", order_id))
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        println!("      Request 1: user → {:?}", user.get("name"));
        println!("      Request 2: order → {:?}", order.get("total"));
        println!("      Total: {:?} (sequential)\n", start.elapsed());

        // With gateway aggregation: 1 request, gateway fans out in parallel
        println!("    WITH gateway aggregation (1 request, parallel fan-out):");
        let start = Instant::now();
        let (user, order) = tokio::join!(
            async {
                client
                    .get(format!("http://127.0.0.1:9101/users/{}", user_id))
                    .send()
                    .await
                    .unwrap()
                    .json::<Value>()
                    .await
                    .unwrap()
            },
            async {
                client
                    .get(format!("http://127.0.0.1:9102/orders/{}", order_id))
                    .send()
                    .await
                    .unwrap()
                    .json::<Value>()
                    .await
                    .unwrap()
            },
        );
        let _combined = json!({
            "user": user,
            "order": order,
        });
        println!(
            "      Combined response: user={}, order={}",
            user.get("name").unwrap(),
            order.get("total").unwrap()
        );
        println!("      Total: {:?} (parallel fan-out)\n", start.elapsed());

        println!("    On mobile (100ms RTT): 2 requests = 200ms, 1 aggregated = 100ms");
        println!("    This is the BFF (Backend for Frontend) pattern.\n");
    });
}
