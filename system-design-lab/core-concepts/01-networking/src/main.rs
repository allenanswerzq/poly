//! # Networking Essentials Demo
//!
//! Demonstrates core networking concepts:
//! - TCP server/client
//! - Basic HTTP server
//! - Connection handling

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

// =============================================================================
// TCP Echo Server
// =============================================================================

/// A simple TCP echo server that echoes back any message received
fn run_echo_server(addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("[Echo Server] Listening on {}", addr);

    for stream in listener.incoming().take(3) {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    handle_echo_client(stream);
                });
            }
            Err(e) => eprintln!("[Echo Server] Connection error: {}", e),
        }
    }
    Ok(())
}

fn handle_echo_client(mut stream: TcpStream) {
    let peer = stream.peer_addr().unwrap();
    println!("[Echo Server] Client connected: {}", peer);

    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();

    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        println!("[Echo Server] Received: {}", line.trim());
        stream.write_all(format!("ECHO: {}", line).as_bytes()).ok();
        line.clear();
    }

    println!("[Echo Server] Client disconnected: {}", peer);
}

/// TCP client that sends messages
fn run_echo_client(addr: &str, messages: &[&str]) -> std::io::Result<()> {
    let mut stream = TcpStream::connect(addr)?;
    let mut reader = BufReader::new(stream.try_clone()?);

    for msg in messages {
        // Send message
        stream.write_all(format!("{}\n", msg).as_bytes())?;
        stream.flush()?;

        // Read response
        let mut response = String::new();
        reader.read_line(&mut response)?;
        println!("[Echo Client] Sent '{}', Got '{}'", msg, response.trim());
    }

    Ok(())
}

// =============================================================================
// Simple HTTP Server
// =============================================================================

fn run_http_server(addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("[HTTP Server] Listening on {}", addr);

    for stream in listener.incoming().take(5) {
        match stream {
            Ok(stream) => {
                handle_http_request(stream);
            }
            Err(e) => eprintln!("[HTTP Server] Error: {}", e),
        }
    }
    Ok(())
}

fn handle_http_request(mut stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();

    // Read first line of HTTP request
    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let method = parts[0];
    let path = parts[1];

    println!("[HTTP Server] {} {}", method, path);

    // Route handling
    let (status, body) = match (method, path) {
        ("GET", "/") => ("200 OK", r#"{"message": "Hello, World!"}"#),
        ("GET", "/health") => ("200 OK", r#"{"status": "healthy"}"#),
        ("GET", "/api/users") => ("200 OK", r#"[{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]"#),
        ("POST", "/api/users") => ("201 Created", r#"{"id": 3, "message": "User created"}"#),
        _ => ("404 Not Found", r#"{"error": "Not found"}"#),
    };

    // HTTP Response
    let response = format!(
        "HTTP/1.1 {}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        status,
        body.len(),
        body
    );

    stream.write_all(response.as_bytes()).ok();
    stream.flush().ok();
}

/// Simple HTTP client using TCP
fn http_get(addr: &str, path: &str) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(addr)?;

    // Send HTTP request
    let request = format!(
        "GET {} HTTP/1.1\r\n\
         Host: {}\r\n\
         Connection: close\r\n\
         \r\n",
        path, addr
    );
    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    // Read response
    let mut reader = BufReader::new(&stream);
    let mut response = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        response.push_str(&line);
    }

    Ok(response)
}

// =============================================================================
// Connection Pool Demo
// =============================================================================

/// Simple connection pool concept
struct ConnectionPool {
    connections: Vec<TcpStream>,
    max_size: usize,
}

impl ConnectionPool {
    fn new(max_size: usize) -> Self {
        Self {
            connections: Vec::new(),
            max_size,
        }
    }

    fn get(&mut self, addr: &str) -> std::io::Result<TcpStream> {
        // Try to reuse an existing connection
        if let Some(conn) = self.connections.pop() {
            println!("[Pool] Reusing connection");
            return Ok(conn);
        }

        // Create new connection
        println!("[Pool] Creating new connection");
        TcpStream::connect(addr)
    }

    fn put(&mut self, conn: TcpStream) {
        if self.connections.len() < self.max_size {
            println!("[Pool] Returning connection to pool");
            self.connections.push(conn);
        } else {
            println!("[Pool] Pool full, closing connection");
            drop(conn);
        }
    }
}

// =============================================================================
// Demonstration
// =============================================================================

fn main() {
    println!("=== Networking Essentials Demo ===\n");

    // Demo 1: TCP Echo Server
    println!("--- TCP Echo Server/Client ---");
    let echo_addr = "127.0.0.1:9001";

    // Start server in background
    let server_handle = thread::spawn(move || {
        run_echo_server(echo_addr).ok();
    });

    // Give server time to start
    thread::sleep(Duration::from_millis(100));

    // Run client
    run_echo_client(echo_addr, &["Hello", "World", "Goodbye"]).ok();

    println!();

    // Demo 2: HTTP Server
    println!("--- Simple HTTP Server ---");
    let http_addr = "127.0.0.1:9002";

    // Start HTTP server in background
    let http_handle = thread::spawn(move || {
        run_http_server(http_addr).ok();
    });

    thread::sleep(Duration::from_millis(100));

    // Make HTTP requests
    for path in &["/", "/health", "/api/users", "/unknown"] {
        match http_get(http_addr, path) {
            Ok(response) => {
                // Extract just the status line and body
                let lines: Vec<&str> = response.lines().collect();
                if let Some(status) = lines.first() {
                    println!("[HTTP Client] GET {} -> {}", path, status);
                }
            }
            Err(e) => println!("[HTTP Client] Error: {}", e),
        }
    }

    println!();

    // Demo 3: Connection Pool concept
    println!("--- Connection Pool Concept ---");
    println!("
Connection Pool Benefits:
- Avoid TCP handshake overhead (saves ~1-3 RTT)
- Reuse TLS sessions (saves expensive crypto)
- Limit concurrent connections
- Better resource management

Typical configuration:
- Min connections: 5
- Max connections: 20
- Idle timeout: 30 seconds
- Connection timeout: 5 seconds
");

    let mut pool = ConnectionPool::new(2);

    // Simulate getting and returning connections
    if let Ok(conn1) = pool.get("127.0.0.1:9002") {
        pool.put(conn1);  // Return to pool
    }
    if let Ok(conn2) = pool.get("127.0.0.1:9002") {
        // Reuses connection from pool
        println!();
        drop(conn2);  // Just drop, not returned
    }

    // Demo 4: Protocol comparison
    println!("--- Protocol Quick Reference ---");
    println!("
| Protocol | Layer | Use Case | Latency |
|----------|-------|----------|---------|
| TCP | L4 | Reliable data | Higher |
| UDP | L4 | Real-time | Lower |
| HTTP/1.1 | L7 | Web | Medium |
| HTTP/2 | L7 | Modern web | Lower |
| HTTP/3 | L7 | Mobile/lossy | Lowest |
| WebSocket | L7 | Real-time | Very low |
| gRPC | L7 | Microservices | Low |

Connection Establishment:
- TCP: 1.5 RTT (SYN, SYN-ACK, ACK)
- TLS 1.3: +1 RTT (or 0-RTT with resumption)
- QUIC: 0-1 RTT (built-in TLS)
");

    // Demo 5: Latency breakdown
    println!("--- Request Latency Breakdown ---");
    println!("
Typical API request latency:

DNS lookup:        1-50ms (cached vs uncached)
TCP handshake:     1 RTT (~50ms cross-region)
TLS handshake:     1-2 RTT (~100ms)
HTTP request:      Server processing + 1 RTT
Data transfer:     Size / bandwidth

Optimization strategies:
✓ Keep-alive connections
✓ Connection pooling
✓ HTTP/2 multiplexing
✓ CDN for static content
✓ Regional deployment
");

    // Wait for server threads
    thread::sleep(Duration::from_millis(500));
    drop(server_handle);
    drop(http_handle);

    println!("\n=== Demo Complete ===");
}
