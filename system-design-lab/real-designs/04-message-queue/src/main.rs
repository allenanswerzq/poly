#![allow(dead_code, unused_variables, unused_imports)]
//! # Message Queue Implementation
//!
//! A simplified Kafka-like message queue demonstrating:
//! - Topics and partitions
//! - Consumer groups
//! - Offset tracking
//! - At-least-once delivery

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

// =============================================================================
// Message Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub key: Option<String>,
    pub value: Vec<u8>,
    pub timestamp: u64,
    pub headers: HashMap<String, String>,
}

impl Message {
    pub fn new(value: Vec<u8>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            key: None,
            value,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            headers: HashMap::new(),
        }
    }

    pub fn with_key(mut self, key: &str) -> Self {
        self.key = Some(key.to_string());
        self
    }

    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }
}

#[derive(Debug, Clone)]
pub struct RecordMetadata {
    pub topic: String,
    pub partition: usize,
    pub offset: u64,
    pub timestamp: u64,
}

// =============================================================================
// Partition
// =============================================================================

/// A single partition (append-only log)
pub struct Partition {
    id: usize,
    messages: RwLock<Vec<Message>>,
    high_watermark: AtomicU64,
}

impl Partition {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            messages: RwLock::new(Vec::new()),
            high_watermark: AtomicU64::new(0),
        }
    }

    /// Append a message and return its offset
    pub fn append(&self, message: Message) -> u64 {
        let mut messages = self.messages.write();
        let offset = messages.len() as u64;
        messages.push(message);
        self.high_watermark.store(offset + 1, Ordering::SeqCst);
        offset
    }

    /// Read messages from offset
    pub fn read(&self, offset: u64, max_messages: usize) -> Vec<(u64, Message)> {
        let messages = self.messages.read();
        let start = offset as usize;

        if start >= messages.len() {
            return vec![];
        }

        messages[start..]
            .iter()
            .take(max_messages)
            .enumerate()
            .map(|(i, msg)| (start as u64 + i as u64, msg.clone()))
            .collect()
    }

    pub fn high_watermark(&self) -> u64 {
        self.high_watermark.load(Ordering::SeqCst)
    }

    pub fn id(&self) -> usize {
        self.id
    }
}

// =============================================================================
// Topic
// =============================================================================

/// A topic containing multiple partitions
pub struct Topic {
    name: String,
    partitions: Vec<Arc<Partition>>,
}

impl Topic {
    pub fn new(name: &str, num_partitions: usize) -> Self {
        let partitions = (0..num_partitions)
            .map(|id| Arc::new(Partition::new(id)))
            .collect();

        Self {
            name: name.to_string(),
            partitions,
        }
    }

    /// Get partition for a key (using hash partitioning)
    pub fn get_partition(&self, key: Option<&str>) -> &Arc<Partition> {
        let partition_id = match key {
            Some(k) => Self::hash_key(k) % self.partitions.len(),
            None => rand::random::<usize>() % self.partitions.len(),
        };
        &self.partitions[partition_id]
    }

    pub fn get_partition_by_id(&self, id: usize) -> Option<&Arc<Partition>> {
        self.partitions.get(id)
    }

    fn hash_key(key: &str) -> usize {
        let mut hash: usize = 5381;
        for byte in key.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as usize);
        }
        hash
    }

    pub fn num_partitions(&self) -> usize {
        self.partitions.len()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

// =============================================================================
// Broker
// =============================================================================

/// The message broker managing topics
pub struct Broker {
    topics: DashMap<String, Arc<Topic>>,
    consumer_groups: DashMap<String, Arc<ConsumerGroup>>,
}

impl Broker {
    pub fn new() -> Self {
        Self {
            topics: DashMap::new(),
            consumer_groups: DashMap::new(),
        }
    }

    /// Create a new topic
    pub fn create_topic(&self, name: &str, num_partitions: usize) -> Arc<Topic> {
        let topic = Arc::new(Topic::new(name, num_partitions));
        self.topics.insert(name.to_string(), Arc::clone(&topic));
        topic
    }

    pub fn get_topic(&self, name: &str) -> Option<Arc<Topic>> {
        self.topics.get(name).map(|t| Arc::clone(&t))
    }

    /// Get or create a consumer group
    pub fn get_or_create_consumer_group(&self, name: &str) -> Arc<ConsumerGroup> {
        self.consumer_groups
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(ConsumerGroup::new(name)))
            .clone()
    }
}

impl Default for Broker {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Producer
// =============================================================================

/// A message producer
pub struct Producer {
    broker: Arc<Broker>,
}

impl Producer {
    pub fn new(broker: Arc<Broker>) -> Self {
        Self { broker }
    }

    /// Send a message to a topic
    pub fn send(&self, topic: &str, message: Message) -> Option<RecordMetadata> {
        let topic_ref = self.broker.get_topic(topic)?;
        let partition = topic_ref.get_partition(message.key.as_deref());
        let offset = partition.append(message.clone());

        Some(RecordMetadata {
            topic: topic.to_string(),
            partition: partition.id(),
            offset,
            timestamp: message.timestamp,
        })
    }

    /// Send with explicit partition
    pub fn send_to_partition(
        &self,
        topic: &str,
        partition_id: usize,
        message: Message,
    ) -> Option<RecordMetadata> {
        let topic_ref = self.broker.get_topic(topic)?;
        let partition = topic_ref.get_partition_by_id(partition_id)?;
        let offset = partition.append(message.clone());

        Some(RecordMetadata {
            topic: topic.to_string(),
            partition: partition_id,
            offset,
            timestamp: message.timestamp,
        })
    }
}

// =============================================================================
// Consumer Group
// =============================================================================

/// Tracks consumer group state
pub struct ConsumerGroup {
    name: String,
    /// Offset tracking: topic -> partition -> offset
    committed_offsets: DashMap<String, DashMap<usize, u64>>,
    /// Partition assignments: consumer_id -> (topic, partitions)
    assignments: Mutex<HashMap<String, Vec<(String, usize)>>>,
}

impl ConsumerGroup {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            committed_offsets: DashMap::new(),
            assignments: Mutex::new(HashMap::new()),
        }
    }

    /// Commit offset for a partition
    pub fn commit_offset(&self, topic: &str, partition: usize, offset: u64) {
        self.committed_offsets
            .entry(topic.to_string())
            .or_default()
            .insert(partition, offset);
    }

    /// Get committed offset for a partition
    pub fn get_committed_offset(&self, topic: &str, partition: usize) -> u64 {
        self.committed_offsets
            .get(topic)
            .and_then(|partitions| partitions.get(&partition).map(|o| *o))
            .unwrap_or(0)
    }

    /// Simple round-robin partition assignment
    pub fn assign_partitions(&self, consumer_id: &str, topic: &str, partitions: Vec<usize>) {
        let mut assignments = self.assignments.lock();
        let consumer_assignments = assignments.entry(consumer_id.to_string()).or_default();

        for partition in partitions {
            consumer_assignments.push((topic.to_string(), partition));
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

// =============================================================================
// Consumer
// =============================================================================

/// A message consumer
pub struct Consumer {
    id: String,
    broker: Arc<Broker>,
    group: Arc<ConsumerGroup>,
    subscriptions: Mutex<Vec<(String, usize)>>, // (topic, partition)
}

impl Consumer {
    pub fn new(broker: Arc<Broker>, group: Arc<ConsumerGroup>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            broker,
            group,
            subscriptions: Mutex::new(vec![]),
        }
    }

    /// Subscribe to specific partitions
    pub fn subscribe(&self, topic: &str, partitions: Vec<usize>) {
        let mut subs = self.subscriptions.lock();
        for partition in &partitions {
            subs.push((topic.to_string(), *partition));
        }
        self.group.assign_partitions(&self.id, topic, partitions);
    }

    /// Poll for messages (returns messages from all subscribed partitions)
    pub fn poll(&self, max_messages: usize) -> Vec<ConsumerRecord> {
        let subs = self.subscriptions.lock();
        let mut records = vec![];

        for (topic, partition) in subs.iter() {
            if let Some(topic_ref) = self.broker.get_topic(topic) {
                if let Some(partition_ref) = topic_ref.get_partition_by_id(*partition) {
                    let offset = self.group.get_committed_offset(topic, *partition);

                    let messages = partition_ref.read(offset, max_messages);
                    for (msg_offset, message) in messages {
                        records.push(ConsumerRecord {
                            topic: topic.clone(),
                            partition: *partition,
                            offset: msg_offset,
                            message,
                        });
                    }
                }
            }
        }

        records
    }

    /// Commit processed offsets
    pub fn commit(&self, records: &[ConsumerRecord]) {
        for record in records {
            // Commit offset + 1 (next message to read)
            self.group
                .commit_offset(&record.topic, record.partition, record.offset + 1);
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone)]
pub struct ConsumerRecord {
    pub topic: String,
    pub partition: usize,
    pub offset: u64,
    pub message: Message,
}

// =============================================================================
// Demonstration
// =============================================================================

fn main() {
    println!("=== Message Queue Demo ===\n");

    // Create broker
    let broker = Arc::new(Broker::new());

    // Create topic with 3 partitions
    println!("\n  ═══ Creating Topic ═══");
    broker.create_topic("orders", 3);
    println!("Created topic 'orders' with 3 partitions\n");

    // Create producer
    let producer = Producer::new(Arc::clone(&broker));

    // Send messages with keys (for partitioning)
    println!("\n  ═══ Producing Messages ═══");
    let orders = vec![
        (
            "order-1",
            "customer-a",
            r#"{"item": "laptop", "price": 999}"#,
        ),
        (
            "order-2",
            "customer-b",
            r#"{"item": "phone", "price": 599}"#,
        ),
        (
            "order-3",
            "customer-a",
            r#"{"item": "tablet", "price": 399}"#,
        ),
        (
            "order-4",
            "customer-c",
            r#"{"item": "watch", "price": 299}"#,
        ),
        (
            "order-5",
            "customer-b",
            r#"{"item": "earbuds", "price": 149}"#,
        ),
    ];

    for (order_id, customer, payload) in &orders {
        let msg = Message::new(payload.as_bytes().to_vec())
            .with_key(customer)
            .with_header("order_id", order_id);

        let metadata = producer.send("orders", msg).unwrap();
        println!(
            "  Sent {} -> partition {}, offset {}",
            order_id, metadata.partition, metadata.offset
        );
    }

    // Same customer's orders should go to same partition
    println!("\nNote: Same customer's orders go to same partition (ordering preserved)");

    // Create consumer group
    println!("\n--- Consumer Group Setup ---");
    let group = broker.get_or_create_consumer_group("order-processors");

    // Create two consumers
    let consumer1 = Consumer::new(Arc::clone(&broker), Arc::clone(&group));
    let consumer2 = Consumer::new(Arc::clone(&broker), Arc::clone(&group));

    // Assign partitions (simulating rebalance)
    consumer1.subscribe("orders", vec![0, 1]);
    consumer2.subscribe("orders", vec![2]);

    println!(
        "Consumer {} assigned partitions [0, 1]",
        &consumer1.id()[..8]
    );
    println!("Consumer {} assigned partition [2]", &consumer2.id()[..8]);

    // Poll and process messages
    println!("\n--- Consuming Messages ---");

    println!("\nConsumer 1 polling:");
    let records1 = consumer1.poll(10);
    for record in &records1 {
        let payload = String::from_utf8_lossy(&record.message.value);
        println!(
            "  [P{}:O{}] key={:?}, value={}",
            record.partition, record.offset, record.message.key, payload
        );
    }
    // Commit after processing
    consumer1.commit(&records1);
    println!("  Committed {} records", records1.len());

    println!("\nConsumer 2 polling:");
    let records2 = consumer2.poll(10);
    for record in &records2 {
        let payload = String::from_utf8_lossy(&record.message.value);
        println!(
            "  [P{}:O{}] key={:?}, value={}",
            record.partition, record.offset, record.message.key, payload
        );
    }
    consumer2.commit(&records2);
    println!("  Committed {} records", records2.len());

    // Poll again - should get no messages (already committed)
    println!("\n--- Polling Again (after commit) ---");
    let records3 = consumer1.poll(10);
    let records4 = consumer2.poll(10);
    println!("Consumer 1 got {} new messages", records3.len());
    println!("Consumer 2 got {} new messages", records4.len());

    // Produce more messages
    println!("\n--- Producing More Messages ---");
    let msg = Message::new(b"new order".to_vec()).with_key("customer-a");
    let metadata = producer.send("orders", msg).unwrap();
    println!(
        "Sent new order -> partition {}, offset {}",
        metadata.partition, metadata.offset
    );

    // Poll again - should get new message
    println!("\n--- Polling After New Message ---");
    let new_records = consumer1.poll(10);
    for record in &new_records {
        println!("Consumer 1 got: [P{}:O{}]", record.partition, record.offset);
    }

    // Demo at-least-once delivery
    println!("\n--- At-Least-Once Delivery Demo ---");
    println!("If consumer crashes before committing, messages will be redelivered");
    println!("This ensures no message is lost, but duplicates are possible");
    println!("Use idempotent processing to handle duplicates");

    println!("\n=== Demo Complete ===");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_ordering() {
        let partition = Partition::new(0);

        partition.append(Message::new(b"msg1".to_vec()));
        partition.append(Message::new(b"msg2".to_vec()));
        partition.append(Message::new(b"msg3".to_vec()));

        let messages = partition.read(0, 10);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].0, 0);
        assert_eq!(messages[1].0, 1);
        assert_eq!(messages[2].0, 2);
    }

    #[test]
    fn test_key_partitioning() {
        let broker = Arc::new(Broker::new());
        broker.create_topic("test", 3);

        let producer = Producer::new(broker);

        // Same key should go to same partition
        let msg1 = Message::new(b"v1".to_vec()).with_key("same-key");
        let msg2 = Message::new(b"v2".to_vec()).with_key("same-key");

        let meta1 = producer.send("test", msg1).unwrap();
        let meta2 = producer.send("test", msg2).unwrap();

        assert_eq!(meta1.partition, meta2.partition);
    }

    #[test]
    fn test_consumer_group_offset_tracking() {
        let group = ConsumerGroup::new("test-group");

        group.commit_offset("topic", 0, 5);
        assert_eq!(group.get_committed_offset("topic", 0), 5);

        group.commit_offset("topic", 0, 10);
        assert_eq!(group.get_committed_offset("topic", 0), 10);
    }
}
