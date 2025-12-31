// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/02_StorageLayout.sol";

/**
 * EXERCISE 2: EXPLORE STORAGE LAYOUT
 *
 * Run with: forge test --match-contract StorageTest -vvvv
 */
contract StorageTest is Test {
    StorageLayout public store;

    function setUp() public {
        store = new StorageLayout();
    }

    function testReadBasicSlots() public view {
        console.log("=== Basic Storage Slots ===");

        // Slot 0: value1 (111)
        bytes32 slot0 = store.readSlot(0);
        console.log("Slot 0 (value1):", uint256(slot0));

        // Slot 1: value2 (222)
        bytes32 slot1 = store.readSlot(1);
        console.log("Slot 1 (value2):", uint256(slot1));

        // Slot 2: Packed values (smallValue1, smallValue2, smallValue3)
        bytes32 slot2 = store.readSlot(2);
        console.log("Slot 2 (packed):", vm.toString(slot2));
        console.log("  - Contains: 333, 444, 555 packed together!");

        // Slot 3: bool + uint8
        bytes32 slot3 = store.readSlot(3);
        console.log("Slot 3 (bool+uint8):", vm.toString(slot3));

        // Slot 4: owner address
        bytes32 slot4 = store.readSlot(4);
        console.log("Slot 4 (owner):", vm.toString(slot4));
    }

    function testReadPackedValues() public view {
        console.log("\n=== Unpacking Slot 2 ===");

        bytes32 slot2 = store.readSlot(2);

        // Values are packed right-to-left (little endian)
        // smallValue1 (uint128) is in the lower 16 bytes
        // smallValue2 (uint64) is in the next 8 bytes
        // smallValue3 (uint64) is in the next 8 bytes

        uint256 raw = uint256(slot2);

        uint128 sv1 = uint128(raw);                    // Lower 128 bits
        uint64 sv2 = uint64(raw >> 128);               // Next 64 bits
        uint64 sv3 = uint64(raw >> 192);               // Next 64 bits

        console.log("smallValue1:", sv1);  // 333
        console.log("smallValue2:", sv2);  // 444
        console.log("smallValue3:", sv3);  // 555

        assertEq(sv1, 333);
        assertEq(sv2, 444);
        assertEq(sv3, 555);
    }

    function testMappingStorage() public view {
        console.log("\n=== Mapping Storage ===");

        // Mapping is at slot 6
        // Data for key is at keccak256(key . slot)

        address key = address(this);
        bytes32 dataSlot = store.readMappingSlot(key, 6);
        console.log("Storage slot for balances[this]:", vm.toString(dataSlot));

        // Read the actual value
        bytes32 value = store.readSlot(uint256(dataSlot));
        console.log("Value at that slot:", uint256(value));

        // Should match balances[address(this)]
        console.log("balances[this]:", store.balances(address(this)));
    }

    function testDynamicArrayStorage() public view {
        console.log("\n=== Dynamic Array Storage ===");

        // dynamicArray is at slot 8
        // Slot 8 stores the LENGTH
        bytes32 lengthSlot = store.readSlot(8);
        console.log("Array length (slot 8):", uint256(lengthSlot));

        // Elements start at keccak256(8)
        bytes32 element0Slot = store.readArrayElementSlot(8, 0);
        bytes32 element1Slot = store.readArrayElementSlot(8, 1);
        bytes32 element2Slot = store.readArrayElementSlot(8, 2);

        console.log("Element 0 slot:", vm.toString(element0Slot));
        console.log("Element 0 value:", uint256(store.readSlot(uint256(element0Slot))));
        console.log("Element 1 value:", uint256(store.readSlot(uint256(element1Slot))));
        console.log("Element 2 value:", uint256(store.readSlot(uint256(element2Slot))));
    }

    function testWriteToArbitrarySlot() public {
        console.log("\n=== Arbitrary Storage Write ===");

        // We can write to ANY slot!
        // This is how storage collision attacks work

        console.log("value1 before:", store.value1());

        // Overwrite slot 0 (value1)
        store.writeSlot(0, bytes32(uint256(99999)));

        console.log("value1 after:", store.value1());
        assertEq(store.value1(), 99999);
    }

    function testGasCostComparison() public {
        console.log("\n=== Gas Cost: Packed vs Unpacked ===");

        // Writing to packed variables is cheaper (single SSTORE)

        uint256 gasBefore = gasleft();
        store.writeSlot(2, bytes32(uint256(123456))); // One SSTORE
        uint256 gasAfter = gasleft();

        console.log("Gas for 1 SSTORE:", gasBefore - gasAfter);

        // Compare: 3 separate writes would cost 3x SSTORE
    }
}
