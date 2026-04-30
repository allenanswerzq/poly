//! # DevP2P - Ethereum P2P Networking
//!
//! Implementation of Ethereum's DevP2P protocol:
//! - Node discovery (discv4)
//! - RLPx transport layer
//! - Wire protocol messages
//!
//! This is a simplified educational implementation.

pub mod node;
pub mod discovery;
pub mod rlpx;
pub mod wire;
pub mod error;

pub use node::{NodeId, NodeRecord, Endpoint};
pub use discovery::Discovery;
pub use rlpx::{RlpxConnection, Capability};
pub use wire::{Message, EthMessage};
pub use error::P2pError;
