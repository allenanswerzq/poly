#!/bin/bash
#
# Quick Demo: Launch 3 nodes, submit transactions, show results
#

set -e

cd "$(dirname "$0")/.."
BINARY="./target/debug/mini-eth"

# Build if needed
if [ ! -f "$BINARY" ]; then
    echo "Building mini-eth..."
    cargo build --bin mini-eth
fi

# Cleanup
cleanup() {
    echo -e "\n🛑 Stopping nodes..."
    pkill -f "mini-eth --name" 2>/dev/null || true
    rm -rf /tmp/mini-eth-demo
}
trap cleanup EXIT

# Create temp dir
rm -rf /tmp/mini-eth-demo
mkdir -p /tmp/mini-eth-demo/{node1,node2,node3}

echo "╔══════════════════════════════════════════════╗"
echo "║       Mini-ETH Quick Demo (3 nodes)          ║"
echo "╚══════════════════════════════════════════════╝"

# Start node 1 (miner)
echo -e "\n🚀 Starting Node 1 (Miner) on port 30303..."
$BINARY --name node1 --port 30303 --rpc-port 8545 --mine \
    --coinbase 0x0000000000000000000000000000000000000001 \
    --data-dir /tmp/mini-eth-demo/node1 &
sleep 1

# Start node 2
echo "🚀 Starting Node 2 on port 30304..."
$BINARY --name node2 --port 30304 --rpc-port 8546 \
    --bootnode 127.0.0.1:30303 \
    --data-dir /tmp/mini-eth-demo/node2 &
sleep 1

# Start node 3
echo "🚀 Starting Node 3 on port 30305..."
$BINARY --name node3 --port 30305 --rpc-port 8547 \
    --bootnode 127.0.0.1:30303 \
    --data-dir /tmp/mini-eth-demo/node3 &
sleep 2

echo -e "\n✅ All 3 nodes running!"
echo "   Node 1: http://127.0.0.1:8545 (miner)"
echo "   Node 2: http://127.0.0.1:8546"
echo "   Node 3: http://127.0.0.1:8547"

# Test RPC calls
echo -e "\n📡 Testing RPC..."

# Get block number
echo "Block number (node 1):"
curl -s -X POST -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
    http://127.0.0.1:8545 2>/dev/null || echo "RPC not available yet"

echo -e "\n\n💡 Try these commands:"
echo "   curl -X POST -H 'Content-Type: application/json' \\"
echo "     -d '{\"jsonrpc\":\"2.0\",\"method\":\"eth_blockNumber\",\"params\":[],\"id\":1}' \\"
echo "     http://127.0.0.1:8545"
echo ""
echo "   curl -X POST -H 'Content-Type: application/json' \\"
echo "     -d '{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"0x0000000000000000000000000000000000000100\",\"latest\"],\"id\":1}' \\"
echo "     http://127.0.0.1:8545"

echo -e "\n⏳ Nodes running... Press Ctrl+C to stop"
wait
