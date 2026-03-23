//! TCP Echo Server/Client — raw TCP connection demo.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

pub fn run_echo_server(addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("[Echo Server] Listening on {}", addr);

    for stream in listener.incoming().take(3) {
        match stream {
            Ok(stream) => {
                thread::spawn(move || handle_echo_client(stream));
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

pub fn run_echo_client(addr: &str, messages: &[&str]) -> std::io::Result<()> {
    let mut stream = TcpStream::connect(addr)?;
    let mut reader = BufReader::new(stream.try_clone()?);

    for msg in messages {
        stream.write_all(format!("{}\n", msg).as_bytes())?;
        stream.flush()?;

        let mut response = String::new();
        reader.read_line(&mut response)?;
        println!("[Echo Client] Sent '{}', Got '{}'", msg, response.trim());
    }
    Ok(())
}
