# ZooKeeper

## What is ZooKeeper?

ZooKeeper is a distributed coordination service for:
- **Configuration management**
- **Leader election**
- **Distributed locking**
- **Service discovery**
- **Group membership**

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
