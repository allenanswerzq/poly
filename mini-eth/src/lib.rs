//! # Mini-ETH: A Fully Functional Mini Ethereum
//!
//! This crate brings together all the component crates to create a working
//! Ethereum-like blockchain:
//!
//! ## Components Used
//! - `eth_primitives` - Core types (Address, H256, U256, Transaction, Signature)
//! - `mini_evm` - EVM execution engine with smart contract support
//! - `mpt_trie` - Merkle Patricia Trie for state storage
//! - `devp2p` - P2P networking for node communication
//! - `block_builder` - Block construction and MEV
//!
//! ## Architecture
//!
//! ```text
//!                           ┌─────────────────────────────────────────────┐
//!                           │              MINI-ETH NETWORK               │
//!                           └─────────────────────────────────────────────┘
//!                                             │
//!           ┌─────────────────────────────────┼─────────────────────────────────┐
//!           │                                 │                                 │
//!           ▼                                 ▼                                 ▼
//!     ┌──────────┐                      ┌──────────┐                      ┌──────────┐
//!     │  Node 1  │◄────── P2P ─────────►│  Node 2  │◄────── P2P ─────────►│  Node 3  │
//!     └──────────┘                      └──────────┘                      └──────────┘
//!           │                                 │                                 │
//!           ▼                                 ▼                                 ▼
//!     ┌──────────┐                      ┌──────────┐                      ┌──────────┐
//!     │  State   │                      │  State   │                      │  State   │
//!     │(MPT Trie)│                      │(MPT Trie)│                      │(MPT Trie)│
//!     └──────────┘                      └──────────┘                      └──────────┘
//!
//!                           ┌─────────────────────────────────────────────┐
//!                           │                ETH-CLIENT                   │
//!                           │  • Submit transactions                      │
//!                           │  • Deploy smart contracts                   │
//!                           │  • Query state / balances                   │
//!                           │  • Call contract methods                    │
//!                           └─────────────────────────────────────────────┘
//! ```
//!
//! ## Features
//! - Multiple nodes with P2P communication
//! - Transaction submission and propagation
//! - Smart contract deployment and execution
//! - EVM-compatible bytecode execution
//! - State storage using MPT
//! - Block production with configurable consensus
//! - JSON-RPC style client interface

pub mod node;
pub mod state;
pub mod txpool;
pub mod executor;
pub mod evm_bridge;
pub mod consensus;
pub mod network;
pub mod rpc;
pub mod genesis;
pub mod config;
pub mod types;
pub mod error;

// Re-export key types
pub use node::{Node, NodeStatus, Chain};
pub use state::WorldState;
pub use txpool::TransactionPool;
pub use executor::Executor;
pub use evm_bridge::EvmBridge;
pub use consensus::Consensus;
pub use network::{Network, NetworkConfig, NetworkMessage};
pub use rpc::{RpcServer, RpcClient, RpcHandler, RpcRequest, RpcResponse};
pub use genesis::{GenesisConfig, GenesisBuilder, GenesisAlloc};
pub use config::{NodeConfig, RpcConfig, MiningConfig, TxPoolConfig};
pub use types::*;
pub use error::{MiniEthError, Result};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize logging
pub fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("mini_eth=info".parse().unwrap())
        )
        .try_init();
}

/// Start a simple development node
pub async fn run_dev_node() -> Result<Node> {
    init_logging();

    let config = NodeConfig::dev();
    let mut node = Node::new(config);

    node.start().await?;

    Ok(node)
}

/// Start a multi-node testnet
pub async fn run_testnet(node_count: usize) -> Result<Vec<Node>> {
    init_logging();

    let genesis = GenesisConfig::default();
    let mut nodes = Vec::new();

    for i in 0..node_count {
        let mut config = NodeConfig::testnet(i as u8);
        config = config.with_genesis(genesis.clone());

        // Add bootnodes (connect to previous nodes)
        for j in 0..i {
            let bootnode = format!("127.0.0.1:{}", 30303 + j as u16);
            config = config.with_bootnode(bootnode);
        }

        let mut node = Node::new(config);
        node.start().await?;
        nodes.push(node);
    }

    Ok(nodes)
}
