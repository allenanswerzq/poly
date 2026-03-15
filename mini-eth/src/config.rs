//! Node configuration

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use eth_primitives::Address;

use crate::genesis::GenesisConfig;

/// Node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node name
    pub name: String,

    /// Data directory
    pub data_dir: PathBuf,

    /// Chain ID
    pub chain_id: u64,

    /// Network configuration
    pub network: NetworkConfig,

    /// RPC configuration
    pub rpc: RpcConfig,

    /// Mining/validator configuration
    pub mining: MiningConfig,

    /// Transaction pool configuration
    pub txpool: TxPoolConfig,

    /// Genesis configuration
    #[serde(skip)]
    pub genesis: Option<GenesisConfig>,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Enable P2P networking
    pub enabled: bool,

    /// Listen address
    pub listen_addr: String,

    /// Listen port
    pub port: u16,

    /// Bootstrap nodes
    pub bootnodes: Vec<String>,

    /// Maximum peers
    pub max_peers: usize,

    /// Node discovery enabled
    pub discovery: bool,
}

/// RPC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    /// Enable HTTP RPC
    pub http_enabled: bool,

    /// HTTP listen address
    pub http_addr: String,

    /// HTTP port
    pub http_port: u16,

    /// Enabled APIs
    pub apis: Vec<String>,

    /// CORS domains
    pub cors_domains: Vec<String>,

    /// Enable WebSocket
    pub ws_enabled: bool,

    /// WebSocket port
    pub ws_port: u16,
}

/// Mining configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningConfig {
    /// Enable mining/block production
    pub enabled: bool,

    /// Coinbase address (receives block rewards)
    pub coinbase: Address,

    /// Block gas limit target
    pub gas_limit: u64,

    /// Extra data to include in blocks
    pub extra_data: Vec<u8>,

    /// Block interval in seconds (for PoA)
    pub block_interval: u64,
}

/// Transaction pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxPoolConfig {
    /// Maximum pending transactions
    pub pending_limit: usize,

    /// Maximum queued transactions per account
    pub queue_limit: usize,

    /// Maximum global queued transactions
    pub global_queue: usize,

    /// Minimum gas price to accept
    pub price_limit: u64,

    /// Transaction lifetime in seconds
    pub lifetime: u64,
}

impl Default for NodeConfig {
    fn default() -> Self {
        NodeConfig {
            name: "mini-eth".to_string(),
            data_dir: PathBuf::from("./data"),
            chain_id: 1337,
            network: NetworkConfig::default(),
            rpc: RpcConfig::default(),
            mining: MiningConfig::default(),
            txpool: TxPoolConfig::default(),
            genesis: Some(GenesisConfig::default()),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        NetworkConfig {
            enabled: true,
            listen_addr: "0.0.0.0".to_string(),
            port: 30303,
            bootnodes: vec![],
            max_peers: 25,
            discovery: true,
        }
    }
}

impl Default for RpcConfig {
    fn default() -> Self {
        RpcConfig {
            http_enabled: true,
            http_addr: "127.0.0.1".to_string(),
            http_port: 8545,
            apis: vec!["eth".into(), "net".into(), "web3".into()],
            cors_domains: vec!["*".into()],
            ws_enabled: false,
            ws_port: 8546,
        }
    }
}

impl Default for MiningConfig {
    fn default() -> Self {
        MiningConfig {
            enabled: false,
            coinbase: Address::zero(),
            gas_limit: 30_000_000,
            extra_data: b"mini-eth".to_vec(),
            block_interval: 12,
        }
    }
}

impl Default for TxPoolConfig {
    fn default() -> Self {
        TxPoolConfig {
            pending_limit: 4096,
            queue_limit: 64,
            global_queue: 1024,
            price_limit: 1_000_000_000, // 1 gwei
            lifetime: 3 * 60 * 60,       // 3 hours
        }
    }
}

impl NodeConfig {
    /// Create a new node config
    pub fn new(name: &str) -> Self {
        NodeConfig {
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// Create a development config
    pub fn dev() -> Self {
        NodeConfig {
            name: "mini-eth-dev".to_string(),
            data_dir: PathBuf::from("./dev-data"),
            chain_id: 1337,
            network: NetworkConfig {
                enabled: false,
                ..Default::default()
            },
            rpc: RpcConfig {
                http_enabled: true,
                http_addr: "127.0.0.1".to_string(),
                http_port: 8545,
                ..Default::default()
            },
            mining: MiningConfig {
                enabled: true,
                coinbase: Address::zero(),
                gas_limit: 30_000_000,
                extra_data: b"dev".to_vec(),
                block_interval: 1, // Fast blocks for dev
            },
            txpool: TxPoolConfig::default(),
            genesis: Some(GenesisConfig::dev()),
        }
    }

    /// Create a test network node config
    pub fn testnet(node_index: u8) -> Self {
        NodeConfig {
            name: format!("testnet-node-{}", node_index),
            data_dir: PathBuf::from(format!("./testnet-data-{}", node_index)),
            chain_id: 31337,
            network: NetworkConfig {
                enabled: true,
                port: 30303 + node_index as u16,
                ..Default::default()
            },
            rpc: RpcConfig {
                http_enabled: true,
                http_port: 8545 + node_index as u16,
                ..Default::default()
            },
            mining: MiningConfig {
                enabled: node_index == 0, // Only first node mines
                block_interval: 5,
                ..Default::default()
            },
            txpool: TxPoolConfig::default(),
            genesis: Some(GenesisConfig::default()),
        }
    }

    /// Set the data directory
    pub fn with_data_dir(mut self, path: PathBuf) -> Self {
        self.data_dir = path;
        self
    }

    /// Set the network port
    pub fn with_port(mut self, port: u16) -> Self {
        self.network.port = port;
        self
    }

    /// Set the RPC port
    pub fn with_rpc_port(mut self, port: u16) -> Self {
        self.rpc.http_port = port;
        self
    }

    /// Enable mining
    pub fn with_mining(mut self, coinbase: Address) -> Self {
        self.mining.enabled = true;
        self.mining.coinbase = coinbase;
        self
    }

    /// Add bootnode
    pub fn with_bootnode(mut self, node: String) -> Self {
        self.network.bootnodes.push(node);
        self
    }

    /// Set genesis
    pub fn with_genesis(mut self, genesis: GenesisConfig) -> Self {
        self.chain_id = genesis.chain_id;
        self.genesis = Some(genesis);
        self
    }

    /// Load from file
    pub fn load(path: &PathBuf) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let config: NodeConfig = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(config)
    }

    /// Save to file
    pub fn save(&self, path: &PathBuf) -> Result<(), std::io::Error> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Ensure data directory exists
    pub fn ensure_data_dir(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.data_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NodeConfig::default();
        assert_eq!(config.chain_id, 1337);
        assert!(config.rpc.http_enabled);
    }

    #[test]
    fn test_dev_config() {
        let config = NodeConfig::dev();
        assert!(config.mining.enabled);
        assert_eq!(config.mining.block_interval, 1);
    }

    #[test]
    fn test_testnet_config() {
        let node0 = NodeConfig::testnet(0);
        let node1 = NodeConfig::testnet(1);

        assert!(node0.mining.enabled);
        assert!(!node1.mining.enabled);
        assert_ne!(node0.network.port, node1.network.port);
    }

    #[test]
    fn test_config_builder() {
        let config = NodeConfig::new("test")
            .with_port(31337)
            .with_rpc_port(9545);

        assert_eq!(config.name, "test");
        assert_eq!(config.network.port, 31337);
        assert_eq!(config.rpc.http_port, 9545);
    }
}
