# Kubernetes (K8s)

## Docker, Containers, and Kubernetes — How They Relate

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    The Container Ecosystem                               │
│                                                                          │
│  CONTAINERS (the concept):                                               │
│    A way to package and run applications in isolated environments.       │
│    Just a Linux process with namespaces (isolation) + cgroups (limits).  │
│    NOT a VM — shares the host kernel.                                    │
│                                                                          │
│  DOCKER (the tool):                                                      │
│    Makes containers EASY to use. Before Docker (2013), containers        │
│    existed (LXC) but were painful to set up.                             │
│    Docker gives you:                                                     │
│      • Dockerfile — recipe to build an image                            │
│      • docker build — package your app + dependencies into an image     │
│      • docker push/pull — share images via registries (Docker Hub)      │
│      • docker run — start a container from an image                     │
│                                                                          │
│  CONTAINER IMAGE (OCI image):                                            │
│    A filesystem snapshot: your binary + libraries + config.              │
│    Built with docker build (or podman, buildah, kaniko).                 │
│    Stored in a registry (Docker Hub, AWS ECR, GCP GCR, GitHub GHCR).    │
│    Immutable — same image always produces the same container.            │
│                                                                          │
│  KUBERNETES (the orchestrator):                                          │
│    Docker runs ONE container on ONE machine.                             │
│    Kubernetes runs THOUSANDS of containers across HUNDREDS of machines.  │
│    K8s decides WHERE to run each container, restarts crashed ones,       │
│    scales up/down, handles networking between them.                      │
│                                                                          │
│  Think of it as:                                                         │
│    Docker    = "how to run a container"     (single machine)             │
│    Kubernetes = "how to manage containers"  (fleet of machines)          │
└─────────────────────────────────────────────────────────────────────────┘

The relationship:

  ┌────────────────┐     ┌────────────────┐     ┌────────────────────────┐
  │   Dockerfile   │────►│  Docker Image   │────►│  Container Registry    │
  │   (recipe)     │build│  (my-app:v2)    │push │  (Docker Hub / ECR)    │
  └────────────────┘     └────────────────┘     └───────────┬────────────┘
                                                             │ pull
                                                             ▼
                                                  ┌────────────────────┐
                                                  │    Kubernetes       │
                                                  │  "run 5 copies of  │
                                                  │   my-app:v2 across │
                                                  │   these 20 nodes"  │
                                                  └────────────────────┘

  You BUILD images with Docker.
  You STORE images in a registry.
  You RUN and MANAGE containers with Kubernetes.
```

### Docker vs Kubernetes — Different Jobs

```
┌──────────────────────┬────────────────────────────────────────────────────┐
│                      │ Docker alone           │ Kubernetes                │
├──────────────────────┼────────────────────────┼───────────────────────────┤
│ Run a container      │ docker run my-app      │ kubectl apply -f pod.yaml │
│ On how many machines │ 1                      │ 1 to 10,000+              │
│ Container crashes    │ stays dead (or --restart)│ K8s restarts it auto     │
│ Need 5 copies        │ run docker 5 times     │ replicas: 5 (declarative) │
│ Load balancing       │ you figure it out       │ built-in (Service)        │
│ Rolling update       │ manual (stop old, start)│ automatic, zero-downtime  │
│ Machine dies         │ containers gone         │ K8s reschedules elsewhere │
│ Service discovery    │ hardcode IPs or links   │ DNS-based (svc-name:port) │
│ Resource limits      │ docker run --memory=4g  │ declarative per pod       │
│ Secrets management   │ env vars or files       │ built-in (K8s Secrets)    │
│ Auto-scaling         │ no                      │ HPA (based on CPU/custom) │
└──────────────────────┴────────────────────────┴───────────────────────────┘

One way to think about it:
  Docker = a chef who can cook one dish really well
  Kubernetes = a restaurant manager who coordinates 50 chefs,
               decides which chef cooks what, replaces sick chefs,
               handles the dinner rush by hiring more
```

### The Container Runtime (Docker is no longer required by K8s!)

```
Fun fact: Kubernetes DROPPED Docker support in v1.24 (2022).

  Before:  K8s → dockershim → Docker → containerd → runc → container
  After:   K8s → containerd → runc → container
                 (or CRI-O → runc → container)

  Docker was a middleman. K8s talked to Docker, Docker talked to containerd,
  containerd actually ran the container. Removing Docker removed one layer.

  ┌─────────────────────────────────────────────────────────────┐
  │  Container Runtime Stack:                                    │
  │                                                              │
  │  Kubernetes (kubelet)                                        │
  │       │                                                      │
  │       ▼  CRI (Container Runtime Interface)                   │
  │  ┌──────────────┐    ┌──────────────┐                       │
  │  │  containerd   │ or │   CRI-O      │  ← high-level runtime│
  │  │ (Docker's guts│    │ (Red Hat)    │    manages images,    │
  │  │  extracted)   │    │              │    lifecycle           │
  │  └──────┬───────┘    └──────┬───────┘                       │
  │         ▼                    ▼                                │
  │  ┌──────────────────────────────────┐                       │
  │  │           runc                    │  ← low-level runtime  │
  │  │  Actually creates the container:  │    sets up namespaces, │
  │  │  clone() + unshare() + exec()     │    cgroups, exec      │
  │  └──────────────────────────────────┘                       │
  │                                                              │
  │  Your Docker images still work! The IMAGE FORMAT is standard │
  │  (OCI). Only the RUNTIME changed. docker build still works.  │
  │  You just don't need Docker installed on K8s nodes anymore.  │
  └─────────────────────────────────────────────────────────────┘

  What you still use Docker for:
    ✓ Building images (docker build / Dockerfile)
    ✓ Local development (docker run, docker compose)
    ✓ CI/CD pipelines (build + push images)

  What K8s no longer needs Docker for:
    ✗ Running containers on K8s nodes (containerd does this directly)
```

## What Problem Does K8s Solve?

Before K8s, deploying services was manual and fragile:

```
The problem — managing many services across many machines:

  2005: You have 3 servers. SSH in, install your app, start it. Easy.

  2012: You have 50 services on 200 servers.
    - Where is service X running? (nobody knows, check the wiki?)
    - Server 47 died. What was on it? (check spreadsheet...)
    - Deploy new version? (SSH into 15 servers, run commands, pray)
    - Service needs more capacity? (buy a server, wait 2 weeks, install OS, ...)
    - Service crashed at 3am? (pager goes off, human restarts it)

  The CORE problems:
  ┌──────────────────────────────────────────────────────────────────┐
  │ 1. PLACEMENT:   Which machine should this service run on?        │
  │                 (has enough CPU/RAM/GPU? not overloaded?)         │
  │                                                                  │
  │ 2. SCHEDULING:  When a new version deploys, how to do it         │
  │                 without downtime? Roll back if bad?               │
  │                                                                  │
  │ 3. SELF-HEALING:If a process crashes, who restarts it?           │
  │                 If a machine dies, who moves the workload?        │
  │                                                                  │
  │ 4. DISCOVERY:   How does service A find service B?               │
  │                 IPs change when containers restart.               │
  │                                                                  │
  │ 5. SCALING:     Traffic doubles — how to add more instances      │
  │                 automatically and route traffic to them?          │
  │                                                                  │
  │ 6. RESOURCE MGMT:How to prevent one service from eating all      │
  │                 the CPU/RAM on a shared machine?                  │
  └──────────────────────────────────────────────────────────────────┘

  Before K8s, humans solved these problems. Badly.
    - Config management tools (Puppet, Chef, Ansible) help INSTALL software
      but don't restart crashed processes or migrate dead nodes.
    - Custom scripts for deployment (fragile, every team reinvents).
    - Manual capacity planning (over-provision or risk outages).
```

## How Does K8s Solve It?

```
K8s is a DECLARATIVE RECONCILIATION LOOP.

You tell K8s WHAT you want:
  "I want 3 copies of my API server, each with 2 CPU and 4GB RAM"

K8s figures out HOW and continuously MAINTAINS that state:

  ┌─────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │   You declare desired state          K8s observes actual state   │
  │   (YAML → API server → etcd)         (kubelet reports to API)    │
  │            │                                   │                 │
  │            └──────────┐    ┌───────────────────┘                 │
  │                       ▼    ▼                                     │
  │                   ┌────────────┐                                 │
  │                   │  COMPARE   │                                 │
  │                   │ desired vs │                                 │
  │                   │   actual   │                                 │
  │                   └─────┬──────┘                                 │
  │                         │                                        │
  │                    Different?                                     │
  │                    │        │                                     │
  │                   Yes       No                                    │
  │                    │        │                                     │
  │                    ▼        ▼                                     │
  │              Take action   Sleep,                                │
  │              to converge   check again                           │
  │                            in a few seconds                      │
  │                                                                  │
  └─────────────────────────────────────────────────────────────────┘

  This is THE key insight. You don't say "start 3 pods."
  You say "there should always be 3 pods."
  K8s continuously enforces this. Pod dies? It creates a new one.
  Node dies? It reschedules pods elsewhere. No human needed.
```

How each problem maps to a K8s concept:

```
  Problem                K8s Solution                 How it works
  ─────────────────────────────────────────────────────────────────────
  PLACEMENT              Scheduler                    Scores each node:
                                                      "Node 2 has 4 free
                                                      CPUs, pod needs 2,
                                                      score = good."
                                                      Bin-packs pods onto
                                                      nodes efficiently.

  DEPLOY / ROLLBACK      Deployment +                 Rolling update: start
                         RollingUpdate                1 new pod, wait until
                                                      healthy, kill 1 old
                                                      pod. Repeat. Bad
                                                      health check → auto
                                                      rollback.

  SELF-HEALING           kubelet +                    kubelet watches pods.
                         Controller Manager           Crash → restart.
                                                      Node dies → controller
                                                      sees "desired=3,
                                                      actual=2" → creates
                                                      new pod on live node.

  DISCOVERY              Service + DNS                Each Service gets a
                                                      stable DNS name:
                                                      api.default.svc.local
                                                      Points to healthy pods.
                                                      Pod IP changes? DNS
                                                      updates automatically.

  SCALING                HPA (Horizontal              Watches CPU/memory/
                         Pod Autoscaler)              custom metrics. CPU >
                                                      70%? Add more pods.
                                                      Traffic drops? Remove
                                                      pods. 30-60s response.

  RESOURCE MGMT          Requests + Limits            requests: guaranteed
                         + cgroups                    minimum (scheduler).
                                                      limits: hard ceiling
                                                      (memory.max in cgroup).
                                                      Prevents noisy neighbors.
```

## Is K8s Good at Solving It? (Honest Assessment)

```
K8s solves real problems. But it comes with MASSIVE complexity.
Is the trade-off worth it?

  ┌─────────────────────────────────────────────────────────────────┐
  │ WHEN K8S IS WORTH IT:                                           │
  │                                                                  │
  │ ✓ 10+ services, multiple teams                                  │
  │   → Need standardized deployment, service discovery, isolation  │
  │                                                                  │
  │ ✓ Need auto-scaling and self-healing                            │
  │   → Traffic is variable, 3am crashes shouldn't page humans      │
  │                                                                  │
  │ ✓ GPU workloads (ML training/inference)                         │
  │   → K8s GPU scheduling + device plugins are hard to replicate   │
  │                                                                  │
  │ ✓ Multi-cloud or hybrid deployments                             │
  │   → K8s abstracts away the cloud. Same YAML on AWS/GCP/on-prem │
  │                                                                  │
  │ ✓ Already using managed K8s (EKS, GKE, AKS)                    │
  │   → Cloud provider handles control plane. 80% of the pain gone.│
  └─────────────────────────────────────────────────────────────────┘

  ┌─────────────────────────────────────────────────────────────────┐
  │ WHEN K8S IS NOT WORTH IT:                                       │
  │                                                                  │
  │ ✗ 1-5 services, small team                                     │
  │   → Docker Compose, or just systemd on a VM. Seriously.         │
  │   → You'll spend more time learning K8s than running your app.  │
  │                                                                  │
  │ ✗ Serverless fits your workload                                 │
  │   → Lambda/Cloud Functions: zero infra to manage.               │
  │   → K8s is overkill for request/response workloads < 1000 QPS  │
  │                                                                  │
  │ ✗ Self-hosted K8s (running your own control plane)              │
  │   → etcd ops alone is a full-time job. Don't.                   │
  │   → Use managed K8s or don't use K8s at all.                    │
  │                                                                  │
  │ ✗ Your team doesn't have K8s expertise                          │
  │   → Debugging K8s networking, RBAC, storage is a SKILL.         │
  │   → Budget 3-6 months for a team to become productive.          │
  └─────────────────────────────────────────────────────────────────┘
```

**The complexity cost:**

```
What you get:                        What it costs:
  Auto-scaling                         YAML sprawl (100s of files)
  Self-healing                         Networking complexity (CNI, iptables, DNS)
  Rolling deploys                      RBAC, security policies
  Service discovery                    Observability tax (Prometheus, Grafana, tracing)
  Resource isolation                   Debugging is HARD (pod, node, network, storage)
  GPU scheduling                       Steep learning curve (3-6 months)
  Multi-cloud portability              Control plane ops (if self-hosted)

The honest answer:
  If you're on AWS/GCP with managed K8s (EKS/GKE) and 10+ services,
  the benefits outweigh the costs.

  If you're a small team with < 5 services, use:
    1. A single VM + Docker Compose  (simplest)
    2. AWS ECS / Cloud Run           (managed containers, no K8s complexity)
    3. Lambda / Cloud Functions       (for event-driven workloads)
```

**Alternatives comparison:**

```
┌─────────────────┬──────────┬──────────────┬───────────┬────────────────┐
│                 │ K8s      │ ECS/Cloud Run│ Compose   │ Serverless     │
│                 │ (managed)│              │ (VM)      │ (Lambda)       │
├─────────────────┼──────────┼──────────────┼───────────┼────────────────┤
│ Complexity      │ High     │ Medium       │ Low       │ Lowest         │
│ Services        │ 10-1000s │ 5-100s       │ 1-10      │ 1-100s         │
│ Auto-scale      │ Yes (HPA)│ Yes          │ Manual    │ Yes (instant)  │
│ Self-healing    │ Yes      │ Yes          │ systemd   │ Yes            │
│ GPU support     │ Good     │ Limited      │ Manual    │ No             │
│ Cost at scale   │ Good     │ Good         │ Good      │ Expensive      │
│ Cost at low use │ Expensive│ Pay-per-use  │ Cheap     │ Free/cheap     │
│ Portability     │ Multi-cloud│ Vendor lock │ Any VM    │ Vendor lock    │
│ Learning curve  │ 3-6 months│ 1-2 weeks   │ 1 day    │ 1 week         │
└─────────────────┴──────────┴──────────────┴───────────┴────────────────┘

Interview tip:
  Don't just say "use K8s." Say WHY.
  "For this system with 15 microservices, variable traffic, and GPU
  inference workloads, I'd use managed K8s (EKS). The auto-scaling,
  GPU scheduling, and self-healing justify the complexity. For a
  simpler system with 3 services, I'd use ECS or Cloud Run instead."
```

## Overview

Kubernetes is the **operating system for the cloud**. If your system runs more than a few services, K8s is likely orchestrating them. You don't need to know every kubectl flag, but you must understand the architecture, scheduling, and failure modes.

## Core Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Control Plane                                │
│                                                                  │
│  ┌──────────┐  ┌──────────────┐  ┌────────────┐  ┌──────────┐ │
│  │ API      │  │ etcd         │  │ Scheduler  │  │ Controller│ │
│  │ Server   │  │ (state store)│  │ (place pods)│  │ Manager  │ │
│  └────┬─────┘  └──────────────┘  └────────────┘  └──────────┘ │
│       │                                                          │
└───────┼──────────────────────────────────────────────────────────┘
        │
        │ kubelet communicates with API server
        │
┌───────▼──────────────────────────────────────────────────────────┐
│                     Worker Nodes                                  │
│                                                                   │
│  Node 1                    Node 2                    Node 3       │
│  ┌──────────────────┐    ┌──────────────────┐    ┌─────────────┐│
│  │ kubelet          │    │ kubelet          │    │ kubelet     ││
│  │ ┌──────┐┌──────┐│    │ ┌──────┐┌──────┐│    │ ┌──────┐   ││
│  │ │Pod A ││Pod B ││    │ │Pod C ││Pod D ││    │ │Pod E │   ││
│  │ └──────┘└──────┘│    │ └──────┘└──────┘│    │ └──────┘   ││
│  │ kube-proxy       │    │ kube-proxy       │    │ kube-proxy ││
│  └──────────────────┘    └──────────────────┘    └─────────────┘│
└──────────────────────────────────────────────────────────────────┘
```

## Key Concepts

### 1. Pod — The Smallest Deployable Unit

**What a pod actually IS under the hood:**

```
A pod is NOT a VM. NOT a special runtime. NOT a new type of process.

A pod = a group of Linux processes that share:
  1. Network namespace  (same IP, same ports, localhost works between them)
  2. IPC namespace      (can use shared memory between containers)
  3. Volumes            (shared filesystem mounts)
  4. cgroup             (resource limits applied as a group)

Each container in the pod is just a regular Linux PROCESS with:
  - Its own PID namespace (container sees itself as PID 1)
  - Its own mount namespace (own root filesystem from container image)
  - Its own cgroup limits (CPU, memory per container)

Pod creation — what actually happens on the node:
  ┌─────────────────────────────────────────────────────────────────┐
  │ 1. kubelet tells container runtime (containerd) to create pod  │
  │                                                                 │
  │ 2. Runtime creates the "pause" container first:                 │
  │    - Tiny process (~700KB, does nothing, just sleeps forever)   │
  │    - Creates the shared network namespace                       │
  │    - Gets the pod IP assigned (from CNI plugin)                 │
  │    - All other containers JOIN this namespace                   │
  │                                                                 │
  │ 3. Runtime creates each app container:                          │
  │    - fork() + clone() with CLONE_NEWPID, CLONE_NEWNS           │
  │    - Joins pause container's NET namespace (shared IP)          │
  │    - Sets up cgroup limits (memory.max, cpu.max)                │
  │    - Mount overlayfs (image layers → root filesystem)           │
  │    - exec() the container's entrypoint                          │
  │                                                                 │
  │ Result: regular Linux processes with namespace + cgroup isolation│
  └─────────────────────────────────────────────────────────────────┘

  Why the "pause" container?
    If container A and B share a network namespace, and A dies,
    who owns the namespace? The pause container. It holds the
    namespace alive even if all app containers crash and restart.
    Without it, restarting container A would change the pod IP.
```

```
What "sharing network namespace" actually means:

  Pod (IP: 10.0.1.5)
  ┌──────────────────────────────────────────────┐
  │  pause container (holds the network namespace)│
  │                                                │
  │  Container A (app)        Container B (envoy)  │
  │  ┌──────────────────┐   ┌──────────────────┐  │
  │  │ Listens on :8080  │   │ Listens on :9090  │  │
  │  │                   │   │                   │  │
  │  │ curl localhost:9090 ──► works!            │  │
  │  │ (same network ns) │   │                   │  │
  │  └──────────────────┘   └──────────────────┘  │
  │                                                │
  │  Both see: eth0 = 10.0.1.5                     │
  │  Both share the same port space                │
  │  (A can't also use :9090 — port conflict)      │
  └────────────────────────────────────────────────┘

  From outside the pod: 10.0.1.5:8080 → reaches container A
                        10.0.1.5:9090 → reaches container B
```

**Pod vs bare-metal performance:**

```
A pod IS bare-metal performance, with tiny overhead.
Here's why — and where the overhead actually comes from:

  ┌───────────────────────────────────────────────────────────────────┐
  │ Component          │ Overhead vs bare metal │ Why                 │
  ├────────────────────┼────────────────────────┼─────────────────────┤
  │ CPU (compute)      │ ~0%                    │ NO virtualization.  │
  │                    │                        │ Same CPU, same      │
  │                    │                        │ instructions, same  │
  │                    │                        │ speed. cgroup just  │
  │                    │                        │ limits HOW MUCH cpu,│
  │                    │                        │ not HOW FAST.       │
  │                    │                        │                     │
  │ Memory (access)    │ ~0%                    │ Same physical RAM.  │
  │                    │                        │ No memory            │
  │                    │                        │ virtualization.     │
  │                    │                        │ Namespace is just a │
  │                    │                        │ kernel label, not   │
  │                    │                        │ an abstraction layer.│
  │                    │                        │                     │
  │ Disk I/O           │ ~0-5%                  │ overlayfs has small │
  │                    │                        │ overhead for writes │
  │                    │                        │ (copy-up). Reading  │
  │                    │                        │ is near-zero. Use   │
  │                    │                        │ hostPath or local PV│
  │                    │                        │ for zero overhead.  │
  │                    │                        │                     │
  │ NETWORK            │ ~2-10%  ←── HERE       │ THIS is where the   │
  │                    │                        │ overhead lives.     │
  │                    │                        │                     │
  │ Syscalls           │ ~0%                    │ No interception.    │
  │                    │                        │ Process calls kernel│
  │                    │                        │ directly (not a VM).│
  │                    │                        │                     │
  │ Startup            │ ~100-500ms             │ Pull image, setup   │
  │                    │                        │ namespaces. Not a   │
  │                    │                        │ throughput issue.   │
  └────────────────────┴────────────────────────┴─────────────────────┘
```

**Why network overhead exists (and how much):**

```
Bare metal:
  App → kernel TCP/IP stack → NIC → wire

Pod in K8s:
  App → kernel TCP/IP stack → veth pair → bridge/overlay → NIC → wire

  The extra hops:
  ┌─────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │  Pod network namespace          Host network namespace           │
  │  ┌──────────────┐              ┌──────────────────────────┐     │
  │  │ eth0 (veth)  │──── veth ───►│ bridge (cbr0) or         │     │
  │  │ 10.0.1.5     │    pair      │ overlay network (VXLAN)   │     │
  │  └──────────────┘              │         │                 │     │
  │                                │         ▼                 │     │
  │                                │  iptables/eBPF rules      │     │
  │                                │  (kube-proxy or Cilium)   │     │
  │                                │         │                 │     │
  │                                │         ▼                 │     │
  │                                │  eth0 (physical NIC)      │     │
  │                                └──────────────────────────────┘  │
  └─────────────────────────────────────────────────────────────────┘

  Overhead breakdown:
    veth pair:         ~1-2 µs extra latency
    iptables (kube-proxy): ~1-5 µs per connection (O(n) rules!)
    VXLAN overlay:     ~5-10% bandwidth overhead (encapsulation)
    eBPF (Cilium):     ~0.5-1 µs (much faster than iptables)

  Pod-to-pod on SAME node (veth + bridge, no overlay):
    Latency:  ~10-15 µs  (bare metal loopback: ~5-8 µs)
    Bandwidth: ~95% of bare metal

  Pod-to-pod ACROSS nodes (overlay network, VXLAN):
    Latency:  +5-20 µs over bare metal
    Bandwidth: ~85-95% of bare metal (VXLAN header overhead)

  Pod-to-pod ACROSS nodes (no overlay, native routing / Cilium):
    Latency:  +1-3 µs over bare metal
    Bandwidth: ~98% of bare metal

  FOR COMPARISON — a VM:
    Latency:  +50-100 µs (hypervisor, virtual NIC, virtual switch)
    Bandwidth: ~70-90% of bare metal

  Bottom line:
    Container networking overhead: small (2-10%)
    VM networking overhead: significant (10-30%)
    The gap matters for latency-sensitive workloads (trading, gaming, HPC)
```

**When network overhead matters (and how to eliminate it):**

```
If you need bare-metal network performance in K8s:

  1. hostNetwork: true
     Pod uses the NODE's network namespace directly. No veth, no overlay.
     Same performance as bare metal. But: pod port conflicts, less isolation.

     Used for: high-frequency trading, DPDK, network monitoring agents

     spec:
       hostNetwork: true    # pod gets node's IP and ports directly

  2. Cilium with eBPF (instead of kube-proxy + iptables)
     Bypasses iptables entirely. Service routing in eBPF.
     ~1 µs overhead instead of ~5 µs. Scales to 100K+ services.

  3. SR-IOV (Single Root I/O Virtualization)
     Hardware NIC creates virtual functions, assigned DIRECTLY to pod.
     Bypasses kernel networking entirely. True bare-metal speed.
     Used for: HPC, telecom, ultra-low-latency applications.

  4. Use node-local traffic when possible
     Pod affinity: schedule communicating pods on the same node.
     Node-local Service: externalTrafficPolicy: Local
     Avoids overlay network hops entirely.
```

**Comparison summary:**

```
  ┌──────────────────┬──────────┬──────────┬──────────┬──────────────┐
  │                  │ Bare     │ Pod      │ Pod      │ VM           │
  │                  │ Metal    │ (native) │ (overlay)│ (hypervisor) │
  ├──────────────────┼──────────┼──────────┼──────────┼──────────────┤
  │ CPU overhead     │ 0%       │ ~0%      │ ~0%      │ 1-5%         │
  │ Memory overhead  │ 0%       │ ~0%      │ ~0%      │ 5-10%        │
  │ Network latency  │ baseline │ +2-5 µs  │ +10-20µs │ +50-100 µs   │
  │ Network bandwidth│ 100%     │ ~98%     │ ~90%     │ ~80%         │
  │ Disk I/O         │ 100%     │ ~95-100% │ ~95-100% │ ~85-95%      │
  │ Startup time     │ minutes  │ <1s      │ <1s      │ 30-60s       │
  │ Isolation level  │ none     │ process  │ process  │ hardware     │
  └──────────────────┴──────────┴──────────┴──────────┴──────────────┘

  A pod is a process with resource limits. NOT a virtualized machine.
  The only real overhead is networking. Everything else is bare-metal.
```

```yaml
# NOT a container. A Pod = 1 or more containers sharing network + storage.
apiVersion: v1
kind: Pod
metadata:
  name: my-app
spec:
  containers:
  - name: app
    image: my-app:v2
    resources:
      requests:          # scheduler uses this to find a node
        cpu: "500m"      # 0.5 CPU cores
        memory: "256Mi"
      limits:            # hard ceiling — OOM killed if exceeded
        cpu: "1000m"
        memory: "512Mi"
    ports:
    - containerPort: 8080
  - name: sidecar        # second container in same pod
    image: envoy-proxy
```

**Requests vs Limits — THE most important K8s concept for interviews:**
```
Requests: guaranteed minimum (scheduler reserves this)
Limits:   hard maximum (container killed if it exceeds)

requests < limits = "burstable" (common)
requests = limits = "guaranteed" (predictable, used for databases)
no limits set     = "best effort" (evicted first under pressure)
```

### 2. Deployment — Manage Pod Replicas

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-server
spec:
  replicas: 3                    # run 3 identical pods
  strategy:
    type: RollingUpdate          # zero-downtime deploys
    rollingUpdate:
      maxSurge: 1                # 1 extra pod during update
      maxUnavailable: 0          # never go below 3 healthy
  selector:
    matchLabels:
      app: api-server
  template:
    spec:
      containers:
      - name: api
        image: api-server:v2
        readinessProbe:          # don't send traffic until ready
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 5
        livenessProbe:           # restart if unhealthy
          httpGet:
            path: /health
            port: 8080
          periodSeconds: 10
```

### 3. Service — Stable Network Endpoint

```
Pods have ephemeral IPs (they change on restart).
Service = stable DNS name + load balancing across pods.

  ┌──────────────────────────────────────────┐
  │  Service: api-server (ClusterIP)          │
  │  DNS: api-server.default.svc.cluster.local│
  │  Port: 80 → targetPort: 8080             │
  │                                           │
  │  Endpoints:                               │
  │    10.0.1.5:8080 (Pod A)                  │
  │    10.0.1.6:8080 (Pod B)                  │
  │    10.0.1.7:8080 (Pod C)                  │
  └──────────────────────────────────────────┘

Service types:
  ClusterIP:    internal only (default)
  NodePort:     expose on each node's IP:port
  LoadBalancer: provision cloud LB (AWS ALB/NLB)
  Ingress:      HTTP routing (path/host-based)
```

### 4. HPA — Horizontal Pod Autoscaler

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
spec:
  scaleTargetRef:
    kind: Deployment
    name: api-server
  minReplicas: 3
  maxReplicas: 50
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70    # scale up when CPU > 70%
```

```
Traffic spike:
  CPU > 70% → HPA adds pods → new pods scheduled → ~30-60s to scale

Scaling decision:
  desired = ceil(current_replicas × current_cpu / target_cpu)
  3 pods at 90% CPU → ceil(3 × 90/70) = ceil(3.86) = 4 pods
```

### 5. StatefulSet — For Databases & Stateful Apps

```
Deployment: pods are interchangeable (web servers)
StatefulSet: pods have stable identity (databases, Kafka)

  Pod names: redis-0, redis-1, redis-2 (ordered, stable)
  Storage: each pod gets its own PersistentVolume
  Startup: redis-0 starts before redis-1 (ordered)
  Deletion: redis-2 deleted before redis-1 (reverse order)

Use for: PostgreSQL, Redis Cluster, Kafka, Elasticsearch
```

### 6. ConfigMap + Secrets

```yaml
# ConfigMap: non-sensitive config
apiVersion: v1
kind: ConfigMap
data:
  DATABASE_URL: "postgres://db:5432/myapp"
  LOG_LEVEL: "info"

# Secret: sensitive data (base64 encoded, NOT encrypted by default!)
apiVersion: v1
kind: Secret
data:
  DB_PASSWORD: cGFzc3dvcmQxMjM=    # base64("password123")
  API_KEY: c2stYWJjMTIz              # base64("sk-abc123")

# In production: use external secrets (Vault, AWS Secrets Manager)
```

### 7. Namespace — Logical Isolation

```
Namespaces = folders for K8s resources

  default:     your apps
  kube-system: K8s internal components
  monitoring:  Prometheus, Grafana
  staging:     staging environment

Resource quotas per namespace:
  staging: max 10 CPU, 20GB RAM (prevent staging from eating prod resources)
```

## GPU Scheduling (Critical for ML)

```yaml
# Request GPU for ML workload
spec:
  containers:
  - name: training
    image: pytorch-train:latest
    resources:
      limits:
        nvidia.com/gpu: 4    # request 4 GPUs

# Scheduling:
#   K8s finds a node with 4 free GPUs
#   NVIDIA device plugin manages GPU allocation
#   GPU is NOT oversubscribed (unlike CPU)
#   1 GPU = 1 container (no sharing by default)

# Multi-Instance GPU (MIG) on A100:
#   Split 1 A100 into up to 7 slices
#   Each slice = isolated GPU instance
#   Good for inference (small models don't need full GPU)
```

## Common Failure Modes

| Failure | What Happens | K8s Response |
|---------|-------------|-------------|
| Pod crashes | Container exits with error | kubelet restarts it (restartPolicy) |
| Node dies | All pods on node gone | Scheduler reschedules pods to other nodes |
| OOM | Container exceeds memory limit | K8s kills the container (OOMKilled) |
| Liveness fail | Health check fails | K8s restarts the container |
| Readiness fail | Health check fails | K8s stops sending traffic (but doesn't restart) |
| etcd down | Cluster state unavailable | Cluster is read-only, no new scheduling |

## Interview Talking Points

> "I'd deploy the service as a Deployment with 3 replicas and an HPA scaling on CPU utilization. Each pod gets a readiness probe on /health so the Service only routes to healthy pods. For zero-downtime deploys, we use RollingUpdate with maxUnavailable=0."

> "For GPU workloads, K8s treats GPUs as a non-oversubscribable resource. The training pods request `nvidia.com/gpu: 4` and the scheduler bin-packs them onto GPU nodes. For inference with smaller models, we'd use MIG to split A100s into slices."

> "The database runs as a StatefulSet with PersistentVolumeClaims so each replica has stable storage that survives pod restarts. The primary is always pod-0, replicas are pod-1 and pod-2."
