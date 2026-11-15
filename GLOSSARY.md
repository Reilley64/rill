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

**Callable requirement**:
A compiler-checked invocation contract on a concrete callable type, written with callable, callableMut, or callableOnce. It preserves the callable's argument, result, access and capture evidence without introducing a user-implementable trait.
_Avoid_: erased callback guarantee, function trait

**Bytes**:
Rill's owned contiguous byte sequence, distinct from List<u8>. Read-only logical views can denote whole sequences or subranges while retaining their sources.
_Avoid_: byte string, contiguous List guarantee, owning view

**Cancellation**:
Rill's cooperative runtime process for stopping a goroutine while completing required cleanup; delivery can remain pending during protected work. A cancellation-token request is a separate signal and does not by itself stop a goroutine.
_Avoid_: panic, force kill, immediate termination

**Cancellation source**:
An owned capability to request one-shot cancellation state, including its directed descendants. A request establishes a signal, not completion of the operations observing it.
_Avoid_: join handle, task owner, cancellation token

**Cancellation token**:
An owned observation capability for one-shot cancellation state, used explicitly by an operation. It grants no authority to request cancellation and is not a goroutine or completion result.
_Avoid_: ambient context, stop handle, cancellation source

**Character**:
A `char` value representing one Unicode scalar, not necessarily a whole user-perceived character. Its code point is distinct from its UTF-8 byte encoding.
_Avoid_: byte, grapheme, UTF-16 code unit

**Caused-by wrapping**:
An explicit error-context step written `inner ? outer`: use the successful value, or return the outer error with the original failure as its cause. Merely propagating an error adds no new context.
_Avoid_: error-mapping callback, propagation report

**Dispose**:
The `dispose` overload for a type: extra cleanup followed by disposal of its remaining owned fields. Calling it directly consumes the value early; moved or already disposed fields are not disposed again.
_Avoid_: drop, destructor, deinit, finalizer

**Enum**:
A nominal type defining a closed set of unit, tuple, or record variants. Enums are Rill's v0 sum-type mechanism; matching can distinguish their variants at runtime.
_Avoid_: untagged union, structural union, backing-value list

**Enum variant**:
One of an enum's named alternatives, optionally carrying payload data. Its qualified name also denotes a distinct variant type, usable in annotations and overload parameters.
_Avoid_: string tag, singleton payload value, independent record member

**Error wrapper**:
An Err<E> value owning a typed error, its construction origin and an optional retained cause. Forwarding preserves it unchanged; explicit wrapping adds a new outer error.
_Avoid_: Report, propagation trace, mandatory message

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

**Inputs**:
An independently owned snapshot of a program's startup arguments and environment, exposed through std::env. It preserves byte data and is distinct from live process-global state or parsed application configuration.
_Avoid_: live environment, ambient configuration, shared process handle

**Instant**:
A Rill clock-specific elapsed-time reading, distinct from a wall Timestamp. It can mark a deadline but is not a persistent or cross-clock time identifier.
_Avoid_: Unix timestamp, wall time, deadline guarantee

**JSON number**:
A Number value preserving one validated JSON numeric token, including its spelling, without implicit primitive rounding. It is not a primitive float or an arbitrary-precision arithmetic value.
_Avoid_: f64 number, quoted numeric string, bigint

**JSON object**:
An Object with unique decoded member names and preserved insertion order. It is distinct from Rill's unordered Map and nominal application records.
_Avoid_: reflected record, unordered object map, class instance

**JSON value**:
An owned Value of one of JSON's fixed alternatives, independent of its parsed input buffer. Read-only or mutable views retain that owner's ordinary borrowing restrictions.
_Avoid_: borrowed DOM, reflected application value, shared snapshot

**Lock guard**:
A non-copyable value that borrows a mutex and owns responsibility for releasing its lock. Its disposer unlocks; protected borrows depend on it, and ownership stays in the locking goroutine.
_Avoid_: shared lock owner, raw lock, OS-thread-affine guard

**Map**:
Rill's owned hash-based key/value collection, written `Map<K, V>`, with no iteration-order guarantee. It is distinct from the lowercase `map(...)` transformation function.
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

**Numeric conversion**:
A conversion between explicitly sized numeric types. `from`/`into` are infallible, while `try_from`/`try_into` return a Result; infallibility does not promise losslessness.
_Avoid_: as cast, type assertion, implicit coercion

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

**Retained cause**:
A uniquely owned underlying error preserved with its origin and complete cause chain. Retention requires display, borrow-freedom, sendability and shareability evidence, independently of the outer error.
_Avoid_: erased unchecked error, stack trace, shared Report

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
A `Shared<T>` handle gives a value more than one owner by explicitly counting its owners. Ordinary values are not implicitly counted, and shared ownership alone grants no mutable access to the payload.
_Avoid_: Arc, Rc

**Span**:
A one-sign temporal value retaining calendar and time units. It is distinct from SignedDuration: interpreting calendar units as elapsed quantities needs explicit context.
_Avoid_: milliseconds, fixed duration, fieldwise sum

**Signed duration**:
A SignedDuration expressing an exact signed elapsed quantity, without calendar-unit meaning. Negative values are valid quantities but not relative wait requests.
_Avoid_: calendar span, timeout sentinel

**String**:
An owned value of valid UTF-8 text, written `string`. Named string bindings move unless explicitly copied.
_Avoid_: String class, byte buffer, implicitly shared string

**String view**:
Read-only borrowed text written `&string`, which may cover a whole string or substring. Duplicating its descriptor retains the backing-source dependencies rather than producing owned text.
_Avoid_: str type, owned substring, shared owner

**Timestamp**:
A Unix wall-time value with nanosecond precision, distinct from a monotonic Instant and a civil date/time. A numeric representation does not imply clock accuracy.
_Avoid_: monotonic instant, local datetime, implicit timezone

**Zone snapshot**:
The immutable rules retained by a TimeZone or Zoned value. A database update or matching zone name does not change or reproduce those retained rules.
_Avoid_: current database, numeric offset, serialized rules
