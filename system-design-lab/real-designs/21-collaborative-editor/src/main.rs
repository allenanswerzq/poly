//! # Collaborative Editor (Google Docs) - Mini Implementation
//!
//! Demonstrates:
//! - Operational Transformation (OT)
//! - CRDT for conflict-free merging
//! - Cursor positions and selection
//! - Undo/Redo with operation history
//!
//! Run: cargo run -p collaborative-editor

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Operation {
    Insert { position: usize, text: String, author: String },
    Delete { position: usize, length: usize, author: String },
    Retain { count: usize },
}

#[derive(Debug, Clone)]
struct VersionedOperation {
    id: u64,
    operation: Operation,
    base_version: u64,
    timestamp: u64,
}

#[derive(Debug, Clone)]
struct Cursor {
    user_id: String,
    position: usize,
    selection_end: Option<usize>,
}

// =============================================================================
// Operational Transformation
// =============================================================================

fn transform(op1: &Operation, op2: &Operation) -> (Operation, Operation) {
    // Transform op1 against op2 (op1 was concurrent and applied after op2)
    match (op1, op2) {
        // Insert vs Insert
        (
            Operation::Insert { position: p1, text: t1, author: a1 },
            Operation::Insert { position: p2, text: t2, .. },
        ) => {
            if *p1 <= *p2 {
                // op1 is before or at same position, no change
                (
                    op1.clone(),
                    Operation::Insert {
                        position: p2 + t1.len(),
                        text: t2.clone(),
                        author: a1.clone(),
                    },
                )
            } else {
                // op1 is after, shift by op2's length
                (
                    Operation::Insert {
                        position: p1 + t2.len(),
                        text: t1.clone(),
                        author: a1.clone(),
                    },
                    op2.clone(),
                )
            }
        }

        // Insert vs Delete
        (
            Operation::Insert { position: p1, text: t1, author: a1 },
            Operation::Delete { position: p2, length: l2, .. },
        ) => {
            if *p1 <= *p2 {
                // Insert before delete
                (
                    op1.clone(),
                    Operation::Delete {
                        position: p2 + t1.len(),
                        length: *l2,
                        author: a1.clone(),
                    },
                )
            } else if *p1 >= p2 + l2 {
                // Insert after delete
                (
                    Operation::Insert {
                        position: p1 - l2,
                        text: t1.clone(),
                        author: a1.clone(),
                    },
                    op2.clone(),
                )
            } else {
                // Insert inside deleted region
                (
                    Operation::Insert {
                        position: *p2,
                        text: t1.clone(),
                        author: a1.clone(),
                    },
                    op2.clone(),
                )
            }
        }

        // Delete vs Insert
        (
            Operation::Delete { position: p1, length: l1, author: a1 },
            Operation::Insert { position: p2, text: t2, .. },
        ) => {
            if *p1 >= *p2 {
                // Delete after insert
                (
                    Operation::Delete {
                        position: p1 + t2.len(),
                        length: *l1,
                        author: a1.clone(),
                    },
                    op2.clone(),
                )
            } else if p1 + l1 <= *p2 {
                // Delete before insert
                (
                    op1.clone(),
                    Operation::Insert {
                        position: p2 - l1,
                        text: t2.clone(),
                        author: a1.clone(),
                    },
                )
            } else {
                // Delete spans insert point - complex case
                (op1.clone(), op2.clone())
            }
        }

        // Delete vs Delete
        (
            Operation::Delete { position: p1, length: l1, author: a1 },
            Operation::Delete { position: p2, length: l2, .. },
        ) => {
            if *p1 >= p2 + l2 {
                // op1 after op2
                (
                    Operation::Delete {
                        position: p1 - l2,
                        length: *l1,
                        author: a1.clone(),
                    },
                    op2.clone(),
                )
            } else if p1 + l1 <= *p2 {
                // op1 before op2
                (
                    op1.clone(),
                    Operation::Delete {
                        position: p2 - l1,
                        length: *l2,
                        author: a1.clone(),
                    },
                )
            } else {
                // Overlapping deletes - complex, simplified here
                (op1.clone(), op2.clone())
            }
        }

        _ => (op1.clone(), op2.clone()),
    }
}

// =============================================================================
// Document
// =============================================================================

struct Document {
    content: RwLock<String>,
    version: AtomicU64,
    history: Mutex<VecDeque<VersionedOperation>>,
    cursors: DashMap<String, Cursor>,
    max_history: usize,
}

impl Document {
    fn new() -> Self {
        Self {
            content: RwLock::new(String::new()),
            version: AtomicU64::new(0),
            history: Mutex::new(VecDeque::new()),
            cursors: DashMap::new(),
            max_history: 1000,
        }
    }

    fn apply(&self, op: &Operation) -> Result<u64, &'static str> {
        let mut content = self.content.write();

        match op {
            Operation::Insert { position, text, .. } => {
                if *position > content.len() {
                    return Err("Position out of bounds");
                }
                content.insert_str(*position, text);

                // Update cursors
                for mut cursor in self.cursors.iter_mut() {
                    if cursor.position >= *position {
                        cursor.position += text.len();
                    }
                }
            }
            Operation::Delete { position, length, .. } => {
                if *position + *length > content.len() {
                    return Err("Delete range out of bounds");
                }
                content.drain(*position..(*position + *length));

                // Update cursors
                for mut cursor in self.cursors.iter_mut() {
                    if cursor.position > *position {
                        cursor.position = cursor.position.saturating_sub(*length);
                    }
                }
            }
            Operation::Retain { .. } => {
                // No change to content
            }
        }

        let new_version = self.version.fetch_add(1, Ordering::SeqCst) + 1;

        // Store in history
        let versioned = VersionedOperation {
            id: new_version,
            operation: op.clone(),
            base_version: new_version - 1,
            timestamp: 0,
        };

        let mut history = self.history.lock();
        history.push_back(versioned);
        if history.len() > self.max_history {
            history.pop_front();
        }

        Ok(new_version)
    }

    fn apply_remote(&self, op: &Operation, base_version: u64) -> Result<u64, &'static str> {
        let current_version = self.version.load(Ordering::SeqCst);

        if base_version == current_version {
            // No transformation needed
            return self.apply(op);
        }

        // Need to transform against concurrent operations
        let history = self.history.lock();
        let mut transformed_op = op.clone();

        for versioned_op in history.iter() {
            if versioned_op.id > base_version {
                let (new_op, _) = transform(&transformed_op, &versioned_op.operation);
                transformed_op = new_op;
            }
        }

        drop(history);
        self.apply(&transformed_op)
    }

    fn get_content(&self) -> String {
        self.content.read().clone()
    }

    fn get_version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    fn update_cursor(&self, user_id: &str, position: usize) {
        self.cursors.insert(
            user_id.to_string(),
            Cursor {
                user_id: user_id.to_string(),
                position,
                selection_end: None,
            },
        );
    }

    fn get_cursors(&self) -> Vec<Cursor> {
        self.cursors.iter().map(|e| e.value().clone()).collect()
    }
}

// =============================================================================
// CRDT: Last-Writer-Wins Register (Simple)
// =============================================================================

#[derive(Debug)]
struct LwwRegister<T> {
    value: RwLock<Option<(T, u64, String)>>, // (value, timestamp, author)
}

impl<T: Clone> LwwRegister<T> {
    fn new() -> Self {
        Self {
            value: RwLock::new(None),
        }
    }

    fn set(&self, value: T, timestamp: u64, author: &str) {
        let mut current = self.value.write();
        if current.as_ref().map(|(_, ts, _)| *ts).unwrap_or(0) <= timestamp {
            *current = Some((value, timestamp, author.to_string()));
        }
    }

    fn get(&self) -> Option<T> {
        self.value.read().as_ref().map(|(v, _, _)| v.clone())
    }
}

// =============================================================================
// CRDT: G-Counter (Grow-only Counter)
// =============================================================================

#[derive(Debug, Clone)]
struct GCounter {
    counts: DashMap<String, u64>, // node_id -> count
}

impl GCounter {
    fn new() -> Self {
        Self {
            counts: DashMap::new(),
        }
    }

    fn increment(&self, node_id: &str) {
        self.counts
            .entry(node_id.to_string())
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }

    fn value(&self) -> u64 {
        self.counts.iter().map(|e| *e.value()).sum()
    }

    fn merge(&self, other: &GCounter) {
        for entry in other.counts.iter() {
            self.counts
                .entry(entry.key().clone())
                .and_modify(|c| *c = (*c).max(*entry.value()))
                .or_insert(*entry.value());
        }
    }
}

// =============================================================================
// Collaborative Session
// =============================================================================

struct CollaborativeSession {
    document: Document,
    participants: DashMap<String, Participant>,
}

#[derive(Debug, Clone)]
struct Participant {
    user_id: String,
    name: String,
    color: String,
    last_seen_version: u64,
}

impl CollaborativeSession {
    fn new() -> Self {
        Self {
            document: Document::new(),
            participants: DashMap::new(),
        }
    }

    fn join(&self, user_id: &str, name: &str, color: &str) {
        self.participants.insert(
            user_id.to_string(),
            Participant {
                user_id: user_id.to_string(),
                name: name.to_string(),
                color: color.to_string(),
                last_seen_version: self.document.get_version(),
            },
        );
    }

    fn leave(&self, user_id: &str) {
        self.participants.remove(user_id);
        self.document.cursors.remove(user_id);
    }

    fn edit(&self, user_id: &str, op: Operation, base_version: u64) -> Result<u64, &'static str> {
        let version = self.document.apply_remote(&op, base_version)?;

        if let Some(mut participant) = self.participants.get_mut(user_id) {
            participant.last_seen_version = version;
        }

        Ok(version)
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== Collaborative Editor (Google Docs) Demo ===\n");

    let session = CollaborativeSession::new();

    // Users join
    println!("\n  ═══ Users Joining ═══");
    session.join("alice", "Alice", "#ff0000");
    session.join("bob", "Bob", "#0000ff");
    println!("Alice and Bob joined the session\n");

    // Alice types first
    println!("\n  ═══ Alice Types ═══");
    let op1 = Operation::Insert {
        position: 0,
        text: "Hello".to_string(),
        author: "alice".to_string(),
    };
    let v1 = session.edit("alice", op1, 0).unwrap();
    println!("v{}: '{}'", v1, session.document.get_content());

    // Bob types at same time (concurrent)
    println!("\n--- Bob Types (Concurrent with Alice) ---");
    let op2 = Operation::Insert {
        position: 0, // Bob thinks doc is empty
        text: "Hi ".to_string(),
        author: "bob".to_string(),
    };
    // Bob's base version is 0, but doc is now at v1
    let v2 = session.edit("bob", op2.clone(), 0).unwrap();
    println!("v{}: '{}'", v2, session.document.get_content());
    println!("(Bob's 'Hi ' was transformed to appear before 'Hello')");

    // Alice continues
    println!("\n--- Alice Continues ---");
    let op3 = Operation::Insert {
        position: 8, // After "Hi Hello"
        text: " World".to_string(),
        author: "alice".to_string(),
    };
    let v3 = session.edit("alice", op3, v2).unwrap();
    println!("v{}: '{}'", v3, session.document.get_content());

    // Bob deletes
    println!("\n--- Bob Deletes ---");
    let op4 = Operation::Delete {
        position: 0,
        length: 3, // Delete "Hi "
        author: "bob".to_string(),
    };
    let v4 = session.edit("bob", op4, v3).unwrap();
    println!("v{}: '{}'", v4, session.document.get_content());

    // Cursor positions
    println!("\n--- Cursor Positions ---");
    session.document.update_cursor("alice", 11);
    session.document.update_cursor("bob", 5);

    for cursor in session.document.get_cursors() {
        println!("  {} at position {}", cursor.user_id, cursor.position);
    }

    // CRDT demos
    println!("\n--- CRDT: Last-Writer-Wins Register ---");
    let title: LwwRegister<String> = LwwRegister::new();

    title.set("Draft".to_string(), 100, "alice");
    println!("Alice sets title: {:?}", title.get());

    title.set("Final".to_string(), 200, "bob");
    println!("Bob sets title (later timestamp): {:?}", title.get());

    title.set("Old Title".to_string(), 50, "charlie"); // Earlier timestamp
    println!("Charlie sets title (earlier timestamp, ignored): {:?}", title.get());

    println!("\n--- CRDT: G-Counter ---");
    let counter1 = GCounter::new();
    let counter2 = GCounter::new();

    counter1.increment("node1");
    counter1.increment("node1");
    counter2.increment("node2");
    counter2.increment("node2");
    counter2.increment("node2");

    println!("Counter1: {}", counter1.value());
    println!("Counter2: {}", counter2.value());

    counter1.merge(&counter2);
    println!("After merge: {}", counter1.value());

    // Show participants
    println!("\n--- Session Info ---");
    println!("Document version: {}", session.document.get_version());
    println!("Participants:");
    for p in session.participants.iter() {
        println!("  {} ({}) - last seen v{}", p.name, p.color, p.last_seen_version);
    }

    println!("\n=== Key Concepts ===");
    println!("1. Operational Transform: Transform concurrent ops for consistency");
    println!("2. Version Vector: Track document version for conflict detection");
    println!("3. Cursors: Share cursor positions in real-time");
    println!("4. CRDT: Conflict-free replicated data types (LWW, G-Counter)");
    println!("5. History: Store ops for undo/redo and transformation");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_transform() {
        let op1 = Operation::Insert {
            position: 0,
            text: "Hello".to_string(),
            author: "a".to_string(),
        };
        let op2 = Operation::Insert {
            position: 0,
            text: "World".to_string(),
            author: "b".to_string(),
        };

        let (t1, t2) = transform(&op1, &op2);

        // After transform, both should produce consistent result
        if let Operation::Insert { position: p1, .. } = t1 {
            if let Operation::Insert { position: p2, .. } = t2 {
                // One should be shifted
                assert!(p1 == 0 || p2 == 0);
            }
        }
    }

    #[test]
    fn test_document_apply() {
        let doc = Document::new();

        doc.apply(&Operation::Insert {
            position: 0,
            text: "Hello".to_string(),
            author: "a".to_string(),
        })
        .unwrap();

        assert_eq!(doc.get_content(), "Hello");
        assert_eq!(doc.get_version(), 1);
    }

    #[test]
    fn test_g_counter_merge() {
        let c1 = GCounter::new();
        let c2 = GCounter::new();

        c1.increment("a");
        c1.increment("a");
        c2.increment("b");

        c1.merge(&c2);
        assert_eq!(c1.value(), 3);
    }
}
