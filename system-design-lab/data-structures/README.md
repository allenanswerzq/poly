# Data Structures

Common data structures implemented from scratch in Rust — covering basic, thread-safe,
lock-free, and advanced variants used daily in projects and interviews.

## Contents

### Basic
| Module | Structures | Key Concepts |
|--------|-----------|--------------|
| `linked_list` | Singly linked list, Doubly linked list (arena-based) | Arena allocation, index-based pointers |
| `stack_queue` | Stack, Queue (ring buffer), Deque | LIFO/FIFO, circular buffer, amortized O(1) |
| `hash_map` | Separate chaining, Open addressing (linear probing) | Load factor, rehashing, collision resolution |
| `bst` | BST, AVL tree (self-balancing) | Rotations, height balance, in-order traversal |
| `heap` | Binary min/max heap, Priority queue | Heapify, sift-up/down, extract-min |
| `trie` | Prefix trie | Autocomplete, prefix search, word dictionaries |
| `graph` | Adjacency list, BFS, DFS, Dijkstra, topological sort | Shortest path, cycle detection, DAG ordering |

### Thread-Safe
| Module | Structures | Key Concepts |
|--------|-----------|--------------|
| `concurrent` | Sharded concurrent HashMap, Bounded blocking queue, Read-write locked BST | Sharding, condition variables, reader-writer locks |

### Lock-Free
| Module | Structures | Key Concepts |
|--------|-----------|--------------|
| `lock_free` | Treiber stack, Lock-free counter, SPSC ring buffer | CAS loops, ABA problem, memory ordering |

### Advanced
| Module | Structures | Key Concepts |
|--------|-----------|--------------|
| `lru_lfu` | LRU cache, LFU cache | O(1) eviction, frequency tracking, doubly-linked lists |
| `bloom_filter` | Bloom filter | Probabilistic membership, false positive rate, hash functions |
| `skip_list` | Skip list | Probabilistic balancing, O(log n) search, sorted collections |
| `union_find` | Union-Find / Disjoint set | Path compression, union by rank, connected components |
| `segment_tree` | Segment tree, Fenwick tree (BIT) | Range queries, point updates, prefix sums |
| `arena` | Arena allocator, Object pool | Bump allocation, memory reuse, batch deallocation |

## Run

```bash
cargo run --bin data-structures
```

## Interview Checklist

- [ ] Implement a hash map from scratch (chaining vs open addressing)
- [ ] LRU cache with O(1) get/put
- [ ] Trie for autocomplete
- [ ] Union-Find for connected components
- [ ] Thread-safe bounded queue (producer-consumer)
- [ ] Lock-free stack using CAS
- [ ] Bloom filter for membership testing
- [ ] Segment tree for range queries
- [ ] Binary heap / priority queue
- [ ] Skip list as sorted map alternative
- [ ] AVL tree rotations
- [ ] Graph BFS/DFS/Dijkstra

## Complexity Reference

```
┌─────────────────┬──────────┬──────────┬──────────┬──────────┐
│ Structure       │ Search   │ Insert   │ Delete   │ Space    │
├─────────────────┼──────────┼──────────┼──────────┼──────────┤
│ Array           │ O(n)     │ O(1)*    │ O(n)     │ O(n)     │
│ Linked List     │ O(n)     │ O(1)     │ O(1)     │ O(n)     │
│ Hash Map        │ O(1)     │ O(1)     │ O(1)     │ O(n)     │
│ BST (balanced)  │ O(log n) │ O(log n) │ O(log n) │ O(n)     │
│ Heap            │ O(n)     │ O(log n) │ O(log n) │ O(n)     │
│ Trie            │ O(k)     │ O(k)     │ O(k)     │ O(n·k)   │
│ Skip List       │ O(log n) │ O(log n) │ O(log n) │ O(n)     │
│ Bloom Filter    │ O(k)     │ O(k)     │ N/A      │ O(m)     │
│ Union-Find      │ O(α(n))  │ O(α(n))  │ N/A      │ O(n)     │
│ Segment Tree    │ O(log n) │ O(log n) │ N/A      │ O(n)     │
└─────────────────┴──────────┴──────────┴──────────┴──────────┘
* amortized for dynamic array
k = key length (trie) or number of hash functions (bloom filter)
m = bit array size, α = inverse Ackermann function
```
