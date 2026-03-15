//! # DevP2P Demo
//!
//! Demonstration of Ethereum P2P networking concepts

use devp2p::{
    NodeId, NodeRecord, Endpoint, Discovery,
    RlpxConnection, Capability,
    EthMessage, Message,
};
use devp2p::wire::{StatusMessage, GetBlockHeadersRequest, BlockId};
use eth_primitives::{H256, U256};
use std::net::{IpAddr, Ipv4Addr};

fn main() {
    println!("🌐 DevP2P - Ethereum P2P Networking Demo\n");

    // =========================================
    // Test 1: Node Identity
    // =========================================
    println!("=== Test 1: Node Identity ===");

    let (node_id, _private_key) = NodeId::random();

    println!("Node ID: {}...", &node_id.to_hex()[..32]);
    println!("Address: 0x{}", node_id.address().to_hex());
    println!();

    // =========================================
    // Test 2: enode URLs
    // =========================================
    println!("=== Test 2: enode URLs ===");

    let endpoint = Endpoint::new(
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 50)),
        30303,
        30303
    );
    let record = NodeRecord::new(node_id.clone(), endpoint);

    let enode_url = record.to_enode_url();
    println!("enode URL: {}...@{}", &enode_url[..50], "203.0.113.50:30303");

    // Parse it back
    let parsed = NodeRecord::from_enode_url(&enode_url).unwrap();
    println!("Parsed IP: {}", parsed.endpoint.ip);
    println!("Parsed port: {}", parsed.endpoint.tcp_port);
    println!();

    // =========================================
    // Test 3: Node Distance (Kademlia)
    // =========================================
    println!("=== Test 3: Kademlia Distance ===");

    let (node_a, _) = NodeId::random();
    let (node_b, _) = NodeId::random();
    let (node_c, _) = NodeId::random();

    println!("Node A: {}...", &node_a.to_hex()[..16]);
    println!("Node B: {}...", &node_b.to_hex()[..16]);
    println!("Node C: {}...", &node_c.to_hex()[..16]);
    println!();

    let dist_ab = node_a.log_distance(&node_b);
    let dist_ac = node_a.log_distance(&node_c);

    println!("Log distance A->B: {} bits", dist_ab);
    println!("Log distance A->C: {} bits", dist_ac);

    if dist_ab < dist_ac {
        println!("Node B is closer to A");
    } else {
        println!("Node C is closer to A");
    }
    println!();

    // =========================================
    // Test 4: Discovery Protocol
    // =========================================
    println!("=== Test 4: Discovery Protocol ===");

    let (local_id, _) = NodeId::random();
    let mut discovery = Discovery::new(local_id.clone());

    // Add bootstrap nodes
    println!("Adding bootstrap nodes...");
    for i in 0..5 {
        let (id, _) = NodeId::random();
        let endpoint = Endpoint::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, i as u8 + 1)),
            30303,
            30303
        );
        discovery.add_bootstrap(NodeRecord::new(id, endpoint));
    }

    println!("Routing table nodes: {}", discovery.routing_table.node_count());

    // Find nodes close to a target
    let (target, _) = NodeId::random();
    let closest = discovery.find_node(&target);
    println!("Found {} nodes close to target", closest.len());
    println!();

    // =========================================
    // Test 5: RLPx Connection
    // =========================================
    println!("=== Test 5: RLPx Connection ===");

    let (local_id, _) = NodeId::random();
    let mut conn = RlpxConnection::new(local_id);

    println!("Local capabilities:");
    let hello = conn.create_hello();
    for cap in &hello.capabilities {
        println!("  - {}/{}", cap.name, cap.version);
    }

    // Simulate remote hello
    let (remote_id, _) = NodeId::random();
    let remote_hello = devp2p::rlpx::Hello::new(
        "Geth/v1.13.0",
        vec![
            Capability::eth(68),
            Capability::new("les", 4),
        ],
        30303,
        remote_id,
    );

    let common = conn.handle_hello(remote_hello).unwrap();
    println!("\nNegotiated capabilities:");
    for cap in &common {
        println!("  - {}/{}", cap.name, cap.version);
    }
    println!("Connection established: {}", conn.connected);
    println!();

    // =========================================
    // Test 6: Wire Protocol Messages
    // =========================================
    println!("=== Test 6: Wire Protocol (eth/68) ===");

    // Status message
    let best_hash = H256::new([0xab; 32]);
    let total_difficulty = U256::from_u64(58_750_000_000_000_000);

    let status = StatusMessage::mainnet(best_hash, total_difficulty);
    println!("Status message:");
    println!("  Protocol version: {}", status.version);
    println!("  Network ID: {} (mainnet)", status.network_id);
    println!("  Best hash: 0x{}...", &hex::encode(best_hash.as_bytes())[..16]);
    println!();

    // GetBlockHeaders request
    let request = GetBlockHeadersRequest {
        request_id: 42,
        start: BlockId::Number(18_500_000),
        limit: 64,
        skip: 0,
        reverse: false,
    };

    println!("GetBlockHeaders request:");
    println!("  Request ID: {}", request.request_id);
    println!("  Start block: 18,500,000");
    println!("  Limit: {} headers", request.limit);
    println!();

    // =========================================
    // Test 7: Message IDs
    // =========================================
    println!("=== Test 7: Protocol Message IDs ===");

    let messages: Vec<(&str, EthMessage)> = vec![
        ("Status", EthMessage::Status(status.clone())),
        ("NewBlockHashes", EthMessage::NewBlockHashes(vec![])),
        ("GetBlockHeaders", EthMessage::GetBlockHeaders(request)),
        ("BlockHeaders", EthMessage::BlockHeaders(vec![])),
        ("GetBlockBodies", EthMessage::GetBlockBodies(vec![])),
        ("NewBlock", EthMessage::NewBlock(Box::new(devp2p::wire::NewBlockMessage {
            header: create_dummy_header(),
            body: devp2p::wire::BlockBody {
                transactions: vec![],
                uncles: vec![],
                withdrawals: None,
            },
            total_difficulty: U256::from_u64(0),
        }))),
    ];

    println!("eth/68 message IDs (offset from capability start):");
    for (name, msg) in messages {
        println!("  0x{:02x} - {}", msg.message_id(), name);
    }
    println!();

    // =========================================
    // Test 8: Full Message Wrapping
    // =========================================
    println!("=== Test 8: Full Message Flow ===");

    let eth_offset = 0x10; // After base protocol messages

    let msg = Message::Ping;
    println!("Ping message code: 0x{:02x}", msg.code(eth_offset));

    let msg = Message::Eth(EthMessage::Status(status));
    println!("eth/Status message code: 0x{:02x} (eth offset=0x10)", msg.code(eth_offset));

    let msg = Message::Eth(EthMessage::GetBlockHeaders(GetBlockHeadersRequest {
        request_id: 1,
        start: BlockId::Number(0),
        limit: 1,
        skip: 0,
        reverse: false,
    }));
    println!("eth/GetBlockHeaders code: 0x{:02x}", msg.code(eth_offset));
    println!();

    println!("✅ DevP2P demo completed!");
}

fn create_dummy_header() -> devp2p::wire::BlockHeader {
    use eth_primitives::Address;

    devp2p::wire::BlockHeader {
        parent_hash: H256::default(),
        uncle_hash: H256::default(),
        coinbase: Address::zero(),
        state_root: H256::default(),
        tx_root: H256::default(),
        receipt_root: H256::default(),
        logs_bloom: [0u8; 256],
        difficulty: U256::from_u64(0),
        number: 0,
        gas_limit: 30_000_000,
        gas_used: 0,
        timestamp: 0,
        extra_data: vec![],
        mix_hash: H256::default(),
        nonce: [0u8; 8],
        base_fee: Some(1_000_000_000),
        withdrawals_root: None,
        blob_gas_used: None,
        excess_blob_gas: None,
    }
}
