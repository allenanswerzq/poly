//! # RLPx Transport Layer
//!
//! RLPx is the encrypted transport protocol for Ethereum P2P.
//!
//! Connection setup:
//! 1. ECIES handshake (encrypted key exchange)
//! 2. Protocol negotiation (capabilities)
//! 3. Encrypted framed messages

use eth_primitives::{H256, keccak256};
use crate::node::NodeId;
use crate::error::{P2pError, Result};
use k256::ecdsa::SigningKey;
use std::io::{Read, Write};

/// Protocol capability
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Capability {
    /// Protocol name (e.g., "eth")
    pub name: String,
    /// Protocol version
    pub version: u32,
}

impl Capability {
    pub fn new(name: &str, version: u32) -> Self {
        Capability {
            name: name.to_string(),
            version,
        }
    }

    /// Standard Ethereum wire protocol
    pub fn eth(version: u32) -> Self {
        Capability::new("eth", version)
    }

    /// Snap sync protocol
    pub fn snap() -> Self {
        Capability::new("snap", 1)
    }
}

/// Handshake message sent during connection setup
#[derive(Debug, Clone)]
pub struct Hello {
    /// Protocol version
    pub protocol_version: u32,
    /// Client identifier
    pub client_id: String,
    /// Supported capabilities
    pub capabilities: Vec<Capability>,
    /// Listen port
    pub listen_port: u16,
    /// Node ID
    pub node_id: NodeId,
}

impl Hello {
    pub fn new(
        client_id: &str,
        capabilities: Vec<Capability>,
        listen_port: u16,
        node_id: NodeId,
    ) -> Self {
        Hello {
            protocol_version: 5,
            client_id: client_id.to_string(),
            capabilities,
            listen_port,
            node_id,
        }
    }

    /// Find common capabilities with another hello
    pub fn common_capabilities(&self, other: &Hello) -> Vec<Capability> {
        self.capabilities.iter()
            .filter(|c| other.capabilities.contains(c))
            .cloned()
            .collect()
    }
}

/// Disconnect reason codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DisconnectReason {
    DisconnectRequested = 0x00,
    TcpSubsystemError = 0x01,
    BreachOfProtocol = 0x02,
    UselessPeer = 0x03,
    TooManyPeers = 0x04,
    AlreadyConnected = 0x05,
    IncompatibleProtocol = 0x06,
    NullNodeId = 0x07,
    ClientQuitting = 0x08,
    UnexpectedIdentity = 0x09,
    LocalIdentity = 0x0a,
    PingTimeout = 0x0b,
    SubprotocolReason = 0x10,
}

impl DisconnectReason {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(DisconnectReason::DisconnectRequested),
            0x01 => Some(DisconnectReason::TcpSubsystemError),
            0x02 => Some(DisconnectReason::BreachOfProtocol),
            0x03 => Some(DisconnectReason::UselessPeer),
            0x04 => Some(DisconnectReason::TooManyPeers),
            0x05 => Some(DisconnectReason::AlreadyConnected),
            0x06 => Some(DisconnectReason::IncompatibleProtocol),
            0x07 => Some(DisconnectReason::NullNodeId),
            0x08 => Some(DisconnectReason::ClientQuitting),
            0x09 => Some(DisconnectReason::UnexpectedIdentity),
            0x0a => Some(DisconnectReason::LocalIdentity),
            0x0b => Some(DisconnectReason::PingTimeout),
            0x10 => Some(DisconnectReason::SubprotocolReason),
            _ => None,
        }
    }
}

/// RLPx base protocol messages
#[derive(Debug, Clone)]
pub enum RlpxMessage {
    Hello(Hello),
    Disconnect(DisconnectReason),
    Ping,
    Pong,
}

impl RlpxMessage {
    /// Message type ID
    pub fn message_id(&self) -> u8 {
        match self {
            RlpxMessage::Hello(_) => 0x00,
            RlpxMessage::Disconnect(_) => 0x01,
            RlpxMessage::Ping => 0x02,
            RlpxMessage::Pong => 0x03,
        }
    }
}

/// Session keys for encrypted communication
#[derive(Debug, Clone)]
pub struct SessionKeys {
    /// MAC secret
    pub mac_secret: H256,
    /// Encryption key
    pub enc_key: H256,
    /// Egress MAC state
    pub egress_mac: [u8; 32],
    /// Ingress MAC state
    pub ingress_mac: [u8; 32],
}

impl SessionKeys {
    /// Derive session keys from shared secret (simplified)
    pub fn derive(shared_secret: &[u8]) -> Self {
        let mac_secret = keccak256(shared_secret);
        let enc_key = keccak256(mac_secret.as_bytes());

        SessionKeys {
            mac_secret,
            enc_key,
            egress_mac: [0u8; 32],
            ingress_mac: [0u8; 32],
        }
    }
}

/// RLPx connection state
#[derive(Debug)]
pub struct RlpxConnection {
    /// Local node ID
    pub local_id: NodeId,
    /// Remote node ID (after handshake)
    pub remote_id: Option<NodeId>,
    /// Session keys (after handshake)
    pub session_keys: Option<SessionKeys>,
    /// Negotiated capabilities
    pub capabilities: Vec<Capability>,
    /// Connected state
    pub connected: bool,
}

impl RlpxConnection {
    /// Create new connection
    pub fn new(local_id: NodeId) -> Self {
        RlpxConnection {
            local_id,
            remote_id: None,
            session_keys: None,
            capabilities: Vec::new(),
            connected: false,
        }
    }

    /// Create hello message for handshake
    pub fn create_hello(&self) -> Hello {
        Hello::new(
            "Rust-DevP2P/v0.1.0",
            vec![
                Capability::eth(68),
                Capability::snap(),
            ],
            30303,
            self.local_id.clone(),
        )
    }

    /// Process incoming hello
    pub fn handle_hello(&mut self, hello: Hello) -> Result<Vec<Capability>> {
        self.remote_id = Some(hello.node_id.clone());

        // Find common capabilities
        let my_hello = self.create_hello();
        let common = my_hello.common_capabilities(&hello);

        if common.is_empty() {
            return Err(P2pError::HandshakeFailed("No common capabilities".to_string()));
        }

        self.capabilities = common.clone();
        self.connected = true;

        Ok(common)
    }

    /// Handle disconnect message
    pub fn handle_disconnect(&mut self, reason: DisconnectReason) {
        self.connected = false;
        tracing::info!("Peer disconnected: {:?}", reason);
    }

    /// Create disconnect message
    pub fn disconnect(&mut self, reason: DisconnectReason) -> RlpxMessage {
        self.connected = false;
        RlpxMessage::Disconnect(reason)
    }

    /// Check if connection supports capability
    pub fn has_capability(&self, name: &str, version: u32) -> bool {
        self.capabilities.iter().any(|c| c.name == name && c.version >= version)
    }
}

/// Simulated ECIES handshake (real impl would use ECDH)
pub struct Handshake {
    /// Local private key
    pub local_key: SigningKey,
    /// Local nonce
    pub local_nonce: H256,
    /// Remote nonce
    pub remote_nonce: Option<H256>,
    /// Shared secret
    pub shared_secret: Option<Vec<u8>>,
}

impl Handshake {
    /// Create new handshake
    pub fn new(local_key: SigningKey) -> Self {
        use rand::RngCore;
        let mut nonce = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut nonce);

        Handshake {
            local_key,
            local_nonce: H256::new(nonce),
            remote_nonce: None,
            shared_secret: None,
        }
    }

    /// Create auth message (initiator)
    pub fn create_auth(&self) -> Vec<u8> {
        // Simplified: In reality this is ECIES encrypted
        // Contains: signature, public key, nonce, version
        let mut msg = Vec::new();
        msg.extend_from_slice(self.local_nonce.as_bytes());
        msg.extend_from_slice(&[4]); // version
        msg
    }

    /// Create auth-ack message (responder)
    pub fn create_auth_ack(&self) -> Vec<u8> {
        // Simplified: In reality this is ECIES encrypted
        let mut msg = Vec::new();
        msg.extend_from_slice(self.local_nonce.as_bytes());
        msg.extend_from_slice(&[4]); // version
        msg
    }

    /// Process auth message and derive keys
    pub fn process_auth(&mut self, auth: &[u8]) -> Result<SessionKeys> {
        if auth.len() < 33 {
            return Err(P2pError::HandshakeFailed("Invalid auth message".to_string()));
        }

        // Extract remote nonce
        let mut nonce = [0u8; 32];
        nonce.copy_from_slice(&auth[..32]);
        self.remote_nonce = Some(H256::new(nonce));

        // Derive shared secret (simplified - real impl uses ECDH)
        let mut shared = Vec::new();
        shared.extend_from_slice(self.local_nonce.as_bytes());
        shared.extend_from_slice(&nonce);
        self.shared_secret = Some(shared.clone());

        Ok(SessionKeys::derive(&shared))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability() {
        let eth = Capability::eth(68);
        assert_eq!(eth.name, "eth");
        assert_eq!(eth.version, 68);

        let snap = Capability::snap();
        assert_eq!(snap.name, "snap");
    }

    #[test]
    fn test_hello() {
        let (node_id, _) = NodeId::random();
        let hello = Hello::new(
            "Test/v1.0",
            vec![Capability::eth(68)],
            30303,
            node_id,
        );

        assert_eq!(hello.protocol_version, 5);
        assert_eq!(hello.client_id, "Test/v1.0");
        assert_eq!(hello.capabilities.len(), 1);
    }

    #[test]
    fn test_common_capabilities() {
        let (id1, _) = NodeId::random();
        let (id2, _) = NodeId::random();

        let hello1 = Hello::new(
            "Client1",
            vec![Capability::eth(68), Capability::snap()],
            30303,
            id1,
        );

        let hello2 = Hello::new(
            "Client2",
            vec![Capability::eth(68), Capability::new("les", 4)],
            30303,
            id2,
        );

        let common = hello1.common_capabilities(&hello2);
        assert_eq!(common.len(), 1);
        assert_eq!(common[0].name, "eth");
    }

    #[test]
    fn test_connection() {
        let (local_id, _) = NodeId::random();
        let mut conn = RlpxConnection::new(local_id);

        assert!(!conn.connected);
        assert!(conn.remote_id.is_none());

        let (remote_id, _) = NodeId::random();
        let remote_hello = Hello::new(
            "Remote",
            vec![Capability::eth(68)],
            30303,
            remote_id.clone(),
        );

        let common = conn.handle_hello(remote_hello).unwrap();

        assert!(conn.connected);
        assert_eq!(conn.remote_id.as_ref().unwrap(), &remote_id);
        assert!(!common.is_empty());
    }

    #[test]
    fn test_disconnect_reason() {
        assert_eq!(
            DisconnectReason::from_u8(0x04),
            Some(DisconnectReason::TooManyPeers)
        );
        assert_eq!(
            DisconnectReason::from_u8(0xff),
            None
        );
    }

    #[test]
    fn test_handshake() {
        let key = SigningKey::random(&mut rand::thread_rng());
        let mut handshake = Handshake::new(key);

        let auth = handshake.create_auth();
        assert!(!auth.is_empty());

        let ack = handshake.create_auth_ack();
        assert!(!ack.is_empty());
    }
}
