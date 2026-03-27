//! gRPC demo using tonic — real .proto codegen, real gRPC server + client.
//!
//! The proto file (proto/greeter.proto) defines:
//!   service Greeter {
//!     rpc SayHello (HelloRequest) returns (HelloReply);           // unary
//!     rpc SayHelloStream (HelloRequest) returns (stream HelloReply); // server streaming
//!   }

use std::time::Instant;

// Import the generated code from greeter.proto
pub mod greeter {
    tonic::include_proto!("greeter");
}

use greeter::greeter_client::GreeterClient;
use greeter::greeter_server::{Greeter, GreeterServer};
use greeter::{HelloReply, HelloRequest};

/// gRPC service implementation
#[derive(Default)]
struct MyGreeter;

#[tonic::async_trait]
impl Greeter for MyGreeter {
    /// Unary RPC: one request → one response
    async fn say_hello(
        &self,
        request: tonic::Request<HelloRequest>,
    ) -> Result<tonic::Response<HelloReply>, tonic::Status> {
        let name = request.into_inner().name;
        println!("  [gRPC Server] SayHello(name=\"{}\")", name);
        Ok(tonic::Response::new(HelloReply {
            message: format!("Hello, {}!", name),
            count: 1,
        }))
    }

    /// Server streaming RPC: one request → stream of responses
    type SayHelloStreamStream =
        tokio_stream::wrappers::ReceiverStream<Result<HelloReply, tonic::Status>>;

    async fn say_hello_stream(
        &self,
        request: tonic::Request<HelloRequest>,
    ) -> Result<tonic::Response<Self::SayHelloStreamStream>, tonic::Status> {
        let name = request.into_inner().name;
        println!(
            "  [gRPC Server] SayHelloStream(name=\"{}\") — streaming 5 responses",
            name
        );

        let (tx, rx) = tokio::sync::mpsc::channel(5);
        tokio::spawn(async move {
            for i in 1..=5 {
                let reply = HelloReply {
                    message: format!("Hello #{} to {}!", i, name),
                    count: i,
                };
                tx.send(Ok(reply)).await.ok();
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });

        Ok(tonic::Response::new(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        ))
    }
}

/// Real gRPC demo: start tonic server, make unary + streaming calls.
pub fn demo(_base_url: &str) {
    println!("\n  ═══ demo_grpc ═══\n");
    println!("  Using tonic (production gRPC framework) with real .proto codegen.\n");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // ── Start gRPC server ────────────────────────────────────────────
        let addr = "127.0.0.1:9012".parse().unwrap();
        let server = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(GreeterServer::new(MyGreeter))
                .serve(addr)
                .await
                .unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        println!("  [gRPC Server] tonic listening on {} (HTTP/2)\n", addr);

        // ── Unary RPC ────────────────────────────────────────────────────
        println!("  1) Unary RPC: SayHello(\"Alice\")\n");
        let mut client = GreeterClient::connect("http://127.0.0.1:9012")
            .await
            .unwrap();

        let start = Instant::now();
        let response = client
            .say_hello(tonic::Request::new(HelloRequest {
                name: "Alice".into(),
            }))
            .await
            .unwrap();

        let reply = response.into_inner();
        println!(
            "     Response: message=\"{}\" count={} ({:?})\n",
            reply.message,
            reply.count,
            start.elapsed()
        );

        // ── Concurrent unary RPCs ────────────────────────────────────────
        println!("  2) 5 concurrent unary RPCs (multiplexed on one HTTP/2 connection):\n");

        let names = ["Bob", "Carol", "Dave", "Eve", "Frank"];
        let start = Instant::now();
        let mut handles = vec![];

        for name in &names {
            let mut c = client.clone();
            let name = name.to_string();
            handles.push(tokio::spawn(async move {
                let t = Instant::now();
                let resp = c
                    .say_hello(tonic::Request::new(HelloRequest { name: name.clone() }))
                    .await
                    .unwrap();
                (name, resp.into_inner().message, t.elapsed())
            }));
        }

        let mut results = vec![];
        for h in handles {
            results.push(h.await.unwrap());
        }
        results.sort_by(|a, b| a.2.cmp(&b.2));

        for (name, msg, elapsed) in &results {
            println!("     SayHello(\"{}\") → \"{}\" ({:?})", name, msg, elapsed);
        }
        println!(
            "     Total: {:?} (all 5 on one HTTP/2 connection)\n",
            start.elapsed()
        );

        // ── Server streaming RPC ─────────────────────────────────────────
        println!("  3) Server streaming RPC: SayHelloStream(\"World\")\n");

        let start = Instant::now();
        let mut stream = client
            .say_hello_stream(tonic::Request::new(HelloRequest {
                name: "World".into(),
            }))
            .await
            .unwrap()
            .into_inner();

        let mut stream_count = 0;
        while let Some(Ok(reply)) = futures::StreamExt::next(&mut stream).await {
            stream_count += 1;
            println!(
                "     Stream #{}: message=\"{}\" ({:?})",
                reply.count,
                reply.message,
                start.elapsed()
            );
        }
        println!(
            "     Stream complete: {} messages ({:?})\n",
            stream_count,
            start.elapsed()
        );

        server.abort();
    });

    println!("  WHAT JUST HAPPENED:");
    println!("  • proto/greeter.proto → tonic-build generates Rust types + client/server");
    println!("  • Server: impl Greeter for MyGreeter (just fill in the functions)");
    println!("  • Client: GreeterClient::connect() → client.say_hello()");
    println!("  • All over HTTP/2 with protobuf serialization (binary, compact)");
    println!();
    println!("  STREAMING MODES in our .proto:");
    println!("  • SayHello:       unary (req → resp)");
    println!("  • SayHelloStream: server streaming (req → stream of resp)");
    println!("  • (not shown):    client streaming, bidirectional streaming");
    println!();
}
