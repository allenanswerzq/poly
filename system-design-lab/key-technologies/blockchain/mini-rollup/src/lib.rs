//! # Mini Rollup
//!
//! A minimal L2 sequencer prototype demonstrating:
//! - Transaction batching
//! - State root computation
//! - Batch commitments to L1
//! - Optimistic rollup concepts

pub mod transaction;
pub mod state;
pub mod batch;
pub mod sequencer;
pub mod proof;
pub mod error;

pub use transaction::{L2Transaction, TransactionType, TransactionResult};
pub use state::{StateDB, Account};
pub use batch::{Batch, BatchHeader};
pub use sequencer::{Sequencer, SequencerConfig, SequencerMetrics};
pub use proof::{FraudProof, AccountProof, Challenge, ChallengeStatus, L1RollupContract};
pub use error::{RollupError, Result};
