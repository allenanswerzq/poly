//! # Mini EVM Demo
//!
//! Execute EVM bytecode

use mini_evm::{execute, ExecutionContext, Interpreter};
use mini_evm::storage::StateDB;
use eth_primitives::{Address, U256};

fn main() {
    println!("🔷 Mini EVM Interpreter Demo\n");

    // =========================================
    // Test 1: Simple Addition (1 + 2 = 3)
    // =========================================
    println!("=== Test 1: Simple Addition ===");
    println!("Bytecode: PUSH1 1, PUSH1 2, ADD, STOP");
    println!("Expected: Stack top = 3\n");

    // PUSH1 1, PUSH1 2, ADD, STOP
    let code = vec![0x60, 0x01, 0x60, 0x02, 0x01, 0x00];
    let ctx = ExecutionContext::default();
    let mut state = StateDB::new();

    let result = execute(&code, &ctx, &mut state);
    println!("Success: {}", result.success);
    println!("Gas used: {}", result.gas_used);
    println!();

    // =========================================
    // Test 2: Memory Store & Return
    // =========================================
    println!("=== Test 2: Memory Operations ===");
    println!("Bytecode: PUSH1 0x42, PUSH1 0, MSTORE, PUSH1 32, PUSH1 0, RETURN");
    println!("Expected: Returns 32 bytes with 0x42 at position 31\n");

    let code = vec![
        0x60, 0x42,  // PUSH1 0x42
        0x60, 0x00,  // PUSH1 0x00
        0x52,        // MSTORE
        0x60, 0x20,  // PUSH1 0x20 (32)
        0x60, 0x00,  // PUSH1 0x00
        0xf3,        // RETURN
    ];

    let result = execute(&code, &ctx, &mut state);
    println!("Success: {}", result.success);
    println!("Output length: {} bytes", result.output.len());
    println!("Output (hex): 0x{}", hex::encode(&result.output));
    println!("Output[31]: 0x{:02x}", result.output.get(31).unwrap_or(&0));
    println!();

    // =========================================
    // Test 3: Conditional Jump
    // =========================================
    println!("=== Test 3: Conditional Jump ===");
    println!("Testing JUMP and JUMPDEST\n");

    // PUSH1 0x05, JUMP, INVALID, INVALID, JUMPDEST, PUSH1 0xFF, STOP
    let code = vec![
        0x60, 0x05,  // PUSH1 0x05 (jump destination)
        0x56,        // JUMP
        0xfe,        // INVALID (should be skipped)
        0xfe,        // INVALID (should be skipped)
        0x5b,        // JUMPDEST (offset 5)
        0x60, 0xff,  // PUSH1 0xFF
        0x00,        // STOP
    ];

    let result = execute(&code, &ctx, &mut state);
    println!("Success: {} (jumped over INVALID opcodes)", result.success);
    println!();

    // =========================================
    // Test 4: Storage Operations
    // =========================================
    println!("=== Test 4: Storage (SSTORE/SLOAD) ===");
    println!("Store 0x1234 at slot 0, then load it back\n");

    let code = vec![
        0x61, 0x12, 0x34,  // PUSH2 0x1234
        0x60, 0x00,        // PUSH1 0x00 (slot)
        0x55,              // SSTORE
        0x60, 0x00,        // PUSH1 0x00 (slot)
        0x54,              // SLOAD
        0x60, 0x00,        // PUSH1 0x00 (offset)
        0x52,              // MSTORE
        0x60, 0x20,        // PUSH1 32
        0x60, 0x00,        // PUSH1 0
        0xf3,              // RETURN
    ];

    let result = execute(&code, &ctx, &mut state);
    println!("Success: {}", result.success);
    println!("Returned value: 0x{}", hex::encode(&result.output));
    println!();

    // =========================================
    // Test 5: Keccak256
    // =========================================
    println!("=== Test 5: Keccak256 Hash ===");
    println!("Hash the calldata\n");

    let code = vec![
        0x36,        // CALLDATASIZE
        0x60, 0x00,  // PUSH1 0
        0x60, 0x00,  // PUSH1 0
        0x37,        // CALLDATACOPY (copy to memory)
        0x36,        // CALLDATASIZE
        0x60, 0x00,  // PUSH1 0
        0x20,        // KECCAK256
        0x60, 0x00,  // PUSH1 0
        0x52,        // MSTORE
        0x60, 0x20,  // PUSH1 32
        0x60, 0x00,  // PUSH1 0
        0xf3,        // RETURN
    ];

    let mut ctx = ExecutionContext::default();
    ctx.call.data = b"hello".to_vec();

    let result = execute(&code, &ctx, &mut state);
    println!("Success: {}", result.success);
    println!("Hash of 'hello': 0x{}", hex::encode(&result.output));

    // Verify against known hash
    let expected = "1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8";
    println!("Expected:        0x{}", expected);
    println!("Match: {}", hex::encode(&result.output) == expected);
    println!();

    // =========================================
    // Test 6: Caller and Value
    // =========================================
    println!("=== Test 6: Environment Info ===");

    let code = vec![
        0x33,        // CALLER
        0x34,        // CALLVALUE
        0x32,        // ORIGIN
        0x00,        // STOP
    ];

    let caller = Address::from_hex("0x1234567890abcdef1234567890abcdef12345678").unwrap();
    let mut ctx = ExecutionContext::default();
    ctx.call.caller = caller;
    ctx.call.value = U256::from_u64(1_000_000_000_000_000_000); // 1 ETH
    ctx.tx.origin = caller;

    let mut interpreter = Interpreter::new(code.clone(), 1_000_000);
    let result = interpreter.run(&ctx, &mut state);

    println!("Success: {}", result.success);
    println!("Stack after execution:");
    for (i, val) in interpreter.stack().values().iter().enumerate() {
        println!("  [{}]: {}", i, val.to_hex());
    }
    println!();

    // =========================================
    // Test 7: Gas Metering
    // =========================================
    println!("=== Test 7: Gas Metering ===");

    let code = vec![
        0x60, 0x01,  // PUSH1 1 (3 gas)
        0x60, 0x02,  // PUSH1 2 (3 gas)
        0x01,        // ADD (3 gas)
        0x60, 0x03,  // PUSH1 3 (3 gas)
        0x02,        // MUL (5 gas)
        0x00,        // STOP (0 gas)
    ];

    let ctx = ExecutionContext::default();
    let result = execute(&code, &ctx, &mut state);

    println!("Gas used: {}", result.gas_used);
    println!("Gas remaining: {}", result.gas_remaining);
    println!("Expected: ~17 gas (3+3+3+3+5+0)");
    println!();

    println!("✅ All tests completed!");
}
