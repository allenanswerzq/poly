# Blockchain Hands-On Lab 🔬

Practical exercises to learn blockchain development by doing.

## Setup

```bash
# Install Foundry (Solidity toolkit)
curl -L https://foundry.paradigm.xyz | bash
foundryup

# Initialize project
cd blockchain-lab
forge init --no-commit
```

## Exercises

1. **EVM Basics** - Understand opcodes and gas
2. **Reentrancy Attack** - Exploit and fix vulnerable contracts
3. **Flash Loan Attack** - Price manipulation demo
4. **Storage Layout** - See how Solidity stores data
5. **Gas Optimization** - Make contracts cheaper

## Running Tests

```bash
# Run all tests
forge test -vvvv

# Run specific test
forge test --match-test testReentrancyAttack -vvvv

# See gas usage
forge test --gas-report
```
