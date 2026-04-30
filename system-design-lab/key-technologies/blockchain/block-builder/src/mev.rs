//! # MEV (Maximal Extractable Value)
//!
//! MEV extraction and bundle management

use eth_primitives::{Address, H256};
use crate::transaction::{PendingTransaction, TransactionPriority};
use crate::error::{BuilderError, Result};

/// Type of MEV opportunity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpportunityType {
    /// Arbitrage between DEXs
    Arbitrage,
    /// Liquidation of undercollateralized position
    Liquidation,
    /// Sandwich attack (frontrun + backrun)
    Sandwich,
    /// Just-in-time (JIT) liquidity
    JitLiquidity,
    /// Backrun (after a target transaction)
    Backrun,
}

/// An MEV opportunity
#[derive(Debug, Clone)]
pub struct Opportunity {
    /// Type of opportunity
    pub opportunity_type: OpportunityType,
    /// Expected profit in wei
    pub expected_profit: u64,
    /// Target transaction (if any)
    pub target_tx: Option<H256>,
    /// Required transactions to extract value
    pub transactions: Vec<PendingTransaction>,
    /// Risk score (0-100, lower is safer)
    pub risk_score: u8,
}

impl Opportunity {
    /// Create arbitrage opportunity
    pub fn arbitrage(expected_profit: u64, arb_tx: PendingTransaction) -> Self {
        Opportunity {
            opportunity_type: OpportunityType::Arbitrage,
            expected_profit,
            target_tx: None,
            transactions: vec![arb_tx],
            risk_score: 20, // Low risk
        }
    }

    /// Create liquidation opportunity
    pub fn liquidation(expected_profit: u64, liq_tx: PendingTransaction) -> Self {
        Opportunity {
            opportunity_type: OpportunityType::Liquidation,
            expected_profit,
            target_tx: None,
            transactions: vec![liq_tx],
            risk_score: 30,
        }
    }

    /// Create sandwich opportunity (controversial)
    pub fn sandwich(
        expected_profit: u64,
        target: H256,
        frontrun: PendingTransaction,
        backrun: PendingTransaction,
    ) -> Self {
        Opportunity {
            opportunity_type: OpportunityType::Sandwich,
            expected_profit,
            target_tx: Some(target),
            transactions: vec![frontrun, backrun],
            risk_score: 80, // High risk, ethically questionable
        }
    }

    /// Create backrun opportunity
    pub fn backrun(expected_profit: u64, target: H256, backrun_tx: PendingTransaction) -> Self {
        Opportunity {
            opportunity_type: OpportunityType::Backrun,
            expected_profit,
            target_tx: Some(target),
            transactions: vec![backrun_tx],
            risk_score: 40,
        }
    }

    /// Calculate profit after gas costs
    pub fn net_profit(&self, base_fee: u64) -> i64 {
        let gas_cost: u64 = self.transactions.iter()
            .map(|tx| tx.gas_limit * (base_fee + tx.max_priority_fee))
            .sum();

        self.expected_profit as i64 - gas_cost as i64
    }

    /// Is this opportunity profitable?
    pub fn is_profitable(&self, base_fee: u64) -> bool {
        self.net_profit(base_fee) > 0
    }
}

/// MEV Bundle - atomic set of transactions
#[derive(Debug, Clone)]
pub struct MevBundle {
    /// Bundle ID
    pub id: H256,
    /// Transactions (executed in order)
    pub transactions: Vec<PendingTransaction>,
    /// Target block number (0 = any)
    pub target_block: u64,
    /// Minimum timestamp
    pub min_timestamp: Option<u64>,
    /// Maximum timestamp
    pub max_timestamp: Option<u64>,
    /// Reverting transaction hashes allowed
    pub revert_allowed: Vec<H256>,
}

impl MevBundle {
    /// Create new bundle
    pub fn new(transactions: Vec<PendingTransaction>, target_block: u64) -> Self {
        use eth_primitives::keccak256;

        let mut bundle_data = Vec::new();
        for tx in &transactions {
            bundle_data.extend_from_slice(tx.hash.as_bytes());
        }

        MevBundle {
            id: keccak256(&bundle_data),
            transactions,
            target_block,
            min_timestamp: None,
            max_timestamp: None,
            revert_allowed: Vec::new(),
        }
    }

    /// Calculate total gas used
    pub fn total_gas(&self) -> u64 {
        self.transactions.iter().map(|tx| tx.gas_limit).sum()
    }

    /// Calculate total tip offered
    pub fn total_tip(&self) -> u64 {
        self.transactions.iter()
            .map(|tx| tx.max_priority_fee * tx.gas_limit)
            .sum()
    }

    /// With timestamp constraints
    pub fn with_timestamp_range(mut self, min: u64, max: u64) -> Self {
        self.min_timestamp = Some(min);
        self.max_timestamp = Some(max);
        self
    }
}

/// MEV Extractor - finds opportunities
pub struct MevExtractor {
    /// Minimum profit threshold
    min_profit: u64,
    /// Maximum risk score
    max_risk: u8,
    /// Allowed opportunity types
    allowed_types: Vec<OpportunityType>,
}

impl MevExtractor {
    /// Create new extractor
    pub fn new(min_profit: u64) -> Self {
        MevExtractor {
            min_profit,
            max_risk: 50, // Conservative default
            allowed_types: vec![
                OpportunityType::Arbitrage,
                OpportunityType::Liquidation,
                OpportunityType::Backrun,
                // Sandwich not allowed by default (ethically questionable)
            ],
        }
    }

    /// Allow all opportunity types including sandwich
    pub fn allow_all(mut self) -> Self {
        self.allowed_types = vec![
            OpportunityType::Arbitrage,
            OpportunityType::Liquidation,
            OpportunityType::Sandwich,
            OpportunityType::JitLiquidity,
            OpportunityType::Backrun,
        ];
        self.max_risk = 100;
        self
    }

    /// Set max risk
    pub fn with_max_risk(mut self, max_risk: u8) -> Self {
        self.max_risk = max_risk;
        self
    }

    /// Find arbitrage opportunities (simplified simulation)
    pub fn find_arbitrage(&self, _pending_txs: &[PendingTransaction]) -> Vec<Opportunity> {
        // In real implementation:
        // 1. Simulate pending DEX swaps
        // 2. Check price differences across DEXs
        // 3. Calculate profitable arbitrage paths

        // Simplified: return empty (would need DEX state simulation)
        Vec::new()
    }

    /// Find liquidation opportunities (simplified)
    pub fn find_liquidations(&self, _pending_txs: &[PendingTransaction]) -> Vec<Opportunity> {
        // In real implementation:
        // 1. Monitor lending protocol positions
        // 2. Check health factors
        // 3. Calculate liquidation profit

        Vec::new()
    }

    /// Find backrun opportunities
    pub fn find_backruns(&self, pending_txs: &[PendingTransaction]) -> Vec<Opportunity> {
        let mut opportunities = Vec::new();

        for tx in pending_txs {
            // Look for large DEX swaps that might create arbitrage
            if tx.is_contract_call() && tx.value > 1_000_000_000_000_000_000 {
                // Simulate what happens after this transaction
                // In reality, would need full EVM simulation

                // Placeholder for demonstration
            }
        }

        opportunities
    }

    /// Filter opportunities by constraints
    pub fn filter(&self, opportunities: Vec<Opportunity>, base_fee: u64) -> Vec<Opportunity> {
        opportunities.into_iter()
            .filter(|op| {
                // Check type is allowed
                if !self.allowed_types.contains(&op.opportunity_type) {
                    return false;
                }

                // Check risk
                if op.risk_score > self.max_risk {
                    return false;
                }

                // Check profitability
                if !op.is_profitable(base_fee) {
                    return false;
                }

                let profit = op.net_profit(base_fee);
                profit >= self.min_profit as i64
            })
            .collect()
    }

    /// Create bundle from opportunity
    pub fn create_bundle(
        &self,
        opportunity: Opportunity,
        target_block: u64,
    ) -> MevBundle {
        MevBundle::new(opportunity.transactions, target_block)
    }
}

/// Strategies for MEV extraction
#[derive(Debug, Clone, Copy)]
pub enum Strategy {
    /// Pure arbitrage only
    ArbitrageOnly,
    /// Liquidations only
    LiquidationsOnly,
    /// All non-harmful MEV
    EthicalMev,
    /// Maximum extraction (including sandwich)
    MaxExtraction,
}

impl Strategy {
    /// Get allowed opportunity types
    pub fn allowed_types(&self) -> Vec<OpportunityType> {
        match self {
            Strategy::ArbitrageOnly => vec![OpportunityType::Arbitrage],
            Strategy::LiquidationsOnly => vec![OpportunityType::Liquidation],
            Strategy::EthicalMev => vec![
                OpportunityType::Arbitrage,
                OpportunityType::Liquidation,
                OpportunityType::JitLiquidity,
                OpportunityType::Backrun,
            ],
            Strategy::MaxExtraction => vec![
                OpportunityType::Arbitrage,
                OpportunityType::Liquidation,
                OpportunityType::Sandwich,
                OpportunityType::JitLiquidity,
                OpportunityType::Backrun,
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_addresses() -> (Address, Address) {
        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();
        (alice, bob)
    }

    #[test]
    fn test_bundle_creation() {
        let (alice, bob) = test_addresses();

        let tx1 = PendingTransaction::transfer(alice, bob, 1000, 0, 100, 10);
        let tx2 = PendingTransaction::transfer(alice, bob, 2000, 1, 100, 10);

        let bundle = MevBundle::new(vec![tx1, tx2], 100);

        assert_eq!(bundle.transactions.len(), 2);
        assert_eq!(bundle.target_block, 100);
        assert_eq!(bundle.total_gas(), 42000);
    }

    #[test]
    fn test_opportunity_profit() {
        let (alice, bob) = test_addresses();

        let arb_tx = PendingTransaction::transfer(alice, bob, 0, 0, 100, 10);
        let opp = Opportunity::arbitrage(1_000_000_000, arb_tx);

        let base_fee = 50;
        let net = opp.net_profit(base_fee);

        // profit - (21000 * (50 + 10))
        let expected = 1_000_000_000i64 - (21000 * 60);
        assert_eq!(net, expected);
    }

    #[test]
    fn test_strategy_types() {
        let arb_only = Strategy::ArbitrageOnly;
        assert_eq!(arb_only.allowed_types().len(), 1);

        let ethical = Strategy::EthicalMev;
        assert!(!ethical.allowed_types().contains(&OpportunityType::Sandwich));

        let max = Strategy::MaxExtraction;
        assert!(max.allowed_types().contains(&OpportunityType::Sandwich));
    }

    #[test]
    fn test_extractor_filter() {
        let (alice, bob) = test_addresses();

        let extractor = MevExtractor::new(100_000);

        let tx = PendingTransaction::transfer(alice, bob, 0, 0, 100, 5);
        let profitable = Opportunity::arbitrage(10_000_000, tx.clone());
        let risky = Opportunity::sandwich(
            1_000_000,
            H256::default(),
            tx.clone(),
            tx,
        );

        let filtered = extractor.filter(vec![profitable.clone(), risky], 10);

        // Should only include arbitrage (sandwich not allowed by default)
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].opportunity_type, OpportunityType::Arbitrage);
    }
}
