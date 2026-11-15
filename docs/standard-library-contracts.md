# Rill v0 language-required standard-library contracts

Confirmed design input for [#23](https://github.com/Reilley64/rill/issues/23), summarized by [ADR 0015](adr/0015-language-required-library-contracts.md). This is **not** an implemented library, accepted feature RFC, or checker/backend proof. #28 writes and validates the feature RFCs; #24 owns compiler diagnostics. JSON has its own approved scope/design ticket, [#31](https://github.com/Reilley64/rill/issues/31), and does not gate closing #23.

Read the newer decisions over historical examples: #18 explicit widths/UTF-8; #19 enums, not unions/singleton types; #29 origins and explicit causes, not propagation reports; #22 bounded stacks/proper tails. This contract refines #29's cause-retention boundary and #15/#17's copy/disposal interfaces explicitly below. ADRs 0013/0014 and their fixtures were prepared in separate sessions and may not yet be present in this checkout. Their GitHub decisions remain source inputs; their absence is not evidence those policies were dropped.

## Reading the catalogue

[#41 / ADR 0017](adr/0017-user-implementable-conversions.md) adds language-wide `implement` syntax for family bodies: `implement show(value: &Local): string { ... }` and `implement<T> show(value: &Box<T>): string where show(T) { ... }`. Body-less family declarations and standalone definitions retain `function`. Infer omitted family arguments from implementation signatures, not bodies. Implementations inherit family visibility. Only the canonical conversion families have the destination/source key exception described in §9; ordinary return-only dispatch remains forbidden.

Signatures below specify public operations, not complete source files: repeated operation signatures describe implementations of **one** family, not multiple conflicting body-less declarations. Each overload family has one declaration. All ordinary implementations preserve its parameter count, reference structure as specialized through type parameters, and return pattern. No dispatch on return type alone, constraint truth, runtime value, import order, or dead-code elimination is added.

A loose declaration deliberately exposes its result type parameters when parameter specialization determines the result. For example:

```rill
function map<C, F, R>(from: C, transform: F): List<R>;
```

Its two input-pattern implementations are `(List<T>, F) -> List<R>` with `callableMut(F, [T], R)` and `(&List<T>, F) -> List<R>` with `callableMut(F, [&T], R)`. Determine the input pattern/element access before contextualizing the callback, including an overloaded name such as `show`. Do not guess callback capabilities by inspecting eventual monomorphized bodies. Ambiguous/missing inference requires an annotation; no look-ahead from later statements.

`where family(A, B)` identifies the canonical family's type arguments, including result-related arguments where present; it is not a structural name-based promise. Requirements retain the selected signature, invocation/access, property and dependency evidence. Constraint differences cannot distinguish duplicate parameter patterns. Public requirements name exported canonical declarations. Imported aliases preserve identity, but compiler metadata does not inject names into caller source.

The arity schemas below describe exactly the **2-, 3-, and 4-item implementations**, not source-level variadic generics, ellipsis syntax, or optional parameters. Explicit tuples and List remain distinct types; candidate/contextual inference must not silently reinterpret an ambiguous literal based only on a desired return. Explicitly typed source expressions/annotations resolve such ambiguity; #28 must validate direct tuple callback literals as well as named inputs.

## 1. Public paths and the fixed prelude

| Home | Required exports |
| --- | --- |
| Fixed prelude | Numeric primitives; bool, char, string, void, never; intrinsic predicates; canonical copy/dispose declarations |
| `std` | log, panic, spawn, spawnWithStack, stop |
| `std::collection` | List, Map type/factory, length, get, getMut, first, push, pop, insert, remove, map, sum, join |
| `std::compare`, `std::hash` | eq; hash |
| `std::iter` | iterate, next and the standard iterator/entry types described below |
| `std::option` | Option, map, andThen, isSome, asRef, asMut |
| `std::result` | Result, Ok, Err type/construction operation, map, mapErr, andThen, isOk, ok, err |
| `std::error` | Error, error, Cause, SourceLocation, display, value, intoValue, origin, cause, formatLocation, formatError, formatCause |
| `std::format` | show |
| `std::convert` | into/from, tryInto/tryFrom, ConversionError |
| `std::number` | parseNumber, ParseError; wrappingAdd/Sub/Mul, saturatingAdd/Sub/Mul |
| `std::bytes` | Bytes type/factory, length, slice, get, getMut, append, clear, truncate, reserve, intoList, copyFromList, copyToList |
| `std::string` | length, slice, append, bytes, viewUtf8, decodeUtf8, decodeLossy, chars, charIndices, CharAt, Utf8Error |
| `std::char` | codePoint, fromCodePoint |
| `std::shared` | Shared, shared, view |
| `std::mutex` | Mutex, LockGuard, mutex, lock |
| `std::channel` | Sender, Receiver, SendError, channel, send, receive, after, select, unsent, intoUnsent |
| `std::routine` | Routine, AllFailed, failures, join, all, allSettled, race, any |
| `std::time` | sleep |
| `std::io` | IoError, IoErrorKind, kind, nativeCode, readStdin, writeStdout, writeStderr, flushStdout, flushStderr; read, write, flush, close, readExact, writeAll, readToEnd, IoFailure, ReadExactError, ReadToEndError, BufferedReader, BufferedWriter, bufferedReader, bufferedWriter, intoParts, BufferedFlushError, BufferedCloseError |
| `std::path` | Path type/factory, PathError, PathComponent, Components, pathBytes, pathText, isAbsolute, join, parent, fileName, components |
| `std::sql`, `std::sql::driver` | **Partial #40 design, not a complete required callable surface:** common Parameter/Decimal, typed errors/codecs, canonical driver families and explicit helpers; deadline/time integration/final arities/native-provider prerequisites remain open; see the partial SQL contract |
| `std::fs` | readFile, writeFile; ReadFile, WriteFile, ReadWriteFile, WriteOptions, CreateMode, WriteBehavior, SeekFrom, openRead, openWrite, openReadWrite, seek, sync, syncDirectory; Metadata, FileKind, metadata, symlinkMetadata, readLink, createSymlink, currentDir; DirEntry, ReadDir, readDir, createDir, createDirs, removeFile, removeDir, rename; TempFile, TempDir, TempFileCloseError, tempFile, tempDir, tempPath, keep; uses canonical std::io::IoError |

Library types/functions need explicit imports. Primitive `string`/`char` do not; their modules contain helpers, not replacement primitive types. Canonical interpolation/iteration/cleanup can load required metadata without source bindings. `map` in collection/result/option and `join` in collection/routine are different families; import aliases resolve source collisions. No implicit broad std import, mutable exported globals, module initializers, methods, traits or user-defined conversion operators are added.

## 2. Callable and ownership evidence

The fixed compiler predicates include `noBorrow(T)`, `sendable(T)`, `shareable(T)` and:

```rill
where callable(F, [A], R)      // invoke through &F
where callableMut(F, [A], R)   // invoke through &mut F
where callableOnce(F, [A], R)  // invoke by consuming F
```

Brackets list argument types; `[]` means zero arguments. Read-only implies mutable-repeatable and consuming capability; mutable-repeatable implies consuming, never the reverse. Predicate satisfaction does not copy a callable or prove it borrow-free/pure/sendable/shareable. Borrowed callable forms forward only the capability their access permits: `&F` needs read-only invocation; `&mut F` can forward repeatable mutable invocation but cannot consume F's owned captures. No user overload may forge a predicate.

Generic bodies invoke F using only established access and requirements. Concrete capture evidence survives keeping F generic; an erased arrow annotation cannot invent environment evidence. Returning/storing an erased interface conservatively loses evidence not promised by it. Calls/joins/records/enum payloads preserve borrowed dependencies and exclusive restrictions before monomorphization.

Canonical declarations:

```rill
function copy<T>(value: &T): T;
function dispose<T>(value: T): void;
```

**Write `copy(&value)`, not the historical `copy(value)` convenience.** Copy returns the type's explicit copy result; it never silently moves an owner through a read-only borrow or erases embedded dependencies. Strings/Bytes copy their observed extent into independent ownership. List copies require `copy(T)`; Map copies require `copy(K), copy(V), eq(K), hash(K)`. Shared/Sender copy adds an owner of the same resource without copying its payload. No standard copy is supplied for guards, receiver ownership, retained causes, Err or Result. Copying an error value deliberately through a custom user operation is not a guarantee that the standard error envelope is clonable.

Retain the existing implicit-copy scalar/read-only-view/record/tuple classification. Extend it structurally to enum variants and parents: a parent needs every possible payload to qualify, not just its current runtime variant. Strings, owned collections, counted handles and exclusive references gain no implicit-copy exception. A static unit variant can qualify independently of a wider non-copying parent enum.

### Remaining-field cleanup and protected operations

This is a newly confirmed #23 rule, not a claim that #17 already specified it. A custom disposer performs extra cleanup; afterward the compiler disposes remaining initialized owned fields in normal established cleanup order. Moved/explicitly disposed fields are not disposed twice. The outer disposer is not recursively invoked on its own consumed parameter. An empty custom wrapper disposer cannot suppress a contained guard/owner/native obligation. Partial initialization, moves, branches and replacement slots must be tracked. Fatal panic still promises no cleanup; normal cancellation stays deferred through required disposal/installation/join work.

Protected identities are compiler/trusted metadata attached to genuine runtime types and operations, not their spelling or module path. Required guard release, count-preserving Shared/Sender copying/release and channel endpoint ownership, and outstanding-native-operation source retention are closed ownership actions. Private fields prevent construction, **not interception**.

During family collection normalize aliases/type parameters and reject any non-trusted implementation that could replace/tie a required protected action, including `dispose(LockGuard<Payload>)` in Payload's module and nested-payload specializations of counted owners. Negative capabilities such as guard copying cannot be enabled through a generic adapter or new specialization. Less-specific ordinary defaults remain usable only when they cannot displace the protected action. Apply checks even without calls and preserve canonical actions through expected function values, generic requirements and implicit cleanup. Broad orphan permissions and ordinary unprotected default specialization remain; do not seal every std family/type or impose an outermost-type orphan rule.

Standard wrapper layouts contain owned, non-forgeable resource state whose required action is preserved by field cleanup. Trusted native registrations own their borrowed-source obligations until completion/confirmed abort, even after cancellation/handle drop; cancellation requests are not release confirmations. Exactly-once state transitions, publication, replacement initialization and Wasm host-pointer stability still require #28/#30 validation. A private-field story or this registry alone is not a safety proof.

## 3. Collections, equality and traversal

```rill
// std::compare / std::hash
function eq<T>(left: &T, right: &T): bool;
function hash<T>(value: &T): u64;
```

Required bootstrap implementations supply canonical eq/hash for string, bool, char and the ten integer primitives, with the same equality as their built-in value comparison (exact UTF-8 equality for string). Hash algorithms are not prescribed. Read-only reference forwarding preserves the underlying key's equality/hash behavior; there is no automatic record/enum field-reflection fallback. Other key types need suitable implementations under ordinary family/orphan rules; this minimum does not promise primitive float-key hashing.

Map is an owned hash map with unspecified iteration order. Equal keys hash equally, and key equality/hash identity must remain stable while stored. Violations may break logical lookup behavior but cannot permit memory unsafety. Hash values need no cross-process/version stability. No user-defined operator-overloading feature is implied.

`List<T>`, `Map<K,V>` and their private storage are nominal library types; List contiguity is unspecified. `Map<K,V>()` is a zero-argument ordinary factory in the value namespace, separate from lowercase collection map. It requires `eq(K), hash(K)`; type arguments may come from immediate context. List literals still need sufficient expected/element types, especially an empty list. Neither factory supplies a default numeric width.

| Operation/input pattern | Result | Additional requirements |
| --- | --- | --- |
| length(&List<T>), length(&Map<K,V>) | u64 | none |
| get(&List<T>, u64), first(&List<T>) | Option<&T> | none |
| getMut(&mut List<T>, u64) | Option<&mut T> | none |
| push(&mut List<T>, T) | void | retained-input dependency checks |
| pop(&mut List<T>) | Option<T> | retained/result dependency checks |
| get(&Map<K,V>, &K) | Option<&V> | eq(K), hash(K) |
| getMut(&mut Map<K,V>, &K) | Option<&mut V> | eq(K), hash(K) |
| insert(&mut Map<K,V>, K, V) | Option<V> | eq(K), hash(K) |
| remove(&mut Map<K,V>, &K) | Option<V> | eq(K), hash(K) |
| map(List<T>, F) | List<R> | callableMut(F, [T], R) |
| map(&List<T>, F) | List<R> | callableMut(F, [&T], R) |
| sum(&List<N>) | N | supplied numeric-primitive implementations |
| join(List<string>, &string), join(&List<string>, &string) | string | separator borrowed; independent output |

The matching family declarations use fixed shapes: `length<C>(&C):u64`; `get<C,Q,R>(C,Q):Option<R>`; `getMut<C,Q,R>(C,Q):Option<R>`; `first<C,R>(C):Option<R>`; `sum<C,N>(C):N`; `join<C>(C,&string):string`. Single-pattern push/pop/insert/remove can be ordinary generic functions. Implementation-specific requirements above are not duplicate same-pattern bodies differentiated by constraints.

Map insertion without an equal key stores both inputs and returns None. Otherwise retain the old key, dispose the incoming unused key, replace the value and return the old value. Removal returns the value and disposes the removed key. None means ordinary absence/out-of-range, not an exceptional error. No mutable stored-key access; changing a key requires removal/reinsertion. Recovering a whole removed entry and panicking access conveniences are outside the minimum surface.

Map/transform callbacks run once per element, in list order, sequentially, and may suspend/effect. The operation owns F; reuse requires explicit `&callback` or `&mut callback`. Results may retain valid caller/source borrows, but not destroyed owned-element storage or the operation's local callback environment. A once-only callback cannot implement repeated mapping. Borrowed map does not copy its elements; in-place mutation is traversal/getMut, not a third map meaning. No persistent snapshots or implicit storage sharing. Allocation failure is fatal, not a recoverable allocation Result. Public u64 sizes do not guarantee target addressability/capacity.

### Iteration

```rill
// std::iter, also used canonically by for ... of
function iterate<C, I>(source: C): I;
function next<I, T>(iterator: &mut I): Option<T>;
```

`for ... of` evaluates its source once, constructs an iterator and repeats next until None; user-defined containers may implement the same canonical families under ordinary checking/orphan rules. The compiler loads these identities for syntax without binding their names in source; user extensions/generic requirements import them explicitly. Standard nominal iterator types and yielded types are:

| Source | Iterator | Yield |
| --- | --- | --- |
| List<T> | OwnedListIter<T> | T |
| &List<T> | ListIter<T> | &T |
| &mut List<T> | ListIterMut<T> | &mut T |
| Map<K,V> | OwnedMapIter<K,V> | Entry<K,V> |
| &Map<K,V> | MapIter<K,V> | EntryRef<K,V> |
| &mut Map<K,V> | MapIterMut<K,V> | EntryMut<K,V> |
| Bytes / &Bytes | OwnedBytesIter / BytesIter | u8 |
| Chars / CharIndices | same owned iterator value | char / CharAt |
| &Receiver<T> / &Shared<Receiver<T>> | ReceiverIter<T> / SharedReceiverIter<T> | T until closed/drained |

Iterator/entry names are exported from std::iter except Chars/CharIndices/CharAt from std::string. Entry, EntryRef and EntryMut are named records with respectively `(key:K,value:V)`, `(key:&K,value:&V)`, `(key:&K,value:&mut V)` fields. They are not anonymous structural types. Borrowed iterators retain their source; consuming ones own unyielded storage. Breaking/returning/canceling disposes remaining iterator state without losing obligations.

Exclusive yielded access must reach last use before advancing; no accumulating multiple exclusive references or arbitrary disjoint-index inference is promised. Structural modification conflicts with a live collection iterator. Shared yields/owned elements with embedded references retain the conservative dependencies of #15; #28 must explicitly validate transitive last use, source versus iterator-slot storage, callee exits and caller-dependent outputs. Iteration syntax is not an exemption from those rules.

## 4. Option, Result, origins and causes

```rill
// std::option
export enum Option<T> {
  Some(T),
  None,
}

// std::result; Err<E> is a separate nominal private-field record type.
export enum Result<T, E = Error> {
  Ok(T),
  Err(Err<E>),
}
```

The public type `std::result::Err<E>` and value construction name Err coexist in separate namespaces. A wrapper owns E, its construction SourceLocation and optional Cause. Its fields are private; public borrowed accessors inspect them. User error types need no injected message/cause fields. Default E is canonical std::error::Error without injecting that source binding.

| Operation | Signature/contract |
| --- | --- |
| Ok | ordinary generic helper `Ok<T,E=Error>(T):Result<T,E>`; contextual inference, no error construction |
| Err(value) | closed construction operation, produces Result<T,E>; requires display(E); T needs context/explicit arguments |
| Err(value, cause) | same, consumes Err<C> as cause; also requires display(C), noBorrow(C), sendable(C), shareable(C) |
| value | `value<E>(&Err<E>):&E` |
| intoValue | `intoValue<E>(Err<E>):E`; intentionally disposes origin/cause wrapper state |
| origin | one `origin<W>(&W):&SourceLocation` family for Err<E> and Cause |
| formatLocation | `formatLocation(&SourceLocation):string`; independent owned rendering of the opaque construction location |
| cause | one `cause<W>(&W):Option<&Cause>` family for Err<E> and Cause |
| error | `error(string):Error`; owns text, no mandatory message on other E |
| display | canonical `display<E>(&E):string` family; one payload only |
| formatError | `formatError<E>(&Err<E>):string where display(E)`; full origin/chain |
| formatCause | `formatCause(&Cause):string`; full retained cause chain |

Err is a specific closed compiler-recognized 1-/2-argument construction exception, **not** a general variable-arity/default-argument overload feature. Aliases preserve canonical identity and helper construction sites are attributed to the source construction expression, not a hidden helper body. Infix wrapping attributes the outer node to its operator. Adapt construction as a first-class callable with an arrow, e.g. `(value) => Err(value)`; the special constructor itself is not an ordinary function value. Raw public Result.Err construction requires an already valid wrapper; it cannot fabricate private wrapper state.

`Err("text")` uses E=string, with standard display(string), when context accepts that error type. It does not convert to nominal Error; use `Err(error("text"))` for default Error. Postfix Result ? consumes/forwards an existing owned failure with the exact expected E. Borrowed queries cannot clone an owned envelope for propagation. Infix `inner ? outerValue` evaluates the Result once, skips the RHS on success, or evaluates one outer error value and returns a new wrapper caused by the original. It is Result-only, not a handler form or implicit constructor invocation. Option postfix propagation remains.

**Retained-cause boundary (new #23 refinement of #29):** E may carry checked borrows in a primary typed failure, and unchanged forwarding preserves them. Erasing/retaining E as a heterogeneous cause requires noBorrow(E), sendable(E), shareable(E), display(E). This applies to Err(value,cause) and infix wrapping. It is a compile-time requirement, not a dynamic flag test or an assumption from the outer type. A borrowed/affine error can be propagated directly or explicitly converted into a suitable owned error, not hidden behind a safe-looking outer error.

Cause is a sealed, uniquely owned, non-copyable representation preserving each retained value, origin, next cause, formatter and disposal metadata. It is not a public dyn/trait/downcast facility or counted Report. Its invariant provides noBorrow/sendable/shareable evidence independently of the visible outer E. Private runtime metadata/canonical identities must enforce construction and cleanup, including through generic/erased interfaces. SourceLocation is opaque; formatLocation is its minimal public observation operation, not a stable machine-readable path/line/column record or a new coordinate-format contract. It identifies the original source construction site using the compiler's source map. SourceLocation owns its required diagnostic data (or uses justified immutable compiler metadata), never a borrowed input/temporary disguised as borrow-free state. No standard Err/Result copy is supplied merely because E/T are copyable; an optional Cause can hold non-copyable state.

`display` receives a borrow, returns owned string and may suspend/effect. Supply it for primitive error values, default Error and every library error; custom E needs its own canonical implementation, not reflected field printing. Standard show(Err<E>) bridges to formatError with display(E); show(Cause) bridges to formatCause. Ordinary show identity/orphan/reference-forwarding rules remain, not a sealed formatting family. Construction records origins only: returns, postfix ?, map and andThen do not append history. Full standard rendering visits every explicit cause, using bounded-stack traversal per #22; no promised automatic full backtrace or eliminated-tail reconstruction. User formatters/disposers remain callbacks outside the standard recursion bound. #24 owns compiler-diagnostic presentation; runtime error rendering here must not require users to parse a fixed text format.

### Consuming combinators and borrowed queries

In std::option: `map<T,U,F>(Option<T>,F):Option<U>` requires callableOnce(F,[T],U); `andThen<T,U,F>(Option<T>,F):Option<U>` requires callableOnce(F,[T],Option<U>). isSome borrows and returns bool; asRef(&Option<T>):Option<&T>; asMut(&mut Option<T>):Option<&mut T>.

In std::result: `map<T,U,E,F>(Result<T,E>,F):Result<U,E>` requires callableOnce(F,[T],U); `andThen<T,U,E,F>(Result<T,E>,F):Result<U,E>` requires callableOnce(F,[T],Result<U,E>); `mapErr<T,E,D,F>(Result<T,E>,F):Result<T,D>` requires callableOnce(F,[E],D), display(D). A callback runs at most once, only for its selected variant. map/andThen forward failures intact. mapErr replaces E with D while moving the existing origin/cause into the new envelope, not constructing another causal layer. No implicit E conversion or new propagation site.

`isOk(&Result<T,E>):bool`, `ok(&Result<T,E>):Option<&T>`, and `err(&Result<T,E>):Option<&Err<E>>` inspect without moving/cloning. Do not invent a borrowed Result rewrap that would require copying the unique cause. Canonical private implementations can preserve envelope fields; callers need no representation privileges. Generic requirements and output dependencies remain explicit before monomorphization.

## 5. Shared ownership, mutexes and native retention

```rill
shared<T>(value: T): Shared<T>
view<T>(owner: &Shared<T>): &T
mutex<T>(value: T): Mutex<T> where noBorrow(T), sendable(T)
```

Shared creates explicit counted ownership, not a persistent collection snapshot. Borrow-containing payloads retain their dependencies; creation does not invent noBorrow/copy/sendability. Read-only view never grants &mut T. Cross-goroutine Shared use requires sendable(T), shareable(T); property/loan rules still distinguish borrowing from ownership.

One `lock<M,T>(source:M):LockGuard<T>` family supplies &Mutex<T> and &Shared<Mutex<T>> input implementations, requiring noBorrow(T), sendable(T). These are explicit forwarding implementations, not general auto-dereference. A guard borrows/retains its source, cannot copy/send ownership, is read-shareable when T is shareable, and has only the approved built-in dereference. Mutable guard access can modify the payload; protected views prevent premature guard/source release. Dispose unlocks, not destroys T. No callback-only withLock, raw unlock, borrowMut or user-defined Deref protocol is introduced.

Trusted primitive state retains active waiters/native borrowers through actual completion/confirmed abort; explicit/automatic disposal cannot bypass its release state machine. Owner counts, lock publication, acquire-to-guard handoff and channel commits must remain correct across suspension/cancellation and all code paths. Source ownership alone does not establish pointer stability across Wasm memory.grow. #30 remains an explicit backend gate, not established support.

## 6. Background and scoped work

```rill
// root std
spawn<F>(work: F): Routine
  where callableOnce(F, [], void), noBorrow(F), sendable(F)
spawnWithStack<F>(work: F, stackBytes: u64): Routine
  where callableOnce(F, [], void), noBorrow(F), sendable(F)
stop(routine: &Routine): void
```

Spawn owns F, never accepts erased capture evidence by assumption, and returns Routine rather than Result. Stop joins execution/cleanup; repeated/concurrent calls share completion, self-stop panics, and dropping Routine does not stop/join. No global exit join is added.

#22's source/build capacity contract is preserved: u64 usable addressable bytes; zero/below-minimum/excessive/unrepresentable/effective-alignment-invalid dynamic settings panic, build settings reject; reservation/backing failure is fatal. The runtime publishes actual target limits and checked upward alignment, not a portable recursion-depth guarantee. `--stack-size` sets program default, `--entry-stack-size` entry only, with equivalent run/test compile settings; no manifest/environment/optional/named parameter mechanism. Ordinary/scoped children use program default, not parent overrides. Native default usable capacity is 1 MiB; Wasm addressable and saved budgets default separately to 1 MiB per routine. `--saved-stack-size` affects Wasm program-wide saved state, is rejected on native, and does not set host-engine limits. Handle drop/cancel requests do not authorize reclamation while borrowers/cleanup remain. Full support is gated by #30; these declarations do not establish a compatible tail-call/Asyncify pipeline.

Scoped helpers live in std::routine and take one tuple of 2, 3 or 4 owned callable values. Fi may itself be an explicit caller borrow; no blanket noBorrow requirement is imposed on scoped jobs/results. Each job requires callableOnce(Fi,[],Ri), sendable(Fi), sendable(Ri). Borrowed captures need the corresponding access/property evidence; results cannot reference child-local/destroyed storage. Every helper joins all child execution and cleanup before returning or completing cancellation.

| Family declaration | 2-item implementation schema; extend structurally to 3/4 |
| --- | --- |
| join<W,R>(work:W):R | [F1,F2] -> [R1,R2] |
| all<W,R,E>(work:W):Result<R,E> | callable results Result<T1,E>/Result<T2,E> -> Result<[T1,T2],E> |
| allSettled<W,R>(work:W):R | callable results Result<T1,E1>/Result<T2,E2> -> [Result<T1,E1>,Result<T2,E2>] |
| race<W,R>(work:W):R | both callable results R -> R |
| any<W,T,E>(work:W):Result<T,AllFailed<E>> | both callable results Result<T,E>; also display(E) |

Schemas require sendable for each whole job result; their T/E components must supply the evidence needed to establish it. Requirements do not select duplicate implementations with identical parameter patterns. all has no plain-return implementation; use join. allSettled's shape specifically collects Result-valued jobs, preserving individual E types; it is not implicit runtime type detection.

all chooses its first observed failure unchanged, cancels/joins others, or returns all successes in input order. race chooses the first completion even if R is Result and it is Err. any chooses the first success, canceling/joining remaining work; if all fail, it constructs a new AllFailed<E> aggregate preserving every original Err<E> in input order. AllFailed is an opaque named record; `failures<E>(&AllFailed<E>):&List<Err<E>>` inspects it, and its display implementation formats the preserved entries. Simultaneous ready outcomes have no specified winner. Losing results/closures are disposed under normal ownership. Fatal panic is not caught as Result.

No hidden runtime dispatcher based on callback result types. No variadic generics or dynamic-sized work-list helpers in the required v0 surface. Nested fixed groups and explicit worker queues remain possible. Cleanup/join latency is not bounded; timeout races are not hard deadlines. Worklists/explicit error data may grow; that is not hidden activation growth or a total-memory quota.

## 7. Channels, selection and timers

```rill
channel<T>(capacity: u64): [Sender<T>, Receiver<T>]
  where noBorrow(T), sendable(T)
send<T>(to: &Sender<T>, value: T): Result<void, SendError<T>>
  where noBorrow(T), sendable(T)
```

Zero capacity is unbuffered rendezvous; positive capacity is bounded. Capacity is explicit, no no-argument/default-argument constructor and no unbounded mode. Unsupported allocation/capacity is not a recoverable out-of-memory API; runtime target limits/checks still apply. Last Sender closes the channel; buffered messages drain before None. Last receiver ownership release makes pending/future sends fail and disposes undelivered buffered ownership exactly once. Sender copy preserves the same channel/count; receivers are shared through Shared, not duplicated by a standard receiver copy.

One `receive<S,T>(from:S):Option<T>` family implements &Receiver<T> and &Shared<Receiver<T>>, with noBorrow(T), sendable(T). This is explicit forwarding. It moves a received message or yields None once closed/drained. A pending send owns its message; success commits one transfer, failure returns the undelivered T in opaque SendError<T>. `unsent<T>(&SendError<T>):&T` inspects; `intoUnsent<T>(SendError<T>):T` recovers ownership. display(SendError<T>) needs no display(T). Cancellation never rolls back a committed transfer; abandoning an uncommitted pending operation disposes its remaining message after unregister/confirmed abort. Deferred waits remain registered through required cleanup.

```rill
// std::time / std::channel
sleep(milliseconds: u64): void
after(milliseconds: u64): Receiver<void>
```

Timers use monotonic time, not calendar time. after produces one void notification then closes. Zero is immediately eligible, not a forced yield. Scheduling/native work/cleanup can delay completion indefinitely; checked duration/deadline representation must not silently wrap. No portable maximum scheduling-latency or hard-deadline promise.

### select without string tags

One `select<C,R>(cases:C):R` family implements 2/3/4-item tuples whose items are `[&Receiver<Ti>, Fi]`, requiring noBorrow(Ti), sendable(Ti), callableOnce(Fi,[Option<Ti>],R). For a Shared receiver use its explicit view; select need not erase arbitrary source/container types. Handlers run in the selecting goroutine, after the winning commit: no extra sendability/noBorrow constraint on F/R is invented merely for local selection. Captures/results still obey dependencies and cleanup.

```rill
enum Event {
  Beat(Option<string>),
  Idle,
}
let timer = after(50u64);
let next: Event = select([
  [&beatInbox, (value) => Event.Beat(value)],
  [&timer, (value) => Event.Idle],
]);
```

Only one receive commits, only its handler runs, and nonwinning cases consume no messages. Closed/drained receivers are ready with None; multiple ready cases have unspecified order, no source-order priority or starvation guarantee. A permanently closed case must be removed/restructured by the program if it should no longer compete. Handler failure as Result is just the chosen R; no implicit catching/conversion. Source/handler state remains valid through registration, commit, handler execution and losing-case cleanup. No tag("label",source), singleton label type, erased heterogeneous descriptor or race-of-receives implementation is required.

## 8. Contiguous bytes, UTF-8 and Unicode

Bytes owns a contiguous byte sequence; &Bytes is logical read access to a whole/subrange, not necessarily a pointer to an owning header. &mut Bytes exclusively accesses a whole owner. As with #18 string views, &mut &Bytes modifies a descriptor slot, not underlying byte permissions; generic dereference/reborrow/copy cannot fabricate an owner from a view. Source/input/temporary dependencies persist through nested values, iterators, suspension and native use.

| Home/operation | Signature |
| --- | --- |
| bytes factory | Bytes(List<u8>):Bytes; consumes source; reuse optional, never assumed |
| bytes length | length(&Bytes):u64 |
| bytes slice | slice(&Bytes,u64,u64):Option<&Bytes> |
| bytes get/getMut | get(&Bytes,u64):Option<u8>; getMut(&mut Bytes,u64):Option<&mut u8> |
| bytes append | append(&mut Bytes,&Bytes):void |
| bytes reuse (#32) | clear(&mut Bytes):void; truncate(&mut Bytes,u64):void; reserve(&mut Bytes,u64):void |
| bytes conversions | intoList(Bytes):List<u8>; copyFromList(&List<u8>):Bytes; copyToList(&Bytes):List<u8> |
| string length | length(&string):u64, counts UTF-8 bytes |
| string slice | slice(&string,u64,u64):Option<&string> |
| string append | append(&mut string,&string):void; valid UTF-8 preserved |
| string bytes | bytes(&string):&Bytes, no mutable escape |
| strict borrowed validation | viewUtf8(&Bytes):Result<&string,Utf8Error> |
| strict copying decode | decodeUtf8(&Bytes):Result<string,Utf8Error> |
| lossy copying decode | decodeLossy(&Bytes):string |
| scalar traversal | chars(&string):Chars; charIndices(&string):CharIndices |
| scalar queries | codePoint(char):u32; fromCodePoint(u32):Option<char> |

Slices use half-open byte bounds; reject reversed/out-of-bounds ranges, and for text reject non-scalar endpoints. Empty valid ranges succeed. Byte get may observe continuation bytes. Append cannot bypass conflicting loans/self-aliasing. Decoding errors own their diagnostic data; `Utf8Error` is a named record exposing `validUpTo:u64` and `errorLength:Option<u64>` (None for incomplete trailing input), with display. Strict borrowed success retains the original byte storage; copying success does not. Lossy decode replaces invalid input with U+FFFD rather than dropping it. Concrete invalid-sequence grouping follows the strict UTF-8 decoder's maximal valid-prefix/error-span contract and must be tested in #28, not confused with a view-cast.

[#32 / ADR 0018](adr/0018-streaming-and-buffered-io.md) adds Bytes reuse: clear sets length to zero while retaining reusable allocation; truncate shortens or does nothing when already shorter; reserve ensures room for at least the requested additional bytes without changing length. No exact capacity/allocator footprint or public uninitialized memory is exposed. Checked length/addressability/reservation failure follows existing fatal allocation policy; no native-size wrapping or bypass of conflicting views. Streaming reads append into a whole exclusively borrowed owner, not a new writable subrange API.

CharAt is a named record `{ offset:u64; value:char }`, exported from std::string. Chars/CharIndices are opaque source-retaining iterators, yielding scalars/CharAt through canonical next. fromCodePoint accepts Unicode scalars, excluding surrogates/out-of-range values, not merely printable/assigned characters. No grapheme indexing, normalization, locale collation, writable UTF-8 bytes, mutable substrings, implicit List contiguity or fabricated &List<u8> temporary headers.

String literals are expressions of string type, not singleton types. Uninterpolated literal expressions can be rematerialized into fresh logical owners/static backing under #18, but a named string binding moves unless explicitly copied. Optimizer knowledge does not grant implicit copying, noBorrow to a view, capture access or a safe numeric conversion. `copy(&owner)` copies owned text; `copy(view)` already passes a &string and copies the observed extent. To copy the descriptor itself use its read-only implicit-copy rules or the appropriately typed borrowed descriptor, preserving dependencies. These are not duplicate freeable owners or hidden reference counts.

## 9. Conversion, numeric compatibility, parsing and formatting

[#41 / ADR 0017](adr/0017-user-implementable-conversions.md) supersedes the closed-only conversion boundary from #18/#23:

```rill
// std::convert canonical user-extensible families
export function into<To, From>(value: From): To;
export function tryInto<To, From, E>(value: From): Result<To, E>;
// from aliases into; tryFrom aliases tryInto, same destination-first argument order.
// Historical try_into/try_from spellings are superseded, not extra implementation sets.

implement into<UserId>(value: string): UserId { /* body */ }
implement tryInto<UserId>(value: &string): Result<UserId, InvalidUserId> { /* body */ }

// closed std::number operation, not automatically registered as a conversion
parseNumber<T>(text: &string): Result<T, ParseError>
```

Only these canonical conversion families distinguish implementations by `(To, From)` rather than ordinary parameter patterns. E is the selected implementation's output evidence, never a selection key. Infer source/error family arguments from registration signatures. Calls use `into<To>(value)` / `tryInto<To>(value)` or an immediate unique concrete expected destination (including a Result success type). Do not choose a destination from the number of available implementations, later uses or a default. Source widths must be independently established. Concrete function values require resolved endpoints and the exact selected signature/error; there is no erased runtime registry.

Canonical requirements are `into(To, From)`, `tryInto(To, From, E)` and `parseNumber(T)`; aliases preserve identity. Generic bodies are checked before monomorphization using symbolic selected-signature/property/dependency evidence. Both endpoints must be named in requirements; unlike a call, no value supplies the source. E can be inferred at a concrete call, but exact equality is checked against the winning implementation. A generic body cannot assume an overridable fallback's concrete error and later substitute a differently typed specialization. Additional properties such as display(E) remain explicit. For example, `stringify<T>(value:T):string where into(string,T)` accepts any owned T with the conversion; a borrowed variant requires `into(string,&T)`.

Collect loaded implementations once. Reject identical canonical pair patterns regardless of error/constraints; aliases and generic renaming do not create distinct patterns. Choose the unique most-specific applicable source/destination pattern, allowing concrete specializations of generic defaults. Incomparable applicable patterns are ambiguous; constraints, expected errors and import order do not rank candidates. An expected-error mismatch cannot force fallback selection. The implementation module must own the family or an endpoint's outer nominal type after peeling references. Local wrappers count; foreign wrappers containing local types, bare generic parameters, local errors, constraints and aliases do not confer ownership. Ordinary families retain their existing orphan/duplicate rules and declaration-wide implementation visibility.

Built-in numeric pairs remain protected. Standard `into<T,T>` identity takes precedence for equal endpoints even over overlapping generic wrappers; explicit user infallible identity registrations are rejected. Identity preserves moves/copies/reference dependencies, not backing-storage cloning. No user infallible-to-fallible adapter or universal fallible identity is generated. Numeric safe/identity pairs retain standard fallible ConversionError entries; other fallible identity entries require explicit registration with a concrete E.

Owned T, &T and &mut T sources are distinct contracts. Calls explicitly move/borrow/copy; no implicit borrowed adapters. Destinations and errors preserve nested dependencies and exclusive access, reject local-source escape, and cannot fabricate noBorrow/sendable/shareable or extend lifetimes. Custom errors use canonical display for Err construction; exact-E propagation, explicit mapErr and existing cause/origin rules are unchanged. Infallible implementations may have ordinary effects or fatal panics; infallibility does not promise purity or losslessness.

No as, fallback width, arbitrary ordinary return-only implementations, source guessing or conversion/parser trait. No implicit casts, conversion chaining, structural mapping, automatic display/serialization/JSON/database registration or SQL-expression capture. Applications can register explicit conversions with their own semantics/errors; #31 JSON numeric behavior stays distinct. #40's [partial SQL contract](sql-contracts.md) explicitly adds bindInto/tryBind/get helpers invoking declared canonical conversions, not automatic conversion discovery/chaining. User JSON Value conversions remain explicit around parse/stringify; no new Serializable trait or structural mapping is added. Intended-valid and independent rejection cases are in [prototype 18](../prototypes/18-conversions.md).

Preserve #18: infallible integer pairs require full base-range inclusion; integer/float and float/float destinations use IEEE rounding and may lose precision/overflow to infinity. Float-to-integer truncates first, then checks range/non-finiteness; try supports safe pairs too and keeps Result shape. ConversionError is an enum with OutOfRange and NotFinite unit variants, supplied display/eq implementations and owned origin handling through Err. No rejected infallible call becomes a panic-catching helper.

ParseError has InvalidSyntax and OutOfRange unit variants with standard display/eq. Parsing consumes the whole text, no whitespace trimming/separators/suffix/base guessing; integer parsing does not truncate fractions. Float parsing accepts #18's canonical NaN/inf/-inf/-0 and decimal grammar. Finite rounded overflow errors; underflow may zero. Numeric destinations are the 12 established primitives only. Explicit radix parsing is outside the required surface.

Six ordinary same-type parameter families in std::number provide wrappingAdd/Sub/Mul and saturatingAdd/Sub/Mul for the ten integer primitives. Each declaration has `(T,T):T` shape; implementations specialize parameter types. Primitive floats have no supplied implementation. These helpers do not change checked ordinary arithmetic, bit/shift rules, constant-trap timing or introduce a general unsafe arithmetic mode. Numeric sum has the ordinary same-type arithmetic behavior, including checked integer overflow and input-order float evaluation; no reassociation promise is added.

Canonical std::format::show has `show<T>(&T):string`; output is owned, inputs borrowed. Supply primitive bootstrap renderers (not self-recursive interpolation), standard read-only/exclusive-reference observation forwarding, and error-wrapper bridges. Generic interpolation needs canonical show(T), formats each expression once immediately left-to-right, and retains temporary/resource obligations through suspension. No local namesake/reflected record fallback or implicit show source binding. Keep #18's exact primitive formatting/parse rules; special formatting can use explicit functions, not a template mini-language.

## 10. I/O, logging, errors and fatal failures

```rill
// std::io
readStdin(maxBytes: u64): Result<Option<Bytes>, IoError>
writeStdout(bytes: &Bytes): Result<void, IoError>
writeStderr(bytes: &Bytes): Result<void, IoError>
flushStdout(): Result<void, IoError>
flushStderr(): Result<void, IoError>

// std::fs
readFile(path: &string): Result<Bytes, IoError>
writeFile(path: &string, bytes: &Bytes): Result<void, IoError>
```

Stdin may return a short nonempty read; None means EOF, not temporarily unavailable input. A zero request returns Some(empty Bytes) without consuming input. A request is an upper bound, not a promise the target can allocate maxBytes; implementations may perform smaller reads without narrowing/wrapping the public u64. Writes attempt the complete sequence; errors/cancellation can leave a committed prefix and are not rollback or safe whole-buffer-retry promises. Flush does not imply filesystem crash durability. Shared standard-stream state is trusted synchronized runtime state, not named functions capturing exported runtime lets. Native operations retain buffers/path sources until completion or confirmed release, including deferred cleanup; Wasm pointers need the #30 validation, not owner retention alone.

readFile returns owned bytes. writeFile creates/truncates and may leave partial output after failure/cancellation, with no atomic replacement, rollback, durable-sync or file-locking promise. Embedded-NUL/invalid platform paths return InvalidInput rather than unsafe path truncation. That #23 baseline alone did not require a general streaming-file/reader-writer abstraction, networking, seeking or directory traversal. #32 adds the streaming foundations below; concrete handles/protocols remain in their own tickets. Text I/O composes bytes and explicit UTF-8 handling.

IoError is an opaque owned diagnostic type with `kind(&IoError):IoErrorKind` and `nativeCode(&IoError):Option<i64>`. IoErrorKind has NotFound, PermissionDenied, AlreadyExists, InvalidInput, Other plus #35's Unsupported, NotDirectory, IsDirectory, DirectoryNotEmpty variants; unmapped platform failures use Other. Optional native codes are platform-specific, not portable branching contracts. Its display uses owned diagnostic context; callers do not parse human-readable messages. IoError, ConversionError, ParseError and Utf8Error provide display and noBorrow/sendable/shareable evidence. Underlying native-code translation must not make cancellation a recoverable I/O error or fabricate successful completion.

Root log/panic each use one `(S)` declaration shape with string and &string input implementations. Log writes text plus newline, returns void, synchronizes whole messages (same stream synchronization as output operations), and has unspecified concurrent message order. Failure is fatal; fallible I/O is the alternative. It is text-only: show/interpolation is explicit, not generic reflection or implicit owner copying.

Source panic requires importing std::panic and returns never. Compiler-generated arithmetic/allocation/stack failures use canonical runtime metadata without a lexical panic binding. User shadowing cannot redirect them. Allocation/reservation failure is fatal, not ubiquitous recoverable allocation Results. #22 overflow uses bounded best-effort emergency reporting without user formatters/full trace/required allocation/unwind; host failure may prevent a report. Other fatal panic remains uncatchable, with no cleanup/global-exit guarantee.

### Streaming and buffering extension (#32)

**Q36 confirmed scoped restriction:** local probes/internal adapter buffers and consuming-close flush failures require the scoped noBorrow evidence listed below. The user explicitly approved this conservative rule after artifact validation; [ADR 0018](adr/0018-streaming-and-buffered-io.md#scoped-borrow-free-errors-confirmed-after-artifact-validation-q36) records its reason and limits. This is design approval, not a completed checker or soundness proof.

[#32 / ADR 0018](adr/0018-streaming-and-buffered-io.md) adds these ordinary canonical families in std::io; it leaves the preceding stdio/whole-file convenience signatures unchanged. Their existing partial-effect warnings do not retroactively become reliable progress counts. No global standard-handle ownership scheme, traits, methods, public dyn, return-only dispatch or new prelude binding is introduced.

```rill
function read<R,E>(source: &mut R, destination: &mut Bytes, maxBytes: u64)
  : Result<Option<u64>, IoFailure<E>>;
function write<W,E>(destination: &mut W, source: &Bytes)
  : Result<u64, IoFailure<E>>;
function flush<S,E>(stream: &mut S): Result<void,E>;
function close<S,E>(stream: S): Result<void,E>;

type IoFailure<E> = { transferred: u64; error: Err<E> };

// Generic helpers; requirements shown are part of each public contract.
readExact<R,E>(&mut R, &mut Bytes, u64): Result<void,ReadExactError<E>>
  where read(R,E), display(E)
writeAll<W,E>(&mut W, &Bytes): Result<void,IoFailure<E>>
  where write(W,E), display(E)
readToEnd<R,E>(&mut R, &mut Bytes, u64): Result<u64,ReadToEndError<E>>
  where read(R,E), display(E), noBorrow(E)

bufferedReader<R>(R,u64): BufferedReader<R>
bufferedWriter<W>(W,u64): BufferedWriter<W>
function intoParts<B,S>(buffer: B): [S,Bytes];
// Distinct input-pattern implementations: BufferedReader<R> -> [R,Bytes],
// BufferedWriter<W> -> [W,Bytes].

enum ReadExactError<E> {
  UnexpectedEof { transferred: u64 },
  Read(Err<IoFailure<E>>),
}
enum ReadToEndError<E> {
  Read(Err<IoFailure<E>>),
  LimitExceeded {
    transferred: u64,
    extra: Bytes,
    probeFailure: Option<Err<IoFailure<E>>>,
  },
}
enum BufferedFlushError<WE,FE> {
  Write(Err<IoFailure<WE>>),
  Flush(Err<FE>),
}
enum BufferedCloseError<WE,FE,CE> {
  Flush(Err<BufferedFlushError<WE,FE>>),
  Close(Err<CE>),
  Both { flushing: Err<BufferedFlushError<WE,FE>>, closing: Err<CE> },
}
```

Scoped requirements beyond capability/display evidence: readToEnd needs noBorrow(E); BufferedReader read needs noBorrow of its underlying read error; BufferedWriter write/flush need noBorrow of the underlying write error; BufferedWriter consuming close additionally needs noBorrow of the underlying flush error. These prevent local-buffer escape/retained mutation and errors borrowing a consumed resource. No extra noBorrow is imposed on primitive errors, caller-buffer readExact/writeAll, buffered-reader close or the consuming underlying close error. No sendable/shareable follows. Generic callers must state the scoped evidence before monomorphization; concrete borrow-free errors may satisfy it structurally.

Each capability independently determines E from its input implementation. Generic requirements are read(R,E), write(W,E), flush(S,E) and close(S,E), plus separately needed properties such as display(E); check bodies before monomorphization. Family bodies use implement and ordinary collection/duplicate/orphan/visibility rules. Desired errors cannot select implementations. Standard display for the error wrappers/enums requires display of their parameter errors. Retaining typed Err values preserves actual dependencies without automatic heterogeneous cause erasure or blanket noBorrow/sendable/shareable. Normal exact-E propagation, explicit adaptation, origins and causes remain.

Read appends at most maxBytes, preserving existing contents. Positive requests succeed with Some(n), 1 <= n <= maxBytes, or None at EOF with no appended bytes. Short reads are not EOF or message/UTF-8 boundaries. Zero request immediately returns Some(0) without consumption or EOF testing. Write accepts a prefix of its supplied logical extent: positive progress for nonempty success, zero for empty input. Calls suspend the routine while unavailable, not return WouldBlock; backend execution must not simply block the scheduler thread. Counts are public u64 upper bounds, never implicit native-size wrapping/allocation promises.

IoFailure.transferred exactly reports appended/accepted bytes from this operation, including failure; no rollback, unreported destination bytes or whole-buffer retry promise. Acceptance is relative to the layer, not peer receipt/durable storage. Helpers check successful zero progress, bounds and read length/count/EOF consistency before arithmetic/slicing; impossible reports are fatal adapter bugs, not synthetic E or infinite loops. Existing-prefix preservation is required without promising byte-for-byte validation of every adapter.

Exact/all helpers loop with bounded stacks; empty exact/all operations do not touch the stream. readExact succeeds only after appending the requested count. Its errors retain the obtained prefix and cumulative progress. writeAll accepts the entire input or reports cumulative failure; it does not flush/close or create framing. Rebase IoFailure progress with metadata-preserving mapping, retaining both the I/O envelope and nested original Err<E>; new context envelopes have normal new construction origins.

readToEnd requires a maximum additional-byte budget and returns the appended count. At the limit, probe separately for one byte: EOF succeeds; a byte yields LimitExceeded with exactly one owned extra byte, never appended beyond the bound. A simultaneous error is retained in probeFailure with its original probe-relative count; the top-level transferred is destination-relative. Probe failure without a byte is Read with cumulative progress. Zero budget still probes. Existing destination length is excluded; subtract it when enforcing a total-size budget. No unlimited variant or silently discarded lookahead.

BufferedReader/Writer own their wrapped values and have explicit nonzero u64 capacities bounding logical pending bytes, not exact allocation footprint. Zero capacity is a fatal programmer error; allocation/reservation failures retain fatal policy. No borrowed constructors, hidden background work, unbounded queues, public writable views or peek/consume API. Capabilities require wrapped evidence rather than being fabricated by construction. Reader may read ahead up to capacity; a partial refill failure reports immediately with caller-relative progress and retains the unread remainder. No deferred-error queue. Writer drains when necessary before accepting more input, with no implicit underlying flush merely to create space. Current-input progress may be zero when older output partially commits. Remove only confirmed prefixes and retain unsent suffixes on recoverable failure. Continued transport usability is implementation-specific.

intoParts consumes without I/O, returning the wrapped value and all unread/pending bytes in order. No silent buffer loss; pending output must precede later output if the caller preserves ordering. Reader close discards unread input and delegates underlying close, not draining to EOF.

BufferedWriter flush requires write(W,WE), flush(W,FE), display(WE), display(FE), noBorrow(WE): drain local pending output first, then underlying flush. Drain errors retain suffixes/skip underlying flush; underlying-flush errors leave local pending empty. Retry cannot replay confirmed bytes. Flush is layer handoff, not durability/peer acknowledgement. Consuming buffered close additionally requires close(W,CE), display(CE), noBorrow(FE), attempts safe underlying close even after flush failure, and preserves flush-only/close-only/both errors. Ownership is gone after either recoverable outcome; required cleanup completes. Flush while retaining ownership first when recovery is wanted. Disposal instead discards unsent output and does not flush or perform graceful protocol shutdown; remaining-field cleanup cannot double-close a moved/consumed resource.

Operations exclusively borrow their stream; reads also exclusively borrow destination and writes retain read-only source access. Suspension/cancellation must retain buffers, callback/native state and handles until completion or confirmed abortion, not merely an abort request. No fabricated lifetimes/properties or overlapping whole-stream operations. Separate directional handles are #34's decision. Cooperative cancellation is outside Result and may leave partial external effects without returning progress to stopped code. Cleanup can suspend indefinitely with cancellation deferred, including after close's cancellable flush phase. No force-kill, fatal-panic cleanup or global-exit delivery promise.

#33 owns concrete deadlines/context/races; no timeout parameter is added here or hard return promised by racing a timer. #34/#35/#38 own actual socket/file/TLS handles, shutdown and fatal states; #39/#40 own body/driver abandonment and connection reuse. Disposal does not acknowledge input or guarantee remote rollback. #30 still gates native/Wasm pointer stability and scheduling/cleanup machinery. [Prototype 19](../prototypes/19-streaming-io.rill) and its [case inventory/composition sketches](../prototypes/19-streaming-io.md) are hand-checked evidence, not implementations or validation proofs.

### Native paths and filesystem extension (#35)

[#35 / ADR 0019](adr/0019-paths-files-and-filesystem-operations.md) confirms the filesystem extension. These are public contracts, not an implemented filesystem/provider or security proof. Ordinary families retain canonical identity/parameter-pattern collection; implementations use `implement`. No methods, traits, return-only dispatch or new prelude bindings.

```rill
// std::path: Path is an opaque owned byte-preserving value.
function Path<S>(source: S): Result<Path,PathError>;
// Ordinary implementations accept &string / &Bytes and copy their extent.
enum PathError { EmbeddedNul { offset: u64 } }
pathBytes(&Path): &Bytes
pathText(&Path): Result<&string,Utf8Error>
isAbsolute(&Path): bool
join(&Path,&Path): Path
parent(&Path): Option<Path>
fileName(&Path): Option<&Bytes>
components(&Path): Components
enum PathComponent { Root(u64), Current, Parent, Name(&Bytes) }

// std::fs
function readFile<P>(path: P): Result<Bytes,IoError>;
function writeFile<P>(path: P, bytes: &Bytes): Result<void,IoError>;
// Ordinary input-pattern implementations accept &Path and the existing &string.
enum CreateMode { ExistingOnly, IfMissing, NewOnly }
enum WriteBehavior { Preserve, Truncate, Append }
type WriteOptions = { creation: CreateMode; behavior: WriteBehavior };
openRead(&Path): Result<ReadFile,IoError>
openWrite(&Path,WriteOptions): Result<WriteFile,IoError>
openReadWrite(&Path,WriteOptions): Result<ReadWriteFile,IoError>
enum SeekFrom { Start(u64), Current(i64), End(i64) }
function seek<F>(file: &mut F, from: SeekFrom): Result<u64,IoError>;
function sync<F>(file: &mut F): Result<void,IoError>;
syncDirectory(&Path): Result<void,IoError>

enum FileKind { Regular, Directory, Symlink, Other }
type Metadata = { kind: FileKind; size: Option<u64> };
function metadata<S>(source: S): Result<Metadata,IoError>;
// &Path follows final links; borrowed file handles inspect their opened object.
symlinkMetadata(&Path): Result<Metadata,IoError>
readLink(&Path): Result<Path,IoError>
createSymlink(target: &Path, link: &Path): Result<void,IoError>
currentDir(): Result<Path,IoError>

type DirEntry = { name: Path };
readDir(&Path): Result<ReadDir,IoError>
createDir(&Path): Result<void,IoError>
createDirs(&Path): Result<void,IoError>
removeFile(&Path): Result<void,IoError>
removeDir(&Path): Result<void,IoError>
rename(source: &Path, destination: &Path): Result<void,IoError>
tempFile(parent: &Path): Result<TempFile,IoError>
tempDir(parent: &Path): Result<TempDir,IoError>
function tempPath<T>(temporary: &T): &Path;
function keep<T,R>(temporary: T): R;
// keep(TempFile) -> [ReadWriteFile,Path]; keep(TempDir) -> Path

enum TempFileCloseError {
  Close(Err<IoError>),
  Remove(Err<IoError>),
  Both { closing: Err<IoError>, removing: Err<IoError> },
}
```

Path factories reject NUL with a byte offset, preserving all other bytes without existence/backend checks. No Windows drive/UNC grammar, normalization/case folding, tilde/env expansion or lossy decoding. Empty Path is lexically valid but InvalidInput for every filesystem location. pathBytes/pathText borrow underlying storage; parent returns owned storage, not a new &Path subrange abstraction. Canonical explicit copy, byte-based eq/hash and escaped diagnostic show are supplied. Diagnostic text is not a round-trip path encoding. Path/PathError have standard borrow-free/transfer/share metadata as appropriate to their owned byte/scalar representations; display(PathError) is supplied. Invalid encoding on actual backend use is InvalidInput, not filename replacement. Text whole-file conveniences keep IoError, validate the same native path rules and do not introduce implicit conversion/error adaptation.

Join: absolute child replaces base; empty child/base preserve the other; insert a separator only when needed while preserving spelling. Components retain root slash count and explicit `.`/`..`, skip empty interior/trailing components, and preserve source dependencies through canonical iterate/next. fileName ignores trailing separators and returns only a final ordinary name. parent removes the last component and preceding separator run while preserving the root run: `a/b/` -> `a`, `a` -> empty, `/a` -> `/`, `//a` -> `//`, `a/.` and `a/..` -> `a`; roots/empty have no parent. No lexical operation is symlink resolution, filesystem identity or confinement; exact-two-leading-slash native behavior is not normalized away.

ReadFile implements read/close; WriteFile implements write/flush/close; ReadWriteFile implements both directions/flush/close. All have seek and borrowed-handle metadata; only write-capable handles have sync. TempFile delegates the same read/write/seek/flush/sync/metadata capabilities to its owned ReadWriteFile but has its own close error below. File primitive errors are IoError; read/write errors are IoFailure<IoError>. Handle owners are not implicitly copied; no duplication/shared-cursor wrapper. Generic file requirements are seek(F)/sync(F) with fixed IoError; metadata uses distinct &Path/&ReadFile/&WriteFile/&ReadWriteFile/&TempFile input patterns. Access modes are static but object seekability is not; filesystem pipes/devices can fail Unsupported and directory byte I/O is IsDirectory.

WriteOptions fields are mandatory. Read-only never creates. ExistingOnly requires existence; IfMissing does not truncate unless explicitly requested; NewOnly atomically fails AlreadyExists on any existing final entry including a dangling link. Preserve starts at zero without shortening; Truncate is destructive; Append obeys native append regardless of seek. Every combination is meaningful. Unsupported faithful exclusivity/append must fail, not be emulated by prechecks. Each underlying append is distinct; writeAll has no whole-record atomicity. Seek checks negative/native-range errors without wrapping, returns absolute u64 position and does not extend/write merely by seeking beyond EOF. Separate handles do not isolate contents from external changes. Open/create/truncate may leave effects after later failure/cancellation; finish safe cleanup, not rollback.

Unbuffered file flush has no Rill-owned output buffer and does not sync. Explicit file sync requests platform data/required-metadata synchronization; syncDirectory separately requests directory metadata synchronization. No universal hardware/crash transaction, silent skip, or transparent buffered seek/sync. Flush/decompose adapters explicitly and account for unread/pending bytes; Current uses the actual advanced underlying cursor.

Metadata size is Some only for regular files; no timestamps pending #33 coordination, ownership/ACL management, cached entry-kind/identity or exists helper. Path metadata follows the final link; symlinkMetadata/readLink inspect it but intermediate links can still resolve. Relative link targets resolve from the link parent. Ordinary opens/directory lookup follow native traversal. removeFile/rename operate on the final link; removeDir does not follow a link to recursively delete a target. No hard-link/all-components-no-symlink API or portable sandbox is implied. Native currentDir observes process state; no process-wide setter, and external embedding changes can still affect relative paths.

ReadDir uses canonical iterate/next: next(&mut ReadDir) yields Option<Result<DirEntry,IoError>>. Owned one-component entry names exclude `.`/`..`; order is unspecified, not a snapshot or recursive walk. EOF or one yielded enumeration error makes next terminal. Remaining enumeration state stays owned until close(ReadDir):Result<void,IoError> or disposal, exactly once. createDir creates one entry and errors on existence; createDirs iteratively creates missing ancestors and accepts existing directories, with possible partial effects. removeDir only removes empty directories; no recursive deletion helper. Rename uses native compatible-target replacement, not portable no-replace or copy-and-delete fallback; cross-filesystem/unsupported cases fail and success is not durability.

Ordinary creation requests conventional native permissions filtered by host umask/ACL/capability policy; temporaries request owner-only access. No process umask mutation/general chmod/chown/ACL API. TempFile/TempDir require an explicit parent, fresh exclusive names and a finite collision-retry policy, not env/global temp selection or secrecy tokens. Missing faithful private temporary-creation support is Unsupported. tempPath borrows the guard; keep consumes/disarms deletion without renaming/syncing/closing. TempFile close closes then attempts removal even after close failure, preserving close-only/remove-only/both envelopes. TempDir close attempts empty-directory removal with IoError. Both consume on failure; disposal attempts best-effort nonrecursive removal and required native cleanup without a recoverable error/deletion guarantee. External name replacement affects path-based cleanup; moved initialized fields cannot be closed twice. Standard display for TempFileCloseError preserves inspectable owned typed errors.

IoError adds Unsupported/NotDirectory/IsDirectory/DirectoryNotEmpty; authority denial is PermissionDenied, invalid paths/encoding are InvalidInput. Native codes remain optional/nonportable. WASI absolute paths choose the longest component-boundary preopen guest-name match, relative paths require `.`; no first-preopen guess, ambient access or broader-capability fallback after denial. Leave `..` for host-enforced resolution after capability binding. Ambiguous/unusable mappings fail explicitly. Missing namespace/operation support is Unsupported; currentDir is Unsupported on WASI v0. Provider/private-creation/namespace guarantees require actual support, not reduced-success fallbacks. #28 must formalize root/mount spelling and errors; host mechanisms are not validated here.

All path/buffer/handle/native dependencies survive suspension until actual completion or confirmed abortion. Cancellation is outside Result, may leave effects and cleanup can wait indefinitely. Do not blindly retry interrupted native close across descriptor reuse. Existing #32 Q36 scoped requirements remain; standard IoError already satisfies them. #30 gates actual native/Wasm scheduling, pointers and reclamation. Process spawning/watching, general ACL management, migration/static-file frameworks and new copyFile helper are excluded. [Prototype 20](../prototypes/20-filesystem.rill) and its [case inventory](../prototypes/20-filesystem.md) cover lexical checks and bounded copy/config/certificate/directory/temp workflows by inspection, not execution/security proof.

### Partial standard SQL checkpoint (#40)

[#40](https://github.com/Reilley64/rill/issues/40) / [ADR 0021](adr/0021-partial-standard-sql-contract.md) records **confirmed independent decisions, not a completed SQL interface**. The user explicitly chose std::sql over an external shared execution module and approved Q1–Q57. [The partial contract](sql-contracts.md) owns the detailed value/error vocabulary, defaults, generic relationships and remaining questions. #40 stays open: #33 deadline/time integration, final operation arities and the unprovided embedded native provider remain unresolved. Do not treat the following as shipped operations or a frozen ABI.

- Canonical driver-facing families live under std::sql::driver, with identity-preserving std::sql exports/helpers. Concrete typed connectors select external drivers; concrete pools, dialect/builders/ORM/migrations remain external. No global registry, driver-name lookup, universal connection-string parser, public dyn, reflection, associated types or general FFI.
- Common Parameter is an owning subset (typed NULL, bool, signed/unsigned widths, f32/f64, text, bytes, Decimal), not the only driver argument type. Default arguments/rows/prepared descriptors provide noBorrow evidence, not implicit send/share or blanket error constraints. Borrowed views retain their actual owners.
- Direct bind encodes driver-supported values; bindInto explicitly invokes into<Parameter>; tryBind invokes tryInto<Parameter> and preserves conversion/binding errors separately. get<T> is one generic cell-access-plus-canonical-conversion helper, not return-only ordinary implementations. Source/destination/error evidence remains explicit before monomorphization; Into does not create TryInto. User modules can provide SQL and JSON Value conversions without importing every driver or adding a serialization trait. One SQL conversion occupies one slot; record/table mapping remains explicit/external.
- Checked extraction rejects incompatible NULL/type/range/precision/encoding; Option<T> preserves nullability. No silent numeric float intermediate, text replacement, unsupported-float-to-NULL or decimal text fallback. Parameter limits do not audit coercions performed by SQL/columns/triggers. Decimal validates/preserves a finite JSON-number-style token without float conversion/exponent expansion or a new arithmetic library; explicit token-budget override and borrowed decimalText are available.
- DbError<E> has first-class uniqueness/native and local argument/decode/cardinality/state/unsupported/limit cases. Unique details name ordered direct key columns when established, distinguish expressions/partial conditions, preserve qualified identity/provenance, and explicitly say unavailable rather than fabricate a universal column list. Metadata enrichment is explicit optional I/O, not an error accessor side effect or failure-time snapshot proof. Original native and codec Err payloads stay typed fields, not automatic erased causes. Default diagnostics omit raw SQL/values/native/codec/expression text; no recursive native-cause rendering bypass.
- Raw SQL/arguments are consumed; prepared descriptors are borrowed and independent of bounded session-owned native caches. Session use is exclusive, rows own data, cursors retain dependencies, iteration yields Option<Result<Row,DbError<E>>>. One statement/result stream, exact zero-based/name lookup with ambiguity errors, strict queryOne/queryOptional cardinality, no unlimited collector or silent batch-prefix execution.
- Transactions exclusively retain sessions; commit/rollback consume guards and distinguish confirmed committed/rolled-back/unknown outcomes from cleanup failures/reuse state. Abandoned transactions discard; early row abandonment requires actual safe cleanup evidence or discard. External pools validate/reset candidates. No automatic replay or cleanup-as-rollback guarantee. Unsupported isolation/read-only requests fail rather than substitute; managed transaction control is not mixed with ad-hoc commands or enforced by an imaginary SQL parser.
- Accepted configurable defaults: parameter/field payload 16 MiB each, row/frame 64 MiB each, metadata/decimal token 1 MiB each, 4,096 columns and 32,767 parameters. These are safety defaults, not benchmarked optimal settings, exact allocator/native/server-memory bounds or permission to exceed backend limits. Check before controlled allocation where possible; safe resynchronization or discard on limit errors. Source/native retention and #30 backend gates remain; cancellation is outside Result and cleanup can wait indefinitely.

[Prototype 22](../prototypes/22-sql-codecs/main.rill), its [domain codecs](../prototypes/22-sql-codecs/domain.rill) and [inventory](../prototypes/22-sql-codecs/README.md) supply 20 hand-checked value/codec/error expectations and independent adapter/transaction/privacy/invalid traces. Network/embedded cells and the memory argument encoder are not database drivers. [Research](research/0040-standard-sql-and-unique-errors.md) cites backend/library evidence; no Rill/database/provider/security proof is claimed. JSON #31's separately prepared artifacts are authoritative by issue snapshot, not overwritten or confused by historical numbering. #28 must validate inference/dependencies/redaction/progress/cleanup; this checkpoint neither closes #40 nor accepts future RFCs.

## 11. Validation and follow-through

`prototypes/15-library-contracts.rill` and its `.md` inventory contain hand-checked intended-valid and rejected cases. They are not compiler-executed tests or a demonstration of stack, callback, native, or borrow-checker soundness. Earlier fixtures whose no-arg channels, copy(value), variadic work calls, plain all, string tags, implicit Error construction or propagation reports conflict are historical in those respects; do not count them as v0-valid without migration.

#28 must reconcile these interfaces with all source tickets, formally specify generic requirement/candidate inference (including direct tuple literals/overloaded callbacks), source/iterator/callback dependencies, callee exits, mutable storage and transitive last use, enum joins/widening, property evidence and sealed cause invariants. Validate protected overlaps/negative capabilities/remaining-field cleanup, exactly-once native/channel state, cancellation masks, standard bounded-stack traversal/default release, numeric/parser/UTF-8 edge cases, alias identity/public contracts, and both accepted/rejected adversarial examples. A missing consistent required rule is returned to the map, not concealed by a conservative-sounding safety claim. #30 separately gates pinned backend/ABI/Wasm mechanisms; no target compatibility proof is asserted here.

#24 owns diagnostics; #25 exercises a realistic program using these interfaces; #31 settles JSON separately before its RFC coverage. #26 owns final RFC organization/status. These design inputs neither implement the standard library nor accept the future feature proposals.
