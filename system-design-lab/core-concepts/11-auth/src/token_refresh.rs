use std::collections::HashMap;
use chrono::Utc;
use rand::Rng;

// =============================================================================
// Token Refresh Flow
//
//   Two tokens:
//     Access token:  short-lived (15 min), sent with every API request
//     Refresh token: long-lived (7 days), used ONLY to get a new access token
//
//   Why two?
//     - Access token is in every request → high exposure → keep it short
//     - If stolen, attacker gets only 15 min window
//     - Refresh token sent rarely → lower exposure
//     - Refresh token can be revoked server-side (stored in DB)
//
//   Flow:
//     1. Login → get access_token (15min) + refresh_token (7d)
//     2. Use access_token for API calls
//     3. Access token expires
//     4. POST /refresh with refresh_token → new access_token
//     5. Refresh token expires → user must re-login
// =============================================================================

struct TokenPair {
    access_token: String,
    refresh_token: String,
    access_expires_at: i64,  // unix timestamp
    refresh_expires_at: i64,
}

/// Simulated token store (in production: Redis or DB)
struct TokenStore {
    // refresh_token → (user_id, expires_at, revoked)
    refresh_tokens: HashMap<String, (String, i64, bool)>,
}

impl TokenStore {
    fn new() -> Self {
        Self {
            refresh_tokens: HashMap::new(),
        }
    }

    fn issue_tokens(&mut self, user_id: &str) -> TokenPair {
        let now = Utc::now().timestamp();
        let access_token = generate_token();
        let refresh_token = generate_token();

        let access_expires = now + 900;      // 15 minutes
        let refresh_expires = now + 604800;  // 7 days

        // Store refresh token server-side (this is why it's revocable)
        self.refresh_tokens.insert(
            refresh_token.clone(),
            (user_id.to_string(), refresh_expires, false),
        );

        TokenPair {
            access_token,
            refresh_token,
            access_expires_at: access_expires,
            refresh_expires_at: refresh_expires,
        }
    }

    fn refresh(&mut self, refresh_token: &str) -> Result<TokenPair, &'static str> {
        let (user_id, expires_at, revoked) = self
            .refresh_tokens
            .get(refresh_token)
            .ok_or("invalid refresh token")?;

        if *revoked {
            return Err("refresh token has been revoked");
        }

        let now = Utc::now().timestamp();
        if now > *expires_at {
            return Err("refresh token expired");
        }

        let user_id = user_id.clone();

        // Rotate: invalidate old refresh token, issue new pair
        self.refresh_tokens.get_mut(refresh_token).unwrap().2 = true;

        Ok(self.issue_tokens(&user_id))
    }

    fn revoke(&mut self, refresh_token: &str) -> bool {
        if let Some(entry) = self.refresh_tokens.get_mut(refresh_token) {
            entry.2 = true; // mark revoked
            true
        } else {
            false
        }
    }
}

fn generate_token() -> String {
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect()
}

pub fn demo() {
    println!("\n  ═══ Token Refresh Flow ═══\n");

    let mut store = TokenStore::new();

    // Step 1: Login — get both tokens
    println!("    Step 1: User logs in");
    let pair = store.issue_tokens("user-42");
    println!("    access_token:  {}... (expires in 15 min)", &pair.access_token[..16]);
    println!("    refresh_token: {}... (expires in 7 days)\n", &pair.refresh_token[..16]);

    // Step 2: Access token expires, use refresh token to get new pair
    println!("    Step 2: Access token expired, refresh it");
    let old_refresh = pair.refresh_token.clone();
    let new_pair = store.refresh(&old_refresh).unwrap();
    println!("    new access_token:  {}...", &new_pair.access_token[..16]);
    println!("    new refresh_token: {}... (old one rotated out)\n", &new_pair.refresh_token[..16]);

    // Step 3: Old refresh token no longer works (rotation)
    println!("    Step 3: Old refresh token is now invalid (token rotation)");
    match store.refresh(&old_refresh) {
        Ok(_) => println!("    old refresh token: ACCEPTED (bad!)"),
        Err(e) => println!("    old refresh token: REJECTED ✓ ({e})"),
    }

    // Step 4: Explicit revocation (e.g., user clicks "logout everywhere")
    println!("\n    Step 4: Revoke all tokens (logout everywhere)");
    let revoked = store.revoke(&new_pair.refresh_token);
    println!("    revoked: {revoked}");
    match store.refresh(&new_pair.refresh_token) {
        Ok(_) => println!("    revoked token: ACCEPTED (bad!)"),
        Err(e) => println!("    revoked token: REJECTED ✓ ({e})"),
    }

    println!("\n    Summary:");
    println!("    ┌──────────────────┬─────────────┬───────────────────────┐");
    println!("    │ Token            │ Lifetime    │ Storage               │");
    println!("    ├──────────────────┼─────────────┼───────────────────────┤");
    println!("    │ Access token     │ 15 min      │ Client only (JWT)     │");
    println!("    │ Refresh token    │ 7 days      │ Server (Redis/DB)     │");
    println!("    └──────────────────┴─────────────┴───────────────────────┘");
    println!("    Refresh tokens rotate on use → stolen token detected on next refresh.\n");
}
