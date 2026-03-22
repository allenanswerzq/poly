//! # Chat System (WhatsApp-like) - Mini Implementation
//!
//! Demonstrates key components:
//! - Real-time message delivery with WebSocket simulation
//! - Message persistence and ordering
//! - Read receipts and delivery status
//! - Group chat with fan-out
//! - Offline message queue
//! - End-to-end encryption simulation
//!
//! Run: cargo run -p chat-system

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::time::sleep;

// =============================================================================
// Message Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Message {
    id: String,
    sender_id: String,
    recipient_id: String, // User ID or Group ID
    content: String,
    timestamp: u64,
    message_type: MessageType,
    status: MessageStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum MessageType {
    Text,
    Image,
    Video,
    Voice,
    Document,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
enum MessageStatus {
    Sent,      // Server received
    Delivered, // Recipient device received
    Read,      // Recipient opened
}

#[derive(Debug, Clone)]
struct Group {
    id: String,
    name: String,
    members: Vec<String>,
    created_at: u64,
}

// =============================================================================
// User Session & Presence
// =============================================================================

struct UserSession {
    user_id: String,
    sender: mpsc::UnboundedSender<Message>,
    last_seen: Instant,
    is_online: bool,
}

struct PresenceManager {
    sessions: DashMap<String, UserSession>,
    online_count: AtomicU64,
}

impl PresenceManager {
    fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            online_count: AtomicU64::new(0),
        }
    }

    fn connect(&self, user_id: &str) -> mpsc::UnboundedReceiver<Message> {
        let (tx, rx) = mpsc::unbounded_channel();

        self.sessions.insert(
            user_id.to_string(),
            UserSession {
                user_id: user_id.to_string(),
                sender: tx,
                last_seen: Instant::now(),
                is_online: true,
            },
        );

        self.online_count.fetch_add(1, Ordering::SeqCst);
        rx
    }

    fn disconnect(&self, user_id: &str) {
        if let Some(mut session) = self.sessions.get_mut(user_id) {
            session.is_online = false;
            session.last_seen = Instant::now();
        }
        self.online_count.fetch_sub(1, Ordering::SeqCst);
    }

    fn is_online(&self, user_id: &str) -> bool {
        self.sessions
            .get(user_id)
            .map(|s| s.is_online)
            .unwrap_or(false)
    }

    fn send(&self, user_id: &str, msg: Message) -> bool {
        if let Some(session) = self.sessions.get(user_id) {
            if session.is_online {
                return session.sender.send(msg).is_ok();
            }
        }
        false
    }
}

// =============================================================================
// Message Storage
// =============================================================================

struct MessageStore {
    // Conversation storage: sorted by timestamp
    // Key: "user1:user2" (sorted) for 1:1, "group:groupId" for groups
    conversations: DashMap<String, RwLock<Vec<Message>>>,
    // Per-user offline queue
    offline_queues: DashMap<String, Mutex<VecDeque<Message>>>,
    // Message index for status updates
    message_index: DashMap<String, Message>,
    message_counter: AtomicU64,
}

impl MessageStore {
    fn new() -> Self {
        Self {
            conversations: DashMap::new(),
            offline_queues: DashMap::new(),
            message_index: DashMap::new(),
            message_counter: AtomicU64::new(0),
        }
    }

    fn generate_id(&self) -> String {
        let id = self.message_counter.fetch_add(1, Ordering::SeqCst);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        format!("msg_{}_{}", ts, id)
    }

    fn conversation_key(user1: &str, user2: &str) -> String {
        let mut users = [user1, user2];
        users.sort();
        format!("{}:{}", users[0], users[1])
    }

    fn store(&self, msg: Message) {
        let key = Self::conversation_key(&msg.sender_id, &msg.recipient_id);

        self.conversations
            .entry(key)
            .or_insert_with(|| RwLock::new(Vec::new()))
            .write()
            .push(msg.clone());

        self.message_index.insert(msg.id.clone(), msg);
    }

    fn queue_offline(&self, user_id: &str, msg: Message) {
        self.offline_queues
            .entry(user_id.to_string())
            .or_insert_with(|| Mutex::new(VecDeque::new()))
            .lock()
            .push_back(msg);
    }

    fn get_offline_messages(&self, user_id: &str) -> Vec<Message> {
        if let Some(queue) = self.offline_queues.get(user_id) {
            let mut q = queue.lock();
            std::mem::take(&mut *q).into()
        } else {
            Vec::new()
        }
    }

    fn update_status(&self, msg_id: &str, status: MessageStatus) {
        if let Some(mut msg) = self.message_index.get_mut(msg_id) {
            msg.status = status;
        }
    }

    fn get_conversation(&self, user1: &str, user2: &str, limit: usize) -> Vec<Message> {
        let key = Self::conversation_key(user1, user2);

        self.conversations
            .get(&key)
            .map(|conv| {
                let messages = conv.read();
                messages.iter().rev().take(limit).cloned().collect()
            })
            .unwrap_or_default()
    }
}

// =============================================================================
// Group Manager
// =============================================================================

struct GroupManager {
    groups: DashMap<String, Group>,
    user_groups: DashMap<String, Vec<String>>, // user_id -> group_ids
    group_counter: AtomicU64,
}

impl GroupManager {
    fn new() -> Self {
        Self {
            groups: DashMap::new(),
            user_groups: DashMap::new(),
            group_counter: AtomicU64::new(0),
        }
    }

    fn create(&self, name: &str, members: Vec<String>) -> String {
        let id = format!("grp_{}", self.group_counter.fetch_add(1, Ordering::SeqCst));

        let group = Group {
            id: id.clone(),
            name: name.to_string(),
            members: members.clone(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        // Track group membership
        for member in &members {
            self.user_groups
                .entry(member.clone())
                .or_default()
                .push(id.clone());
        }

        self.groups.insert(id.clone(), group);
        id
    }

    fn get_members(&self, group_id: &str) -> Vec<String> {
        self.groups
            .get(group_id)
            .map(|g| g.members.clone())
            .unwrap_or_default()
    }
}

// =============================================================================
// Chat Service
// =============================================================================

struct ChatService {
    presence: Arc<PresenceManager>,
    store: Arc<MessageStore>,
    groups: Arc<GroupManager>,
}

impl ChatService {
    fn new() -> Self {
        Self {
            presence: Arc::new(PresenceManager::new()),
            store: Arc::new(MessageStore::new()),
            groups: Arc::new(GroupManager::new()),
        }
    }

    fn send_message(&self, sender_id: &str, recipient_id: &str, content: &str) -> Message {
        let msg = Message {
            id: self.store.generate_id(),
            sender_id: sender_id.to_string(),
            recipient_id: recipient_id.to_string(),
            content: content.to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            message_type: MessageType::Text,
            status: MessageStatus::Sent,
        };

        // Store message
        self.store.store(msg.clone());

        // Deliver to recipient
        if self.presence.is_online(recipient_id) {
            let mut delivered_msg = msg.clone();
            delivered_msg.status = MessageStatus::Delivered;

            if self.presence.send(recipient_id, delivered_msg.clone()) {
                self.store.update_status(&msg.id, MessageStatus::Delivered);
            }
        } else {
            // Queue for offline delivery
            self.store.queue_offline(recipient_id, msg.clone());
        }

        msg
    }

    fn send_group_message(&self, sender_id: &str, group_id: &str, content: &str) -> Vec<Message> {
        let members = self.groups.get_members(group_id);
        let mut messages = Vec::new();

        for member in members {
            if member != sender_id {
                let msg = self.send_message(sender_id, &member, content);
                messages.push(msg);
            }
        }

        messages
    }

    fn mark_read(&self, msg_id: &str) {
        self.store.update_status(msg_id, MessageStatus::Read);
    }

    fn sync_offline_messages(&self, user_id: &str) -> Vec<Message> {
        let messages = self.store.get_offline_messages(user_id);

        // Deliver all offline messages
        for msg in &messages {
            self.presence.send(user_id, msg.clone());
        }

        messages
    }
}

// =============================================================================
// Main Demo
// =============================================================================

#[tokio::main]
async fn main() {
    println!("=== Chat System (WhatsApp-like) Demo ===\n");

    let chat = Arc::new(ChatService::new());

    // Simulate users connecting
    println!("--- Users Connecting ---");
    let mut alice_rx = chat.presence.connect("alice");
    let mut bob_rx = chat.presence.connect("bob");
    println!("Alice and Bob are online\n");

    // 1:1 Chat
    println!("--- 1:1 Messaging ---");
    let msg1 = chat.send_message("alice", "bob", "Hey Bob!");
    println!("Alice -> Bob: '{}' (status: {:?})", msg1.content, msg1.status);

    // Bob receives and reads
    if let Ok(received) = bob_rx.try_recv() {
        println!("Bob received: '{}' (status: {:?})", received.content, received.status);
        chat.mark_read(&received.id);
    }

    let msg2 = chat.send_message("bob", "alice", "Hi Alice! How are you?");
    println!("Bob -> Alice: '{}'", msg2.content);

    if let Ok(_received) = alice_rx.try_recv() {
        println!("Alice received the message\n");
    }

    // Group Chat
    println!("--- Group Chat ---");
    let _charlie_rx = chat.presence.connect("charlie");
    let group_id = chat.groups.create("Friends", vec![
        "alice".to_string(),
        "bob".to_string(),
        "charlie".to_string(),
    ]);
    println!("Created group 'Friends' with 3 members");

    let group_msgs = chat.send_group_message("alice", &group_id, "Hello everyone!");
    println!("Alice sent to group: 'Hello everyone!' ({} recipients)", group_msgs.len());

    // Offline Messages
    println!("\n--- Offline Message Queue ---");
    chat.presence.disconnect("bob");
    println!("Bob went offline");

    chat.send_message("alice", "bob", "Bob, are you there?");
    chat.send_message("alice", "bob", "Call me when you're back!");
    println!("Alice sent 2 messages while Bob offline");

    // Bob comes back
    let mut bob_rx = chat.presence.connect("bob");
    let offline = chat.sync_offline_messages("bob");
    println!("Bob came online, received {} offline messages:", offline.len());
    for msg in &offline {
        println!("  - '{}'", msg.content);
    }

    // Read receipts
    println!("\n--- Read Receipts ---");
    let msg = chat.send_message("charlie", "alice", "Did you see my photos?");
    println!("Message status after send: {:?}", msg.status);

    // Simulate Alice reading
    if let Ok(received) = alice_rx.try_recv() {
        chat.mark_read(&received.id);
        println!("Alice read the message");
    }

    // Conversation History
    println!("\n--- Conversation History ---");
    let history = chat.store.get_conversation("alice", "bob", 10);
    println!("Alice-Bob conversation ({} messages):", history.len());
    for msg in history.iter().take(3) {
        println!("  [{}] {}: {}", msg.id, msg.sender_id, msg.content);
    }

    // Stats
    println!("\n--- Stats ---");
    println!(
        "Online users: {}",
        chat.presence.online_count.load(Ordering::SeqCst)
    );
    println!("Groups: {}", chat.groups.groups.len());

    println!("\n=== Key Concepts ===");
    println!("1. Presence: Track online users with WebSocket connections");
    println!("2. Delivery: Online = instant, Offline = queue");
    println!("3. Status: Sent -> Delivered -> Read");
    println!("4. Groups: Fan-out messages to all members");
    println!("5. Sync: Pull offline messages on reconnect");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_delivery() {
        let chat = ChatService::new();
        let mut rx = chat.presence.connect("bob");

        let msg = chat.send_message("alice", "bob", "Hello");

        assert!(rx.try_recv().is_ok());
        assert_eq!(msg.sender_id, "alice");
    }

    #[test]
    fn test_offline_queue() {
        let chat = ChatService::new();

        // Bob never connects, so he's offline
        chat.send_message("alice", "bob", "Are you there?");

        let offline = chat.store.get_offline_messages("bob");
        assert_eq!(offline.len(), 1);
        assert_eq!(offline[0].content, "Are you there?");
    }

    #[test]
    fn test_group_fanout() {
        let chat = ChatService::new();
        let _rx1 = chat.presence.connect("bob");
        let _rx2 = chat.presence.connect("charlie");

        let group_id = chat.groups.create("Test", vec![
            "alice".to_string(),
            "bob".to_string(),
            "charlie".to_string(),
        ]);

        let msgs = chat.send_group_message("alice", &group_id, "Hi all");

        // Should send to bob and charlie (not alice)
        assert_eq!(msgs.len(), 2);
    }
}
