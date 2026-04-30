# Blockchain Hands-On Learning Path 🎓

Complete curriculum to deeply understand Ethereum and L2.

## Your Learning Exercises

### ✅ Phase 1: Security & EVM (Complete)
| # | Exercise | What You Learn |
|---|----------|----------------|
| 01 | Reentrancy | Attack/defend vulnerable contracts |
| 02 | Storage Layout | How EVM stores data |
| 03 | Flash Loans | Price manipulation attacks |
| 04 | Gas Optimization | Write efficient contracts |
| 05 | Access Control | Ownership vulnerabilities |

### 📝 Phase 2: DeFi Building Blocks (New)
| # | Exercise | What You Learn |
|---|----------|----------------|
| 06 | Build AMM | Constant product, liquidity, swaps |
| 07 | Upgradeable | Proxy patterns, DELEGATECALL |
| 08 | Merkle Airdrop | Proofs, gas-efficient airdrops |
| 09 | Signatures | EIP-712, permits, meta-tx |
| 10 | L2 Concepts | State channels, rollup mechanics |

---

## Running the Labs

```bash
cd /home/yibai/poly/blockchain-lab

# Phase 1: Security
forge test --match-contract ReentrancyTest -vvvv
forge test --match-contract StorageTest -vvvv
forge test --match-contract FlashLoanTest -vvvv
forge test --match-contract GasOptimizationTest --gas-report
forge test --match-contract AccessControlTest -vvvv

# Phase 2: Build (after writing tests)
forge test --match-contract AMMTest -vvvv
forge test --match-contract UpgradeableTest -vvvv
```

---

## Beyond The Lab: What Else To Do

### 1. 🎮 Security Challenges (Essential!)
| Challenge | Difficulty | Link |
|-----------|------------|------|
| Ethernaut | Beginner | https://ethernaut.openzeppelin.com |
| Damn Vulnerable DeFi | Intermediate | https://damnvulnerabledefi.xyz |
| Capture The Ether | Intermediate | https://capturetheether.com |
| Paradigm CTF | Advanced | Past challenges on GitHub |

### 2. 📚 Deep Reading
| Topic | Resource |
|-------|----------|
| EVM Opcodes | https://evm.codes |
| Yellow Paper | Ethereum specification |
| L2Beat | https://l2beat.com (L2 security) |
| Secureum | https://secureum.substack.com |

### 3. 🔨 Build Real Projects
| Project | Skills Learned |
|---------|----------------|
| **NFT Marketplace** | ERC721, escrow, royalties |
| **Staking Protocol** | Rewards math, time-weighted |
| **Governance (DAO)** | Voting, timelock, proposals |
| **Lending Protocol** | Interest rates, liquidation |
| **DEX Aggregator** | Multi-hop routing |

### 4. 🌐 Deploy to Real Networks
See `L2_DEPLOYMENT.md` for deploying to:
- Arbitrum Sepolia
- Optimism Sepolia
- Base Sepolia

### 5. 🔬 Advanced Topics

#### MEV (Maximal Extractable Value)
```
- Flashbots: https://docs.flashbots.net
- MEV-Share, MEV-Boost
- Build a simple arbitrage bot
```

#### Account Abstraction (ERC-4337)
```
- Smart contract wallets
- Bundlers and paymasters
- Session keys
```

#### ZK Proofs
```
- Circom & SnarkJS
- Noir (Aztec)
- Build a ZK-based voting system
```

---

## Recommended Learning Order

```
Week 1-2: Complete Phase 1 exercises ✅
    ↓
Week 3: Ethernaut challenges (all 30 levels)
    ↓
Week 4: Complete Phase 2 exercises
    ↓
Week 5: Damn Vulnerable DeFi
    ↓
Week 6: Deploy to L2 testnets
    ↓
Week 7-8: Build your own project
    ↓
Ongoing: Security auditing, MEV, ZK
```

---

## File Structure

```
blockchain-lab/
├── src/
│   ├── 01_Reentrancy.sol      ✅
│   ├── 02_StorageLayout.sol   ✅
│   ├── 03_FlashLoan.sol       ✅
│   ├── 04_GasOptimization.sol ✅
│   ├── 05_AccessControl.sol   ✅
│   ├── 06_BuildAMM.sol        📝
│   ├── 07_Upgradeable.sol     📝
│   ├── 08_MerkleAirdrop.sol   📝
│   ├── 09_Signatures.sol      📝
│   ├── 10_L2Concepts.sol      📝
│   └── tokens/
│       └── ERC20.sol
├── test/
│   └── *.t.sol
├── script/
│   └── (deployment scripts)
├── L2_DEPLOYMENT.md
└── README.md
```

---

## Quick Commands

```bash
# Run all tests
forge test -v

# Run with gas report
forge test --gas-report

# Run specific test
forge test --match-test testReentrancyAttack -vvvv

# Deploy to testnet
forge create --rpc-url arbitrum_sepolia \
  --private-key $PRIVATE_KEY \
  src/06_BuildAMM.sol:SimpleAMM

# Verify contract
forge verify-contract <ADDRESS> SimpleAMM \
  --chain arbitrum-sepolia
```

---

## 🦀 Phase 3: Low-Level Rust Implementation

After mastering Solidity and understanding Ethereum at the smart contract level, go deeper by implementing core components in Rust.

### Why Rust for Blockchain?
- **Reth** (Paradigm) - Ethereum execution client in Rust
- **Lighthouse** - Consensus client in Rust
- **Starknet/Cairo** - ZK rollup tooling
- **Solana, Sui, Aptos** - All Rust-based chains

---

### 📚 Rust Prerequisites

| Task | Resource | Est. Time |
|------|----------|-----------|
| Rust basics | [The Rust Book](https://doc.rust-lang.org/book/) | 1-2 weeks |
| Async Rust | Tokio tutorial | 3-4 days |
| Rust crypto | `k256`, `sha3`, `tiny-keccak` crates | 2-3 days |
| RLP encoding | `alloy-rlp` crate | 1 day |

---

### 🔧 Project 1: EVM Primitives Library

Build fundamental types from scratch:

```
rust-eth-primitives/
├── src/
│   ├── lib.rs
│   ├── address.rs      # 20-byte Ethereum address
│   ├── hash.rs         # H256, Keccak256
│   ├── uint.rs         # U256 arithmetic
│   ├── rlp.rs          # RLP encode/decode
│   ├── transaction.rs  # Legacy, EIP-1559, EIP-4844
│   └── signature.rs    # ECDSA secp256k1, recovery
├── Cargo.toml
└── tests/
```

**Tasks:**
- [ ] Implement `Address` with checksum (EIP-55)
- [ ] Implement `U256` with overflow-safe math
- [ ] RLP encode/decode structs
- [ ] Sign transactions with `k256`
- [ ] Recover signer from signature (ecrecover)
- [ ] Parse raw transaction bytes

---

### 🔧 Project 2: Mini EVM

Build a simplified EVM interpreter:

```
mini-evm/
├── src/
│   ├── lib.rs
│   ├── opcode.rs       # All EVM opcodes enum
│   ├── stack.rs        # 1024-item U256 stack
│   ├── memory.rs       # Expandable byte memory
│   ├── storage.rs      # Key-value storage (H256 -> H256)
│   ├── context.rs      # Call context, gas, etc.
│   ├── interpreter.rs  # Main execution loop
│   └── opcodes/
│       ├── arithmetic.rs   # ADD, MUL, SUB, DIV, MOD
│       ├── comparison.rs   # LT, GT, EQ, ISZERO
│       ├── bitwise.rs      # AND, OR, XOR, NOT, SHL, SHR
│       ├── memory.rs       # MLOAD, MSTORE, MSTORE8
│       ├── storage.rs      # SLOAD, SSTORE
│       ├── control.rs      # JUMP, JUMPI, PC, STOP
│       └── environment.rs  # ADDRESS, CALLER, CALLVALUE
├── Cargo.toml
└── tests/
    └── bytecode_tests.rs   # Test against real bytecode
```

**Tasks:**
- [ ] Implement stack with push/pop/swap/dup
- [ ] Implement memory with dynamic expansion + gas
- [ ] Basic arithmetic opcodes (ADD, MUL, SUB, DIV)
- [ ] Comparison opcodes (LT, GT, EQ)
- [ ] Memory opcodes (MLOAD, MSTORE)
- [ ] Storage opcodes (SLOAD, SSTORE)
- [ ] Control flow (JUMP, JUMPI, JUMPDEST)
- [ ] Execute simple bytecode (e.g., 1+2)
- [ ] Gas metering per opcode
- [ ] CALL/DELEGATECALL/STATICCALL (advanced)

---

### 🔧 Project 3: Merkle Patricia Trie

Implement Ethereum's state trie:

```
mpt-trie/
├── src/
│   ├── lib.rs
│   ├── nibbles.rs      # Path as nibbles (half-bytes)
│   ├── node.rs         # Branch, Extension, Leaf nodes
│   ├── trie.rs         # Insert, get, delete, root hash
│   ├── proof.rs        # Generate/verify Merkle proofs
│   └── db.rs           # Backend storage trait
├── Cargo.toml
└── tests/
```

**Tasks:**
- [ ] Nibble path encoding (HP encoding)
- [ ] Node types: Leaf, Extension, Branch
- [ ] RLP encoding of nodes
- [ ] Insert key-value pairs
- [ ] Compute state root hash
- [ ] Generate Merkle proofs
- [ ] Verify proofs (critical for L2!)

---

### 🔧 Project 4: P2P Networking (DevP2P)

Build Ethereum networking layer:

```
devp2p/
├── src/
│   ├── lib.rs
│   ├── rlpx.rs         # RLPx encrypted transport
│   ├── ecies.rs        # ECIES encryption
│   ├── discovery.rs    # Node discovery (discv4/v5)
│   ├── eth_protocol.rs # eth/66, eth/67 wire protocol
│   └── peer.rs         # Peer connection management
├── Cargo.toml
```

**Tasks:**
- [ ] ECIES encryption/decryption
- [ ] RLPx handshake (ECDH key exchange)
- [ ] Frame encryption with AES-CTR
- [ ] Implement `eth` wire protocol messages
- [ ] Connect to real Ethereum nodes
- [ ] Sync block headers

---

### 🔧 Project 5: L2 Sequencer Prototype

Build a simple rollup sequencer:

```
mini-rollup/
├── src/
│   ├── lib.rs
│   ├── batch.rs        # Transaction batching
│   ├── state.rs        # State commitment (Merkle root)
│   ├── sequencer.rs    # Order transactions
│   ├── prover.rs       # Generate state diff proofs
│   ├── bridge.rs       # L1 <-> L2 messaging
│   └── api.rs          # JSON-RPC interface
├── contracts/          # Solidity L1 contracts
│   ├── Rollup.sol      # State root submission
│   └── Bridge.sol      # Deposit/withdraw
├── Cargo.toml
```

**Tasks:**
- [ ] Accept transactions via JSON-RPC
- [ ] Batch transactions (compress with zstd)
- [ ] Execute against mini-EVM
- [ ] Compute new state root
- [ ] Submit state root to L1 (mock or testnet)
- [ ] Implement fraud proof logic (optimistic)
- [ ] OR: Generate validity proof (ZK approach)

---

### 🔧 Project 6: Block Builder / MEV

Understand block construction:

```
block-builder/
├── src/
│   ├── lib.rs
│   ├── mempool.rs      # Transaction pool
│   ├── simulator.rs    # Simulate tx execution
│   ├── ordering.rs     # Order by profit (gas * priority)
│   ├── bundle.rs       # MEV bundle handling
│   └── builder.rs      # Construct optimal block
├── Cargo.toml
```

**Tasks:**
- [ ] Simulate transactions against state
- [ ] Calculate effective gas price
- [ ] Order transactions by profitability
- [ ] Handle MEV bundles (atomic sequences)
- [ ] Build blocks within gas limit
- [ ] Connect to Flashbots relay (mev-boost)

---

### 📖 Essential Rust Blockchain Resources

| Resource | What You Learn |
|----------|----------------|
| [Reth source code](https://github.com/paradigmxyz/reth) | Production EVM, networking |
| [revm](https://github.com/bluealloy/revm) | Fast EVM implementation |
| [alloy-rs](https://github.com/alloy-rs/alloy) | Ethereum primitives |
| [ethers-rs](https://github.com/gakonst/ethers-rs) | Ethereum interactions |
| [lighthouse](https://github.com/sigp/lighthouse) | Consensus client |
| [Paradigm's articles](https://www.paradigm.xyz/writing) | Deep technical posts |

---

### 🗓️ Extended Learning Timeline

```
Months 1-2: Solidity mastery (Phase 1 & 2)
    ↓
Month 3: Rust fundamentals + async
    ↓
Month 4: Project 1 (Primitives) + Project 2 (Mini EVM)
    ↓
Month 5: Project 3 (MPT) + read Reth/revm source
    ↓
Month 6: Project 4 (P2P) or Project 5 (Rollup)
    ↓
Month 7+: Contribute to Reth, build real L2, or MEV
```

---

### 🎯 Milestone Checkpoints

| Milestone | Proof of Completion |
|-----------|---------------------|
| **EVM Basics** | Execute `0x6001600101` (PUSH1 1 PUSH1 1 ADD) returns 2 |
| **Transactions** | Sign & broadcast tx to Sepolia from your Rust code |
| **State Trie** | Compute same state root as Geth for test vectors |
| **Networking** | Connect to mainnet peer, receive block headers |
| **Rollup** | Submit batch to L1 testnet, verify state transition |

---

### 💡 Pro Tips

1. **Start with `revm`** - Don't build EVM from scratch initially. Study revm's architecture, then rebuild parts.

2. **Use `alloy-rs`** - Modern replacement for ethers-rs. Has excellent primitives.

3. **Test against real data** - Use Ethereum test vectors from `ethereum/tests` repo.

4. **Read EIPs obsessively** - Every feature is an EIP. Understand EIP-1559, EIP-4844, EIP-4337.

5. **Join communities**:
   - Paradigm Discord (Reth)
   - Ethereum R&D Discord
   - Rust Ethereum Telegram

---

Happy building! 🚀
