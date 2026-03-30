#![allow(dead_code, unused_variables, unused_imports)]
//! # Linked List
//!
//! Singly and doubly linked list implementations.
//! Uses arena-based allocation (indices into a Vec) to stay safe without raw pointers.

use std::fmt;

// =============================================================================
// Singly Linked List
// =============================================================================

/// A singly linked list node stored in an arena.
struct SinglyNode<T> {
    value: T,
    next: Option<usize>, // index into the arena
}

/// Singly linked list backed by a Vec arena.
pub struct SinglyLinkedList<T> {
    arena: Vec<SinglyNode<T>>,
    head: Option<usize>,
    len: usize,
}

impl<T> SinglyLinkedList<T> {
    pub fn new() -> Self {
        Self {
            arena: Vec::new(),
            head: None,
            len: 0,
        }
    }

    /// Push to the front — O(1).
    pub fn push_front(&mut self, value: T) {
        let idx = self.arena.len();
        self.arena.push(SinglyNode {
            value,
            next: self.head,
        });
        self.head = Some(idx);
        self.len += 1;
    }

    /// Pop from the front — O(1).
    pub fn pop_front(&mut self) -> Option<&T> {
        let head_idx = self.head?;
        let node = &self.arena[head_idx];
        self.head = node.next;
        self.len -= 1;
        Some(&node.value)
    }

    /// Push to the back — O(n).
    pub fn push_back(&mut self, value: T) {
        let new_idx = self.arena.len();
        self.arena.push(SinglyNode {
            value,
            next: None,
        });

        if self.head.is_none() {
            self.head = Some(new_idx);
        } else {
            // Walk to the last node
            let mut cur = self.head.unwrap();
            while let Some(next) = self.arena[cur].next {
                cur = next;
            }
            self.arena[cur].next = Some(new_idx);
        }
        self.len += 1;
    }

    /// Search for a value — O(n).
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        let mut cur = self.head;
        while let Some(idx) = cur {
            if &self.arena[idx].value == value {
                return true;
            }
            cur = self.arena[idx].next;
        }
        false
    }

    /// Reverse the list in place — O(n).
    pub fn reverse(&mut self) {
        let mut prev = None;
        let mut cur = self.head;
        while let Some(idx) = cur {
            let next = self.arena[idx].next;
            self.arena[idx].next = prev;
            prev = cur;
            cur = next;
        }
        self.head = prev;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Collect values into a Vec for display/testing.
    pub fn to_vec(&self) -> Vec<&T> {
        let mut result = Vec::new();
        let mut cur = self.head;
        while let Some(idx) = cur {
            result.push(&self.arena[idx].value);
            cur = self.arena[idx].next;
        }
        result
    }
}

impl<T: fmt::Display> fmt::Display for SinglyLinkedList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut cur = self.head;
        while let Some(idx) = cur {
            write!(f, "{}", self.arena[idx].value)?;
            cur = self.arena[idx].next;
            if cur.is_some() {
                write!(f, " -> ")?;
            }
        }
        Ok(())
    }
}

// =============================================================================
// Doubly Linked List (Arena-based)
// =============================================================================

struct DoublyNode<T> {
    value: T,
    prev: Option<usize>,
    next: Option<usize>,
}

/// Doubly linked list backed by a Vec arena.
/// Supports O(1) push/pop at both ends.
pub struct DoublyLinkedList<T> {
    arena: Vec<DoublyNode<T>>,
    head: Option<usize>,
    tail: Option<usize>,
    len: usize,
    /// Free list for reusing removed node slots.
    free: Vec<usize>,
}

impl<T> DoublyLinkedList<T> {
    pub fn new() -> Self {
        Self {
            arena: Vec::new(),
            head: None,
            tail: None,
            len: 0,
            free: Vec::new(),
        }
    }

    fn alloc(&mut self, value: T) -> usize {
        if let Some(idx) = self.free.pop() {
            self.arena[idx] = DoublyNode {
                value,
                prev: None,
                next: None,
            };
            idx
        } else {
            let idx = self.arena.len();
            self.arena.push(DoublyNode {
                value,
                prev: None,
                next: None,
            });
            idx
        }
    }

    pub fn push_front(&mut self, value: T) {
        let idx = self.alloc(value);
        self.arena[idx].next = self.head;
        if let Some(old_head) = self.head {
            self.arena[old_head].prev = Some(idx);
        } else {
            self.tail = Some(idx);
        }
        self.head = Some(idx);
        self.len += 1;
    }

    pub fn push_back(&mut self, value: T) {
        let idx = self.alloc(value);
        self.arena[idx].prev = self.tail;
        if let Some(old_tail) = self.tail {
            self.arena[old_tail].next = Some(idx);
        } else {
            self.head = Some(idx);
        }
        self.tail = Some(idx);
        self.len += 1;
    }

    pub fn pop_front(&mut self) -> Option<usize> {
        let head_idx = self.head?;
        let next = self.arena[head_idx].next;
        if let Some(next_idx) = next {
            self.arena[next_idx].prev = None;
        } else {
            self.tail = None;
        }
        self.head = next;
        self.len -= 1;
        self.free.push(head_idx);
        Some(head_idx)
    }

    pub fn pop_back(&mut self) -> Option<usize> {
        let tail_idx = self.tail?;
        let prev = self.arena[tail_idx].prev;
        if let Some(prev_idx) = prev {
            self.arena[prev_idx].next = None;
        } else {
            self.head = None;
        }
        self.tail = prev;
        self.len -= 1;
        self.free.push(tail_idx);
        Some(tail_idx)
    }

    /// Move a node to the front (used in LRU cache).
    pub fn move_to_front(&mut self, idx: usize) {
        if self.head == Some(idx) {
            return;
        }
        // Unlink from current position
        let prev = self.arena[idx].prev;
        let next = self.arena[idx].next;
        if let Some(p) = prev {
            self.arena[p].next = next;
        }
        if let Some(n) = next {
            self.arena[n].prev = prev;
        }
        if self.tail == Some(idx) {
            self.tail = prev;
        }
        // Link at front
        self.arena[idx].prev = None;
        self.arena[idx].next = self.head;
        if let Some(old_head) = self.head {
            self.arena[old_head].prev = Some(idx);
        }
        self.head = Some(idx);
    }

    /// Remove a specific node by index — O(1).
    pub fn remove(&mut self, idx: usize) {
        let prev = self.arena[idx].prev;
        let next = self.arena[idx].next;
        if let Some(p) = prev {
            self.arena[p].next = next;
        } else {
            self.head = next;
        }
        if let Some(n) = next {
            self.arena[n].prev = prev;
        } else {
            self.tail = prev;
        }
        self.len -= 1;
        self.free.push(idx);
    }

    pub fn get(&self, idx: usize) -> &T {
        &self.arena[idx].value
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn head_idx(&self) -> Option<usize> {
        self.head
    }

    pub fn tail_idx(&self) -> Option<usize> {
        self.tail
    }

    pub fn to_vec(&self) -> Vec<&T> {
        let mut result = Vec::new();
        let mut cur = self.head;
        while let Some(idx) = cur {
            result.push(&self.arena[idx].value);
            cur = self.arena[idx].next;
        }
        result
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Singly Linked List ===");
    let mut sll = SinglyLinkedList::new();
    sll.push_front(3);
    sll.push_front(2);
    sll.push_front(1);
    sll.push_back(4);
    println!("List: {sll}");
    println!("Contains 3: {}", sll.contains(&3));
    sll.reverse();
    println!("Reversed: {sll}");

    println!("\n=== Doubly Linked List ===");
    let mut dll = DoublyLinkedList::new();
    dll.push_back(1);
    dll.push_back(2);
    dll.push_back(3);
    dll.push_front(0);
    println!("List: {:?}", dll.to_vec());
    dll.pop_front();
    println!("After pop_front: {:?}", dll.to_vec());
    dll.pop_back();
    println!("After pop_back: {:?}", dll.to_vec());
}
