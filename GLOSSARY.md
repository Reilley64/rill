# Rill

Rill is a functional-first programming language with TypeScript-style syntax. It compiles to native code and WebAssembly through LLVM, and it is memory safe without a tracing garbage collector.

## Language

**Borrow**:
Access to a value through a read-only reference (`&T`) or an exclusive mutable reference (`&mut T`) without owning it. Reference types do not name lifetimes.
_Avoid_: lifetime, pointer

**Borrow dependency**:
A requirement that a source remain valid and obey access restrictions while a reference, or a value carrying it, can still be used.
_Avoid_: ownership, lifetime annotation

**Borrow-free value**:
A value carrying no borrowed dependencies, including through nested fields or captures, as required by `noBorrow(T)`. This does not imply copyability or safe concurrent sharing.
_Avoid_: owned value, thread-safe value

**Callable**:
A function value with a read-only, mutable, or consuming invocation contract. That contract is separate from purity, borrowed captures, and copyability.
_Avoid_: Fn trait, async function

**Cancellation**:
A cooperative request to stop a goroutine while completing its required cleanup. It is distinct from panic and may remain pending while cleanup or another protected operation finishes.
_Avoid_: panic, force kill, immediate termination

**Dispose**:
The `dispose` overload for a type: a function the compiler calls when a value of that type is freed. Calling it directly ends the value early.
_Avoid_: drop, destructor, deinit, finalizer

**Enum**:
A nominal type defining a closed set of unit, tuple, or record variants. Enums are Rill's v0 sum-type mechanism; matching can distinguish their variants at runtime.
_Avoid_: untagged union, structural union, backing-value list

**Enum variant**:
One of an enum's named alternatives, optionally carrying payload data. Its qualified name also denotes a distinct variant type, usable in annotations and overload parameters.
_Avoid_: string tag, singleton payload value, independent record member

**Expectation**:
A top-level test declaration written `expect <expression>;`, whose boolean result determines whether it passes. Expectations run only under `rill test`, not as production assertions.
_Avoid_: runtime assertion, precondition

**Function**:
A named or anonymous piece of code taking arguments and returning a value; named functions receive runtime state explicitly, while arrow functions may capture it. Every Rill function can suspend, and callers wait without extra syntax.
_Avoid_: async function, sync function, method

**Goroutine**:
A lightweight concurrent unit of execution, started with `spawn(() => f(x))`.
_Avoid_: thread, task, fiber, green thread

**Guard**:
A statement of the form `guard <condition> else { ... }` whose `else` block must leave its scope, with `return`, `break`, `continue`, or a call to a `never` function. It can also unwrap a value, as in `guard let user = findUser(id) else { ... }`.
_Avoid_: precondition, assert, early return

**Lock guard**:
A non-copyable value that borrows a mutex and owns responsibility for releasing its lock. Its disposer unlocks; protected borrows depend on it, and ownership stays in the locking goroutine.
_Avoid_: shared lock owner, raw lock, OS-thread-affine guard

**Map**:
Rill's key/value collection type, written `Map<K, V>`, distinct from the lowercase `map(...)` transformation function.
_Avoid_: Dictionary, dict

**Module**:
A source file's declaration scope and privacy boundary, with one identity preserved through import aliases and re-exports. Its defining location owns nominal types and overload permissions.
_Avoid_: namespace object, runtime initializer

**Mutable borrow**:
Exclusive access through `&mut T` that permits mutation without ownership. Conflicting access to overlapping data is excluded while the borrow is needed.
_Avoid_: shared mutable reference, raw pointer

**Mutex**:
A synchronization value granting exclusive access to a protected payload through a lock guard.
_Avoid_: mutable Shared, runtime borrow checker

**Opaque value**:
A value whose canonical nominal type is private outside its defining module. Clients may hold, pass, and dispose it through visible operations, but cannot name or inspect its representation.
_Avoid_: anonymous record, structural value, erased identity

**Overload**:
One of several functions that share a name and fit that name's overload declaration, each with more specific parameter types. Overloads are how Rill gives one operation different behavior for different types.
_Avoid_: method, trait implementation, instance

**Overload declaration**:
A body-less `function` signature, ending in `;`, that owns an overload family's identity, visibility, and required shape. Imported aliases identify that same family, not new declarations.
_Avoid_: trait, interface, protocol, type class

**Prelude**:
The fixed primitive types, intrinsic predicates, and canonical copy/dispose declarations available without explicit imports. It introduces no runtime global bindings.
_Avoid_: wildcard std import, module initializer

**Private field**:
A record field accessible only in its defining module. Its presence prevents external whole-record construction or rebuilding, regardless of aliases.
_Avoid_: protected field, project-private field

**Record**:
A value of a declared nominal type with named fields, constructed with its type name as in `Point { x: 0, y: 0 }`. Matching fields do not make different record types interchangeable; anonymous structural records do not exist.
_Avoid_: object literal, structural record

**Reborrow**:
A temporary loan of existing borrowed access, restricting conflicting use of the original access until the loan is no longer needed.
_Avoid_: copy, new owner

**Routine**:
The handle `spawn(() => f(x))` returns for the goroutine it starts. `stop(&routine)` requests cancellation and waits for termination and cleanup; freeing the handle does not stop it.
_Avoid_: task, future, promise, join handle

**Sendable**:
Safe to transfer ownership or permitted exclusive access between independently executing goroutines, as required by `sendable(T)`. This does not imply borrow-freedom or safe concurrent read-sharing.
_Avoid_: borrow-free, copyable, thread-affine

**Shareable**:
Safe for read-only access by concurrently executing goroutines, as required by `shareable(T)`. This grants neither mutable access nor copyability.
_Avoid_: shared owner, mutable, copyable

**Shared**:
A `Shared<T>` handle gives a value more than one owner by counting its owners. It is the only counted value in Rill, and shared ownership alone grants no mutable access to its payload.
_Avoid_: Arc, Rc
