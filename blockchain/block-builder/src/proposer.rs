//! # Proposer (Validator)
//!
//! The proposer is a validator who has the right to propose a block for a given slot.
//! In Proof of Stake Ethereum:
//! - Validators stake 32 ETH
//! - The beacon chain randomly selects a proposer for each slot
//! - Proposer can build their own block OR use MEV-Boost
//! - If using MEV-Boost, proposer queries relays for the best block

use eth_primitives::{H256, keccak256, Address};
use crate::builder::{Block, BlockBuilder, BuilderConfig};
use crate::relay::{Relay, BlindedBlockHeader, SignedCommitment, BuilderSubmission, BuilderId};
use crate::error::{BuilderError, Result};
use std::sync::{Arc, Mutex};

/// Validator identity
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidatorId(pub u64);

impl ValidatorId {
    pub fn new(index: u64) -> Self {
        ValidatorId(index)
    }
}

impl std::fmt::Display for ValidatorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Validator({})", self.0)
    }
}

/// Validator's private key (simplified as bytes)
#[derive(Clone)]
pub struct ValidatorKey {
    /// Private key bytes
    secret: [u8; 32],
    /// Public key / address
    pub pubkey: H256,
}

impl ValidatorKey {
    pub fn new(secret: [u8; 32]) -> Self {
        let pubkey = keccak256(&secret);
        ValidatorKey { secret, pubkey }
    }

    pub fn random() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let hash = keccak256(&seed.to_le_bytes());
        let mut secret = [0u8; 32];
        secret.copy_from_slice(hash.as_bytes());
        Self::new(secret)
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> H256 {
        let mut data = Vec::new();
        data.extend_from_slice(&self.secret);
        data.extend_from_slice(message);
        keccak256(&data)
    }
}

/// How the proposer gets blocks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockSource {
    /// Build locally (no MEV-Boost)
    Local,
    /// Use relay (MEV-Boost)
    Relay,
    /// Prefer relay, fallback to local
    RelayWithFallback,
}

/// Proposer configuration
#[derive(Debug, Clone)]
pub struct ProposerConfig {
    /// Minimum bid to accept from relay
    pub min_bid: u64,
    /// Block source preference
    pub block_source: BlockSource,
    /// Fee recipient address
    pub fee_recipient: Address,
}

impl Default for ProposerConfig {
    fn default() -> Self {
        ProposerConfig {
            min_bid: 0,
            block_source: BlockSource::RelayWithFallback,
            fee_recipient: Address::default(),
        }
    }
}

/// Proposer - a validator who proposes blocks
pub struct Proposer {
    /// Validator ID
    pub id: ValidatorId,
    /// Signing key
    key: ValidatorKey,
    /// Configuration
    config: ProposerConfig,
    /// Local block builder (fallback)
    local_builder: BlockBuilder,
    /// Statistics
    stats: ProposerStats,
}

/// Proposer statistics
#[derive(Debug, Clone, Default)]
pub struct ProposerStats {
    /// Blocks proposed via relay
    pub relay_blocks: u64,
    /// Blocks built locally
    pub local_blocks: u64,
    /// Total earnings from relay payments
    pub total_earnings: u64,
    /// Missed slots
    pub missed_slots: u64,
}

/// Result of block proposal
#[derive(Debug)]
pub struct ProposalResult {
    /// The proposed block
    pub block: Block,
    /// Was it from relay?
    pub from_relay: bool,
    /// Payment received (if from relay)
    pub payment: u64,
}

impl Proposer {
    /// Create new proposer
    pub fn new(id: ValidatorId, key: ValidatorKey, config: ProposerConfig) -> Self {
        Proposer {
            id,
            key,
            config: config.clone(),
            local_builder: BlockBuilder::new(BuilderConfig {
                ..Default::default()
            }),
            stats: ProposerStats::default(),
        }
    }

    /// Create with random key
    pub fn new_random(id: u64) -> Self {
        Self::new(
            ValidatorId::new(id),
            ValidatorKey::random(),
            ProposerConfig::default(),
        )
    }

    /// Propose a block for a slot
    /// This is the main function called when it's this validator's turn
    pub fn propose_block(&mut self, slot: u64, relay: Option<&mut Relay>) -> Result<ProposalResult> {
        match self.config.block_source {
            BlockSource::Local => {
                self.propose_local(slot)
            }
            BlockSource::Relay => {
                match relay {
                    Some(r) => self.propose_from_relay(slot, r),
                    None => Err(BuilderError::BuildError("No relay configured".to_string())),
                }
            }
            BlockSource::RelayWithFallback => {
                if let Some(r) = relay {
                    match self.propose_from_relay(slot, r) {
                        Ok(result) => Ok(result),
                        Err(_) => {
                            // Fallback to local
                            self.propose_local(slot)
                        }
                    }
                } else {
                    self.propose_local(slot)
                }
            }
        }
    }

    /// Get block from relay
    fn propose_from_relay(&mut self, slot: u64, relay: &mut Relay) -> Result<ProposalResult> {
        // 1. Request header from relay
        let header = relay.get_header(slot)
            .ok_or(BuilderError::BuildError("No block available from relay".to_string()))?;

        // 2. Check if bid meets minimum
        if header.bid < self.config.min_bid {
            return Err(BuilderError::BuildError(
                format!("Bid {} below minimum {}", header.bid, self.config.min_bid)
            ));
        }

        // 3. Sign commitment (blind signing!)
        let commitment = self.sign_commitment(slot, header.block_hash);

        // 4. Submit commitment and get full block
        let block = relay.submit_commitment(commitment)?;

        // 5. Update stats
        self.stats.relay_blocks += 1;
        self.stats.total_earnings += header.bid;

        Ok(ProposalResult {
            block,
            from_relay: true,
            payment: header.bid,
        })
    }

    /// Build block locally
    fn propose_local(&mut self, slot: u64) -> Result<ProposalResult> {
        let block = self.local_builder.build_block()?;
        self.stats.local_blocks += 1;

        Ok(ProposalResult {
            block,
            from_relay: false,
            payment: 0,
        })
    }

    /// Sign a commitment to a block hash
    fn sign_commitment(&self, slot: u64, block_hash: H256) -> SignedCommitment {
        let mut message = Vec::new();
        message.extend_from_slice(&slot.to_le_bytes());
        message.extend_from_slice(block_hash.as_bytes());

        let signature = self.key.sign(&message);

        SignedCommitment {
            block_hash,
            slot,
            signature,
        }
    }

    /// Get mutable reference to local builder
    pub fn local_builder_mut(&mut self) -> &mut BlockBuilder {
        &mut self.local_builder
    }

    /// Get statistics
    pub fn stats(&self) -> &ProposerStats {
        &self.stats
    }

    /// Get public key
    pub fn pubkey(&self) -> H256 {
        self.key.pubkey
    }

    /// Check if this validator is the proposer for a slot
    /// In real Ethereum, this is determined by RANDAO
    pub fn is_proposer_for_slot(&self, slot: u64, total_validators: u64) -> bool {
        // Simplified: deterministic based on slot
        (slot % total_validators) == self.id.0
    }
}

/// Validator set - manages multiple validators
pub struct ValidatorSet {
    /// All validators
    validators: Vec<Proposer>,
}

impl ValidatorSet {
    /// Create a new validator set
    pub fn new() -> Self {
        ValidatorSet {
            validators: Vec::new(),
        }
    }

    /// Create with n random validators
    pub fn with_validators(count: u64) -> Self {
        let mut set = Self::new();
        for i in 0..count {
            set.add_validator(Proposer::new_random(i));
        }
        set
    }

    /// Add a validator
    pub fn add_validator(&mut self, validator: Proposer) {
        self.validators.push(validator);
    }

    /// Get validator by index
    pub fn get(&self, index: usize) -> Option<&Proposer> {
        self.validators.get(index)
    }

    /// Get mutable validator
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Proposer> {
        self.validators.get_mut(index)
    }

    /// Get proposer for a slot
    pub fn get_proposer_for_slot(&self, slot: u64) -> Option<&Proposer> {
        if self.validators.is_empty() {
            return None;
        }
        let index = (slot as usize) % self.validators.len();
        self.validators.get(index)
    }

    /// Get mutable proposer for a slot
    pub fn get_proposer_for_slot_mut(&mut self, slot: u64) -> Option<&mut Proposer> {
        if self.validators.is_empty() {
            return None;
        }
        let index = (slot as usize) % self.validators.len();
        self.validators.get_mut(index)
    }

    /// Total validator count
    pub fn len(&self) -> usize {
        self.validators.len()
    }

    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::PendingTransaction;

    fn test_addresses() -> (Address, Address) {
        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();
        (alice, bob)
    }

    #[test]
    fn test_proposer_local_block() {
        let mut proposer = Proposer::new_random(0);
        proposer.config.block_source = BlockSource::Local;

        let result = proposer.propose_block(1, None).unwrap();
        assert!(!result.from_relay);
        assert_eq!(result.payment, 0);
        assert_eq!(proposer.stats().local_blocks, 1);
    }

    #[test]
    fn test_proposer_from_relay() {
        let mut proposer = Proposer::new_random(0);
        proposer.config.block_source = BlockSource::Relay;

        let mut relay = Relay::default();

        // Builder submits a block
        let mut builder = BlockBuilder::new(BuilderConfig::default());
        let block = builder.build_block().unwrap();
        let submission = BuilderSubmission::new(BuilderId::random(), block, 1_000_000);
        relay.submit_block(submission).unwrap();

        // Proposer gets block from relay
        let result = proposer.propose_block(1, Some(&mut relay)).unwrap();
        assert!(result.from_relay);
        assert_eq!(result.payment, 1_000_000);
        assert_eq!(proposer.stats().relay_blocks, 1);
    }

    #[test]
    fn test_proposer_fallback() {
        let mut proposer = Proposer::new_random(0);
        proposer.config.block_source = BlockSource::RelayWithFallback;

        // No relay, should fallback to local
        let result = proposer.propose_block(1, None).unwrap();
        assert!(!result.from_relay);
    }

    #[test]
    fn test_validator_set() {
        let set = ValidatorSet::with_validators(10);
        assert_eq!(set.len(), 10);

        // Slot 0 -> validator 0
        assert_eq!(set.get_proposer_for_slot(0).unwrap().id.0, 0);
        // Slot 5 -> validator 5
        assert_eq!(set.get_proposer_for_slot(5).unwrap().id.0, 5);
        // Slot 10 -> validator 0 (wraps)
        assert_eq!(set.get_proposer_for_slot(10).unwrap().id.0, 0);
    }

    #[test]
    fn test_signing() {
        let key = ValidatorKey::random();
        let message = b"test message";

        let sig1 = key.sign(message);
        let sig2 = key.sign(message);

        // Same message, same key -> same signature
        assert_eq!(sig1, sig2);

        // Different message -> different signature
        let sig3 = key.sign(b"different");
        assert_ne!(sig1, sig3);
    }
}
