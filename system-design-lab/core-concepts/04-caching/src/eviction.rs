use crate::store::Store;
use std::thread;
use std::time::Duration;

// =============================================================================
// TTL + Eviction
//
//   Every cached key MUST have a TTL. Keys without TTL = memory leak.
//
//   Redis TTL commands:
//     SET key val EX 60     → create with 60s TTL
//     EXPIRE key 30         → set/reset TTL on existing key
//     PERSIST key           → remove TTL (dangerous — lives forever)
//     TTL key               → seconds left (-1 = no TTL, -2 = expired/gone)
//
//   Eviction policies (when maxmemory hit):
//     noeviction      → reject new writes (safest, app handles error)
//     allkeys-lru     → evict least-recently-used across ALL keys
//     volatile-lru    → evict LRU among keys WITH a TTL only
//     allkeys-lfu     → evict least-frequently-used (Redis 4.0+)
//     volatile-ttl    → evict keys closest to expiring
//
//   Production rules:
//     1. Always set a TTL. No exceptions.
//     2. Use allkeys-lru for cache workloads (safe default)
//     3. Use noeviction for session stores (explicit failure > silent loss)
//     4. Monitor eviction rate: high eviction = need more memory
//
//   Sliding TTL pattern:
//     Reset TTL on every read → key stays alive while actively used.
//     Used for sessions: 30-min inactivity timeout.
//     Gotcha: popular keys never expire → memory grows. Cap with max TTL.
// =============================================================================

pub fn demo() {
    println!("\n  ═══ TTL + Eviction ═══\n");

    let store = Store::new();

    // ── 1. Basic TTL: create with expiry ──

    store.cache.set("temp", "disappears", 2); // 2-second TTL
    store.cache.set_permanent("perm", "stays"); // no TTL (bad practice!)

    println!("    SET temp (2s TTL), perm (no TTL)");
    println!(
        "      temp: value={:?}, TTL={}s",
        store.cache.get("temp"),
        store.cache.ttl("temp")
    );
    println!(
        "      perm: value={:?}, TTL={} (no expiry)",
        store.cache.get("perm"),
        store.cache.ttl("perm")
    );

    thread::sleep(Duration::from_secs(3));
    println!("\n    After 3 seconds:");
    println!(
        "      temp: value={:?}, TTL={} (gone)",
        store.cache.get("temp"),
        store.cache.ttl("temp")
    );
    println!(
        "      perm: value={:?}, TTL={} (still here)",
        store.cache.get("perm"),
        store.cache.ttl("perm")
    );

    // ── 2. EXPIRE: add TTL to existing key ──

    store.cache.set_permanent("session:abc", "user_data");
    println!("\n    EXPIRE — add TTL to existing key:");
    println!("      Before: TTL={}", store.cache.ttl("session:abc"));
    store.cache.expire("session:abc", 10);
    println!("      EXPIRE 10 → TTL={}", store.cache.ttl("session:abc"));

    // ── 3. PERSIST: remove TTL (use with caution) ──

    store.cache.persist("session:abc");
    println!("\n    PERSIST — remove TTL:");
    println!(
        "      After PERSIST → TTL={} (immortal, dangerous!)",
        store.cache.ttl("session:abc")
    );

    // ── 4. Sliding TTL: reset on every access (session pattern) ──
    //
    //   Use case: "session expires after 30 min of inactivity"
    //   Every request calls EXPIRE to push the deadline forward.
    //

    println!("\n    Sliding TTL (session timeout pattern):");
    store.cache.set("session:x", "active_user", 5); // 5s inactivity timeout

    for i in 1..=3 {
        thread::sleep(Duration::from_secs(1));
        let _val = store.cache.get("session:x"); // user is active
        store.cache.expire("session:x", 5); // reset timeout
        println!(
            "      +{}s: user active → TTL reset to {}s",
            i,
            store.cache.ttl("session:x")
        );
    }
    println!("      Key alive after 3s because each access resets the clock");

    // Stop accessing → key expires
    println!("      Now user goes idle...");
    thread::sleep(Duration::from_secs(6));
    println!(
        "      +6s inactivity → value={:?}, TTL={} (expired)\n",
        store.cache.get("session:x"),
        store.cache.ttl("session:x")
    );
}
