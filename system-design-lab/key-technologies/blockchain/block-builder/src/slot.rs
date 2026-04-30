//! # Slot and Timing
//!
//! Ethereum Proof of Stake timing concepts:
//! - Slot: 12 seconds, one block opportunity
//! - Epoch: 32 slots (6.4 minutes)
//! - Beacon chain tracks slots/epochs

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Slot duration in seconds
pub const SLOT_DURATION_SECS: u64 = 12;

/// Slots per epoch
pub const SLOTS_PER_EPOCH: u64 = 32;

/// Genesis time for mainnet (September 1, 2022, 12:00:35 UTC for Beacon)
/// Using a simpler value for simulation
pub const GENESIS_TIME: u64 = 1606824023; // Dec 1, 2020 (actual Beacon genesis)

/// Slot number (absolute)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot(pub u64);

impl Slot {
    pub fn new(slot: u64) -> Self {
        Slot(slot)
    }

    /// Get epoch this slot belongs to
    pub fn epoch(&self) -> Epoch {
        Epoch(self.0 / SLOTS_PER_EPOCH)
    }

    /// Slot index within epoch (0-31)
    pub fn slot_in_epoch(&self) -> u64 {
        self.0 % SLOTS_PER_EPOCH
    }

    /// Is this the first slot of an epoch?
    pub fn is_epoch_start(&self) -> bool {
        self.slot_in_epoch() == 0
    }

    /// Is this the last slot of an epoch?
    pub fn is_epoch_end(&self) -> bool {
        self.slot_in_epoch() == SLOTS_PER_EPOCH - 1
    }

    /// Next slot
    pub fn next(&self) -> Slot {
        Slot(self.0 + 1)
    }

    /// Previous slot (saturating)
    pub fn prev(&self) -> Slot {
        Slot(self.0.saturating_sub(1))
    }

    /// Start time of this slot
    pub fn start_time(&self, genesis: u64) -> u64 {
        genesis + self.0 * SLOT_DURATION_SECS
    }

    /// End time of this slot
    pub fn end_time(&self, genesis: u64) -> u64 {
        self.start_time(genesis) + SLOT_DURATION_SECS
    }
}

impl std::fmt::Display for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Slot({})", self.0)
    }
}

/// Epoch number
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Epoch(pub u64);

impl Epoch {
    pub fn new(epoch: u64) -> Self {
        Epoch(epoch)
    }

    /// First slot of this epoch
    pub fn start_slot(&self) -> Slot {
        Slot(self.0 * SLOTS_PER_EPOCH)
    }

    /// Last slot of this epoch
    pub fn end_slot(&self) -> Slot {
        Slot(self.0 * SLOTS_PER_EPOCH + SLOTS_PER_EPOCH - 1)
    }

    /// All slots in this epoch
    pub fn slots(&self) -> impl Iterator<Item = Slot> {
        let start = self.start_slot().0;
        let end = self.end_slot().0;
        (start..=end).map(Slot)
    }

    /// Next epoch
    pub fn next(&self) -> Epoch {
        Epoch(self.0 + 1)
    }

    /// Previous epoch
    pub fn prev(&self) -> Epoch {
        Epoch(self.0.saturating_sub(1))
    }
}

impl std::fmt::Display for Epoch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Epoch({})", self.0)
    }
}

/// Clock for tracking slots
pub struct SlotClock {
    /// Genesis timestamp
    genesis_time: u64,
    /// When simulation started (for simulated time)
    start_instant: Instant,
    /// Starting slot (for simulation)
    start_slot: Slot,
    /// Use real time or simulated?
    use_real_time: bool,
}

impl SlotClock {
    /// Create clock using real time
    pub fn real_time(genesis_time: u64) -> Self {
        SlotClock {
            genesis_time,
            start_instant: Instant::now(),
            start_slot: Slot(0),
            use_real_time: true,
        }
    }

    /// Create clock for simulation starting at slot
    pub fn simulated(start_slot: Slot) -> Self {
        SlotClock {
            genesis_time: current_timestamp(),
            start_instant: Instant::now(),
            start_slot,
            use_real_time: false,
        }
    }

    /// Create clock starting now at slot 0
    pub fn new_simulation() -> Self {
        Self::simulated(Slot(0))
    }

    /// Get current slot
    pub fn current_slot(&self) -> Slot {
        if self.use_real_time {
            let now = current_timestamp();
            if now < self.genesis_time {
                return Slot(0);
            }
            Slot((now - self.genesis_time) / SLOT_DURATION_SECS)
        } else {
            // Simulated: advance based on elapsed time
            let elapsed = self.start_instant.elapsed().as_secs();
            Slot(self.start_slot.0 + elapsed / SLOT_DURATION_SECS)
        }
    }

    /// Get current epoch
    pub fn current_epoch(&self) -> Epoch {
        self.current_slot().epoch()
    }

    /// Time remaining in current slot
    pub fn time_in_slot(&self) -> Duration {
        let elapsed = if self.use_real_time {
            let now = current_timestamp();
            (now - self.genesis_time) % SLOT_DURATION_SECS
        } else {
            self.start_instant.elapsed().as_secs() % SLOT_DURATION_SECS
        };
        Duration::from_secs(elapsed)
    }

    /// Time until next slot
    pub fn time_until_next_slot(&self) -> Duration {
        Duration::from_secs(SLOT_DURATION_SECS) - self.time_in_slot()
    }

    /// Check if we're in the first third of the slot (proposal window)
    pub fn is_proposal_window(&self) -> bool {
        self.time_in_slot().as_secs() < SLOT_DURATION_SECS / 3
    }

    /// Check if we're in the middle third (attestation window)
    pub fn is_attestation_window(&self) -> bool {
        let secs = self.time_in_slot().as_secs();
        secs >= SLOT_DURATION_SECS / 3 && secs < 2 * SLOT_DURATION_SECS / 3
    }

    /// Genesis time
    pub fn genesis_time(&self) -> u64 {
        self.genesis_time
    }
}

/// Slot timing breakdown within a slot
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotPhase {
    /// 0-4s: Block proposal
    Proposal,
    /// 4-8s: Attestations
    Attestation,
    /// 8-12s: Aggregation
    Aggregation,
}

impl SlotPhase {
    pub fn from_time_in_slot(secs: u64) -> Self {
        if secs < 4 {
            SlotPhase::Proposal
        } else if secs < 8 {
            SlotPhase::Attestation
        } else {
            SlotPhase::Aggregation
        }
    }
}

/// Get current unix timestamp
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Builder timing - when different actions should happen
#[derive(Debug, Clone)]
pub struct BuilderTiming {
    /// When to start building for a slot
    pub build_start_offset: Duration,
    /// When to submit to relay
    pub submit_deadline: Duration,
    /// Latest time to accept bundles
    pub bundle_deadline: Duration,
}

impl Default for BuilderTiming {
    fn default() -> Self {
        BuilderTiming {
            // Start building at the beginning of the slot
            build_start_offset: Duration::from_secs(0),
            // Submit by 4 seconds into the slot
            submit_deadline: Duration::from_secs(4),
            // Accept bundles until 3 seconds
            bundle_deadline: Duration::from_secs(3),
        }
    }
}

/// Slot schedule for simulation
pub struct SlotScheduler {
    /// Clock
    clock: SlotClock,
    /// Last processed slot
    last_processed: Option<Slot>,
}

impl SlotScheduler {
    pub fn new(clock: SlotClock) -> Self {
        SlotScheduler {
            clock,
            last_processed: None,
        }
    }

    /// Check if there's a new slot to process
    pub fn next_slot(&mut self) -> Option<Slot> {
        let current = self.clock.current_slot();

        match self.last_processed {
            None => {
                self.last_processed = Some(current);
                Some(current)
            }
            Some(last) if current > last => {
                self.last_processed = Some(current);
                Some(current)
            }
            _ => None,
        }
    }

    /// Current slot
    pub fn current_slot(&self) -> Slot {
        self.clock.current_slot()
    }

    /// Force advance to a specific slot (for testing)
    pub fn advance_to(&mut self, slot: Slot) {
        self.last_processed = Some(slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_epoch() {
        assert_eq!(Slot(0).epoch(), Epoch(0));
        assert_eq!(Slot(31).epoch(), Epoch(0));
        assert_eq!(Slot(32).epoch(), Epoch(1));
        assert_eq!(Slot(64).epoch(), Epoch(2));
    }

    #[test]
    fn test_slot_in_epoch() {
        assert_eq!(Slot(0).slot_in_epoch(), 0);
        assert_eq!(Slot(31).slot_in_epoch(), 31);
        assert_eq!(Slot(32).slot_in_epoch(), 0);
        assert_eq!(Slot(33).slot_in_epoch(), 1);
    }

    #[test]
    fn test_epoch_slots() {
        let epoch = Epoch(1);
        let slots: Vec<Slot> = epoch.slots().collect();
        assert_eq!(slots.len(), 32);
        assert_eq!(slots[0], Slot(32));
        assert_eq!(slots[31], Slot(63));
    }

    #[test]
    fn test_slot_boundaries() {
        assert!(Slot(0).is_epoch_start());
        assert!(Slot(32).is_epoch_start());
        assert!(!Slot(1).is_epoch_start());

        assert!(Slot(31).is_epoch_end());
        assert!(Slot(63).is_epoch_end());
        assert!(!Slot(30).is_epoch_end());
    }

    #[test]
    fn test_simulated_clock() {
        let clock = SlotClock::new_simulation();
        let slot = clock.current_slot();
        // Should start at 0
        assert_eq!(slot, Slot(0));
    }

    #[test]
    fn test_slot_time() {
        let genesis = 1000000;
        let slot = Slot(10);
        assert_eq!(slot.start_time(genesis), genesis + 120);
        assert_eq!(slot.end_time(genesis), genesis + 132);
    }

    #[test]
    fn test_slot_phase() {
        assert_eq!(SlotPhase::from_time_in_slot(0), SlotPhase::Proposal);
        assert_eq!(SlotPhase::from_time_in_slot(3), SlotPhase::Proposal);
        assert_eq!(SlotPhase::from_time_in_slot(4), SlotPhase::Attestation);
        assert_eq!(SlotPhase::from_time_in_slot(7), SlotPhase::Attestation);
        assert_eq!(SlotPhase::from_time_in_slot(8), SlotPhase::Aggregation);
        assert_eq!(SlotPhase::from_time_in_slot(11), SlotPhase::Aggregation);
    }
}
