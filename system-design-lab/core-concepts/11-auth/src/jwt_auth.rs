use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use serde::{Deserialize, Serialize};
use chrono::Utc;

// =============================================================================
// JWT Auth — stateless authentication using real jsonwebtoken crate
//
//   Token structure:
//     eyJhbGci...  .  eyJ1c2Vy...  .  SflKxwRJ...
//     ───────────     ───────────     ───────────
//       Header          Payload        Signature
//     {"alg":"HS256"}  {"user_id":42}  HMAC(header+payload, SECRET)
//
//   Flow:
//     1. User logs in → server creates JWT with user info
//     2. Client sends JWT with every request (Authorization: Bearer xxx)
//     3. Server verifies signature (no DB call!) and extracts claims
//
//   The signature proves the token wasn't tampered with.
//   If anyone changes the payload, the signature won't match.
// =============================================================================

// Claims = the data stored inside the JWT payload
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,      // subject (user ID)
    role: String,     // user role (admin, user, etc.)
    email: String,    // user email
    exp: usize,       // expiry (unix timestamp) — REQUIRED
    iat: usize,       // issued at
}

pub fn demo() {
    println!("\n  ═══ JWT Auth (jsonwebtoken) ═══\n");

    // In production server side secret: load from environment variable, NEVER hardcode
    let secret = "my-super-secret-key-at-least-32-bytes";

    // ── Create a JWT (what happens at login) ──
    println!("    Step 1: User logs in → server creates JWT\n");

    let now = Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: "user-42".to_string(),
        role: "admin".to_string(),
        email: "alice@example.com".to_string(),
        exp: now + 900, // 15 minutes from now
        iat: now,
    };

    let token = encode(
        &Header::default(), // HS256 algorithm
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    ).unwrap();

    println!("    Claims: {:?}\n", claims);
    println!("    JWT token:");
    // Show the 3 parts
    let parts: Vec<&str> = token.split('.').collect();
    println!("    Header:    {}...", &parts[0][..20]);
    println!("    Payload:   {}...", &parts[1][..20]);
    println!("    Signature: {}...", &parts[2][..20]);
    println!("    Full:      {}...{}\n", &token[..40], &token[token.len()-20..]);

    // ── Verify and decode (what happens on every API request) ──
    println!("    Step 2: Server verifies JWT on every request\n");

    let token_data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(secret.as_ref()),
        &Validation::new(Algorithm::HS256),
    ).unwrap();

    println!("    Decoded claims:");
    println!("    user_id: {}", token_data.claims.sub);
    println!("    role:    {}", token_data.claims.role);
    println!("    email:   {}", token_data.claims.email);
    println!("    expires: {} (unix timestamp)\n", token_data.claims.exp);

    // ── Tampered token → verification fails ──
    println!("    Step 3: Tampered token is REJECTED\n");

    let mut tampered = token.clone();
    // Flip a character in the signature
    let last = tampered.len() - 1;
    unsafe {
        let bytes = tampered.as_bytes_mut();
        bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
    }

    match decode::<Claims>(
        &tampered,
        &DecodingKey::from_secret(secret.as_ref()),
        &Validation::new(Algorithm::HS256),
    ) {
        Ok(_) => println!("    Tampered token: ACCEPTED (this shouldn't happen!)"),
        Err(e) => println!("    Tampered token: REJECTED ✓ ({})", e),
    }

    // ── Wrong secret → verification fails ──
    match decode::<Claims>(
        &token,
        &DecodingKey::from_secret("wrong-secret-key-blah-blah-blah".as_ref()),
        &Validation::new(Algorithm::HS256),
    ) {
        Ok(_) => println!("    Wrong secret:   ACCEPTED (this shouldn't happen!)"),
        Err(e) => println!("    Wrong secret:   REJECTED ✓ ({})\n", e),
    }

    // ── Expired token → verification fails ──
    println!("    Step 4: Expired token is REJECTED\n");

    let expired_claims = Claims {
        sub: "user-42".to_string(),
        role: "admin".to_string(),
        email: "alice@example.com".to_string(),
        exp: now - 3600, // 1 hour AGO (expired)
        iat: now - 7200,
    };

    let expired_token = encode(
        &Header::default(),
        &expired_claims,
        &EncodingKey::from_secret(secret.as_ref()),
    ).unwrap();

    match decode::<Claims>(
        &expired_token,
        &DecodingKey::from_secret(secret.as_ref()),
        &Validation::new(Algorithm::HS256),
    ) {
        Ok(_) => println!("    Expired token: ACCEPTED (shouldn't happen!)"),
        Err(e) => println!("    Expired token: REJECTED ✓ ({})\n", e),
    }

    println!("    JWT: stateless verification (no DB call), tamper-proof, auto-expiry.");
    println!("    In production: RS256 (asymmetric) so services can verify without the secret.\n");
}
