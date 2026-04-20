# ZooKeeper

## What is ZooKeeper?

ZooKeeper is a distributed coordination service for:
- **Configuration management**
- **Leader election**
- **Distributed locking**
- **Service discovery**
- **Group membership**

## History & Why It Exists

```
The problem (2006):
  Every distributed system needs coordination:
    - Which node is the leader? (leader election)
    - What's the current cluster configuration? (config management)
    - How do we prevent two nodes from doing the same work? (distributed lock)
    - Which nodes are alive? (service discovery / group membership)

  Before ZooKeeper, every system implemented these from scratch:
    Hadoop had its own NameNode failover logic.
    HBase had its own region assignment coordination.
    Every system re-invented consensus, and most got it wrong.

  Yahoo engineers (Patrick Hunt, Mahadev Konar, et al.) built ZooKeeper:
  a GENERAL-PURPOSE coordination service that any distributed system
  can use. Implement leader election ONCE, correctly, and share it.

  The name: "ZooKeeper" because it coordinates distributed systems,
  which they jokingly called a "zoo" of services (Pig, Hive, HBase...).

Timeline:
  2006  Built at Yahoo for Hadoop coordination
  2008  Open-sourced as part of Hadoop project
  2011  Apache top-level project
  2010s ZooKeeper becomes the de facto coordination service for:
        Kafka (broker coordination, partition assignment)
        HBase (region server coordination)
        Hadoop (NameNode HA, YARN)
        Solr/SolrCloud (cluster state)
  2020s Being replaced in some systems:
        Kafka → KRaft (built-in consensus, no ZK dependency)
        etcd → preferred for Kubernetes coordination
        But ZooKeeper still runs in thousands of clusters.

ZooKeeper vs etcd vs Consul:
  ZooKeeper: Java, ZAB consensus, oldest, battle-tested
  etcd:      Go, Raft consensus, Kubernetes-native, simpler API
  Consul:    Go, Raft consensus, built-in service mesh, HashiCorp

  ZooKeeper was first and most widely deployed.
  etcd is winning in new deployments (Kubernetes ecosystem).
  Consul is strong for service mesh use cases.

Key design philosophy:
  - Small data, high reads: NOT a database. Stores kilobytes of metadata.
  - Sequential consistency: reads may be stale, but always in order.
  - Linearizable writes: all writes go through a single leader.
  - Watches: clients subscribe to changes (notification-based, not polling).
  - Ephemeral nodes: disappear when client disconnects (heartbeat-based).
    This is how you detect node failure → leader election, service discovery.
  - ZAB protocol: ZooKeeper Atomic Broadcast (similar to Paxos/Raft).

Who uses it:
  Kafka (until KRaft migration), HBase, Hadoop, Solr, LinkedIn,
  Twitter, eBay. Millions of production clusters worldwide.
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│               ZooKeeper Ensemble (cluster)                        │
│                                                                   │
│  Typical: 3 or 5 nodes (odd number for majority voting)          │
│                                                                   │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐             │
│  │   Leader    │  │  Follower  │  │  Follower  │             │
│  │            │  │            │  │            │             │
│  │ All writes │─►│ Replicates │  │ Replicates │             │
│  │ go here    │  │ from leader│  │ from leader│             │
│  │            │  │ Serves     │  │ Serves     │             │
│  │            │  │ reads      │  │ reads      │             │
│  └──────┬─────┘  └──────┬─────┘  └──────┬─────┘             │
│         │              │              │                         │
│         └────── ZAB protocol (atomic broadcast) ────┘         │
│                                                                   │
│  Leader elected by majority vote.                                │
│  WRITES: client → any node → forwarded to leader                │
│          → leader proposes → majority ack → committed            │
│  READS:  served by ANY node (may be slightly stale)              │
│          sync read: client can force read from leader             │
└──────────────────────────────────────────────────────────────────┘

SINGLE SERVER INTERNALS:
  ┌───────────────────────────────────────────────────────┐
  │  Client request (create /leader, set /config)          │
  │       │                                                │
  │       ▼                                                │
  │  PrepRequestProcessor                                  │
  │    └─► validate request, check ACLs, create txn        │
  │       │                                                │
  │       ▼                                                │
  │  SyncRequestProcessor                                  │
  │    └─► write txn to transaction log (WAL on disk)      │
  │    └─► periodic snapshots of entire ZNode tree          │
  │       │                                                │
  │       ▼                                                │
  │  FinalRequestProcessor                                 │
  │    └─► apply txn to in-memory ZNode tree               │
  │    └─► trigger watches (notify subscribed clients)      │
  │    └─► return response to client                       │
  │                                                        │
  │  Data model: entirely in MEMORY                        │
  │    └─► all ZNodes + data in RAM (fast reads)           │
  │    └─► transaction log on disk (durability)             │
  │    └─► snapshots on disk (faster recovery)              │
  │                                                        │
  │  NOT a database — stores kilobytes, not gigabytes.     │
  │  Designed for metadata: configs, locks, leader info.    │
  └───────────────────────────────────────────────────────┘

SESSION & WATCHES:
  Client opens a SESSION with the ensemble (heartbeat-based).
  If client dies (heartbeat stops) → ephemeral nodes deleted.

  WATCHES: client subscribes to changes on a ZNode.
  When that ZNode changes → server sends ONE notification.
  Client must re-register watch to get next notification.
  (This is push-based, not polling — efficient for coordination.)
```

## Core Concepts

### ZNodes (like files/directories)
```
/
├── /config
│   └── /config/database
├── /election
│   ├── /election/candidate-0000000001
│   └── /election/candidate-0000000002
└── /services
    └── /services/api
        ├── /services/api/server-1
        └── /services/api/server-2
```

### ZNode Types

| Type | Description | Use Case |
|------|-------------|----------|
| **Persistent** | Exists until explicitly deleted | Configuration |
| **Ephemeral** | Deleted when session ends | Service discovery, locks |
| **Sequential** | Has monotonically increasing suffix | Leader election |
| **Ephemeral Sequential** | Both ephemeral and sequential | Distributed queue |

### Sessions
- Client maintains heartbeat with ZK
- Session timeout = client presumed dead
- Ephemeral nodes deleted on session end

### Watches
- One-time triggers on node changes
- Types: NodeCreated, NodeDeleted, DataChanged, ChildrenChanged

## Leader Election Pattern

```
┌─────────────────────────────────────────────────────────────────┐
│                       Leader Election                            │
│                                                                  │
│  /election                                                       │
│      │                                                          │
│      ├── candidate-0000000001  ◄── Leader (lowest seq)          │
│      ├── candidate-0000000002  ◄── Watches 0001                 │
│      └── candidate-0000000003  ◄── Watches 0002                 │
│                                                                  │
│  Algorithm:                                                      │
│  1. Create /election/candidate- (ephemeral sequential)          │
│  2. Get all children, sort by sequence                          │
│  3. If lowest, I'm the leader                                   │
│  4. Else, watch the node just before me                         │
│  5. On watch trigger, repeat from step 2                        │
│                                                                  │
│  Why watch previous only?                                        │
│  - Avoids "herd effect" (all watching leader)                   │
│  - O(1) notifications vs O(N)                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Distributed Lock Pattern

```
┌─────────────────────────────────────────────────────────────────┐
│                       Distributed Lock                           │
│                                                                  │
│  /locks/resource-1                                               │
│      │                                                          │
│      ├── lock-0000000001  ◄── Lock holder                       │
│      ├── lock-0000000002  ◄── Waiting (watches 0001)            │
│      └── lock-0000000003  ◄── Waiting (watches 0002)            │
│                                                                  │
│  Acquire:                                                        │
│  1. Create /locks/resource-1/lock- (ephemeral sequential)       │
│  2. Get children, sort                                          │
│  3. If I'm lowest, lock acquired                                │
│  4. Else, watch node just before me                             │
│                                                                  │
│  Release:                                                        │
│  1. Delete my lock node                                         │
│  2. Next waiter automatically notified                          │
│                                                                  │
│  Fault tolerance:                                                │
│  - Client dies → ephemeral node deleted → lock released         │
└─────────────────────────────────────────────────────────────────┘
```

## Service Discovery Pattern

```
┌─────────────────────────────────────────────────────────────────┐
│                      Service Discovery                           │
│                                                                  │
│  /services/api                                                   │
│      │                                                          │
│      ├── server-1 (ephemeral)  data: "10.0.0.1:8080"            │
│      ├── server-2 (ephemeral)  data: "10.0.0.2:8080"            │
│      └── server-3 (ephemeral)  data: "10.0.0.3:8080"            │
│                                                                  │
│  Registration:                                                   │
│  1. Service starts                                              │
│  2. Create ephemeral node with address as data                  │
│  3. Maintain session heartbeat                                  │
│                                                                  │
│  Discovery:                                                      │
│  1. Get children of /services/api                               │
│  2. Read data from each child (get addresses)                   │
│  3. Set watch for changes                                       │
│                                                                  │
│  Failure handling:                                               │
│  - Server crashes → session ends → node deleted                 │
│  - Clients watching get notified                                │
│  - Remove dead server from load balancer                        │
└─────────────────────────────────────────────────────────────────┘
```

## ZAB Protocol (ZooKeeper Atomic Broadcast)

```
┌─────────────────────────────────────────────────────────────────┐
│                         ZAB Protocol                             │
│                                                                  │
│  Leader (1)                                                      │
│      │                                                          │
│      │◄─── All writes go to leader                              │
│      │                                                          │
│      ├──────────────────┐                                       │
│      ▼                  ▼                                       │
│  Follower (2)      Follower (3)                                 │
│                                                                  │
│  Write:                                                          │
│  1. Client sends write to any server                            │
│  2. Forwarded to leader                                         │
│  3. Leader proposes to all followers                            │
│  4. Followers ACK                                               │
│  5. Leader commits when quorum ACKs                             │
│  6. Leader tells followers to commit                            │
│                                                                  │
│  Quorum: (N/2) + 1                                              │
│  - 3 nodes: need 2 for quorum (1 failure tolerance)             │
│  - 5 nodes: need 3 for quorum (2 failure tolerance)             │
└─────────────────────────────────────────────────────────────────┘
```

## Implementation

Our implementation demonstrates:
1. Basic ZNode operations
2. Leader election
3. Distributed locks
4. Service discovery with ephemeral nodes

Run the demo:
```bash
cargo run --bin mini-zookeeper
```
