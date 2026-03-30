#![allow(dead_code, unused_variables, unused_imports)]
//! # Segment Tree & Fenwick Tree (Binary Indexed Tree)
//!
//! For efficient range queries and point updates:
//! - Segment Tree: range query + range/point update in O(log n)
//! - Fenwick Tree: prefix sums + point update in O(log n), simpler and faster

// =============================================================================
// Segment Tree (Range Sum Query + Point Update)
// =============================================================================

pub struct SegmentTree {
    tree: Vec<i64>,
    n: usize,
}

impl SegmentTree {
    /// Build from array — O(n).
    pub fn from_slice(data: &[i64]) -> Self {
        let n = data.len();
        let mut tree = vec![0i64; 4 * n];
        if n > 0 {
            Self::build(&mut tree, data, 1, 0, n - 1);
        }
        Self { tree, n }
    }

    fn build(tree: &mut [i64], data: &[i64], node: usize, start: usize, end: usize) {
        if start == end {
            tree[node] = data[start];
            return;
        }
        let mid = (start + end) / 2;
        Self::build(tree, data, 2 * node, start, mid);
        Self::build(tree, data, 2 * node + 1, mid + 1, end);
        tree[node] = tree[2 * node] + tree[2 * node + 1];
    }

    /// Point update: data[idx] += delta — O(log n).
    pub fn update(&mut self, idx: usize, delta: i64) {
        self.update_rec(1, 0, self.n - 1, idx, delta);
    }

    fn update_rec(&mut self, node: usize, start: usize, end: usize, idx: usize, delta: i64) {
        if start == end {
            self.tree[node] += delta;
            return;
        }
        let mid = (start + end) / 2;
        if idx <= mid {
            self.update_rec(2 * node, start, mid, idx, delta);
        } else {
            self.update_rec(2 * node + 1, mid + 1, end, idx, delta);
        }
        self.tree[node] = self.tree[2 * node] + self.tree[2 * node + 1];
    }

    /// Range sum query: sum(data[l..=r]) — O(log n).
    pub fn query(&self, l: usize, r: usize) -> i64 {
        self.query_rec(1, 0, self.n - 1, l, r)
    }

    fn query_rec(&self, node: usize, start: usize, end: usize, l: usize, r: usize) -> i64 {
        if r < start || end < l {
            return 0; // no overlap
        }
        if l <= start && end <= r {
            return self.tree[node]; // complete overlap
        }
        let mid = (start + end) / 2;
        self.query_rec(2 * node, start, mid, l, r)
            + self.query_rec(2 * node + 1, mid + 1, end, l, r)
    }
}

// =============================================================================
// Segment Tree with Lazy Propagation (Range Update + Range Query)
// =============================================================================

pub struct LazySegmentTree {
    tree: Vec<i64>,
    lazy: Vec<i64>,
    n: usize,
}

impl LazySegmentTree {
    pub fn from_slice(data: &[i64]) -> Self {
        let n = data.len();
        let mut tree = vec![0i64; 4 * n];
        let lazy = vec![0i64; 4 * n];
        if n > 0 {
            Self::build(&mut tree, data, 1, 0, n - 1);
        }
        Self { tree, lazy, n }
    }

    fn build(tree: &mut [i64], data: &[i64], node: usize, start: usize, end: usize) {
        if start == end {
            tree[node] = data[start];
            return;
        }
        let mid = (start + end) / 2;
        Self::build(tree, data, 2 * node, start, mid);
        Self::build(tree, data, 2 * node + 1, mid + 1, end);
        tree[node] = tree[2 * node] + tree[2 * node + 1];
    }

    fn push_down(&mut self, node: usize, start: usize, end: usize) {
        if self.lazy[node] != 0 {
            let mid = (start + end) / 2;
            self.apply(2 * node, start, mid, self.lazy[node]);
            self.apply(2 * node + 1, mid + 1, end, self.lazy[node]);
            self.lazy[node] = 0;
        }
    }

    fn apply(&mut self, node: usize, start: usize, end: usize, delta: i64) {
        self.tree[node] += delta * (end - start + 1) as i64;
        self.lazy[node] += delta;
    }

    /// Range update: add delta to all elements in [l, r] — O(log n).
    pub fn range_update(&mut self, l: usize, r: usize, delta: i64) {
        self.range_update_rec(1, 0, self.n - 1, l, r, delta);
    }

    fn range_update_rec(
        &mut self,
        node: usize,
        start: usize,
        end: usize,
        l: usize,
        r: usize,
        delta: i64,
    ) {
        if r < start || end < l {
            return;
        }
        if l <= start && end <= r {
            self.apply(node, start, end, delta);
            return;
        }
        self.push_down(node, start, end);
        let mid = (start + end) / 2;
        self.range_update_rec(2 * node, start, mid, l, r, delta);
        self.range_update_rec(2 * node + 1, mid + 1, end, l, r, delta);
        self.tree[node] = self.tree[2 * node] + self.tree[2 * node + 1];
    }

    /// Range query: sum of [l, r] — O(log n).
    pub fn query(&mut self, l: usize, r: usize) -> i64 {
        self.query_rec(1, 0, self.n - 1, l, r)
    }

    fn query_rec(&mut self, node: usize, start: usize, end: usize, l: usize, r: usize) -> i64 {
        if r < start || end < l {
            return 0;
        }
        if l <= start && end <= r {
            return self.tree[node];
        }
        self.push_down(node, start, end);
        let mid = (start + end) / 2;
        self.query_rec(2 * node, start, mid, l, r)
            + self.query_rec(2 * node + 1, mid + 1, end, l, r)
    }
}

// =============================================================================
// Fenwick Tree (Binary Indexed Tree)
// =============================================================================
// Simpler and faster than segment tree for prefix sums.
// Uses the "lowest set bit" trick: i & (-i).

pub struct FenwickTree {
    tree: Vec<i64>,
    n: usize,
}

impl FenwickTree {
    pub fn new(n: usize) -> Self {
        Self {
            tree: vec![0; n + 1], // 1-indexed
            n,
        }
    }

    /// Build from array — O(n).
    pub fn from_slice(data: &[i64]) -> Self {
        let n = data.len();
        let mut tree = vec![0i64; n + 1];
        for i in 0..n {
            tree[i + 1] = data[i];
        }
        for i in 1..=n {
            let j = i + (i & i.wrapping_neg());
            if j <= n {
                tree[j] += tree[i];
            }
        }
        Self { tree, n }
    }

    /// Point update: data[i] += delta — O(log n).
    pub fn update(&mut self, mut i: usize, delta: i64) {
        i += 1; // convert to 1-indexed
        while i <= self.n {
            self.tree[i] += delta;
            i += i & i.wrapping_neg();
        }
    }

    /// Prefix sum: sum(data[0..=i]) — O(log n).
    pub fn prefix_sum(&self, mut i: usize) -> i64 {
        i += 1; // convert to 1-indexed
        let mut sum = 0;
        while i > 0 {
            sum += self.tree[i];
            i -= i & i.wrapping_neg();
        }
        sum
    }

    /// Range sum: sum(data[l..=r]) — O(log n).
    pub fn range_sum(&self, l: usize, r: usize) -> i64 {
        if l == 0 {
            self.prefix_sum(r)
        } else {
            self.prefix_sum(r) - self.prefix_sum(l - 1)
        }
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    let data = vec![1, 3, 5, 7, 9, 11];

    println!("=== Segment Tree ===");
    let mut st = SegmentTree::from_slice(&data);
    println!("Data: {data:?}");
    println!("Sum [1, 3] = {} (expected {})", st.query(1, 3), 3 + 5 + 7);
    println!("Sum [0, 5] = {} (expected {})", st.query(0, 5), data.iter().sum::<i64>());
    st.update(2, 5); // data[2] += 5: [1, 3, 10, 7, 9, 11]
    println!("After data[2] += 5:");
    println!("Sum [1, 3] = {} (expected {})", st.query(1, 3), 3 + 10 + 7);

    println!("\n=== Lazy Segment Tree (Range Update) ===");
    let mut lst = LazySegmentTree::from_slice(&data);
    println!("Sum [0, 5] = {}", lst.query(0, 5));
    lst.range_update(1, 4, 10); // add 10 to [1..=4]
    println!("After [1..=4] += 10:");
    println!("Sum [0, 5] = {} (expected {})", lst.query(0, 5), 36 + 40);
    println!("Sum [1, 4] = {} (expected {})", lst.query(1, 4), 3 + 5 + 7 + 9 + 40);

    println!("\n=== Fenwick Tree (BIT) ===");
    let data = vec![1, 3, 5, 7, 9, 11];
    let mut ft = FenwickTree::from_slice(&data);
    println!("Data: {data:?}");
    println!("Prefix sum [0..=3] = {} (expected {})", ft.prefix_sum(3), 1 + 3 + 5 + 7);
    println!("Range sum [2, 4] = {} (expected {})", ft.range_sum(2, 4), 5 + 7 + 9);
    ft.update(2, 5); // data[2] += 5
    println!("After data[2] += 5:");
    println!("Range sum [2, 4] = {} (expected {})", ft.range_sum(2, 4), 10 + 7 + 9);
}
