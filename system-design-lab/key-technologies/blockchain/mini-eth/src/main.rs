//! Mini-Eth Node Binary
//!
//! Run a mini Ethereum node

use std::path::PathBuf;
use mini_eth::{NodeConfig, Node, GenesisConfig, init_logging};
use eth_primitives::Address;

/// CLI Arguments (simplified)
struct Args {
    /// Node name
    name: String,
    /// Data directory
    data_dir: PathBuf,
    /// Network port
    port: u16,
    /// RPC port
    rpc_port: u16,
    /// Enable mining
    mine: bool,
    /// Coinbase address for mining
    coinbase: Option<String>,
    /// Bootstrap nodes
    bootnodes: Vec<String>,
    /// Dev mode
    dev: bool,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            name: "mini-eth".to_string(),
            data_dir: PathBuf::from("./data"),
            port: 30303,
            rpc_port: 8545,
            mine: false,
            coinbase: None,
            bootnodes: vec![],
            dev: false,
        }
    }
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let cli_args: Vec<String> = std::env::args().collect();

    let mut i = 1;
    while i < cli_args.len() {
        match cli_args[i].as_str() {
            "--name" => {
                if i + 1 < cli_args.len() {
                    args.name = cli_args[i + 1].clone();
                    i += 1;
                }
            }
            "--data-dir" => {
                if i + 1 < cli_args.len() {
                    args.data_dir = PathBuf::from(&cli_args[i + 1]);
                    i += 1;
                }
            }
            "--port" => {
                if i + 1 < cli_args.len() {
                    args.port = cli_args[i + 1].parse().unwrap_or(30303);
                    i += 1;
                }
            }
            "--rpc-port" => {
                if i + 1 < cli_args.len() {
                    args.rpc_port = cli_args[i + 1].parse().unwrap_or(8545);
                    i += 1;
                }
            }
            "--mine" => {
                args.mine = true;
            }
            "--coinbase" => {
                if i + 1 < cli_args.len() {
                    args.coinbase = Some(cli_args[i + 1].clone());
                    i += 1;
                }
            }
            "--bootnode" => {
                if i + 1 < cli_args.len() {
                    args.bootnodes.push(cli_args[i + 1].clone());
                    i += 1;
                }
            }
            "--dev" => {
                args.dev = true;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    args
}

fn print_help() {
    println!(r#"
Mini-Eth Node

USAGE:
    mini-eth [OPTIONS]

OPTIONS:
    --name <NAME>           Node name (default: mini-eth)
    --data-dir <PATH>       Data directory (default: ./data)
    --port <PORT>           P2P port (default: 30303)
    --rpc-port <PORT>       RPC port (default: 8545)
    --mine                  Enable mining/block production
    --coinbase <ADDRESS>    Coinbase address for mining rewards
    --bootnode <ENODE>      Bootstrap node address (can be used multiple times)
    --dev                   Run in development mode
    -h, --help              Print help

EXAMPLES:
    # Run a dev node
    mini-eth --dev

    # Run a mining node
    mini-eth --mine --coinbase 0x0000000000000000000000000000000000000001

    # Run a node connecting to an existing network
    mini-eth --bootnode 127.0.0.1:30303 --port 30304 --rpc-port 8546
"#);
}

fn parse_address(s: &str) -> Option<Address> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).ok()?;
    if bytes.len() != 20 {
        return None;
    }
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Some(Address::from(arr))
}

#[tokio::main]
async fn main() {
    init_logging();

    let args = parse_args();

    // Build configuration
    let config = if args.dev {
        NodeConfig::dev()
    } else {
        let mut config = NodeConfig::new(&args.name)
            .with_data_dir(args.data_dir)
            .with_port(args.port)
            .with_rpc_port(args.rpc_port)
            .with_genesis(GenesisConfig::default());

        // Add bootnodes
        for bootnode in args.bootnodes {
            config = config.with_bootnode(bootnode);
        }

        // Enable mining
        if args.mine {
            let coinbase = args.coinbase
                .and_then(|s| parse_address(&s))
                .unwrap_or_else(Address::zero);
            config = config.with_mining(coinbase);
        }

        config
    };

    println!(r#"
    ╔═══════════════════════════════════════════════════════════╗
    ║                                                           ║
    ║              🔷 Mini-ETH Node v{}                   ║
    ║                                                           ║
    ║  A minimal Ethereum implementation for learning           ║
    ║                                                           ║
    ╚═══════════════════════════════════════════════════════════╝
    "#, mini_eth::VERSION);

    println!("Node Name:    {}", config.name);
    println!("Chain ID:     {}", config.chain_id);
    println!("P2P Port:     {}", config.network.port);
    println!("RPC Port:     {}", config.rpc.http_port);
    println!("Mining:       {}", if config.mining.enabled { "enabled" } else { "disabled" });
    println!();

    // Create and start node
    let mut node = Node::new(config);

    if let Err(e) = node.start().await {
        eprintln!("Failed to start node: {}", e);
        std::process::exit(1);
    }

    println!("✓ Node started successfully!");
    println!("  Current block: #{}", node.block_number());
    println!("  Peers: {}", node.peer_count());
    println!();
    println!("Press Ctrl+C to stop...");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");

    println!("\nShutting down...");
    if let Err(e) = node.stop().await {
        eprintln!("Error during shutdown: {}", e);
    }

    println!("Goodbye!");
}
