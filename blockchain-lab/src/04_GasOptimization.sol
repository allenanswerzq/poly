// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * EXERCISE 4: GAS OPTIMIZATION
 *
 * Learn how to write gas-efficient smart contracts.
 * Compare gas costs between optimized and unoptimized versions.
 */

// ============================================
// UNOPTIMIZED CONTRACT
// ============================================
contract GasHog {
    // BAD: Variables not packed
    uint128 public value1;    // Slot 0
    uint256 public value2;    // Slot 1 (can't pack, value1 left alone)
    uint128 public value3;    // Slot 2 (wasted space!)
    bool public flag1;        // Slot 3
    uint256 public value4;    // Slot 4
    bool public flag2;        // Slot 5

    uint256[] public dynamicArray;
    mapping(address => uint256) public balances;

    // BAD: Reads storage multiple times
    function inefficientSum() external view returns (uint256) {
        return value1 + value2 + value3 + value4 + value1 + value2;
        // 6 SLOAD operations! (well, 4 unique, but still bad pattern)
    }

    // BAD: Loop with storage writes
    function inefficientLoop(uint256[] calldata values) external {
        for (uint256 i = 0; i < values.length; i++) {
            dynamicArray.push(values[i]);  // SSTORE in every iteration!
        }
    }

    // BAD: Uses memory when calldata would work
    function inefficientArray(uint256[] memory data) external pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < data.length; i++) {
            sum += data[i];
        }
        return sum;
    }

    // BAD: Unnecessary operations
    function inefficientTransfer(address to, uint256 amount) external {
        require(balances[msg.sender] >= amount, "Insufficient balance");
        require(to != address(0), "Invalid address");
        require(amount > 0, "Amount must be positive");

        balances[msg.sender] = balances[msg.sender] - amount;  // Reads twice
        balances[to] = balances[to] + amount;  // Reads twice
    }

    // BAD: Not using unchecked for safe math
    function inefficientCounter() external pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < 100; i++) {  // Overflow checks each iteration
            sum += i;
        }
        return sum;
    }

    function setValues(
        uint128 _v1,
        uint256 _v2,
        uint128 _v3,
        uint256 _v4,
        bool _f1,
        bool _f2
    ) external {
        value1 = _v1;  // SSTORE
        value2 = _v2;  // SSTORE
        value3 = _v3;  // SSTORE
        value4 = _v4;  // SSTORE
        flag1 = _f1;   // SSTORE
        flag2 = _f2;   // SSTORE
        // 6 SSTORE operations!
    }
}

// ============================================
// OPTIMIZED CONTRACT
// ============================================
contract GasOptimized {
    // GOOD: Variables are packed
    uint128 public value1;    // Slot 0 (16 bytes)
    uint128 public value3;    // Slot 0 (16 bytes) - PACKED!
    bool public flag1;        // Slot 1 (1 byte)
    bool public flag2;        // Slot 1 (1 byte) - PACKED!
    uint256 public value2;    // Slot 2 (32 bytes)
    uint256 public value4;    // Slot 3 (32 bytes)

    uint256[] public dynamicArray;
    mapping(address => uint256) public balances;

    // GOOD: Cache storage reads
    function efficientSum() external view returns (uint256) {
        uint256 v1 = value1;  // Cache in memory
        uint256 v2 = value2;
        return v1 + v2 + value3 + value4 + v1 + v2;  // 4 SLOAD total
    }

    // GOOD: Batch storage writes
    function efficientLoop(uint256[] calldata values) external {
        uint256 length = values.length;
        uint256 currentLength = dynamicArray.length;

        // Resize array once
        assembly {
            sstore(dynamicArray.slot, add(currentLength, length))
        }

        // Or use more readable approach with single SSTORE per item
        for (uint256 i = 0; i < length;) {
            dynamicArray.push(values[i]);
            unchecked { i++; }
        }
    }

    // GOOD: Use calldata for read-only external arrays
    function efficientArray(uint256[] calldata data) external pure returns (uint256) {
        uint256 sum = 0;
        uint256 length = data.length;
        for (uint256 i = 0; i < length;) {
            sum += data[i];
            unchecked { i++; }  // Safe: i < length
        }
        return sum;
    }

    // GOOD: Cache storage, use unchecked where safe
    function efficientTransfer(address to, uint256 amount) external {
        require(to != address(0) && amount > 0, "Invalid params");

        uint256 senderBalance = balances[msg.sender];  // Cache: 1 SLOAD
        require(senderBalance >= amount, "Insufficient");

        unchecked {
            // Safe: we checked senderBalance >= amount
            balances[msg.sender] = senderBalance - amount;
        }
        balances[to] += amount;  // Only 2 SLOAD, 2 SSTORE total
    }

    // GOOD: Use unchecked for loop counter
    function efficientCounter() external pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < 100;) {
            sum += i;
            unchecked { i++; }  // Save gas: no overflow check
        }
        return sum;
    }

    // GOOD: Pack writes to same slot
    function setPackedValues(uint128 _v1, uint128 _v3, bool _f1, bool _f2) external {
        // These share slots, so fewer SSTORE operations
        value1 = _v1;
        value3 = _v3;  // Same slot as value1
        flag1 = _f1;
        flag2 = _f2;   // Same slot as flag1
        // Only 2 SSTORE operations for 4 values!
    }

    function setLargeValues(uint256 _v2, uint256 _v4) external {
        value2 = _v2;
        value4 = _v4;
        // 2 SSTORE for 2 values
    }
}

// ============================================
// GAS TRICKS & PATTERNS
// ============================================
contract GasTricks {

    // TRICK 1: Short-circuit evaluation
    function shortCircuit(uint256 x, uint256 y) external pure returns (bool) {
        // If x < 10, y is never evaluated
        return x < 10 && y > expensiveCheck(y);
    }

    function expensiveCheck(uint256 y) internal pure returns (uint256) {
        // Simulates expensive operation
        for (uint256 i = 0; i < 100; i++) {
            y = y * 2 / 2;
        }
        return y;
    }

    // TRICK 2: != 0 is cheaper than > 0 for uints
    function checkNonZero(uint256 x) external pure returns (bool) {
        return x != 0;  // Slightly cheaper than x > 0
    }

    // TRICK 3: Use bytes32 instead of string when possible
    bytes32 public constantString = "Hello";  // Cheaper than string

    // TRICK 4: Make errors shorter
    error InsufficientBalance(uint256 available, uint256 required);
    // Custom errors are cheaper than require(... "long string message")

    function customError(uint256 amount) external pure {
        if (amount > 100) {
            revert InsufficientBalance(100, amount);
        }
    }

    // TRICK 5: Use immutable for constructor-set values
    address public immutable owner;
    uint256 public immutable deployTime;

    constructor() {
        owner = msg.sender;
        deployTime = block.timestamp;
        // These are embedded in bytecode, not storage!
    }

    // TRICK 6: Payable functions are slightly cheaper
    function cheaperFunction() external payable {
        // Saves ~20 gas (no msg.value check)
    }

    // TRICK 7: Use assembly for simple operations
    function assemblyAdd(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := add(a, b)
        }
    }
}
