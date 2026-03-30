#![allow(dead_code, unused_variables, unused_imports)]
//! # Graph
//!
//! Adjacency list representation with BFS, DFS, Dijkstra, and topological sort.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

// =============================================================================
// Adjacency List Graph
// =============================================================================

pub struct Graph {
    adj: Vec<Vec<(usize, u64)>>, // (neighbor, weight)
    directed: bool,
    num_nodes: usize,
}

impl Graph {
    pub fn new(num_nodes: usize, directed: bool) -> Self {
        Self {
            adj: vec![Vec::new(); num_nodes],
            directed,
            num_nodes,
        }
    }

    pub fn add_edge(&mut self, from: usize, to: usize, weight: u64) {
        self.adj[from].push((to, weight));
        if !self.directed {
            self.adj[to].push((from, weight));
        }
    }

    pub fn neighbors(&self, node: usize) -> &[(usize, u64)] {
        &self.adj[node]
    }

    // =========================================================================
    // BFS — O(V + E)
    // =========================================================================

    pub fn bfs(&self, start: usize) -> Vec<usize> {
        let mut visited = vec![false; self.num_nodes];
        let mut order = Vec::new();
        let mut queue = VecDeque::new();

        visited[start] = true;
        queue.push_back(start);

        while let Some(node) = queue.pop_front() {
            order.push(node);
            for &(neighbor, _) in &self.adj[node] {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        order
    }

    /// BFS shortest path (unweighted). Returns distances from start.
    pub fn bfs_distances(&self, start: usize) -> Vec<Option<u64>> {
        let mut dist = vec![None; self.num_nodes];
        let mut queue = VecDeque::new();
        dist[start] = Some(0);
        queue.push_back(start);

        while let Some(node) = queue.pop_front() {
            let d = dist[node].unwrap();
            for &(neighbor, _) in &self.adj[node] {
                if dist[neighbor].is_none() {
                    dist[neighbor] = Some(d + 1);
                    queue.push_back(neighbor);
                }
            }
        }
        dist
    }

    // =========================================================================
    // DFS — O(V + E)
    // =========================================================================

    pub fn dfs(&self, start: usize) -> Vec<usize> {
        let mut visited = vec![false; self.num_nodes];
        let mut order = Vec::new();
        self.dfs_rec(start, &mut visited, &mut order);
        order
    }

    fn dfs_rec(&self, node: usize, visited: &mut [bool], order: &mut Vec<usize>) {
        visited[node] = true;
        order.push(node);
        for &(neighbor, _) in &self.adj[node] {
            if !visited[neighbor] {
                self.dfs_rec(neighbor, visited, order);
            }
        }
    }

    /// Iterative DFS using an explicit stack.
    pub fn dfs_iterative(&self, start: usize) -> Vec<usize> {
        let mut visited = vec![false; self.num_nodes];
        let mut order = Vec::new();
        let mut stack = vec![start];

        while let Some(node) = stack.pop() {
            if visited[node] {
                continue;
            }
            visited[node] = true;
            order.push(node);
            // Push neighbors in reverse for consistent ordering
            for &(neighbor, _) in self.adj[node].iter().rev() {
                if !visited[neighbor] {
                    stack.push(neighbor);
                }
            }
        }
        order
    }

    // =========================================================================
    // Dijkstra's Shortest Path — O((V + E) log V)
    // =========================================================================

    pub fn dijkstra(&self, start: usize) -> (Vec<u64>, Vec<Option<usize>>) {
        let mut dist = vec![u64::MAX; self.num_nodes];
        let mut prev = vec![None; self.num_nodes];
        let mut heap = BinaryHeap::new();

        dist[start] = 0;
        heap.push(Reverse((0u64, start)));

        while let Some(Reverse((cost, node))) = heap.pop() {
            if cost > dist[node] {
                continue; // stale entry
            }
            for &(neighbor, weight) in &self.adj[node] {
                let new_cost = cost + weight;
                if new_cost < dist[neighbor] {
                    dist[neighbor] = new_cost;
                    prev[neighbor] = Some(node);
                    heap.push(Reverse((new_cost, neighbor)));
                }
            }
        }
        (dist, prev)
    }

    /// Reconstruct path from Dijkstra's prev array.
    pub fn reconstruct_path(prev: &[Option<usize>], target: usize) -> Vec<usize> {
        let mut path = Vec::new();
        let mut cur = Some(target);
        while let Some(node) = cur {
            path.push(node);
            cur = prev[node];
        }
        path.reverse();
        path
    }

    // =========================================================================
    // Topological Sort (Kahn's algorithm) — O(V + E)
    // Only valid for directed acyclic graphs (DAG)
    // =========================================================================

    pub fn topological_sort(&self) -> Option<Vec<usize>> {
        let mut in_degree = vec![0usize; self.num_nodes];
        for node in 0..self.num_nodes {
            for &(neighbor, _) in &self.adj[node] {
                in_degree[neighbor] += 1;
            }
        }

        let mut queue: VecDeque<usize> = in_degree
            .iter()
            .enumerate()
            .filter(|(_, &d)| d == 0)
            .map(|(i, _)| i)
            .collect();

        let mut order = Vec::new();

        while let Some(node) = queue.pop_front() {
            order.push(node);
            for &(neighbor, _) in &self.adj[node] {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    queue.push_back(neighbor);
                }
            }
        }

        if order.len() == self.num_nodes {
            Some(order)
        } else {
            None // cycle detected
        }
    }

    // =========================================================================
    // Cycle Detection (directed graph using DFS coloring)
    // =========================================================================

    pub fn has_cycle(&self) -> bool {
        // 0 = white (unvisited), 1 = gray (in progress), 2 = black (done)
        let mut color = vec![0u8; self.num_nodes];

        for start in 0..self.num_nodes {
            if color[start] == 0 && self.cycle_dfs(start, &mut color) {
                return true;
            }
        }
        false
    }

    fn cycle_dfs(&self, node: usize, color: &mut [u8]) -> bool {
        color[node] = 1;
        for &(neighbor, _) in &self.adj[node] {
            if color[neighbor] == 1 {
                return true; // back edge = cycle
            }
            if color[neighbor] == 0 && self.cycle_dfs(neighbor, color) {
                return true;
            }
        }
        color[node] = 2;
        false
    }

    // =========================================================================
    // Connected Components (undirected)
    // =========================================================================

    pub fn connected_components(&self) -> Vec<Vec<usize>> {
        let mut visited = vec![false; self.num_nodes];
        let mut components = Vec::new();

        for start in 0..self.num_nodes {
            if !visited[start] {
                let mut component = Vec::new();
                let mut queue = VecDeque::new();
                visited[start] = true;
                queue.push_back(start);

                while let Some(node) = queue.pop_front() {
                    component.push(node);
                    for &(neighbor, _) in &self.adj[node] {
                        if !visited[neighbor] {
                            visited[neighbor] = true;
                            queue.push_back(neighbor);
                        }
                    }
                }
                components.push(component);
            }
        }
        components
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Graph BFS/DFS ===");
    // Undirected graph:
    //   0 -- 1 -- 3
    //   |    |
    //   2    4
    let mut g = Graph::new(5, false);
    g.add_edge(0, 1, 1);
    g.add_edge(0, 2, 1);
    g.add_edge(1, 3, 1);
    g.add_edge(1, 4, 1);
    println!("BFS from 0: {:?}", g.bfs(0));
    println!("DFS from 0: {:?}", g.dfs(0));
    println!("Components: {:?}", g.connected_components());

    println!("\n=== Dijkstra ===");
    //   0 --(4)--> 1 --(1)--> 3
    //   |                      ^
    //   (2)                   (3)
    //   |                      |
    //   v                      |
    //   2 ---------(5)-------> 3
    let mut wg = Graph::new(4, true);
    wg.add_edge(0, 1, 4);
    wg.add_edge(0, 2, 2);
    wg.add_edge(1, 3, 1);
    wg.add_edge(2, 3, 5);
    let (dist, prev) = wg.dijkstra(0);
    println!("Distances from 0: {:?}", dist);
    let path = Graph::reconstruct_path(&prev, 3);
    println!("Shortest path to 3: {:?} (cost={})", path, dist[3]);

    println!("\n=== Topological Sort ===");
    // DAG: 5 -> 0, 5 -> 2, 4 -> 0, 4 -> 1, 2 -> 3, 3 -> 1
    let mut dag = Graph::new(6, true);
    dag.add_edge(5, 0, 1);
    dag.add_edge(5, 2, 1);
    dag.add_edge(4, 0, 1);
    dag.add_edge(4, 1, 1);
    dag.add_edge(2, 3, 1);
    dag.add_edge(3, 1, 1);
    println!("Topo sort: {:?}", dag.topological_sort());
    println!("Has cycle: {}", dag.has_cycle());

    // Add a cycle
    let mut cyclic = Graph::new(3, true);
    cyclic.add_edge(0, 1, 1);
    cyclic.add_edge(1, 2, 1);
    cyclic.add_edge(2, 0, 1);
    println!("Cyclic graph has cycle: {}", cyclic.has_cycle());
}
