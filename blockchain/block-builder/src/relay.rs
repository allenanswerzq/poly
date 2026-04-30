//! # Relay
//!
//! The relay is a trusted intermediary between block builders and proposers.
//! It performs these functions:
//! - Receives blocks from multiple builders
//! - Validates blocks (simulates execution)
//! - Runs an auction to select the highest-paying block
//! - Hides block contents from proposer until commitment
//! - Reveals block after proposer signs

use eth_primitives::{H256, keccak256, Address};
use crate::builder::Block;
use crate::error::{BuilderError, Result};
use std::collections::HashMap;

/// Builder identity (their public key)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BuilderId(pub [u8; 32]);

impl BuilderId {
    pub fn new(bytes: [u8; 32]) -> Self {
        BuilderId(bytes)
    }

    pub fn random() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let hash = keccak256(&seed.to_le_bytes());
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(hash.as_bytes());
        BuilderId(bytes)
    }

    pub fn to_hex(&self) -> String {
        hex::encode(&self.0[..8])
    }
}

impl std::fmt::Display for BuilderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Builder({}...)", self.to_hex())
    }
}

/// A block submission from a builder
#[derive(Debug, Clone)]
pub struct BuilderSubmission {
    /// Who submitted this block
    pub builder_id: BuilderId,
    /// The full block
    pub block: Block,
    /// Bid amount (wei) - payment to proposer
    pub bid: u64,
    /// Signature (simplified - just a hash for now)
    pub signature: H256,
}

impl BuilderSubmission {
    pub fn new(builder_id: BuilderId, block: Block, bid: u64) -> Self {
        // Create a simple signature (hash of block + bid)
        let mut data = Vec::new();
        data.extend_from_slice(block.hash().as_bytes());
        data.extend_from_slice(&bid.to_le_bytes());
        data.extend_from_slice(&builder_id.0);
        let signature = keccak256(&data);

        BuilderSubmission {
            builder_id,
            block,
            bid,
            signature,
        }
    }
}

/// Block header for blind signing (hides tx contents)
#[derive(Debug, Clone)]
pub struct BlindedBlockHeader {
    /// Block number
    pub number: u64,
    /// Parent hash
    pub parent_hash: H256,
    /// Block hash
    pub block_hash: H256,
    /// Builder who created this
    pub builder_id: BuilderId,
    /// Bid amount
    pub bid: u64,
    /// Gas used
    pub gas_used: u64,
    /// Transaction count (but not contents!)
    pub tx_count: usize,
}

impl BlindedBlockHeader {
    pub fn from_submission(submission: &BuilderSubmission) -> Self {
        BlindedBlockHeader {
            number: submission.block.number,
            parent_hash: submission.block.parent_hash,
            block_hash: submission.block.hash(),
            builder_id: submission.builder_id.clone(),
            bid: submission.bid,
            gas_used: submission.block.gas_used,
            tx_count: submission.block.transactions.len(),
        }
    }
}

/// Proposer's signed commitment to a block
#[derive(Debug, Clone)]
pub struct SignedCommitment {
    /// Block hash being committed to
    pub block_hash: H256,
    /// Slot number
    pub slot: u64,
    /// Proposer's signature (simplified)
    pub signature: H256,
}

/// Auction state for a single slot
#[derive(Debug)]
pub struct SlotAuction {
    /// Slot number
    pub slot: u64,
    /// All submissions for this slot
    pub submissions: Vec<BuilderSubmission>,
    /// Winning submission (after auction closes)
    pub winner: Option<usize>,
    /// Has proposer committed?
    pub committed: bool,
}

impl SlotAuction {
    pub fn new(slot: u64) -> Self {
        SlotAuction {
            slot,
            submissions: Vec::new(),
            winner: None,
            committed: false,
        }
    }

    /// Add a submission, return index
    pub fn add_submission(&mut self, submission: BuilderSubmission) -> usize {
        let idx = self.submissions.len();
        self.submissions.push(submission);
        idx
    }

    /// Find the winning bid
    fn find_winner(&mut self) -> Option<&BuilderSubmission> {
        if self.submissions.is_empty() {
            return None;
        }

        // Find highest bid
        let winner_idx = self.submissions
            .iter()
            .enumerate()
            .max_by_key(|(_, s)| s.bid)
            .map(|(i, _)| i)?;

        self.winner = Some(winner_idx);
        self.submissions.get(winner_idx)
    }

    /// Get winner
    fn get_winner(&self) -> Option<&BuilderSubmission> {
        self.winner.and_then(|idx| self.submissions.get(idx))
    }
}

/// Relay configuration
#[derive(Debug, Clone)]
pub struct RelayConfig {
    /// Relay name
    pub name: String,
    /// Minimum bid to accept
    pub min_bid: u64,
    /// Whether to simulate blocks
    pub simulate_blocks: bool,
    /// Fee charged by relay (basis points, 0-10000)
    pub relay_fee_bps: u16,
}

impl Default for RelayConfig {
    fn default() -> Self {
        RelayConfig {
            name: "Local Relay".to_string(),
            min_bid: 0,
            simulate_blocks: true,
            relay_fee_bps: 0, // No fee
        }
    }
}

/// The Relay - trusted intermediary
pub struct Relay {
    /// Configuration
    config: RelayConfig,
    /// Auctions by slot
    auctions: HashMap<u64, SlotAuction>,
    /// Revealed blocks (after proposer commits)
    revealed_blocks: HashMap<H256, Block>,
    /// Statistics
    stats: RelayStats,
}

/// Relay statistics
#[derive(Debug, Clone, Default)]
pub struct RelayStats {
    pub total_submissions: u64,
    pub total_slots: u64,
    pub total_value_delivered: u64,
    pub blocks_revealed: u64,
}

impl Relay {
    /// Create new relay
    pub fn new(config: RelayConfig) -> Self {
        Relay {
            config,
            auctions: HashMap::new(),
            revealed_blocks: HashMap::new(),
            stats: RelayStats::default(),
        }
    }

    /// Create with default config
    pub fn default() -> Self {
        Self::new(RelayConfig::default())
    }

    /// Builder submits a block
    /// POST /relay/v1/builder/blocks
    pub fn submit_block(&mut self, submission: BuilderSubmission) -> Result<()> {
        let slot = submission.block.number; // Using block number as slot for simplicity

        // Validate bid
        if submission.bid < self.config.min_bid {
            return Err(BuilderError::InvalidBundle(
                format!("Bid {} below minimum {}", submission.bid, self.config.min_bid)
            ));
        }

        // Validate block (simplified)
        if self.config.simulate_blocks {
            self.validate_block(&submission.block)?;
        }

        // Get or create auction for this slot
        let auction = self.auctions
            .entry(slot)
            .or_insert_with(|| SlotAuction::new(slot));

        // Reject if already committed
        if auction.committed {
            return Err(BuilderError::InvalidBundle(
                "Slot already committed".to_string()
            ));
        }

        // Add submission
        auction.add_submission(submission);
        self.stats.total_submissions += 1;

        Ok(())
    }

    /// Validate a block (simplified simulation)
    fn validate_block(&self, block: &Block) -> Result<()> {
        // Check gas used doesn't exceed limit
        if block.gas_used > block.gas_limit {
            return Err(BuilderError::GasLimitExceeded);
        }

        // Check transactions have valid structure
        for tx in &block.transactions {
            if tx.gas_limit == 0 {
                return Err(BuilderError::InvalidTransaction(
                    "Zero gas limit".to_string()
                ));
            }
        }

        // In real implementation: full EVM simulation
        Ok(())
    }

    /// Proposer requests the best block header for a slot
    /// GET /relay/v1/builder/header/{slot}/{parent_hash}
    pub fn get_header(&mut self, slot: u64) -> Option<BlindedBlockHeader> {
        let auction = self.auctions.get_mut(&slot)?;
        let winner = auction.find_winner()?;
        Some(BlindedBlockHeader::from_submission(winner))
    }

    /// Get all available bids for a slot (for transparency)
    pub fn get_all_bids(&self, slot: u64) -> Vec<(BuilderId, u64)> {
        self.auctions.get(&slot)
            .map(|auction| {
                auction.submissions.iter()
                    .map(|s| (s.builder_id.clone(), s.bid))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Proposer commits to a block by signing
    /// POST /relay/v1/builder/blinded_blocks
    pub fn submit_commitment(&mut self, commitment: SignedCommitment) -> Result<Block> {
        let slot = commitment.slot;

        let auction = self.auctions.get_mut(&slot)
            .ok_or(BuilderError::InvalidBundle("No auction for slot".to_string()))?;

        // Get the winning block info first
        let (block, bid, winner_hash) = {
            let winner = auction.get_winner()
                .ok_or(BuilderError::InvalidBundle("No winner for slot".to_string()))?;
            (winner.block.clone(), winner.bid, winner.block.hash())
        };

        // Verify commitment matches winning block
        if commitment.block_hash != winner_hash {
            return Err(BuilderError::InvalidBundle(
                "Commitment doesn't match winning block".to_string()
            ));
        }

        // Mark as committed
        auction.committed = true;

        // Store revealed block
        self.revealed_blocks.insert(block.hash(), block.clone());
        self.stats.blocks_revealed += 1;
        self.stats.total_value_delivered += bid;

        Ok(block)
    }

    /// Get a revealed block by hash
    pub fn get_revealed_block(&self, hash: &H256) -> Option<&Block> {
        self.revealed_blocks.get(hash)
    }

    /// Get relay statistics
    pub fn stats(&self) -> &RelayStats {
        &self.stats
    }

    /// Get config
    pub fn config(&self) -> &RelayConfig {
        &self.config
    }

    /// Clear old auctions (housekeeping)
    pub fn clear_old_auctions(&mut self, before_slot: u64) {
        self.auctions.retain(|&slot, _| slot >= before_slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{BlockBuilder, BuilderConfig};
    use crate::transaction::PendingTransaction;
    use eth_primitives::Address;

    fn test_addresses() -> (Address, Address) {
        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();
        (alice, bob)
    }

    #[test]
    fn test_submit_block() {
        let mut relay = Relay::default();
        let builder_id = BuilderId::random();

        // Create a block
        let config = BuilderConfig {
            gas_limit: 1_000_000,
            base_fee: 10,
            ..Default::default()
        };
        let mut builder = BlockBuilder::new(config);
        let block = builder.build_block().unwrap();

        // Submit to relay
        let submission = BuilderSubmission::new(builder_id, block, 1_000_000);
        relay.submit_block(submission).unwrap();

        assert_eq!(relay.stats().total_submissions, 1);
    }

    #[test]
    fn test_auction_winner() {
        let mut relay = Relay::default();
        let (alice, bob) = test_addresses();

        let config = BuilderConfig {
            gas_limit: 1_000_000,
            base_fee: 10,
            ..Default::default()
        };

        // Builder 1 submits with low bid
        let mut builder1 = BlockBuilder::new(config.clone());
        let tx1 = PendingTransaction::transfer(alice, bob, 1000, 0, 100, 10);
        builder1.mempool_mut().add(tx1).unwrap();
        let block1 = builder1.build_block().unwrap();
        let submission1 = BuilderSubmission::new(BuilderId::random(), block1, 100);
        relay.submit_block(submission1).unwrap();

        // Builder 2 submits with high bid
        let mut builder2 = BlockBuilder::new(config);
        let tx2 = PendingTransaction::transfer(alice, bob, 2000, 1, 100, 20);
        builder2.mempool_mut().add(tx2).unwrap();
        let block2 = builder2.build_block().unwrap();
        let submission2 = BuilderSubmission::new(BuilderId::random(), block2, 500);
        relay.submit_block(submission2).unwrap();

        // Get header - should be highest bidder
        let header = relay.get_header(1).unwrap();
        assert_eq!(header.bid, 500);
    }

    #[test]
    fn test_commitment_reveals_block() {
        let mut relay = Relay::default();
        let builder_id = BuilderId::random();

        let config = BuilderConfig {
            gas_limit: 1_000_000,
            base_fee: 10,
            ..Default::default()
        };

        let mut builder = BlockBuilder::new(config);
        let block = builder.build_block().unwrap();
        let block_hash = block.hash();

        let submission = BuilderSubmission::new(builder_id, block, 1000);
        relay.submit_block(submission).unwrap();

        // Get header
        let header = relay.get_header(1).unwrap();
        assert_eq!(header.block_hash, block_hash);

        // Commit
        let commitment = SignedCommitment {
            block_hash,
            slot: 1,
            signature: H256::default(),
        };
        let revealed = relay.submit_commitment(commitment).unwrap();

        assert_eq!(revealed.hash(), block_hash);
        assert!(relay.get_revealed_block(&block_hash).is_some());
    }
}
