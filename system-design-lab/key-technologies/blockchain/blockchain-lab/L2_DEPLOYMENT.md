# L2 Deployment Guide 🚀

Hands-on guide to deploy contracts on real L2 testnets.

## Setup

### 1. Get Testnet ETH

| Network | Faucet |
|---------|--------|
| Sepolia (L1) | https://sepoliafaucet.com |
| Arbitrum Sepolia | https://faucet.arbitrum.io |
| Optimism Sepolia | https://app.optimism.io/faucet |
| Base Sepolia | https://www.coinbase.com/faucets/base-ethereum-goerli-faucet |

### 2. Configure Foundry

Add to `foundry.toml`:

```toml
[rpc_endpoints]
sepolia = "https://rpc.sepolia.org"
arbitrum_sepolia = "https://sepolia-rollup.arbitrum.io/rpc"
optimism_sepolia = "https://sepolia.optimism.io"
base_sepolia = "https://sepolia.base.org"
```

### 3. Set Private Key

```bash
export PRIVATE_KEY=0x...your_testnet_private_key...
```

## Deploy Commands

### Deploy to Sepolia (L1)
```bash
forge create --rpc-url sepolia \
  --private-key $PRIVATE_KEY \
  src/01_Reentrancy.sol:SecureBank
```

### Deploy to Arbitrum Sepolia (L2)
```bash
forge create --rpc-url arbitrum_sepolia \
  --private-key $PRIVATE_KEY \
  src/01_Reentrancy.sol:SecureBank
```

### Deploy to Optimism Sepolia (L2)
```bash
forge create --rpc-url optimism_sepolia \
  --private-key $PRIVATE_KEY \
  src/01_Reentrancy.sol:SecureBank
```

### Deploy to Base Sepolia (L2)
```bash
forge create --rpc-url base_sepolia \
  --private-key $PRIVATE_KEY \
  src/01_Reentrancy.sol:SecureBank
```

## Compare Gas Costs

Create a script to compare gas costs:

```bash
# Run this after deploying to see gas differences
forge script script/CompareGas.s.sol --rpc-url sepolia
forge script script/CompareGas.s.sol --rpc-url arbitrum_sepolia
```

## What to Notice

1. **Deployment cost**: L2 is 10-100x cheaper
2. **Transaction speed**: L2 confirms in seconds
3. **Same code**: Your Solidity works identically
4. **Block explorer**: Each L2 has its own explorer

## Block Explorers

| Network | Explorer |
|---------|----------|
| Sepolia | https://sepolia.etherscan.io |
| Arbitrum Sepolia | https://sepolia.arbiscan.io |
| Optimism Sepolia | https://sepolia-optimism.etherscan.io |
| Base Sepolia | https://sepolia.basescan.org |

## Exercise: Deploy AMM to L2

1. Deploy TokenA and TokenB
2. Deploy SimpleAMM with both tokens
3. Add liquidity
4. Perform swaps
5. Compare total gas cost to doing same on L1

```bash
# Example deployment script
forge script script/DeployAMM.s.sol \
  --rpc-url arbitrum_sepolia \
  --private-key $PRIVATE_KEY \
  --broadcast
```
