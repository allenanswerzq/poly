use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct CircuitBreaker {
    failure_count: AtomicU32,
    threshold: u32,
    last_failure: AtomicU64,
    cooldown_secs: u64,
}

impl CircuitBreaker {
    fn new(threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            threshold,
            last_failure: AtomicU64::new(0),
            cooldown_secs,
        }
    }

    fn is_open(&self) -> bool {
        let failures = self.failure_count.load(Ordering::Relaxed);
        if failures < self.threshold {
            return false;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let last = self.last_failure.load(Ordering::Relaxed);
        now - last < self.cooldown_secs
    }

    fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
    }

    fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.last_failure.store(now, Ordering::Relaxed);
    }
}

pub fn demo_circuit_breaker() {
    println!("\n  ═══ demo_circuit_breaker ═══\n");
    println!("  Circuit breaker protects against cascading failures:\n");

    let cb = CircuitBreaker::new(3, 5);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap();

    for i in 1..=8 {
        if cb.is_open() {
            println!(
                "    Request #{}: ⚡ CIRCUIT OPEN — skipped (returning 503)",
                i
            );
            continue;
        }

        match client.get("http://127.0.0.1:9103/data").send() {
            Ok(resp) if resp.status().is_success() => {
                cb.record_success();
                println!("    Request #{}: ✓ Success (circuit closed)", i);
            }
            _ => {
                cb.record_failure();
                let failures = cb.failure_count.load(Ordering::Relaxed);
                println!(
                    "    Request #{}: ✗ Failure (failures: {}/{}{})",
                    i,
                    failures,
                    cb.threshold,
                    if failures >= cb.threshold {
                        " → CIRCUIT OPENS!"
                    } else {
                        ""
                    }
                );
            }
        }
    }

    println!("\n    States: CLOSED (normal) → OPEN (stop sending) → HALF-OPEN (try one)");
    println!("    Prevents: cascading failures, resource exhaustion, thundering herd\n");
}
