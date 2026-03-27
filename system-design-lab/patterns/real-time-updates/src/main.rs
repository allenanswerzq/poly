#![allow(dead_code, unused_variables, unused_imports, clippy::all)]
//! # Real-Time Updates Pattern Demos
//!
//! This module demonstrates patterns for pushing real-time updates to clients:
//! 1. Pub/Sub Channel System
//! 2. Event Broadcasting with Subscribers
//! 3. Presence Tracking (Online/Offline)
//! 4. Message Ordering with Sequence Numbers

use dashmap::DashMap;
use futures::channel::mpsc;
use futures::StreamExt;
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// =============================================================================
// Pattern 1: Pub/Sub Channels
// =============================================================================
// Core pattern for real-time: Publishers send to channels, subscribers receive

#[derive(Clone, Debug)]
struct Message {
    channel: String,
    payload: String,
    sequence: u64,
    timestamp: Instant,
}

struct Subscriber {
    id: String,
    channels: HashSet<String>,
    inbox: Mutex<Vec<Message>>,
}

impl Subscriber {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            channels: HashSet::new(),
            inbox: Mutex::new(Vec::new()),
        }
    }

    fn receive(&self, msg: Message) {
        self.inbox.lock().push(msg);
    }

    fn poll(&self) -> Vec<Message> {
        std::mem::take(&mut *self.inbox.lock())
    }
}

struct PubSub {
    channels: DashMap<String, HashSet<String>>, // channel -> subscriber_ids
    subscribers: DashMap<String, Arc<Subscriber>>,
    sequence: AtomicU64,
    message_count: AtomicU64,
}

impl PubSub {
    fn new() -> Self {
        Self {
            channels: DashMap::new(),
            subscribers: DashMap::new(),
            sequence: AtomicU64::new(0),
            message_count: AtomicU64::new(0),
        }
    }

    fn subscribe(&self, subscriber_id: &str, channel: &str) {
        // Add subscriber if not exists
        let sub = self
            .subscribers
            .entry(subscriber_id.to_string())
            .or_insert_with(|| Arc::new(Subscriber::new(subscriber_id)));

        // This is a workaround since we can't mutate through Arc
        // In real code, you'd use interior mutability properly

        // Add to channel's subscriber list
        self.channels
            .entry(channel.to_string())
            .or_default()
            .insert(subscriber_id.to_string());
    }

    fn unsubscribe(&self, subscriber_id: &str, channel: &str) {
        if let Some(mut subs) = self.channels.get_mut(channel) {
            subs.remove(subscriber_id);
        }
    }

    fn publish(&self, channel: &str, payload: String) {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        self.message_count.fetch_add(1, Ordering::SeqCst);

        let msg = Message {
            channel: channel.to_string(),
            payload,
            sequence: seq,
            timestamp: Instant::now(),
        };

        // Deliver to all subscribers of this channel
        if let Some(subscriber_ids) = self.channels.get(channel) {
            for sub_id in subscriber_ids.iter() {
                if let Some(sub) = self.subscribers.get(sub_id) {
                    sub.receive(msg.clone());
                }
            }
        }
    }

    fn get_subscriber(&self, id: &str) -> Option<Arc<Subscriber>> {
        self.subscribers.get(id).map(|s| Arc::clone(&s))
    }

    fn stats(&self) -> (usize, usize, u64) {
        (
            self.channels.len(),
            self.subscribers.len(),
            self.message_count.load(Ordering::SeqCst),
        )
    }
}

// =============================================================================
// Pattern 2: Presence System
// =============================================================================
// Track who's online, notify others of status changes

#[derive(Clone, Debug, PartialEq)]
enum PresenceStatus {
    Online,
    Away,
    Offline,
}

#[derive(Clone, Debug)]
struct PresenceUpdate {
    user_id: String,
    status: PresenceStatus,
    timestamp: Instant,
}

struct PresenceSystem {
    status: DashMap<String, (PresenceStatus, Instant)>,
    watchers: DashMap<String, HashSet<String>>, // user -> set of users watching them
    updates: Mutex<Vec<PresenceUpdate>>,
    heartbeat_timeout: Duration,
}

impl PresenceSystem {
    fn new(heartbeat_timeout: Duration) -> Self {
        Self {
            status: DashMap::new(),
            watchers: DashMap::new(),
            updates: Mutex::new(Vec::new()),
            heartbeat_timeout,
        }
    }

    fn set_online(&self, user_id: &str) {
        let prev = self.status.get(user_id).map(|s| s.0.clone());
        self.status.insert(
            user_id.to_string(),
            (PresenceStatus::Online, Instant::now()),
        );

        if prev != Some(PresenceStatus::Online) {
            self.notify_watchers(user_id, PresenceStatus::Online);
        }
    }

    fn heartbeat(&self, user_id: &str) {
        if let Some(mut entry) = self.status.get_mut(user_id) {
            entry.1 = Instant::now();
        }
    }

    fn set_offline(&self, user_id: &str) {
        self.status.insert(
            user_id.to_string(),
            (PresenceStatus::Offline, Instant::now()),
        );
        self.notify_watchers(user_id, PresenceStatus::Offline);
    }

    fn watch(&self, watcher_id: &str, target_id: &str) {
        self.watchers
            .entry(target_id.to_string())
            .or_default()
            .insert(watcher_id.to_string());
    }

    fn notify_watchers(&self, user_id: &str, status: PresenceStatus) {
        self.updates.lock().push(PresenceUpdate {
            user_id: user_id.to_string(),
            status,
            timestamp: Instant::now(),
        });
    }

    fn get_status(&self, user_id: &str) -> PresenceStatus {
        self.status
            .get(user_id)
            .map(|entry| {
                let (status, last_seen) = entry.value();
                if *status == PresenceStatus::Online && last_seen.elapsed() > self.heartbeat_timeout
                {
                    PresenceStatus::Away // Stale heartbeat
                } else {
                    status.clone()
                }
            })
            .unwrap_or(PresenceStatus::Offline)
    }

    fn get_updates(&self) -> Vec<PresenceUpdate> {
        std::mem::take(&mut *self.updates.lock())
    }

    fn online_count(&self) -> usize {
        self.status
            .iter()
            .filter(|e| e.value().0 == PresenceStatus::Online)
            .count()
    }
}

// =============================================================================
// Pattern 3: Ordered Message Delivery
// =============================================================================
// Ensure messages are processed in order even with concurrent senders

struct OrderedChannel {
    messages: RwLock<Vec<Message>>,
    last_delivered: AtomicU64,
    waiting: Mutex<HashMap<u64, Message>>, // Out-of-order messages waiting
}

impl OrderedChannel {
    fn new() -> Self {
        Self {
            messages: RwLock::new(Vec::new()),
            last_delivered: AtomicU64::new(0),
            waiting: Mutex::new(HashMap::new()),
        }
    }

    /// Add message with sequence number
    fn add(&self, msg: Message) {
        let expected = self.last_delivered.load(Ordering::SeqCst) + 1;

        if msg.sequence == expected {
            // In order - deliver immediately
            self.deliver(msg);

            // Check if we can deliver waiting messages
            self.deliver_waiting();
        } else if msg.sequence > expected {
            // Out of order - buffer it
            self.waiting.lock().insert(msg.sequence, msg);
        }
        // If sequence < expected, it's a duplicate - ignore
    }

    fn deliver(&self, msg: Message) {
        self.messages.write().push(msg.clone());
        self.last_delivered.store(msg.sequence, Ordering::SeqCst);
    }

    fn deliver_waiting(&self) {
        let mut waiting = self.waiting.lock();
        loop {
            let next = self.last_delivered.load(Ordering::SeqCst) + 1;
            if let Some(msg) = waiting.remove(&next) {
                self.deliver(msg);
            } else {
                break;
            }
        }
    }

    fn get_messages(&self) -> Vec<Message> {
        self.messages.read().clone()
    }
}

// =============================================================================
// Pattern 4: Fan-Out (Broadcast to Many)
// =============================================================================
// Efficiently deliver one message to many subscribers

struct Broadcaster {
    subscribers: DashMap<String, mpsc::UnboundedSender<String>>,
    broadcast_count: AtomicU64,
    total_deliveries: AtomicU64,
}

impl Broadcaster {
    fn new() -> Self {
        Self {
            subscribers: DashMap::new(),
            broadcast_count: AtomicU64::new(0),
            total_deliveries: AtomicU64::new(0),
        }
    }

    fn subscribe(&self, id: &str) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded();
        self.subscribers.insert(id.to_string(), tx);
        rx
    }

    fn unsubscribe(&self, id: &str) {
        self.subscribers.remove(id);
    }

    fn broadcast(&self, message: String) {
        self.broadcast_count.fetch_add(1, Ordering::SeqCst);

        let mut to_remove = Vec::new();

        for entry in self.subscribers.iter() {
            if entry.value().unbounded_send(message.clone()).is_ok() {
                self.total_deliveries.fetch_add(1, Ordering::SeqCst);
            } else {
                // Channel closed - subscriber disconnected
                to_remove.push(entry.key().clone());
            }
        }

        // Clean up disconnected subscribers
        for id in to_remove {
            self.subscribers.remove(&id);
        }
    }

    fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    fn stats(&self) -> (u64, u64) {
        (
            self.broadcast_count.load(Ordering::SeqCst),
            self.total_deliveries.load(Ordering::SeqCst),
        )
    }
}

// =============================================================================
// Demo
// =============================================================================

fn main() {
    println!("=== Real-Time Updates Pattern Demos ===\n");

    // Demo 1: Pub/Sub
    println!("\n  ═══ Pattern 1: Pub/Sub Channels ═══");
    let pubsub = PubSub::new();

    // Create subscribers
    pubsub.subscribe("user_alice", "chat:room1");
    pubsub.subscribe("user_bob", "chat:room1");
    pubsub.subscribe("user_alice", "chat:room2");

    // Publish messages
    pubsub.publish("chat:room1", "Hello room 1!".to_string());
    pubsub.publish("chat:room2", "Hello room 2!".to_string());
    pubsub.publish("chat:room1", "Another message".to_string());

    // Check received messages
    if let Some(alice) = pubsub.get_subscriber("user_alice") {
        let msgs = alice.poll();
        println!("Alice received {} messages:", msgs.len());
        for msg in &msgs {
            println!("  [{}] {}", msg.channel, msg.payload);
        }
    }

    if let Some(bob) = pubsub.get_subscriber("user_bob") {
        let msgs = bob.poll();
        println!("Bob received {} messages", msgs.len());
    }

    let (channels, subs, msg_count) = pubsub.stats();
    println!(
        "Stats: {} channels, {} subscribers, {} messages\n",
        channels, subs, msg_count
    );

    // Demo 2: Presence System
    println!("\n  ═══ Pattern 2: Presence System ═══");
    let presence = PresenceSystem::new(Duration::from_secs(30));

    presence.set_online("alice");
    presence.set_online("bob");
    presence.watch("alice", "bob"); // Alice watches Bob's status

    println!("Alice status: {:?}", presence.get_status("alice"));
    println!("Bob status: {:?}", presence.get_status("bob"));
    println!("Charlie status: {:?}", presence.get_status("charlie"));

    // Bob goes offline
    presence.set_offline("bob");
    let updates = presence.get_updates();
    println!("Presence updates: {}", updates.len());
    for update in updates {
        println!("  {} -> {:?}", update.user_id, update.status);
    }

    println!("Online users: {}\n", presence.online_count());

    // Demo 3: Ordered Message Delivery
    println!("\n  ═══ Pattern 3: Ordered Message Delivery ═══");
    let ordered = OrderedChannel::new();

    // Simulate out-of-order arrival
    let make_msg = |seq: u64, text: &str| Message {
        channel: "test".to_string(),
        payload: text.to_string(),
        sequence: seq,
        timestamp: Instant::now(),
    };

    // Messages arrive out of order
    ordered.add(make_msg(3, "Third"));
    ordered.add(make_msg(1, "First"));
    ordered.add(make_msg(2, "Second"));
    ordered.add(make_msg(5, "Fifth"));
    ordered.add(make_msg(4, "Fourth"));

    let msgs = ordered.get_messages();
    println!("Delivered in order:");
    for msg in msgs {
        println!("  [seq={}] {}", msg.sequence, msg.payload);
    }
    println!();

    // Demo 4: Fan-Out Broadcasting
    println!("\n  ═══ Pattern 4: Fan-Out Broadcasting ═══");
    let broadcaster = Broadcaster::new();

    // Create subscribers (simulated - in real code these would be async)
    let mut rx1 = broadcaster.subscribe("sub1");
    let rx2 = broadcaster.subscribe("sub2");
    let rx3 = broadcaster.subscribe("sub3");

    // Broadcast
    broadcaster.broadcast("Breaking news!".to_string());
    broadcaster.broadcast("More updates!".to_string());

    println!("Subscribers: {}", broadcaster.subscriber_count());

    // Check messages (would normally be async poll)
    let mut count = 0;
    while let Ok(msg) = rx1.try_recv() {
        count += 1;
        println!("sub1 received: {}", msg);
    }

    let (broadcasts, deliveries) = broadcaster.stats();
    println!(
        "Stats: {} broadcasts, {} total deliveries",
        broadcasts, deliveries
    );

    println!("\n=== Key Takeaways ===");
    println!("1. Pub/Sub: Decouple publishers from subscribers");
    println!("2. Presence: Track online status with heartbeats");
    println!("3. Ordering: Buffer out-of-order, deliver in sequence");
    println!("4. Fan-Out: One message to many subscribers efficiently");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pubsub() {
        let ps = PubSub::new();
        ps.subscribe("s1", "ch1");
        ps.publish("ch1", "test".to_string());

        let sub = ps.get_subscriber("s1").unwrap();
        let msgs = sub.poll();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].payload, "test");
    }

    #[test]
    fn test_ordered_delivery() {
        let ch = OrderedChannel::new();

        // Out of order
        ch.add(Message {
            channel: "t".to_string(),
            payload: "2".to_string(),
            sequence: 2,
            timestamp: Instant::now(),
        });
        ch.add(Message {
            channel: "t".to_string(),
            payload: "1".to_string(),
            sequence: 1,
            timestamp: Instant::now(),
        });

        let msgs = ch.get_messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].sequence, 1);
        assert_eq!(msgs[1].sequence, 2);
    }
}
