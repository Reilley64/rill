# How GC-free functional languages manage memory

Research for [issue #3](https://github.com/Reilley64/rill/issues/3), part of the [v0 spec map](https://github.com/Reilley64/rill/issues/1).

Rill has immutable data, no tracing GC, and no manual memory management. This note looks at six languages that work under similar constraints. For each one it records:

- whether the programmer sees ownership in the source,
- how in-place updates of uniquely referenced data work,
- how closures are handled,
- what it costs to share data across threads.

I read all sources on 2026-10-21. Links to source code point at the commit I read.

## Summary

| Language | Ownership in source | In-place update of unique data | Closures | Sharing across threads |
| --- | --- | --- | --- | --- |
| Koka | Hidden. Optional `^` borrow mark and `fip`/`fbip` checks. | Reuse analysis: a matched constructor with count 1 is reused for a new one of the same size. | Heap object that owns its free variables. | Values are marked thread-shared when they cross; shared counts use atomics and are never reused. |
| Lean 4 | Hidden. `@&` borrow only on `extern` functions. | Reset/reuse for constructors; primitive `Array` and `String` ops update in place when the count is 1. | Heap closure object with captured values. | `markMT` walks the value before a task starts; shared objects use atomic counts and are never treated as unique. |
| Roc | Hidden. | Builtins (for example `List.set`) update in place when the count is 1, else shallow clone. | Defunctionalized into tag unions (lambda sets); no heap allocation, except across the platform boundary. | Roc has no threads of its own; values the host can see use atomic counts. |
| Swift | Hidden by default. Optional `borrowing`, `consuming`, `consume`, `~Copyable`, `weak`, `unowned`. | Value types mutate in place; library types use copy-on-write with `isKnownUniquelyReferenced`. | Reference type; heap context when escaping, stack context when not. | Counts are stored atomically in every heap object. `Sendable` and region checks are compile-time only. |
| Rust | Visible: moves, `&`, `&mut`, lifetimes. | Owned or `&mut` data is mutated directly; `Rc::make_mut` / `Arc::make_mut` for shared data. | Unboxed anonymous struct per closure; `Fn`/`FnMut`/`FnOnce`; `Box<dyn Fn>` for dynamic. | `Send`/`Sync` checked at compile time. `Rc` cannot cross threads; `Arc` uses atomics. Scoped threads can borrow. |
| Hylo | Visible: `let`/`inout`/`sink`/`set` conventions, explicit `.copy()`. | `inout` gives exclusive access, so updates are always in place; no reference counts in the model. | Lambda types carry their environment; closures may capture projections; capture lists copy. | Design in progress; structured `spawn` passes projections; no counts to synchronize. |

The rest of the note gives the detail and sources.

## Koka: Perceus reference counting with reuse

Koka manages memory with Perceus, a compiler-optimized reference counting scheme. It compiles to C with no garbage collector or runtime system ([Koka book, Perceus](https://koka-lang.github.io/koka/doc/book.html#why-perceus)). Perceus was published at PLDI 2021 ([paper PDF](https://xnning.github.io/papers/perceus.pdf), [DOI](https://doi.org/10.1145/3453483.3454032)).

### Ownership in the source

The programmer does not write ownership. The compiler inserts `dup` operations, which increment a count, and `drop` operations, which decrement one, so that, for cycle-free programs, only live references are kept. The paper calls this "garbage free" ([Perceus paper, abstract](https://xnning.github.io/papers/perceus.pdf)). Inside the compiler the analysis splits variables into an owned environment and a borrowed environment. Borrowing lets a `dup` happen as late as possible ([Perceus paper, section 3.2](https://xnning.github.io/papers/perceus.pdf)).

Two opt-in source features expose part of this:

- A parameter can be marked borrowed with `^` ([Koka book, grammar](https://koka-lang.github.io/koka/doc/book.html#sec-value-and-function-declarations)). Borrowing annotations shipped in Koka v2.2.0 ([Koka book, news](https://koka-lang.github.io/koka/doc/book.html)).
- The `fip` and `fbip` keywords, added in Koka v2.4.2, mark functions that must run fully in place ([Koka book, news](https://koka-lang.github.io/koka/doc/book.html)). The FP² paper proves that FIP functions need no allocation or deallocation and use constant stack space. The Koka implementation checks this with precise reference counting at run time, not with uniqueness types ([FP² abstract](https://icfp23.sigplan.org/details/icfp-2023-artifact-evaluation/18/FP-2-Fully-in-Place-Functional-Programming), [DOI](https://doi.org/10.1145/3607840)).

### In-place updates

Reuse analysis pairs each pattern match with a constructor of the same size allocated in the same branch. If the matched object has a reference count of one, its memory is reused for the new constructor. Otherwise the count is decremented and a fresh object is allocated ([Perceus paper, section 2.4 and figure 1](https://xnning.github.io/papers/perceus.pdf)). For `map` over an unshared list, this means the list is updated in place with no extra allocation ([Koka book, reuse analysis](https://koka-lang.github.io/koka/doc/book.html#why-fbip)). The book says the reuse optimization "is guaranteed and a programmer can see when the optimization applies", and names the resulting style FBIP, "functional but in-place" ([Koka book, reuse analysis](https://koka-lang.github.io/koka/doc/book.html#why-fbip)).

In the runtime, a count of 0 means unique, and `kk_block_is_unique` tests for exactly that ([kklib.h](https://github.com/koka-lang/koka/blob/198fc3bbd158246e7604210ee6692ee72c2d9c50/kklib/include/kklib.h)).

### Closures

A lambda is allocated as a closure that holds all of its free variables, so creating the closure consumes those variables and the closure owns them. Its body is checked with an empty borrowed environment and owns only its argument and its captured variables ([Perceus paper, section 3.2](https://xnning.github.io/papers/perceus.pdf)).

### Sharing across threads

Every object carries a thread-shared flag. All objects start unshared. Starting a thread marks its argument, and everything reachable from it, as thread-shared. Setting a thread-shared mutable reference is the only other way to share. The marking walk is linear but happens at most once per object, because shared objects never become unshared ([Perceus paper, section 2.7.2](https://xnning.github.io/papers/perceus.pdf)).

Thread-shared objects store a negative count, so one inlined test, `rc <= 1`, sends both "shared" and "about to be freed" to the slow path, and the common unshared case avoids atomics ([Perceus paper, section 2.7.2](https://xnning.github.io/papers/perceus.pdf); [kklib.h](https://github.com/koka-lang/koka/blob/198fc3bbd158246e7604210ee6692ee72c2d9c50/kklib/include/kklib.h)). Because unique means a count of exactly 0, a shared object is never unique and never reused in place ([kklib.h](https://github.com/koka-lang/koka/blob/198fc3bbd158246e7604210ee6692ee72c2d9c50/kklib/include/kklib.h)). The paper cites a report of slowdowns up to 50% when all count operations are atomic, and notes that Swift has to assume almost every reference may be thread-shared ([Perceus paper, section 2.7.2](https://xnning.github.io/papers/perceus.pdf)).

### Cycles

Koka has no cycle collector. Inductive data types and eager evaluation prevent cycles outside explicit mutable references, and it is statically known where cycles can appear ([Perceus paper, section 1](https://xnning.github.io/papers/perceus.pdf); [Koka book](https://koka-lang.github.io/koka/doc/book.html#why-perceus)).

## Lean 4: reference counting tuned for pure code

Ullrich and de Moura describe Lean's scheme in "Counting Immutable Beans", published at IFL 2019 ([arXiv 1908.05647](https://arxiv.org/abs/1908.05647)). Perceus builds on it ([Perceus paper, section 1](https://xnning.github.io/papers/perceus.pdf)).

### Ownership in the source

Ordinary Lean code has no ownership syntax. The compiler infers, for each function, which parameters are owned and which are borrowed ([Counting Immutable Beans, section 5.2](https://arxiv.org/abs/1908.05647)). The heuristic makes a parameter owned if it, or one of its projections, is reused or passed to a function that takes it owned. The trade-off is that borrowing removes count updates but blocks reuse of that parameter's memory ([Counting Immutable Beans, section 5.2](https://arxiv.org/abs/1908.05647)).

The `@&` borrow marker exists, but only for `@[extern]` functions. It has no effect on the type system or on other functions ([Lean reference, FFI borrowing](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Foreign-Function-Interface/)).

### In-place updates

The compiler inserts `reset`/`reuse` pairs so that a constructor's cell is reused for a new constructor when the old one is no longer referenced ([Counting Immutable Beans, section 5.1](https://arxiv.org/abs/1908.05647)). The reference manual describes two effects. Traversals such as `List.map` need no allocation when there is one reference. Primitive types such as `Array` and `String` copy shared data but modify unshared data in place ([Lean reference, reference counting](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Reference-Counting/)). The manual offers `dbgTraceIfShared` to check at run time whether a value is aliased, because "ensuring that arrays and strings are uniquely referenced is key to writing fast code in Lean" ([Lean reference, observing uniqueness](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Reference-Counting/)).

### Closures

A closure is a heap object of type `lean_closure_object` that holds the function pointer, its arity, and the captured or already-applied arguments ([lean.h](https://github.com/leanprover/lean4/blob/9b72c4060097680d373bf0468c868c86947a70fd/src/include/lean/lean.h)). Partially applying a function with borrowed parameters is a problem because the result may escape. The compiler wraps such functions in an all-owned wrapper ([Counting Immutable Beans, section 5.2](https://arxiv.org/abs/1908.05647)).

### Sharing across threads

Values are tagged single-threaded, multi-threaded, or persistent. Single-threaded values use no memory fences. Before a task is created, `markMT` tags everything reachable from the closure and its inputs as multi-threaded while only one thread can still see them ([Counting Immutable Beans, section 7.2](https://arxiv.org/abs/1908.05647)). The authors made this change after seeing that fences had "a significant performance impact even when only one thread is being executed" ([Counting Immutable Beans, section 7.2](https://arxiv.org/abs/1908.05647)).

In current `lean.h`, the sign of `m_rc` stores the tag: positive is single-threaded, negative is multi-threaded, and zero means no counting ([lean.h](https://github.com/leanprover/lean4/blob/9b72c4060097680d373bf0468c868c86947a70fd/src/include/lean/lean.h)). `lean_is_exclusive` returns false for any object that is not single-threaded ([lean.h](https://github.com/leanprover/lean4/blob/9b72c4060097680d373bf0468c868c86947a70fd/src/include/lean/lean.h)). So, as in Koka, data that has crossed to another task never gets in-place updates again.

### Cycles

The verifiable fragment of Lean cannot create cyclic data, so the runtime has no cycle detection ([Lean reference, reference counting](https://lean-lang.org/doc/reference/latest/Run-Time-Code/Reference-Counting/)).

## Roc: Perceus-style counting, defunctionalized closures

Roc's compiler is being rewritten in Zig. The links below point at the `main` branch of that rewrite.

### Ownership in the source

None. Strings, lists, boxes, and recursive tag unions live on the heap, and Roc counts references to them automatically. Numbers, records, tuples, and non-recursive tag unions are stored inline and have no count ([Roc langref, reference counting](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/docs/langref/expressions.md)). The compiler infers when a function only reads an argument, and then passing it does not touch the count ([Roc langref, functions performance](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/docs/langref/functions.md)).

### In-place updates

Roc calls this "opportunistic mutation" and credits Perceus. Builtin operations update a value in place when its count is 1. Otherwise they make a shallow clone and update the clone ([Roc langref, opportunistic mutation](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/docs/langref/expressions.md)). The docs show the cost model: `list.append(42)` is in place only if `list` is not used afterwards, so a later `List.len(list)` forces a copy. They tell users not to hold on to old versions inside loops ([Roc langref, opportunistic mutation](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/docs/langref/expressions.md)). Because borrowed arguments keep their count, a list passed to a read-only function stays unique ([Roc langref, functions performance](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/docs/langref/functions.md)). Constants evaluated at compile time are never freed, so updating one always copies first ([Roc langref, opportunistic mutation](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/docs/langref/expressions.md)).

The runtime check is `isUnique`, which tests for a count of 1 ([utils.zig](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/src/builtins/utils.zig)). In debug builds, a list write that the compiler proved safe is checked against the real count, to catch a wrong proof ([list.zig](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/src/builtins/list.zig)).

### Closures

Roc uses lambda set specialization, a form of defunctionalization. By code generation time a closure is a tagged union: the tag names the procedure, the payload holds the captures, and a call becomes a `match` on the tag ([lambda set design doc](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/design/lambda-set-specialization.md)). The user docs list the effects. Function values need no heap allocation. A value that is always one known function with no captures takes no space. The compiler can inline every target ([Roc langref, function values](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/docs/langref/functions.md)). The exception is functions that cross into the separately compiled platform, which are boxed as a heap allocation with a function pointer and captures ([Roc langref, function values](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/docs/langref/functions.md)).

### Sharing across threads

Roc code does not start threads; the platform host may share values between threads. Counts on values the host can see are updated atomically. When the compiler can tell a value never reaches the host, it uses plain updates ([Roc langref, reference counting](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/docs/langref/expressions.md)). The runtime defaults to atomic, and callers pass `single_thread` only for allocations the compiler proved no other thread can touch ([utils.zig](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/src/builtins/utils.zig)).

### Cycles and allocation

Roc has no way to express reference cycles, so it needs no cycle collector or weak references ([Roc langref, reference cycles](https://github.com/roc-lang/roc/blob/534956997e8e5c62888bc14cbc763f60109d0b72/docs/langref/expressions.md)). The host supplies `malloc` and `free` ([Roc platforms](https://www.roc-lang.org/platforms)). That lets a platform choose a strategy: skip frees in a short script, or put each web request in an arena that is reset after the response ([Roc, fast](https://www.roc-lang.org/fast)).

## Swift: automatic reference counting

### Ownership in the source

By default the programmer does not manage memory: ARC frees class instances when they are no longer needed ([Swift book, ARC](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/automaticreferencecounting/)). Reference counting applies only to class instances. Structs and enums are value types ([Swift book, ARC](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/automaticreferencecounting/)). Ownership becomes visible in a few places:

- `weak` and `unowned` references break strong reference cycles, which ARC cannot free on its own ([Swift book, ARC](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/automaticreferencecounting/)).
- `borrowing` and `consuming` parameter modifiers pick the calling convention, to cut ARC calls or copies. A parameter marked this way is no longer implicitly copyable and needs `copy x` ([SE-0377](https://github.com/swiftlang/swift-evolution/blob/58a9986b616a427a9cf6584219cb6cb1a12cc1fe/proposals/0377-parameter-ownership-modifiers.md)).
- The `consume` operator ends a binding's lifetime ([SE-0366](https://github.com/swiftlang/swift-evolution/blob/58a9986b616a427a9cf6584219cb6cb1a12cc1fe/proposals/0366-move-function.md)), and noncopyable `~Copyable` structs and enums give unique ownership ([SE-0390](https://github.com/swiftlang/swift-evolution/blob/58a9986b616a427a9cf6584219cb6cb1a12cc1fe/proposals/0390-noncopyable-structs-and-enums.md)).

### In-place updates

Standard library containers are value types that use copy-on-write. The buffer is copied only if its count is greater than 1 when it is mutated ([OptimizationTips.rst](https://github.com/swiftlang/swift/blob/9ee233505592ca67c607d3800a06e0874390d670/docs/OptimizationTips.rst)). Library authors build this with `isKnownUniquelyReferenced`, which returns true when an object has a single strong reference ([ManagedBuffer.swift](https://github.com/swiftlang/swift/blob/9ee233505592ca67c607d3800a06e0874390d670/stdlib/public/core/ManagedBuffer.swift)). Its docs warn that it can return true while another thread is accessing the object, so it must only be called with proper synchronization ([ManagedBuffer.swift](https://github.com/swiftlang/swift/blob/9ee233505592ca67c607d3800a06e0874390d670/stdlib/public/core/ManagedBuffer.swift)).

### Closures

Closures are reference types: two variables assigned the same closure refer to the same closure ([Swift book, closures](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/closures/)). A closure captures variables, not just values, and can modify them after the defining scope ends ([Swift book, closures](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/closures/)). In SIL, a non-escaping closure gets its context on the stack and does not retain the captured values. An escaping closure gets a heap context with a count of 1 that owns its captures ([SIL instructions, partial_apply](https://github.com/swiftlang/swift/blob/9ee233505592ca67c607d3800a06e0874390d670/docs/SIL/Instructions.md)). A closure stored on a class instance that captures `self` forms a cycle, which capture lists break ([Swift book, ARC](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/automaticreferencecounting/)).

### Sharing across threads

The runtime stores counts in an `atomic<InlineRefCountBits>` field in each heap object ([RefCount.h](https://github.com/swiftlang/swift/blob/9ee233505592ca67c607d3800a06e0874390d670/stdlib/public/SwiftShims/swift/shims/RefCount.h)). The Perceus authors note that, with unrestricted threads, Swift must treat almost every count operation as possibly thread-shared ([Perceus paper, section 2.7.2](https://xnning.github.io/papers/perceus.pdf)). Safety across threads is a compile-time check: `Sendable` marks types that are safe to pass between concurrency domains ([SE-0302](https://github.com/swiftlang/swift-evolution/blob/58a9986b616a427a9cf6584219cb6cb1a12cc1fe/proposals/0302-concurrent-value-and-concurrent-closures.md)). Region-based isolation and `sending` let a non-`Sendable` value move to another domain when the caller provably stops using it ([SE-0414](https://github.com/swiftlang/swift-evolution/blob/58a9986b616a427a9cf6584219cb6cb1a12cc1fe/proposals/0414-region-based-isolation.md), [SE-0430](https://github.com/swiftlang/swift-evolution/blob/58a9986b616a427a9cf6584219cb6cb1a12cc1fe/proposals/0430-transferring-parameters-and-results.md)). SE-0302 notes that copy-on-write collections can be sent without copying their storage ([SE-0302](https://github.com/swiftlang/swift-evolution/blob/58a9986b616a427a9cf6584219cb6cb1a12cc1fe/proposals/0302-concurrent-value-and-concurrent-closures.md)).

## Rust: ownership and borrowing

### Ownership in the source

Ownership is the core of the language. Each value has one owner, and the value is dropped when the owner goes out of scope ([Rust book, ownership](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html)). Moves, shared borrows with `&`, mutable borrows with `&mut`, and lifetimes appear in source, and the compiler checks them. Shared ownership is opt-in through `Rc` and `Arc` ([Rust book, Rc](https://doc.rust-lang.org/book/ch15-04-rc.html)). Leaks are memory safe in Rust, and `Rc` plus `RefCell` can leak through cycles ([Rust book, reference cycles](https://doc.rust-lang.org/book/ch15-06-reference-cycles.html)).

### In-place updates

An owner or a `&mut` borrower updates data directly, since the borrow checker guarantees no other access. For shared data, `Rc::make_mut` clones the value only if other `Rc` pointers exist, which the docs call clone-on-write, and `Rc::get_mut` returns a mutable reference only when the pointer is unique ([std Rc](https://doc.rust-lang.org/std/rc/struct.Rc.html)). `Arc` has the same pair ([std Arc](https://doc.rust-lang.org/std/sync/struct.Arc.html)). Unlike Koka, Lean, and Roc, this is a library call the programmer writes, not a compiler optimization.

### Closures

Each closure has its own anonymous type, roughly a struct holding the captured values ([Rust reference, closure types](https://doc.rust-lang.org/reference/types/closure.html)). So a closure needs no heap allocation unless the programmer boxes it. Closures with different bodies have different types, and the book stores them together as `Box<dyn Fn>` trait objects ([Rust book, advanced functions and closures](https://doc.rust-lang.org/book/ch20-04-advanced-functions-and-closures.html)). The traits `FnOnce`, `FnMut`, and `Fn` are implemented automatically based on whether the body moves, mutates, or only reads its captures ([Rust book, closures](https://doc.rust-lang.org/book/ch13-01-closures.html)). The `move` keyword forces capture by value ([Rust book, closures](https://doc.rust-lang.org/book/ch13-01-closures.html)).

### Sharing across threads

`Send` and `Sync` are checked at compile time. `Rc` is neither, because its count is shared and unsynchronized ([Rustonomicon, Send and Sync](https://doc.rust-lang.org/nomicon/send-and-sync.html)). `Arc` uses atomic counts, and the docs note that atomic operations cost more than plain memory accesses ([std Arc](https://doc.rust-lang.org/std/sync/struct.Arc.html)). The programmer picks `Rc` or `Arc` per value. `thread::scope` lets threads borrow non-`'static` data with no counting, because all scoped threads are joined before the scope returns ([std thread::scope](https://doc.rust-lang.org/std/thread/fn.scope.html)).

## Hylo: mutable value semantics

Hylo is still in development; its concurrency design in particular is marked as subject to change ([Hylo tour, concurrency](https://docs.hylo-lang.org/language-tour/concurrency)).

### Ownership in the source

Every parameter has one of four passing conventions: `let`, `inout`, `sink`, `set` ([Hylo tour, functions](https://docs.hylo-lang.org/language-tour/functions-and-methods)).

- `let` is like an immutable borrow, with a static guarantee that nothing else can change the value during the call.
- `inout` is like a mutable borrow. The docs suggest thinking of it as "move-in/move-out".
- `sink` transfers ownership, and the argument is no longer usable in the caller.
- `set` lets a callee initialize an uninitialized value.

`let` and `inout` parameters cannot escape the callee without an explicit copy. Returning a `let` parameter is an error that asks for `v.copy()` ([Hylo tour, functions](https://docs.hylo-lang.org/language-tour/functions-and-methods)). An `inout` binding is a projection that has full ownership of the projected value while it lasts, not a reference in the usual sense ([Hylo tour, bindings](https://docs.hylo-lang.org/language-tour/bindings)). The design paper describes the strict form of mutable value semantics: references are second-class, created only implicitly at function boundaries, and cannot be stored, so variables never share mutable state ([JOT 2022 paper, abstract](https://www.jot.fm/issues/issue_2022_02/article2.pdf)).

### In-place updates

Hylo has mutation, but only through exclusive access, so updates are always in place and are part-wise ([JOT 2022 paper, abstract](https://www.jot.fm/issues/issue_2022_02/article2.pdf)). The compiler rejects overlapping `inout` access, such as passing `v` both `inout` and as another argument ([Hylo tour, functions](https://docs.hylo-lang.org/language-tour/functions-and-methods)). The JOT paper studies a Swift-like core language in which dynamically sized containers use copy-on-write with a reference counter ([JOT 2022 paper, sections 5.1.2 and 6.3](https://www.jot.fm/issues/issue_2022_02/article2.pdf)). Hylo's own standard library `Array` currently does not: `Array.copy()` allocates new storage and copies each element ([Array.hylo](https://github.com/hylo-lang/hylo/blob/6889158246df571baa9079115817ab2bff61156b/StandardLibrary/Sources/Array.hylo)).

### Closures

A lambda expression may have a capture list ([Hylo spec, lambda expressions](https://github.com/hylo-lang/specification/blob/6f4c645101bac80dbb7bd5833cf07c8cbf699bd1/spec.md)). A lambda type records its environment as `thin` or `[E]` ([Hylo spec, lambda type expressions](https://github.com/hylo-lang/specification/blob/6f4c645101bac80dbb7bd5833cf07c8cbf699bd1/spec.md)). An object, including a closure, can capture a projection when it is created, and the projection is released when that object dies ([Hylo spec, projections](https://github.com/hylo-lang/specification/blob/6f4c645101bac80dbb7bd5833cf07c8cbf699bd1/spec.md)). In the JOT core language, captures must be declared, are copied, and are immutable. The paper contrasts this with Swift, whose implicit by-reference captures bring back reference semantics ([JOT 2022 paper, section 3](https://www.jot.fm/issues/issue_2022_02/article2.pdf)).

### Sharing across threads

There are no reference counts in the core model, so there is nothing to synchronize. The JOT paper only covers single-threaded execution and notes that mutable value semantics is immune to data races ([JOT 2022 paper, section 9](https://www.jot.fm/issues/issue_2022_02/article2.pdf)). The planned concurrency model has no function colouring and is structured. `spawn` returns a handle. `await` joins the work, and dropping the handle asks the work to stop. The sort example passes an `inout` slice into a spawned closure ([Hylo tour, concurrency](https://docs.hylo-lang.org/language-tour/concurrency)). It uses "thread inversion": `await` does not block an OS thread, and the function continues on whichever thread finishes last ([Hylo tour, concurrency](https://docs.hylo-lang.org/language-tour/concurrency)).

## Implications for Rill

These are the questions the follow-up grilling ticket needs to settle. This note does not pick answers.

### 1. Hidden ownership or visible ownership

- **Hidden, with reference counting, as in Koka, Lean, and Roc.** This fits immutable data and keeps the source free of ownership syntax. The costs are a count in every heap object, count traffic that the compiler must work to remove, and performance that depends on whether a value happens to be shared. Roc's docs tell users not to keep old versions alive. Lean ships a debug helper to find aliasing.
- **Visible, with static ownership, as in Rust and Hylo.** Most values need no count, and cost is predictable. The cost is syntax and checking that a functional-first language with TypeScript-style syntax may not want. Rill also has no mutation, so `inout` or `&mut` would exist only as an optimization.
- **Hidden by default, with opt-in annotations.** Examples are Koka's `^` and `fip`, Swift's `borrowing` and `consuming`, and Lean's `@&` for FFI. This keeps the common case simple and gives experts a tool. It still adds syntax that the spec must define.

### 2. How in-place updates are guaranteed or reported

- Leave it as a pure optimization, as Lean and Roc do.
- Guarantee reuse for the patterns the compiler documents, so it is predictable. Koka's reuse analysis does this.
- Add a checked annotation that fails when a function cannot run in place, like Koka's `fip` and `fbip`.
- Expose only library-level copy-on-write that the programmer calls explicitly, like Rust's `make_mut`.

### 3. Closure representation

- Heap-allocated, counted closure objects, as in Koka, Lean, and escaping Swift closures. This is simple, works with separate compilation, and costs one allocation per closure.
- Defunctionalization through lambda sets, as in Roc. There is no allocation and calls can inline. It needs whole-program knowledge, it interacts with Rill's generics and overloading, and it falls back to boxing at boundaries the compiler cannot see.
- A unique unboxed type per closure, as in Rust. It is fast, but the type system needs some way to talk about "a callable", and Rill has no traits.
- Stack contexts for non-escaping closures, as in Swift. This would need an escaping/non-escaping distinction.

### 4. Counts and goroutines

With `go f(x)` and Go-style suspension, more than one OS thread can see a value. The languages above show these options:

- **Always atomic, as in Swift and Roc's default.** This is the simplest option. Every count update pays for an atomic operation, which the Perceus authors and the Lean authors both report as a measurable cost.
- **Mark on share, as in Koka and Lean.** Mark values as shared when they cross to another goroutine, at `go` and probably at channel send, and use atomics only for marked values. Marking costs a walk of the value, once per object. A shared object never counts as unique again, so it loses in-place updates for good.
- **Prove non-escape, as Roc does.** Use plain counts only where the compiler proves the value stays on one thread. That needs a definition of "one thread" that holds up when a suspended goroutine resumes on a different OS thread.
- **Structured sharing with no counts, as in Hylo and Rust's `thread::scope`.** This depends on spawned work being joined in scope. Go-style `go f(x)` is unstructured, so this would need a change to that model.

### 5. Cycles

Koka, Lean, and Roc avoid a cycle collector because immutable, eager data cannot form cycles. Rill must check whether any of its features can create a cycle. Candidates are recursive closures, recursive `const` bindings, channels, and any mutable cell in the standard library. If one can, Rill needs weak references as Swift has, or must accept leaks as Rust does.

### 6. Allocation policy

Roc lets the host supply the allocator, so a program can use an arena per request or skip frees in a short script. The grilling ticket can decide whether the runtime fixes the allocator or the program can replace it. The emitted LLVM IR calls the allocator either way, so the choice sets which symbol it calls.
