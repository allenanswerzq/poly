//! # Search Engine (FB Post Search / Elasticsearch) - Mini Implementation
//!
//! Demonstrates:
//! - Inverted index construction
//! - TF-IDF scoring
//! - Boolean queries (AND, OR)
//! - Fuzzy matching basics
//! - Sharded index
//!
//! Run: cargo run -p search-engine

use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone)]
struct Document {
    id: String,
    content: String,
    timestamp: u64,
    author: String,
}

#[derive(Debug, Clone)]
struct SearchResult {
    doc_id: String,
    score: f64,
    snippet: String,
}

#[derive(Debug, Clone)]
enum Query {
    Term(String),
    And(Box<Query>, Box<Query>),
    Or(Box<Query>, Box<Query>),
    Phrase(Vec<String>),
}

// =============================================================================
// Tokenizer
// =============================================================================

struct Tokenizer;

impl Tokenizer {
    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty() && s.len() > 1)
            .map(|s| s.to_string())
            .collect()
    }

    fn stem(word: &str) -> String {
        // Very simple stemmer (just remove common suffixes)
        let word = word.to_lowercase();
        if word.ends_with("ing") && word.len() > 5 {
            return word[..word.len() - 3].to_string();
        }
        if word.ends_with("ed") && word.len() > 4 {
            return word[..word.len() - 2].to_string();
        }
        if word.ends_with("s") && word.len() > 3 && !word.ends_with("ss") {
            return word[..word.len() - 1].to_string();
        }
        word
    }
}

// =============================================================================
// Inverted Index
// =============================================================================

#[derive(Debug, Clone)]
struct Posting {
    doc_id: String,
    positions: Vec<usize>, // Word positions for phrase queries
    term_freq: u32,
}

struct InvertedIndex {
    // term -> list of postings
    index: DashMap<String, Vec<Posting>>,
    // doc_id -> document
    documents: DashMap<String, Document>,
    // doc_id -> document length (for scoring)
    doc_lengths: DashMap<String, u32>,
    // Total document count
    doc_count: AtomicU64,
    // Average document length
    avg_doc_length: RwLock<f64>,
}

impl InvertedIndex {
    fn new() -> Self {
        Self {
            index: DashMap::new(),
            documents: DashMap::new(),
            doc_lengths: DashMap::new(),
            doc_count: AtomicU64::new(0),
            avg_doc_length: RwLock::new(0.0),
        }
    }

    fn index_document(&self, doc: Document) {
        let tokens = Tokenizer::tokenize(&doc.content);
        let doc_length = tokens.len() as u32;

        // Count term frequencies and positions
        let mut term_positions: HashMap<String, Vec<usize>> = HashMap::new();
        for (pos, token) in tokens.iter().enumerate() {
            let stemmed = Tokenizer::stem(token);
            term_positions.entry(stemmed).or_default().push(pos);
        }

        // Add to inverted index
        for (term, positions) in term_positions {
            let posting = Posting {
                doc_id: doc.id.clone(),
                positions: positions.clone(),
                term_freq: positions.len() as u32,
            };

            self.index
                .entry(term)
                .or_default()
                .push(posting);
        }

        // Store document
        self.documents.insert(doc.id.clone(), doc.clone());
        self.doc_lengths.insert(doc.id.clone(), doc_length);

        // Update stats
        let count = self.doc_count.fetch_add(1, Ordering::SeqCst) + 1;
        let total_length: u32 = self.doc_lengths.iter().map(|e| *e.value()).sum();
        *self.avg_doc_length.write() = total_length as f64 / count as f64;
    }

    fn search_term(&self, term: &str) -> Vec<String> {
        let stemmed = Tokenizer::stem(term);
        self.index
            .get(&stemmed)
            .map(|postings| postings.iter().map(|p| p.doc_id.clone()).collect())
            .unwrap_or_default()
    }

    fn get_postings(&self, term: &str) -> Vec<Posting> {
        let stemmed = Tokenizer::stem(term);
        self.index
            .get(&stemmed)
            .map(|p| p.clone())
            .unwrap_or_default()
    }

    fn idf(&self, term: &str) -> f64 {
        let stemmed = Tokenizer::stem(term);
        let doc_freq = self
            .index
            .get(&stemmed)
            .map(|p| p.len())
            .unwrap_or(0);

        let n = self.doc_count.load(Ordering::SeqCst) as f64;
        if doc_freq == 0 {
            return 0.0;
        }

        // IDF formula: log(N / df)
        (n / doc_freq as f64).ln()
    }
}

// =============================================================================
// Scorer (BM25)
// =============================================================================

struct BM25Scorer {
    k1: f64, // Term frequency saturation
    b: f64,  // Length normalization
}

impl BM25Scorer {
    fn new() -> Self {
        Self { k1: 1.2, b: 0.75 }
    }

    fn score(&self, index: &InvertedIndex, query_terms: &[String], doc_id: &str) -> f64 {
        let doc_length = index.doc_lengths.get(doc_id).map(|l| *l).unwrap_or(0) as f64;
        let avg_length = *index.avg_doc_length.read();

        let mut score = 0.0;

        for term in query_terms {
            let idf = index.idf(term);
            let postings = index.get_postings(term);

            if let Some(posting) = postings.iter().find(|p| p.doc_id == doc_id) {
                let tf = posting.term_freq as f64;

                // BM25 formula
                let numerator = tf * (self.k1 + 1.0);
                let denominator = tf + self.k1 * (1.0 - self.b + self.b * doc_length / avg_length);

                score += idf * numerator / denominator;
            }
        }

        score
    }
}

// =============================================================================
// Query Executor
// =============================================================================

struct QueryExecutor {
    index: InvertedIndex,
    scorer: BM25Scorer,
}

impl QueryExecutor {
    fn new() -> Self {
        Self {
            index: InvertedIndex::new(),
            scorer: BM25Scorer::new(),
        }
    }

    fn index(&self, doc: Document) {
        self.index.index_document(doc);
    }

    fn execute(&self, query: &Query) -> HashSet<String> {
        match query {
            Query::Term(term) => self.index.search_term(term).into_iter().collect(),

            Query::And(left, right) => {
                let left_results = self.execute(left);
                let right_results = self.execute(right);
                left_results.intersection(&right_results).cloned().collect()
            }

            Query::Or(left, right) => {
                let left_results = self.execute(left);
                let right_results = self.execute(right);
                left_results.union(&right_results).cloned().collect()
            }

            Query::Phrase(terms) => {
                // Find documents with terms in sequence
                if terms.is_empty() {
                    return HashSet::new();
                }

                let first_postings = self.index.get_postings(&terms[0]);
                let mut candidates: HashMap<String, Vec<usize>> = first_postings
                    .into_iter()
                    .map(|p| (p.doc_id, p.positions))
                    .collect();

                for (i, term) in terms.iter().enumerate().skip(1) {
                    let postings = self.index.get_postings(term);
                    let posting_map: HashMap<String, Vec<usize>> = postings
                        .into_iter()
                        .map(|p| (p.doc_id, p.positions))
                        .collect();

                    candidates = candidates
                        .into_iter()
                        .filter_map(|(doc_id, positions)| {
                            if let Some(next_positions) = posting_map.get(&doc_id) {
                                // Check if any position is consecutive
                                let valid: Vec<usize> = positions
                                    .iter()
                                    .filter(|p| next_positions.contains(&(*p + i)))
                                    .cloned()
                                    .collect();

                                if !valid.is_empty() {
                                    return Some((doc_id, valid));
                                }
                            }
                            None
                        })
                        .collect();
                }

                candidates.keys().cloned().collect()
            }
        }
    }

    fn search(&self, query_str: &str, limit: usize) -> Vec<SearchResult> {
        let terms = Tokenizer::tokenize(query_str);
        if terms.is_empty() {
            return Vec::new();
        }

        // Build OR query from terms
        let query = terms
            .iter()
            .map(|t| Query::Term(t.clone()))
            .reduce(|a, b| Query::Or(Box::new(a), Box::new(b)))
            .unwrap();

        let doc_ids = self.execute(&query);

        // Score and rank
        let mut results: Vec<SearchResult> = doc_ids
            .iter()
            .filter_map(|doc_id| {
                let doc = self.index.documents.get(doc_id)?;
                let score = self.scorer.score(&self.index, &terms, doc_id);

                // Generate snippet
                let snippet = doc
                    .content
                    .chars()
                    .take(100)
                    .collect::<String>()
                    + "...";

                Some(SearchResult {
                    doc_id: doc_id.clone(),
                    score,
                    snippet,
                })
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.truncate(limit);
        results
    }
}

// =============================================================================
// Fuzzy Matcher (Edit Distance)
// =============================================================================

struct FuzzyMatcher;

impl FuzzyMatcher {
    fn edit_distance(a: &str, b: &str) -> usize {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();

        let mut dp = vec![vec![0; b.len() + 1]; a.len() + 1];

        for i in 0..=a.len() {
            dp[i][0] = i;
        }
        for j in 0..=b.len() {
            dp[0][j] = j;
        }

        for i in 1..=a.len() {
            for j in 1..=b.len() {
                let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
                dp[i][j] = (dp[i - 1][j] + 1)
                    .min(dp[i][j - 1] + 1)
                    .min(dp[i - 1][j - 1] + cost);
            }
        }

        dp[a.len()][b.len()]
    }

    fn find_fuzzy_matches(index: &InvertedIndex, term: &str, max_distance: usize) -> Vec<String> {
        let mut matches = Vec::new();

        for entry in index.index.iter() {
            let indexed_term = entry.key();
            if Self::edit_distance(term, indexed_term) <= max_distance {
                matches.push(indexed_term.clone());
            }
        }

        matches
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== Search Engine Demo ===\n");

    let engine = QueryExecutor::new();

    // Index documents
    println!("--- Indexing Documents ---");
    let docs = vec![
        Document {
            id: "doc1".to_string(),
            content: "Rust is a systems programming language focused on safety and performance"
                .to_string(),
            timestamp: 1000,
            author: "alice".to_string(),
        },
        Document {
            id: "doc2".to_string(),
            content: "Python is great for machine learning and data science applications"
                .to_string(),
            timestamp: 2000,
            author: "bob".to_string(),
        },
        Document {
            id: "doc3".to_string(),
            content: "JavaScript and TypeScript are popular for web development programming"
                .to_string(),
            timestamp: 3000,
            author: "charlie".to_string(),
        },
        Document {
            id: "doc4".to_string(),
            content: "Rust and Python are both excellent programming languages for different uses"
                .to_string(),
            timestamp: 4000,
            author: "diana".to_string(),
        },
        Document {
            id: "doc5".to_string(),
            content: "Systems programming requires understanding of memory and performance"
                .to_string(),
            timestamp: 5000,
            author: "eve".to_string(),
        },
    ];

    for doc in docs {
        println!("  Indexed: {} - {}", doc.id, &doc.content[..50.min(doc.content.len())]);
        engine.index(doc);
    }
    println!("Total: {} documents indexed\n", engine.index.documents.len());

    // Simple search
    println!("--- Simple Search: 'rust' ---");
    let results = engine.search("rust", 10);
    for (i, result) in results.iter().enumerate() {
        println!(
            "  {}. {} (score: {:.3})",
            i + 1,
            result.doc_id,
            result.score
        );
        println!("     {}", result.snippet);
    }
    println!();

    // Multi-term search
    println!("--- Multi-term Search: 'programming language' ---");
    let results = engine.search("programming language", 10);
    for (i, result) in results.iter().enumerate() {
        println!(
            "  {}. {} (score: {:.3})",
            i + 1,
            result.doc_id,
            result.score
        );
    }
    println!();

    // Boolean AND query
    println!("--- Boolean AND: 'rust' AND 'python' ---");
    let query = Query::And(
        Box::new(Query::Term("rust".to_string())),
        Box::new(Query::Term("python".to_string())),
    );
    let doc_ids = engine.execute(&query);
    println!("  Matching docs: {:?}", doc_ids);
    println!();

    // Boolean OR query
    println!("--- Boolean OR: 'rust' OR 'javascript' ---");
    let query = Query::Or(
        Box::new(Query::Term("rust".to_string())),
        Box::new(Query::Term("javascript".to_string())),
    );
    let doc_ids = engine.execute(&query);
    println!("  Matching docs: {:?}", doc_ids);
    println!();

    // Phrase query
    println!("--- Phrase Query: \"programming language\" ---");
    let query = Query::Phrase(vec!["programming".to_string(), "language".to_string()]);
    let doc_ids = engine.execute(&query);
    println!("  Docs with exact phrase: {:?}", doc_ids);
    println!();

    // Fuzzy matching
    println!("--- Fuzzy Matching ---");
    let fuzzy_term = "progamming"; // Typo
    let matches = FuzzyMatcher::find_fuzzy_matches(&engine.index, fuzzy_term, 2);
    println!("  '{}' fuzzy matches: {:?}", fuzzy_term, matches);

    // IDF demonstration
    println!("\n--- IDF Values ---");
    for term in &["rust", "programming", "the"] {
        let idf = engine.index.idf(term);
        println!("  '{}': IDF = {:.3}", term, idf);
    }

    println!("\n=== Key Concepts ===");
    println!("1. Inverted Index: term -> list of (doc_id, positions)");
    println!("2. Tokenization: Lowercase + split + stem");
    println!("3. TF-IDF/BM25: Relevance scoring");
    println!("4. Boolean Queries: AND, OR for combining terms");
    println!("5. Phrase Queries: Check term positions are consecutive");
    println!("6. Fuzzy Matching: Edit distance for typo tolerance");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer() {
        let tokens = Tokenizer::tokenize("Hello World! This is a test.");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
    }

    #[test]
    fn test_stemmer() {
        assert_eq!(Tokenizer::stem("running"), "runn");
        assert_eq!(Tokenizer::stem("jumped"), "jump");
        assert_eq!(Tokenizer::stem("cats"), "cat");
    }

    #[test]
    fn test_search() {
        let engine = QueryExecutor::new();

        engine.index(Document {
            id: "1".to_string(),
            content: "rust programming".to_string(),
            timestamp: 0,
            author: "a".to_string(),
        });

        let results = engine.search("rust", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "1");
    }

    #[test]
    fn test_edit_distance() {
        assert_eq!(FuzzyMatcher::edit_distance("kitten", "sitting"), 3);
        assert_eq!(FuzzyMatcher::edit_distance("rust", "rust"), 0);
        assert_eq!(FuzzyMatcher::edit_distance("rust", "rusts"), 1);
    }
}
