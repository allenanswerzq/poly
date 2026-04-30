//! Mini-Eth CLI Client
//!
//! Command-line client for interacting with a mini-eth node

use std::io::{self, Write};
use serde_json::json;
use eth_primitives::{Address, U256, H256};

/// Client for connecting to a mini-eth node
struct EthClient {
    url: String,
    next_id: u64,
}

impl EthClient {
    fn new(url: &str) -> Self {
        EthClient {
            url: url.to_string(),
            next_id: 1,
        }
    }

    /// Make an RPC call (simulated for now - would use HTTP in real impl)
    async fn call(&mut self, method: &str, params: Vec<serde_json::Value>) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let request = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id
        });

        println!("→ {} {}", method, serde_json::to_string(&params).unwrap_or_default());

        // In a real implementation, this would make an HTTP POST request
        // For now, just show what would be sent
        Err(format!("Would send to {}: {}", self.url, request))
    }

    async fn eth_chain_id(&mut self) -> Result<String, String> {
        self.call("eth_chainId", vec![]).await.map(|v| v.to_string())
    }

    async fn eth_block_number(&mut self) -> Result<String, String> {
        self.call("eth_blockNumber", vec![]).await.map(|v| v.to_string())
    }

    async fn eth_get_balance(&mut self, address: &str) -> Result<String, String> {
        self.call("eth_getBalance", vec![json!(address), json!("latest")]).await.map(|v| v.to_string())
    }

    async fn eth_get_transaction_count(&mut self, address: &str) -> Result<String, String> {
        self.call("eth_getTransactionCount", vec![json!(address), json!("latest")]).await.map(|v| v.to_string())
    }

    async fn eth_send_transaction(&mut self, from: &str, to: &str, value: &str, data: &str) -> Result<String, String> {
        let tx = json!({
            "from": from,
            "to": to,
            "value": value,
            "data": data
        });
        self.call("eth_sendTransaction", vec![tx]).await.map(|v| v.to_string())
    }

    async fn eth_call(&mut self, to: &str, data: &str) -> Result<String, String> {
        let tx = json!({
            "to": to,
            "data": data
        });
        self.call("eth_call", vec![tx, json!("latest")]).await.map(|v| v.to_string())
    }

    async fn eth_get_block_by_number(&mut self, number: &str) -> Result<String, String> {
        self.call("eth_getBlockByNumber", vec![json!(number), json!(false)]).await.map(|v| v.to_string())
    }

    async fn net_peer_count(&mut self) -> Result<String, String> {
        self.call("net_peerCount", vec![]).await.map(|v| v.to_string())
    }
}

fn print_help() {
    println!(r#"
Mini-Eth Client Commands:

  chainid                          Get chain ID
  block                            Get latest block number
  block <number>                   Get block by number
  balance <address>                Get account balance
  nonce <address>                  Get account nonce
  send <from> <to> <value>         Send ETH
  call <to> <data>                 Call contract (read-only)
  deploy <from> <bytecode>         Deploy contract
  peers                            Get peer count
  help                             Show this help
  exit                             Exit client

Examples:
  balance 0x0000000000000000000000000000000000000001
  send 0x...from 0x...to 0x1000000000000000
  deploy 0x...from 0x6080604052...
"#);
}

fn print_banner() {
    println!(r#"
    ╔═══════════════════════════════════════════════════════════╗
    ║                                                           ║
    ║           🔷 Mini-ETH Client                              ║
    ║                                                           ║
    ║  Interactive command-line client for mini-eth nodes       ║
    ║  Type 'help' for available commands                       ║
    ║                                                           ║
    ╚═══════════════════════════════════════════════════════════╝
    "#);
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let url = if args.len() > 1 {
        args[1].clone()
    } else {
        "http://127.0.0.1:8545".to_string()
    };

    print_banner();
    println!("Connected to: {}", url);
    println!();

    let mut client = EthClient::new(&url);

    loop {
        print!("eth> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let command = parts.get(0).map(|s| *s).unwrap_or("");

        match command {
            "exit" | "quit" | "q" => {
                println!("Goodbye!");
                break;
            }

            "help" | "h" | "?" => {
                print_help();
            }

            "chainid" => {
                match client.eth_chain_id().await {
                    Ok(result) => println!("Chain ID: {}", result),
                    Err(e) => println!("Error: {}", e),
                }
            }

            "block" => {
                if parts.len() > 1 {
                    let number = parts[1];
                    match client.eth_get_block_by_number(number).await {
                        Ok(result) => println!("Block: {}", result),
                        Err(e) => println!("Error: {}", e),
                    }
                } else {
                    match client.eth_block_number().await {
                        Ok(result) => println!("Latest block: {}", result),
                        Err(e) => println!("Error: {}", e),
                    }
                }
            }

            "balance" => {
                if parts.len() < 2 {
                    println!("Usage: balance <address>");
                    continue;
                }
                match client.eth_get_balance(parts[1]).await {
                    Ok(result) => println!("Balance: {} wei", result),
                    Err(e) => println!("Error: {}", e),
                }
            }

            "nonce" => {
                if parts.len() < 2 {
                    println!("Usage: nonce <address>");
                    continue;
                }
                match client.eth_get_transaction_count(parts[1]).await {
                    Ok(result) => println!("Nonce: {}", result),
                    Err(e) => println!("Error: {}", e),
                }
            }

            "send" => {
                if parts.len() < 4 {
                    println!("Usage: send <from> <to> <value>");
                    continue;
                }
                let from = parts[1];
                let to = parts[2];
                let value = parts[3];

                match client.eth_send_transaction(from, to, value, "0x").await {
                    Ok(result) => println!("Transaction hash: {}", result),
                    Err(e) => println!("Error: {}", e),
                }
            }

            "call" => {
                if parts.len() < 3 {
                    println!("Usage: call <to> <data>");
                    continue;
                }
                let to = parts[1];
                let data = parts[2];

                match client.eth_call(to, data).await {
                    Ok(result) => println!("Result: {}", result),
                    Err(e) => println!("Error: {}", e),
                }
            }

            "deploy" => {
                if parts.len() < 3 {
                    println!("Usage: deploy <from> <bytecode>");
                    continue;
                }
                let from = parts[1];
                let bytecode = parts[2];

                // Deploy is a send with no 'to' address
                let tx = json!({
                    "from": from,
                    "data": bytecode
                });

                match client.call("eth_sendTransaction", vec![tx]).await {
                    Ok(result) => println!("Contract deployment tx: {}", result),
                    Err(e) => println!("Error: {}", e),
                }
            }

            "peers" => {
                match client.net_peer_count().await {
                    Ok(result) => println!("Peer count: {}", result),
                    Err(e) => println!("Error: {}", e),
                }
            }

            _ => {
                println!("Unknown command: '{}'. Type 'help' for available commands.", command);
            }
        }
    }
}
