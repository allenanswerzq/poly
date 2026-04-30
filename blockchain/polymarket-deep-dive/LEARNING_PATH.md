# Polymarket Deep Dive - Learning Path

A comprehensive study of how Polymarket works under the hood.

## Overview

Polymarket is a decentralized prediction market platform built on Polygon that allows users to trade on the outcomes of real-world events. This learning path will take you from the fundamentals to a deep understanding of the entire system.

## Learning Modules

### Module 1: Prediction Markets Fundamentals
- [x] What are prediction markets?
- [ ] Market efficiency and price discovery
- [ ] Binary vs. multi-outcome markets
- [ ] How odds translate to probabilities
- [ ] Market making basics

### Module 2: Polymarket Architecture Overview
- [ ] System components diagram
- [ ] On-chain vs. off-chain components
- [ ] Order flow lifecycle
- [ ] User interaction flow

### Module 3: Conditional Token Framework (CTF)
- [ ] Gnosis Conditional Tokens standard
- [ ] Position tokens explained
- [ ] Collateral mechanics
- [ ] Splitting and merging positions
- [ ] ERC-1155 for conditional tokens

### Module 4: Central Limit Order Book (CLOB)
- [ ] Hybrid on-chain/off-chain architecture
- [ ] Order matching engine
- [ ] Operator and exchange contracts
- [ ] Signature-based trading
- [ ] Gas optimization strategies

### Module 5: Oracle & Resolution
- [ ] UMA Optimistic Oracle integration
- [ ] Resolution process
- [ ] Dispute mechanisms
- [ ] Truth verification flow

### Module 6: Smart Contracts Deep Dive
- [ ] CTFExchange contract
- [ ] NegRiskCTFExchange
- [ ] Proxy patterns used
- [ ] Access control mechanisms

### Module 7: Market Making & Liquidity
- [ ] Automated market makers vs. CLOB
- [ ] Liquidity provision strategies
- [ ] Spread dynamics
- [ ] Risk management

### Module 8: Build Your Own
- [ ] Implement simplified prediction market
- [ ] Create conditional tokens
- [ ] Build basic order book
- [ ] Oracle integration

## File Structure

```
polymarket-deep-dive/
├── LEARNING_PATH.md          # This file
├── 01-prediction-markets/    # Theory & fundamentals
├── 02-architecture/          # System design
├── 03-conditional-tokens/    # CTF deep dive
├── 04-clob/                  # Order book mechanics
├── 05-oracle-resolution/     # UMA & dispute resolution
├── 06-smart-contracts/       # Contract analysis
├── 07-market-making/         # Trading strategies
├── 08-implementation/        # Build your own
└── resources/                # Additional materials
```

## Prerequisites

- Solid understanding of Ethereum/EVM
- Solidity smart contract development
- Basic understanding of DeFi concepts
- Familiarity with order books (traditional finance)

## Key Technologies

| Component | Technology |
|-----------|------------|
| Blockchain | Polygon PoS |
| Conditional Tokens | Gnosis CTF (ERC-1155) |
| Oracle | UMA Optimistic Oracle |
| Order Book | Hybrid CLOB |
| Signatures | EIP-712 |

## Resources

- [Polymarket Docs](https://docs.polymarket.com/)
- [Gnosis Conditional Tokens](https://docs.gnosis.io/conditionaltokens/)
- [UMA Protocol](https://docs.uma.xyz/)
- [Polymarket GitHub](https://github.com/Polymarket)
