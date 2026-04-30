#!/bin/bash
#
# Mini-ETH Network Launcher
#
# This script launches multiple mini-eth nodes and demonstrates
# transaction submission using the client.
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Configuration
NUM_NODES=${1:-3}
BASE_P2P_PORT=30303
BASE_RPC_PORT=8545
DATA_DIR="/tmp/mini-eth"
BINARY_DIR="$(cd "$(dirname "$0")/.." && pwd)/target/debug"

# Ensure binaries exist
if [ ! -f "$BINARY_DIR/mini-eth" ]; then
    echo -e "${YELLOW}Building mini-eth...${NC}"
    cd "$(dirname "$0")/.."
    cargo build --bin mini-eth --bin eth-client
fi

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Shutting down nodes...${NC}"
    for pid in "${NODE_PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done
    rm -rf "$DATA_DIR"
    echo -e "${GREEN}Cleanup complete${NC}"
}

trap cleanup EXIT INT TERM

# Print banner
echo -e "${CYAN}"
echo "╔════════════════════════════════════════════════════════════╗"
echo "║                                                            ║"
echo "║              🔷 Mini-ETH Network Launcher                  ║"
echo "║                                                            ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

# Create data directory
rm -rf "$DATA_DIR"
mkdir -p "$DATA_DIR"

# Array to store PIDs
declare -a NODE_PIDS

# Launch nodes
echo -e "${BLUE}📦 Launching $NUM_NODES nodes...${NC}\n"

for i in $(seq 1 $NUM_NODES); do
    P2P_PORT=$((BASE_P2P_PORT + i - 1))
    RPC_PORT=$((BASE_RPC_PORT + i - 1))
    NODE_DATA="$DATA_DIR/node$i"
    mkdir -p "$NODE_DATA"

    # First node is a miner, others connect to it
    if [ $i -eq 1 ]; then
        echo -e "${GREEN}🚀 Starting Node $i (Miner)${NC}"
        echo -e "   P2P: $P2P_PORT, RPC: $RPC_PORT"

        "$BINARY_DIR/mini-eth" \
            --name "node-$i-miner" \
            --data-dir "$NODE_DATA" \
            --port $P2P_PORT \
            --rpc-port $RPC_PORT \
            --mine \
            --coinbase "0x0000000000000000000000000000000000000001" \
            > "$NODE_DATA/node.log" 2>&1 &
        NODE_PIDS+=($!)
        MINER_PORT=$P2P_PORT
    else
        echo -e "${GREEN}🚀 Starting Node $i${NC}"
        echo -e "   P2P: $P2P_PORT, RPC: $RPC_PORT, Bootnode: 127.0.0.1:$MINER_PORT"

        "$BINARY_DIR/mini-eth" \
            --name "node-$i" \
            --data-dir "$NODE_DATA" \
            --port $P2P_PORT \
            --rpc-port $RPC_PORT \
            --bootnode "127.0.0.1:$MINER_PORT" \
            > "$NODE_DATA/node.log" 2>&1 &
        NODE_PIDS+=($!)
    fi

    sleep 0.5
done

echo -e "\n${YELLOW}⏳ Waiting for nodes to initialize...${NC}"
sleep 2

# Check node status
echo -e "\n${BLUE}📊 Node Status:${NC}"
for i in $(seq 1 $NUM_NODES); do
    RPC_PORT=$((BASE_RPC_PORT + i - 1))
    NODE_DATA="$DATA_DIR/node$i"

    if kill -0 "${NODE_PIDS[$((i-1))]}" 2>/dev/null; then
        echo -e "   ${GREEN}✓${NC} Node $i running (PID: ${NODE_PIDS[$((i-1))]})"
    else
        echo -e "   ${RED}✗${NC} Node $i failed to start"
        echo "   Log: $(tail -5 "$NODE_DATA/node.log" 2>/dev/null || echo 'No log')"
    fi
done

# Function to make RPC call
rpc_call() {
    local port=$1
    local method=$2
    local params=$3

    curl -s -X POST \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}" \
        "http://127.0.0.1:$port" 2>/dev/null || echo '{"error":"connection failed"}'
}

# Interactive menu
show_menu() {
    echo -e "\n${CYAN}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}                    Available Commands                      ${NC}"
    echo -e "${CYAN}═══════════════════════════════════════════════════════════${NC}"
    echo -e "  ${GREEN}1${NC}) Get block number (all nodes)"
    echo -e "  ${GREEN}2${NC}) Get balance of address"
    echo -e "  ${GREEN}3${NC}) Submit transfer transaction"
    echo -e "  ${GREEN}4${NC}) View node logs"
    echo -e "  ${GREEN}5${NC}) Check peer connections"
    echo -e "  ${GREEN}6${NC}) Deploy simple contract"
    echo -e "  ${GREEN}7${NC}) Run automated demo"
    echo -e "  ${GREEN}q${NC}) Quit"
    echo -e "${CYAN}═══════════════════════════════════════════════════════════${NC}"
}

# Get block numbers from all nodes
get_block_numbers() {
    echo -e "\n${BLUE}📦 Block Numbers:${NC}"
    for i in $(seq 1 $NUM_NODES); do
        RPC_PORT=$((BASE_RPC_PORT + i - 1))
        result=$(rpc_call $RPC_PORT "eth_blockNumber" "[]")
        block=$(echo "$result" | grep -o '"result":"[^"]*"' | cut -d'"' -f4 || echo "error")
        echo -e "   Node $i (port $RPC_PORT): $block"
    done
}

# Get balance
get_balance() {
    read -p "Enter address (0x...): " address
    read -p "Enter node number (1-$NUM_NODES): " node_num

    if [ "$node_num" -ge 1 ] && [ "$node_num" -le "$NUM_NODES" ]; then
        RPC_PORT=$((BASE_RPC_PORT + node_num - 1))
        result=$(rpc_call $RPC_PORT "eth_getBalance" "[\"$address\", \"latest\"]")
        echo -e "\n${GREEN}Balance: $result${NC}"
    else
        echo -e "${RED}Invalid node number${NC}"
    fi
}

# Submit transaction
submit_transaction() {
    echo -e "\n${YELLOW}Submit Transfer Transaction${NC}"
    read -p "From address (0x...): " from_addr
    read -p "To address (0x...): " to_addr
    read -p "Value in wei (e.g., 1000000000000000000 for 1 ETH): " value
    read -p "Submit to node (1-$NUM_NODES): " node_num

    if [ "$node_num" -ge 1 ] && [ "$node_num" -le "$NUM_NODES" ]; then
        RPC_PORT=$((BASE_RPC_PORT + node_num - 1))

        # Create transaction object
        tx_params="[{\"from\":\"$from_addr\",\"to\":\"$to_addr\",\"value\":\"0x$(printf '%x' $value)\",\"gas\":\"0x5208\",\"gasPrice\":\"0x3b9aca00\"}]"

        result=$(rpc_call $RPC_PORT "eth_sendTransaction" "$tx_params")
        echo -e "\n${GREEN}Transaction result: $result${NC}"
    else
        echo -e "${RED}Invalid node number${NC}"
    fi
}

# View logs
view_logs() {
    read -p "View logs for node (1-$NUM_NODES): " node_num

    if [ "$node_num" -ge 1 ] && [ "$node_num" -le "$NUM_NODES" ]; then
        NODE_DATA="$DATA_DIR/node$node_num"
        echo -e "\n${BLUE}Last 20 lines of Node $node_num log:${NC}"
        tail -20 "$NODE_DATA/node.log" 2>/dev/null || echo "No log file"
    else
        echo -e "${RED}Invalid node number${NC}"
    fi
}

# Check peers
check_peers() {
    echo -e "\n${BLUE}🔗 Peer Connections:${NC}"
    for i in $(seq 1 $NUM_NODES); do
        RPC_PORT=$((BASE_RPC_PORT + i - 1))
        result=$(rpc_call $RPC_PORT "net_peerCount" "[]")
        peers=$(echo "$result" | grep -o '"result":"[^"]*"' | cut -d'"' -f4 || echo "error")
        echo -e "   Node $i: $peers peers"
    done
}

# Deploy contract
deploy_contract() {
    echo -e "\n${YELLOW}Deploy Simple Storage Contract${NC}"
    read -p "From address (0x...): " from_addr
    read -p "Deploy to node (1-$NUM_NODES): " node_num

    # Simple storage contract bytecode (stores a number)
    # contract SimpleStorage { uint256 value; function set(uint256 v) { value = v; } function get() returns (uint256) { return value; } }
    bytecode="0x608060405234801561001057600080fd5b5060df8061001f6000396000f3fe6080604052348015600f57600080fd5b506004361060325760003560e01c806360fe47b11460375780636d4ce63c146049575b600080fd5b60476042366004608c565b600055565b005b60005460405190815260200160405180910390f35b600060208284031215606e57600080fd5b5035919050565b634e487b7160e01b600052604160045260246000fdfea264697066735822122012345678901234567890123456789012345678901234567890123456789012"

    if [ "$node_num" -ge 1 ] && [ "$node_num" -le "$NUM_NODES" ]; then
        RPC_PORT=$((BASE_RPC_PORT + node_num - 1))

        tx_params="[{\"from\":\"$from_addr\",\"data\":\"$bytecode\",\"gas\":\"0x100000\",\"gasPrice\":\"0x3b9aca00\"}]"

        result=$(rpc_call $RPC_PORT "eth_sendTransaction" "$tx_params")
        echo -e "\n${GREEN}Deployment result: $result${NC}"
    else
        echo -e "${RED}Invalid node number${NC}"
    fi
}

# Automated demo
run_demo() {
    echo -e "\n${CYAN}🎬 Running Automated Demo...${NC}"

    # Pre-funded test addresses
    ALICE="0x0000000000000000000000000000000000000100"
    BOB="0x0000000000000000000000000000000000000200"

    echo -e "\n${BLUE}Step 1: Check initial block numbers${NC}"
    get_block_numbers

    echo -e "\n${BLUE}Step 2: Check Alice's initial balance${NC}"
    result=$(rpc_call $BASE_RPC_PORT "eth_getBalance" "[\"$ALICE\", \"latest\"]")
    echo -e "   Alice: $result"

    echo -e "\n${BLUE}Step 3: Submit transfer from Alice to Bob${NC}"
    tx_params="[{\"from\":\"$ALICE\",\"to\":\"$BOB\",\"value\":\"0xde0b6b3a7640000\",\"gas\":\"0x5208\",\"gasPrice\":\"0x3b9aca00\"}]"
    result=$(rpc_call $BASE_RPC_PORT "eth_sendTransaction" "$tx_params")
    echo -e "   Transaction: $result"

    echo -e "\n${YELLOW}⏳ Waiting for block production (3 seconds)...${NC}"
    sleep 3

    echo -e "\n${BLUE}Step 4: Check updated block numbers${NC}"
    get_block_numbers

    echo -e "\n${BLUE}Step 5: Check updated balances${NC}"
    result=$(rpc_call $BASE_RPC_PORT "eth_getBalance" "[\"$ALICE\", \"latest\"]")
    echo -e "   Alice: $result"
    result=$(rpc_call $BASE_RPC_PORT "eth_getBalance" "[\"$BOB\", \"latest\"]")
    echo -e "   Bob: $result"

    echo -e "\n${GREEN}✅ Demo complete!${NC}"
}

# Main loop
echo -e "\n${GREEN}All nodes started successfully!${NC}"

while true; do
    show_menu
    read -p "Enter choice: " choice

    case $choice in
        1) get_block_numbers ;;
        2) get_balance ;;
        3) submit_transaction ;;
        4) view_logs ;;
        5) check_peers ;;
        6) deploy_contract ;;
        7) run_demo ;;
        q|Q)
            echo -e "\n${YELLOW}Shutting down...${NC}"
            break
            ;;
        *)
            echo -e "${RED}Invalid choice${NC}"
            ;;
    esac
done

echo -e "${GREEN}Goodbye!${NC}"
