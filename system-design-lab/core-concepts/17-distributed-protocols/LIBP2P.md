# libp2p & P2P Networking Protocols

## What libp2p Is

libp2p is a modular networking stack for peer-to-peer applications. It handles transport, encryption, peer discovery, and multiplexing — so you focus on your protocol logic, not raw networking.

```
┌──────────────────────────────────────────────────────────────────────────┐
│                          libp2p Stack                                     │
│                                                                           │
│  Your protocol (Bitswap, your custom protocol, etc.)                     │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │ Application layer                                                  │  │
│  └────────────────────────────────────────────────────────────────────┘  │
│                                                                           │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐   │
│  │ Kademlia DHT │ │ GossipSub    │ │ Identify     │ │ mDNS         │   │
│  │ (routing)    │ │ (pub/sub)    │ │ (handshake)  │ │ (LAN disco.) │   │
│  └──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘   │
│                                                                           │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │ Swarm — manages connections, protocols, events                     │  │
│  └────────────────────────────────────────────────────────────────────┘  │
│                                                                           │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐                    │
│  │ Yamux/Mplex  │ │ Noise/TLS    │ │ TCP/QUIC/    │                    │
│  │ (mux)        │ │ (encryption) │ │ WebRTC/WS    │                    │
│  └──────────────┘ └──────────────┘ └──────────────┘                    │
│                                                                           │
│  Used by: IPFS, Ethereum (consensus), Filecoin, Polkadot, Celestia     │
└──────────────────────────────────────────────────────────────────────────┘
```

## Content Discovery — "Who Has File X?"

Two main approaches in the libp2p ecosystem:

```
1. Kademlia DHT — "find who has content X" (content routing / lookup)
2. GossipSub   — "broadcast new content to interested peers" (pub/sub)

They solve DIFFERENT problems:
  Kademlia: point query   ("I need CID xyz, who has it?")
  GossipSub: broadcast    ("here's a new block for everyone subscribed")
```

---------------
Kademlia's trick: responsibility is assigned by closeness.

  "who stores the provider record for cid 1010?"
  → the k nodes with the smallest xor distance to 1010.

  If Node 1011 exists (XOR distance = 0001 = 1), it's responsible.
  If Node 1010 exists (XOR distance = 0000 = 0), it's responsible.

  This is deterministic. ANY node can compute "who SHOULD be
  responsible for CID X" by XORing X with known node IDs
  and picking the smallest results.

  No central authority needed. The math assigns responsibility.

---------------
THING 1: THE ACTUAL FILE (content bytes)
  Stored by: whoever downloaded/created it. ANY node.
  Lives on: their local disk.
  NOT determined by XOR distance. You choose to keep it or not.

THING 2: THE PROVIDER RECORD (a pointer: "CID xyz → Alice has it")
  Stored by: the K nodes closest to the CID by XOR distance.
  Lives on: their DHT record table (small metadata, not the file).
  Determined by XOR distance. These nodes are the "directory service."

The DHT is NOT storing files. It's storing POINTERS to files.
Like DNS doesn't store websites — it stores "google.com → 142.250.80.46"

## Kademlia DHT — Distributed Hash Table

```
Every node has a 256-bit ID (hash of public key).
Every piece of content has a CID (content ID = hash of content).

The DHT stores: CID → list of provider nodes
NOT the content itself — just POINTERS to who has it.

XOR Distance:
  distance(A, B) = A XOR B  (bitwise)

  Why XOR?
    - distance(A, A) = 0                    (identity)
    - distance(A, B) = distance(B, A)       (symmetric)
    - distance(A, C) ≤ distance(A, B) + distance(B, C)  (triangle inequality)
    - Uniform: flipping any bit changes distance unpredictably
    → Well-behaved metric that distributes nodes evenly in the space.

Routing Table (k-buckets):
  Each node maintains k-buckets, one for each bit of distance.

  ┌──────────────────────────────────────────────────────────────┐
  │ Node 0110's routing table:                                    │
  │                                                               │
  │ Bucket 0 (distance 1, differ in bit 0):    1 node  (close)  │
  │ Bucket 1 (distance 2-3, differ in bit 1):  2 nodes          │
  │ Bucket 2 (distance 4-7, differ in bit 2):  k nodes          │
  │ Bucket 3 (distance 8-15, differ in bit 3): k nodes          │
  │ ...                                                           │
  │ Bucket 7 (distance 128-255):               k nodes (far)    │
  │                                                               │
  │ Know MANY nodes in your neighborhood, FEW nodes far away.    │
  │ Total: ~O(log N) nodes in routing table.                     │
  │ Any lookup: O(log N) hops (each hop halves the distance).    │
  └──────────────────────────────────────────────────────────────┘
```

### PROVIDE — Announce "I Have Content X"

```
  1. Node hashes file → CID = sha256(file) = 0xABCD...
  2. Iterative lookup: find the K closest nodes to CID
     - Ask nodes in your routing table closest to CID
     - They return nodes THEY know that are even closer
     - Repeat until you converge on the K closest
  3. Send Provider Record to those K nodes:
     "I (peer_id=QmAlice) have content CID=0xABCD"
  4. Those K nodes store this mapping

  ┌────────┐   who's close     ┌────────┐  even closer   ┌────────┐
  │ Alice  │──to 0xABCD?─────►│ Node M │────────────────►│ Node X │
  │ (has   │                   │        │                  │(closest│
  │  file) │                   └────────┘                  │to CID) │
  │        │───── PROVIDE: "I have 0xABCD" ──────────────►│        │
  └────────┘                                               └────────┘
```

CONCRETE WALKTHROUGH:

  Alice (ID=0110) has a file. She wants others to find it.

  Step 1: Alice stores the file LOCALLY on her disk.
    alice_disk/blocks/0xABCD = [actual file bytes, 50MB]

  Step 2: Alice does PROVIDE — tells the DHT "I have CID 0xABCD"
    Find K=3 nodes closest to CID 0xABCD by XOR:
      Node 1011 (XOR dist 1 from CID)  ← gets provider record
      Node 1000 (XOR dist 2 from CID)  ← gets provider record
      Node 1110 (XOR dist 4 from CID)  ← gets provider record

    What's actually sent (simplified):
      ProviderRecord {
          cid: 0xABCD,
          provider: PeerId("QmAlice"),
          addresses: ["/ip4/1.2.3.4/tcp/4001"],
          ttl: 24h,
      }
    Size: ~200 bytes. NOT the file. Just a pointer.

    These 3 DHT nodes store it in memory/disk:
      node_1011.provider_store = {
          0xABCD → [QmAlice @ /ip4/1.2.3.4/tcp/4001],
          0xFFFF → [QmBob @ /ip4/5.6.7.8/tcp/4001],
          ...
      }

  Step 3: Bob (ID=0001) wants the file. Does FIND_PROVIDERS(0xABCD).
    Iterative lookup toward CID 0xABCD...
    Eventually reaches Node 1011 (close to CID).
    Node 1011 responds: "Alice (QmAlice @ 1.2.3.4:4001) has CID 0xABCD"

  Step 4: Bob connects DIRECTLY to Alice. Downloads the file.
    Bob → TCP connect to 1.2.3.4:4001 → Bitswap: "give me CID 0xABCD"
    Alice → sends the actual file bytes to Bob.

  Step 5 (optional): Bob now also has the file.
    Bob can PROVIDE too: send provider records to those same K nodes.
    Now both Alice and Bob are listed as providers.
    More providers = faster downloads + redundancy.

  ┌─────────────────────────────────────────────────────────────────────┐
  │                                                                     │
  │  Alice (has file)          DHT nodes near CID       Bob (wants file)│
  │  ┌───────────┐            ┌──────────────┐         ┌──────────────┐│
  │  │ disk:     │──PROVIDE──►│ pointer store:│◄─QUERY──│ "who has     ││
  │  │ CID=0xABCD│  (200B)   │ 0xABCD →     │ (FIND)  │  0xABCD?"    ││
  │  │ [50MB file]│           │   [Alice]     │────────►│              ││
  │  │           │            └──────────────┘ "Alice"  │              ││
  │  │           │◄──────── DIRECT DOWNLOAD ────────────│              ││
  │  │           │  (50MB, via Bitswap)                  │              ││
  │  └───────────┘                                       └──────────────┘│
  │                                                                     │
  │  The K DHT nodes never see the file. They only store pointers.     │
  │  The file transfer is peer-to-peer between Alice and Bob.          │
  └─────────────────────────────────────────────────────────────────────┘



### FIND_PROVIDERS — Lookup "Who Has Content X?"

```
  1. Compute CID = sha256(file) = 0xABCD...
  2. Iterative lookup toward CID (same as PROVIDE)
  3. Nodes close to CID return stored Provider Records
  4. Get back: [QmAlice, QmBob, QmCharlie] have this content
  5. Connect directly to one of them and download

  ┌────────┐  "who has 0xABCD?"   ┌────────┐  provider records  ┌────────┐
  │ Bob    │─────────────────────►│ Node X │─────────────────►  │ Alice  │
  │ (wants │  (iterative lookup)  │(stores │  "Alice has it"    │(has the│
  │  file) │                      │records)│                    │ file)  │
  │        │◄──── direct connect + download ────────────────────│        │
  └────────┘                      └────────┘                    └────────┘

  Lookup complexity: O(log N) hops for N nodes in the network.
  1M nodes → ~20 network round-trips to find any content.
```

### Iterative vs Recursive Lookup

```
Iterative (what libp2p Kademlia uses):
  Requester drives the lookup. Asks node A → A returns closer nodes
  → requester asks those nodes → they return even closer → ...
  Requester controls the entire process. Easier to debug, no amplification.

  You ──► A: "who's close to CID?"
  A ──► You: "try B and C"
  You ──► B: "who's close to CID?"
  B ──► You: "try D"
  You ──► D: "got provider records?"
  D ──► You: "yes! Alice has it"

Recursive (what original Kademlia paper describes):
  A forwards query to B, B forwards to C, C responds back through chain.
  Faster (parallel forwarding) but harder to control, risk of amplification.

libp2p uses ITERATIVE because it's simpler and the requester
can implement timeouts, parallelism (α concurrent queries), and
fallback logic without depending on intermediary nodes.
```

## GossipSub — Pub/Sub Messaging

```
Different problem: not "find who has X" but "tell everyone about X."

Nodes subscribe to TOPICS (e.g., "/eth/blocks", "/eth/attestations").
When a node publishes to a topic, the message spreads via gossip.

Two layers:

  Mesh (eager push):
    Each node maintains ~D mesh peers per topic (D=6 by default).
    Messages forwarded immediately through mesh links.
    Fast! But mesh alone can miss nodes.

  Gossip (lazy pull):
    Periodically announce "I have messages [hash1, hash2, ...]"
    to random non-mesh peers (IHAVE messages).
    If they don't have it, they request it (IWANT).
    Reliable! Catches anything the mesh missed.

  ┌───────────────────────────────────────────────────────────┐
  │ GossipSub mesh for topic "/new-blocks":                    │
  │                                                            │
  │      ┌───┐  mesh   ┌───┐  mesh   ┌───┐                   │
  │      │ A │◄────────►│ B │◄────────►│ C │                   │
  │      └─┬─┘          └─┬─┘          └───┘                   │
  │        │mesh          │mesh                                │
  │      ┌─▼─┐          ┌─▼─┐                                 │
  │      │ D │          │ E │   ···gossip links (IHAVE/IWANT)  │
  │      └───┘          └───┘                                  │
  │                                                            │
  │ A publishes block → B and D get it immediately (mesh)      │
  │ B forwards to C and E (mesh)                               │
  │ Gossip layer catches anything mesh missed                  │
  └───────────────────────────────────────────────────────────┘

  Mesh maintenance:
    GRAFT:  "add me to your mesh" (when you need more mesh peers)
    PRUNE:  "remove me from your mesh" (when you have too many)
    Target: D_low ≤ mesh_size ≤ D_high (typically 4 ≤ 6 ≤ 12)
```

### GossipSub v1.1 — Hardened Against Attacks

```
  Problem: malicious nodes could flood, eclipse, or censor messages.

  Peer scoring (v1.1):
    Each peer gets a score based on:
      - Message delivery rate (are they forwarding messages?)
      - Invalid message rate (are they sending garbage?)
      - Mesh time (how long have they stayed in mesh?)
      - IP colocation (too many peers from same IP = suspicious)

    Score < threshold → PRUNE from mesh, stop gossiping to them.

  Used by Ethereum's consensus layer to prevent eclipse attacks.
```

## How IPFS Combines Everything

```
IPFS (the main user of libp2p) uses Kademlia + Bitswap + optional GossipSub.

Upload a file:
  1. Split file into chunks (256KB each)
  2. Hash each chunk → CID (content-addressed)
  3. Build Merkle DAG (tree of CIDs linking chunks)
  4. Announce to Kademlia DHT: "I have these CIDs" (PROVIDE)

Download a file:
  1. You have the root CID (from URL, link, etc.)
  2. Query Kademlia DHT: "who has CID xyz?" (FIND_PROVIDERS)
  3. Get back list of peers
  4. Connect to peer via Bitswap: "send me block CID xyz"
  5. Recursively fetch child blocks from Merkle DAG

  ┌─────────────────────────────────────────────────────────────┐
  │ IPFS stack:                                                  │
  │                                                              │
  │  Bitswap          ← block exchange ("give me CID xyz")      │
  │  Kademlia DHT     ← content routing ("who has CID xyz?")    │
  │  GossipSub        ← pub/sub (optional, for pubsub features) │
  │  libp2p           ← transport, encryption, multiplexing     │
  │  TCP/QUIC/WebRTC  ← actual network transport                │
  └─────────────────────────────────────────────────────────────┘

Bitswap (IPFS-specific, NOT part of libp2p core):
  Block exchange protocol. Tracks: what I want, what I have.
  Tit-for-tat: prioritize peers who share with you (like BitTorrent).
  Wantlist: "I need blocks [CID1, CID2, CID3]"
  Peer responds with blocks it has from your wantlist.
```

WORKAROUNDS FOR SMALL FILES:

  1. PACK FILES INTO A DIRECTORY (single DAG)
     ipfs add -r my_folder/
     Creates one Merkle DAG with a single root CID.
     By default still PROVIDEs every block to DHT (no savings).
     BUT with Reprovider.Strategy="roots": only PROVIDE the root CID.
     Children found via Bitswap from whoever has the root (no DHT needed).
     → Actual savings require "roots" strategy + Bitswap session reuse.

  2. CAR FILES (Content Addressable Archive)
     Bundle many blocks into one .car file.
     Transfer the whole archive in one shot.
     Receiver unpacks into their local blockstore.
     Used by Filecoin and IPFS pinning services.

  3. UNIXFS SHARDING
     For directories with 1000s of files, IPFS uses HAMT
     (hash array mapped trie) to shard the directory listing.
     Avoids a single huge directory node.

        THE PROBLEM HAMT SOLVES:

        Bob has the ROOT CID of a directory (QmRoot).
        Bob wants "a.txt" from that directory.
        Bob does NOT know QmFileA (the CID of a.txt).

        Without HAMT:
            Bob downloads the ENTIRE directory listing (one huge block)
            to find: "a.txt" → QmFileA
            Then fetches QmFileA.

        With HAMT:
            Bob downloads just 2-3 small tree nodes to find: "a.txt" → QmFileA
            Then fetches QmFileA.

        If Bob already knows QmFileA → skip the HAMT entirely.


  4. DON'T USE IPFS FOR THIS
     If your workload is "millions of tiny key-value pairs":
       → Use a database (Redis, DynamoDB, Cassandra)
       → Use a DHT directly without the IPFS content layer
       → Use a traditional CDN
     IPFS is designed for content-addressed BLOBS, not tiny records.

## Comparison: Which Protocol for What

```
┌──────────────┬────────────────────────────────────┬────────────────────┐
│ Protocol     │ What it does                       │ Used by            │
├──────────────┼────────────────────────────────────┼────────────────────┤
│ Kademlia DHT │ "Who has content X?" (point query) │ IPFS, BitTorrent   │
│ GossipSub    │ "Broadcast X to subscribers"       │ Ethereum, Filecoin │
│ Bitswap      │ "Send me block X" (exchange)       │ IPFS               │
│ mDNS         │ Local network peer discovery (LAN) │ libp2p (dev/local) │
│ Identify     │ Exchange peer metadata on connect   │ All libp2p apps    │
│ Relay/DCUtR  │ NAT traversal (hole punching)      │ libp2p (behind NAT)│
│ Rendezvous   │ Lightweight peer discovery          │ libp2p (alt to DHT)│
└──────────────┴────────────────────────────────────┴────────────────────┘

Decision guide:
  Need to find who has specific content?  → Kademlia DHT
  Need to broadcast to all subscribers?   → GossipSub
  Need to exchange data blocks?           → Bitswap (or custom)
  Need peer discovery on LAN?             → mDNS
  Behind NAT?                             → Relay + DCUtR hole punching
```

## Kademlia vs Gossip (from gossip.rs) — Key Differences

```
┌──────────────────────┬──────────────────────┬──────────────────────┐
│                      │ Kademlia DHT         │ Gossip (epidemic)    │
├──────────────────────┼──────────────────────┼──────────────────────┤
│ Purpose              │ Find specific data   │ Spread ALL data      │
│ Query model          │ Point lookup (key)   │ Broadcast to all     │
│ Data stored          │ Key→provider mapping │ Full value replicated│
│ Lookup cost          │ O(log N) hops        │ O(log N) rounds      │
│ Consistency          │ Eventual             │ Eventual             │
│ Fault tolerance      │ K replicas per key   │ Epidemic spread      │
│ Structure            │ Structured (XOR)     │ Unstructured (random)│
│ Best for             │ Sparse data (not all │ Dense data (everyone │
│                      │ nodes need all data) │ needs everything)    │
│ Example              │ IPFS file lookup     │ Cassandra membership │
└──────────────────────┴──────────────────────┴──────────────────────┘

Gossip (your gossip.rs): every node ends up with ALL the data.
  Good for: membership lists, cluster state, metrics.
  Bad for: large content libraries (can't store everything everywhere).

Kademlia: each node stores a SLICE of the index.
  Good for: "find needle in haystack" across millions of nodes.
  Bad for: broadcasting updates to everyone quickly.

That's why real systems use BOTH:
  Ethereum: Kademlia for peer discovery + GossipSub for block propagation.
  IPFS: Kademlia for content routing + Bitswap for block exchange.
```
