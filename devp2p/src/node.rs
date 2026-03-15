//! # Node Identity
//!
//! Node identification in Ethereum P2P:
//! - NodeId: 64-byte public key (secp256k1)
//! - enode URL format: enode://<node_id>@<ip>:<port>

use eth_primitives::{H256, keccak256, Address};
use k256::ecdsa::{SigningKey, VerifyingKey};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use rand::rngs::OsRng;
use std::fmt;
use std::net::{IpAddr, SocketAddr};

/// Node ID - 64 bytes (uncompressed secp256k1 public key without prefix)
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct NodeId(pub [u8; 64]);

impl NodeId {
    /// Create NodeId from public key bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() == 64 {
            let mut id = [0u8; 64];
            id.copy_from_slice(bytes);
            Some(NodeId(id))
        } else if bytes.len() == 65 && bytes[0] == 0x04 {
            // Uncompressed with prefix
            let mut id = [0u8; 64];
            id.copy_from_slice(&bytes[1..]);
            Some(NodeId(id))
        } else {
            None
        }
    }

    /// Create from VerifyingKey
    pub fn from_pubkey(key: &VerifyingKey) -> Self {
        let point = key.to_encoded_point(false);
        let bytes = point.as_bytes();
        let mut id = [0u8; 64];
        id.copy_from_slice(&bytes[1..65]);
        NodeId(id)
    }

    /// Generate random NodeId (for testing)
    pub fn random() -> (Self, SigningKey) {
        let key = SigningKey::random(&mut OsRng);
        let pubkey = VerifyingKey::from(&key);
        (Self::from_pubkey(&pubkey), key)
    }

    /// Get node's address (keccak256 of pubkey, last 20 bytes)
    pub fn address(&self) -> Address {
        let hash = keccak256(&self.0);
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&hash.as_bytes()[12..]);
        Address::new(addr)
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// Get as hex string
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }

    /// Parse from hex string
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s).ok()?;
        Self::from_bytes(&bytes)
    }

    /// Distance between two nodes (XOR, used in Kademlia)
    pub fn distance(&self, other: &NodeId) -> H256 {
        let mut result = [0u8; 32];
        for i in 0..32 {
            result[i] = self.0[i] ^ other.0[i];
        }
        H256::new(result)
    }

    /// Log distance (position of highest bit in XOR distance)
    pub fn log_distance(&self, other: &NodeId) -> usize {
        let dist = self.distance(other);
        let bytes = dist.as_bytes();

        for (i, byte) in bytes.iter().enumerate() {
            if *byte != 0 {
                return 256 - (i * 8) - byte.leading_zeros() as usize;
            }
        }
        0
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({}...)", &self.to_hex()[..16])
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Network endpoint (IP + ports)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// IP address
    pub ip: IpAddr,
    /// UDP port (for discovery)
    pub udp_port: u16,
    /// TCP port (for RLPx)
    pub tcp_port: u16,
}

impl Endpoint {
    pub fn new(ip: IpAddr, udp_port: u16, tcp_port: u16) -> Self {
        Endpoint { ip, udp_port, tcp_port }
    }

    /// Get TCP socket address
    pub fn tcp_addr(&self) -> SocketAddr {
        SocketAddr::new(self.ip, self.tcp_port)
    }

    /// Get UDP socket address
    pub fn udp_addr(&self) -> SocketAddr {
        SocketAddr::new(self.ip, self.udp_port)
    }
}

/// Node record (enode URL)
#[derive(Debug, Clone)]
pub struct NodeRecord {
    /// Node ID
    pub id: NodeId,
    /// Endpoint
    pub endpoint: Endpoint,
}

impl NodeRecord {
    pub fn new(id: NodeId, endpoint: Endpoint) -> Self {
        NodeRecord { id, endpoint }
    }

    /// Parse enode URL
    /// Format: enode://<node_id>@<ip>:<port>
    pub fn from_enode_url(url: &str) -> Option<Self> {
        let url = url.strip_prefix("enode://")?;
        let parts: Vec<&str> = url.split('@').collect();
        if parts.len() != 2 {
            return None;
        }

        let id = NodeId::from_hex(parts[0])?;

        let addr_parts: Vec<&str> = parts[1].split(':').collect();
        if addr_parts.len() != 2 {
            return None;
        }

        let ip: IpAddr = addr_parts[0].parse().ok()?;
        let port: u16 = addr_parts[1].split('?').next()?.parse().ok()?;

        let endpoint = Endpoint::new(ip, port, port);
        Some(NodeRecord::new(id, endpoint))
    }

    /// Convert to enode URL
    pub fn to_enode_url(&self) -> String {
        format!(
            "enode://{}@{}:{}",
            self.id.to_hex(),
            self.endpoint.ip,
            self.endpoint.tcp_port
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_node_id_random() {
        let (id1, _) = NodeId::random();
        let (id2, _) = NodeId::random();

        assert_ne!(id1, id2);
        assert_eq!(id1.0.len(), 64);
    }

    #[test]
    fn test_node_id_hex() {
        let (id, _) = NodeId::random();
        let hex = id.to_hex();
        let parsed = NodeId::from_hex(&hex).unwrap();

        assert_eq!(id, parsed);
    }

    #[test]
    fn test_node_distance() {
        let (id1, _) = NodeId::random();
        let (id2, _) = NodeId::random();

        // Distance to self is zero
        let self_dist = id1.distance(&id1);
        assert!(self_dist.as_bytes().iter().all(|&b| b == 0));

        // Distance is symmetric
        assert_eq!(id1.distance(&id2), id2.distance(&id1));
    }

    #[test]
    fn test_log_distance() {
        let (id1, _) = NodeId::random();
        let (id2, _) = NodeId::random();

        let log_dist = id1.log_distance(&id2);
        assert!(log_dist > 0);
        assert!(log_dist <= 256);
    }

    #[test]
    fn test_enode_url() {
        let (id, _) = NodeId::random();
        let endpoint = Endpoint::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            30303,
            30303
        );
        let record = NodeRecord::new(id.clone(), endpoint);

        let url = record.to_enode_url();
        assert!(url.starts_with("enode://"));

        let parsed = NodeRecord::from_enode_url(&url).unwrap();
        assert_eq!(parsed.id, id);
        assert_eq!(parsed.endpoint.ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[test]
    fn test_node_address() {
        let (id, _) = NodeId::random();
        let addr = id.address();

        // Address is 20 bytes
        assert_eq!(addr.as_bytes().len(), 20);
    }
}
