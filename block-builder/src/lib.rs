//! # Block Builder - Mini Ethereum Simulation
//!
//! A comprehensive block builder implementation demonstrating the full
//! Ethereum block production pipeline including:
//!
//! ## Core Components
//! - **Transaction**: Pending transactions with priority and gas pricing
//! - **Mempool**: Transaction pool management and eviction
//! - **Builder**: Block construction and transaction ordering
//! - **MEV**: Maximum Extractable Value detection and bundling
//!
//! ## PBS (Proposer-Builder Separation)
//! - **Relay**: Trusted intermediary between builders and proposers
//! - **Proposer**: Validator that proposes blocks to the network
//! - **Slot**: Beacon chain timing (slots, epochs)
//!
//! ## Infrastructure
//! - **Chain**: Blockchain state with fork handling and reorgs
//! - **Network**: P2P network simulation for message passing
//!
//! ## Architecture
//!
//! ```text
//!   Users            Searchers
//!     |                  |
//!     v                  v
//! +--------+      +-----------+
//! | Mempool| <--- | MEV Bundles|
//! +--------+      +-----------+
//!     |                  |
//!     +--------+---------+
//!              |
//!              v
//!        +---------+
//!        | Builder |  (constructs optimal blocks)
//!        +---------+
//!              |
//!              v
//!        +---------+
//!        |  Relay  |  (runs auction, hides block contents)
//!        +---------+
//!              |
//!              v
//!       +----------+
//!       | Proposer |  (signs header, gets block)
//!       +----------+
//!              |
//!              v
//!        +-------+
//!        | Chain |  (canonical state)
//!        +-------+
//! ```

// Core block building modules
pub mod error;
pub mod transaction;
pub mod mempool;
pub mod builder;
pub mod mev;

// PBS (Proposer-Builder Separation) modules
pub mod relay;
pub mod proposer;
pub mod slot;

// Infrastructure modules
pub mod chain;
pub mod network;

// Re-export core types
pub use error::{BuilderError, Result};
pub use transaction::{PendingTransaction, TransactionPriority};
pub use mempool::{Mempool, MempoolConfig, MempoolStats};
pub use builder::{BlockBuilder, Block, BuilderConfig};
pub use mev::{MevBundle, MevExtractor, Opportunity, OpportunityType, Strategy};

// Re-export PBS types
pub use relay::{Relay, RelayConfig, BuilderSubmission, SignedCommitment, SlotAuction};
pub use proposer::{Proposer, ProposerConfig, ValidatorSet, BlockSource};
pub use slot::{Slot, Epoch, SlotClock, SlotPhase, SlotScheduler};

// Re-export infrastructure types
pub use chain::{ChainState, Block as ChainBlock, ForkChoiceRule, BlockInsertResult};
pub use network::{NetworkSimulator, NetworkNode, NetworkMessage, NodeType, NodeId};
