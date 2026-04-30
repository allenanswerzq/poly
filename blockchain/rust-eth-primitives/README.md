# Ethereum Primitives Library (Rust)

Low-level implementation of core Ethereum data structures in Rust.

## 📚 What You'll Learn

By building this library, you'll understand:

1. **Keccak256 Hashing** - Ethereum's hash function
2. **Addresses** - EIP-55 checksummed addresses
3. **U256 Arithmetic** - 256-bit math operations
4. **RLP Encoding** - Ethereum's serialization format
5. **ECDSA Signatures** - secp256k1 curve operations
6. **Transaction Types** - Legacy, EIP-1559, EIP-4844

## 🚀 Quick Start

```bash
# Build the library
cargo build

# Run tests
cargo test

# Run the demo
cargo run
```

## 📁 Project Structure

```
src/
├── lib.rs          # Library entry point
├── main.rs         # Demo binary
├── error.rs        # Custom error types
├── hash.rs         # H256, Keccak256
├── address.rs      # 20-byte Ethereum address
├── uint.rs         # U256 arithmetic
├── rlp.rs          # RLP encode/decode
├── signature.rs    # ECDSA secp256k1, recovery
└── transaction.rs  # Legacy, EIP-1559, EIP-4844
```

## ✅ Tasks Checklist

### Phase 1: Core Types
- [x] Implement `H256` with hex conversion
- [x] Implement `keccak256` hashing
- [x] Implement `Address` with EIP-55 checksum
- [x] Implement `U256` with overflow-safe math
- [x] Implement comparison operators for `U256`
- [x] Implement bitwise operators for `U256`
- [x] Implement shift operators for `U256`

### Phase 2: Encoding
- [x] RLP encode single bytes
- [x] RLP encode short strings (≤55 bytes)
- [x] RLP encode long strings (>55 bytes)
- [x] RLP encode lists
- [x] RLP decode bytes
- [x] RLP decode lists

### Phase 3: Signatures
- [x] Sign message hash with private key
- [x] Recover signer address from signature (ecrecover)
- [x] Support recovery id formats (0/1, 27/28, EIP-155)
- [x] Create EIP-712 domain separator
- [x] Hash typed data (EIP-712)

### Phase 4: Transactions
- [x] Create legacy transactions
- [x] Create EIP-1559 transactions
- [x] Sign transactions
- [x] Encode signed transactions for broadcast
- [x] Recover sender from signed transaction
- [ ] Parse raw transaction bytes
- [ ] Add access list support (EIP-2930)
- [ ] Add blob transaction support (EIP-4844)

## 🧪 Test Vectors

The library includes tests based on official Ethereum test vectors:

```bash
# Run all tests with output
cargo test -- --nocapture

# Run specific module tests
cargo test hash::tests
cargo test address::tests
cargo test uint::tests
cargo test rlp::tests
cargo test signature::tests
cargo test transaction::tests
```

## 📖 Key Concepts

### Keccak256 vs SHA3

Ethereum uses **Keccak256**, not standard SHA3-256. They have the same algorithm but different padding. This library uses the correct Keccak variant.

### EIP-55 Checksum

Addresses are case-insensitive, but EIP-55 uses mixed case to encode a checksum:
```
0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed
  ^lowercase  ^UPPERCASE based on hash
```

### U256 in EVM

The EVM is a 256-bit machine. All arithmetic is modular (wraps on overflow) and division by zero returns 0.

### RLP Encoding

RLP (Recursive Length Prefix) is Ethereum's serialization format:
- Single byte < 0x80: itself
- String ≤55 bytes: 0x80+len, then bytes
- String >55 bytes: 0xb7+len_len, len, bytes
- List ≤55 bytes total: 0xc0+len, items
- List >55 bytes total: 0xf7+len_len, len, items

### Transaction Signing

1. Build the transaction struct
2. RLP encode (with chain_id for EIP-155)
3. Hash with Keccak256
4. Sign with ECDSA
5. RLP encode again (with signature)

## 🔗 Next Steps

After completing this library:

1. **Project 2: Mini EVM** - Build an interpreter that executes bytecode
2. **Project 3: Merkle Patricia Trie** - Implement Ethereum's state storage
3. **Study revm** - Compare with production EVM implementation

## 📚 Resources

- [EVM Codes](https://evm.codes) - Opcode reference
- [Ethereum Yellow Paper](https://ethereum.github.io/yellowpaper/paper.pdf)
- [EIP-155](https://eips.ethereum.org/EIPS/eip-155) - Replay protection
- [EIP-1559](https://eips.ethereum.org/EIPS/eip-1559) - Fee market
- [EIP-712](https://eips.ethereum.org/EIPS/eip-712) - Typed data signing
- [RLP Spec](https://ethereum.org/en/developers/docs/data-structures-and-encoding/rlp/)
