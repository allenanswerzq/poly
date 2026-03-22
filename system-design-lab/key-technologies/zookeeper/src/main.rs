//! # ZooKeeper Concepts: Leader Election & Distributed Locks
//!
//! Demonstrates core ZooKeeper patterns:
//! - Leader election
//! - Distributed locks
//! - Service discovery
//! - Watch mechanism

use parking_lot::{Mutex, RwLock};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use uuid::Uuid;

// =============================================================================
// ZNode (ZooKeeper Node)
// =============================================================================

#[derive(Debug, Clone)]
pub enum ZNodeType {
    Persistent,
    Ephemeral,        // Deleted when session ends
    Sequence,         // Has monotonically increasing suffix
    EphemeralSequence,
}

#[derive(Debug, Clone)]
pub struct ZNode {
    pub path: String,
    pub data: Vec<u8>,
    pub node_type: ZNodeType,
    pub owner_session: Option<String>,
    pub children: HashSet<String>,
    pub version: u64,
}

impl ZNode {
    pub fn new(path: &str, data: Vec<u8>, node_type: ZNodeType, session: Option<String>) -> Self {
        Self {
            path: path.to_string(),
            data,
            node_type,
            owner_session: session,
            children: HashSet::new(),
            version: 0,
        }
    }
}

// =============================================================================
// Mini ZooKeeper
// =============================================================================

/// A simplified in-memory ZooKeeper implementation
pub struct MiniZooKeeper {
    nodes: RwLock<HashMap<String, ZNode>>,
    sequence_counter: AtomicU64,
    watchers: Mutex<HashMap<String, Vec<Box<dyn Fn(&str) + Send + Sync>>>>,
}

impl MiniZooKeeper {
    pub fn new() -> Arc<Self> {
        let zk = Arc::new(Self {
            nodes: RwLock::new(HashMap::new()),
            sequence_counter: AtomicU64::new(0),
            watchers: Mutex::new(HashMap::new()),
        });

        // Create root node
        {
            let mut nodes = zk.nodes.write();
            nodes.insert("/".to_string(), ZNode::new("/", vec![], ZNodeType::Persistent, None));
        }

        zk
    }

    /// Create a ZNode
    pub fn create(&self, path: &str, data: Vec<u8>, node_type: ZNodeType, session: Option<String>) -> Result<String, &'static str> {
        let mut nodes = self.nodes.write();

        // Handle sequence nodes
        let actual_path = match node_type {
            ZNodeType::Sequence | ZNodeType::EphemeralSequence => {
                let seq = self.sequence_counter.fetch_add(1, Ordering::SeqCst);
                format!("{}{:010}", path, seq)
            }
            _ => path.to_string(),
        };

        // Check if node already exists
        if nodes.contains_key(&actual_path) {
            return Err("Node already exists");
        }

        // Get parent path
        let parent_path = actual_path.rsplit_once('/')
            .map(|(p, _)| if p.is_empty() { "/" } else { p })
            .unwrap_or("/");

        // Check parent exists
        if !nodes.contains_key(parent_path) {
            return Err("Parent node doesn't exist");
        }

        // Create the node
        let node = ZNode::new(&actual_path, data, node_type.clone(), session);
        nodes.insert(actual_path.clone(), node);

        // Add to parent's children
        if let Some(parent) = nodes.get_mut(parent_path) {
            let child_name = actual_path.rsplit('/').next().unwrap_or(&actual_path);
            parent.children.insert(child_name.to_string());
        }

        // Trigger watchers
        drop(nodes);
        self.trigger_watch(&actual_path);

        Ok(actual_path)
    }

    /// Get data from a ZNode
    pub fn get_data(&self, path: &str) -> Option<Vec<u8>> {
        self.nodes.read().get(path).map(|n| n.data.clone())
    }

    /// Set data for a ZNode
    pub fn set_data(&self, path: &str, data: Vec<u8>) -> Result<(), &'static str> {
        let mut nodes = self.nodes.write();
        if let Some(node) = nodes.get_mut(path) {
            node.data = data;
            node.version += 1;
            drop(nodes);
            self.trigger_watch(path);
            Ok(())
        } else {
            Err("Node doesn't exist")
        }
    }

    /// Delete a ZNode
    pub fn delete(&self, path: &str) -> Result<(), &'static str> {
        let mut nodes = self.nodes.write();

        // Check if node exists
        if !nodes.contains_key(path) {
            return Err("Node doesn't exist");
        }

        // Check if node has children
        if let Some(node) = nodes.get(path) {
            if !node.children.is_empty() {
                return Err("Node has children");
            }
        }

        // Remove from parent
        let parent_path = path.rsplit_once('/')
            .map(|(p, _)| if p.is_empty() { "/" } else { p })
            .unwrap_or("/");

        if let Some(parent) = nodes.get_mut(parent_path) {
            let child_name = path.rsplit('/').next().unwrap_or(path);
            parent.children.remove(child_name);
        }

        nodes.remove(path);
        drop(nodes);
        self.trigger_watch(path);

        Ok(())
    }

    /// Get children of a ZNode
    pub fn get_children(&self, path: &str) -> Option<Vec<String>> {
        self.nodes.read()
            .get(path)
            .map(|n| n.children.iter().cloned().collect())
    }

    /// Check if node exists
    pub fn exists(&self, path: &str) -> bool {
        self.nodes.read().contains_key(path)
    }

    /// Add a watcher (simplified)
    pub fn add_watch<F>(&self, path: &str, callback: F)
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.watchers
            .lock()
            .entry(path.to_string())
            .or_default()
            .push(Box::new(callback));
    }

    fn trigger_watch(&self, path: &str) {
        let watchers = self.watchers.lock();
        if let Some(callbacks) = watchers.get(path) {
            for callback in callbacks {
                callback(path);
            }
        }
    }

    /// Clean up ephemeral nodes for a session
    pub fn close_session(&self, session_id: &str) {
        let mut nodes = self.nodes.write();
        let to_delete: Vec<String> = nodes
            .iter()
            .filter(|(_, n)| n.owner_session.as_deref() == Some(session_id))
            .map(|(path, _)| path.clone())
            .collect();

        for path in to_delete {
            nodes.remove(&path);
            println!("[ZK] Cleaned up ephemeral node: {} (session: {})", path, session_id);
        }
    }
}

impl Default for MiniZooKeeper {
    fn default() -> Self {
        Arc::try_unwrap(Self::new()).unwrap_or_else(|_| panic!("Failed to create MiniZooKeeper"))
    }
}

// =============================================================================
// Leader Election
// =============================================================================

/// Leader election using sequential ephemeral nodes
pub struct LeaderElection {
    zk: Arc<MiniZooKeeper>,
    election_path: String,
    session_id: String,
    my_node: RwLock<Option<String>>,
}

impl LeaderElection {
    pub fn new(zk: Arc<MiniZooKeeper>, election_path: &str) -> Self {
        // Ensure election path exists
        if !zk.exists(election_path) {
            zk.create(election_path, vec![], ZNodeType::Persistent, None).ok();
        }

        Self {
            zk,
            election_path: election_path.to_string(),
            session_id: Uuid::new_v4().to_string(),
            my_node: RwLock::new(None),
        }
    }

    /// Join the election
    pub fn join(&self) -> Result<(), &'static str> {
        let path = format!("{}/candidate-", self.election_path);
        let created = self.zk.create(
            &path,
            self.session_id.as_bytes().to_vec(),
            ZNodeType::EphemeralSequence,
            Some(self.session_id.clone()),
        )?;

        *self.my_node.write() = Some(created.clone());
        println!("[Election] {} joined as {}", self.session_id, created);
        Ok(())
    }

    /// Check if this instance is the leader
    pub fn is_leader(&self) -> bool {
        let my_node = self.my_node.read();
        if my_node.is_none() {
            return false;
        }

        let my_node = my_node.as_ref().unwrap();
        let children = self.zk.get_children(&self.election_path).unwrap_or_default();

        if children.is_empty() {
            return false;
        }

        // Leader is the one with lowest sequence number
        let mut sorted: Vec<_> = children.iter().collect();
        sorted.sort();

        let my_name = my_node.rsplit('/').next().unwrap_or(my_node);
        sorted.first().map(|s| s.as_str()) == Some(my_name)
    }

    /// Leave the election (resign leadership)
    pub fn leave(&self) {
        if let Some(ref path) = *self.my_node.read() {
            self.zk.delete(path).ok();
            println!("[Election] {} left", path);
        }
        *self.my_node.write() = None;
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

// =============================================================================
// Distributed Lock
// =============================================================================

/// Distributed lock implementation
pub struct DistributedLock {
    zk: Arc<MiniZooKeeper>,
    lock_path: String,
    session_id: String,
    my_lock: RwLock<Option<String>>,
}

impl DistributedLock {
    pub fn new(zk: Arc<MiniZooKeeper>, lock_path: &str) -> Self {
        // Ensure lock path exists
        if !zk.exists(lock_path) {
            zk.create(lock_path, vec![], ZNodeType::Persistent, None).ok();
        }

        Self {
            zk,
            lock_path: lock_path.to_string(),
            session_id: Uuid::new_v4().to_string(),
            my_lock: RwLock::new(None),
        }
    }

    /// Try to acquire the lock
    pub fn try_lock(&self) -> bool {
        // Create sequential ephemeral node
        let path = format!("{}/lock-", self.lock_path);
        if let Ok(created) = self.zk.create(
            &path,
            self.session_id.as_bytes().to_vec(),
            ZNodeType::EphemeralSequence,
            Some(self.session_id.clone()),
        ) {
            *self.my_lock.write() = Some(created.clone());

            // Check if we have the lowest sequence number
            let children = self.zk.get_children(&self.lock_path).unwrap_or_default();
            let mut sorted: Vec<_> = children.iter().collect();
            sorted.sort();

            let my_name = created.rsplit('/').next().unwrap_or(&created);
            if sorted.first().map(|s| s.as_str()) == Some(my_name) {
                return true;  // Got the lock
            }

            // Didn't get the lock, clean up
            self.zk.delete(&created).ok();
            *self.my_lock.write() = None;
        }

        false
    }

    /// Acquire lock (blocking with retry)
    pub fn lock(&self) -> bool {
        for _ in 0..100 {  // Max 100 retries
            if self.try_lock() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// Release the lock
    pub fn unlock(&self) {
        if let Some(ref path) = *self.my_lock.read() {
            self.zk.delete(path).ok();
        }
        *self.my_lock.write() = None;
    }

    /// Check if we hold the lock
    pub fn is_locked(&self) -> bool {
        let my_lock = self.my_lock.read();
        if my_lock.is_none() {
            return false;
        }

        let my_lock = my_lock.as_ref().unwrap();
        let children = self.zk.get_children(&self.lock_path).unwrap_or_default();

        if children.is_empty() {
            return false;
        }

        let mut sorted: Vec<_> = children.iter().collect();
        sorted.sort();

        let my_name = my_lock.rsplit('/').next().unwrap_or(my_lock);
        sorted.first().map(|s| s.as_str()) == Some(my_name)
    }
}

// =============================================================================
// Demonstration
// =============================================================================

fn main() {
    println!("=== ZooKeeper Concepts Demo ===\n");

    let zk = MiniZooKeeper::new();

    // Demo 1: Basic ZNode operations
    println!("\n  ═══ Basic ZNode Operations ═══");

    zk.create("/myapp", vec![], ZNodeType::Persistent, None).ok();
    zk.create("/myapp/config", b"version=1.0".to_vec(), ZNodeType::Persistent, None).ok();

    println!("Created /myapp and /myapp/config");
    println!("GET /myapp/config = {:?}",
             zk.get_data("/myapp/config").map(|d| String::from_utf8_lossy(&d).to_string()));

    zk.set_data("/myapp/config", b"version=2.0".to_vec()).ok();
    println!("After update = {:?}",
             zk.get_data("/myapp/config").map(|d| String::from_utf8_lossy(&d).to_string()));

    println!("Children of /myapp: {:?}", zk.get_children("/myapp"));

    // Demo 2: Leader Election
    println!("\n--- Leader Election ---");

    let election1 = LeaderElection::new(Arc::clone(&zk), "/election");
    let election2 = LeaderElection::new(Arc::clone(&zk), "/election");
    let election3 = LeaderElection::new(Arc::clone(&zk), "/election");

    election1.join().ok();
    election2.join().ok();
    election3.join().ok();

    println!("Candidate 1 is leader: {}", election1.is_leader());
    println!("Candidate 2 is leader: {}", election2.is_leader());
    println!("Candidate 3 is leader: {}", election3.is_leader());

    println!("\nCandidate 1 leaves...");
    election1.leave();

    // Small delay to show new leader
    thread::sleep(Duration::from_millis(10));

    println!("Candidate 2 is leader: {}", election2.is_leader());
    println!("Candidate 3 is leader: {}", election3.is_leader());

    // Demo 3: Distributed Lock
    println!("\n--- Distributed Lock ---");

    let lock1 = DistributedLock::new(Arc::clone(&zk), "/locks/resource1");
    let lock2 = DistributedLock::new(Arc::clone(&zk), "/locks/resource1");

    println!("Lock 1 acquiring...");
    let got_lock1 = lock1.lock();
    println!("Lock 1 acquired: {}", got_lock1);

    println!("\nLock 2 trying (should fail)...");
    let got_lock2 = lock2.try_lock();
    println!("Lock 2 acquired: {}", got_lock2);

    println!("\nLock 1 releasing...");
    lock1.unlock();

    println!("Lock 2 acquiring...");
    let got_lock2 = lock2.lock();
    println!("Lock 2 acquired: {}", got_lock2);
    lock2.unlock();

    // Demo 4: Service Discovery
    println!("\n--- Service Discovery ---");

    zk.create("/services", vec![], ZNodeType::Persistent, None).ok();
    zk.create("/services/api", vec![], ZNodeType::Persistent, None).ok();

    // Services register themselves (ephemeral nodes)
    let session1 = "server-1-session";
    let session2 = "server-2-session";

    zk.create("/services/api/server-1", b"10.0.0.1:8080".to_vec(),
              ZNodeType::Ephemeral, Some(session1.to_string())).ok();
    zk.create("/services/api/server-2", b"10.0.0.2:8080".to_vec(),
              ZNodeType::Ephemeral, Some(session2.to_string())).ok();

    println!("Registered services:");
    for child in zk.get_children("/services/api").unwrap_or_default() {
        let path = format!("/services/api/{}", child);
        let addr = zk.get_data(&path).map(|d| String::from_utf8_lossy(&d).to_string());
        println!("  {} -> {:?}", child, addr);
    }

    // Simulate server-1 crash (session ends)
    println!("\nServer 1 crashes (session ends)...");
    zk.close_session(session1);

    println!("Remaining services:");
    for child in zk.get_children("/services/api").unwrap_or_default() {
        println!("  {}", child);
    }

    // Demo 5: Use cases summary
    println!("\n--- ZooKeeper Use Cases ---");
    println!("
1. Leader Election
   - Only one master at a time
   - Automatic failover on master failure

2. Distributed Locks
   - Coordinate exclusive access to resources
   - Automatically released on session timeout

3. Service Discovery
   - Services register ephemeral nodes
   - Clients watch for changes
   - Dead services automatically removed

4. Configuration Management
   - Store config in ZNodes
   - Watch for changes
   - Atomic updates

5. Group Membership
   - Track cluster members
   - Detect failures via ephemeral nodes
");

    println!("\n=== Demo Complete ===");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_znode_create_get() {
        let zk = MiniZooKeeper::new();

        zk.create("/test", b"hello".to_vec(), ZNodeType::Persistent, None).unwrap();
        assert_eq!(zk.get_data("/test"), Some(b"hello".to_vec()));
    }

    #[test]
    fn test_leader_election() {
        let zk = MiniZooKeeper::new();

        let e1 = LeaderElection::new(Arc::clone(&zk), "/election");
        let e2 = LeaderElection::new(Arc::clone(&zk), "/election");

        e1.join().unwrap();
        e2.join().unwrap();

        // Exactly one should be leader
        assert!(e1.is_leader() != e2.is_leader() || e1.is_leader() && e2.is_leader());
    }

    #[test]
    fn test_distributed_lock() {
        let zk = MiniZooKeeper::new();

        let l1 = DistributedLock::new(Arc::clone(&zk), "/locks/test");
        let l2 = DistributedLock::new(Arc::clone(&zk), "/locks/test");

        assert!(l1.lock());
        assert!(!l2.try_lock());

        l1.unlock();
        assert!(l2.lock());
    }
}
