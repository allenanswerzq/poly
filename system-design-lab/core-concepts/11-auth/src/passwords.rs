use bcrypt::{hash, verify, DEFAULT_COST};
use std::time::Instant;

// =============================================================================
// Password Hashing — NEVER store plaintext passwords
//
//   Bad:  users table → password = "hunter2"
//   Good: users table → password_hash = "$2b$12$LJ3m5..."
//
//   bcrypt:
//   - Intentionally SLOW (100ms+) to prevent brute force
//   - Includes random salt (same password → different hash each time)
//   - Cost factor controls slowness (12 = ~250ms, higher = slower)
//
//   Flow:
//     Register: hash(password) → store hash in DB
//     Login:    verify(password, stored_hash) → true/false
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Password Hashing (bcrypt) ═══\n");

    let password = "my_secret_password123";

    // Hash the password (this is what you store in the DB)
    println!("    Password: \"{}\"", password);
    let start = Instant::now();
    let hashed = hash(password, DEFAULT_COST).unwrap();
    println!("    Hash:     \"{}\"", hashed);
    println!("    Time:     {:?} (intentionally slow!)\n", start.elapsed());

    // Same password → different hash each time (random salt)
    let hashed2 = hash(password, DEFAULT_COST).unwrap();
    println!("    Hash again: \"{}\"", hashed2);
    println!("    Same password, different hash (random salt).\n");

    // Verify password (what happens at login)
    println!("    Login verification:");
    let correct = verify(password, &hashed).unwrap();
    println!("    verify(\"my_secret_password123\", hash) → {} ✓", correct);

    let wrong = verify("wrong_password", &hashed).unwrap();
    println!("    verify(\"wrong_password\", hash)        → {} ✗\n", wrong);

    // Show cost factor impact
    println!("    Cost factor (higher = slower = more secure):");
    for cost in [4, 8, 12] {
        let start = Instant::now();
        let _ = hash(password, cost).unwrap();
        println!("    cost={:2}: {:?}", cost, start.elapsed());
    }

    println!("\n    Production: cost=12 (~250ms). Adjust so login takes 200-500ms.");
    println!("    NEVER use: MD5, SHA256, plaintext. Always: bcrypt or argon2.\n");
}
