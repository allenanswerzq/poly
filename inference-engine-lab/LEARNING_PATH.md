# Modern Inference Engine Learning Path 🦀🚀

Build high-performance LLM inference engines in **Rust** - from scratch.

---

## Why Rust for Inference?

- **Performance**: Zero-cost abstractions, no GC pauses
- **Memory safety**: No segfaults in production
- **Concurrency**: Fearless parallelism with async/threads
- **Growing ecosystem**: candle, burn, mistral.rs, llama.cpp bindings

### Rust Inference Projects to Study
| Project | What it does |
|---------|--------------|
| [candle](https://github.com/huggingface/candle) | HuggingFace's Rust ML framework |
| [burn](https://github.com/tracel-ai/burn) | Deep learning framework |
| [mistral.rs](https://github.com/EricLBuehler/mistral.rs) | Fast LLM inference |
| [llm](https://github.com/rustformers/llm) | Run LLMs locally |
| [ratchet](https://github.com/huggingface/ratchet) | WebGPU ML runtime |

---

## 🎯 Core Concepts to Master

### 1. GPU Programming in Rust
| Topic | Rust Crates |
|-------|-------------|
| CUDA bindings | `cudarc`, `cuda-sys`, `rustacuda` |
| GPU tensors | `candle-core`, `burn-tensor` |
| Custom kernels | `cudarc` + raw CUDA, or `rust-gpu` |
| Profiling | `nsys`, `ncu` (same tools) |

### 2. Transformer Architecture
| Component | Optimizations |
|-----------|---------------|
| Attention | FlashAttention, PagedAttention |
| KV Cache | Memory management, paging |
| Linear layers | Tensor parallelism, quantization |
| Sampling | Top-k, top-p, speculative decoding |

### 3. Systems Engineering
| Topic | Rust Advantage |
|-------|----------------|
| Batching | Zero-copy with slices |
| Scheduling | async with tokio |
| Memory management | Custom allocators, no GC |
| Distributed | Fast serialization (rkyv, bincode) |

---

## 📚 Phase 1: Rust + GPU Foundations (Weeks 1-4)

### Prerequisites
- [ ] Rust fundamentals (ownership, lifetimes, traits)
- [ ] Async Rust (tokio)
- [ ] Basic linear algebra concepts

### Tasks

#### 1.1 Learn Rust GPU Programming
```bash
# Key crates to learn
cargo add cudarc        # CUDA bindings
cargo add half          # f16 support
cargo add rayon         # CPU parallelism
cargo add ndarray       # N-dimensional arrays
```

**Study:**
- [ ] `cudarc` examples - launching kernels, memory transfers
- [ ] CUDA memory model (host vs device)
- [ ] How candle wraps CUDA operations

#### 1.2 Implement Tensors from Scratch
```
rust-tensors/
├── src/
│   ├── lib.rs
│   ├── tensor.rs         # Tensor struct (shape, strides, data)
│   ├── ops/
│   │   ├── mod.rs
│   │   ├── matmul.rs     # Matrix multiplication
│   │   ├── softmax.rs    # Numerically stable softmax
│   │   ├── layernorm.rs  # Layer normalization
│   │   └── broadcast.rs  # Broadcasting rules
│   ├── device.rs         # CPU vs CUDA backend
│   └── dtype.rs          # f32, f16, bf16
├── Cargo.toml
└── benches/
    └── matmul_bench.rs
```

**Checkpoints:**
- [ ] Tensor creation with shape/strides
- [ ] Matmul works (compare with ndarray)
- [ ] GPU matmul with cudarc/cublas
- [ ] Benchmark CPU vs GPU

#### 1.3 Implement Transformer from Scratch
```
rust-transformer/
├── src/
│   ├── lib.rs
│   ├── model.rs          # GPT-2 style decoder
│   ├── attention.rs      # Multi-head attention
│   ├── mlp.rs            # Feed-forward network
│   ├── embedding.rs      # Token + position embeddings
│   ├── kv_cache.rs       # KV cache struct
│   ├── generate.rs       # Autoregressive loop
│   └── weights.rs        # Load safetensors
├── Cargo.toml
└── benches/
```

**Checkpoints:**
- [ ] Load weights from safetensors
- [ ] Forward pass produces logits
- [ ] KV cache avoids recomputation
- [ ] Generate text autoregressively
- [ ] Match candle's output for same weights

#### 1.4 Profile & Understand Bottlenecks
```bash
# NVIDIA profiling (same as Python)
nsys profile ./target/release/generate
ncu --set full ./target/release/generate

# Rust-specific profiling
cargo install flamegraph
cargo flamegraph --bin generate
```

**Tasks:**
- [ ] Identify memory-bound vs compute-bound ops
- [ ] Profile memory allocations (is there churn?)
- [ ] Find attention bottleneck
- [ ] Measure tokens/sec baseline

---

## 🔥 Phase 2: FlashAttention in Rust (Weeks 5-6)

### Why FlashAttention?
- Standard attention: O(N²) memory
- FlashAttention: O(N) memory, fused kernel
- 2-4x speedup, enables longer sequences

### Tasks

#### 2.1 Understand the Algorithm
- [ ] Read FlashAttention paper (Dao et al.)
- [ ] Understand tiling strategy
- [ ] Understand online softmax trick
- [ ] Study memory access patterns

#### 2.2 Implement FlashAttention
```
flash-attention-rs/
├── src/
│   ├── lib.rs
│   ├── naive.rs          # O(N²) baseline in Rust
│   ├── flash.rs          # Tiled implementation
│   ├── cuda/
│   │   ├── mod.rs
│   │   ├── kernel.cu     # CUDA kernel (PTX)
│   │   └── wrapper.rs    # cudarc bindings
│   └── bench.rs
├── Cargo.toml
└── build.rs              # Compile CUDA kernel
```

**Options for CUDA in Rust:**
1. **cudarc + PTX**: Write CUDA, load as PTX
2. **rust-gpu**: Write GPU code in Rust (experimental)
3. **Bind to existing**: Link flash-attn C++ library

**Checkpoints:**
- [ ] Naive attention works in pure Rust
- [ ] CUDA kernel compiles and runs
- [ ] Output matches naive (1e-3 tolerance)
- [ ] 2x+ speedup over naive
- [ ] Memory usage is O(N) not O(N²)

#### 2.3 Study Existing Rust Implementations
```bash
# Candle's attention implementations
git clone https://github.com/huggingface/candle
# Study: candle-nn/src/ops.rs
# Study: candle-flash-attn/

# mistral.rs attention
git clone https://github.com/EricLBuehler/mistral.rs
# Study: mistralrs-core/src/attention/
```

---

## ⚡ Phase 3: PagedAttention & Continuous Batching (Weeks 7-9)

### Why PagedAttention?
- KV cache wastes memory (fragmentation)
- PagedAttention: virtual memory for KV cache
- Enables 2-4x more concurrent requests

### Tasks

#### 3.1 Study vLLM Architecture (for concepts)
```
Key concepts to port to Rust:
├── Block manager      # Allocates KV cache blocks
├── Scheduler          # Manages request queue
├── Worker             # Runs model on GPU
├── Sampler            # Token sampling
└── PagedAttention     # Custom CUDA kernel
```

#### 3.2 Implement Mini PagedAttention in Rust
```
paged-attention-rs/
├── src/
│   ├── lib.rs
│   ├── block_table.rs      # Logical -> physical block mapping
│   ├── block_manager.rs    # Allocate/free blocks (slab allocator)
│   ├── kv_cache.rs         # GPU memory pool for KV
│   ├── paged_attn.rs       # Attention with block indirection
│   ├── scheduler.rs        # Request queue + scheduling
│   └── engine.rs           # Main inference loop
├── Cargo.toml
└── tests/
    └── integration.rs
```

**Rust-specific patterns:**
- Use `Arc<Mutex<>>` or channels for scheduler
- Custom allocator with `std::alloc` for block pool
- `tokio` for async request handling

**Checkpoints:**
- [ ] Block allocation/deallocation works
- [ ] Attention reads from non-contiguous blocks
- [ ] Multiple requests share GPU memory
- [ ] Preemption/swap to CPU works

#### 3.3 Continuous Batching
```
continuous-batching-rs/
├── src/
│   ├── lib.rs
│   ├── request.rs          # Request struct with state
│   ├── batch.rs            # Dynamic batch builder
│   ├── scheduler.rs        # Iteration-level scheduling
│   └── server.rs           # HTTP API with axum/actix
├── Cargo.toml
```

**Tasks:**
- [ ] Add new requests mid-batch
- [ ] Remove completed requests
- [ ] Handle variable-length sequences
- [ ] Async HTTP server with streaming
- [ ] Measure throughput improvement

---

## 🌳 Phase 4: RadixAttention & Prefix Caching (Weeks 10-11)

### Why RadixAttention?
- Prefix caching across requests
- Radix tree for efficient prefix matching
- Great for chat/multi-turn, shared system prompts

### Tasks

#### 4.1 Study SGLang Concepts
```
Key components to implement in Rust:
├── RadixCache         # Prefix tree for KV cache
├── Constraint decoding # Regex/JSON/grammar
└── Structured output   # Type-safe generation
```

#### 4.2 Implement Radix Tree Cache in Rust
```
radix-cache-rs/
├── src/
│   ├── lib.rs
│   ├── radix_tree.rs     # Radix tree data structure
│   ├── node.rs           # Tree node with KV pointers
│   ├── cache.rs          # LRU eviction, ref counting
│   ├── prefix_match.rs   # Find longest common prefix
│   └── kv_pool.rs        # GPU memory pool integration
├── Cargo.toml
└── tests/
```

**Rust advantages:**
- Efficient tree with `Box<Node>` or arena allocation
- Reference counting with `Arc` for shared KV blocks
- LRU with `lru` crate or custom intrusive list

**Checkpoints:**
- [ ] Insert/lookup/delete work correctly
- [ ] Prefix matching finds longest common prefix
- [ ] Reference counting prevents premature free
- [ ] LRU eviction frees memory properly
- [ ] Multi-turn chat reuses KV cache

#### 4.3 Constrained Decoding (Bonus)
```
constrained-decoding-rs/
├── src/
│   ├── lib.rs
│   ├── grammar.rs        # CFG parser
│   ├── regex.rs          # Regex to token mask
│   ├── json_schema.rs    # JSON schema constraints
│   └── sampler.rs        # Masked sampling
├── Cargo.toml
```

**Crates to use:**
- `regex-automata` - DFA for fast matching
- `serde_json` - JSON schema validation

---

## 🔢 Phase 5: Quantization (Week 12)

### Why Quantize?
- FP16 → INT8: 2x memory reduction
- FP16 → INT4: 4x memory reduction
- Enables larger models on same GPU

### Tasks

#### 5.1 Implement Basic Quantization in Rust
```
quantization-rs/
├── src/
│   ├── lib.rs
│   ├── quant_linear.rs   # Quantized linear layer
│   ├── int8.rs           # W8A8 with scaling
│   ├── int4.rs           # W4A16 (weights only)
│   ├── ggml_format.rs    # Load GGUF/GGML files
│   ├── calibration.rs    # Compute scaling factors
│   └── kernels/
│       └── int4_matmul.cu # CUDA kernel for int4
├── Cargo.toml
└── benches/
```

**Rust crates:**
- `half` - f16/bf16 support
- `bytemuck` - Safe transmute for packed ints
- `memmap2` - Memory-mapped weight loading

**Checkpoints:**
- [ ] INT8 matmul with scaling
- [ ] Load GGUF quantized models
- [ ] Measure perplexity degradation
- [ ] Compare speed vs FP16

#### 5.2 Study Existing Rust Quantization
- [ ] candle-quantized (llama.cpp integration)
- [ ] llm crate (GGML support)
- [ ] mistral.rs quantization

---

## 🚄 Phase 6: Speculative Decoding (Week 13)

### Why Speculative Decoding?
- Small model drafts tokens
- Large model verifies in parallel
- 2-3x speedup for greedy/low-temp

### Tasks

```
speculative-decoding-rs/
├── src/
│   ├── lib.rs
│   ├── draft.rs          # Small/quantized draft model
│   ├── target.rs         # Large target model
│   ├── speculator.rs     # Draft + verify loop
│   ├── tree.rs           # Tree-based speculation (Medusa)
│   └── sampler.rs        # Rejection sampling
├── Cargo.toml
└── benches/
```

**Implementation:**
```rust
// Speculative decoding loop
fn speculate(&mut self, prompt: &[u32]) -> Vec<u32> {
    let mut tokens = prompt.to_vec();
    loop {
        // Draft K tokens with small model
        let drafts = self.draft_model.generate_k(&tokens, K);

        // Verify with large model (parallel forward)
        let (accepted, next) = self.target_model.verify(&tokens, &drafts);

        tokens.extend(&accepted);
        if next == EOS { break; }
        tokens.push(next);
    }
    tokens
}
```

**Checkpoints:**
- [ ] Basic speculation with rejection sampling
- [ ] Measure acceptance rate
- [ ] Tree-based speculation (Medusa-style)
- [ ] 1.5x+ speedup over vanilla

---

## 🌐 Phase 7: Distributed Inference (Weeks 14-15)

### Parallelism Strategies
| Strategy | What it splits |
|----------|----------------|
| Tensor Parallel | Splits layers across GPUs |
| Pipeline Parallel | Splits model into stages |
| Sequence Parallel | Splits sequence dimension |

### Tasks

```
distributed-rs/
├── src/
│   ├── lib.rs
│   ├── tensor_parallel.rs    # Column/row parallel linear
│   ├── pipeline.rs           # Micro-batching stages
│   ├── comm/
│   │   ├── mod.rs
│   │   ├── nccl.rs           # NCCL bindings
│   │   └── all_reduce.rs     # AllReduce, AllGather
│   ├── worker.rs             # GPU worker process
│   └── coordinator.rs        # Multi-GPU orchestration
├── Cargo.toml
```

**Rust crates:**
- `nccl-rs` or `cudarc` NCCL - GPU communication
- `mpi` crate - For multi-node
- `tokio` - Async coordination

**Checkpoints:**
- [ ] AllReduce works between 2 GPUs
- [ ] Tensor parallel attention works
- [ ] Linear scaling (2 GPUs ≈ 2x throughput)
- [ ] Run 70B model on 4x GPU

---

## 📖 Essential Resources

### Papers (Must Read)
| Paper | Key Contribution |
|-------|------------------|
| FlashAttention (Dao 2022) | Fused attention, IO-aware |
| FlashAttention-2 (Dao 2023) | Better parallelism |
| vLLM (Kwon 2023) | PagedAttention, continuous batching |
| SGLang (Zheng 2024) | RadixAttention, frontend DSL |
| Orca (Yu 2022) | Iteration-level scheduling |
| Speculative Decoding (Leviathan 2023) | Draft-verify paradigm |

### Rust Codebases to Study
| Repo | Focus |
|------|-------|
| [candle](https://github.com/huggingface/candle) | HuggingFace's ML framework |
| [burn](https://github.com/tracel-ai/burn) | Deep learning framework |
| [mistral.rs](https://github.com/EricLBuehler/mistral.rs) | Fast LLM inference |
| [llm](https://github.com/rustformers/llm) | GGML-based inference |
| [ratchet](https://github.com/huggingface/ratchet) | WebGPU inference |
| [dfdx](https://github.com/coreylowman/dfdx) | Tensor library |

### Also Study (Python, but concepts transfer)
| Repo | Focus |
|------|-------|
| [vLLM](https://github.com/vllm-project/vllm) | PagedAttention reference |
| [SGLang](https://github.com/sgl-project/sglang) | RadixAttention reference |
| [llama.cpp](https://github.com/ggerganov/llama.cpp) | C/C++ inference |

### Rust GPU Programming Resources
- [cudarc docs](https://docs.rs/cudarc) - CUDA bindings
- [Rust GPU](https://github.com/EmbarkStudios/rust-gpu) - GPU shaders in Rust
- [Are We Learning Yet?](https://www.arewelearningyet.com/) - Rust ML ecosystem
- [GPU MODE lectures](https://github.com/gpu-mode/lectures) - CUDA concepts

---

## 🗓️ Timeline

```
Week 1-4:   Rust + GPU foundations, transformer from scratch
    ↓
Week 5-6:   FlashAttention in Rust/CUDA
    ↓
Week 7-9:   PagedAttention, continuous batching
    ↓
Week 10-11: RadixAttention, prefix caching
    ↓
Week 12:    Quantization (INT8/INT4, GGUF)
    ↓
Week 13:    Speculative decoding
    ↓
Week 14-15: Distributed inference
    ↓
Ongoing:    Contribute to candle/mistral.rs, build your own
```

---

## 🎯 Milestone Projects

| Level | Project | Proof |
|-------|---------|-------|
| **Beginner** | GPT-2 inference in Rust | Match candle output |
| **Intermediate** | FlashAttention kernel | 2x speedup, correct output |
| **Advanced** | Mini vLLM in Rust | Continuous batching works |
| **Expert** | Contribute to mistral.rs/candle | Merged PR |

---

## 💡 Pro Tips

1. **Start with candle** - Don't build tensors from scratch initially. Study candle, then rebuild parts.

2. **Profile everything** - Use `nsys`, `ncu`, `cargo flamegraph`

3. **Read the papers** - Implementation details matter (tiling, memory layout)

4. **FFI is your friend** - Link existing CUDA kernels when needed

5. **Memory layout matters** - Row-major vs col-major affects performance

6. **Join communities**:
   - HuggingFace Discord (candle)
   - Rust ML community
   - GPU MODE Discord

---

## 🛠️ Environment Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install CUDA toolkit (needed for cudarc)
# Follow NVIDIA instructions for your distro

# Create project
cargo new inference-lab
cd inference-lab

# Add dependencies
cargo add candle-core candle-nn --features cuda
cargo add cudarc --features cuda-12050
cargo add half
cargo add tokio --features full
cargo add rayon
cargo add anyhow thiserror
cargo add safetensors
cargo add serde serde_json
cargo add tracing tracing-subscriber

# For benchmarking
cargo add criterion --dev

# Clone repos to study
git clone https://github.com/huggingface/candle
git clone https://github.com/EricLBuehler/mistral.rs
git clone https://github.com/rustformers/llm
```

### Cargo.toml Example
```toml
[package]
name = "inference-lab"
version = "0.1.0"
edition = "2021"

[dependencies]
candle-core = { version = "0.8", features = ["cuda"] }
candle-nn = { version = "0.8", features = ["cuda"] }
candle-transformers = { version = "0.8", features = ["cuda"] }
cudarc = { version = "0.12", features = ["cuda-12050"] }
half = "2.3"
tokio = { version = "1", features = ["full"] }
rayon = "1.8"
safetensors = "0.4"
anyhow = "1.0"
thiserror = "1.0"
serde = { version = "1.0", features = ["derive"] }
tracing = "0.1"

[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "attention"
harness = false

[profile.release]
lto = true
codegen-units = 1
```

---

Happy building! 🦀🚀
