// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * EXERCISE 2: STORAGE LAYOUT
 *
 * Learn how Solidity stores variables in the EVM.
 * Each storage slot is 32 bytes.
 */

contract StorageLayout {
    // ============================================
    // BASIC STORAGE
    // ============================================

    // Slot 0: Full 32 bytes for uint256
    uint256 public value1 = 111;

    // Slot 1: Full 32 bytes
    uint256 public value2 = 222;

    // Slot 2: Packed! (16 + 8 + 8 = 32 bytes)
    uint128 public smallValue1 = 333;  // 16 bytes
    uint64 public smallValue2 = 444;   // 8 bytes
    uint64 public smallValue3 = 555;   // 8 bytes

    // Slot 3: bool + uint8 = only 2 bytes used, 30 wasted!
    bool public flag = true;           // 1 byte
    uint8 public tinyValue = 99;       // 1 byte

    // Slot 4: address = 20 bytes
    address public owner;              // 20 bytes

    // Slot 5: This starts a new slot (can't pack with address above)
    uint256 public value3 = 666;

    // ============================================
    // MAPPINGS - Tricky!
    // ============================================

    // Slot 6: Empty! Mapping keys aren't stored here
    mapping(address => uint256) public balances;
    // Actual data at: keccak256(abi.encode(key, 6))

    // Slot 7: Also empty
    mapping(uint256 => mapping(address => uint256)) public nestedMap;
    // Data at: keccak256(abi.encode(innerKey, keccak256(abi.encode(outerKey, 7))))

    // ============================================
    // DYNAMIC ARRAYS - Also tricky!
    // ============================================

    // Slot 8: Stores LENGTH only
    uint256[] public dynamicArray;
    // Elements at: keccak256(abi.encode(8)) + index

    // Slot 9: Stores LENGTH
    bytes public dynamicBytes;
    // Short bytes (< 32) stored inline
    // Long bytes: slot stores (length * 2 + 1), data at keccak256(9)

    // ============================================
    // FIXED ARRAYS
    // ============================================

    // Slots 10, 11, 12: Stored contiguously
    uint256[3] public fixedArray;

    constructor() {
        owner = msg.sender;

        // Initialize some data
        balances[msg.sender] = 1000;
        balances[address(0x1)] = 2000;

        dynamicArray.push(100);
        dynamicArray.push(200);
        dynamicArray.push(300);

        fixedArray[0] = 10;
        fixedArray[1] = 20;
        fixedArray[2] = 30;
    }

    // ============================================
    // HELPER: Read raw storage slots
    // ============================================

    function readSlot(uint256 slot) public view returns (bytes32) {
        bytes32 value;
        assembly {
            value := sload(slot)
        }
        return value;
    }

    function readMappingSlot(address key, uint256 mappingSlot) public pure returns (bytes32) {
        // This is how Solidity computes mapping storage location
        return keccak256(abi.encode(key, mappingSlot));
    }

    function readArrayElementSlot(uint256 arraySlot, uint256 index) public pure returns (bytes32) {
        // Array elements start at keccak256(slot)
        bytes32 startSlot = keccak256(abi.encode(arraySlot));
        return bytes32(uint256(startSlot) + index);
    }

    // ============================================
    // DANGEROUS: Write to arbitrary storage!
    // ============================================

    function writeSlot(uint256 slot, bytes32 value) public {
        require(msg.sender == owner, "Only owner");
        assembly {
            sstore(slot, value)
        }
    }
}

/**
 * EXERCISE 2B: STORAGE COLLISION IN PROXIES
 *
 * A common bug in upgradeable contracts!
 */
contract ProxyStorageCollision {
    // Proxy's storage
    address public implementation;  // Slot 0
    address public admin;           // Slot 1

    // If implementation contract also uses slots 0 and 1,
    // they will COLLIDE!
}

contract ImplementationV1 {
    // These WILL collide with proxy's storage!
    uint256 public value;           // Slot 0 - COLLISION with implementation!
    address public owner;           // Slot 1 - COLLISION with admin!
}

// FIX: Use EIP-1967 storage slots (random locations)
contract ImplementationV1Fixed {
    // EIP-1967: keccak256("eip1967.proxy.implementation") - 1
    // This is a random-looking slot that won't collide
    bytes32 constant IMPLEMENTATION_SLOT =
        0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc;

    // Start implementation storage at slot 0 safely
    // (proxy uses random slots above)
    uint256 public value;
    address public owner;
}
