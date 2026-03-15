//! # Ethereum Primitives Demo
//!
//! Demonstrates basic usage of the library.

use eth_primitives::{
    Address, H256, U256, keccak256,
    signature::{sign, private_to_address, hash_message, eip712_domain_separator},
    Transaction,
};

fn main() {
    println!("🦀 Ethereum Primitives Library Demo\n");

    // =========================================
    // 1. Hashing
    // =========================================
    println!("=== 1. Keccak256 Hashing ===");
    let hash = keccak256(b"hello");
    println!("keccak256('hello') = {}", hash);

    let empty_hash = keccak256(b"");
    println!("keccak256('') = {}", empty_hash);
    println!();

    // =========================================
    // 2. Addresses
    // =========================================
    println!("=== 2. Ethereum Addresses ===");
    let addr = Address::from_hex("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").unwrap();
    println!("Parsed address: {}", addr.to_hex());
    println!("With checksum:  {}", addr.to_checksum());

    // Derive address from private key
    let private_key: [u8; 32] = [
        0xac, 0x09, 0x74, 0xbe, 0xc3, 0x9a, 0x17, 0xe3,
        0x6b, 0xa4, 0xa6, 0xb4, 0xd2, 0x38, 0xff, 0x94,
        0x4b, 0xac, 0xb4, 0x78, 0xcb, 0xed, 0x5e, 0xfb,
        0xca, 0x97, 0x96, 0xb2, 0x5e, 0xcd, 0xb7, 0x49,
    ];
    let derived_addr = private_to_address(&private_key).unwrap();
    println!("Derived address from private key: {}", derived_addr);
    println!();

    // =========================================
    // 3. U256 Arithmetic
    // =========================================
    println!("=== 3. U256 Arithmetic ===");
    let a = U256::from_u64(1_000_000_000_000_000_000); // 1 ETH in wei
    let b = U256::from_u64(500_000_000_000_000_000);   // 0.5 ETH

    println!("1 ETH   = {} wei", a.to_hex());
    println!("0.5 ETH = {} wei", b.to_hex());
    println!("Sum     = {} wei", (a + b).to_hex());
    println!("Diff    = {} wei", (a - b).to_hex());

    let max = U256::MAX;
    println!("U256 MAX = {} bits", max.bits());
    println!();

    // =========================================
    // 4. Signing & Verification
    // =========================================
    println!("=== 4. ECDSA Signatures ===");

    let message = b"Hello, Ethereum!";
    let message_hash = hash_message(message);
    println!("Message: \"Hello, Ethereum!\"");
    println!("Message hash: {}", message_hash);

    let signature = sign(&message_hash, &private_key).unwrap();
    println!("Signature:");
    println!("  r: {}", signature.r);
    println!("  s: {}", signature.s);
    println!("  v: {}", signature.v);

    let recovered = signature.recover(&message_hash).unwrap();
    println!("Recovered signer: {}", recovered);
    println!("Matches original: {}", recovered == derived_addr);
    println!();

    // =========================================
    // 5. EIP-712 Domain Separator
    // =========================================
    println!("=== 5. EIP-712 Domain Separator ===");
    let domain = eip712_domain_separator(
        "MyDApp",
        "1",
        1, // mainnet
        &derived_addr,
    );
    println!("Domain separator: {}", domain);
    println!();

    // =========================================
    // 6. Transactions
    // =========================================
    println!("=== 6. Transactions ===");

    let mut tx = Transaction::legacy(
        1, // mainnet
        0, // nonce
        U256::from_u64(20_000_000_000), // 20 gwei
        21000, // standard transfer gas
        Some(Address::zero()), // sending to zero address
        U256::from_u64(1_000_000_000_000_000_000), // 1 ETH
        vec![],
    );

    println!("Created legacy transaction:");
    println!("  Chain ID: {}", tx.chain_id);
    println!("  Nonce: {}", tx.nonce);
    println!("  Gas Price: {} gwei", tx.gas_price / U256::from_u64(1_000_000_000));
    println!("  Gas Limit: {}", tx.gas_limit);
    println!("  Value: {} wei", tx.value);

    tx.sign(&private_key).unwrap();
    println!("  Signed: ✅");

    let tx_hash = tx.hash().unwrap();
    println!("  Tx Hash: {}", tx_hash);

    let sender = tx.recover_sender().unwrap();
    println!("  Sender: {}", sender);

    let encoded = tx.encode().unwrap();
    println!("  Encoded size: {} bytes", encoded.len());
    println!();

    // =========================================
    // 7. EIP-1559 Transaction
    // =========================================
    println!("=== 7. EIP-1559 Transaction ===");

    let mut tx2 = Transaction::eip1559(
        1, // mainnet
        1, // nonce
        U256::from_u64(2_000_000_000),   // 2 gwei priority fee
        U256::from_u64(100_000_000_000), // 100 gwei max fee
        21000,
        Some(Address::from_hex("0x1234567890abcdef1234567890abcdef12345678").unwrap()),
        U256::from_u64(500_000_000_000_000_000), // 0.5 ETH
        vec![],
    );

    tx2.sign(&private_key).unwrap();
    let tx2_hash = tx2.hash().unwrap();

    println!("EIP-1559 transaction:");
    println!("  Max Priority Fee: 2 gwei");
    println!("  Max Fee: 100 gwei");
    println!("  Tx Hash: {}", tx2_hash);

    let encoded2 = tx2.encode().unwrap();
    println!("  Type prefix: 0x{:02x}", encoded2[0]);
    println!();

    println!("✅ All demos completed successfully!");
}
