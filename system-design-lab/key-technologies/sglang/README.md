# SGLang — Fast Structured LLM Generation

## Overview

SGLang (Structured Generation Language) is a **serving framework optimized for complex LLM programs** — multi-turn conversations, tool calling, constrained decoding, and branching logic. Where vLLM optimizes single-request throughput, SGLang optimizes **multi-call LLM programs** with features like RadixAttention (prefix caching on steroids).

## The Problem SGLang Solves

```
Modern LLM apps aren't single prompts. They're PROGRAMS:

  Step 1: "Classify this input" → LLM → category
  Step 2: if category == "question": "Answer this" → LLM → answer
  Step 3: "Verify the answer" → LLM → verified_answer
  Step 4: "Format as JSON" → LLM → structured output

Each step shares a prefix with previous steps.
Naive: re-compute the KV cache for the shared prefix every call.
SGLang: cache the prefix, reuse it across calls (RadixAttention).
```

## Key Innovations

### 1. RadixAttention (Prefix Caching with Radix Tree)

```
Multiple requests share prefixes:

Request 1: "You are a helpful assistant. User: What is 2+2?"
Request 2: "You are a helpful assistant. User: What is the capital of France?"
Request 3: "You are a helpful assistant. User: What is 2+2? Now what is 3+3?"

Radix tree of KV cache:
  "You are a helpful assistant. User: "  ← shared prefix (cached once)
       │
       ├── "What is 2+2?"                ← branch 1
       │       └── " Now what is 3+3?"   ← branch 1a (extends branch 1)
       │
       └── "What is the capital..."      ← branch 2

Without RadixAttention: compute prefix KV cache 3 times
With RadixAttention:    compute prefix ONCE, reuse for all branches
Speedup: 2-5x for multi-turn / tree-structured programs
```

### 2. Constrained Decoding (Structured Output)

```python
# Force LLM to output valid JSON matching a schema
@sgl.function
def extract_info(s, text):
    s += "Extract name and age from: " + text + "\n"
    s += sgl.gen("result",
                 regex=r'\{"name": "[a-zA-Z ]+", "age": \d+\}')

# The regex constraint is compiled to a finite state machine
# At each token, only valid next tokens are allowed
# → Guaranteed valid JSON, no retry loops needed

# Also supports:
#   JSON schema constraints
#   Choice (pick from N options)
#   Grammar (any CFG)
```

### 3. Frontend Language (Python DSL)

```python
import sglang as sgl

@sgl.function
def multi_turn_qa(s, question):
    # Turn 1: initial answer
    s += sgl.system("You are a helpful assistant.")
    s += sgl.user(question)
    s += sgl.assistant(sgl.gen("answer", max_tokens=256))

    # Turn 2: self-critique (reuses Turn 1's KV cache)
    s += sgl.user("Is your answer correct? Think step by step.")
    s += sgl.assistant(sgl.gen("critique", max_tokens=256))

    # Turn 3: final answer (reuses Turn 1+2's KV cache)
    s += sgl.user("Give your final answer based on the critique.")
    s += sgl.assistant(sgl.gen("final", max_tokens=256))

# All 3 turns share incrementally extending KV cache
# RadixAttention ensures no redundant prefix computation
```

### 4. Parallelism Primitives

```python
@sgl.function
def parallel_eval(s, question):
    s += sgl.system("You are an expert.")
    s += sgl.user(question)

    # Fork: generate 3 candidate answers in PARALLEL
    forks = s.fork(3)
    for f in forks:
        f += sgl.assistant(sgl.gen("candidate", temperature=0.8))

    # Select: pick the best one
    s += sgl.user("Which of these is best? " +
                  str([f["candidate"] for f in forks]))
    s += sgl.assistant(sgl.gen("best", max_tokens=100))
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                   SGLang Runtime                         │
│                                                          │
│  ┌────────────────┐   ┌──────────────────────────────┐ │
│  │  Frontend       │   │  Radix Cache                  │ │
│  │  (Python DSL)   │   │  ┌─────────────────────────┐ │ │
│  │  Programs       │   │  │ Radix tree of KV caches │ │ │
│  │  → IR            │   │  │ Auto eviction (LRU)     │ │ │
│  │  → Schedule      │   │  │ Copy-on-write branches  │ │ │
│  └────────┬────────┘   │  └─────────────────────────┘ │ │
│           │             └──────────────┬───────────────┘ │
│           │                            │                  │
│  ┌────────▼────────────────────────────▼──────────────┐ │
│  │  Scheduler (extends vLLM-style continuous batching) │ │
│  │  + Prefix-aware scheduling                          │ │
│  │  + Constrained decoding FSM                         │ │
│  └────────────────────────────────────────────────────┘ │
│                                                          │
│  ┌────────────────────────────────────────────────────┐ │
│  │  Model Execution (same as vLLM: TP, CUDA graphs)   │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

## SGLang vs vLLM

| Feature | SGLang | vLLM |
|---------|--------|------|
| **Single request** | Fast | Fast |
| **Multi-turn programs** | 2-5x faster (RadixAttention) | No prefix caching between calls |
| **Structured output** | Native (regex, JSON schema, grammar) | Limited |
| **Fork/branch** | Native parallelism | Not supported |
| **Constrained decoding** | Compiled FSM (fast) | Basic |
| **Prefix caching** | Radix tree (automatic) | Hash-based (manual) |
| **Best for** | LLM agents, multi-step, structured | Single-turn batch serving |

## Performance

```
Benchmark: multi-turn chatbot (3 turns per conversation)

                    Throughput (requests/sec)
vLLM:               100
SGLang:              350  (3.5x faster — prefix reuse)

Benchmark: JSON extraction (constrained output)
vLLM:               200 (with retry on invalid JSON)
SGLang:              500 (guaranteed valid, no retries)
```

## Interview Sound Bite

> "For the LLM agent that does multi-step reasoning, I'd use SGLang over vLLM. Each agent step shares a prefix with previous steps — SGLang's RadixAttention caches the KV state in a radix tree so step 3 doesn't recompute step 1 and 2's prefix. For structured output like JSON tool calls, SGLang's constrained decoding compiles the schema into a finite state machine, guaranteeing valid output without retries. This gives us 3-5x throughput improvement over stateless serving."
