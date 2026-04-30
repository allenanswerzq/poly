//! Multi-node Demo
//!
//! This script demonstrates running multiple mini-eth nodes that communicate
//! with each other, submit transactions, and deploy smart contracts.

use std::path::PathBuf;
use std::time::Duration;
use mini_eth::{
    Node, NodeConfig, GenesisConfig, GenesisBuilder,
    TxPoolConfig,
};
use mini_eth::config::{NetworkConfig, RpcConfig, MiningConfig};
use eth_primitives::{Address, U256};

/// Demo accounts
mod accounts {
    use eth_primitives::Address;

    pub fn alice() -> Address {
        let mut bytes = [0u8; 20];
        bytes[19] = 1;
        Address::from(bytes)
    }

    pub fn bob() -> Address {
        let mut bytes = [0u8; 20];
        bytes[19] = 2;
        Address::from(bytes)
    }

    pub fn charlie() -> Address {
        let mut bytes = [0u8; 20];
        bytes[19] = 3;
        Address::from(bytes)
    }

    pub fn validator() -> Address {
        let mut bytes = [0u8; 20];
        bytes[18] = 1;
        Address::from(bytes)
    }
}

/// Create a genesis configuration with funded accounts
fn create_genesis() -> GenesisConfig {
    // 1 million ETH in wei (1e24 = 1 million * 1e18)
    let eth_in_wei = U256::from(1_000_000_000_000_000_000u64); // 1 ETH
    let one_million_eth = eth_in_wei * U256::from(1_000_000u64);

    GenesisBuilder::new()
        .chain_id(1337)
        .timestamp(1704067200)
        .gas_limit(30_000_000)
        .base_fee(1_000_000_000)
        // Fund test accounts
        .account(accounts::alice(), one_million_eth)
        .account(accounts::bob(), one_million_eth)
        .account(accounts::charlie(), one_million_eth)
        .account(accounts::validator(), one_million_eth)
        // Add validator
        .validator(accounts::validator())
        .build()
}

/// Create node configuration
fn create_node_config(
    name: &str,
    p2p_port: u16,
    rpc_port: u16,
    mining: bool,
    bootnodes: Vec<String>,
    genesis: GenesisConfig,
) -> NodeConfig {
    NodeConfig {
        name: name.to_string(),
        chain_id: 1337,
        data_dir: PathBuf::from(format!("./data/{}", name)),
        network: NetworkConfig {
            enabled: true,
            listen_addr: "127.0.0.1".to_string(),
            port: p2p_port,
            bootnodes,
            max_peers: 25,
            discovery: true,
        },
        rpc: RpcConfig {
            http_enabled: true,
            http_addr: "127.0.0.1".to_string(),
            http_port: rpc_port,
            apis: vec!["eth".to_string(), "net".to_string(), "web3".to_string()],
            cors_domains: vec!["*".to_string()],
            ws_enabled: false,
            ws_port: rpc_port + 100,
        },
        mining: MiningConfig {
            enabled: mining,
            coinbase: accounts::validator(),
            block_interval: 2,
            gas_limit: 30_000_000,
            extra_data: b"mini-eth".to_vec(),
        },
        txpool: TxPoolConfig::default(),
        genesis: Some(genesis),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    mini_eth::init_logging();

    println!(r#"
    ╔═══════════════════════════════════════════════════════════════════╗
    ║                                                                   ║
    ║              🔷 Mini-ETH Multi-Node Demo                          ║
    ║                                                                   ║
    ║  This demo runs 3 connected nodes with:                           ║
    ║    • P2P networking between nodes                                 ║
    ║    • Transaction submission and propagation                       ║
    ║    • Block production and consensus                               ║
    ║                                                                   ║
    ╚═══════════════════════════════════════════════════════════════════╝
    "#);

    // Create shared genesis
    let genesis = create_genesis();

    println!("📦 Creating network with 3 nodes...\n");

    // Node 1 - Mining node (primary)
    let config1 = create_node_config(
        "node-1-miner",
        30303,
        8545,
        true,
        vec![],
        genesis.clone(),
    );

    // Node 2 - Full node
    let config2 = create_node_config(
        "node-2-full",
        30304,
        8546,
        false,
        vec!["127.0.0.1:30303".to_string()],
        genesis.clone(),
    );

    // Node 3 - Full node
    let config3 = create_node_config(
        "node-3-full",
        30305,
        8547,
        false,
        vec!["127.0.0.1:30303".to_string()],
        genesis.clone(),
    );

    // Create nodes
    let mut node1 = Node::new(config1);
    let mut node2 = Node::new(config2);
    let mut node3 = Node::new(config3);

    // Start nodes
    println!("🚀 Starting Node 1 (Mining node)...");
    node1.start().await?;
    println!("   ✓ Node 1 started on P2P:30303, RPC:8545\n");

    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("🚀 Starting Node 2...");
    node2.start().await?;
    println!("   ✓ Node 2 started on P2P:30304, RPC:8546\n");

    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("🚀 Starting Node 3...");
    node3.start().await?;
    println!("   ✓ Node 3 started on P2P:30305, RPC:8547\n");

    // Wait for peer discovery
    println!("⏳ Waiting for peer discovery...\n");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Show network status
    println!("📊 Network Status:");
    println!("   Node 1: Block #{}, Peers: {}", node1.block_number(), node1.peer_count());
    println!("   Node 2: Block #{}, Peers: {}", node2.block_number(), node2.peer_count());
    println!("   Node 3: Block #{}, Peers: {}", node3.block_number(), node3.peer_count());
    println!();

    // Check balances
    println!("💰 Account Balances:");
    let alice_balance = node1.get_balance(accounts::alice())?;
    let bob_balance = node1.get_balance(accounts::bob())?;
    println!("   Alice: {} wei", alice_balance);
    println!("   Bob:   {} wei", bob_balance);
    println!();

    // Submit a transaction
    println!("📤 Submitting transaction: Alice → Bob (1 ETH)...");
    let one_eth = U256::from(1_000_000_000_000_000_000u64); // 1 ETH in wei
    let tx = mini_eth::SignedTransaction {
        from: accounts::alice(),
        to: Some(accounts::bob()),
        value: one_eth,
        data: vec![],
        nonce: 0,
        gas_limit: 21000,
        max_fee_per_gas: 2_000_000_000,
        max_priority_fee_per_gas: 1_000_000_000,
        hash: eth_primitives::H256::zero(),
        signature: vec![],
    };

    match node1.submit_transaction(tx) {
        Ok(hash) => println!("   ✓ Transaction submitted: {:?}\n", hash),
        Err(e) => println!("   ✗ Failed: {}\n", e),
    }

    // Wait for block production
    println!("⏳ Waiting for block production...\n");
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Check updated balances
    println!("💰 Updated Balances:");
    let alice_balance = node1.get_balance(accounts::alice())?;
    let bob_balance = node1.get_balance(accounts::bob())?;
    println!("   Alice: {} wei", alice_balance);
    println!("   Bob:   {} wei", bob_balance);
    println!();

    // Show block info
    println!("📦 Latest Blocks:");
    println!("   Node 1: Block #{}", node1.block_number());
    println!("   Node 2: Block #{}", node2.block_number());
    println!("   Node 3: Block #{}", node3.block_number());
    println!();

    // Deploy a simple contract
    println!("📝 Deploying simple storage contract...");

    // Simple storage contract bytecode (stores a number)
    // PUSH1 0x42, PUSH1 0, SSTORE, PUSH1 32, PUSH1 0, RETURN
    let bytecode = vec![
        0x60, 0x42,  // PUSH1 0x42 (value to store)
        0x60, 0x00,  // PUSH1 0 (storage slot)
        0x55,        // SSTORE
        0x60, 0x20,  // PUSH1 32
        0x60, 0x00,  // PUSH1 0
        0xf3,        // RETURN
    ];

    let deploy_tx = mini_eth::SignedTransaction {
        from: accounts::charlie(),
        to: None, // Contract creation
        value: U256::zero(),
        data: bytecode,
        nonce: 0,
        gas_limit: 100_000,
        max_fee_per_gas: 2_000_000_000,
        max_priority_fee_per_gas: 1_000_000_000,
        hash: eth_primitives::H256::zero(),
        signature: vec![],
    };

    match node1.submit_transaction(deploy_tx) {
        Ok(hash) => println!("   ✓ Contract deployment submitted: {:?}\n", hash),
        Err(e) => println!("   ✗ Failed: {}\n", e),
    }

    // Wait for contract deployment
    println!("⏳ Waiting for contract deployment...\n");
    tokio::time::sleep(Duration::from_secs(3)).await;

    println!("✅ Demo Complete!");
    println!();
    println!("The mini-eth network demonstrated:");
    println!("  • 3 connected nodes with P2P networking");
    println!("  • Transaction submission and propagation");
    println!("  • Block production with PoA consensus");
    println!("  • Smart contract deployment");
    println!();
    println!("Press Ctrl+C to stop the nodes...");

    // Keep running
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Print periodic status
        println!("📊 [Status] Node 1: Block #{}, Node 2: Block #{}, Node 3: Block #{}",
            node1.block_number(),
            node2.block_number(),
            node3.block_number(),
        );
    }
}
