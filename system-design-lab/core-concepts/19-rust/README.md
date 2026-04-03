# Rust — Interview Essentials

## Why Rust in Systems Interviews

Rust is the go-to for new systems software: cloud infrastructure (AWS Firecracker, S3),
databases (TiKV, SurrealDB), networking (Cloudflare, Linkerd), crypto (Solana), and
increasingly ML inference. If you're interviewing at these companies, know Rust deeply.

---

## 1. Ownership — The Core Idea

Every value has exactly **one owner**. When the owner goes out of scope, the value is dropped.
No garbage collector. No manual free. The compiler enforces it at compile time.

```rust
fn main() {
    let s1 = String::from("hello");   // s1 owns the String
    let s2 = s1;                       // ownership MOVED to s2. s1 is now invalid.
    // println!("{s1}");               // ← compile error! s1 was moved.
    println!("{s2}");                  // ✓ s2 is the owner
}  // s2 goes out of scope → String is dropped (freed)
```

### Why this matters

```
C/C++:   you forget to free → memory leak
         you free twice     → crash (double free)
         you use after free → undefined behavior

Rust:    compiler won't LET you do any of these.
         Ownership + borrow checker = memory safety at compile time.
         Zero runtime cost (no GC, no ref counting by default).
```

---

## 2. Borrowing & Lifetimes

**Borrowing** = temporarily accessing data without taking ownership.

```
Two rules (enforced at compile time):
  1. Many immutable borrows (&T)   → readers can share
  2. One mutable borrow (&mut T)   → writer gets exclusive access
  Never both at the same time.
```

```rust
let mut s = String::from("hello");

// Multiple immutable borrows — OK
let r1 = &s;
let r2 = &s;
println!("{r1} {r2}");  // ✓

// One mutable borrow — OK
let r3 = &mut s;
r3.push_str(" world");  // ✓

// Immutable + mutable at same time — compile error!
// let r4 = &s;
// let r5 = &mut s;      // ✗ can't borrow as mutable while immutable borrow exists
```

### Lifetimes

Tell the compiler how long a reference is valid. Usually inferred, sometimes you need to annotate.

```rust
// Compiler can't figure out which input the return borrows from
fn longest<'a>(s1: &'a str, s2: &'a str) -> &'a str {
    if s1.len() > s2.len() { s1 } else { s2 }
}
// 'a means: the returned reference lives at least as long as BOTH inputs.

// Common lifetime patterns:
&'a T          // reference valid for lifetime 'a
&'static str   // reference valid for entire program (string literals)
```

### When you'll fight the borrow checker

```rust
// Problem: can't mutate while iterating
let mut v = vec![1, 2, 3];
for item in &v {
    // v.push(4);  // ✗ can't mutate v while borrowed by iterator
}

// Solutions:
// 1. Collect indices first, mutate after
// 2. Use .iter_mut() if you need to modify elements
// 3. Use interior mutability (RefCell, Mutex)
```

---

## 3. Error Handling — Result & Option

**No exceptions in Rust.** Errors are values, handled explicitly.

```rust
// Option<T> = value might not exist (replaces null)
fn find_user(id: u64) -> Option<User> {
    if id == 42 { Some(user) } else { None }
}

// Result<T, E> = operation might fail (replaces exceptions)
fn read_file(path: &str) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}

// The ? operator — propagate errors cleanly
fn process() -> Result<(), Box<dyn Error>> {
    let content = std::fs::read_to_string("config.toml")?;  // return Err if fails
    let config: Config = toml::from_str(&content)?;          // return Err if fails
    Ok(())
}

// Pattern matching — handle all cases
match find_user(42) {
    Some(user) => println!("Found: {}", user.name),
    None => println!("Not found"),
}
```

### Comparison with other languages

```
Java/C++:    try { ... } catch (Exception e) { ... }  ← hidden control flow
Go:          val, err := foo(); if err != nil { ... }  ← manual, verbose
Rust:        let val = foo()?;                          ← concise, compile-checked
```

---

## 4. Traits (Rust's "Interfaces")

```rust
trait Summary {
    fn summarize(&self) -> String;

    // Default implementation (can be overridden)
    fn preview(&self) -> String {
        format!("{}...", &self.summarize()[..50])
    }
}

struct Article { title: String, content: String }

impl Summary for Article {
    fn summarize(&self) -> String {
        format!("{}: {}", self.title, self.content)
    }
}

// Trait bounds — "this function works for any type that implements Summary"
fn notify(item: &impl Summary) {
    println!("Breaking: {}", item.summarize());
}

// Equivalent, more explicit:
fn notify<T: Summary>(item: &T) { ... }

// Multiple bounds:
fn process<T: Summary + Display + Clone>(item: &T) { ... }
```

### Key traits to know

```
┌──────────────────┬──────────────────────────────────────────────┐
│ Trait             │ What it does                                │
├──────────────────┼──────────────────────────────────────────────┤
│ Clone            │ Explicit deep copy (.clone())                │
│ Copy             │ Implicit bitwise copy (only for small types) │
│ Debug            │ {:?} formatting                              │
│ Display          │ {} formatting (user-facing)                  │
│ Drop             │ Destructor (cleanup logic)                   │
│ From/Into        │ Type conversion                              │
│ Iterator         │ .next() → Option<Item>                       │
│ Send             │ Safe to transfer between threads             │
│ Sync             │ Safe to share references between threads     │
│ Deref            │ Smart pointer dereferencing                  │
│ Serialize/       │ serde serialization (not built-in, but       │
│   Deserialize    │ universally used)                            │
└──────────────────┴──────────────────────────────────────────────┘
```

---

## 5. Enums & Pattern Matching

Rust enums are **algebraic data types** — each variant can hold different data.

```rust
enum Message {
    Quit,                       // no data
    Move { x: i32, y: i32 },   // named fields
    Write(String),              // single value
    Color(u8, u8, u8),          // tuple
}

// Pattern matching — must handle ALL variants (exhaustive)
fn handle(msg: Message) {
    match msg {
        Message::Quit => println!("quit"),
        Message::Move { x, y } => println!("move to {x},{y}"),
        Message::Write(text) => println!("write: {text}"),
        Message::Color(r, g, b) => println!("color: {r},{g},{b}"),
    }
}

// Option and Result are just enums:
// enum Option<T> { Some(T), None }
// enum Result<T, E> { Ok(T), Err(E) }
```

---

## 6. Concurrency — "Fearless Concurrency"

The compiler prevents data races at compile time using `Send` and `Sync` traits.

```rust
use std::sync::{Arc, Mutex};
use std::thread;

// Shared mutable state: Arc (atomic ref count) + Mutex
let counter = Arc::new(Mutex::new(0));
let mut handles = vec![];

for _ in 0..10 {
    let counter = Arc::clone(&counter);
    handles.push(thread::spawn(move || {
        let mut num = counter.lock().unwrap();
        *num += 1;
    }));
}

for h in handles { h.join().unwrap(); }
println!("Result: {}", *counter.lock().unwrap());  // 10
```

### Concurrency primitives

```
┌──────────────────┬──────────────────────────────────────────────┐
│ Primitive        │ When to use                                  │
├──────────────────┼──────────────────────────────────────────────┤
│ Mutex<T>         │ Shared mutable data (lock + unlock)          │
│ RwLock<T>        │ Many readers, few writers                    │
│ Arc<T>           │ Shared ownership across threads              │
│ mpsc::channel    │ Message passing (multiple producers, 1 consumer) │
│ Atomic*          │ Lock-free counters and flags                 │
│ Condvar          │ Wait for a condition                         │
│ thread::spawn    │ OS threads                                   │
│ tokio::spawn     │ Async tasks (green threads)                  │
│ rayon            │ Data parallelism (parallel iterators)        │
└──────────────────┴──────────────────────────────────────────────┘

// Data race at compile time? Impossible.
let data = vec![1, 2, 3];
thread::spawn(move || println!("{data:?}"));
// println!("{data:?}");  // ✗ compile error: data was moved into the thread
```

---

## 7. Async/Await

```rust
// async fn returns a Future — doesn't run until .await'd
async fn fetch_data(url: &str) -> Result<String, reqwest::Error> {
    let body = reqwest::get(url).await?.text().await?;
    Ok(body)
}

// Futures are lazy — nothing happens until an executor polls them
#[tokio::main]
async fn main() {
    // Sequential:
    let a = fetch_data("url1").await;
    let b = fetch_data("url2").await;

    // Concurrent (both in-flight at once):
    let (a, b) = tokio::join!(
        fetch_data("url1"),
        fetch_data("url2"),
    );

    // Spawn independent task:
    tokio::spawn(async {
        // runs concurrently on the tokio thread pool
    });
}
```

### Key async concepts

```
Future:      a value that will be ready later (.poll() → Pending | Ready)
Runtime:     tokio, async-std — drives futures to completion
.await:      suspend this task, let other tasks run, resume when ready
Pin:         prevents moving a self-referential future in memory
Send:        future can be sent across threads (required for tokio::spawn)
```

---

## 8. Iterators & Closures

```rust
let nums = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

// Chained iterator — lazy, zero allocation until .collect()
let result: Vec<i32> = nums.iter()
    .filter(|&&x| x % 2 == 0)     // keep evens
    .map(|&x| x * x)               // square them
    .take(3)                        // first 3 only
    .collect();                     // [4, 16, 36]

// Common iterator methods:
//   .map()       transform each element
//   .filter()    keep elements matching predicate
//   .flat_map()  map + flatten (like flatMap in Scala/JS)
//   .fold()      accumulate (like reduce)
//   .any()       true if any element matches
//   .all()       true if all elements match
//   .find()      first matching element → Option
//   .enumerate() adds index: (0, elem), (1, elem), ...
//   .zip()       combine two iterators pairwise
//   .collect()   gather into a collection (Vec, HashMap, String, etc.)

// Closures (anonymous functions)
let add = |a, b| a + b;            // inferred types
let add: fn(i32, i32) -> i32 = |a, b| a + b;  // explicit

// Closures capture variables from enclosing scope:
let threshold = 5;
let big = nums.iter().filter(|&&x| x > threshold);  // captures threshold
```

---

## 9. Common Interview Questions

### Ownership & Borrowing
```
Q: What's the difference between & and &mut?
A: & = shared immutable borrow (many allowed)
   &mut = exclusive mutable borrow (only one allowed)
   Enforced at compile time → no data races.

Q: What does 'move' do in a closure?
A: Forces the closure to take ownership of captured variables
   (instead of borrowing). Required for thread::spawn closures.

Q: When would you use Rc vs Arc?
A: Rc = single-threaded shared ownership (cheaper, no atomic ops)
   Arc = multi-threaded shared ownership (atomic ref counting)
   Rc is NOT Send → compiler won't let you use it across threads.

Q: What's the difference between String and &str?
A: String = owned, heap-allocated, growable string
   &str = borrowed reference to a string slice (can point to String, literal, or substring)
   Functions should usually take &str (more flexible).
```

### Trait & Type System
```
Q: How are traits different from interfaces (Java) or abstract classes (C++)?
A: Traits can have default methods, can be implemented for external types,
   support static dispatch (generics) AND dynamic dispatch (dyn Trait).
   No inheritance hierarchy — composition over inheritance.

Q: What's dyn Trait vs impl Trait?
A: impl Trait = static dispatch (monomorphized, each type gets its own code, faster)
   dyn Trait  = dynamic dispatch (vtable, one code path, allows heterogeneous collections)

Q: What's a zero-sized type (ZST)?
A: A type with no data (e.g., PhantomData, empty struct). Takes 0 bytes.
   Vec<()> allocates 0 bytes for elements. Used for type-level markers.
```

### Concurrency
```
Q: How does Rust prevent data races?
A: Ownership + Send + Sync traits. You can't share &mut across threads.
   The compiler rejects code that could have data races.

Q: What's the difference between thread::spawn and tokio::spawn?
A: thread::spawn = OS thread (heavy, ~8MB stack, pre-emptive)
   tokio::spawn = async task on thread pool (light, ~few KB, cooperative)
   Use OS threads for CPU-heavy work, async tasks for I/O-heavy work.
```

---

## 10. Rust's Key Differentiators

```
┌─────────────────────┬─────────────────────────────────────────────────┐
│ Feature             │ Why it matters                                   │
├─────────────────────┼─────────────────────────────────────────────────┤
│ No null             │ Option<T> forces you to handle the None case     │
│ No exceptions       │ Result<T,E> makes errors visible in the type     │
│ No data races       │ Compiler rejects unsafe sharing at compile time  │
│ No GC               │ Predictable latency (no GC pauses)              │
│ Zero-cost abstracts │ Iterators, generics compile to same code as C    │
│ Cargo               │ Best-in-class package manager + build system     │
│ Pattern matching    │ Exhaustive match → compiler catches missing cases│
│ Algebraic types     │ enum variants carry data (tagged unions)         │
│ Macros              │ Compile-time code generation (derive, proc macros)│
│ Unsafe              │ Escape hatch when you need it (FFI, raw ptrs)    │
└─────────────────────┴─────────────────────────────────────────────────┘
```

---

## 11. Ecosystem to Know

```
Web:         axum, actix-web, warp
Async:       tokio, async-std
Serialization: serde (+ serde_json, toml, etc.)
CLI:         clap
Database:    sqlx, diesel, sea-orm
HTTP client: reqwest
Logging:     tracing
Error:       thiserror (libraries), anyhow (applications)
Testing:     built-in (#[test]), criterion (benchmarks)
Concurrency: rayon (data parallelism), crossbeam (lock-free)
Crypto:      ring, rustls
```
