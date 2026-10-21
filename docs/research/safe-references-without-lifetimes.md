# How languages make references and mutation safe without lifetime annotations

Research for [issue #14](https://github.com/Reilley64/rill/issues/14), part of the [v0 spec map](https://github.com/Reilley64/rill/issues/1). It feeds the [Rill mutation](https://github.com/Reilley64/rill/issues/15) grilling ticket.

[Rill memory model](https://github.com/Reilley64/rill/issues/7) and [ADR 0002](https://github.com/Reilley64/rill/blob/main/docs/adr/0002-visible-ownership-without-lifetimes.md) already let any value hold references, with no lifetime syntax. Two rules make that work:

1. A value that holds a reference is itself a borrow.
2. A returned reference borrows from every `&` parameter.

Both rules were written before mutation was in scope. This note looks at how other languages combine references, mutation, and memory safety without asking the programmer to write lifetimes, or with as few as possible. For each language it records:

- the rules for mutable vs shared access,
- whether references can be stored inside values,
- what a returned reference is assumed to borrow from,
- what is proven sound,
- which common programs the rules reject.

It ends with the risks for Rill's two rules and the options the mutation ticket must decide.

I read all sources on 2026-10-21. Links to source code point at the commit or tag I read where the page allowed it. Paragraphs marked "Analysis" are my reading, not claims from a source. Mutable borrow syntax in Rill is not decided yet; this note writes `&mut T` only as a placeholder.

## Summary

| Language | Shared vs mutable access | References inside values | What a returned reference borrows from | Proven sound |
| --- | --- | --- | --- | --- |
| Rust | One `&mut` or many `&`, checked statically. `RefCell` moves the check to run time. | Yes, but every such type declares a lifetime parameter (`struct S<'a>`). | Elision: the single input lifetime, or `&self`. Anything else needs named lifetimes. | Yes for the λRust model (RustBelt, Coq). Not for rustc's checker itself. |
| Mojo | Argument exclusivity at each call, plus flow-sensitive invalidation of references into containers (experimental). | Only through `Pointer`/`Span`, and the struct must declare an `origin` parameter. | Must be written: `-> ref[a, b] T`. No default. | No. Modular lists known holes. |
| Hylo | Law of Exclusivity at call sites by path disjointness. No first-class references. | Planned `remote` parts, still research. Values holding them cannot escape the local scope. | Only subscripts project, and a projection depends on every argument. Functions cannot return projections. | Only the Swiftlet core (no projections, no stored references), on paper. |
| Swift | Law of Exclusivity: static for locals and value types, dynamic traps for class properties, globals, and escaping captures. | Classic Swift: no. Since 6.2, `~Escapable` types can hold borrows, and the container becomes non-escapable too. | `@lifetime` annotations (still experimental), with defaults for one parameter or same-typed parameters. | No formal proof found. |
| Vale | No aliasing restriction. Generational references are checked at each dereference and halt on a stale one. | Yes, freely. Regions add optional static immutability. | Nothing tracked statically in the default mode. | No. Detection is probabilistic. |
| Austral | Linear types. A mutable borrow is exclusive within a statement or a `borrow` block. | Yes, but the record takes an explicit region type parameter. | Whatever region the signature names. No inference. | No formal proof found. |
| OxCaml | `local` values cannot escape their region. `unique` and `borrow_` ship; an exclusive mutable borrow is only planned. | A value containing a `local` value is `local` (deep). Mutable fields must hold global values. | `a @ local -> b @ local`: the result is local to the caller's region. | The mode calculus, on paper (ICFP 2024) and in Rocq for data-race freedom (POPL 2025). |

OxCaml was not on the ticket's list. It is here because its locality mode is the closest shipped match to Rill's two rules.

## Rust: lifetimes with elision

### Shared and mutable access

The Book states the rule: "At any given time, you can have either one mutable reference or any number of immutable references. References must always be valid" ([Book 4.2](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html)). Breaking it is a compile error such as E0499 or E0502 ([Nomicon, limits of lifetimes](https://doc.rust-lang.org/nomicon/lifetime-mismatch.html)). `RefCell<T>` enforces the same invariants at run time and panics on a violation ([Book 15.5](https://doc.rust-lang.org/book/ch15-05-interior-mutability.html)).

Two details matter for Rill:

- **Two-phase borrows.** `vec.push(vec.len())` is accepted because an implicit `&mut` (an autoref, a reborrow in an argument, or the `&mut` in `x += x`) first acts as a shared borrow and only becomes mutable when it is used. An explicit `&mut` in source is never two-phase ([rustc-dev-guide](https://rustc-dev-guide.rust-lang.org/borrow-check/two-phase-borrows.html)).
- **Variance.** `&'a mut T` is covariant in `'a` but invariant in `T`. The Nomicon's example `fn assign<T>(input: &mut T, val: T) { *input = val; }` shows why: if `&mut &'static str` could be used as `&mut &'world str`, the function could store a short-lived `&world` into a `'static` slot, and rustc rejects the call with E0597 ([Nomicon, subtyping](https://doc.rust-lang.org/nomicon/subtyping.html)). This is the rule that stops an older value from pointing at a younger one through a mutable borrow.

### References inside values

Any struct, enum, or collection can hold references, but the type must declare a lifetime parameter, and every type that contains it must pass the parameter on. `struct Foo1 { x: &bool }` is error E0106, and so is `struct Bar1 { x: Foo2 }` when `Foo2` has a lifetime ([E0106](https://doc.rust-lang.org/error_codes/E0106.html)). The 1.31 release let impl headers write `'_` but kept the rule that "Lifetimes are still required to be defined in structs" ([RELEASES.md](https://github.com/rust-lang/rust/blob/master/RELEASES.md)). RFC 2115 calls it "a mistake" to leave the lifetime off a type such as `Iter<T>` in a signature, because "it makes it difficult to see at a glance that borrowing is occurring" ([RFC 2115](https://rust-lang.github.io/rfcs/2115-argument-lifetimes.html)). Rust 1.89 added the warn-by-default `mismatched_lifetime_syntaxes` lint for signatures that hide a lifetime in one place and show it in another ([lint listing](https://doc.rust-lang.org/rustc/lints/listing/warn-by-default.html#mismatched-lifetime-syntaxes)).

### What a returned reference borrows from

The Reference gives three elision rules ([Reference, lifetime elision](https://doc.rust-lang.org/reference/lifetime-elision.html)):

1. "Each elided lifetime in the parameters becomes a distinct lifetime parameter."
2. "If there is exactly one lifetime used in the parameters (elided or not), that lifetime is assigned to *all* elided output lifetimes."
3. "If the receiver has type `&Self` or `&mut Self`, then the lifetime of that reference to `Self` is assigned to all elided output lifetime parameters."

"It is an error to elide lifetime parameters that cannot be inferred." The Reference's own examples are `fn get_str() -> &str; // ILLEGAL` and `fn frob(s: &str, t: &str) -> &str; // ILLEGAL`, the second because it is "ambiguous if it is borrowed from the first or second parameter" ([Reference](https://doc.rust-lang.org/reference/lifetime-elision.html)). Elision does not apply to the annotated return type of a closure literal, so `|x: &i32| -> &i32 { x }` fails ([rust-lang/rust#58052](https://github.com/rust-lang/rust/issues/58052)).

A returned reference that borrows a `&mut` parameter keeps that parameter mutably borrowed for as long as the result lives. The Nomicon's `fn mutate_and_share(&mut self) -> &Self` example is rejected when the caller then calls a `&self` method while the result is alive. The Nomicon says the program "is clearly correct according to the reference semantics we actually care about, but the lifetime system is too coarse-grained to handle that" ([Nomicon, limits of lifetimes](https://doc.rust-lang.org/nomicon/lifetime-mismatch.html)).

### What is proven

RustBelt is "the first formal (and machine-checked) safety proof for a language representing a realistic subset of Rust", done in Coq with Iris ([RustBelt](https://plv.mpi-sws.org/rustbelt/popl18/)). It proves λRust, a MIR-like language whose syntax has explicit lifetimes, and it verifies unsafe libraries such as `Rc`, `RefCell`, `Arc`, and `Mutex`. It does not model trait objects, unwinding, automatic destruction, or full type polymorphism ([RustBelt paper](https://plv.mpi-sws.org/rustbelt/popl18/paper.pdf)). Stacked Borrows and Tree Borrows are proven aliasing models for unsafe code ([Stacked Borrows](https://plv.mpi-sws.org/rustbelt/stacked-borrows/), [Tree Borrows](https://plf.inf.ethz.ch/research/pldi25-tree-borrows.html)), but the Reference still says "The exact aliasing rules are not determined yet" ([Reference, undefined behavior](https://doc.rust-lang.org/reference/behavior-considered-undefined.html)). The 2026 Polonius goal plans "a formal model and specification of the borrow checking analysis", which means rustc's own checker has none yet ([Polonius goal](https://goals.rust-lang.org/2026/polonius.html)).

### Programs it rejects

- **Conditional return of a borrow** (NLL problem case #3). A `get_default` that returns `map.get_mut(&key)` on one path and inserts on the other is rejected, because the returned borrow must last into the caller on every path. The workarounds are a double lookup or the `entry` API, and "the limitation of the borrow checker here was one of the motivations for developing the entry API" ([RFC 2094](https://rust-lang.github.io/rfcs/2094-nll.html)). Polonius Alpha accepts it and is on nightly. It still rejects some linked-list walks with conditional reborrows ([Polonius on nightly](https://blog.rust-lang.org/2026/08/04/enabling-polonius-alpha-on-nightly/), [Polonius goal](https://goals.rust-lang.org/2026/polonius.html)).
- **Disjoint fields across functions.** `get_a(&self)` followed by `inc_b(&mut self)` while the first result is alive fails, because each call borrows all of `self`. The workaround is to call functions on the fields ([RFC 2094, appendix](https://rust-lang.github.io/rfcs/2094-nll.html)).
- **Self-referential structs.** Not expressible. The NLL RFC suggests "a vector with indices" or a library crate ([RFC 2094](https://rust-lang.github.io/rfcs/2094-nll.html)).
- **Several input references with a reference result.** `frob` above needs named lifetimes ([Reference](https://doc.rust-lang.org/reference/lifetime-elision.html)).

## Mojo: origins

Mojo 1.1.0 is the current release ([v1.1.0 notes](https://mojolang.org/releases/v1.1.0/)). Older sources say "lifetime" where newer ones say "origin", and `borrowed`/`read` and `inout` are now `imm` and `mut` ([v24.6 changelog](https://github.com/modular/modular/blob/modular/v26.2/mojo/docs/changelog/v0.24.6.md), [v1.0.0 notes](https://mojolang.org/releases/v1.0.0/)).

### Shared and mutable access

A parameter is an immutable reference by default, `mut` makes it a mutable reference, `var` takes ownership, and `ref` follows the mutability of the argument ([Mojo manual, ownership](https://docs.modular.com/mojo/manual/values/ownership)). Mojo checks "argument exclusivity": a function that receives a mutable reference to a value "can't receive any other references to the same value", whether mutable or immutable. `append_twice(my_string, my_string)` is a compile error, and the documented fix is to copy first ([Mojo manual, ownership](https://docs.modular.com/mojo/manual/values/ownership)). The check runs at each call site and is field-sensitive: `mutate_two(s.a, s.b)` is fine and `mutate_two(s.a, s)` is not ([call_emission_errors.mojo](https://github.com/modular/modular/blob/main/Mojo/test/mojo-parser/exprs/call_emission_errors.mojo)). A value whose type carries a mutable origin counts as a mutable access to that origin, with the note "memory accessed through reference embedded in value of type 'MyMutSpan[spanlife]'" ([same test](https://github.com/modular/modular/blob/main/Mojo/test/mojo-parser/exprs/call_emission_errors.mojo)).

Outside a single call, Mojo does not apply Rust's rule. Instead a live reference extends the life of its owner: "If there are existing references to a value, Mojo extends the lifetime of the owner" ([Mojo manual, ownership](https://docs.modular.com/mojo/manual/values/ownership)). Since 1.0.0, an experimental feature called interior origins makes `List`, `Dict`, `String`, and other containers hand out element references bound to the container's interior. Mutating the container then invalidates them: `ref elem = list[0]; list.append(4); print(elem)` is "use of invalidated interior reference" ([v1.0.0 notes](https://mojolang.org/releases/v1.0.0/)). The design doc calls this "flow-sensitive invalidation" and contrasts it with Rust's model, which it calls "sound, but it can feel restrictive" ([origin-design.md](https://github.com/modular/modular/blob/main/Mojo/proposals/origin-design.md)).

### References inside values

`ref` is an argument convention, not a type, so a field, an `Optional`, or a tuple cannot hold one ([ref-convention.md](https://github.com/modular/modular/blob/main/Mojo/proposals/ref-convention.md)). To store a reference you use `Pointer[T, origin]` or `Span`, and the struct must declare an origin parameter: `struct Example[origin: Origin]: var ptr: Pointer[Int, Self.origin]`. Since 1.0.0 a field cannot hide its origin behind `UnsafeAnyOrigin` ([v1.0.0 notes](https://mojolang.org/releases/v1.0.0/)). Because `ref` cannot be optional, `Dict.__getitem__` raises on a missing key and returns a `ref`, while `Dict.get` returns an `Optional` copy ([dict.mojo](https://github.com/modular/modular/blob/main/Mojo/stdlib/std/collections/dict.mojo)).

### What a returned reference borrows from

"You must provide an origin specifier for a ref return value" ([Mojo manual, origins](https://docs.modular.com/mojo/manual/values/lifetimes)). The usual getter is `def __getitem__(ref self, index: Int) -> ref[self] String`. When the result can come from either of two arguments, the signature names a union: `def pick_one(cond: Bool, ref a: String, ref b: String) -> ref[a, b] String`. The manual explains that the union "ensures that the compiler extends the lifetime of both values as long as the returned reference is live", and that the result is mutable only if both are ([Mojo manual, origins](https://docs.modular.com/mojo/manual/values/lifetimes)). A 2023 proposal rejected implicit origins on purpose: "it is better for learnability to not have magic like this" ([lifetimes-and-provenance.md](https://github.com/modular/modular/blob/main/Mojo/proposals/lifetimes-and-provenance.md)).

### What is proven

No formal model or proof was found. The design doc is marked "Draft" ([origin-design.md](https://github.com/modular/modular/blob/main/Mojo/proposals/origin-design.md)), and the roadmap says "some situations still aren't safe by default", with "Finalizing internal origins" and "Supporting mutable aliasing" still open ([Mojo roadmap](https://mojolang.org/docs/roadmap/)). Chris Lattner called `List.__getitem__` returning the whole list's origin "/the/ major memory safety problem" ([modular#4839](https://github.com/modular/modular/issues/4839)). Open issues still show references surviving a reallocation through two `ArcPointer`s to one list ([modular#5593](https://github.com/modular/modular/issues/5593)) and through iterators and `Span` constructors ([modular#6846](https://github.com/modular/modular/issues/6846)).

### Programs it rejects

- Passing the same value as `mut` and as another argument. Workaround: copy ([Mojo manual, ownership](https://docs.modular.com/mojo/manual/values/ownership)).
- A list of mutable pointers to two different locals, `[Pointer(to=a), Pointer(to=b)]`. The element type unifies to `Pointer[Int, origin_of(a, b)]`, so the two elements look aliased. The test says "FIXME: This really should work" ([expressions_errors.mojo](https://github.com/modular/modular/blob/main/Mojo/test/mojo-parser/exprs/expressions_errors.mojo)).
- Holding an element reference across any `mut` call on the container. Even a method that only appends to one field invalidates references into the other fields. A "selective invalidation" annotation is planned ([origin-design.md](https://github.com/modular/modular/blob/main/Mojo/proposals/origin-design.md)).
- Returning an optional or a tuple of `ref`s. Use `Pointer` ([ref-convention.md](https://github.com/modular/modular/blob/main/Mojo/proposals/ref-convention.md)).

## Hylo: mutable value semantics and second-class references

Hylo "is under active development and is not ready to be used yet" ([Hylo introduction](https://hylo-lang.org/introduction/)). The current compiler is [hylo-new](https://github.com/hylo-lang/hylo-new/tree/7961d90c54a2015b7a41ac4b8a827ad82b33897c); the old repository is archived ([hylo-lang/hylo](https://github.com/hylo-lang/hylo)).

### Shared and mutable access

Hylo has no references in its user model. "Lifetime annotations were not elided, they simply do not exist in Hylo because it uses a simpler model, devoid of references" ([Hylo introduction](https://hylo-lang.org/introduction/)). In the strict form of mutable value semantics, references "are only created implicitly, at function boundaries, and cannot be stored in variables or object fields" ([JOT 2022](https://www.jot.fm/issues/issue_2022_02/article2.pdf)).

Parameters use four conventions: `let` (immutable borrow), `inout` (mutable borrow, marked `&` at the call), `sink` (ownership transfer), and `set` (initialization) ([language tour, functions](https://hylo-lang.org/docs/user/language-tour/functions-and-methods/)). A `let` parameter "can't be returned, or stored anywhere that outlives the call", and the error suggests `v.copy()` ([same page](https://hylo-lang.org/docs/user/language-tour/functions-and-methods/)). The Law of Exclusivity is checked at the call site: `offset_inout(&v, by: v)` is "overlapping `inout` access to `v`" ([same page](https://hylo-lang.org/docs/user/language-tour/functions-and-methods/)). The core calculus checks it by path: "the 'path' to a value (i.e., a list of member accesses and/or array subscripts) never appears twice in inout arguments to a single function call", so `swap(&x[0], &x[1])` is well-typed and `swap(&x[f(0)], &x[1])` is not ([JOT 2022, section 3](https://www.jot.fm/issues/issue_2022_02/article2.pdf)).

Hylo is less strict than that core. It allows local `let` and `inout` bindings, which act as second-class references inside a function. While `let y = x.first` is alive, `x.first` cannot be mutated, and while `inout z = x.first` is alive, `x` cannot be read ([P2676R0, section 3.5](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2022/p2676r0.pdf)). Binding lifetimes end after their last use, not at the end of a scope ([Hylo spec](https://github.com/hylo-lang/specification/blob/main/spec.md)). The checker is an abstract interpreter over Hylo IR and runs on one function at a time ([Borrow checking Hylo, IWACO 2023](https://2023.splashcon.org/details/iwaco-2023-papers/5/Borrow-checking-Hylo)).

### References inside values

Not yet. Stored references are planned as `remote` parts: `type S { var part: remote inout Int }` ([RemoteParts.md](https://github.com/hylo-lang/hylo/blob/6889158246df571baa9079115817ab2bff61156b/Docs/RemoteParts.md)). A maintainer described them in 2025 as "still in the research phase, we haven't yet settled for the design" ([discussion #1347](https://github.com/orgs/hylo-lang/discussions/1347)). The rule proposed for them is that values with remote parts "aren't allowed to escape their local scope", the same rule as for local second-class references ([discussion #754](https://github.com/orgs/hylo-lang/discussions/754)). A constructor that takes a remote value extends the lifetime of its sources, but the designer was only "confident" that this works "for constructors of structural types and memberwise initializers" ([discussion #754](https://github.com/orgs/hylo-lang/discussions/754)). I found no stored `remote` property in hylo-new's positive tests or standard library.

### What a returned reference borrows from

Functions cannot return references. A function "must return an escapable object" ([Hylo spec, return statements](https://github.com/hylo-lang/specification/blob/main/spec.md)). Only subscripts and properties project a value, through a coroutine that `yield`s once and resumes after the caller's last use ([language tour, subscripts](https://hylo-lang.org/docs/user/language-tour/subscripts/)). The projection depends on every argument: "The projection yielded by a `let` or `inout` subscript call or property access projects the arguments bound to the subscript or property's parameters", with the example `let z = min[x, y] // 'z' projects both 'x' and 'y'` ([Hylo spec, projections](https://github.com/hylo-lang/specification/blob/main/spec.md)). This is the same as Rill's rule 2. Hylo has no syntax to narrow it. A hylo-new test shows the cost: after `let y = either[w, x]`, both `modify(&w)` and `modify(&x)` are errors until `y` is last used ([exclusivity-violation.hylo](https://github.com/hylo-lang/hylo-new/blob/7961d90c54a2015b7a41ac4b8a827ad82b33897c/Tests/CompilerTests/negative/exclusivity-violation.hylo)).

### What is proven

The JOT paper proves progress, preservation, and type soundness for Swiftlet, on paper, and shows that "all its local bindings denote disjoint regions of the memory". Swiftlet has structs, arrays, functions, `inout` parameters, and closures that capture by copy. It has no subscripts, local borrows, `sink`, remote parts, or generics ([JOT 2022](https://www.jot.fm/issues/issue_2022_02/article2.pdf)). Hylo itself is not formalized: the IWACO paper lists "formalizing Hylo's type system" as a next step ([IWACO 2023](https://2023.splashcon.org/details/iwaco-2023-papers/5/Borrow-checking-Hylo)). Separately, Osvald and Rompf prove in Coq that second-class values (values that cannot be returned, stored in fields, or captured by first-class closures) follow a stack discipline. That result covers lifetimes only, not aliasing ([Osvald et al., OOPSLA 2016](https://www.cs.purdue.edu/homes/rompf/papers/osvald-oopsla16.pdf)).

### Programs it rejects

- Returning a borrowed parameter from a function. Workarounds: `.copy()`, a `sink` parameter, or a subscript ([language tour, functions](https://hylo-lang.org/docs/user/language-tour/functions-and-methods/)).
- `&v.remove(v[2])`, because `v[2]` overlaps `v`. Workaround: `v[2].copy()` ([P2676R0](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2022/p2676r0.pdf)).
- `swap(&x[i], &x[j])` with run-time indices. Workaround: `&x.index_swap(i, j)`, which checks `i == j` at run time ([P2676R0](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2022/p2676r0.pdf)).
- A subscript that yields from inside a loop: "subscript cannot project more than once" ([illegal-yield.hylo](https://github.com/hylo-lang/hylo-new/blob/7961d90c54a2015b7a41ac4b8a827ad82b33897c/Tests/CompilerTests/negative/illegal-yield.hylo)).
- Iterators or views that outlive a scope, and graphs or doubly linked lists. The suggested design is indices into a whole that owns every node ([Racordon, Programming 2025](https://drops.dagstuhl.de/storage/01oasics/oasics-vol134-programming2025/OASIcs.Programming.2025.25/OASIcs.Programming.2025.25.pdf)).

## Swift: exclusivity enforcement

### Shared and mutable access

SE-0176 states the Law of Exclusivity: "two accesses to the same variable are not allowed to overlap unless both accesses are reads" ([SE-0176](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0176-enforce-exclusive-access-to-memory.md)). An `inout` argument is one long modification for the whole call. Its write access "starts after all of the non-in-out parameters have been evaluated", so `x += x` is fine and `swap(&x, &x)` is not ([TSPL, memory safety](https://github.com/swiftlang/swift-book/blob/main/TSPL.docc/LanguageGuide/MemorySafety.md)). Formally `inout` is copy-in copy-out, and passing by reference is only an optimization ([TSPL, declarations](https://github.com/swiftlang/swift-book/blob/main/TSPL.docc/ReferenceManual/Declarations.md)).

Enforcement is static for local variables, `inout` arguments, and properties of value types. It is dynamic for class properties, globals, statics, and locals captured by escaping closures ([Ownership Manifesto](https://github.com/swiftlang/swift/blob/main/docs/OwnershipManifesto.md)). A dynamic violation traps with "Fatal access conflict detected", and since Swift 5 the run-time checks are on in release builds ([Swift 5 exclusivity](https://www.swift.org/blog/swift-5-exclusivity/)). Dynamic checks are "not required to detect race conditions", and races stay undefined behavior ([SE-0176](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0176-enforce-exclusive-access-to-memory.md)).

Granularity is coarse. Different stored properties of a local struct may overlap, but only if it is a local variable, not captured by an escaping closure, and the properties are stored, not computed ([TSPL, memory safety](https://github.com/swiftlang/swift-book/blob/main/TSPL.docc/LanguageGuide/MemorySafety.md)). Collections get no special treatment: "mutating an element of an array will require exclusive access to the entire array" ([SE-0176](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0176-enforce-exclusive-access-to-memory.md)).

### References inside values

Classic Swift has none. The manifesto says a program "cannot return a shared value, construct an array of shared values, store shared values into `struct` fields", and that lifetime systems "add a lot of complexity to the user model" ([Ownership Manifesto](https://github.com/swiftlang/swift/blob/main/docs/OwnershipManifesto.md)). Swift 6.2 added `~Escapable` types, which "cannot be assigned or transferred outside of the immediate context" ([SE-0446](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0446-non-escapable.md)). Containment spreads, much like Rill's rule 1: "an escapable struct or enum can only contain escapable values" ([SE-0446](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0446-non-escapable.md)), and an `Optional` of a non-escapable value is non-escapable with "precisely the same lifetime constraints" ([SE-0465](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0465-nonescapable-stdlib-primitives.md)). Collections of non-escapable elements are not supported yet ([SE-0465](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0465-nonescapable-stdlib-primitives.md)). The read-only view `Span` is copyable, but `MutableSpan` is not, because "a copy would represent an additional access, in violation of the law of exclusivity" ([SE-0467](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0467-MutableSpan.md)).

The lifetime dependency proposal "models every value as having at most one lifetime dependency". Building a `Pair` from two views makes the pair depend on both, and after `pair.second = shortLived`, using `pair.first` past `shortLived`'s scope is an error. Per-field dependencies are only a future direction ([lifetime dependency proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md)).

### What a returned reference borrows from

A function that returns a non-escapable value needs a `@lifetime` annotation. The annotation is not accepted yet; it ships behind the `Lifetimes` experimental flag as `@_lifetime` ([proposal PR #3145](https://github.com/swiftlang/swift-evolution/pull/3145)). It names the kind of dependency: `borrow x` (shared), `&x` (exclusive), or `copy x` (inherit the dependencies of a non-escapable `x`), and the target can be an `inout` parameter, not only the result ([lifetime dependency proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md)). The defaults are:

- **Same type.** A non-`inout` parameter of the same non-escapable type as the result gives `copy a`, for every such parameter.
- **Single parameter.** With exactly one parameter, the result borrows it (or depends on it exclusively if it is `inout`).
- **`inout`.** A non-escapable `inout` parameter depends on its own old value, and on no other parameter. `span = another` inside a function taking `span: inout Span<Int>` and `another: Span<Int>` is an error until it is annotated `@lifetime(span: copy another)`.

All three come from the [lifetime dependency proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md). There is no default for two or more escapable parameters. Defaults are decided from declared types, so a generic `foo(a: T) -> U` is an error even if callers pass `T == U`. A closure that returns a non-escapable value depends on the closure, which "borrows all its captured values" ([same proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md)).

### What is proven

No formal or mechanized proof of exclusivity, `~Escapable`, or lifetime dependencies was found. The Swift 5 post admits "Some rare corner cases involving illegal code aren't yet diagnosed" ([Swift 5 exclusivity](https://www.swift.org/blog/swift-5-exclusivity/)). SE-0176 calls its rule for non-escaping closures "a conservative over-approximation" and argues for it informally ([SE-0176](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0176-enforce-exclusive-access-to-memory.md)). Featherweight Swift formalizes only a purely functional subset ([Racordon and Buchs, SLE 2020](https://kyouko-taiga.github.io/assets/papers/sle2020-fs.pdf)).

### Programs it rejects

- `swap(&array[0], &array[1])`. Workaround: `array.swapAt(0, 1)` ([SE-0176](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0176-enforce-exclusive-access-to-memory.md), [SE-0173](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0173-swap-indices.md)).
- Reading one element while mutating another, such as `array[0].forEach { array[1].append($0) }` ([SE-0176](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0176-enforce-exclusive-access-to-memory.md)).
- A callee that reads a global it was also passed `inout`, such as `increment(&stepSize)` where the body reads `stepSize`. Workaround: copy to a local ([TSPL, memory safety](https://github.com/swiftlang/swift-book/blob/main/TSPL.docc/LanguageGuide/MemorySafety.md)).
- A closure that captures the variable being mutated, `modifyTwice(&count) { $0 += count }` ([Swift 5 exclusivity](https://www.swift.org/blog/swift-5-exclusivity/)).
- An escaping closure that captures an `inout` parameter ([TSPL, declarations](https://github.com/swiftlang/swift-book/blob/main/TSPL.docc/ReferenceManual/Declarations.md)).
- Appending to an array while a `Span` of it is alive: "'a' is borrowed by 'ref1' and 'ref2'" ([lifetime dependency proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md)).

## Vale: generational references and regions

Most of Vale's region features are previews: "the check override operator, region borrow checker, and hybrid-generational memory are all upcoming features, not available yet" ([Vale memory safety](https://vale.dev/memory-safe)). Its author now works on a successor, Valen ([group borrowing post](https://verdagon.dev/blog/valen-group-borrowing)).

### Shared and mutable access

Vale has no aliasing rule: "There's nothing like the borrow checker's aliasability-xor-mutability restrictions; objects can be aliased freely" ([regions part 1](https://verdagon.dev/blog/zero-cost-borrowing-regions-part-1-immutable-borrowing)). Every mutable struct has one owning reference ([Vale guide, references](https://vale.dev/guide/references)). Every other reference is a generational reference: it stores a remembered generation, and each dereference asserts that it still matches the object's current one ([Vale memory safety](https://vale.dev/memory-safe)). A failed check halts the program ([generational references](https://verdagon.dev/blog/generational-references)). The default implementation uses random generations, so "The odds of an invalid access happening undetected is always 1/2^64" ([generational references](https://verdagon.dev/blog/generational-references)).

Regions add opt-in static immutability. A `pure` function sees "its parameters as all inside an immutable region", so it cannot mutate them, and the compiler can skip generation checks on them ([regions overview](https://verdagon.dev/blog/zero-cost-memory-safety-regions-overview)). Regions "don't change the semantics of the program"; the test interpreter ignores them ([regions overview](https://verdagon.dev/blog/zero-cost-memory-safety-regions-overview)).

### References inside values

Yes, with no annotation in the default mode: `struct Ship { name str; from &Base; }` ([Vale guide, regions](https://vale.dev/guide/regions)). A struct that points into another region takes a region parameter, `struct Ship<x'> { ... dock &x'Dock; }` ([regions overview](https://verdagon.dev/blog/zero-cost-memory-safety-regions-overview)).

### What a returned reference borrows from

In the default mode nothing is tracked statically; any returned reference is checked when it is used ([generational references](https://verdagon.dev/blog/generational-references)). Valen changes this: a reference type remembers a path, such as `&Entity in world.entities[]`, mutating an object invalidates references that point inside it, and a returned reference is written `-> &Gem in self.gem`. Where the path is unknown, Valen reads `-> &Gem` as "points somewhere inside `self`" ([group borrowing](https://verdagon.dev/blog/valen-group-borrowing)).

### What is proven

No formal semantics or proof was found on vale.dev or verdagon.dev. The safety claim rests on "if we always call `__check` before dereferencing, then our code will be memory-safe" ([generational references](https://verdagon.dev/blog/generational-references)). The author notes that LLVM would probably treat a generational check as undefined behavior if it could see that freed memory is read on purpose ([first regions prototype](https://verdagon.dev/blog/first-regions-prototype)).

### Programs it rejects

Few at compile time. The cost is moved to run-time halts and checks. Removing checks needs the "linear style", which threads values through returns: `func RandInt(rand Random) (i64, Random)` instead of `func RandInt(rand &Random) i64` ([linear types and borrowing](https://verdagon.dev/blog/linear-types-borrowing)). A `pure` function that tries `tenShips.add(Ship(42))` on a parameter is rejected ([regions part 1](https://verdagon.dev/blog/zero-cost-borrowing-regions-part-1-immutable-borrowing)).

## Austral: linear types and explicit regions

### Shared and mutable access

A value of a linear type "must be used once and exactly once". It must be used in every branch of an `if` or in none, and it may not appear in a loop body ([Austral spec](https://austral-lang.org/spec/spec.html)). A borrow does not count as a use. The same variable cannot be borrowed mutably twice in one expression, and a mutably borrowed variable cannot be borrowed again ([How Austral's linear type checker works](https://borretti.me/article/how-australs-linear-type-checker-works)). The checker is about 600 lines of OCaml ([same post](https://borretti.me/article/how-australs-linear-type-checker-works)). Violations are compile errors.

### References inside values

References are values with a region type parameter: `&[T, R]` and `&![T, R]` ([Austral spec](https://austral-lang.org/spec/spec.html)). An anonymous borrow `&x` cannot leave its statement, because its region has no name. A named borrow, `borrow x as y in R do ... end`, introduces `R` for one block ([Austral spec](https://austral-lang.org/spec/spec.html)). Records can hold references if they take the region as a parameter, as in `record Container[R: Region]: Free is sp: Span[Int32, R]; end;` ([test 016-spans/007](https://github.com/austral/austral/blob/master/test-programs/suites/016-spans/007-transform-span-in-record/Test.aum)). A record that holds a mutable view must be linear ([test 016-spans/008](https://github.com/austral/austral/blob/master/test-programs/suites/016-spans/008-write-through-span-record/Test.aum)). Escape is prevented by scope alone: "There's no way to write the type of a reference outside the scope where that reference is defined" ([Borretti](https://borretti.me/article/how-australs-linear-type-checker-works)).

### What a returned reference borrows from

The region named in the signature, with no inference: `generic [T: Type, R: Region] function loadRead(ref: &[Pointer[T], R]): Reference[T, R]` ([Austral spec](https://austral-lang.org/spec/spec.html)). Two borrows of different variables need two region parameters, as in `generic [R: Region, S: Region] function bar(a: &![Foo, R], b: &![Foo, S])` ([test 007-borrowing](https://github.com/austral/austral/tree/master/test-programs/suites/007-borrowing)).

### What is proven

The README says linear types "allow resources to be handled in a provably-safe manner" ([Austral README](https://github.com/austral/austral)), but no formal proof is linked. The design argues from simplicity: "the rules should be simple enough they fit in a page of text" ([Borretti](https://borretti.me/article/how-australs-linear-type-checker-works)). The spec and the compiler have drifted. The spec puts write references in the free universe and the compiler puts them in the linear one ([TypeSystem.ml](https://github.com/austral/austral/blob/master/lib/TypeSystem.ml)).

### Programs it rejects

- Consuming a linear value in one branch only, or in a loop that runs once ([Borretti](https://borretti.me/article/how-australs-linear-type-checker-works)).
- `doSomething(&!x, &!x)` ([Borretti](https://borretti.me/article/how-australs-linear-type-checker-works)).
- Keeping a reference past its statement without a `borrow` block, as in `let ref: &[Lin, R] := foo(&x);` ([Borretti](https://borretti.me/article/how-australs-linear-type-checker-works)).
- A getter on an opaque linear type without borrowing has to return the value back: `function length(list: List[T]): Pair[Index, List[T]]` ([Borretti](https://borretti.me/article/how-australs-linear-type-checker-works)).

## OxCaml: locality and uniqueness modes

Jane Street's OCaml branch tracks modes, which are "deep properties of values" that are inferred like types but are not types ([OxCaml modes](https://oxcaml.org/documentation/modes/intro/)). The authors chose modes over lifetime variables because "higher-order functions require higher-order polymorphism, for which type inference is undecidable in general" ([Oxidizing OCaml: locality](https://blog.janestreet.com/oxidizing-ocaml-locality/)).

### Shared and mutable access

A `local` value cannot escape its region, where each function body and loop body is a region. Returning it or storing it in a global is an error ([OxCaml modes](https://oxcaml.org/documentation/modes/intro/)). A `unique` parameter is consumed, and `borrow_` lends a unique value as aliased and local for one `let`, function argument, or `match` ([OxCaml borrowing](https://oxcaml.org/documentation/uniqueness/borrow/)). There is no exclusive mutable borrow yet; "exclusive mutable" is planned ([uniqueness pitfalls](https://oxcaml.org/documentation/uniqueness/pitfalls/)). Data races are prevented by a separate contention mode: a reference "can be written only if it is uncontended", and Rust-style aliasing XOR mutability is not imposed "because we want all legacy (sequential) OCaml code to be well-typed" ([Data Race Freedom à la Mode, POPL 2025](https://iris-project.org/pdfs/2025-popl-drfcaml.pdf)).

### References inside values

Locality is deep, which is Rill's rule 1: "a local value of type `string list` means a local list of local strings", and the checker "does not separately track parts of the variable", so taking a global part out of a local tuple still gives a local value ([stack allocation reference](https://oxcaml.org/documentation/stack-allocation/reference/)). A `global_` field opts out. Mutable fields must hold global values, because "naively permitting mutability might allow an older local mutable value to be mutated to point to a younger one, creating a dangling reference" ([stack allocation reference](https://oxcaml.org/documentation/stack-allocation/reference/)). A closure that captures a local value is local, so higher-order functions mark their callbacks `@ local` ([stack allocation reference](https://oxcaml.org/documentation/stack-allocation/reference/)).

### What a returned reference borrows from

A function of type `a @ local -> b @ local` "promises not to capture any reference to its argument except possibly in its return value", and the result is local to the caller's region ([stack allocation reference](https://oxcaml.org/documentation/stack-allocation/reference/)). There is no per-argument dependency. There is also no mode polymorphism, and Jane Street counted 85 functions duplicated across locality modes ([Lorenzen et al., ICFP 2024](https://antonlorenzen.de/papers/oxidizing-ocaml-modal-memory-management.pdf)).

### What is proven

The ICFP 2024 paper proves its calculus of locality, uniqueness, and affinity sound on paper, and that calculus has no mutable references ([ICFP 2024](https://antonlorenzen.de/papers/oxidizing-ocaml-modal-memory-management.pdf), [POPL 2025](https://iris-project.org/pdfs/2025-popl-drfcaml.pdf)). The POPL 2025 paper adds mutable references, contention, and portability, and "All results in this paper have been mechanized in Rocq" ([POPL 2025](https://iris-project.org/pdfs/2025-popl-drfcaml.pdf)). The shipped compiler lists a known unsoundness with uniqueness and pattern matching ([OxCaml uniqueness](https://oxcaml.org/documentation/uniqueness/intro/)).

### Programs it rejects

- Storing a local value into a mutable field or array. Workaround: copy it to the heap ([stack allocation reference](https://oxcaml.org/documentation/stack-allocation/reference/)).
- Returning a part of a local tuple or record, even if that part is global ([stack allocation reference](https://oxcaml.org/documentation/stack-allocation/reference/)).
- Using a unique array repeatedly in a `for` loop. Workaround: thread it through returns ([uniqueness pitfalls](https://oxcaml.org/documentation/uniqueness/pitfalls/)).
- A closure that uses a borrowed unique value counts as a permanent aliased use ([OxCaml borrowing](https://oxcaml.org/documentation/uniqueness/borrow/)).

## Risks for Rill's two rules

The rules are in [issue #7](https://github.com/Reilley64/rill/issues/7) and [ADR 0002](https://github.com/Reilley64/rill/blob/main/docs/adr/0002-visible-ownership-without-lifetimes.md). Ownership is checked on generic MIR before monomorphization ([ADR 0006](https://github.com/Reilley64/rill/blob/main/docs/adr/0006-whole-program-monomorphization-on-llvm-23.md)). The risks below are analysis that applies the sources above to Rill.

### Rule 1: a value that holds a reference is itself a borrow

1. **One borrow per value merges its sources.** A record `{ a: &x, b: &y }` or a `List<&User>` filled from two lists borrows everything it was built from, and a reference taken out of it still borrows all of them. Swift has the same limit ("at most one lifetime dependency" per value), OxCaml's deep locality does too, and Mojo's union origins make two pointers to different locals look aliased ([Swift proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md), [OxCaml](https://oxcaml.org/documentation/stack-allocation/reference/), [Mojo test](https://github.com/modular/modular/blob/main/Mojo/test/mojo-parser/exprs/expressions_errors.mojo)). With mutation this is stricter than without it, because the merged borrow also blocks mutating every source.
2. **Writing a reference into a value through a mutable borrow.** Consider `function remember(seen: &mut List<&User>, user: &User): void` that pushes `user`. Nothing in rule 2 relates `user` to `seen`, because rule 2 only covers the return value. If this is allowed, the caller can free `user` while `seen` still holds it. Every language here closes this hole explicitly: Rust makes `&mut T` invariant in `T` ([Nomicon](https://doc.rust-lang.org/nomicon/subtyping.html)), OxCaml requires mutable fields to hold global values ([OxCaml](https://oxcaml.org/documentation/stack-allocation/reference/)), Swift's `inout` default refuses `span = another` without `@lifetime(span: copy another)` ([Swift proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md)), and Mojo puts the origin in the element type ([Mojo v1.0.0](https://mojolang.org/releases/v1.0.0/)). Rill needs a rule for flows between parameters, not only from parameters to the result.
3. **The outer and inner borrows must stay separate.** If a `&mut List<&User>` were treated as one borrow, the list would stay mutably borrowed for as long as the users it holds. That is the Rust pattern `&'a mut Vec<&'a User>`, which invariance makes lock the vector for the rest of its life (analysis from the variance rules in the [Nomicon](https://doc.rust-lang.org/nomicon/subtyping.html)). So "the value is a borrow" has to mean at least two regions once mutable borrows exist: the borrow of the container and the borrow its contents hold.
4. **Owned parameters can hold references too.** `function firstOf(users: List<&User>): Option<&User>` takes its list by value, so it has no `&` parameter, and rule 2 as written says the result borrows nothing. The same applies to a generic `function identity<T>(x: T): T` called with `T = &User`. Swift handles this with `copy` dependencies, where the result inherits the borrows of a reference-holding argument ([Swift proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md)). Rill's rule 2 must count every parameter whose type holds a reference, and it must say that a borrow moves with the value that carries it.
5. **Generic code cannot see whether `T` holds a reference.** Because ownership is checked before monomorphization, a generic body that sends a `T`, passes it to `go`, or wraps it in `Shared<T>` must either be rejected or require a constraint meaning "`T` holds no references". Swift has `Escapable` with conditional conformance for this ([SE-0446](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0446-non-escapable.md)), and Rust's `thread::spawn` requires `F: Send + 'static` ([std docs](https://doc.rust-lang.org/std/thread/fn.spawn.html)). Rill has no traits, so the constraint would be a built-in `where` form.
6. **Closures are values that hold references.** Rill already says a closure passed to a borrowed parameter borrows its captures ([issue #7](https://github.com/Reilley64/rill/issues/7)). A closure that captures a `&T` and is stored or returned is then a borrow under rule 1. Swift and OxCaml both treat such a closure as non-escaping ([Swift proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md), [OxCaml](https://oxcaml.org/documentation/stack-allocation/reference/)). With mutation, a closure that captures a mutable borrow must also be exclusive, which is why Swift's `MutableSpan` is non-copyable ([SE-0467](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0467-MutableSpan.md)) and Swift checks non-escaping closures that capture the same variable ([SE-0176](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0176-enforce-exclusive-access-to-memory.md)).
7. **Shared state that mutates through a shared borrow.** `send(&jobs, n)` in [prototype 4](https://github.com/Reilley64/rill/blob/main/prototypes/04-goroutines-channels.rill) already changes a channel through `&`. If `Shared<T>` also lets its contents change, two `Shared` handles to one list can alias without the checker seeing it. That is the open Mojo hole with two `ArcPointer`s ([modular#5593](https://github.com/modular/modular/issues/5593)). In Rust "you cannot generally obtain a mutable reference to something inside an `Arc`", and mutation goes through `Mutex`, `RwLock`, or atomics ([std docs](https://doc.rust-lang.org/std/sync/struct.Arc.html)). Swift checks class properties at run time ([Ownership Manifesto](https://github.com/swiftlang/swift/blob/main/docs/OwnershipManifesto.md)), and OxCaml adds a contention mode ([POPL 2025](https://iris-project.org/pdfs/2025-popl-drfcaml.pdf)). `all(() => f(&numbers), () => g(&numbers))` is safe today only because nothing can mutate through those borrows.

### Rule 2: a returned reference borrows from every `&` parameter

1. **It keeps arguments borrowed that the result never points into.** `function get(map: &Map<string, User>, key: &string): Option<&User>` keeps `key` borrowed while the user is in use. Once mutation exists, reusing a key buffer in a loop while holding earlier results is rejected. Hylo has the same rule and accepts the cost, with no way to narrow it ([Hylo spec](https://github.com/hylo-lang/specification/blob/main/spec.md), [hylo-new test](https://github.com/hylo-lang/hylo-new/blob/7961d90c54a2015b7a41ac4b8a827ad82b33897c/Tests/CompilerTests/negative/exclusivity-violation.hylo)). Rust's elision refuses this signature instead of guessing ([Reference](https://doc.rust-lang.org/reference/lifetime-elision.html)), and Mojo and Austral make the author name the source ([Mojo manual](https://docs.modular.com/mojo/manual/values/lifetimes), [Austral spec](https://austral-lang.org/spec/spec.html)).
2. **Temporaries.** `get(&users, &makeId())` borrows a temporary for as long as the result lives. Rill must either keep the temporary alive for that long, as Mojo extends an owner's life while references to it are live ([Mojo manual](https://docs.modular.com/mojo/manual/values/ownership)), or reject the call. This interacts with Rill's rule that a value with no `dispose` may be freed after its last use.
3. **Mutable parameters stay locked.** If the result also borrows from a mutable borrow parameter, the caller cannot read that argument until the result is dead. This is the Rust `mutate_and_share` case ([Nomicon](https://doc.rust-lang.org/nomicon/lifetime-mismatch.html)), and under rule 2 it happens to every function that takes a mutable borrow and returns any reference. Swift separates `borrow x` from `&x` dependencies for this reason ([Swift proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md)). Turning the mutable borrow into a shared one when the function returns is a possible relaxation. No source here proves it sound, and the channel case in risk 7 above is where it could break.
4. **The ADR's workaround does not work for mutable borrows.** ADR 0002 says the fix for an over-strict result is to pass the other argument owned or copied ([ADR 0002](https://github.com/Reilley64/rill/blob/main/docs/adr/0002-visible-ownership-without-lifetimes.md)). A mutable borrow cannot be replaced by a copy without losing the mutation, and moving a large value in and back out is the "linear style" that Vale and Austral users fall back on ([Vale](https://verdagon.dev/blog/linear-types-borrowing), [Borretti](https://borretti.me/article/how-australs-linear-type-checker-works)).
5. **No `&` parameter at all.** A function with no reference parameters that returns a reference has nothing to borrow from. Rust rejects `fn get_str() -> &str` ([Reference](https://doc.rust-lang.org/reference/lifetime-elision.html)). Mojo has a static origin for string literals, and Swift has `@lifetime(immortal)` ([Mojo manual](https://docs.modular.com/mojo/manual/values/lifetimes), [Swift proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md)). Rill must say which it does.
6. **Function types and overloads.** The rule must also apply to function types, such as a callback `(user: &User) => &string`, and to captures, which Swift treats as an extra source ([Swift LifetimeAnnotation.md](https://github.com/swiftlang/swift/blob/main/docs/ReferenceGuides/LifetimeAnnotation.md)). Rust shows what happens when a closure literal is left out of elision ([rust-lang/rust#58052](https://github.com/rust-lang/rust/issues/58052)). For overloads, callers should check against the body-less declaration from [ADR 0003](https://github.com/Reilley64/rill/blob/main/docs/adr/0003-overloads-need-a-declaration.md). Swift accepts an implementation whose dependencies are a subset of the interface's ([Swift proposal](https://github.com/swiftlang/swift-evolution/blob/e010d6be0a051544905d2d362e97d0743c4065c4/proposals/NNNN-lifetime-dependency.md)).
7. **The rule says nothing about the function body.** Rule 2 fixes what callers see. Inside the body, a get-or-insert that returns a borrow on one path and mutates on another is Rust's problem case #3. Accepting it needs a Polonius-style flow-sensitive check, not just NLL ([RFC 2094](https://rust-lang.github.io/rfcs/2094-nll.html), [Polonius on nightly](https://blog.rust-lang.org/2026/08/04/enabling-polonius-alpha-on-nightly/)).

### Soundness

None of the lifetime-free designs here has a proof that covers stored references and mutation together. The proofs that exist cover narrower systems: Swiftlet's `inout` without stored references ([JOT 2022](https://www.jot.fm/issues/issue_2022_02/article2.pdf)), second-class values for lifetimes only ([OOPSLA 2016](https://www.cs.purdue.edu/homes/rompf/papers/osvald-oopsla16.pdf)), and OxCaml's single level of locality with mutable fields restricted to global values ([POPL 2025](https://iris-project.org/pdfs/2025-popl-drfcaml.pdf)). RustBelt covers stored references and mutation, but with explicit lifetimes ([RustBelt](https://plv.mpi-sws.org/rustbelt/popl18/paper.pdf)).

Analysis: Rill's two rules read like Rust signatures where every reference parameter, every reference-holding parameter, and the result share one lifetime `'a`. If every Rill signature can be translated that way, Rill can borrow its safety argument from Rust's model. Risks 2 and 3 under rule 1 are the places where that translation breaks once `&mut` exists, because invariance makes a single shared `'a` too strict for `&mut` of a value that holds references.

## Implications for Rill

These are the questions the [Rill mutation](https://github.com/Reilley64/rill/issues/15) ticket has to decide. Each lists the options seen above.

1. **Where references may live.**
   - Keep rule 1 as it is: references in any value, and the value becomes a borrow. This is the most flexible option, and it is close to Swift `~Escapable` and OxCaml `local`. It needs the extra rules in the risks above.
   - Allow references only in parameters, local bindings, and returned results, as Hylo does today. This is simpler to check and has a proof for its core. But it brings back the cost that ADR 0002 rejected Hylo for, which is that lookups must copy or use a projection.
   - Allow references in values but not in mutable positions, as OxCaml does for mutable fields. This closes the older-points-to-younger hole without signatures, but `&mut List<&User>` stops working.
2. **How a mutable borrow can store a reference that came from another parameter.**
   - Forbid it, like Swift's `inout` default.
   - Assume every reference-holding argument may flow into every mutable borrow argument. This is safe and needs no syntax, but it ties all of a call's arguments together.
   - Add an optional mark in signatures for this case only, like Swift's `@lifetime(target: copy source)`.
3. **How precise rule 2 is.**
   - Keep "every `&` parameter", extended to every reference-holding parameter (risk 4 under rule 1). Hylo does the same.
   - Use Rust-style defaults: one source means that source, and anything else is an error until the author writes something.
   - Keep the default, and add an optional way to name the source when the default is too strict, like Mojo's `ref[a]` or Valen's `in self`. This brings back lifetime-like syntax, but only where it is needed.
   - Narrow by type: skip parameters whose type cannot contain the result type. No language here does exactly this. Swift's same-type default is the closest.
4. **What a returned reference does to a mutable borrow parameter.** The options are to keep it locked as mutable (Rust), to downgrade it to shared (needs a soundness argument that accounts for channels and `Shared`), or to separate shared and exclusive dependencies (Swift).
5. **The rule between shared and mutable access.**
   - Static aliasing XOR mutability with last-use lifetimes (Rust NLL, Hylo).
   - The same plus a Polonius-style flow-sensitive check for conditional returns.
   - Per-call exclusivity plus invalidation of references into containers (Mojo). This accepts more programs, but Mojo has not made it sound yet.
   - Run-time checks (Vale generations, Swift's dynamic enforcement). These cost time and turn errors into halts.
6. **Granularity.** Track disjoint fields and literal indices (Hylo paths, Mojo, Swift locals) or treat a value as one unit (Swift collections, OxCaml). Also decide whether that precision crosses function boundaries. In Rust it does not.
7. **What a borrow does to its source's lifetime.** Either it is an error when a borrowed value dies first (Rust), or the borrow keeps the source alive (Mojo). This decides what happens to temporaries passed as `&`.
8. **A generic constraint for "holds no references".** This is needed for `go`, `send`, and `Shared<T>` in generic code, because ownership is checked before monomorphization.
9. **Mutation through `Shared<T>` and channels.** The options are that the contents of `Shared<T>` cannot be mutated and mutation goes through a lock or a channel (Rust `Arc`), a run-time exclusivity check (Swift classes, Vale cells), or a contention mode (OxCaml).
10. **The soundness story.** Decide whether the spec argues safety by translation to single-lifetime Rust signatures, by a restricted design with an existing proof (Hylo's core, OxCaml), or not at all for v0.
