# C++ — Interview Essentials

## Why C++ in Systems Interviews

C++ is everywhere in high-performance systems: databases (MySQL, MongoDB, ClickHouse),
game engines (Unreal), trading systems, OS kernels (partially), browsers (Chrome), and
ML frameworks (PyTorch, TensorFlow). If you're interviewing at these companies, expect
C++-specific questions alongside system design.

---

## 1. Memory Model

### Stack vs Heap

```
Stack:                              Heap:
┌───────────────────────┐          ┌───────────────────────┐
│ Automatic lifetime     │          │ Manual lifetime        │
│ Fast (just bump SP)    │          │ Slow (malloc/free)     │
│ Fixed size (~1-8 MB)   │          │ Virtually unlimited    │
│ LIFO order             │          │ Any order              │
│ Thread-local           │          │ Shared across threads  │
└───────────────────────┘          └───────────────────────┘

void foo() {
    int x = 42;              // stack — freed when foo() returns
    int* p = new int(42);    // heap — leaked if you forget delete
    auto q = make_unique<int>(42);  // heap, but RAII handles cleanup
}
```

### RAII (Resource Acquisition Is Initialization)

The **most important C++ idiom**. Tie resource lifetime to object lifetime.
Constructor acquires, destructor releases. No manual cleanup needed.

```cpp
// Without RAII — easy to leak
void bad() {
    FILE* f = fopen("data.txt", "r");
    // ... if exception here, f is leaked
    fclose(f);  // might never reach this
}

// With RAII — impossible to leak
void good() {
    auto f = std::ifstream("data.txt");
    // ... exception? destructor still runs, file closed
}  // ~ifstream() closes file automatically

// RAII applies to everything: memory, sockets, locks, DB connections
{
    std::lock_guard<std::mutex> lock(mtx);  // acquires lock
    // ... critical section ...
}  // ~lock_guard() releases lock, even if exception
```

---

## 2. Smart Pointers

**Rule**: Never use `new`/`delete` directly. Always use smart pointers.

```
┌──────────────────┬─────────────┬─────────────┬──────────────────────┐
│ Pointer          │ Ownership   │ Overhead     │ Use When             │
├──────────────────┼─────────────┼─────────────┼──────────────────────┤
│ unique_ptr<T>    │ Exclusive   │ Zero         │ Default choice       │
│ shared_ptr<T>    │ Shared      │ Ref count    │ Multiple owners      │
│ weak_ptr<T>      │ Non-owning  │ Ref count    │ Break cycles         │
│ T*  (raw)        │ Non-owning  │ Zero         │ Observing only       │
└──────────────────┴─────────────┴─────────────┴──────────────────────┘

auto a = make_unique<Widget>();     // a owns the Widget. Only one owner.
auto b = std::move(a);              // b owns it now. a is null.

auto c = make_shared<Widget>();     // ref count = 1
auto d = c;                         // ref count = 2
d.reset();                          // ref count = 1
c.reset();                          // ref count = 0 → Widget destroyed
```

### shared_ptr pitfalls (common interview topic)

```cpp
// Problem 1: Circular reference → memory leak
struct Node {
    shared_ptr<Node> next;   // if A→B→A, ref count never reaches 0
};
// Fix: use weak_ptr for back-references

// Problem 2: Thread safety
// ref count operations are atomic (safe)
// the POINTED-TO object is NOT thread-safe (you still need a mutex)

// Problem 3: Performance
// shared_ptr = 2 pointers (object + control block) + atomic ref counting
// unique_ptr = 1 pointer, zero overhead — prefer it
```

---

## 3. Move Semantics (C++11)

**The single biggest performance feature of modern C++.** Avoid unnecessary copies.

```cpp
// Copy: duplicate the data (expensive for large objects)
std::vector<int> a = {1, 2, 3, ..., 1000000};
std::vector<int> b = a;       // copies 1M integers (slow)

// Move: steal the internal pointer (cheap, O(1))
std::vector<int> c = std::move(a);  // c steals a's buffer, a is now empty

// Under the hood:
//   copy:  allocate new buffer, memcpy all elements
//   move:  copy 3 pointers (data, size, capacity), null out source
```

### lvalue vs rvalue

```
           lvalue                        rvalue
    "has a name, has an address"    "temporary, no address"

    int x = 42;     // x is lvalue    42 is rvalue
    std::string s;   // s is lvalue    std::string("hi") is rvalue

    void foo(Widget&& w);  // rvalue reference — can STEAL from w
    Widget a;
    foo(std::move(a));     // std::move casts a to rvalue → foo can steal
```

### Rule of Five (or Zero)

If you define ANY of these, define ALL five:
```
1. Destructor          ~MyClass()
2. Copy constructor    MyClass(const MyClass&)
3. Copy assignment     MyClass& operator=(const MyClass&)
4. Move constructor    MyClass(MyClass&&)
5. Move assignment     MyClass& operator=(MyClass&&)

Or: Rule of Zero — don't define ANY of them, use smart pointers/containers
    and let the compiler generate them. This is the preferred approach.
```

---

## 4. Virtual Functions & Polymorphism

```cpp
class Shape {
public:
    virtual double area() const = 0;   // pure virtual → Shape is abstract
    virtual ~Shape() = default;        // virtual destructor — ALWAYS for base classes
};

class Circle : public Shape {
    double radius;
public:
    Circle(double r) : radius(r) {}
    double area() const override { return 3.14159 * radius * radius; }
};

// How vtable works:
//   Shape*  ──► vtable ptr ──► [ &Circle::area, &Circle::~Circle ]
//                                 ↑ looked up at runtime (dynamic dispatch)

Shape* s = new Circle(5.0);
s->area();  // calls Circle::area (virtual dispatch via vtable)
```

### Interview questions around virtual:

```
Q: Why must base class destructor be virtual?
A: Without it, deleting via base pointer only calls base destructor →
   derived class resources leaked.

Q: What's the overhead of virtual functions?
A: One pointer per object (vtable ptr, 8 bytes on 64-bit) +
   one indirect function call (pointer chase, cache miss possible).

Q: Can constructors be virtual?
A: No. The vtable isn't set up until after construction.
```

---

## 5. Templates & Compile-Time Polymorphism

```cpp
// Runtime polymorphism: virtual functions (vtable, indirect call)
// Compile-time polymorphism: templates (no overhead, code generated at compile)

template<typename T>
T max(T a, T b) {
    return (a > b) ? a : b;
}

max(3, 5);        // compiler generates: int max(int, int)
max(3.14, 2.71);  // compiler generates: double max(double, double)
// No vtable, no indirection — as fast as handwritten code
```

### CRTP (Curiously Recurring Template Pattern)

```cpp
// Static polymorphism — get "virtual" behavior without vtable overhead
template<typename Derived>
class Base {
public:
    void interface() {
        static_cast<Derived*>(this)->implementation();
    }
};

class Concrete : public Base<Concrete> {
public:
    void implementation() { /* ... */ }
};
// Zero overhead — compiler inlines everything
// Used in: Eigen (math library), LLVM, game engines
```

---

## 6. Concurrency

### Mutex & Lock

```cpp
std::mutex mtx;
int shared_counter = 0;

void increment() {
    std::lock_guard<std::mutex> lock(mtx);  // RAII lock
    shared_counter++;
}  // auto-unlock here, even if exception

// C++17: std::scoped_lock — locks MULTIPLE mutexes without deadlock
std::scoped_lock lock(mtx1, mtx2);  // deadlock-free (uses std::lock internally)
```

### Atomic Operations

```cpp
std::atomic<int> counter{0};

counter.fetch_add(1);                  // atomic increment (no mutex needed)
counter.compare_exchange_weak(expected, desired);  // CAS operation

// Memory ordering (from weakest to strongest):
//   relaxed     → no ordering guarantees, just atomic
//   acquire     → subsequent reads see writes before the release
//   release     → preceding writes visible to acquire readers
//   seq_cst     → total order (default, safest, slowest)
```

### Condition Variables

```cpp
std::mutex mtx;
std::condition_variable cv;
std::queue<int> queue;

// Producer
{
    std::lock_guard lock(mtx);
    queue.push(42);
    cv.notify_one();       // wake one waiting consumer
}

// Consumer
{
    std::unique_lock lock(mtx);
    cv.wait(lock, [&]{ return !queue.empty(); });  // sleep until data
    int val = queue.front();
    queue.pop();
}
```

---

## 7. STL Containers — Complexity Cheat Sheet

```
┌──────────────────┬───────────┬───────────┬───────────┬─────────────────────┐
│ Container        │ Access    │ Insert    │ Find      │ Implementation       │
├──────────────────┼───────────┼───────────┼───────────┼─────────────────────┤
│ vector           │ O(1)      │ O(1)*     │ O(n)      │ Dynamic array        │
│ deque            │ O(1)      │ O(1)*     │ O(n)      │ Array of arrays      │
│ list             │ O(n)      │ O(1)      │ O(n)      │ Doubly linked list   │
│ set/map          │ O(log n)  │ O(log n)  │ O(log n)  │ Red-black tree       │
│ unordered_set/map│ O(1)      │ O(1)      │ O(1)      │ Hash table           │
│ priority_queue   │ O(1) top  │ O(log n)  │ N/A       │ Binary heap          │
│ array            │ O(1)      │ N/A       │ O(n)      │ Fixed-size array     │
└──────────────────┴───────────┴───────────┴───────────┴─────────────────────┘
* amortized — occasional reallocation is O(n)

Interview tip: "Which container?" depends on:
  - Need random access? → vector
  - Need sorted order?  → set/map
  - Need fast lookup?   → unordered_set/unordered_map
  - Need FIFO?          → queue (or deque)
  - Need priority?      → priority_queue
```

---

## 8. Common Interview Questions

### Memory Questions
```
Q: What happens when you call `new`?
A: 1. operator new() calls malloc() → gets raw memory from heap
   2. Constructor runs on that memory (placement new)
   Return: pointer to constructed object

Q: What's a memory leak? How to detect?
A: Allocated memory never freed. Tools: Valgrind, AddressSanitizer (ASan),
   LeakSanitizer. Prevention: RAII + smart pointers.

Q: What's a dangling pointer?
A: Pointer to freed memory. Use-after-free → undefined behavior.
   Prevention: smart pointers, never return references to locals.

Q: Stack overflow — when and why?
A: Deep recursion or large stack allocations exhaust the stack (~1-8MB).
   Fix: use iteration, or allocate large objects on heap.
```

### Language Feature Questions
```
Q: What's the difference between struct and class?
A: Only default access: struct=public, class=private. That's it.

Q: What does `const` mean in different positions?
A: const int* p;       → can't change *p (pointed-to value)
   int* const p;       → can't change p itself (pointer)
   const int* const p; → can't change either
   void foo() const;   → method doesn't modify the object

Q: What's std::move actually do?
A: Nothing at runtime! It's a cast: lvalue → rvalue reference.
   Tells the compiler "I'm done with this, you can steal its guts."

Q: What's undefined behavior?
A: Code where the C++ standard says "anything can happen."
   Examples: null deref, signed overflow, use-after-free, data race.
   Compiler ASSUMES UB never happens → can optimize based on that.
```

---

## 9. Modern C++ Features to Know

```
C++11:  auto, move semantics, smart pointers, lambdas, range-for,
        constexpr, nullptr, enum class, threads, atomic, chrono

C++14:  generic lambdas, make_unique, relaxed constexpr

C++17:  optional, variant, string_view, structured bindings,
        if constexpr, filesystem, parallel algorithms, scoped_lock

C++20:  concepts, ranges, coroutines, modules, three-way comparison (<=>,
        consteval, constinit, jthread, span

C++23:  expected, flat_map/flat_set, stacktrace, print/println
```

### Lambdas (know this cold)

```cpp
// Capture nothing
auto add = [](int a, int b) { return a + b; };

// Capture by value (copy)
int x = 10;
auto f = [x]() { return x * 2; };

// Capture by reference
auto g = [&x]() { x += 1; };

// Capture all by value / reference
auto h = [=]() { ... };   // all by value
auto k = [&]() { ... };   // all by reference

// Mutable lambda (modify captured copy)
auto m = [x]() mutable { x++; return x; };
```

---

## 10. C++ vs Rust (for comparison)

```
┌──────────────────┬──────────────────────┬──────────────────────┐
│                  │ C++                  │ Rust                 │
├──────────────────┼──────────────────────┼──────────────────────┤
│ Memory safety    │ Manual (UB possible) │ Compile-time (borrow │
│                  │                      │ checker)             │
│ Null             │ nullptr (crashes)    │ Option<T> (checked)  │
│ Error handling   │ Exceptions           │ Result<T, E>         │
│ Ownership        │ Convention (RAII)    │ Enforced by compiler │
│ Inheritance      │ Yes (virtual)        │ No (traits instead)  │
│ Header files     │ Yes (.h + .cpp)      │ No (modules)         │
│ Build system     │ CMake/Bazel/etc.     │ Cargo (built-in)     │
│ Compile time     │ Slow (templates)     │ Slow (generics)      │
│ Ecosystem        │ Massive, 40+ years   │ Growing fast         │
│ Used at          │ Google, Meta, MS,    │ AWS, Cloudflare,     │
│                  │ HFT, games, Chrome   │ Discord, Linux kernel│
└──────────────────┴──────────────────────┴──────────────────────┘
```
