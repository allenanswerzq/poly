use sha2::{Digest, Sha256};
use std::collections::HashMap;
use rand::Rng;

// =============================================================================
// API Key Authentication
//
//   Simplest auth for developer APIs and service-to-service.
//
//   How it works:
//     1. Developer signs up → system generates API key
//     2. System stores HASH of the key (not plaintext!)
//     3. Developer sends key with every request: X-Api-Key: sk-abc123...
//     4. Server hashes the key, looks up in DB → find account + permissions
//
//   Best practices:
//     - Prefix by type: pk-xxx (publishable), sk-xxx (secret)
//     - Hash keys in DB (if DB is leaked, keys aren't exposed)
//     - Rate limit per key
//     - Support multiple keys per account (rotation)
//     - Set expiry dates
// =============================================================================

#[derive(Debug, Clone)]
struct ApiKeyRecord {
    key_hash: String,   // SHA-256 hash of the key (NOT the raw key)
    prefix: String,     // "sk-abc1" — enough to identify the key in UI
    account_id: String,
    permissions: Vec<String>,
    created_at: String,
    revoked: bool,
}

struct ApiKeyStore {
    // key_hash → record
    keys: HashMap<String, ApiKeyRecord>,
}

impl ApiKeyStore {
    fn new() -> Self {
        Self {
            keys: HashMap::new(),
        }
    }

    /// Generate a new API key. Returns the raw key (shown once to user).
    fn create_key(&mut self, account_id: &str, permissions: Vec<&str>) -> String {
        let raw_key = generate_api_key("sk");
        let key_hash = hash_key(&raw_key);
        let prefix = format!("{}...{}", &raw_key[..7], &raw_key[raw_key.len()-4..]);

        self.keys.insert(key_hash.clone(), ApiKeyRecord {
            key_hash,
            prefix,
            account_id: account_id.to_string(),
            permissions: permissions.iter().map(|s| s.to_string()).collect(),
            created_at: "2024-01-15".to_string(),
            revoked: false,
        });

        raw_key
    }

    /// Validate an API key from a request. Returns the account info if valid.
    fn validate(&self, raw_key: &str) -> Result<&ApiKeyRecord, &'static str> {
        let key_hash = hash_key(raw_key);
        let record = self.keys.get(&key_hash).ok_or("invalid API key")?;

        if record.revoked {
            return Err("API key has been revoked");
        }

        Ok(record)
    }

    /// Revoke a key (by hash, since we don't store the raw key).
    fn revoke(&mut self, raw_key: &str) -> bool {
        let key_hash = hash_key(raw_key);
        if let Some(record) = self.keys.get_mut(&key_hash) {
            record.revoked = true;
            true
        } else {
            false
        }
    }
}

fn generate_api_key(prefix: &str) -> String {
    let mut rng = rand::thread_rng();
    let random: String = (0..24)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect();
    format!("{prefix}-{random}")
}

fn hash_key(raw_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn demo() {
    println!("\n  ═══ API Key Authentication ═══\n");

    let mut store = ApiKeyStore::new();

    // Step 1: Create API keys for different accounts
    println!("    Step 1: Generate API keys\n");

    let key_alice = store.create_key("alice-corp", vec!["read", "write"]);
    let key_bob = store.create_key("bob-inc", vec!["read"]);

    println!("    Alice's key: {}", key_alice);
    println!("    Bob's key:   {}\n", key_bob);
    println!("    ⚠ Raw key shown ONCE at creation. We store only the hash.");

    // Step 2: Validate a key (what happens on every API request)
    println!("\n    Step 2: Validate API key from request header\n");

    println!("    Request: GET /api/data  X-Api-Key: {}", &key_alice);
    match store.validate(&key_alice) {
        Ok(record) => {
            println!("    ✓ Valid key ({})", record.prefix);
            println!("      account: {}", record.account_id);
            println!("      permissions: {:?}", record.permissions);
        }
        Err(e) => println!("    ✗ {e}"),
    }

    // Step 3: Invalid key
    println!("\n    Step 3: Invalid key rejected\n");
    match store.validate("sk-this-is-not-a-real-key-at-all-nope") {
        Ok(_) => println!("    ACCEPTED (bad!)"),
        Err(e) => println!("    ✗ Rejected: {e}"),
    }

    // Step 4: Revoke a key
    println!("\n    Step 4: Revoke a compromised key\n");
    store.revoke(&key_bob);
    println!("    Revoked Bob's key.");
    match store.validate(&key_bob) {
        Ok(_) => println!("    Bob's key: ACCEPTED (bad!)"),
        Err(e) => println!("    Bob's key: ✗ Rejected: {e}"),
    }

    // Show what's stored in DB
    println!("\n    What's stored in DB (NOT the raw key):\n");
    println!("    ┌─────────────────┬────────────────────────────────────────────────┐");
    println!("    │ prefix          │ key_hash (SHA-256)                             │");
    println!("    ├─────────────────┼────────────────────────────────────────────────┤");
    for record in store.keys.values() {
        let short_hash = &record.key_hash[..32];
        let status = if record.revoked { " (REVOKED)" } else { "" };
        println!("    │ {:<15} │ {short_hash}...{status:<10} │", record.prefix);
    }
    println!("    └─────────────────┴────────────────────────────────────────────────┘");

    println!("\n    If DB is breached, attacker gets hashes — not usable as API keys.");
    println!("    Like password hashing, but SHA-256 is fine here (keys are random,");
    println!("    not human-chosen, so brute-force is infeasible).\n");
}
