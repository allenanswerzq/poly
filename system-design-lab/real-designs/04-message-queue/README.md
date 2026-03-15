# Message Queue Design (Kafka-like)

## Problem Statement

Design a distributed message queue that:
- Handles millions of messages per second
- Guarantees message ordering within partitions
- Provides at-least-once delivery
- Supports multiple consumers

## Key Concepts

### Topics and Partitions

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Topic: "orders"                                 │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Partition 0:  [msg0] [msg3] [msg6] [msg9]  ...                      │    │
│  │               offset: 0    1     2     3                            │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Partition 1:  [msg1] [msg4] [msg7] [msg10] ...                      │    │
│  │               offset: 0    1     2     3                            │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │ Partition 2:  [msg2] [msg5] [msg8] [msg11] ...                      │    │
│  │               offset: 0    1     2     3                            │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  Partitioning Key: hash(order_id) % num_partitions                          │
│  Ordering: Guaranteed within partition, not across                          │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Consumer Groups

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Consumer Group: "processors"                        │
│                                                                              │
│  Topic: orders                                                               │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐                               │
│  │Partition 0│  │Partition 1│  │Partition 2│                               │
│  └─────┬─────┘  └─────┬─────┘  └─────┬─────┘                               │
│        │              │              │                                       │
│        ▼              ▼              ▼                                       │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐                               │
│  │Consumer 1 │  │Consumer 2 │  │Consumer 3 │                               │
│  │ offset: 5 │  │ offset: 8 │  │ offset: 3 │                               │
│  └───────────┘  └───────────┘  └───────────┘                               │
│                                                                              │
│  Rule: Each partition assigned to exactly one consumer in group             │
│  Benefit: Parallel processing with ordering guarantee                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Message Delivery Semantics

| Semantic | Description | How |
|----------|-------------|-----|
| **At-most-once** | Message may be lost | Fire and forget |
| **At-least-once** | Message never lost, may duplicate | Ack after processing |
| **Exactly-once** | Perfect delivery | Idempotency + transactions |

### Log-based Storage

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Append-Only Log                                     │
│                                                                              │
│   ┌──────┬──────┬──────┬──────┬──────┬──────┬──────┬──────┐                │
│   │ msg0 │ msg1 │ msg2 │ msg3 │ msg4 │ msg5 │ msg6 │ ...  │                │
│   └──────┴──────┴──────┴──────┴──────┴──────┴──────┴──────┘                │
│   offset: 0     1      2      3      4      5      6                        │
│                                                                              │
│   - Append is O(1)                                                          │
│   - Read by offset is O(1)                                                  │
│   - Immutable once written                                                   │
│   - Retention: time-based (7 days) or size-based (1TB)                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Message Queue Architecture                            │
│                                                                              │
│  Producers                     Brokers                      Consumers        │
│  ┌─────────┐                                               ┌─────────────┐  │
│  │Producer1│──┐            ┌─────────────┐              ┌──│  Consumer   │  │
│  └─────────┘  │            │   Broker 1  │              │  │  Group A    │  │
│               │ ┌─────────►│ Partitions  │◄─────────────┘  └─────────────┘  │
│  ┌─────────┐  │ │          │  0, 3, 6    │                                  │
│  │Producer2│──┼─┤          └─────────────┘                                  │
│  └─────────┘  │ │                                          ┌─────────────┐  │
│               │ │          ┌─────────────┐              ┌──│  Consumer   │  │
│  ┌─────────┐  │ ├─────────►│   Broker 2  │◄─────────────┤  │  Group B    │  │
│  │Producer3│──┘ │          │ Partitions  │              │  └─────────────┘  │
│  └─────────┘    │          │  1, 4, 7    │              │                   │
│                 │          └─────────────┘              │                   │
│                 │                                       │                   │
│                 │          ┌─────────────┐              │                   │
│                 └─────────►│   Broker 3  │◄─────────────┘                   │
│                            │ Partitions  │                                  │
│                            │  2, 5, 8    │                                  │
│                            └─────────────┘                                  │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                         ZooKeeper Cluster                                ││
│  │          (Broker coordination, consumer group management)               ││
│  └─────────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

| Component | Design Choice | Rationale |
|-----------|---------------|-----------|
| Storage | Append-only log files | Fast writes, sequential reads |
| Ordering | Per-partition only | Enables parallelism |
| Delivery | Pull-based | Consumer controls rate |
| Replication | Leader-follower per partition | High availability |
| Offset tracking | Consumer responsibility | Flexibility |

## Implementation

Our implementation includes:
1. In-memory topic with partitions
2. Producer with partitioning
3. Consumer groups with offset tracking
4. At-least-once delivery

Run the demo:
```bash
cargo run --bin message-queue
```
