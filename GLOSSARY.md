# Rill

Rill is a functional-first programming language with TypeScript-style syntax. It compiles to native code and WebAssembly through LLVM, and it is memory safe without a tracing garbage collector.

## Language

**Analysis context**:
A selected Rill entry file, target and compiler-option combination that determines the loaded program graph and meaning of editor results. A workspace folder or search index is not that context.
_Avoid_: guessed project root, merged program, workspace folder

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
An intrinsic cooperative request to stop a goroutine while completing its required cleanup, outside ordinary Result. Explicit cancellation-token requests are separate observable state: token-aware operations can return typed outcomes after required release. Neither mechanism implies rollback or immediate termination.
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

**Controlled TCP view**:
A borrowed TCP view carrying explicit network control for the I/O operations it permits. It retains access dependencies without owning closure of the underlying resource.
_Avoid_: new connection, owning socket, ambient context

**Controlled TLS view**:
A borrowed TLS-stream view carrying explicit network control for its canonical I/O. It retains stream, clock and token dependencies without owning consuming close.
_Avoid_: owning connection, ambient context, independent TLS direction

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

**I/O progress**:
Bytes appended to a read destination or accepted from a write input by the current stream layer. Acceptance does not imply durable storage, peer receipt or application acknowledgement.
_Avoid_: delivered bytes, committed transaction, rollback guarantee

**Instant-domain validation**:
The std::time::validateInstantDomain check that a Clock and Instant share immutable domain identity, without reading time, registering a timer, exposing IDs or testing timeline health. It permits SQL preflight without privileged field access.
_Avoid_: clock read, deadline wait, public domain ID

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

**Native resource offer**:
A provider-owned connection, statement or transaction responsibility awaiting explicit adoption by a Rill owner. Completion or copied token bytes do not themselves transfer ownership.
_Avoid_: returned pointer, adopted owner, completed cleanup

**Native SQL provider**:
An external embedded driver's narrowly approved platform bridge for native resources and their ownership/control guarantees. Its confirmed design and reference checks are not a working integration or authority for general FFI or public thread pinning.
_Avoid_: standard SQL interface, runtime driver registry, working SQLite support

**Network control**:
Rill's explicit clock, optional absolute deadline and cancellation-token descriptor for a networking operation. Its captured snapshot retains ordinary borrow dependencies, not hidden ownership or an ambient request context.
_Avoid_: timeout duration, goroutine cancellation, live resource setting

**Numeric conversion**:
A conversion between explicitly sized numeric types. `into`/`from` are infallible, while `tryInto`/`tryFrom` return a Result; infallibility does not promise losslessness.
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

**Owning SQL row**:
A result row whose data remains valid after advancing or closing its result stream. Views into that data still depend on the row owner.
_Avoid_: cursor view, borrowed protocol buffer, entity mapping

**Path**:
A native filesystem-location spelling, distinct from an opened object or access authority. Lexical equality compares the spelling, not whether two paths resolve to the same object.
_Avoid_: filename string, canonical identity, sandbox boundary

**Prelude**:
The fixed primitive types, intrinsic predicates, and canonical copy/dispose declarations available without explicit imports. It introduces no runtime global bindings.
_Avoid_: wildcard std import, module initializer

**Private field**:
A record field accessible only in its defining module. Its presence prevents external whole-record construction or rebuilding, regardless of aliases.
_Avoid_: protected field, project-private field

**Provider binding contract**:
The narrowly approved agreement admitting a particular external provider's native implementations and ownership roles into a Rill build/runtime. Supplying metadata alone does not grant this authority.
_Avoid_: general FFI, native plugin registry, ordinary overload declaration

**Proper tail call**:
A Rill call that needs no eliminated caller storage or post-return disposal/joins. It guarantees bounded activation growth in all supported builds, not merely a final-looking expression.
_Avoid_: final-looking call, optional tail-call optimization

**Record**:
A value of a declared nominal type with named fields, constructed with its type name as in `Point { x: 0, y: 0 }`. Matching fields do not make different record types interchangeable; anonymous structural records do not exist.
_Avoid_: object literal, structural record

**Reborrow**:
A temporary loan of existing borrowed access, restricting conflicting use of the original access until the loan is no longer needed.
_Avoid_: copy, new owner

**Retained cause**:
A uniquely owned underlying error preserved with its origin and complete cause chain. Retention requires display, borrow-freedom, sendability and shareability evidence, independently of the outer error.
_Avoid_: erased unchecked error, stack trace, shared Report

**Reuse candidate**:
A SQL session eligible for validation and reset, not one already proven clean or guaranteed to remain connected. Reuse candidacy says nothing about whether a failed operation may safely be repeated.
_Avoid_: clean connection, retry permission, rollback acknowledgement

**Routine**:
The handle `spawn(() => f(x))` returns for the goroutine it starts. `stop(&routine)` requests cancellation and waits for termination and cleanup; freeing the handle does not stop it.
_Avoid_: task, future, promise, join handle

**Saved activation budget**:
Rill's per-routine Wasm limit on retained suspension/call state, distinct from its addressable stack and ordinary program data.
_Avoid_: snapshot of all borrowed storage, total heap quota

**Sendable**:
Safe to transfer ownership or permitted exclusive access between independently executing goroutines, as required by `sendable(T)`. This does not imply borrow-freedom or safe concurrent read-sharing.
_Avoid_: borrow-free, copyable, thread-affine

**Shareable**:
Safe for read-only access by concurrently executing goroutines, as required by `shareable(T)`. This grants neither mutable access nor copyability.
_Avoid_: shared owner, mutable, copyable

**Shared**:
A `Shared<T>` handle gives a value more than one owner by explicitly counting its owners. Ordinary values are not implicitly counted, and shared ownership alone grants no mutable access to the payload.
_Avoid_: Arc, Rc

**SQL control**:
Owned std::sql::SqlControl: an optional clock/absolute Instant and a CancellationToken, consumed per operation. Explicit copies retain the same target and state, not a restarted timeout. Sessions and idle transactions do not inherit control; cursors retain query control and mandatory cleanup ignores external requests.
_Avoid_: ambient context, timeout duration, transaction lifetime

**SQL execution observation**:
NotSubmitted or MayHaveExecuted for the requested operation/admitted query, carried by typed cancellation/deadline/clock errors. It does not establish the outcome of prior transaction work, rollback, replay safety or session reuse.
_Avoid_: transaction outcome, safe retry, abortion acknowledgement

**SQL phase error**:
Independently typed query-start/iteration/close or transaction-finalization/cleanup errors, retaining original Err envelopes and both primary and cleanup failures. Scoped helpers prove required borrow-freedom before destroying local owners; direct caller-dependent errors are not universally prohibited.
_Avoid_: unified native error, erased cause, cleanup success

**SQL result stream**:
A cursor retaining the session and execution dependencies needed to produce owning rows. Active advancement checks retained query control even for buffered rows; terminal advancement stays None. Exhaustion alone does not establish safe session reuse.
_Avoid_: owning row, materialized result list, reusable connection

**SQL temporal parameter**:
Date/Time/DateTime from std::time::civil or Timestamp from std::time, explicitly packed into Parameter with faithful checked native mapping. No silent precision/range/sentinel repair, inferred zone/unit or calendar/elapsed conversion. Checked temporal failures retain their original Err<TimeError>.
_Avoid_: monotonic Instant, implicit zone, database interval

**Span**:
A one-sign temporal value retaining calendar and time units. It is distinct from SignedDuration: interpreting calendar units as elapsed quantities needs explicit context.
_Avoid_: milliseconds, fixed duration, fieldwise sum

**Signed duration**:
A SignedDuration expressing an exact signed elapsed quantity, without calendar-unit meaning. Negative values are valid quantities but not relative wait requests.
_Avoid_: calendar span, timeout sentinel

**Stack capacity**:
Rill's usable addressable stack budget selected for a routine before it starts, independently of the Wasm saved activation budget. It is not a recursion-depth count, resident-memory guarantee, or total-memory limit.
_Avoid_: engine-stack limit, heap quota

**String**:
An owned value of valid UTF-8 text, written `string`. Named string bindings move unless explicitly copied.
_Avoid_: String class, byte buffer, implicitly shared string

**String view**:
Read-only borrowed text written `&string`, which may cover a whole string or substring. Duplicating its descriptor retains the backing-source dependencies rather than producing owned text.
_Avoid_: str type, owned substring, shared owner

**TLS accepted prefix**:
The current-input plaintext a TLS stream has accepted into its bounded owned storage. It is distinct from produced ciphertext, transport acceptance and peer application receipt.
_Avoid_: delivered message, acknowledged bytes, ciphertext count

**TLS reference name**:
The independently chosen DNS or IP service identity a Rill client verifies, represented by ServerName. It is not the connected address, resolver canonical name or server-observed SNI.
_Avoid_: routing hostname, authenticated client account, resolver identity

**Unique violation**:
A reported primary-key or unique-key collision, including composite keys whose values are unique as a tuple. The category does not guarantee that every backend supplies the key's column names.
_Avoid_: duplicate UUID, individually unique columns, automatic retry

**Timestamp**:
A Unix wall-time value with nanosecond precision, distinct from a monotonic Instant and a civil date/time. A numeric representation does not imply clock accuracy.
_Avoid_: monotonic instant, local datetime, implicit timezone

**Zone snapshot**:
The immutable rules retained by a TimeZone or Zoned value. A database update or matching zone name does not change or reproduce those retained rules.
_Avoid_: current database, numeric offset, serialized rules
