use std::collections::HashMap;

// =============================================================================
// Saga Pattern — Distributed Transactions Without 2PC
//
//   Problem: in microservices, a "transaction" spans multiple services.
//   2PC requires ALL participants to hold locks during voting → blocking.
//   In a world of 50 microservices, 2PC is impractical.
//
//   Saga: a sequence of LOCAL transactions, each with a COMPENSATING action.
//   If step N fails → run compensations for steps N-1, N-2, ..., 1 (rollback).
//
//   Example: book a trip
//     Step 1: Reserve flight     (compensate: cancel flight)
//     Step 2: Reserve hotel      (compensate: cancel hotel)
//     Step 3: Charge payment     (compensate: refund payment)
//     If payment fails → cancel hotel → cancel flight
//
//   Two coordination styles:
//
//     Choreography (event-driven):
//       Each service listens for events and decides what to do next.
//       Service A emits "flight-reserved" → Service B reacts → "hotel-reserved" → ...
//       Pro: decoupled, no central coordinator
//       Con: hard to track the overall saga state, complex failure paths
//
//     Orchestration (central coordinator):
//       A saga orchestrator tells each service what to do.
//       Orchestrator: "reserve flight" → ok → "reserve hotel" → ok → "charge" → fail → "cancel hotel" → "cancel flight"
//       Pro: clear flow, easy to reason about failures
//       Con: single point of management (but not a SPOF — it's stateless + retryable)
//
//   Key properties:
//     - NO distributed locks (each step is a local transaction)
//     - Eventual consistency (intermediate states are visible)
//     - Compensations must be IDEMPOTENT (safe to retry)
//     - Some actions can't be compensated (e.g., sending an email)
//       → use "semantic compensation" (send a correction email)
//
//   Saga vs 2PC:
//     2PC: strong consistency, blocking, doesn't scale to many services
//     Saga: eventual consistency, non-blocking, scales to 50+ services
//     Most microservice architectures use Sagas, not 2PC.
//
//   Used by: Uber (trip booking), Airbnb (reservation flow), Netflix,
//            any microservice architecture with multi-service transactions
// =============================================================================

#[derive(Debug, Clone)]
struct SagaStep {
    name: String,
    will_fail: bool,
    executed: bool,
    compensated: bool,
}

/// A saga orchestrator: executes steps in order, compensates on failure.
struct SagaOrchestrator {
    #[allow(dead_code)]
    name: String,
    steps: Vec<SagaStep>,
    state: HashMap<String, String>, // accumulated state from each step
}

#[derive(Debug)]
enum SagaResult {
    Committed,
    RolledBack {
        #[allow(dead_code)]
        failed_at: String,
    },
}

impl SagaOrchestrator {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            steps: Vec::new(),
            state: HashMap::new(),
        }
    }

    fn add_step(&mut self, name: &str, will_fail: bool) {
        self.steps.push(SagaStep {
            name: name.to_string(),
            will_fail,
            executed: false,
            compensated: false,
        });
    }

    /// Execute the saga: forward steps, compensate on failure.
    fn execute(&mut self) -> SagaResult {
        println!("\n      ── Forward execution ──");

        let mut failed_idx = None;
        for i in 0..self.steps.len() {
            if self.steps[i].will_fail {
                println!("      Step {}: {} → FAILED!", i + 1, self.steps[i].name);
                failed_idx = Some(i);
                break;
            } else {
                self.steps[i].executed = true;
                let name = self.steps[i].name.clone();
                self.state
                    .insert(name.clone(), format!("{}-confirmed", name.to_lowercase()));
                println!("      Step {}: {} → OK", i + 1, name);
            }
        }

        match failed_idx {
            None => {
                println!("      All steps succeeded → COMMITTED");
                SagaResult::Committed
            }
            Some(idx) => {
                let failed_name = self.steps[idx].name.clone();
                println!("\n      ── Compensating (rolling back) ──");
                // Compensate in REVERSE order, only steps that were executed
                for i in (0..idx).rev() {
                    self.steps[i].compensated = true;
                    self.state.remove(&self.steps[i].name);
                    println!("      Compensate: cancel {} → OK", self.steps[i].name);
                }
                println!("      Saga rolled back (failed at: {})", failed_name);
                SagaResult::RolledBack {
                    failed_at: failed_name,
                }
            }
        }
    }
}

pub fn demo() {
    println!("\n  ═══ Saga Pattern ═══\n");

    // ── Successful saga: book a trip ──

    println!("    ── Saga 1: Book a trip (all steps succeed) ──");

    let mut saga1 = SagaOrchestrator::new("BookTrip");
    saga1.add_step("Reserve Flight", false);
    saga1.add_step("Reserve Hotel", false);
    saga1.add_step("Charge Payment", false);
    saga1.add_step("Send Confirmation Email", false);

    let result = saga1.execute();
    println!("      Result: {:?}", result);
    println!("      State: {:?}\n", saga1.state);

    // ── Failed saga: payment fails → compensate ──

    println!("    ── Saga 2: Payment fails → rollback flight + hotel ──");

    let mut saga2 = SagaOrchestrator::new("BookTrip");
    saga2.add_step("Reserve Flight", false);
    saga2.add_step("Reserve Hotel", false);
    saga2.add_step("Charge Payment", true); // ← this will fail
    saga2.add_step("Send Confirmation Email", false);

    let result = saga2.execute();
    println!("      Result: {:?}", result);
    println!("      State: {:?} (empty — fully rolled back)", saga2.state);
    println!("      Steps:");
    for step in &saga2.steps {
        println!(
            "        {} — executed={}, compensated={}",
            step.name, step.executed, step.compensated
        );
    }

    // ── Failed at first step: nothing to compensate ──

    println!("\n    ── Saga 3: First step fails → nothing to compensate ──");

    let mut saga3 = SagaOrchestrator::new("BookTrip");
    saga3.add_step("Reserve Flight", true); // ← fails immediately
    saga3.add_step("Reserve Hotel", false);
    saga3.add_step("Charge Payment", false);

    let result = saga3.execute();
    println!("      Result: {:?}\n", result);
}
