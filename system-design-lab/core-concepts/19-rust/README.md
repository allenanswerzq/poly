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

### Marker Traits — Send, Sync, Sized, Unpin

These are **auto traits** — the compiler implements them automatically when safe. They carry no methods; they're pure compile-time contracts.

```
┌──────────┬──────────────────────────────────────────────────────────────┐
│ Trait    │ Meaning                                                      │
├──────────┼──────────────────────────────────────────────────────────────┤
│ Send     │ A type can be TRANSFERRED to another thread.                 │
│          │ "I can give this value to another thread."                   │
│          │                                                              │
│ Sync     │ A type can be REFERENCED from multiple threads (&T is Send).│
│          │ "Multiple threads can read this simultaneously."            │
│          │                                                              │
│ Sized    │ Size known at compile time. Most types are Sized.           │
│          │ str and [T] are NOT Sized (dynamically sized types / DSTs). │
│          │                                                              │
│ Unpin    │ Type can be safely moved in memory after being pinned.      │
│          │ Most types are Unpin. Self-referential futures are !Unpin.  │
└──────────┴──────────────────────────────────────────────────────────────┘
```

#### Send & Sync — The Thread Safety Duo

```rust
// Most types are Send + Sync automatically.
// The compiler derives them based on what the type contains.

// ✓ Send + Sync:  i32, String, Vec<T>, Arc<T>, Mutex<T>
// ✓ Send, !Sync:  Cell<T>, RefCell<T>   (interior mutability, not thread-safe)
// !Send, !Sync:   Rc<T>, *mut T          (raw pointers, Rc is single-threaded)

// WHY this matters: the compiler ENFORCES these at every boundary.

use std::rc::Rc;
use std::sync::Arc;

// ✗ COMPILE ERROR — Rc is !Send, can't send to another thread
let data = Rc::new(42);
std::thread::spawn(move || {
    println!("{data}");  // error: Rc<i32> cannot be sent between threads safely
});

// ✓ Arc is Send + Sync — this compiles
let data = Arc::new(42);
std::thread::spawn(move || {
    println!("{data}");  // OK
});
```

```
The relationship between Send and Sync:

  T is Sync  ⟺  &T is Send

  If you can safely share a reference (&T) across threads, T is Sync.
  If you can safely move the value itself to another thread, T is Send.

  Why Mutex<T> is Sync (even though it contains mutable data):
    &Mutex<T> only lets you .lock() → one thread at a time.
    The Mutex ensures exclusive access. Safe to share the reference.

  Why RefCell<T> is !Sync:
    &RefCell<T> lets you .borrow_mut() without compile-time checks.
    Two threads could call .borrow_mut() simultaneously → data race.
    RefCell uses runtime checks that are NOT atomic → not thread-safe.

  Why Rc<T> is !Send:
    Rc uses non-atomic reference counting. If you moved it to another
    thread, both threads could increment/decrement the count simultaneously
    → corrupted refcount → use-after-free. Use Arc instead (atomic).
```

#### Sized — Compile-Time Known Size

```rust
// Almost everything is Sized:
//   i32: 4 bytes. String: 24 bytes (ptr + len + cap). Vec<T>: 24 bytes.

// Dynamically Sized Types (DSTs) are NOT Sized:
//   str     — a string of unknown length
//   [T]     — a slice of unknown length
//   dyn Trait — a trait object of unknown concrete type

// You can never have a bare DST on the stack. Always behind a pointer:
//   &str, &[T], &dyn Trait        — fat pointer (ptr + len or ptr + vtable)
//   Box<str>, Box<[T]>, Box<dyn Trait>

// Generic params are Sized by default:
fn foo<T>(t: T) {}            // T: Sized (implicit)
fn bar<T: ?Sized>(t: &T) {}   // T might NOT be Sized (opt out with ?Sized)
//                                 allows bar(&str), bar(&[i32]), etc.

// Why ?Sized matters:
//   fn print_len(s: &str) {}        — only accepts &str
//   fn print_len<T: AsRef<str>>(s: &T) {} — accepts String, &str, etc.
//   The ?Sized bound is needed anytime you want to accept unsized types.
```

#### Unpin — Safe to Move After Pinning

```rust
// Most types are Unpin — you don't think about this day-to-day.
// It only matters for self-referential types, mainly async futures.

// async fn produces a Future that may contain self-references:
async fn example() {
    let data = vec![1, 2, 3];
    let r = &data;        // r points to data (self-referential!)
    some_async_op().await; // future is suspended here, r must stay valid
    println!("{r:?}");
}
// If the future were moved in memory, r would be a dangling pointer.
// Pin<&mut Future> prevents moving it → self-references stay valid.

// In practice:
//   - You rarely implement !Unpin manually
//   - Box::pin() and tokio::pin!() handle it for you
//   - tokio::spawn requires Send, not Unpin (futures are pinned internally)
//   - You'll hit Pin when writing manual Future impls or low-level async code
```

### Other Essential Std Traits

```rust
// --- Clone & Copy ---
// Clone: explicit deep copy via .clone()
// Copy:  implicit bitwise copy (assignment copies, not moves)
//        Only for small, stack-only types: i32, f64, bool, (i32, i32), etc.
//        Copy implies Clone. Copy types are never moved, always copied.

let a: i32 = 5;
let b = a;       // Copy — a is still valid
let s = String::from("hello");
let t = s;       // Move — s is invalidated (String is NOT Copy)

// --- From / Into ---
// The standard way to do type conversions. Implement From, get Into free.
impl From<i32> for MyType {
    fn from(val: i32) -> Self { MyType(val) }
}
let x: MyType = 42.into();        // uses Into (auto-derived from From)
let y: MyType = MyType::from(42); // uses From directly

// The ? operator uses From to convert error types:
fn read() -> Result<(), MyError> {
    let f = std::fs::read("x")?;  // io::Error → MyError via From
    Ok(())
}

// --- Deref & DerefMut ---
// Smart pointer coercion. Makes Box<T>, Arc<T>, Vec<T> act like T.
let s: Box<String> = Box::new(String::from("hello"));
// s.len() works because Box<String> derefs to String, which derefs to str.
// Deref chain: Box<String> → String → str → .len()

// --- Drop ---
// Destructor. Called automatically when value goes out of scope.
// Used for: closing files, releasing locks, freeing memory.
impl Drop for MyResource {
    fn drop(&mut self) {
        println!("cleaning up!");
    }
}
// You can't call .drop() manually. Use std::mem::drop(value) to drop early.

// --- Default ---
// Provides a default value. #[derive(Default)] works for structs if all fields are Default.
#[derive(Default)]
struct Config {
    retries: u32,      // defaults to 0
    verbose: bool,     // defaults to false
    name: String,      // defaults to ""
}
let cfg = Config::default();
let cfg = Config { retries: 3, ..Default::default() };  // partial override

// --- AsRef & AsMut ---
// Cheap reference conversions. Makes APIs flexible.
fn read_file(path: impl AsRef<Path>) {
    let p: &Path = path.as_ref();
    // accepts: &str, String, PathBuf, &Path — all implement AsRef<Path>
}

// --- PartialEq / Eq, PartialOrd / Ord, Hash ---
// Comparison and hashing. Usually #[derive].
// PartialEq: ==, !=    (f64 is PartialEq but NOT Eq — NaN != NaN)
// Eq: full equivalence  (required for HashMap keys)
// Ord: total ordering   (required for BTreeMap keys)
// Hash: hashing         (required for HashMap keys, must agree with Eq)
#[derive(PartialEq, Eq, Hash)]
struct UserId(u64);
```

### Trait Cheat Sheet — When to Use What

```
Want to...                         → Use this trait
─────────────────────────────────────────────────────
Print for debugging                → Debug     (derive)
Print for users                    → Display   (manual impl)
Compare equality                   → PartialEq, Eq (derive)
Sort / order                       → PartialOrd, Ord (derive)
Use as HashMap key                 → Eq + Hash (derive both)
Convert between types              → From / Into
Accept flexible input              → AsRef<T>
Clone explicitly                   → Clone (derive)
Copy implicitly (small types)      → Copy + Clone (derive)
Custom cleanup logic               → Drop (manual impl)
Provide sensible default           → Default (derive)
Send to another thread             → Send (auto, don't impl manually)
Share reference across threads     → Sync (auto, don't impl manually)
Accept dynamically-sized types     → ?Sized bound
Work with async / Pin              → Unpin (auto for most types)
Serialize / deserialize            → serde::Serialize, Deserialize (derive)
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
