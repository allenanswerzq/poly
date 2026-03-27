use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

// =============================================================================
// Weighted Routing / Canary Deployment
//
// A gateway picks WHICH backend serves each request.
// Weighted routing sends a percentage of traffic to different backends.
//
//   ┌──────────────────┐
//   │     Gateway       │
//   │                   │
//   │  weight=1 (1%) ──────► Canary  (new version)
//   │  weight=99(99%)──────► Stable  (old version)
//   └──────────────────┘
//
// Two approaches:
//   1. Random weighted: each request independently picks (simple, not sticky)
//   2. Hash-based sticky: same user always goes to same backend
// =============================================================================

struct WeightedRouter {
    backends: Vec<String>,
    weights: Vec<u32>,
    total_weight: u32,
}

impl WeightedRouter {
    fn new(backends: Vec<(&str, u32)>) -> Self {
        let total_weight = backends.iter().map(|(_, w)| w).sum();
        Self {
            backends: backends.iter().map(|(name, _)| name.to_string()).collect(),
            weights: backends.iter().map(|(_, w)| *w).collect(),
            total_weight,
        }
    }

    // Random weighted selection: each request independently picks a backend
    fn pick_random(&self) -> &str {
        let r = rand::random::<u32>() % self.total_weight;
        let mut cumulative = 0;
        for (i, &w) in self.weights.iter().enumerate() {
            cumulative += w;
            if r < cumulative {
                return &self.backends[i];
            }
        }
        &self.backends[0]
    }

    // Hash-based sticky: same user_id ALWAYS goes to the same backend.
    // This prevents users from flickering between canary and stable.
    fn pick_sticky(&self, user_id: &str) -> &str {
        let mut hasher = DefaultHasher::new();
        user_id.hash(&mut hasher);
        let shard = (hasher.finish() % self.total_weight as u64) as u32;

        let mut cumulative = 0;
        for (i, &w) in self.weights.iter().enumerate() {
            cumulative += w;
            if shard < cumulative {
                return &self.backends[i];
            }
        }
        &self.backends[0]
    }
}

pub fn demo() {
    println!("\n  ═══ Weighted Routing / Canary Deployment ═══\n");

    // ── Random weighted routing ──
    println!("    ── Random weighted: 1% canary, 99% stable ──\n");

    let router = WeightedRouter::new(vec![("canary", 1), ("stable", 99)]);

    let mut canary_count = 0;
    let mut stable_count = 0;
    let total = 10_000;

    for _ in 0..total {
        match router.pick_random() {
            "canary" => canary_count += 1,
            _ => stable_count += 1,
        }
    }

    println!(
        "    {} requests: canary={} ({:.1}%), stable={} ({:.1}%)",
        total,
        canary_count,
        canary_count as f64 / total as f64 * 100.0,
        stable_count,
        stable_count as f64 / total as f64 * 100.0
    );

    // ── Hash-based sticky routing ──
    println!("\n    ── Sticky routing: same user always hits same backend ──\n");

    let users = [
        "user-42", "user-7", "user-99", "user-123", "user-456", "user-789", "user-0", "user-55",
        "user-31", "user-88",
    ];

    for user in &users {
        let backend = router.pick_sticky(user);
        // Same user called multiple times → must get same result
        let same = router.pick_sticky(user) == backend;
        println!("    {} → {}  (consistent: {})", user, backend, same);
    }

    // ── Gradual rollout simulation ──
    println!("\n    ── Gradual rollout: 1% → 10% → 50% → 100% ──\n");

    let rollout_steps = [(1, 99), (10, 90), (50, 50), (100, 0)];

    for (canary_w, stable_w) in &rollout_steps {
        let router = WeightedRouter::new(vec![("canary", *canary_w), ("stable", *stable_w)]);

        let mut canary_hits = 0;
        for _ in 0..1000 {
            if router.pick_random() == "canary" {
                canary_hits += 1;
            }
        }

        println!(
            "    weight canary={:3}, stable={:3} → canary gets ~{:.0}% of traffic",
            canary_w,
            stable_w,
            canary_hits as f64 / 10.0
        );
    }

    // ── A/B testing (50/50 split) ──
    println!("\n    ── A/B testing: 50/50 split with sticky users ──\n");

    let ab_router = WeightedRouter::new(vec![("variant-A", 50), ("variant-B", 50)]);

    let mut a_users = Vec::new();
    let mut b_users = Vec::new();
    for i in 0..20 {
        let user = format!("user-{}", i);
        match ab_router.pick_sticky(&user) {
            "variant-A" => a_users.push(user),
            _ => b_users.push(user),
        }
    }
    println!(
        "    Variant A ({} users): {:?}",
        a_users.len(),
        &a_users[..3.min(a_users.len())]
    );
    println!(
        "    Variant B ({} users): {:?}",
        b_users.len(),
        &b_users[..3.min(b_users.len())]
    );
    println!("    Each user always sees the same variant (hash-based).\n");
}
