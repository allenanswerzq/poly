#![allow(dead_code, unused_variables, unused_imports, clippy::all)]
//! # Payment System - Mini Implementation
//!
//! Demonstrates:
//! - Idempotent payment processing
//! - Two-phase commit pattern
//! - Balance management with locks
//! - Transaction logging (WAL)
//! - Retry with exactly-once semantics
//!
//! Run: cargo run -p payment-system

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use thiserror::Error;

fn instant_now() -> Instant {
    Instant::now()
}

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Account {
    id: String,
    balance: i64,         // In cents
    pending_balance: i64, // Held amount
    currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PaymentRequest {
    idempotency_key: String,
    from_account: String,
    to_account: String,
    amount: i64,
    currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Payment {
    id: String,
    request: PaymentRequest,
    status: PaymentStatus,
    #[serde(skip, default = "instant_now")]
    created_at: Instant,
    #[serde(skip, default = "instant_now")]
    updated_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum PaymentStatus {
    Pending,
    Authorized, // Amount held
    Captured,   // Money transferred
    Failed,
    Cancelled,
    Refunded,
}

#[derive(Debug, Clone)]
struct Ledger {
    id: String,
    account_id: String,
    payment_id: String,
    amount: i64,
    balance_after: i64,
    entry_type: LedgerEntryType,
    timestamp: Instant,
}

#[derive(Debug, Clone, Copy)]
enum LedgerEntryType {
    Debit,
    Credit,
    Hold,
    Release,
}

#[derive(Error, Debug)]
enum PaymentError {
    #[error("Insufficient balance")]
    InsufficientBalance,
    #[error("Account not found")]
    AccountNotFound,
    #[error("Payment already processed")]
    DuplicatePayment,
    #[error("Invalid payment state")]
    InvalidState,
    #[error("Currency mismatch")]
    CurrencyMismatch,
}

// =============================================================================
// Idempotency Store
// =============================================================================

struct IdempotencyStore {
    // idempotency_key -> payment_id
    keys: DashMap<String, String>,
    ttl_entries: Mutex<VecDeque<(Instant, String)>>,
}

impl IdempotencyStore {
    fn new() -> Self {
        Self {
            keys: DashMap::new(),
            ttl_entries: Mutex::new(VecDeque::new()),
        }
    }

    fn get_or_create(&self, key: &str, payment_id: &str) -> Option<String> {
        if let Some(existing) = self.keys.get(key) {
            return Some(existing.clone());
        }

        self.keys.insert(key.to_string(), payment_id.to_string());
        self.ttl_entries
            .lock()
            .push_back((Instant::now(), key.to_string()));

        None
    }

    fn get(&self, key: &str) -> Option<String> {
        self.keys.get(key).map(|v| v.clone())
    }
}

// =============================================================================
// Write-Ahead Log
// =============================================================================

#[derive(Debug, Clone)]
struct WalEntry {
    sequence: u64,
    payment_id: String,
    action: WalAction,
    timestamp: Instant,
}

#[derive(Debug, Clone)]
enum WalAction {
    PaymentCreated(PaymentRequest),
    PaymentAuthorized,
    PaymentCaptured,
    PaymentFailed(String),
    PaymentCancelled,
}

struct WriteAheadLog {
    entries: RwLock<Vec<WalEntry>>,
    sequence: AtomicU64,
}

impl WriteAheadLog {
    fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            sequence: AtomicU64::new(0),
        }
    }

    fn append(&self, payment_id: &str, action: WalAction) -> u64 {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);

        let entry = WalEntry {
            sequence: seq,
            payment_id: payment_id.to_string(),
            action,
            timestamp: Instant::now(),
        };

        self.entries.write().push(entry);
        seq
    }

    fn get_entries(&self, from_seq: u64) -> Vec<WalEntry> {
        self.entries
            .read()
            .iter()
            .filter(|e| e.sequence >= from_seq)
            .cloned()
            .collect()
    }
}

// =============================================================================
// Account Service
// =============================================================================

struct AccountService {
    accounts: DashMap<String, RwLock<Account>>,
    ledger: DashMap<String, Vec<Ledger>>,
    ledger_counter: AtomicU64,
}

impl AccountService {
    fn new() -> Self {
        Self {
            accounts: DashMap::new(),
            ledger: DashMap::new(),
            ledger_counter: AtomicU64::new(0),
        }
    }

    fn create_account(&self, id: &str, initial_balance: i64, currency: &str) {
        self.accounts.insert(
            id.to_string(),
            RwLock::new(Account {
                id: id.to_string(),
                balance: initial_balance,
                pending_balance: 0,
                currency: currency.to_string(),
            }),
        );
    }

    fn get_balance(&self, account_id: &str) -> Option<i64> {
        self.accounts.get(account_id).map(|a| a.read().balance)
    }

    fn hold(&self, account_id: &str, amount: i64, payment_id: &str) -> Result<(), PaymentError> {
        let account_lock = self
            .accounts
            .get(account_id)
            .ok_or(PaymentError::AccountNotFound)?;

        let mut account = account_lock.write();

        if account.balance < amount {
            return Err(PaymentError::InsufficientBalance);
        }

        account.balance -= amount;
        account.pending_balance += amount;

        // Record ledger entry
        self.record_ledger(
            account_id,
            payment_id,
            -amount,
            account.balance,
            LedgerEntryType::Hold,
        );

        Ok(())
    }

    fn release(&self, account_id: &str, amount: i64, payment_id: &str) -> Result<(), PaymentError> {
        let account_lock = self
            .accounts
            .get(account_id)
            .ok_or(PaymentError::AccountNotFound)?;

        let mut account = account_lock.write();

        account.pending_balance -= amount;
        account.balance += amount;

        self.record_ledger(
            account_id,
            payment_id,
            amount,
            account.balance,
            LedgerEntryType::Release,
        );

        Ok(())
    }

    fn capture(
        &self,
        from_account: &str,
        to_account: &str,
        amount: i64,
        payment_id: &str,
    ) -> Result<(), PaymentError> {
        // Debit from source (already held)
        {
            let account_lock = self
                .accounts
                .get(from_account)
                .ok_or(PaymentError::AccountNotFound)?;

            let mut account = account_lock.write();
            account.pending_balance -= amount;

            self.record_ledger(
                from_account,
                payment_id,
                -amount,
                account.balance,
                LedgerEntryType::Debit,
            );
        }

        // Credit to destination
        {
            let account_lock = self
                .accounts
                .get(to_account)
                .ok_or(PaymentError::AccountNotFound)?;

            let mut account = account_lock.write();
            account.balance += amount;

            self.record_ledger(
                to_account,
                payment_id,
                amount,
                account.balance,
                LedgerEntryType::Credit,
            );
        }

        Ok(())
    }

    fn record_ledger(
        &self,
        account_id: &str,
        payment_id: &str,
        amount: i64,
        balance_after: i64,
        entry_type: LedgerEntryType,
    ) {
        let entry = Ledger {
            id: format!(
                "ledger_{}",
                self.ledger_counter.fetch_add(1, Ordering::SeqCst)
            ),
            account_id: account_id.to_string(),
            payment_id: payment_id.to_string(),
            amount,
            balance_after,
            entry_type,
            timestamp: Instant::now(),
        };

        self.ledger
            .entry(account_id.to_string())
            .or_default()
            .push(entry);
    }

    fn get_ledger(&self, account_id: &str) -> Vec<Ledger> {
        self.ledger
            .get(account_id)
            .map(|l| l.clone())
            .unwrap_or_default()
    }
}

// =============================================================================
// Payment Service
// =============================================================================

struct PaymentService {
    payments: DashMap<String, Payment>,
    accounts: AccountService,
    idempotency: IdempotencyStore,
    wal: WriteAheadLog,
    payment_counter: AtomicU64,
}

impl PaymentService {
    fn new() -> Self {
        Self {
            payments: DashMap::new(),
            accounts: AccountService::new(),
            idempotency: IdempotencyStore::new(),
            wal: WriteAheadLog::new(),
            payment_counter: AtomicU64::new(0),
        }
    }

    fn create_payment(&self, request: PaymentRequest) -> Result<Payment, PaymentError> {
        let payment_id = format!(
            "pay_{}",
            self.payment_counter.fetch_add(1, Ordering::SeqCst)
        );

        // Check idempotency
        if let Some(existing_id) = self
            .idempotency
            .get_or_create(&request.idempotency_key, &payment_id)
        {
            // Return existing payment
            return self
                .payments
                .get(&existing_id)
                .map(|p| p.clone())
                .ok_or(PaymentError::DuplicatePayment);
        }

        // Log to WAL
        self.wal
            .append(&payment_id, WalAction::PaymentCreated(request.clone()));

        let payment = Payment {
            id: payment_id.clone(),
            request,
            status: PaymentStatus::Pending,
            created_at: Instant::now(),
            updated_at: Instant::now(),
        };

        self.payments.insert(payment_id, payment.clone());
        Ok(payment)
    }

    fn authorize(&self, payment_id: &str) -> Result<Payment, PaymentError> {
        let mut payment = self
            .payments
            .get_mut(payment_id)
            .ok_or(PaymentError::InvalidState)?;

        if payment.status != PaymentStatus::Pending {
            return Err(PaymentError::InvalidState);
        }

        // Hold funds
        self.accounts.hold(
            &payment.request.from_account,
            payment.request.amount,
            payment_id,
        )?;

        // Update status
        payment.status = PaymentStatus::Authorized;
        payment.updated_at = Instant::now();

        self.wal.append(payment_id, WalAction::PaymentAuthorized);

        Ok(payment.clone())
    }

    fn capture(&self, payment_id: &str) -> Result<Payment, PaymentError> {
        let mut payment = self
            .payments
            .get_mut(payment_id)
            .ok_or(PaymentError::InvalidState)?;

        if payment.status != PaymentStatus::Authorized {
            return Err(PaymentError::InvalidState);
        }

        // Transfer funds
        self.accounts.capture(
            &payment.request.from_account,
            &payment.request.to_account,
            payment.request.amount,
            payment_id,
        )?;

        payment.status = PaymentStatus::Captured;
        payment.updated_at = Instant::now();

        self.wal.append(payment_id, WalAction::PaymentCaptured);

        Ok(payment.clone())
    }

    fn cancel(&self, payment_id: &str) -> Result<Payment, PaymentError> {
        let mut payment = self
            .payments
            .get_mut(payment_id)
            .ok_or(PaymentError::InvalidState)?;

        if payment.status != PaymentStatus::Authorized {
            return Err(PaymentError::InvalidState);
        }

        // Release held funds
        self.accounts.release(
            &payment.request.from_account,
            payment.request.amount,
            payment_id,
        )?;

        payment.status = PaymentStatus::Cancelled;
        payment.updated_at = Instant::now();

        self.wal.append(payment_id, WalAction::PaymentCancelled);

        Ok(payment.clone())
    }

    fn process(&self, request: PaymentRequest) -> Result<Payment, PaymentError> {
        // Full flow: Create -> Authorize -> Capture
        let payment = self.create_payment(request)?;
        let payment = self.authorize(&payment.id)?;
        self.capture(&payment.id)
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== Payment System Demo ===\n");

    let service = PaymentService::new();

    // Create accounts
    println!("\n  ═══ Creating Accounts ═══");
    service.accounts.create_account("alice", 10000, "USD"); // $100.00
    service.accounts.create_account("bob", 5000, "USD"); // $50.00
    service.accounts.create_account("merchant", 0, "USD");

    println!(
        "Alice: ${:.2}",
        service.accounts.get_balance("alice").unwrap() as f64 / 100.0
    );
    println!(
        "Bob: ${:.2}",
        service.accounts.get_balance("bob").unwrap() as f64 / 100.0
    );
    println!(
        "Merchant: ${:.2}",
        service.accounts.get_balance("merchant").unwrap() as f64 / 100.0
    );
    println!();

    // Process payment
    println!("\n  ═══ Processing Payment ═══");
    let request = PaymentRequest {
        idempotency_key: "order_12345".to_string(),
        from_account: "alice".to_string(),
        to_account: "merchant".to_string(),
        amount: 2500, // $25.00
        currency: "USD".to_string(),
    };

    match service.process(request.clone()) {
        Ok(payment) => {
            println!("Payment {} completed!", payment.id);
            println!("Status: {:?}", payment.status);
        }
        Err(e) => println!("Payment failed: {}", e),
    }

    println!("\nBalances after payment:");
    println!(
        "Alice: ${:.2}",
        service.accounts.get_balance("alice").unwrap() as f64 / 100.0
    );
    println!(
        "Merchant: ${:.2}",
        service.accounts.get_balance("merchant").unwrap() as f64 / 100.0
    );
    println!();

    // Idempotency test
    println!("\n  ═══ Idempotency Test ═══");
    println!("Retrying same payment (same idempotency key)...");

    match service.process(request.clone()) {
        Ok(payment) => println!("Same payment returned: {} (no double charge!)", payment.id),
        Err(PaymentError::DuplicatePayment) => println!("Duplicate detected correctly!"),
        Err(e) => println!("Error: {}", e),
    }
    println!();

    // Authorization hold and cancel
    println!("\n  ═══ Auth/Cancel Flow ═══");
    let request2 = PaymentRequest {
        idempotency_key: "order_67890".to_string(),
        from_account: "bob".to_string(),
        to_account: "merchant".to_string(),
        amount: 3000, // $30.00
        currency: "USD".to_string(),
    };

    let payment = service.create_payment(request2).unwrap();
    println!("Created payment: {}", payment.id);

    let payment = service.authorize(&payment.id).unwrap();
    println!("Authorized - Bob's held funds: {}", payment.status as u8);
    println!(
        "Bob available: ${:.2}",
        service.accounts.get_balance("bob").unwrap() as f64 / 100.0
    );

    // Cancel instead of capture
    let payment = service.cancel(&payment.id).unwrap();
    println!("Cancelled - funds released");
    println!(
        "Bob available: ${:.2}",
        service.accounts.get_balance("bob").unwrap() as f64 / 100.0
    );
    println!();

    // Insufficient balance
    println!("\n  ═══ Insufficient Balance Test ═══");
    let request3 = PaymentRequest {
        idempotency_key: "order_big".to_string(),
        from_account: "bob".to_string(),
        to_account: "merchant".to_string(),
        amount: 100000, // $1000.00
        currency: "USD".to_string(),
    };

    match service.process(request3) {
        Ok(_) => println!("Payment succeeded unexpectedly"),
        Err(e) => println!("Payment failed as expected: {}", e),
    }
    println!();

    // Ledger
    println!("\n  ═══ Ledger Entries for Alice ═══");
    let ledger = service.accounts.get_ledger("alice");
    for entry in ledger {
        let entry_type = match entry.entry_type {
            LedgerEntryType::Debit => "DEBIT",
            LedgerEntryType::Credit => "CREDIT",
            LedgerEntryType::Hold => "HOLD",
            LedgerEntryType::Release => "RELEASE",
        };
        println!(
            "  {} ${:.2} -> balance ${:.2} ({})",
            entry_type,
            entry.amount.abs() as f64 / 100.0,
            entry.balance_after as f64 / 100.0,
            entry.payment_id
        );
    }

    // WAL entries
    println!("\n--- Write-Ahead Log ---");
    let wal_entries = service.wal.get_entries(0);
    println!("{} WAL entries recorded", wal_entries.len());

    println!("\n=== Key Concepts ===");
    println!("1. Idempotency: Same request -> same result (no double charge)");
    println!("2. Two-Phase: Authorize (hold) -> Capture (transfer)");
    println!("3. WAL: Log before state change for recovery");
    println!("4. Ledger: Audit trail of all account movements");
    println!("5. Account Locks: Prevent concurrent balance modifications");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payment_flow() {
        let service = PaymentService::new();
        service.accounts.create_account("a", 10000, "USD");
        service.accounts.create_account("b", 0, "USD");

        let request = PaymentRequest {
            idempotency_key: "test_1".to_string(),
            from_account: "a".to_string(),
            to_account: "b".to_string(),
            amount: 1000,
            currency: "USD".to_string(),
        };

        let payment = service.process(request).unwrap();
        assert_eq!(payment.status, PaymentStatus::Captured);
        assert_eq!(service.accounts.get_balance("a"), Some(9000));
        assert_eq!(service.accounts.get_balance("b"), Some(1000));
    }

    #[test]
    fn test_idempotency() {
        let service = PaymentService::new();
        service.accounts.create_account("a", 10000, "USD");
        service.accounts.create_account("b", 0, "USD");

        let request = PaymentRequest {
            idempotency_key: "test_2".to_string(),
            from_account: "a".to_string(),
            to_account: "b".to_string(),
            amount: 1000,
            currency: "USD".to_string(),
        };

        let p1 = service.process(request.clone()).unwrap();
        let p2 = service.process(request.clone()).unwrap();

        assert_eq!(p1.id, p2.id);
        // Should still only be 9000, not 8000
        assert_eq!(service.accounts.get_balance("a"), Some(9000));
    }

    #[test]
    fn test_insufficient_balance() {
        let service = PaymentService::new();
        service.accounts.create_account("a", 100, "USD");
        service.accounts.create_account("b", 0, "USD");

        let request = PaymentRequest {
            idempotency_key: "test_3".to_string(),
            from_account: "a".to_string(),
            to_account: "b".to_string(),
            amount: 1000,
            currency: "USD".to_string(),
        };

        let result = service.process(request);
        assert!(matches!(result, Err(PaymentError::InsufficientBalance)));
    }
}
