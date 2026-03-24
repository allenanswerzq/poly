//! # Auth Demo
//!
//! Demonstrates real authentication & authorization patterns:
//! 1. Password hashing (bcrypt) — never store plaintext
//! 2. JWT creation + validation — stateless auth tokens
//! 3. Token refresh flow — short access + long refresh
//! 4. RBAC — role-based access control
//! 5. API key auth — hashed storage + validation

mod passwords;
mod jwt_auth;
mod token_refresh;
mod rbac;
mod api_keys;

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║     Authentication & Authorization Demo          ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    println!("━━━ 1. Password Hashing (bcrypt) ━━━");
    passwords::demo();

    println!("━━━ 2. JWT Auth (jsonwebtoken) ━━━");
    jwt_auth::demo();

    println!("━━━ 3. Token Refresh Flow ━━━");
    token_refresh::demo();

    println!("━━━ 4. RBAC (Role-Based Access Control) ━━━");
    rbac::demo();

    println!("━━━ 5. API Key Auth ━━━");
    api_keys::demo();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
