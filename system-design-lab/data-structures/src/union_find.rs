#![allow(dead_code, unused_variables, unused_imports)]
//! # Union-Find (Disjoint Set Union)
//!
//! Supports near-O(1) union and find operations.
//! Uses path compression + union by rank for amortized O(α(n)) per operation
//! where α is the inverse Ackermann function (effectively constant).
//!
//! Common uses:
//! - Connected components in graphs
//! - Kruskal's MST algorithm
//! - Cycle detection in undirected graphs
//! - Network connectivity

// =============================================================================
// Union-Find with Path Compression + Union by Rank
// =============================================================================

pub struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
    count: usize, // number of disjoint sets
}

impl UnionFind {
    pub fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(), // each element is its own parent
            rank: vec![0; n],
            count: n,
        }
    }

    /// Find the root representative of x with path compression.
    pub fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]); // path compression
        }
        self.parent[x]
    }

    /// Union the sets containing x and y. Returns false if already connected.
    pub fn union(&mut self, x: usize, y: usize) -> bool {
        let rx = self.find(x);
        let ry = self.find(y);

        if rx == ry {
            return false; // already in same set
        }

        // Union by rank: attach smaller tree under larger tree
        match self.rank[rx].cmp(&self.rank[ry]) {
            std::cmp::Ordering::Less => self.parent[rx] = ry,
            std::cmp::Ordering::Greater => self.parent[ry] = rx,
            std::cmp::Ordering::Equal => {
                self.parent[ry] = rx;
                self.rank[rx] += 1;
            }
        }

        self.count -= 1;
        true
    }

    /// Check if x and y are in the same set.
    pub fn connected(&mut self, x: usize, y: usize) -> bool {
        self.find(x) == self.find(y)
    }

    /// Number of disjoint sets.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Size of the set containing x.
    pub fn set_size(&mut self, x: usize) -> usize {
        let root = self.find(x);
        (0..self.parent.len())
            .filter(|&i| self.find_no_compress(i) == root)
            .count()
    }

    // Non-mutating find for size computation
    fn find_no_compress(&self, mut x: usize) -> usize {
        while self.parent[x] != x {
            x = self.parent[x];
        }
        x
    }
}

// =============================================================================
// Weighted Union-Find (tracks distances/weights between elements)
// =============================================================================

pub struct WeightedUnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
    weight: Vec<i64>, // weight[x] = weight from x to parent[x]
}

impl WeightedUnionFind {
    pub fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
            weight: vec![0; n],
        }
    }

    /// Find root with path compression, adjusting weights along the path.
    pub fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            let root = self.find(self.parent[x]);
            self.weight[x] += self.weight[self.parent[x]];
            self.parent[x] = root;
        }
        self.parent[x]
    }

    /// Union x and y with relation: weight(x) - weight(y) = w.
    pub fn union(&mut self, x: usize, y: usize, w: i64) -> bool {
        let rx = self.find(x);
        let ry = self.find(y);

        if rx == ry {
            return false;
        }

        // weight_to_apply: weight(rx) -> weight(ry)
        let weight_diff = w + self.weight[y] - self.weight[x];

        match self.rank[rx].cmp(&self.rank[ry]) {
            std::cmp::Ordering::Less => {
                self.parent[rx] = ry;
                self.weight[rx] = -weight_diff;
            }
            std::cmp::Ordering::Greater => {
                self.parent[ry] = rx;
                self.weight[ry] = weight_diff;
            }
            std::cmp::Ordering::Equal => {
                self.parent[ry] = rx;
                self.weight[ry] = weight_diff;
                self.rank[rx] += 1;
            }
        }
        true
    }

    /// Get the weight difference: weight(x) - weight(y), if in the same set.
    pub fn diff(&mut self, x: usize, y: usize) -> Option<i64> {
        if self.find(x) != self.find(y) {
            return None;
        }
        Some(self.weight[x] - self.weight[y])
    }
}

// =============================================================================
// Application: Kruskal's MST
// =============================================================================

pub fn kruskal_mst(
    num_nodes: usize,
    edges: &mut Vec<(u64, usize, usize)>,
) -> (u64, Vec<(usize, usize, u64)>) {
    // Sort edges by weight
    edges.sort_by_key(|e| e.0);

    let mut uf = UnionFind::new(num_nodes);
    let mut mst = Vec::new();
    let mut total_weight = 0;

    for &(weight, u, v) in edges.iter() {
        if uf.union(u, v) {
            mst.push((u, v, weight));
            total_weight += weight;
            if mst.len() == num_nodes - 1 {
                break;
            }
        }
    }

    (total_weight, mst)
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Union-Find ===");
    let mut uf = UnionFind::new(10);
    println!("Initial sets: {}", uf.count());

    uf.union(0, 1);
    uf.union(2, 3);
    uf.union(4, 5);
    uf.union(0, 2);
    println!("After unions: {} sets", uf.count());
    println!("0 and 3 connected: {}", uf.connected(0, 3));
    println!("0 and 4 connected: {}", uf.connected(0, 4));

    println!("\n=== Kruskal's MST ===");
    //   0 --1-- 1
    //   |       |
    //   4       2
    //   |       |
    //   3 --3-- 2
    //    \     /
    //     5   6
    //      \ /
    //       4
    let mut edges = vec![
        (1, 0, 1),
        (4, 0, 3),
        (2, 1, 2),
        (3, 2, 3),
        (5, 3, 4),
        (6, 2, 4),
    ];
    let (cost, mst) = kruskal_mst(5, &mut edges);
    println!("MST edges: {:?}", mst);
    println!("Total cost: {cost}");

    println!("\n=== Weighted Union-Find ===");
    let mut wuf = WeightedUnionFind::new(5);
    wuf.union(0, 1, 3); // weight(0) - weight(1) = 3
    wuf.union(1, 2, 5); // weight(1) - weight(2) = 5
    println!("diff(0, 2) = {:?}", wuf.diff(0, 2)); // 3 + 5 = 8
    println!("diff(2, 0) = {:?}", wuf.diff(2, 0)); // -8
}
