// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/04_GasOptimization.sol";

/**
 * EXERCISE 4: GAS OPTIMIZATION COMPARISON
 *
 * Run with: forge test --match-contract GasOptimizationTest --gas-report
 */
contract GasOptimizationTest is Test {
    GasHog public hog;
    GasOptimized public optimized;
    GasTricks public tricks;

    function setUp() public {
        hog = new GasHog();
        optimized = new GasOptimized();
        tricks = new GasTricks();

        // Initialize some state
        hog.setValues(100, 200, 300, 400, true, false);
        optimized.setPackedValues(100, 300, true, false);
        optimized.setLargeValues(200, 400);
    }

    // ============================================
    // COMPARISON: Storage Packing
    // ============================================

    function testGas_SetValues_Unoptimized() public {
        uint256 gasBefore = gasleft();
        hog.setValues(1, 2, 3, 4, true, true);
        uint256 gasUsed = gasBefore - gasleft();
        console.log("Unoptimized setValues gas:", gasUsed);
    }

    function testGas_SetValues_Optimized() public {
        uint256 gasBefore = gasleft();
        optimized.setPackedValues(1, 3, true, true);
        optimized.setLargeValues(2, 4);
        uint256 gasUsed = gasBefore - gasleft();
        console.log("Optimized setValues gas:", gasUsed);
    }

    // ============================================
    // COMPARISON: Calldata vs Memory
    // ============================================

    function testGas_ArraySum_Memory() public {
        uint256[] memory data = new uint256[](100);
        for (uint256 i = 0; i < 100; i++) {
            data[i] = i;
        }

        uint256 gasBefore = gasleft();
        hog.inefficientArray(data);
        uint256 gasUsed = gasBefore - gasleft();
        console.log("Memory array sum gas:", gasUsed);
    }

    function testGas_ArraySum_Calldata() public {
        uint256[] memory data = new uint256[](100);
        for (uint256 i = 0; i < 100; i++) {
            data[i] = i;
        }

        uint256 gasBefore = gasleft();
        optimized.efficientArray(data);
        uint256 gasUsed = gasBefore - gasleft();
        console.log("Calldata array sum gas:", gasUsed);
    }

    // ============================================
    // COMPARISON: Unchecked Math
    // ============================================

    function testGas_Counter_Checked() public {
        uint256 gasBefore = gasleft();
        hog.inefficientCounter();
        uint256 gasUsed = gasBefore - gasleft();
        console.log("Checked counter gas:", gasUsed);
    }

    function testGas_Counter_Unchecked() public {
        uint256 gasBefore = gasleft();
        optimized.efficientCounter();
        uint256 gasUsed = gasBefore - gasleft();
        console.log("Unchecked counter gas:", gasUsed);
    }

    // ============================================
    // COMPARISON: Cached vs Uncached Storage
    // ============================================

    function testGas_Sum_Uncached() public view {
        uint256 gasBefore = gasleft();
        hog.inefficientSum();
        uint256 gasUsed = gasBefore - gasleft();
        console.log("Uncached sum gas:", gasUsed);
    }

    function testGas_Sum_Cached() public view {
        uint256 gasBefore = gasleft();
        optimized.efficientSum();
        uint256 gasUsed = gasBefore - gasleft();
        console.log("Cached sum gas:", gasUsed);
    }

    // ============================================
    // TEST: Gas Tricks
    // ============================================

    function testGas_Immutable() public view {
        uint256 gasBefore = gasleft();
        tricks.owner();
        tricks.deployTime();
        uint256 gasUsed = gasBefore - gasleft();
        console.log("Reading immutable vars gas:", gasUsed);
        // Compare: reading from storage would cost ~2100 gas each
    }

    function testGas_CustomError() public {
        console.log("\nCustom errors are cheaper than string messages:");
        console.log("revert InsufficientBalance(100, 200) < require(..., 'long string')");
    }

    // ============================================
    // SUMMARY
    // ============================================

    function testGasSummary() public pure {
        console.log("\n=== GAS OPTIMIZATION CHEAT SHEET ===\n");

        console.log("1. STORAGE PACKING");
        console.log("   Pack small vars together (uint128 + uint128 in one slot)");
        console.log("   Saves: ~20,000 gas per avoided SSTORE\n");

        console.log("2. CALLDATA vs MEMORY");
        console.log("   Use calldata for external function array params");
        console.log("   Saves: ~60 gas per array element\n");

        console.log("3. UNCHECKED MATH");
        console.log("   Use unchecked{} when overflow is impossible");
        console.log("   Saves: ~100 gas per operation\n");

        console.log("4. CACHE STORAGE");
        console.log("   Read storage once, use local variable");
        console.log("   Saves: ~100 gas per avoided SLOAD\n");

        console.log("5. IMMUTABLE/CONSTANT");
        console.log("   Use for values set at deploy time");
        console.log("   Saves: ~2100 gas per read (no SLOAD)\n");

        console.log("6. CUSTOM ERRORS");
        console.log("   Use error X() instead of require(..., 'string')");
        console.log("   Saves: Variable based on string length\n");

        console.log("7. SHORT-CIRCUIT");
        console.log("   Put cheap checks first in && conditions");
        console.log("   Saves: Cost of skipped operations\n");
    }
}
