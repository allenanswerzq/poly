use redis::{Client, Connection};
use std::process::{Child, Command};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

// =============================================================================
// Redis Server Manager
//
// Automatically starts/stops a Redis server for demos.
// The binary path comes from build.rs (compiled from source at build time).
//
// Usage:
//   let server = RedisServer::start();    ← starts redis-server on a random port
//   let mut conn = server.connect();      ← get a redis-rs connection
//   conn.set("key", "val")?;             ← use normally
//   drop(server);                         ← kills redis-server on drop
// =============================================================================

static REDIS_PORT_COUNTER: Mutex<u16> = Mutex::new(16379); // start from 16379 to avoid conflicts

pub struct RedisServer {
    process: Option<Child>,
    pub port: u16,
}

impl RedisServer {
    /// Start a Redis server. Uses the binary compiled by build.rs.
    #[allow(clippy::zombie_processes)] // process is moved into struct with Drop that kills it
    pub fn start() -> Self {
        let redis_bin = env!("REDIS_SERVER_PATH");

        // Pick a unique port (each demo gets its own Redis)
        let port = {
            let mut counter = REDIS_PORT_COUNTER.lock().unwrap();
            let p = *counter;
            *counter += 1;
            p
        };

        let mut process = Command::new(redis_bin)
            .args([
                "--port",
                &port.to_string(),
                "--daemonize",
                "no", // run in foreground (we manage lifecycle)
                "--loglevel",
                "warning", // quiet
                "--save",
                "", // no disk persistence
                "--appendonly",
                "no", // no AOF
                "--maxmemory",
                "50mb", // limit memory for demo
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap_or_else(|_| panic!("failed to start redis-server at {}", redis_bin));

        // Wait for Redis to be ready
        let url = format!("redis://127.0.0.1:{}/", port);
        for _attempt in 0..50 {
            thread::sleep(Duration::from_millis(50));
            if let Ok(client) = Client::open(url.as_str()) {
                if let Ok(mut conn) = client.get_connection() {
                    let pong: Result<String, _> = redis::cmd("PING").query(&mut conn);
                    if pong.is_ok() {
                        return Self {
                            process: Some(process),
                            port,
                        };
                    }
                }
            }
        }
        let _ = process.kill();
        panic!("Redis failed to start on port {} within 2.5 seconds", port);
    }

    /// Get a redis-rs connection to this server
    pub fn connect(&self) -> Connection {
        let url = format!("redis://127.0.0.1:{}/", self.port);
        let client = Client::open(url.as_str()).unwrap();
        client.get_connection().unwrap()
    }

    /// Get the connection URL
    pub fn url(&self) -> String {
        format!("redis://127.0.0.1:{}/", self.port)
    }
}

impl Drop for RedisServer {
    fn drop(&mut self) {
        // Shut down Redis cleanly
        if let Ok(mut conn) = Client::open(self.url().as_str()).and_then(|c| c.get_connection()) {
            let _: Result<(), _> = redis::cmd("SHUTDOWN").arg("NOSAVE").query(&mut conn);
        }
        // Kill process if SHUTDOWN didn't work
        if let Some(ref mut child) = self.process {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
