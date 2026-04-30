//! RPC Server and Client
//!
//! JSON-RPC interface for interacting with the node

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use eth_primitives::{Address, H256, U256};

use crate::types::{Block, SignedTransaction, Receipt, SyncStatus};
use crate::error::{MiniEthError, Result};

/// JSON-RPC Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Vec<Value>,
    pub id: u64,
}

/// JSON-RPC Response
#[derive(Debug, Clone, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub result: Option<Value>,
    pub error: Option<RpcError>,
    pub id: u64,
}

/// JSON-RPC Error
#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcResponse {
    pub fn success(id: u64, result: Value) -> Self {
        RpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: u64, code: i32, message: String) -> Self {
        RpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(RpcError {
                code,
                message,
                data: None,
            }),
            id,
        }
    }
}

/// RPC Method trait for handling different methods
#[async_trait::async_trait]
pub trait RpcHandler: Send + Sync {
    /// Get chain ID
    fn eth_chain_id(&self) -> Result<u64>;

    /// Get current block number
    fn eth_block_number(&self) -> Result<u64>;

    /// Get balance
    fn eth_get_balance(&self, address: Address, block: Option<String>) -> Result<U256>;

    /// Get transaction count (nonce)
    fn eth_get_transaction_count(&self, address: Address, block: Option<String>) -> Result<u64>;

    /// Get code at address
    fn eth_get_code(&self, address: Address, block: Option<String>) -> Result<Vec<u8>>;

    /// Get storage at position
    fn eth_get_storage_at(&self, address: Address, position: U256, block: Option<String>) -> Result<H256>;

    /// Send raw transaction
    async fn eth_send_raw_transaction(&self, raw_tx: Vec<u8>) -> Result<H256>;

    /// Call (simulate transaction)
    fn eth_call(&self, tx: TransactionCall, block: Option<String>) -> Result<Vec<u8>>;

    /// Estimate gas
    fn eth_estimate_gas(&self, tx: TransactionCall) -> Result<u64>;

    /// Get block by number
    fn eth_get_block_by_number(&self, number: u64, full_txs: bool) -> Result<Option<Block>>;

    /// Get block by hash
    fn eth_get_block_by_hash(&self, hash: H256, full_txs: bool) -> Result<Option<Block>>;

    /// Get transaction by hash
    fn eth_get_transaction_by_hash(&self, hash: H256) -> Result<Option<SignedTransaction>>;

    /// Get transaction receipt
    fn eth_get_transaction_receipt(&self, hash: H256) -> Result<Option<Receipt>>;

    /// Get gas price
    fn eth_gas_price(&self) -> Result<u64>;

    /// Get syncing status
    fn eth_syncing(&self) -> Result<SyncStatus>;

    /// Get network version
    fn net_version(&self) -> Result<String>;

    /// Get peer count
    fn net_peer_count(&self) -> Result<usize>;
}

/// Transaction call object for eth_call
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionCall {
    pub from: Option<Address>,
    pub to: Option<Address>,
    pub value: Option<U256>,
    pub data: Option<Vec<u8>>,
    pub gas: Option<u64>,
    pub gas_price: Option<u64>,
}

/// RPC Server
pub struct RpcServer<H: RpcHandler> {
    handler: Arc<H>,
    port: u16,
    running: bool,
}

impl<H: RpcHandler> RpcServer<H> {
    /// Create a new RPC server
    pub fn new(handler: Arc<H>, port: u16) -> Self {
        RpcServer {
            handler,
            port,
            running: false,
        }
    }

    /// Start the server
    pub async fn start(&mut self) -> Result<()> {
        self.running = true;
        tracing::info!("RPC server starting on port {}", self.port);

        // In a real implementation, we'd start an HTTP server here
        // For now, this is a placeholder showing the structure

        Ok(())
    }

    /// Stop the server
    pub async fn stop(&mut self) {
        self.running = false;
        tracing::info!("RPC server stopped");
    }

    /// Handle a request
    pub async fn handle_request(&self, request: RpcRequest) -> RpcResponse {
        match self.dispatch(&request).await {
            Ok(result) => RpcResponse::success(request.id, result),
            Err(e) => RpcResponse::error(request.id, -32603, e.to_string()),
        }
    }

    /// Dispatch request to appropriate handler
    async fn dispatch(&self, request: &RpcRequest) -> Result<Value> {
        match request.method.as_str() {
            "eth_chainId" => {
                let chain_id = self.handler.eth_chain_id()?;
                Ok(json!(format!("0x{:x}", chain_id)))
            }

            "eth_blockNumber" => {
                let block_num = self.handler.eth_block_number()?;
                Ok(json!(format!("0x{:x}", block_num)))
            }

            "eth_getBalance" => {
                let address = parse_address(&request.params[0])?;
                let block = request.params.get(1).and_then(|v| v.as_str().map(String::from));
                let balance = self.handler.eth_get_balance(address, block)?;
                Ok(json!(format!("0x{:x}", balance)))
            }

            "eth_getTransactionCount" => {
                let address = parse_address(&request.params[0])?;
                let block = request.params.get(1).and_then(|v| v.as_str().map(String::from));
                let nonce = self.handler.eth_get_transaction_count(address, block)?;
                Ok(json!(format!("0x{:x}", nonce)))
            }

            "eth_getCode" => {
                let address = parse_address(&request.params[0])?;
                let block = request.params.get(1).and_then(|v| v.as_str().map(String::from));
                let code = self.handler.eth_get_code(address, block)?;
                Ok(json!(format!("0x{}", hex::encode(code))))
            }

            "eth_getStorageAt" => {
                let address = parse_address(&request.params[0])?;
                let position = parse_u256(&request.params[1])?;
                let block = request.params.get(2).and_then(|v| v.as_str().map(String::from));
                let value = self.handler.eth_get_storage_at(address, position, block)?;
                Ok(json!(format!("0x{}", hex::encode(value.as_bytes()))))
            }

            "eth_sendRawTransaction" => {
                let raw = parse_bytes(&request.params[0])?;
                let hash = self.handler.eth_send_raw_transaction(raw).await?;
                Ok(json!(format!("0x{}", hex::encode(hash.as_bytes()))))
            }

            "eth_call" => {
                let tx = parse_tx_call(&request.params[0])?;
                let block = request.params.get(1).and_then(|v| v.as_str().map(String::from));
                let result = self.handler.eth_call(tx, block)?;
                Ok(json!(format!("0x{}", hex::encode(result))))
            }

            "eth_estimateGas" => {
                let tx = parse_tx_call(&request.params[0])?;
                let gas = self.handler.eth_estimate_gas(tx)?;
                Ok(json!(format!("0x{:x}", gas)))
            }

            "eth_getBlockByNumber" => {
                let number = parse_block_number(&request.params[0])?;
                let full_txs = request.params.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
                let block = self.handler.eth_get_block_by_number(number, full_txs)?;
                Ok(serde_json::to_value(block).unwrap_or(Value::Null))
            }

            "eth_getBlockByHash" => {
                let hash = parse_h256(&request.params[0])?;
                let full_txs = request.params.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
                let block = self.handler.eth_get_block_by_hash(hash, full_txs)?;
                Ok(serde_json::to_value(block).unwrap_or(Value::Null))
            }

            "eth_getTransactionByHash" => {
                let hash = parse_h256(&request.params[0])?;
                let tx = self.handler.eth_get_transaction_by_hash(hash)?;
                Ok(serde_json::to_value(tx).unwrap_or(Value::Null))
            }

            "eth_getTransactionReceipt" => {
                let hash = parse_h256(&request.params[0])?;
                let receipt = self.handler.eth_get_transaction_receipt(hash)?;
                Ok(serde_json::to_value(receipt).unwrap_or(Value::Null))
            }

            "eth_gasPrice" => {
                let gas_price = self.handler.eth_gas_price()?;
                Ok(json!(format!("0x{:x}", gas_price)))
            }

            "eth_syncing" => {
                let status = self.handler.eth_syncing()?;
                if status.syncing {
                    Ok(serde_json::to_value(status).unwrap_or(Value::Bool(false)))
                } else {
                    Ok(Value::Bool(false))
                }
            }

            "net_version" => {
                let version = self.handler.net_version()?;
                Ok(json!(version))
            }

            "net_peerCount" => {
                let count = self.handler.net_peer_count()?;
                Ok(json!(format!("0x{:x}", count)))
            }

            "web3_clientVersion" => {
                Ok(json!("mini-eth/1.0.0"))
            }

            _ => Err(MiniEthError::Rpc(format!("Unknown method: {}", request.method))),
        }
    }
}

/// RPC Client for connecting to a node
pub struct RpcClient {
    url: String,
    next_id: std::sync::atomic::AtomicU64,
}

impl RpcClient {
    /// Create a new RPC client
    pub fn new(url: String) -> Self {
        RpcClient {
            url,
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Make an RPC call
    pub async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id,
        };

        // In a real implementation, this would make an HTTP request
        // For now, return placeholder
        let _req_json = serde_json::to_string(&request)
            .map_err(|e| MiniEthError::Rpc(e.to_string()))?;

        tracing::debug!("RPC call to {}: {} (id={})", self.url, method, id);

        // Placeholder response
        Err(MiniEthError::Rpc("HTTP client not implemented".into()))
    }

    /// Get chain ID
    pub async fn eth_chain_id(&self) -> Result<u64> {
        let result = self.call("eth_chainId", vec![]).await?;
        parse_u64_result(&result)
    }

    /// Get block number
    pub async fn eth_block_number(&self) -> Result<u64> {
        let result = self.call("eth_blockNumber", vec![]).await?;
        parse_u64_result(&result)
    }

    /// Get balance
    pub async fn eth_get_balance(&self, address: Address, block: &str) -> Result<U256> {
        let result = self.call(
            "eth_getBalance",
            vec![
                json!(format!("0x{}", hex::encode(address.as_bytes()))),
                json!(block),
            ],
        ).await?;
        parse_u256(&result)
    }

    /// Send raw transaction
    pub async fn eth_send_raw_transaction(&self, raw_tx: &[u8]) -> Result<H256> {
        let result = self.call(
            "eth_sendRawTransaction",
            vec![json!(format!("0x{}", hex::encode(raw_tx)))],
        ).await?;
        parse_h256(&result)
    }

    /// Get transaction receipt
    pub async fn eth_get_transaction_receipt(&self, hash: H256) -> Result<Option<Value>> {
        let result = self.call(
            "eth_getTransactionReceipt",
            vec![json!(format!("0x{}", hex::encode(hash.as_bytes())))],
        ).await?;
        Ok(if result.is_null() { None } else { Some(result) })
    }
}

// Helper parsing functions

fn parse_address(value: &Value) -> Result<Address> {
    let s = value.as_str().ok_or(MiniEthError::Rpc("Invalid address".into()))?;
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).map_err(|_| MiniEthError::Rpc("Invalid hex".into()))?;
    if bytes.len() != 20 {
        return Err(MiniEthError::Rpc("Invalid address length".into()));
    }
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Ok(Address::from(arr))
}

fn parse_h256(value: &Value) -> Result<H256> {
    let s = value.as_str().ok_or(MiniEthError::Rpc("Invalid hash".into()))?;
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).map_err(|_| MiniEthError::Rpc("Invalid hex".into()))?;
    if bytes.len() != 32 {
        return Err(MiniEthError::Rpc("Invalid hash length".into()));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(H256::from(arr))
}

fn parse_u256(value: &Value) -> Result<U256> {
    let s = value.as_str().ok_or(MiniEthError::Rpc("Invalid U256".into()))?;
    U256::from_hex(s).map_err(|_| MiniEthError::Rpc("Invalid number".into()))
}

fn parse_u64_result(value: &Value) -> Result<u64> {
    let s = value.as_str().ok_or(MiniEthError::Rpc("Invalid number".into()))?;
    let s = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(s, 16).map_err(|_| MiniEthError::Rpc("Invalid number".into()))
}

fn parse_bytes(value: &Value) -> Result<Vec<u8>> {
    let s = value.as_str().ok_or(MiniEthError::Rpc("Invalid bytes".into()))?;
    let s = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(s).map_err(|_| MiniEthError::Rpc("Invalid hex".into()))
}

fn parse_block_number(value: &Value) -> Result<u64> {
    let s = value.as_str().ok_or(MiniEthError::Rpc("Invalid block number".into()))?;
    match s {
        "latest" | "pending" => Ok(u64::MAX), // Placeholder - would need actual block number
        "earliest" => Ok(0),
        _ => {
            let s = s.strip_prefix("0x").unwrap_or(s);
            u64::from_str_radix(s, 16).map_err(|_| MiniEthError::Rpc("Invalid block number".into()))
        }
    }
}

fn parse_tx_call(value: &Value) -> Result<TransactionCall> {
    serde_json::from_value(value.clone())
        .map_err(|e| MiniEthError::Rpc(format!("Invalid tx call: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_address() {
        let addr = parse_address(&json!("0x0000000000000000000000000000000000000001")).unwrap();
        assert_eq!(addr.as_bytes()[19], 1);
    }

    #[test]
    fn test_parse_u256() {
        let val = parse_u256(&json!("0xff")).unwrap();
        assert_eq!(val.as_u64(), 255);
    }

    #[test]
    fn test_rpc_response() {
        let resp = RpcResponse::success(1, json!({"test": true}));
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());

        let resp = RpcResponse::error(2, -32600, "Invalid request".into());
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
    }
}
