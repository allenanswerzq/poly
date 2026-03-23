//! Simple connection pool concept.

use std::net::TcpStream;

pub struct ConnectionPool {
    connections: Vec<TcpStream>,
    max_size: usize,
}

impl ConnectionPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            connections: Vec::new(),
            max_size,
        }
    }

    pub fn get(&mut self, addr: &str) -> std::io::Result<TcpStream> {
        if let Some(conn) = self.connections.pop() {
            println!("[Pool] Reusing connection");
            return Ok(conn);
        }
        println!("[Pool] Creating new connection");
        TcpStream::connect(addr)
    }

    pub fn put(&mut self, conn: TcpStream) {
        if self.connections.len() < self.max_size {
            println!("[Pool] Returning connection to pool");
            self.connections.push(conn);
        } else {
            println!("[Pool] Pool full, closing connection");
            drop(conn);
        }
    }
}
