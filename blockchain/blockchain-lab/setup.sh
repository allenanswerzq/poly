#!/bin/bash

# ==========================================
# BLOCKCHAIN LAB SETUP SCRIPT
# ==========================================

echo "=== Blockchain Lab Setup ==="
echo ""

# Check if Foundry is installed
if ! command -v forge &> /dev/null; then
    echo "Installing Foundry..."
    curl -L https://foundry.paradigm.xyz | bash
    source ~/.bashrc
    foundryup
else
    echo "Foundry already installed: $(forge --version)"
fi

# Navigate to project
cd "$(dirname "$0")"

# Install dependencies
echo ""
echo "Installing dependencies..."
forge install foundry-rs/forge-std --no-commit 2>/dev/null || echo "forge-std already installed"

echo ""
echo "=== Setup Complete! ==="
echo ""
echo "Run these commands to start learning:"
echo ""
echo "  # Run all tests with verbose output"
echo "  forge test -vvvv"
echo ""
echo "  # Run specific exercise"
echo "  forge test --match-contract ReentrancyTest -vvvv"
echo "  forge test --match-contract StorageTest -vvvv"
echo "  forge test --match-contract FlashLoanTest -vvvv"
echo "  forge test --match-contract GasOptimizationTest --gas-report"
echo "  forge test --match-contract AccessControlTest -vvvv"
echo ""
echo "  # See gas usage for all contracts"
echo "  forge test --gas-report"
echo ""
echo "Happy hacking! 🔐"
