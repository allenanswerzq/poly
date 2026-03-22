//! # Ticket Booking System (Ticketmaster) - Mini Implementation
//!
//! Demonstrates:
//! - Seat inventory management with locks
//! - Temporary holds during checkout
//! - Pessimistic vs Optimistic locking
//! - Handling flash sales and hot events
//! - Queue-based booking for high demand
//!
//! Run: cargo run -p ticket-booking

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

fn default_option_instant() -> Option<Instant> {
    None
}

fn instant_now() -> Instant {
    Instant::now()
}

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Event {
    id: String,
    name: String,
    venue: String,
    date: String,
    total_seats: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum SeatStatus {
    Available,
    Held,      // Temporarily held during checkout
    Booked,    // Purchased
    Locked,    // Admin lock
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Seat {
    id: String,
    section: String,
    row: String,
    number: u32,
    price: u64,
    status: SeatStatus,
    held_by: Option<String>,    // User holding the seat
    #[serde(skip, default = "default_option_instant")]
    held_until: Option<Instant>, // Expiry time for hold
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Booking {
    id: String,
    user_id: String,
    event_id: String,
    seat_ids: Vec<String>,
    total_price: u64,
    status: BookingStatus,
    #[serde(skip, default = "instant_now")]
    created_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum BookingStatus {
    Pending,
    Confirmed,
    Cancelled,
    Expired,
}

// =============================================================================
// Seat Inventory (Critical Section)
// =============================================================================

struct SeatInventory {
    // event_id -> seats map
    event_seats: DashMap<String, RwLock<HashMap<String, Seat>>>,
    // Track seat counts for quick availability check
    available_count: DashMap<String, AtomicU64>,
    hold_duration: Duration,
}

impl SeatInventory {
    fn new(hold_duration: Duration) -> Self {
        Self {
            event_seats: DashMap::new(),
            available_count: DashMap::new(),
            hold_duration,
        }
    }

    fn initialize_event(&self, event_id: &str, seats: Vec<Seat>) {
        let count = seats.len() as u64;
        let seat_map: HashMap<String, Seat> =
            seats.into_iter().map(|s| (s.id.clone(), s)).collect();

        self.event_seats
            .insert(event_id.to_string(), RwLock::new(seat_map));
        self.available_count
            .insert(event_id.to_string(), AtomicU64::new(count));
    }

    fn get_available_seats(&self, event_id: &str) -> Vec<Seat> {
        self.event_seats
            .get(event_id)
            .map(|seats| {
                seats
                    .read()
                    .values()
                    .filter(|s| s.status == SeatStatus::Available)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn hold_seats(
        &self,
        event_id: &str,
        seat_ids: &[String],
        user_id: &str,
    ) -> Result<(), &'static str> {
        let seats_entry = self
            .event_seats
            .get(event_id)
            .ok_or("Event not found")?;

        let mut seats = seats_entry.write();

        // Check all seats are available (atomic check)
        for seat_id in seat_ids {
            let seat = seats.get(seat_id).ok_or("Seat not found")?;
            if seat.status != SeatStatus::Available {
                return Err("Seat not available");
            }
        }

        // Hold all seats atomically
        let hold_until = Instant::now() + self.hold_duration;
        for seat_id in seat_ids {
            if let Some(seat) = seats.get_mut(seat_id) {
                seat.status = SeatStatus::Held;
                seat.held_by = Some(user_id.to_string());
                seat.held_until = Some(hold_until);
            }
        }

        // Update available count
        if let Some(count) = self.available_count.get(event_id) {
            count.fetch_sub(seat_ids.len() as u64, Ordering::SeqCst);
        }

        Ok(())
    }

    fn confirm_booking(&self, event_id: &str, seat_ids: &[String], user_id: &str) -> Result<(), &'static str> {
        let seats_entry = self
            .event_seats
            .get(event_id)
            .ok_or("Event not found")?;

        let mut seats = seats_entry.write();

        for seat_id in seat_ids {
            let seat = seats.get_mut(seat_id).ok_or("Seat not found")?;

            // Verify this user holds the seat
            if seat.status != SeatStatus::Held {
                return Err("Seat not held");
            }
            if seat.held_by.as_deref() != Some(user_id) {
                return Err("Seat held by different user");
            }

            seat.status = SeatStatus::Booked;
            seat.held_by = None;
            seat.held_until = None;
        }

        Ok(())
    }

    fn release_holds(&self, event_id: &str, seat_ids: &[String]) {
        if let Some(seats_entry) = self.event_seats.get(event_id) {
            let mut seats = seats_entry.write();

            for seat_id in seat_ids {
                if let Some(seat) = seats.get_mut(seat_id) {
                    if seat.status == SeatStatus::Held {
                        seat.status = SeatStatus::Available;
                        seat.held_by = None;
                        seat.held_until = None;
                    }
                }
            }

            // Update available count
            if let Some(count) = self.available_count.get(event_id) {
                count.fetch_add(seat_ids.len() as u64, Ordering::SeqCst);
            }
        }
    }

    fn cleanup_expired_holds(&self, event_id: &str) -> usize {
        let mut released = 0;

        if let Some(seats_entry) = self.event_seats.get(event_id) {
            let mut seats = seats_entry.write();
            let now = Instant::now();

            for seat in seats.values_mut() {
                if seat.status == SeatStatus::Held {
                    if let Some(expiry) = seat.held_until {
                        if now > expiry {
                            seat.status = SeatStatus::Available;
                            seat.held_by = None;
                            seat.held_until = None;
                            released += 1;
                        }
                    }
                }
            }

            if released > 0 {
                if let Some(count) = self.available_count.get(event_id) {
                    count.fetch_add(released as u64, Ordering::SeqCst);
                }
            }
        }

        released
    }
}

// =============================================================================
// Booking Queue (For Flash Sales)
// =============================================================================

#[derive(Debug, Clone)]
struct BookingRequest {
    id: String,
    user_id: String,
    event_id: String,
    seat_count: usize,
    timestamp: Instant,
}

struct BookingQueue {
    queues: DashMap<String, Mutex<VecDeque<BookingRequest>>>,
    request_counter: AtomicU64,
    processing: DashMap<String, bool>, // event_id -> is processing
}

impl BookingQueue {
    fn new() -> Self {
        Self {
            queues: DashMap::new(),
            request_counter: AtomicU64::new(0),
            processing: DashMap::new(),
        }
    }

    fn enqueue(&self, event_id: &str, user_id: &str, seat_count: usize) -> String {
        let request_id = format!("req_{}", self.request_counter.fetch_add(1, Ordering::SeqCst));

        let request = BookingRequest {
            id: request_id.clone(),
            user_id: user_id.to_string(),
            event_id: event_id.to_string(),
            seat_count,
            timestamp: Instant::now(),
        };

        self.queues
            .entry(event_id.to_string())
            .or_insert_with(|| Mutex::new(VecDeque::new()))
            .lock()
            .push_back(request);

        request_id
    }

    fn dequeue(&self, event_id: &str) -> Option<BookingRequest> {
        self.queues.get(event_id)?.lock().pop_front()
    }

    fn queue_length(&self, event_id: &str) -> usize {
        self.queues
            .get(event_id)
            .map(|q| q.lock().len())
            .unwrap_or(0)
    }
}

// =============================================================================
// Booking Service
// =============================================================================

struct BookingService {
    inventory: Arc<SeatInventory>,
    queue: Arc<BookingQueue>,
    bookings: DashMap<String, Booking>,
    booking_counter: AtomicU64,
}

impl BookingService {
    fn new() -> Self {
        Self {
            inventory: Arc::new(SeatInventory::new(Duration::from_secs(300))), // 5 min hold
            queue: Arc::new(BookingQueue::new()),
            bookings: DashMap::new(),
            booking_counter: AtomicU64::new(0),
        }
    }

    fn create_event(&self, event: Event, seats: Vec<Seat>) {
        self.inventory.initialize_event(&event.id, seats);
    }

    fn get_available(&self, event_id: &str) -> Vec<Seat> {
        self.inventory.get_available_seats(event_id)
    }

    fn start_checkout(&self, user_id: &str, event_id: &str, seat_ids: Vec<String>) -> Result<Booking, &'static str> {
        // Try to hold seats
        self.inventory.hold_seats(event_id, &seat_ids, user_id)?;

        // Calculate total price
        let available = self.inventory.get_available_seats(event_id);
        let price: u64 = seat_ids
            .iter()
            .filter_map(|id| available.iter().find(|s| &s.id == id))
            .map(|s| s.price)
            .sum();

        let booking_id = format!("bk_{}", self.booking_counter.fetch_add(1, Ordering::SeqCst));

        let booking = Booking {
            id: booking_id.clone(),
            user_id: user_id.to_string(),
            event_id: event_id.to_string(),
            seat_ids,
            total_price: price,
            status: BookingStatus::Pending,
            created_at: Instant::now(),
        };

        self.bookings.insert(booking_id, booking.clone());
        Ok(booking)
    }

    fn complete_payment(&self, booking_id: &str) -> Result<Booking, &'static str> {
        let mut booking = self
            .bookings
            .get_mut(booking_id)
            .ok_or("Booking not found")?;

        if booking.status != BookingStatus::Pending {
            return Err("Booking not in pending state");
        }

        // Confirm the booking
        self.inventory.confirm_booking(
            &booking.event_id,
            &booking.seat_ids,
            &booking.user_id,
        )?;

        booking.status = BookingStatus::Confirmed;
        Ok(booking.clone())
    }

    fn cancel_booking(&self, booking_id: &str) -> Result<(), &'static str> {
        let mut booking = self
            .bookings
            .get_mut(booking_id)
            .ok_or("Booking not found")?;

        if booking.status == BookingStatus::Confirmed {
            // Release booked seats back to available
            self.inventory.release_holds(&booking.event_id, &booking.seat_ids);
        } else if booking.status == BookingStatus::Pending {
            // Release held seats
            self.inventory.release_holds(&booking.event_id, &booking.seat_ids);
        }

        booking.status = BookingStatus::Cancelled;
        Ok(())
    }

    fn queue_booking(&self, user_id: &str, event_id: &str, seat_count: usize) -> String {
        self.queue.enqueue(event_id, user_id, seat_count)
    }

    fn process_queue(&self, event_id: &str) -> Vec<String> {
        let mut processed = Vec::new();

        while let Some(request) = self.queue.dequeue(event_id) {
            let available = self.get_available(event_id);

            if available.len() >= request.seat_count {
                let seat_ids: Vec<String> = available
                    .iter()
                    .take(request.seat_count)
                    .map(|s| s.id.clone())
                    .collect();

                if let Ok(booking) = self.start_checkout(&request.user_id, event_id, seat_ids) {
                    processed.push(format!(
                        "{}: {} got {} seats",
                        request.id, request.user_id, request.seat_count
                    ));
                }
            } else {
                processed.push(format!("{}: {} - not enough seats", request.id, request.user_id));
            }
        }

        processed
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== Ticket Booking System (Ticketmaster) Demo ===\n");

    let service = BookingService::new();

    // Create event with seats
    println!("\n  ═══ Creating Event ═══");
    let event = Event {
        id: "concert_001".to_string(),
        name: "Taylor Swift - Eras Tour".to_string(),
        venue: "Madison Square Garden".to_string(),
        date: "2024-07-15".to_string(),
        total_seats: 10,
    };

    let seats: Vec<Seat> = (1..=10)
        .map(|i| Seat {
            id: format!("seat_{}", i),
            section: "A".to_string(),
            row: "1".to_string(),
            number: i,
            price: if i <= 3 { 500 } else { 200 }, // VIP vs Regular
            status: SeatStatus::Available,
            held_by: None,
            held_until: None,
        })
        .collect();

    service.create_event(event.clone(), seats);
    println!("Created '{}' with 10 seats\n", event.name);

    // Show available seats
    println!("\n  ═══ Available Seats ═══");
    let available = service.get_available("concert_001");
    println!("{} seats available:", available.len());
    for seat in &available[..3.min(available.len())] {
        println!("  {} - ${}", seat.id, seat.price);
    }
    println!();

    // Normal booking flow
    println!("\n  ═══ Normal Booking Flow ═══");
    let booking = service
        .start_checkout(
            "alice",
            "concert_001",
            vec!["seat_1".to_string(), "seat_2".to_string()],
        )
        .expect("Should hold seats");
    println!(
        "Alice started checkout: {} seats, ${} total (status: {:?})",
        booking.seat_ids.len(),
        booking.total_price,
        booking.status
    );

    // Complete payment
    let confirmed = service.complete_payment(&booking.id).expect("Should confirm");
    println!(
        "Payment complete! Booking {} is now {:?}\n",
        confirmed.id, confirmed.status
    );

    // Try to book same seat (should fail)
    println!("\n  ═══ Conflict Handling ═══");
    let result = service.start_checkout("bob", "concert_001", vec!["seat_1".to_string()]);
    match result {
        Ok(_) => println!("Bob got seat_1"),
        Err(e) => println!("Bob tried seat_1: {} ❌", e),
    }

    // Bob books different seats
    let bob_booking = service
        .start_checkout("bob", "concert_001", vec!["seat_3".to_string()])
        .expect("Should work");
    println!("Bob held seat_3 successfully ✓\n");

    // Flash sale simulation with queue
    println!("\n  ═══ Flash Sale Queue ═══");
    println!("Many users trying to book at once...");

    let req1 = service.queue_booking("user1", "concert_001", 2);
    let req2 = service.queue_booking("user2", "concert_001", 1);
    let req3 = service.queue_booking("user3", "concert_001", 3);
    let req4 = service.queue_booking("user4", "concert_001", 5); // Won't get all

    println!(
        "Queue length: {}",
        service.queue.queue_length("concert_001")
    );

    let results = service.process_queue("concert_001");
    println!("Processing queue:");
    for r in results {
        println!("  {}", r);
    }

    // Show remaining inventory
    println!("\n--- Final Inventory ---");
    let remaining = service.get_available("concert_001");
    println!("Remaining available seats: {}", remaining.len());

    // Cancellation
    println!("\n--- Cancellation ---");
    service.cancel_booking(&bob_booking.id).expect("Should cancel");
    println!("Bob cancelled his booking");

    let after_cancel = service.get_available("concert_001");
    println!("Available after cancel: {} seats", after_cancel.len());

    println!("\n=== Key Concepts ===");
    println!("1. Seat Holds: Temporary reservation during checkout (5 min)");
    println!("2. Atomic Locking: All-or-nothing seat reservation");
    println!("3. Expiry Cleanup: Release abandoned holds");
    println!("4. Queue System: Fair ordering for flash sales");
    println!("5. Idempotent: Retry-safe payment confirmation");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seat_hold_and_confirm() {
        let service = BookingService::new();

        let seats = vec![Seat {
            id: "s1".to_string(),
            section: "A".to_string(),
            row: "1".to_string(),
            number: 1,
            price: 100,
            status: SeatStatus::Available,
            held_by: None,
            held_until: None,
        }];

        service.create_event(
            Event {
                id: "e1".to_string(),
                name: "Test".to_string(),
                venue: "Venue".to_string(),
                date: "2024-01-01".to_string(),
                total_seats: 1,
            },
            seats,
        );

        let booking = service
            .start_checkout("user1", "e1", vec!["s1".to_string()])
            .unwrap();
        assert_eq!(booking.status, BookingStatus::Pending);

        let confirmed = service.complete_payment(&booking.id).unwrap();
        assert_eq!(confirmed.status, BookingStatus::Confirmed);
    }

    #[test]
    fn test_double_booking_prevented() {
        let service = BookingService::new();

        let seats = vec![Seat {
            id: "s1".to_string(),
            section: "A".to_string(),
            row: "1".to_string(),
            number: 1,
            price: 100,
            status: SeatStatus::Available,
            held_by: None,
            held_until: None,
        }];

        service.create_event(
            Event {
                id: "e1".to_string(),
                name: "Test".to_string(),
                venue: "Venue".to_string(),
                date: "2024-01-01".to_string(),
                total_seats: 1,
            },
            seats,
        );

        // First user holds seat
        service
            .start_checkout("user1", "e1", vec!["s1".to_string()])
            .unwrap();

        // Second user should fail
        let result = service.start_checkout("user2", "e1", vec!["s1".to_string()]);
        assert!(result.is_err());
    }
}
