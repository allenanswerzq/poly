#![allow(dead_code, unused_variables, unused_imports)]
//! # Binary Search Tree & AVL Tree
//!
//! Arena-based BST and self-balancing AVL tree.

use std::fmt;

// =============================================================================
// BST (Arena-based)
// =============================================================================

struct BstNode<K: Ord, V> {
    key: K,
    value: V,
    left: Option<usize>,
    right: Option<usize>,
}

pub struct Bst<K: Ord, V> {
    arena: Vec<BstNode<K, V>>,
    root: Option<usize>,
}

impl<K: Ord + fmt::Display, V: fmt::Display> Bst<K, V> {
    pub fn new() -> Self {
        Self {
            arena: Vec::new(),
            root: None,
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        let new_idx = self.arena.len();
        self.arena.push(BstNode {
            key,
            value,
            left: None,
            right: None,
        });

        if self.root.is_none() {
            self.root = Some(new_idx);
            return;
        }

        let mut cur = self.root.unwrap();
        loop {
            if self.arena[new_idx].key < self.arena[cur].key {
                if let Some(left) = self.arena[cur].left {
                    cur = left;
                } else {
                    self.arena[cur].left = Some(new_idx);
                    return;
                }
            } else if self.arena[new_idx].key > self.arena[cur].key {
                if let Some(right) = self.arena[cur].right {
                    cur = right;
                } else {
                    self.arena[cur].right = Some(new_idx);
                    return;
                }
            } else {
                // Key exists, update value
                self.arena[cur].value = self.arena.pop().unwrap().value;
                return;
            }
        }
    }

    pub fn search(&self, key: &K) -> Option<&V> {
        let mut cur = self.root;
        while let Some(idx) = cur {
            let node = &self.arena[idx];
            if key < &node.key {
                cur = node.left;
            } else if key > &node.key {
                cur = node.right;
            } else {
                return Some(&node.value);
            }
        }
        None
    }

    /// In-order traversal (sorted).
    pub fn inorder(&self) -> Vec<(&K, &V)> {
        let mut result = Vec::new();
        self.inorder_rec(self.root, &mut result);
        result
    }

    fn inorder_rec<'a>(&'a self, node: Option<usize>, result: &mut Vec<(&'a K, &'a V)>) {
        if let Some(idx) = node {
            self.inorder_rec(self.arena[idx].left, result);
            result.push((&self.arena[idx].key, &self.arena[idx].value));
            self.inorder_rec(self.arena[idx].right, result);
        }
    }

    /// Find min key.
    pub fn min(&self) -> Option<(&K, &V)> {
        let mut cur = self.root?;
        while let Some(left) = self.arena[cur].left {
            cur = left;
        }
        Some((&self.arena[cur].key, &self.arena[cur].value))
    }

    /// Find max key.
    pub fn max(&self) -> Option<(&K, &V)> {
        let mut cur = self.root?;
        while let Some(right) = self.arena[cur].right {
            cur = right;
        }
        Some((&self.arena[cur].key, &self.arena[cur].value))
    }
}

// =============================================================================
// AVL Tree (Self-balancing BST)
// =============================================================================

struct AvlNode<K: Ord> {
    key: K,
    left: Option<usize>,
    right: Option<usize>,
    height: i32,
}

pub struct AvlTree<K: Ord> {
    arena: Vec<AvlNode<K>>,
    root: Option<usize>,
}

impl<K: Ord + Clone + fmt::Display> AvlTree<K> {
    pub fn new() -> Self {
        Self {
            arena: Vec::new(),
            root: None,
        }
    }

    fn height(&self, node: Option<usize>) -> i32 {
        node.map_or(0, |idx| self.arena[idx].height)
    }

    fn balance_factor(&self, idx: usize) -> i32 {
        self.height(self.arena[idx].left) - self.height(self.arena[idx].right)
    }

    fn update_height(&mut self, idx: usize) {
        let lh = self.height(self.arena[idx].left);
        let rh = self.height(self.arena[idx].right);
        self.arena[idx].height = 1 + lh.max(rh);
    }

    /// Right rotation:
    ///       y              x
    ///      / \            / \
    ///     x   C   =>    A   y
    ///    / \                / \
    ///   A   B              B   C
    fn rotate_right(&mut self, y: usize) -> usize {
        let x = self.arena[y].left.unwrap();
        let b = self.arena[x].right;
        self.arena[x].right = Some(y);
        self.arena[y].left = b;
        self.update_height(y);
        self.update_height(x);
        x
    }

    /// Left rotation:
    ///     x                y
    ///    / \              / \
    ///   A   y     =>    x   C
    ///      / \         / \
    ///     B   C       A   B
    fn rotate_left(&mut self, x: usize) -> usize {
        let y = self.arena[x].right.unwrap();
        let b = self.arena[y].left;
        self.arena[y].left = Some(x);
        self.arena[x].right = b;
        self.update_height(x);
        self.update_height(y);
        y
    }

    fn rebalance(&mut self, idx: usize) -> usize {
        self.update_height(idx);
        let bf = self.balance_factor(idx);

        if bf > 1 {
            let left = self.arena[idx].left.unwrap();
            if self.balance_factor(left) < 0 {
                // Left-Right case
                self.arena[idx].left = Some(self.rotate_left(left));
            }
            return self.rotate_right(idx);
        }

        if bf < -1 {
            let right = self.arena[idx].right.unwrap();
            if self.balance_factor(right) > 0 {
                // Right-Left case
                self.arena[idx].right = Some(self.rotate_right(right));
            }
            return self.rotate_left(idx);
        }

        idx
    }

    pub fn insert(&mut self, key: K) {
        let new_root = self.insert_rec(self.root, key);
        self.root = Some(new_root);
    }

    fn insert_rec(&mut self, node: Option<usize>, key: K) -> usize {
        let Some(idx) = node else {
            let new_idx = self.arena.len();
            self.arena.push(AvlNode {
                key,
                left: None,
                right: None,
                height: 1,
            });
            return new_idx;
        };

        if key < self.arena[idx].key {
            let left = self.insert_rec(self.arena[idx].left, key);
            self.arena[idx].left = Some(left);
        } else if key > self.arena[idx].key {
            let right = self.insert_rec(self.arena[idx].right, key);
            self.arena[idx].right = Some(right);
        } else {
            return idx; // duplicate
        }

        self.rebalance(idx)
    }

    pub fn contains(&self, key: &K) -> bool {
        let mut cur = self.root;
        while let Some(idx) = cur {
            if key < &self.arena[idx].key {
                cur = self.arena[idx].left;
            } else if key > &self.arena[idx].key {
                cur = self.arena[idx].right;
            } else {
                return true;
            }
        }
        false
    }

    pub fn inorder(&self) -> Vec<&K> {
        let mut result = Vec::new();
        self.inorder_rec(self.root, &mut result);
        result
    }

    fn inorder_rec<'a>(&'a self, node: Option<usize>, result: &mut Vec<&'a K>) {
        if let Some(idx) = node {
            self.inorder_rec(self.arena[idx].left, result);
            result.push(&self.arena[idx].key);
            self.inorder_rec(self.arena[idx].right, result);
        }
    }

    pub fn tree_height(&self) -> i32 {
        self.height(self.root)
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== BST ===");
    let mut bst = Bst::new();
    for &k in &[5, 3, 7, 1, 4, 6, 8] {
        bst.insert(k, k * 10);
    }
    println!("Search 4: {:?}", bst.search(&4));
    println!("Min: {:?}", bst.min());
    println!("Max: {:?}", bst.max());
    print!("Inorder: ");
    for (k, v) in bst.inorder() {
        print!("({k}:{v}) ");
    }
    println!();

    println!("\n=== AVL Tree ===");
    let mut avl = AvlTree::new();
    // Insert in sorted order — would degenerate a plain BST but AVL stays balanced
    for i in 1..=15 {
        avl.insert(i);
    }
    println!("Inorder: {:?}", avl.inorder());
    println!("Height: {} (optimal for 15 nodes is 4)", avl.tree_height());
    println!("Contains 10: {}", avl.contains(&10));
    println!("Contains 20: {}", avl.contains(&20));
}
