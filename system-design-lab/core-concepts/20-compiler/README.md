# Compilers — From Source Code to Machine Code

## Why This Matters

Every program you write goes through a compiler (or interpreter). Understanding how compilers work explains WHY certain optimizations exist, what `torch.compile` and JAX's XLA actually do, why LLVM dominates the world, and how MLIR is reshaping ML infrastructure. If you're building or optimizing ML systems, compilers are no longer optional knowledge.

## 1. What a Compiler Actually Doesz

```
Source code → Compiler → Machine code (or IR)

That's the 10,000-foot view. The real pipeline:

  ┌─────────────────────────────────────────────────────────────────────────┐
  │                        Compiler Pipeline                                 │
  │                                                                          │
  │  Source Code (C, Rust, Python, CUDA, ...)                                │
  │       │                                                                  │
  │       ▼                                                                  │
  │  ┌──────────────┐                                                       │
  │  │ 1. LEXER      │  Break text into tokens                               │
  │  │  (Tokenizer)  │  "int x = 5 + 3;"  →  [int] [x] [=] [5] [+] [3] [;]│
  │  └──────┬───────┘                                                       │
  │         ▼                                                                │
  │  ┌──────────────┐                                                       │
  │  │ 2. PARSER     │  Build tree structure (AST) from tokens               │
  │  │              │  Enforces grammar rules (syntax errors caught here)   │
  │  └──────┬───────┘                                                       │
  │         ▼                                                                │
  │  ┌──────────────┐       ┌─────────────────────────────┐                 │
  │  │ 3. SEMANTIC   │       │  AST for "int x = 5 + 3;":  │                 │
  │  │   ANALYSIS    │       │                              │                 │
  │  │              │       │     VarDecl (int x)           │                 │
  │  │ Type checking│       │         │                     │                 │
  │  │ Name resolving       │       BinaryOp (+)            │                 │
  │  │ Scope checking       │       ┌──┴──┐                │                 │
  │  └──────┬───────┘       │    Lit(5)  Lit(3)            │                 │
  │         │               └─────────────────────────────┘                 │
  │         ▼                                                                │
  │  ┌──────────────┐                                                       │
  │  │ 4. IR GEN     │  Lower AST to intermediate representation            │
  │  │  (Codegen)    │  (LLVM IR, three-address code, SSA form, etc.)       │
  │  └──────┬───────┘                                                       │
  │         ▼                                                                │
  │  ┌──────────────┐                                                       │
  │  │ 5. OPTIMIZER  │  Transform IR → better IR (the bulk of a compiler)   │
  │  │  (Middle-end) │  Constant folding, inlining, dead code elimination,  │
  │  │              │  loop unrolling, vectorization, register allocation   │
  │  └──────┬───────┘                                                       │
  │         ▼                                                                │
  │  ┌──────────────┐                                                       │
  │  │ 6. CODE GEN   │  IR → target machine code (x86, ARM, RISC-V, PTX)   │
  │  │  (Back-end)   │  Instruction selection, register allocation,         │
  │  │              │  scheduling, final binary encoding                   │
  │  └──────────────┘                                                       │
  │         │                                                                │
  │         ▼                                                                │
  │  Machine Code (.o file) → Linker → Executable                           │
  └─────────────────────────────────────────────────────────────────────────┘
```

### Concrete Example — Following "int x = 5 + 3;" Through Each Stage

```
Stage 1 — LEXER (tokenization):
  Input:  "int x = 5 + 3;"
  Output: [KW_INT, IDENT("x"), EQ, INT_LIT(5), PLUS, INT_LIT(3), SEMICOLON]

  The lexer just splits text into meaningful chunks. It doesn't understand
  structure — "int + = 5 x 3;" would tokenize fine but fail at parsing.

Stage 2 — PARSER (AST construction):
  Input:  token stream
  Output: Abstract Syntax Tree

     VarDecl
     ├── type: int
     ├── name: x
     └── init: BinaryExpr
               ├── op: +
               ├── left: IntLiteral(5)
               └── right: IntLiteral(3)

  Parser uses a grammar (usually context-free):
    declaration → type IDENT '=' expression ';'
    expression  → expression '+' expression | INT_LIT | ...

  Parsing algorithms:
    Recursive descent: handwritten, easy to debug (GCC, Clang, Rust, Go)
    LR / LALR:         table-driven, more powerful (yacc, bison)
    PEG / Packrat:     unlimited lookahead (tree-sitter, pest in Rust)
    Pratt parsing:     great for expressions with operator precedence

Stage 3 — SEMANTIC ANALYSIS:
  - Type check: 5 (int) + 3 (int) = int ✓
  - x declared as int, assigned int ✓
  - x not previously declared in this scope ✓

  This is where you catch:
    float f = "hello";     // type mismatch
    return x + y;          // y not declared
    foo.bar();             // foo has no method bar

Stage 4 — IR GENERATION (lowering to LLVM IR):
  ; LLVM IR for "int x = 5 + 3;"
  %x = add i32 5, 3       ; x = 5 + 3 → will be optimized to 8

  In SSA (Static Single Assignment) form:
    Every variable is assigned EXACTLY ONCE.
    Instead of:  x = 5; x = x + 3;
    SSA form:    x1 = 5; x2 = add x1, 3;
    This makes optimization much easier (no ambiguity about values).

Stage 5 — OPTIMIZATION:
  %x = add i32 5, 3   →   %x = i32 8    (constant folding)
  The compiler computed 5+3 at compile time. No runtime cost.

Stage 6 — CODE GENERATION:
  ; x86-64 assembly
  mov eax, 8           ; x = 8 (already folded)

  If it wasn't folded:
  mov eax, 5
  add eax, 3           ; x = 5 + 3
```

## 2. GCC vs Clang/LLVM — The Two Compiler Empires

```
┌──────────────────────────────────────────────────────────────────┐
│              The Two Major Compiler Ecosystems                    │
│                                                                   │
│  GCC (GNU Compiler Collection)        LLVM + Clang               │
│  ┌───────────────────────┐           ┌───────────────────────┐  │
│  │ Frontend: C, C++,     │           │ Frontend: Clang (C/C++)│  │
│  │ Fortran, Ada, Go      │           │ Rust (rustc), Swift,   │  │
│  │                       │           │ Julia, Zig, Mojo       │  │
│  ├───────────────────────┤           ├───────────────────────┤  │
│  │ Middle: GIMPLE + RTL  │           │ Middle: LLVM IR (SSA) │  │
│  │ (GCC-specific IRs)    │           │ (universal, reusable)  │  │
│  ├───────────────────────┤           ├───────────────────────┤  │
│  │ Backend: GCC codegen  │           │ Backend: LLVM codegen │  │
│  │ x86, ARM, RISC-V,    │           │ x86, ARM, RISC-V,    │  │
│  │ MIPS, PowerPC, etc.   │           │ NVPTX (GPU!), WASM,  │  │
│  │                       │           │ AMDGPU, SPIRV, etc.   │  │
│  └───────────────────────┘           └───────────────────────┘  │
│                                                                   │
│  GCC: monolithic, 40+ years old, battle-tested.                  │
│  LLVM: modular library design, 20+ years, the modern choice.    │
└──────────────────────────────────────────────────────────────────┘

Why LLVM Won:
  ┌──────────────────────┬──────────────────┬─────────────────────┐
  │                      │ GCC              │ LLVM                │
  ├──────────────────────┼──────────────────┼─────────────────────┤
  │ Architecture         │ Monolithic       │ Library / modular   │
  │ IR                   │ GIMPLE + RTL     │ LLVM IR (one IR)    │
  │ Reusability          │ Hard to embed    │ Easy to embed       │
  │ New language support │ Fork GCC (hard)  │ Emit LLVM IR (easy) │
  │ New hardware         │ Add GCC backend  │ Add LLVM backend    │
  │ License              │ GPL (copyleft)   │ Apache 2.0 (permissive) │
  │ C/C++ codegen quality│ Excellent        │ Excellent (≈equal)  │
  │ Fortran              │ Best (gfortran)  │ Flang (catching up) │
  │ Error messages       │ OK               │ Clang: excellent    │
  │ Used by              │ Linux kernel,    │ Apple, Google, Rust, │
  │                      │ most of Linux    │ Chrome, Android, CUDA│
  └──────────────────────┴──────────────────┴─────────────────────┘

  The key insight: LLVM is a LIBRARY for building compilers.
  Want a new language? Write a frontend that emits LLVM IR.
  LLVM handles optimization + code generation for every target.

  This is why so many languages use LLVM:
    Rust  → rustc frontend → LLVM IR → LLVM backend → x86/ARM
    Swift → swiftc frontend → LLVM IR → LLVM backend → x86/ARM
    Zig   → Zig frontend → LLVM IR → LLVM backend → x86/ARM
    Julia → Julia frontend → LLVM IR → LLVM backend → x86/ARM
    Mojo  → Mojo frontend → MLIR → LLVM IR → LLVM backend
```

## 3. LLVM IR — The Universal Compiler Language

```
LLVM IR is what makes LLVM powerful. It's a well-defined intermediate
representation that sits between source languages and machine code.

Three forms (all equivalent, convertible):
  1. Human-readable (.ll):   %x = add i32 5, 3
  2. Bitcode (.bc):          binary, compact, fast to parse
  3. In-memory C++ objects:  used inside the compiler

Example — a simple function in LLVM IR:

  ; C source:
  ; int square(int x) { return x * x; }

  define i32 @square(i32 %x) {
    %result = mul i32 %x, %x
    ret i32 %result
  }

  ; Key features of LLVM IR:
  ;   - SSA form (%result assigned once)
  ;   - Typed (i32 = 32-bit integer)
  ;   - Low-level (explicit types) but not machine-specific
  ;   - Infinite virtual registers (%x, %result, ...)
  ;   - Target-independent — same IR for x86, ARM, GPU

More complex example — if/else:

  ; C source:
  ; int abs(int x) { return x >= 0 ? x : -x; }

  define i32 @abs(i32 %x) {
  entry:
    %cmp = icmp sge i32 %x, 0       ; signed greater-or-equal
    br i1 %cmp, label %then, label %else

  then:
    br label %merge

  else:
    %neg = sub i32 0, %x             ; -x
    br label %merge

  merge:
    %result = phi i32 [%x, %then], [%neg, %else]   ; SSA: pick value
    ret i32 %result
  }

  The PHI node (φ) is the SSA way of saying "if we came from %then,
  use %x; if we came from %else, use %neg."

  This is the key SSA pattern: instead of mutable variables, you
  MERGE values at control flow join points using phi nodes.
```

### Why SSA Matters

```
Without SSA:                      With SSA:
  x = 5                            x1 = 5
  x = x + 3                        x2 = x1 + 3
  y = x * 2                        y1 = x2 * 2
  x = y - 1                        x3 = y1 - 1

Why SSA is better for optimization:
  - Which "x" does y use? In non-SSA: you have to trace execution.
    In SSA: obviously x2. The answer is in the name.
  - Dead code: x1 = 5 is dead if nothing uses x1. Trivial to detect.
  - Constant propagation: x1 = 5, x2 = x1 + 3 → x2 = 8. Easy!
  - Every optimization pass can reason about values without tracking
    mutation. Values are IMMUTABLE once assigned.

  "SSA is the single most important idea in compiler optimization."
```

## 4. Compiler Optimizations — What the Compiler Does For You

```
Optimizations transform IR to produce faster (or smaller) code.
GCC/LLVM have HUNDREDS of optimization passes. Here are the big ones:

┌────────────────────────────────────────────────────────────────────┐
│ Optimization          │ What it does             │ Example          │
├────────────────────────┼──────────────────────────┼──────────────────┤
│ Constant folding       │ Compute at compile time  │ 5+3 → 8         │
│ Constant propagation   │ Replace var with value   │ x=5; y=x+1 →   │
│                        │                          │ y=6              │
│ Dead code elimination  │ Remove unreachable code  │ if(false){...}   │
│                        │                          │ → removed        │
│ Common subexpression   │ Don't compute same thing │ a*b+c; a*b+d → │
│ elimination (CSE)      │ twice                    │ t=a*b; t+c; t+d │
│ Function inlining      │ Replace call with body   │ f(x)→{body of f}│
│ Loop unrolling         │ Duplicate loop body      │ for(i=0;i<4)    │
│                        │ to reduce branch overhead│ → 4 copies       │
│ Loop invariant code    │ Move computation out     │ for(){y=a*b;...} │
│ motion (LICM)          │ of loop                  │ → y=a*b; for(){}│
│ Strength reduction     │ Replace expensive op     │ x*2 → x<<1      │
│                        │ with cheaper one         │ x*8 → x<<3      │
│ Vectorization (auto)   │ Use SIMD instructions    │ 4 adds → 1 AVX  │
│ Tail call optimization │ Reuse stack frame for    │ recursive → loop │
│                        │ tail-position calls      │                  │
│ Alias analysis         │ Determine if two pointers│ key for reorder- │
│                        │ can refer to same memory │ ing memory ops   │
└────────────────────────┴──────────────────────────┴──────────────────┘
```

### Optimization Levels

```
gcc/clang flags:
  -O0: No optimization. Fastest compile. 1:1 source-to-asm mapping.
       Best for debugging (variables aren't optimized away).

  -O1: Basic optimizations. Constant folding, dead code elimination,
       some inlining. Moderate compile time.

  -O2: The sweet spot. All of O1 plus: loop optimizations, vectorization,
       function inlining, instruction scheduling.
       DEFAULT for production builds.

  -O3: Aggressive. All of O2 plus: more aggressive inlining, loop
       unrolling, vectorization. Can make code LARGER (cache pressure).
       Sometimes SLOWER than O2 due to icache misses.

  -Os: Optimize for size. Like O2 but avoids transforms that increase
       code size. Good for embedded / icache-sensitive code.

  -Ofast: -O3 + fast-math (allows reordering floats, breaks IEEE 754).
          Fastest but can change numerical results.

  -flto: Link-Time Optimization. Optimizes ACROSS translation units
         (files). Can inline functions defined in other .c files.
         Whole-program optimization. Big wins for large projects.

  Real-world impact:
    Benchmark (typical C++ program):
      -O0: 100% runtime (baseline)
      -O1:  40% runtime  (2.5x faster)
      -O2:  30% runtime  (3.3x faster)
      -O3:  28% runtime  (3.6x faster)
      -O3 -flto -march=native: 25% runtime (4x faster)
```

### Inlining — The Most Important Optimization

```
Without inlining:                    With inlining:
  int square(int x) {                 // square() body inserted directly
    return x * x;                     int result = x * x;
  }                                   // No function call overhead!
  int result = square(5);             // Plus: now compiler SEES 5*5
                                      // and can constant-fold to 25!

Why inlining is king:
  1. Eliminates call overhead (push args, jump, return, pop)
  2. ENABLES other optimizations: once the body is visible,
     constant propagation, dead code elimination, vectorization
     all become possible across the inlined code.
  3. Without inlining, the optimizer can't see inside function calls.
     It must assume the function could do ANYTHING.

The cost: inlining increases code size → icache pressure.
  The compiler uses heuristics: inline small functions, hot functions,
  functions with few call sites. Skip large functions or deep recursion.

  __attribute__((always_inline))   // "I know better, force inline"
  __attribute__((noinline))        // "Never inline this" (for profiling)
  #[inline(always)] in Rust        // Same thing
```

### Vectorization — Making Loops Use SIMD

```
Source code:
  void add_arrays(float* a, float* b, float* c, int n) {
    for (int i = 0; i < n; i++)
      c[i] = a[i] + b[i];
  }

Without auto-vectorization:
  ; Process 1 float per iteration
  movss  xmm0, [rdi + rsi*4]    ; load a[i]
  addss  xmm0, [rdx + rsi*4]    ; add b[i]
  movss  [rcx + rsi*4], xmm0    ; store c[i]
  inc    rsi
  ; ... loop back

With auto-vectorization (-O2 on modern compilers):
  ; Process 8 floats per iteration (AVX-256)
  vmovups ymm0, [rdi + rsi*4]    ; load a[i..i+7] (8 floats)
  vaddps  ymm0, ymm0, [rdx+rsi*4] ; add b[i..i+7]
  vmovups [rcx + rsi*4], ymm0    ; store c[i..i+7]
  add     rsi, 8
  ; ... loop back

  8x fewer iterations! (In practice ~4-6x speedup due to memory BW.)

  Requirements for auto-vectorization:
    ✓ Simple loop structure (no complex control flow)
    ✓ No loop-carried dependencies (iteration i doesn't depend on i-1)
    ✓ Contiguous memory access (a[i], not a[i*stride])
    ✓ Known or estimable trip count
    ✗ Pointer aliasing breaks it (use restrict keyword or -fno-alias)

  Check if your loop vectorized:
    gcc -O2 -ftree-vectorize -fopt-info-vec-optimized  file.c
    clang -O2 -Rpass=loop-vectorize  file.c
```

## 5. LLVM Architecture — Why It's a Library

```
The genius of LLVM: it's designed as REUSABLE COMPONENTS.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     LLVM as a Library                                 │
  │                                                                       │
  │  ANY language frontend                                                │
  │  (Clang, rustc, swiftc, ...)                                         │
  │       │                                                               │
  │       │ emits                                                         │
  │       ▼                                                               │
  │  ┌──────────────────────────────────────────────────────────────┐    │
  │  │                    LLVM IR                                    │    │
  │  │  (the contract between frontend and backend)                  │    │
  │  └──────────────────────┬───────────────────────────────────────┘    │
  │                         │                                             │
  │                         ▼                                             │
  │  ┌──────────────────────────────────────────────────────────────┐    │
  │  │              LLVM Optimization Passes                         │    │
  │  │                                                               │    │
  │  │  Pass 1: mem2reg (promote memory to registers / SSA)         │    │
  │  │  Pass 2: instcombine (algebraic simplifications)             │    │
  │  │  Pass 3: inline (function inlining)                          │    │
  │  │  Pass 4: gvn (global value numbering / CSE)                  │    │
  │  │  Pass 5: loop-vectorize                                      │    │
  │  │  Pass 6: ... (200+ passes available)                         │    │
  │  │                                                               │    │
  │  │  You can PICK which passes to run!                           │    │
  │  │    opt -O2 file.ll  →  runs the O2 pass pipeline            │    │
  │  │    opt -inline -gvn file.ll  →  runs just those two         │    │
  │  └──────────────────────┬───────────────────────────────────────┘    │
  │                         │                                             │
  │                         ▼                                             │
  │  ┌──────────────────────────────────────────────────────────────┐    │
  │  │              LLVM Target Backends                              │    │
  │  │                                                               │    │
  │  │  X86 backend    → Intel/AMD CPUs                              │    │
  │  │  AArch64 backend → ARM CPUs (M-series, Graviton)             │    │
  │  │  NVPTX backend  → NVIDIA GPUs (PTX assembly)                 │    │
  │  │  AMDGPU backend → AMD GPUs                                   │    │
  │  │  WASM backend   → WebAssembly                                │    │
  │  │  RISCV backend  → RISC-V CPUs                                │    │
  │  │  SPIRV backend  → Vulkan/OpenCL shaders                      │    │
  │  └──────────────────────────────────────────────────────────────┘    │
  │                                                                       │
  │  Key: M frontends × N backends = M+N combinations, not M×N.         │
  │  Add 1 new language: just write a frontend. Get ALL backends free.   │
  │  Add 1 new CPU arch: just write a backend. ALL languages support it. │
  └──────────────────────────────────────────────────────────────────────┘
```

### Using LLVM From the Command Line

```bash
# C source → LLVM IR → optimized IR → assembly → binary
# (normally clang does all this in one step, but you can split it)

# 1. C → LLVM IR (human-readable)
clang -S -emit-llvm -O0 square.c -o square.ll
cat square.ll
# define i32 @square(i32 %0) {
#   %2 = alloca i32
#   store i32 %0, ptr %2
#   %3 = load i32, ptr %2
#   %4 = load i32, ptr %2
#   %5 = mul nsw i32 %3, %4
#   ret i32 %5
# }

# 2. Optimize LLVM IR
opt -O2 -S square.ll -o square-opt.ll
cat square-opt.ll
# define i32 @square(i32 %0) {
#   %2 = mul nsw i32 %0, %0      ← alloca/load/store all eliminated!
#   ret i32 %2
# }

# 3. LLVM IR → target assembly
llc square-opt.ll -o square.s
# Produces x86-64 assembly (or whatever your target is)

# 4. Assembly → object → executable
as square.s -o square.o
ld square.o -o square    # (simplified, real linking needs libc etc)

# Or just let clang handle it all:
clang -O2 square.c -o square
```

## 6. JIT Compilation — Compiling at Runtime

```
Ahead-of-Time (AOT):  compile once → run many times  (C, Rust, Go)
Just-in-Time (JIT):   compile at runtime → run immediately (Java, JS, Julia)

Why JIT?
  - Can specialize for ACTUAL runtime data (not worst-case assumptions)
  - Can inline virtual method calls (knows which class at runtime)
  - Can recompile hot paths with more aggressive optimization
  - Can de-optimize if assumptions are violated

  ┌──────────────────────────────────────────────────────────────────┐
  │ AOT vs JIT                                                       │
  │                                                                   │
  │ AOT (Clang, GCC, rustc):                                        │
  │   Source → Compiler → Binary → Run → Run → Run                  │
  │   Compile cost: once, upfront                                    │
  │   Runtime cost: zero (already compiled)                          │
  │   Optimization: based on static analysis only                    │
  │                                                                   │
  │ JIT (V8, JVM HotSpot, Julia, PyPy):                             │
  │   Source → Interpreter (slow) → profile → JIT compile hot paths  │
  │   → Run fast → maybe recompile                                   │
  │   Compile cost: ongoing (at runtime)                              │
  │   Runtime benefit: specialize to actual usage patterns            │
  │                                                                   │
  │ Example — JavaScript V8:                                          │
  │   1. Parse JS → bytecode → run in interpreter (Ignition)         │
  │   2. Profile: which functions are called 1000+ times?            │
  │   3. JIT compile those functions (TurboFan) → machine code       │
  │   4. If assumptions break (type changes) → deopt → back to step 1│
  └──────────────────────────────────────────────────────────────────┘

Julia — JIT done right for scientific computing:
  Uses LLVM as its JIT backend.
  Julia code → Julia AST → type inference → LLVM IR → machine code
  First call to a function: ~100ms compile time (JIT compiling)
  Second call: full native speed (cached machine code)

  julia> @code_llvm square(5)
  # Shows you the LLVM IR Julia generated for square(Int64)

  julia> @code_native square(5)
  # Shows you the x86 assembly LLVM generated
  # imulq %rdi, %rdi
  # retq
  # That's it — native speed, just like C.
```

## 7. MLIR — The Compiler Framework for ML

```
MLIR (Multi-Level Intermediate Representation) is LLVM's answer to the
explosion of ML compilers. It's a framework for building IRs at multiple
levels of abstraction.

The problem MLIR solves:

  ┌──────────────────────────────────────────────────────────────────┐
  │ Before MLIR: Everyone builds their own compiler stack            │
  │                                                                   │
  │ TensorFlow → XLA HLO → ... → LLVM IR → GPU code                │
  │ PyTorch    → TorchScript → ... → LLVM IR → GPU code            │
  │ ONNX       → ONNX IR    → ... → LLVM IR → GPU code            │
  │ TVM        → Relay IR   → ... → LLVM IR → GPU code            │
  │                                                                   │
  │ Each has its OWN high-level IR, its OWN optimization passes,     │
  │ its OWN lowering pipeline. Massive duplication of effort.        │
  │                                                                   │
  │ AND there's a huge gap between "matmul" (ML level) and          │
  │ "add i32" (LLVM IR level). Many optimizations can't be done     │
  │ at either level — they need something in between.                │
  └──────────────────────────────────────────────────────────────────┘

  ┌──────────────────────────────────────────────────────────────────┐
  │ After MLIR: Shared infrastructure, multiple levels              │
  │                                                                   │
  │  High-level dialects:                                             │
  │    ┌───────────┐ ┌───────────┐ ┌───────────┐                    │
  │    │ tf dialect │ │torch dialect│ │ tosa dialect│                    │
  │    │ (TF ops)  │ │(PyTorch ops)│ │(standard ML)│                    │
  │    └─────┬─────┘ └─────┬─────┘ └─────┬─────┘                    │
  │          │             │             │                             │
  │          ▼             ▼             ▼                             │
  │    ┌──────────────────────────────────────┐                      │
  │    │ linalg dialect (loop nests / tensors) │  ← TILING, FUSION  │
  │    └──────────────────┬───────────────────┘                      │
  │                       ▼                                           │
  │    ┌──────────────────────────────────────┐                      │
  │    │ affine dialect (polyhedral loops)     │  ← LOOP TRANSFORMS │
  │    └──────────────────┬───────────────────┘                      │
  │                       ▼                                           │
  │    ┌──────────────────────────────────────┐                      │
  │    │ scf dialect (structured control flow) │  ← FOR/IF/WHILE    │
  │    └──────────────────┬───────────────────┘                      │
  │                       ▼                                           │
  │    ┌──────────────────────────────────────┐                      │
  │    │ gpu dialect / vector dialect          │  ← GPU MAPPING      │
  │    └──────────────────┬───────────────────┘                      │
  │                       ▼                                           │
  │    ┌──────────────────────────────────────┐                      │
  │    │ llvm dialect                          │  ← LLVM IR          │
  │    └──────────────────┬───────────────────┘                      │
  │                       ▼                                           │
  │              Native code (x86, ARM, NVPTX)                       │
  └──────────────────────────────────────────────────────────────────┘

Key idea: "dialects" — pluggable sets of operations at different abstraction levels.
  - tf dialect understands "conv2d" and "batch_norm"
  - linalg dialect understands "matrix multiply as nested loops"
  - llvm dialect is just LLVM IR in MLIR's format
  - You can MIX dialects in the same program!
  - Lowering = converting from higher dialect to lower dialect
```

### MLIR Example — From High-Level to Low-Level

```
// High level: tensor operation (like PyTorch)
%result = "torch.aten.matmul"(%a, %b) : (tensor<4x8xf32>, tensor<8x4xf32>)
                                         → tensor<4x4xf32>

// After lowering to linalg dialect:
%result = linalg.matmul ins(%a, %b : tensor<4x8xf32>, tensor<8x4xf32>)
                        outs(%c : tensor<4x4xf32>) → tensor<4x4xf32>

// After tiling (optimization at linalg level):
scf.for %i = 0 to 4 step 2 {
  scf.for %j = 0 to 4 step 2 {
    scf.for %k = 0 to 8 step 4 {
      %tile_a = ... // load 2x4 tile from A
      %tile_b = ... // load 4x2 tile from B
      linalg.matmul ins(%tile_a, %tile_b) outs(%tile_c)
    }
  }
}

// After lowering to loops + vector dialect:
scf.for %i = 0 to 4 {
  scf.for %j = 0 to 4 {
    %sum = arith.constant 0.0 : f32
    scf.for %k = 0 to 8 {
      %a_elem = memref.load %A[%i, %k] : memref<4x8xf32>
      %b_elem = memref.load %B[%k, %j] : memref<8x4xf32>
      %prod = arith.mulf %a_elem, %b_elem : f32
      %sum_new = arith.addf %sum, %prod : f32
    }
    memref.store %sum, %C[%i, %j] : memref<4x4xf32>
  }
}

// After lowering to LLVM dialect → LLVM IR → machine code

// The point: each dialect level enables DIFFERENT optimizations.
// - At linalg level: tile the matmul for cache/shared memory
// - At affine level: fuse loops, parallelize
// - At gpu level: map loops to thread blocks and warps
// - At llvm level: register allocation, instruction scheduling
```

## 8. XLA — Google's ML Compiler (JAX & TensorFlow)

```
XLA (Accelerated Linear Algebra) compiles entire computation graphs
into optimized machine code. It's the engine behind JAX.

How XLA works:

  Python (JAX/TensorFlow)
       │
       │  trace computation graph
       ▼
  ┌──────────────────┐
  │ HLO (High Level  │   XLA's IR. Operations like: dot, conv, reduce,
  │ Operations)      │   broadcast, gather, scatter, etc.
  └────────┬─────────┘
           │
           ▼
  ┌──────────────────┐
  │ HLO Optimization │   Operator fusion, layout assignment, CSE,
  │ Passes           │   algebraic simplification, dead code elim.
  └────────┬─────────┘
           │
           ▼
  ┌──────────────────┐
  │ Target Codegen   │   CPU: LLVM IR → x86/ARM machine code
  │                  │   GPU: LLVM IR → PTX → SASS (NVIDIA)
  │                  │   TPU: custom backend
  └──────────────────┘

The killer feature: OPERATOR FUSION.

Without fusion (PyTorch eager mode):
  y = relu(matmul(x, w) + b)

  Kernel 1: matmul  → writes to HBM
  Kernel 2: add b   → reads from HBM, writes to HBM
  Kernel 3: relu    → reads from HBM, writes to HBM
  = 3 kernel launches, 5 HBM read/writes

With fusion (XLA / torch.compile):
  y = fused_kernel(x, w, b)    ← ONE kernel does all three

  1 kernel launch:
    matmul → result stays in registers/shared memory
    add b  → still in registers
    relu   → still in registers
    write final result to HBM
  = 1 kernel launch, 1 HBM write

  For memory-bound ops (which most LLM ops are), fusion is
  the single biggest optimization. 2-5x speedup is common.
```

### JAX — Functional Python That Compiles

```python
import jax
import jax.numpy as jnp

# JAX looks like NumPy but compiles to XLA
def f(x, w, b):
    return jax.nn.relu(x @ w + b)

# jax.jit = "compile this function with XLA"
f_compiled = jax.jit(f)

x = jnp.ones((32, 128))
w = jnp.ones((128, 64))
b = jnp.ones((64,))

# First call: trace → HLO → optimize → codegen → run. Slow (~1s).
result = f_compiled(x, w, b)

# Second call onward: run cached compiled code. Fast (~0.01ms).
result = f_compiled(x, w, b)

# How it works internally:
#   1. JAX "traces" f() with abstract shapes (not real values)
#   2. Tracing produces a "jaxpr" (JAX expression graph)
#   3. jaxpr → XLA HLO graph
#   4. XLA optimizes: fuse matmul+add+relu into one kernel
#   5. XLA compiles to GPU PTX → cache the compiled code
#   6. Subsequent calls just dispatch to the cached kernel

# See the XLA HLO graph:
print(jax.make_jaxpr(f)(x, w, b))
# { lambda ; a:f32[32,128] b:f32[128,64] c:f32[64]. let
#     d:f32[32,64] = dot_general[...] a b
#     e:f32[32,64] = add d c
#     f:f32[32,64] = max e 0.0       ← relu = max(x, 0)
#   in (f,) }
```

### JAX's Key Transformations

```python
# JAX's power: function transformations that compose.

# 1. jit — compile with XLA
@jax.jit
def f(x): return jnp.sin(x) ** 2

# 2. grad — automatic differentiation
grad_f = jax.grad(f)        # df/dx
grad_f(1.0)                  # = 2*sin(1)*cos(1) ≈ 0.909

# 3. vmap — auto-vectorization (batch any function)
# "given a function that works on 1 example, run it on a batch"
def predict_one(params, x):
    return params @ x

# Vectorize over the batch dimension:
predict_batch = jax.vmap(predict_one, in_axes=(None, 0))
# params is shared, x varies over axis 0 (batch)

# 4. pmap — auto-parallelization across devices
@jax.pmap
def train_step(params, batch):
    loss, grads = jax.value_and_grad(loss_fn)(params, batch)
    grads = jax.lax.pmean(grads, 'batch')  # AllReduce across GPUs
    return params - lr * grads

# These compose! grad of vmap of jit works:
batched_grad = jax.jit(jax.vmap(jax.grad(f)))

# How this works at the compiler level:
#   jax.grad:  transforms jaxpr → backward jaxpr (AD rules)
#   jax.vmap:  transforms jaxpr → batched jaxpr (adds batch dims)
#   jax.jit:   sends jaxpr to XLA → compiled kernel
#   All are source-to-source transformations on the same IR!
```

## 9. torch.compile — PyTorch's Compiler Stack

```
PyTorch historically ran in EAGER mode (execute ops immediately).
torch.compile (PyTorch 2.0+) adds a compiler on top:

  ┌──────────────────────────────────────────────────────────────────┐
  │ How torch.compile works:                                         │
  │                                                                   │
  │  @torch.compile                                                   │
  │  def f(x):                                                       │
  │      y = x * 2 + 1                                               │
  │      return torch.relu(y)                                        │
  │                                                                   │
  │  Step 1: TorchDynamo (Python-level tracing)                      │
  │    Intercepts Python bytecode to capture the computation graph.   │
  │    Handles Python control flow, data-dependent shapes, etc.      │
  │    Output: FX Graph (ATen-level operations)                      │
  │                                                                   │
  │  Step 2: AOTAutograd                                              │
  │    Traces the forward AND backward pass at compile time.         │
  │    Produces a joint forward+backward graph.                      │
  │                                                                   │
  │  Step 3: Inductor (default backend)                              │
  │    FX Graph → Triton kernels (GPU) or C++ (CPU)                  │
  │    Operator fusion: mul+add+relu → ONE Triton kernel             │
  │    Memory planning: minimize allocations                          │
  │                                                                   │
  │  Step 4: Triton (GPU kernel language)                            │
  │    Triton code → PTX → SASS (NVIDIA GPU machine code)            │
  │    Auto-tunes tile sizes for your specific GPU.                  │
  │                                                                   │
  │  ┌──────────────────────────────────────────────────────┐       │
  │  │ Python → TorchDynamo → FX Graph → AOTAutograd        │       │
  │  │   → Inductor → Triton → PTX → SASS                   │       │
  │  │                                                       │       │
  │  │ vs JAX:                                               │       │
  │  │ Python → JAX tracing → jaxpr → XLA HLO → PTX → SASS  │       │
  │  └──────────────────────────────────────────────────────┘       │
  └──────────────────────────────────────────────────────────────────┘

# Real usage:
import torch

model = MyModel().cuda()
model = torch.compile(model)     # compile the whole model

# First forward pass: trace, compile, cache. Slow.
y = model(x)

# Subsequent passes: run cached compiled kernels. Fast.
y = model(x)

# Typical speedup: 1.3-2x for training, 1.5-3x for inference.
```

## 10. Triton — Writing GPU Kernels in Python

```python
# Triton (by OpenAI) lets you write GPU kernels in Python.
# It's what torch.compile generates when targeting GPU.
# You can also write Triton kernels directly for custom ops.

import triton
import triton.language as tl
import torch

@triton.jit
def add_kernel(
    x_ptr, y_ptr, out_ptr,
    n_elements,
    BLOCK_SIZE: tl.constexpr,  # compile-time constant
):
    # Each "program" (≈ thread block) processes BLOCK_SIZE elements
    pid = tl.program_id(0)
    offsets = pid * BLOCK_SIZE + tl.arange(0, BLOCK_SIZE)
    mask = offsets < n_elements

    # Load, compute, store
    x = tl.load(x_ptr + offsets, mask=mask)
    y = tl.load(y_ptr + offsets, mask=mask)
    tl.store(out_ptr + offsets, x + y, mask=mask)

# Launch
n = 1000000
x = torch.randn(n, device='cuda')
y = torch.randn(n, device='cuda')
out = torch.empty_like(x)

grid = lambda meta: (triton.cdiv(n, meta['BLOCK_SIZE']),)
add_kernel[grid](x, y, out, n, BLOCK_SIZE=1024)

# Triton vs CUDA:
#   CUDA:   write C++, manage threads/warps/shared memory manually.
#   Triton: write Python-like code, compiler handles tiling/coalescing.
#
#   Triton abstracts away:
#     - Warp-level details (you think in "blocks" not "warps")
#     - Memory coalescing (handled by compiler)
#     - Shared memory management (automatic)
#     - Synchronization (__syncthreads → automatic)
#
#   Triton compilation: Python AST → Triton IR → MLIR → LLVM IR → PTX
#   (Yes, Triton uses MLIR under the hood!)
```

## 11. The Full ML Compiler Landscape

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        ML Compiler Landscape (2025)                      │
│                                                                          │
│  Framework layer:                                                        │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐               │
│  │ PyTorch  │  │  JAX     │  │ TensorFlow│  │  Mojo    │               │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘               │
│       │              │              │              │                      │
│  Compiler layer:     │              │              │                      │
│       ▼              ▼              ▼              ▼                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐               │
│  │TorchDynamo│  │XLA (HLO) │  │XLA (HLO) │  │  MLIR   │               │
│  │+ Inductor│  │          │  │          │  │(directly)│               │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘               │
│       │              │              │              │                      │
│  Kernel layer:       │              │              │                      │
│       ▼              ▼              ▼              ▼                      │
│  ┌──────────┐  ┌──────────────────────────┐  ┌──────────┐              │
│  │ Triton   │  │       LLVM / MLIR        │  │Custom Gen│              │
│  │ (→MLIR)  │  │                          │  │          │              │
│  └────┬─────┘  └────────────┬─────────────┘  └────┬─────┘              │
│       │                     │                      │                     │
│  Hardware layer:            │                      │                     │
│       ▼                     ▼                      ▼                     │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │  cuBLAS / cuDNN          │ GPU (PTX→SASS)       │ TPU            │   │
│  │  (for ops Triton can't   │ via NVPTX backend    │ custom backend │   │
│  │   beat — expert CUDA)    │                      │                │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│  Key insight: EVERYTHING eventually goes through LLVM (for GPU/CPU)     │
│  or custom backends (for TPU/custom accelerators).                      │
│  MLIR is becoming the SHARED LAYER that everyone builds on.             │
└─────────────────────────────────────────────────────────────────────────┘

The trend:
  2015: hand-write CUDA kernels for everything
  2018: use cuBLAS/cuDNN for standard ops, CUDA for custom
  2021: Triton replaces most custom CUDA
  2023: torch.compile/JAX auto-generate Triton from Python
  2025: MLIR-based stacks handle multi-target compilation
  Future: write Python, compiler figures out CPU/GPU/TPU/custom HW
```

## 12. Compilation End-to-End — From Python to GPU

```
Let's trace a REAL operation through the entire compilation stack.

Python:
  import torch
  x = torch.randn(1024, 1024, device='cuda')
  y = torch.relu(x @ x.T + 1.0)

What happens (eager mode, no compile):
  1. x @ x.T → PyTorch dispatches to cuBLAS sgemm kernel
     → cuBLAS calls Tensor Cores → result in HBM
  2. +1.0 → launches element-wise CUDA kernel (add scalar)
     → reads from HBM, writes to HBM
  3. relu → launches element-wise CUDA kernel (max(0, x))
     → reads from HBM, writes to HBM
  Total: 3 kernel launches, 2 unnecessary HBM round-trips

What happens (torch.compile):
  1. TorchDynamo intercepts Python bytecode
  2. Captures FX graph: matmul → add → relu
  3. Inductor sees: add + relu are element-wise → FUSE THEM
     matmul stays as cuBLAS (can't beat it)
  4. Generate 1 Triton kernel for (add + relu):
       @triton.jit
       def fused_add_relu(in_ptr, out_ptr, ...):
           x = tl.load(in_ptr + offsets)
           x = x + 1.0
           x = tl.maximum(x, 0.0)
           tl.store(out_ptr + offsets, x)
  5. Launch: cuBLAS matmul → fused Triton kernel
  Total: 2 kernel launches, 1 fewer HBM round-trip. Faster!

  The Triton kernel compiles:
    Triton Python → Triton IR → MLIR (triton dialect)
    → MLIR (gpu dialect) → LLVM IR → PTX assembly
    → (NVIDIA ptxas) → SASS (GPU machine code)

  The PTX for the fused kernel:
    ld.global.f32 %f1, [%rd1];      // load from HBM
    add.f32       %f2, %f1, 0f3F800000;  // add 1.0
    max.f32       %f3, %f2, 0f00000000;  // relu (max with 0)
    st.global.f32 [%rd2], %f3;      // store to HBM
    // That's it. One load, one store, two arithmetic ops.
    // Without fusion: three loads, three stores.
```

## Interview Quick Reference

| Question | Key Points |
|----------|-----------|
| "How does a compiler work?" | Lexer → Parser → Semantic Analysis → IR Gen → Optimization → Codegen. Key IR is SSA form (each variable assigned once). |
| "Why LLVM?" | Modular library design. M frontends × N backends = M+N work not M×N. Permissive license. Used by Rust, Swift, Julia, Clang, etc. |
| "What is SSA?" | Static Single Assignment. Each variable defined exactly once. Enables easy optimization: dead code detection, constant propagation, value tracking. Uses phi nodes at control flow merges. |
| "What does -O2 do?" | ~100 optimization passes: inlining, constant folding/propagation, dead code elimination, loop vectorization, CSE, LICM, strength reduction. ~3x faster than -O0. |
| "JIT vs AOT?" | AOT: compile once, no runtime cost. JIT: compile at runtime, can specialize to actual data/types. V8 (JS), HotSpot (Java), Julia use JIT. |
| "What is MLIR?" | Multi-level IR framework from LLVM. Pluggable "dialects" at different abstraction levels (tensor ops → loops → LLVM IR). Shared infrastructure for ML compilers. |
| "What does torch.compile do?" | TorchDynamo traces Python → FX graph → Inductor fuses ops → Triton kernels → MLIR → LLVM → PTX. Key win: operator fusion reduces HBM round-trips. |
| "What is XLA / JAX?" | XLA is Google's ML compiler. JAX traces Python → jaxpr → XLA HLO → optimized + fused → GPU/TPU code. Key: jit + grad + vmap + pmap compose as IR transforms. |
| "Why is operator fusion important?" | Element-wise ops are memory-bound. Without fusion: each op reads/writes HBM. With fusion: chain of ops stays in registers, one read + one write total. 2-5x speedup. |
| "Triton vs CUDA?" | Triton: Python-level GPU kernel DSL. Abstracts warps, shared memory, coalescing. Compiles via MLIR → LLVM → PTX. Can't beat hand-tuned CUDA (cuBLAS) for matmul, but great for fused element-wise ops. |
