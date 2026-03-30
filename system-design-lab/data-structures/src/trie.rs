#![allow(dead_code, unused_variables, unused_imports)]
//! # Trie (Prefix Tree)
//!
//! Efficient prefix lookups for autocomplete, spell checking, IP routing.

use std::collections::HashMap;

// =============================================================================
// Array-based Trie (lowercase ASCII a-z)
// =============================================================================

const ALPHABET_SIZE: usize = 26;

struct TrieNode {
    children: [Option<usize>; ALPHABET_SIZE],
    is_end: bool,
    count: usize, // number of words passing through this node
}

impl TrieNode {
    fn new() -> Self {
        Self {
            children: [None; ALPHABET_SIZE],
            is_end: false,
            count: 0,
        }
    }
}

pub struct Trie {
    nodes: Vec<TrieNode>,
}

impl Trie {
    pub fn new() -> Self {
        Self {
            nodes: vec![TrieNode::new()], // root at index 0
        }
    }

    fn char_to_idx(c: char) -> usize {
        (c as u8 - b'a') as usize
    }

    pub fn insert(&mut self, word: &str) {
        let mut cur = 0;
        for c in word.chars() {
            let ci = Self::char_to_idx(c);
            if self.nodes[cur].children[ci].is_none() {
                let new_idx = self.nodes.len();
                self.nodes.push(TrieNode::new());
                self.nodes[cur].children[ci] = Some(new_idx);
            }
            cur = self.nodes[cur].children[ci].unwrap();
            self.nodes[cur].count += 1;
        }
        self.nodes[cur].is_end = true;
    }

    pub fn search(&self, word: &str) -> bool {
        self.find_node(word)
            .map_or(false, |idx| self.nodes[idx].is_end)
    }

    pub fn starts_with(&self, prefix: &str) -> bool {
        self.find_node(prefix).is_some()
    }

    /// Count words that have this prefix.
    pub fn count_prefix(&self, prefix: &str) -> usize {
        self.find_node(prefix)
            .map_or(0, |idx| self.nodes[idx].count)
    }

    fn find_node(&self, prefix: &str) -> Option<usize> {
        let mut cur = 0;
        for c in prefix.chars() {
            let ci = Self::char_to_idx(c);
            cur = self.nodes[cur].children[ci]?;
        }
        Some(cur)
    }

    /// Autocomplete: return all words with the given prefix.
    pub fn autocomplete(&self, prefix: &str) -> Vec<String> {
        let mut results = Vec::new();
        if let Some(node_idx) = self.find_node(prefix) {
            let mut current_word = prefix.to_string();
            self.collect_words(node_idx, &mut current_word, &mut results);
        }
        results
    }

    fn collect_words(&self, idx: usize, current: &mut String, results: &mut Vec<String>) {
        if self.nodes[idx].is_end {
            results.push(current.clone());
        }
        for ci in 0..ALPHABET_SIZE {
            if let Some(child) = self.nodes[idx].children[ci] {
                current.push((b'a' + ci as u8) as char);
                self.collect_words(child, current, results);
                current.pop();
            }
        }
    }
}

// =============================================================================
// HashMap-based Trie (supports any character)
// =============================================================================

struct GenericTrieNode {
    children: HashMap<char, usize>,
    is_end: bool,
}

pub struct GenericTrie {
    nodes: Vec<GenericTrieNode>,
}

impl GenericTrie {
    pub fn new() -> Self {
        Self {
            nodes: vec![GenericTrieNode {
                children: HashMap::new(),
                is_end: false,
            }],
        }
    }

    pub fn insert(&mut self, word: &str) {
        let mut cur = 0;
        for c in word.chars() {
            if !self.nodes[cur].children.contains_key(&c) {
                let new_idx = self.nodes.len();
                self.nodes.push(GenericTrieNode {
                    children: HashMap::new(),
                    is_end: false,
                });
                self.nodes[cur].children.insert(c, new_idx);
            }
            cur = self.nodes[cur].children[&c];
        }
        self.nodes[cur].is_end = true;
    }

    pub fn search(&self, word: &str) -> bool {
        let mut cur = 0;
        for c in word.chars() {
            match self.nodes[cur].children.get(&c) {
                Some(&next) => cur = next,
                None => return false,
            }
        }
        self.nodes[cur].is_end
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Trie ===");
    let mut trie = Trie::new();
    let words = ["apple", "app", "application", "apply", "banana", "band", "bandana"];
    for w in &words {
        trie.insert(w);
    }

    println!("search 'app': {}", trie.search("app"));
    println!("search 'ap': {}", trie.search("ap"));
    println!("starts_with 'app': {}", trie.starts_with("app"));
    println!("starts_with 'baz': {}", trie.starts_with("baz"));
    println!("count_prefix 'app': {}", trie.count_prefix("app"));
    println!("count_prefix 'ban': {}", trie.count_prefix("ban"));

    println!("\nAutocomplete 'app': {:?}", trie.autocomplete("app"));
    println!("Autocomplete 'ban': {:?}", trie.autocomplete("ban"));

    println!("\n=== Generic Trie (Unicode) ===");
    let mut gt = GenericTrie::new();
    gt.insert("hello");
    gt.insert("world");
    println!("search 'hello': {}", gt.search("hello"));
    println!("search 'hell': {}", gt.search("hell"));
}
