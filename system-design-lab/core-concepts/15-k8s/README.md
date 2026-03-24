# Kubernetes (K8s)

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
