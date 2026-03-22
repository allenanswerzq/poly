//! # Database Indexing: B-Tree Implementation
//!
//! Demonstrates how database indexes work:
//! - B-Tree structure (used by most SQL databases)
//! - Index operations: search, insert, range queries
//! - Performance characteristics

use std::collections::VecDeque;

// =============================================================================
// B-Tree Node
// =============================================================================

const MAX_KEYS: usize = 3;  // Small for demonstration (real DBs use 100+)
const MIN_KEYS: usize = MAX_KEYS / 2;

#[derive(Debug, Clone)]
struct BTreeNode<K: Ord + Clone, V: Clone> {
    keys: Vec<K>,
    values: Vec<V>,
    children: Vec<Box<BTreeNode<K, V>>>,
    is_leaf: bool,
}

impl<K: Ord + Clone + std::fmt::Debug, V: Clone + std::fmt::Debug> BTreeNode<K, V> {
    fn new_leaf() -> Self {
        Self {
            keys: Vec::new(),
            values: Vec::new(),
            children: Vec::new(),
            is_leaf: true,
        }
    }

    fn new_internal() -> Self {
        Self {
            keys: Vec::new(),
            values: Vec::new(),
            children: Vec::new(),
            is_leaf: false,
        }
    }

    fn is_full(&self) -> bool {
        self.keys.len() >= MAX_KEYS
    }

    /// Find the index where key should be or is located
    fn find_key_index(&self, key: &K) -> usize {
        self.keys.iter().position(|k| k >= key).unwrap_or(self.keys.len())
    }

    /// Search for a key
    fn search(&self, key: &K) -> Option<&V> {
        let idx = self.find_key_index(key);

        if idx < self.keys.len() && &self.keys[idx] == key {
            // Found in this node
            return Some(&self.values[idx]);
        }

        if self.is_leaf {
            // Not found in leaf
            return None;
        }

        // Search in child
        self.children[idx].search(key)
    }

    /// Range query: find all values where start <= key <= end
    fn range_search(&self, start: &K, end: &K, results: &mut Vec<(K, V)>) {
        if self.is_leaf {
            // In leaf, collect matching keys
            for (i, key) in self.keys.iter().enumerate() {
                if key >= start && key <= end {
                    results.push((key.clone(), self.values[i].clone()));
                }
            }
            return;
        }

        // Internal node: search relevant children
        let start_idx = self.find_key_index(start);
        let end_idx = self.find_key_index(end);

        // Search children in range
        for i in start_idx..=end_idx.min(self.children.len() - 1) {
            self.children[i].range_search(start, end, results);
        }
    }
}

// =============================================================================
// B-Tree
// =============================================================================

#[derive(Debug)]
pub struct BTree<K: Ord + Clone, V: Clone> {
    root: Option<Box<BTreeNode<K, V>>>,
}

impl<K: Ord + Clone + std::fmt::Debug, V: Clone + std::fmt::Debug> BTree<K, V> {
    pub fn new() -> Self {
        Self { root: None }
    }

    /// Search for a key, returning the associated value
    pub fn search(&self, key: &K) -> Option<&V> {
        self.root.as_ref()?.search(key)
    }

    /// Insert a key-value pair
    pub fn insert(&mut self, key: K, value: V) {
        match self.root.take() {
            None => {
                // Create new root
                let mut node = BTreeNode::new_leaf();
                node.keys.push(key);
                node.values.push(value);
                self.root = Some(Box::new(node));
            }
            Some(root) => {
                if root.is_full() {
                    // Root is full, need to split
                    let mut new_root = BTreeNode::new_internal();
                    new_root.children.push(root);
                    self.split_child(&mut new_root, 0);
                    self.insert_non_full(&mut new_root, key, value);
                    self.root = Some(Box::new(new_root));
                } else {
                    let mut root = root;
                    self.insert_non_full(&mut root, key, value);
                    self.root = Some(root);
                }
            }
        }
    }

    fn split_child(&mut self, parent: &mut BTreeNode<K, V>, child_idx: usize) {
        let child = &mut parent.children[child_idx];
        let mid = MAX_KEYS / 2;

        // Create new sibling with right half
        let mut sibling = if child.is_leaf {
            BTreeNode::new_leaf()
        } else {
            BTreeNode::new_internal()
        };

        // Move right half of keys/values to sibling
        sibling.keys = child.keys.split_off(mid + 1);
        sibling.values = child.values.split_off(mid + 1);

        if !child.is_leaf {
            sibling.children = child.children.split_off(mid + 1);
        }

        // Move middle key up to parent
        let mid_key = child.keys.pop().unwrap();
        let mid_value = child.values.pop().unwrap();

        parent.keys.insert(child_idx, mid_key);
        parent.values.insert(child_idx, mid_value);
        parent.children.insert(child_idx + 1, Box::new(sibling));
    }

    fn insert_non_full(&mut self, node: &mut BTreeNode<K, V>, key: K, value: V) {
        let idx = node.find_key_index(&key);

        if node.is_leaf {
            // Insert directly into leaf
            node.keys.insert(idx, key);
            node.values.insert(idx, value);
        } else {
            // Insert into appropriate child
            if node.children[idx].is_full() {
                self.split_child(node, idx);
                // After split, determine which child to insert into
                let idx = node.find_key_index(&key);
                self.insert_non_full(&mut node.children[idx], key, value);
            } else {
                self.insert_non_full(&mut node.children[idx], key, value);
            }
        }
    }

    /// Range query: find all entries where start <= key <= end
    pub fn range_search(&self, start: &K, end: &K) -> Vec<(K, V)> {
        let mut results = Vec::new();
        if let Some(ref root) = self.root {
            root.range_search(start, end, &mut results);
        }
        results
    }

    /// Print tree structure (for visualization)
    pub fn print(&self) where K: std::fmt::Display {
        if let Some(ref root) = self.root {
            println!("B-Tree Structure:");
            self.print_node(root, 0);
        } else {
            println!("Empty tree");
        }
    }

    fn print_node(&self, node: &BTreeNode<K, V>, level: usize) where K: std::fmt::Display {
        let indent = "  ".repeat(level);
        let keys: Vec<String> = node.keys.iter().map(|k| format!("{}", k)).collect();
        println!("{}[{}]", indent, keys.join(", "));

        for child in &node.children {
            self.print_node(child, level + 1);
        }
    }
}

impl<K: Ord + Clone + std::fmt::Debug, V: Clone + std::fmt::Debug> Default for BTree<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Comparison with Sequential Scan
// =============================================================================

fn sequential_scan<T: Eq>(data: &[(i32, T)], key: &i32) -> Option<usize> {
    data.iter().position(|(k, _)| k == key)
}

fn measure_operations(n: usize) {
    use std::time::Instant;

    let mut btree = BTree::new();
    let mut linear_data = Vec::new();

    // Insert N random values
    let mut rng_values: Vec<i32> = (0..n as i32).collect();
    use rand::seq::SliceRandom;
    rng_values.shuffle(&mut rand::thread_rng());

    for &key in &rng_values {
        btree.insert(key, key * 10);
        linear_data.push((key, key * 10));
    }

    // Search for a subset
    let search_keys: Vec<i32> = rng_values.iter().take(1000).copied().collect();

    // B-Tree search
    let start = Instant::now();
    for key in &search_keys {
        btree.search(key);
    }
    let btree_time = start.elapsed();

    // Linear search
    let start = Instant::now();
    for key in &search_keys {
        sequential_scan(&linear_data, key);
    }
    let linear_time = start.elapsed();

    println!("  Dataset size: {} items", n);
    println!("  B-Tree search (1000 queries): {:?}", btree_time);
    println!("  Linear search (1000 queries): {:?}", linear_time);
    println!("  Speedup: {:.1}x", linear_time.as_nanos() as f64 / btree_time.as_nanos() as f64);
}

// =============================================================================
// Demonstration
// =============================================================================

fn main() {
    println!("=== Database Indexing (B-Tree) Demo ===\n");

    // Demo 1: Basic B-Tree operations
    println!("\n  ═══ Basic B-Tree Operations ═══");
    let mut btree: BTree<i32, String> = BTree::new();

    // Insert values
    let entries = vec![
        (10, "ten"),
        (20, "twenty"),
        (5, "five"),
        (6, "six"),
        (12, "twelve"),
        (30, "thirty"),
        (7, "seven"),
        (17, "seventeen"),
    ];

    println!("Inserting entries:");
    for (key, value) in &entries {
        println!("  INSERT {} -> '{}'", key, value);
        btree.insert(*key, value.to_string());
    }

    println!("\nTree structure:");
    btree.print();

    // Search
    println!("\nSearching:");
    for key in &[5, 12, 30, 100] {
        let result = btree.search(key);
        println!("  SEARCH {}: {:?}", key, result);
    }

    // Range query
    println!("\nRange queries:");
    let range_10_20 = btree.range_search(&10, &20);
    println!("  RANGE 10..20: {:?}", range_10_20);

    let range_5_15 = btree.range_search(&5, &15);
    println!("  RANGE 5..15: {:?}", range_5_15);

    // Demo 2: SQL Index behavior
    println!("\n--- SQL Index Behavior ---");
    println!("
Without index:  SELECT * FROM users WHERE age = 30
                -> Full table scan: O(N)

With B-Tree index on 'age':
                -> Index lookup: O(log N)
                -> Follow pointer to row

Range queries work efficiently:
                SELECT * FROM users WHERE age BETWEEN 25 AND 35
                -> Find start position O(log N)
                -> Scan leaves until end
");

    // Demo 3: Performance comparison
    println!("\n  ═══ Performance Comparison ═══\n");

    println!("Small dataset (1,000 items):");
    measure_operations(1_000);

    println!("\nMedium dataset (10,000 items):");
    measure_operations(10_000);

    println!("\nLarge dataset (100,000 items):");
    measure_operations(100_000);

    // Demo 4: Index types explanation
    println!("\n--- Index Types ---");
    println!("
B-Tree Index (most common):
  - Good for: =, <, >, <=, >=, BETWEEN, LIKE 'prefix%'
  - Used by: PostgreSQL, MySQL (InnoDB)
  - Structure: Balanced tree, all leaves at same depth

Hash Index:
  - Good for: = only
  - O(1) lookups
  - Bad for: ranges, ordering

B+ Tree (database variation):
  - All data in leaf nodes
  - Leaves linked for efficient range scans
  - Internal nodes only store keys (more fit in memory)

Covering Index:
  - Index contains all columns needed by query
  - No need to access table data (index-only scan)
");

    // Demo 5: When indexes help/hurt
    println!("\n  ═══ When to Use Indexes ═══");
    println!("
Good candidates for indexing:
  ✓ Primary keys (automatic)
  ✓ Foreign keys
  ✓ Columns in WHERE clauses
  ✓ Columns in JOIN conditions
  ✓ Columns in ORDER BY

Avoid indexing:
  ✗ Small tables (full scan faster)
  ✗ Columns with low cardinality (M/F)
  ✗ Frequently updated columns (index maintenance cost)
  ✗ Columns rarely used in queries

Index overhead:
  - Slows down INSERT/UPDATE/DELETE
  - Uses disk space
  - Needs maintenance (rebalancing, fragmentation)
");

    println!("\n=== Demo Complete ===");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_search() {
        let mut tree = BTree::new();

        tree.insert(10, "ten");
        tree.insert(20, "twenty");
        tree.insert(5, "five");

        assert_eq!(tree.search(&10), Some(&"ten"));
        assert_eq!(tree.search(&20), Some(&"twenty"));
        assert_eq!(tree.search(&5), Some(&"five"));
        assert_eq!(tree.search(&15), None);
    }

    #[test]
    fn test_many_inserts() {
        let mut tree = BTree::new();

        for i in 0..100 {
            tree.insert(i, i * 10);
        }

        for i in 0..100 {
            assert_eq!(tree.search(&i), Some(&(i * 10)));
        }
    }

    #[test]
    fn test_range_search() {
        let mut tree = BTree::new();

        for i in 0..20 {
            tree.insert(i, i);
        }

        let results = tree.range_search(&5, &10);
        assert_eq!(results.len(), 6);  // 5, 6, 7, 8, 9, 10
    }
}
