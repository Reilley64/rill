# Rill v0 standard-library contracts

Confirmed design input for [#23](https://github.com/Reilley64/rill/issues/23), summarized by [ADR 0015](adr/0015-language-required-library-contracts.md). This is **not** an implemented library, accepted feature RFC, or checker/backend proof. #28 writes and validates the feature RFCs; #24 owns compiler diagnostics. Separately approved application foundations extend the closed #23 core without reopening it: std::env is recorded in section 12 / [ADR 0019](adr/0019-owned-startup-environment-and-arguments.md), and secure random bytes in section 13 / [ADR 0020](adr/0020-secure-random-byte-generation.md). JSON has its own [confirmed contract](json-contracts.md) from [#31](https://github.com/Reilley64/rill/issues/31), and does not gate closing #23.

Read the newer decisions over historical examples: #18 explicit widths/UTF-8; #19 enums, not unions/singleton types; #29 origins and explicit causes, not propagation reports; #22 bounded stacks/proper tails. This contract refines #29's cause-retention boundary and #15/#17's copy/disposal interfaces explicitly below. ADRs 0013/0014 and their fixtures were prepared in separate sessions and may not yet be present in this checkout. Their GitHub decisions remain source inputs; their absence is not evidence those policies were dropped.

**Concurrent conversion refinement:** [#41's confirmed decision](https://github.com/Reilley64/rill/issues/41#issuecomment-6886119679) supersedes the historical closed-conversion/snake-case declarations below and uses explicit implement for family bodies. Its separate artifact migration is not duplicated by the JSON work in this checkout. Read both confirmed inputs for current conversion-specific dispatch/error evidence; #28 still owes combined formalization and validation.

## Reading the catalogue

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
| `std::convert` | from/into, try_from/try_into, ConversionError |
| `std::number` | parseNumber, ParseError; wrappingAdd/Sub/Mul, saturatingAdd/Sub/Mul |
| `std::bytes` | Bytes type/factory, length, slice, get, getMut, append, intoList, copyFromList, copyToList |
| `std::string` | length, slice, append, bytes, viewUtf8, decodeUtf8, decodeLossy, chars, charIndices, CharAt, Utf8Error |
| `std::char` | codePoint, fromCodePoint |
| `std::shared` | Shared, shared, view |
| `std::mutex` | Mutex, LockGuard, mutex, lock |
| `std::channel` | Sender, Receiver, SendError, channel, send, receive, after, select, unsent, intoUnsent |
| `std::routine` | Routine, AllFailed, failures, join, all, allSettled, race, any |
| `std::time` | sleep; [#33 temporal/clock/wait contract](time-contracts.md) and [cancellation integration](clock-cancellation-contracts.md) |
| `std::cancel` | [#33 sources, tokens, links, guard and poll/wait/select observation](clock-cancellation-contracts.md) |
| `std::io` | IoError, IoErrorKind, kind, nativeCode, readStdin, writeStdout, writeStderr, flushStdout, flushStderr; [#32 canonical streaming/progress/buffering extension](https://github.com/Reilley64/rill/issues/32#issuecomment-6886715215) and its [scoped noBorrow refinement](https://github.com/Reilley64/rill/issues/32#issuecomment-6886862143) |
| `std::net` | [Address values, TCP owners/halves, resolution/dialing, NetError, NetControl and controlled canonical I/O](networking-contracts.md) |
| `std::fs` | readFile, writeFile; uses the canonical std::io::IoError |
| `std::env` | Inputs, EnvError, EnvErrorKind, EnvPosition; inputs, fromParts, argsBytes, varsBytes, getBytes, args, vars, get, kind, position |
| `std::random` | randomBytes, fillRandom, RandomError, RandomErrorKind, kind, nativeCode |
| `std::encoding::json` | Owned Value/Number/Object, explicit parse/format/limits/errors; see the [JSON contract](json-contracts.md) |

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

#22's source/build capacity contract is preserved: u64 usable addressable bytes; zero/below-minimum/excessive/unrepresentable/effective-alignment-invalid dynamic settings panic, build settings reject; reservation/backing failure is fatal. The runtime publishes actual target limits and checked upward alignment, not a portable recursion-depth guarantee. `--stack-size` sets program default, `--entry-stack-size` entry only, with equivalent run/test compile settings; no manifest/environment/optional/named parameter mechanism. Ordinary/scoped children use program default, not parent overrides. Native default usable capacity is 1 MiB; Wasm addressable and saved budgets default separately to 1 MiB per routine. `--saved-stack-size` affects Wasm program-wide saved state, is rejected on native, and does not set host-engine limits. Handle drop/cancel requests do not authorize reclamation while borrowers/cleanup remain. Full support remains gated by #30. [ADR 0006](adr/0006-whole-program-monomorphization-on-llvm-23.md#confirmed-portable-wasm-lowering-direction-30) records the human-approved explicit-record/iterative-dispatch direction for portable Wasm, not an implemented ABI/runtime or closed support gate; the old tail-call/Asyncify composition is not the selected mechanism.

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

Timers use monotonic time, not calendar time. after produces one void notification then closes. Zero is immediately eligible, not a forced yield. Scheduling/native work/cleanup can delay completion indefinitely; checked duration/deadline representation must not silently wrap. No portable maximum scheduling-latency or hard-deadline promise. [Confirmed #33](clock-cancellation-contracts.md) makes the default clock suspend-inclusive, preserves these millisecond shapes and defines fatal unsupported/failure handling for their error-free signatures. New typed waits take an explicit token and return/deliver WaitError; explicit token cancellation is not intrinsic goroutine cancellation.

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

CharAt is a named record `{ offset:u64; value:char }`, exported from std::string. Chars/CharIndices are opaque source-retaining iterators, yielding scalars/CharAt through canonical next. fromCodePoint accepts Unicode scalars, excluding surrogates/out-of-range values, not merely printable/assigned characters. No grapheme indexing, normalization, locale collation, writable UTF-8 bytes, mutable substrings, implicit List contiguity or fabricated &List<u8> temporary headers.

String literals are expressions of string type, not singleton types. Uninterpolated literal expressions can be rematerialized into fresh logical owners/static backing under #18, but a named string binding moves unless explicitly copied. Optimizer knowledge does not grant implicit copying, noBorrow to a view, capture access or a safe numeric conversion. `copy(&owner)` copies owned text; `copy(view)` already passes a &string and copies the observed extent. To copy the descriptor itself use its read-only implicit-copy rules or the appropriately typed borrowed descriptor, preserving dependencies. These are not duplicate freeable owners or hidden reference counts.

## 9. Numeric conversion, parsing and formatting

```rill
// closed compiler-known std::convert operations, not user-extensible overloads
into<To, From>(value: From): To
try_into<To, From>(value: From): Result<To, ConversionError>
// from aliases into; try_from aliases try_into, same type-argument order.

// closed std::number operation
parseNumber<T>(text: &string): Result<T, ParseError>
```

Requirements are canonical into(To,From), try_into(To,From) and parseNumber(T); aliases preserve the same operation. Destinations come from explicit leading arguments or an immediate unique expected type. Source widths must be independently established. No as, fallback width, arbitrary return-only implementations, source guessing or user conversion/parser trait. Domain parsing/conversion uses separately named functions.

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

readFile returns owned bytes. writeFile creates/truncates and may leave partial output after failure/cancellation, with no atomic replacement, rollback, durable-sync or file-locking promise. Embedded-NUL/invalid platform paths return InvalidInput rather than unsafe path truncation. No general streaming-file/reader-writer abstraction, networking, seeking or directory traversal is required by this historical #23 baseline. Confirmed #32 streaming, #34 networking and #35 filesystem extensions supply their own additional contracts without changing these convenience signatures. Text I/O composes bytes and explicit UTF-8 handling.

IoError is an opaque owned diagnostic type with `kind(&IoError):IoErrorKind` and `nativeCode(&IoError):Option<i64>`. IoErrorKind has NotFound, PermissionDenied, AlreadyExists, InvalidInput, Other variants; unmapped platform failures use Other. Optional native codes are platform-specific, not portable branching contracts. Its display uses owned diagnostic context; callers do not parse human-readable messages. IoError, ConversionError, ParseError and Utf8Error provide display and noBorrow/sendable/shareable evidence. Underlying native-code translation must not make intrinsic routine cancellation a recoverable I/O error or fabricate successful completion. Explicit token requests are distinct: #33 approves ordinary per-operation outcomes and #34 defines the concrete controlled-networking errors, without retrofitting these #23 convenience signatures.

Root log/panic each use one `(S)` declaration shape with string and &string input implementations. Log writes text plus newline, returns void, synchronizes whole messages (same stream synchronization as output operations), and has unspecified concurrent message order. Failure is fatal; fallible I/O is the alternative. It is text-only: show/interpolation is explicit, not generic reflection or implicit owner copying.

Source panic requires importing std::panic and returns never. Compiler-generated arithmetic/allocation/stack failures use canonical runtime metadata without a lexical panic binding. User shadowing cannot redirect them. Rill/runtime allocation/reservation failure is fatal, not ubiquitous recoverable allocation Results. #34 explicitly distinguishes a documented native operation's reported resource shortage (which may become NetErrorKind.ResourceLimit) from failure of Rill allocation, native-operation bookkeeping or error construction itself; reporting such an error can still fail fatally. #22 overflow uses bounded best-effort emergency reporting without user formatters/full trace/required allocation/unwind; host failure may prevent a report. Other fatal panic remains uncatchable, with no cleanup/global-exit guarantee.

## Confirmed time and cancellation extension (#33)

[ADR 0022](adr/0022-jiff-time-and-explicit-cancellation.md), the [temporal contract](time-contracts.md) and [clock/token contract](clock-cancellation-contracts.md) record the explicitly confirmed expansion. Jiff-derived validated values/immutable zones, explicit source limits, exact arithmetic/text boundaries, suspend-inclusive clocks/manual controls and token-aware waits extend this baseline. Sources request one-shot state; tokens observe; stop still joins. There is no generic withCancellation, public callback registry, ambient token or source CancelAfter. withDeadline preserves its R and joins losing work/cleanup. Explicit token outcomes can complete normally during masked cleanup only after safe resource release; intrinsic delivery remains deferred. #36 supplies explicit Inputs for system-zone TZ, never live environment reads. Prototypes 23/24 are hand-checked, not compiler/runtime/provider proof; #28/#30 retain gates.

## Confirmed networking extension (#34)

[ADR 0023](adr/0023-tcp-addresses-resolution-and-controlled-networking.md), the [complete networking contract](networking-contracts.md) and [prototype 25](../prototypes/25-networking.rill) / [inventory](../prototypes/25-networking.md) record explicitly confirmed Q1–Q93. Private-representation numeric values, strict pure parsers/canonical show, scoped SocketAddress and separate native scope lookup keep value identity distinct from system resolution and authenticated service identity. The lookup grammar, numeric-looking guard, owned system-ordered candidates and explicit family filters are fixed by that contract, not delegated to a permissive native literal parser.

Owned noncopyable TCP resources and split halves reuse #32's canonical std::io read/write/flush/consuming-close and IoFailure progress. Raw flush is a no-op. Disposing a BufferedWriter queue never flushes it; releasing its owned raw TcpWriteHalf still attempts the explicitly approved local send-direction release without joining the read half/routine. Native borrowers and shared state remain valid through true completion/release. A close error never authorizes retrying an unowned/recycled descriptor.

NetControl and configuration use ordinary public fields under #15 binding/exclusive-access rules, with stable captured value snapshots and retained dependencies; no per-field mut/readonly syntax or implicit owned handle copies. ControlledTcp borrows its source, has appropriate canonical I/O capabilities but no consuming close, and never silently closes that source merely to abort controlled work. Explicit TokenCancelled/DeadlineExpired/ClockFailure outcomes report final actual prefixes, including late effects; intrinsic cancellation remains outside Result. The clock contract explicitly gains identity-only checkInstantDomain, a canonical alias of the concurrently SQL-approved validateInstantDomain, while withDeadline preserves nested R and joined cleanup.

Dialing resolves one batch, interleaves families, uses at most two live attempts including cleanup, defaults to 250 ms spacing/ten-second attempt caps, forwards one optional overall absolute deadline, and retains the chosen winner through loser cleanup. Defaults/options, typed errors/ordered original failure envelopes and preflight/selection rules are fully specified in the linked contract. No hidden replay, protocol probes, IDNA, provider/FFI selection or stronger return-time guarantee is implied.

Live v0 networking is native Linux/macOS; portable values and typecheck-visible APIs remain on wasm32-wasip1, with typed Unsupported live operations rather than new imports/preview2/inherited-socket handoff. The 32 intended-valid expectations and all real-native matrix rows are hand-checked/NOT RUN, not Rill execution or support/security proof. #28/#30 and the TLS/HTTP/SQL consumers retain their formalization/provider/protocol gates.

## 11. Validation and follow-through

`prototypes/15-library-contracts.rill` and its `.md` inventory contain hand-checked intended-valid and rejected cases. They are not compiler-executed tests or a demonstration of stack, callback, native, or borrow-checker soundness. Earlier fixtures whose no-arg channels, copy(value), variadic work calls, plain all, string tags, implicit Error construction or propagation reports conflict are historical in those respects; do not count them as v0-valid without migration.

#28 must reconcile these interfaces with all source tickets, formally specify generic requirement/candidate inference (including direct tuple literals/overloaded callbacks), source/iterator/callback dependencies, callee exits, mutable storage and transitive last use, enum joins/widening, property evidence and sealed cause invariants. Validate protected overlaps/negative capabilities/remaining-field cleanup, exactly-once native/channel state, cancellation masks, standard bounded-stack traversal/default release, numeric/parser/UTF-8 edge cases, alias identity/public contracts, and both accepted/rejected adversarial examples. A missing consistent required rule is returned to the map, not concealed by a conservative-sounding safety claim. #30 separately gates pinned backend/ABI/Wasm mechanisms; no target compatibility proof is asserted here.

#24 owns diagnostics; #25 supplies the accepted realistic-program evidence; #31 records the separate [JSON contract](json-contracts.md). #41 owns the requested user-extensible camelCase conversion/error interface, including reconciliation of the earlier closed numeric declarations above; it is not settled by the JSON ticket. #26 owns final RFC organization/status. These design inputs neither implement the standard library nor accept the future feature proposals.

## 12. Environment and argument inputs

[#36](https://github.com/Reilley64/rill/issues/36) / [ADR 0019](adr/0019-owned-startup-environment-and-arguments.md) add this explicitly confirmed application foundation after #23. This section is the current std::env contract, not a live-process configuration API.

### Public surface and ownership

All new types/functions are exported from `std::env`; none enters the fixed prelude. `Inputs` and `EnvError` are opaque nominal private-field types. Public signature contracts are:

```rill
inputs(): Result<Inputs, EnvError>
fromParts(arguments: List<Bytes>, variables: List<[Bytes, Bytes]>)
  : Result<Inputs, EnvError>
argsBytes(inputs: &Inputs): &List<Bytes>
varsBytes(inputs: &Inputs): &List<[Bytes, Bytes]>
getBytes(inputs: &Inputs, name: &Bytes): Result<Option<&Bytes>, EnvError>
args(inputs: &Inputs): Result<List<string>, EnvError>
vars(inputs: &Inputs): Result<List<[string, string]>, EnvError>
get(inputs: &Inputs, name: &string): Result<Option<string>, EnvError>
kind(error: &EnvError): EnvErrorKind
position(error: &EnvError): Option<EnvPosition>
```

`inputs()` returns a fresh independently owned copy of the fixed startup inputs, never a live read or a hidden counted handle. `fromParts` consumes the lists/byte owners supplied by the caller; the caller must copy explicitly to retain them. Successful Inputs has no mutable accessor and no implicit copy. The standard `copy(&Inputs):Inputs` implementation independently copies its contents. Inputs and EnvError supply noBorrow/sendable/shareable evidence; EnvError supplies canonical display. Ordinary cleanup disposes owned fields; no new resource action or copy of Err/Cause is implied.

Raw query results retain ordinary source dependencies. In particular, getBytes conservatively depends on both borrowed Inputs and borrowed lookup name; this API does not add a special lifetime annotation or an exception just because the key is not the value's backing storage. Keep those owners alive until the result's last use. Text results own independent strings and do not retain the Inputs or query. Scoped read-only use and explicit owned transfer/copy obey existing callable/concurrency rules.

### Bytes, validation and ordering

Preserve all delivered argument bytes and their order, including argument zero if supplied. An empty list is valid; argument zero is not a guaranteed executable path, and strings are not shell-split, unquoted or interpreted again.

Environment names are nonempty, case-sensitive exact bytes, excluding NUL and `=`; non-UTF-8 names and names outside shell-identifier grammar are valid. Values may be empty or contain `=`, but not NUL. Arguments may be empty, but not contain NUL. A host entry splits at its first `=`; absence of a separator is MalformedEntry and an empty left side is InvalidName. Duplicate exact names reject even if their values agree; case-distinct names remain distinct.

Validate arguments in supplied order for InvalidArgument, then environment entries in supplied order. At each host entry check separator, name, value, then duplication; fromParts already supplies pairs, so its per-entry checks are name, value, then duplication. Report the first failure with no partial Inputs. Only after validation, sort variables lexicographically by unsigned raw name bytes (ordinary prefix ordering), without locale/case folding or Unicode normalization. This canonical enumeration is not unordered Map iteration and does not imply a new public sorting/hash API.

Validate lookup names even on empty Inputs. Invalid queries yield InvalidName/LookupName, not absence. Missing valid names produce Ok(None); present-empty produces Ok(Some(empty bytes/text)). getBytes never validates Unicode. Text get validates only the selected value; unrelated non-UTF-8 keys/values do not break a valid lookup. Bulk args/vars conversion stops on its first invalid UTF-8 item, never skipping/replacing it: argument order for args, canonical variable order with name before value for vars. Returned text is owned; explicit lossy conversion remains existing byte/string composition.

### Safe structured errors

```rill
enum EnvErrorKind {
  Unavailable,
  AcquisitionFailed,
  InvalidName,
  InvalidArgument,
  InvalidValue,
  DuplicateName,
  MalformedEntry,
  InvalidUtf8,
}
enum EnvPosition {
  Argument(u64),
  VariableName(u64),
  VariableValue(u64),
  LookupName,
}
```

Unavailable means the supported running host/embedding cannot provide the operation; AcquisitionFailed means attempted acquisition failed. InvalidArgument/InvalidValue mean embedded NUL; InvalidName covers empty/NUL/equals names. DuplicateName identifies the later duplicate through VariableName. MalformedEntry identifies the unsplittable host entry through VariableName. InvalidUtf8 identifies the argument/name/value being converted.

Positions are optional: acquisition failures need not have an input index. Construction errors use supplied pre-sort indices, while bulk variable text errors and a selected invalid text value use canonical varsBytes indices. Invalid lookup names use LookupName. All counts/indices are u64 with existing target addressability/checked-sizing limits.

EnvError retains only safe category/position information, not offending names/values, full argv/environ or arbitrary host diagnostic strings. Its standard display and wrapper/cause rendering must not reintroduce those contents. A decoder can inspect internally without retaining the offending source in the diagnostic. No general reflected Inputs dump or secret-detection/zeroization guarantee is added. Applications may deliberately retrieve/print values or explicitly wrap errors under normal rules; that is their responsibility. Existing origin/cause semantics remain, not propagation traces or implicit error conversion.

### Startup, tests and hosts

Capture once at a controlled runtime point before Rill workers. No later operation consults mutable process-global state or guest-libc environment state. Publish an immutable owned outcome: captured data or safe acquisition/validation failure information. Each inputs call creates its own owned Inputs or EnvError result under ordinary construction-origin rules, without cloning unique Err/Cause state, executing user formatters at bootstrap or terminating early merely for malformed input. There is no publicly mutable runtime global or module initializer; named functions still receive application state explicitly.

An embedding host must supply owned inputs or establish a genuinely safe capture point; an internal mutex cannot coordinate foreign native mutation. Synthetic/host-supplied pairs use the same validation rules. Native Linux/macOS and wasm32-wasip1 share the API; WASI sees only instance-provided data, not ambient host inheritance, and a host may restrict injection to UTF-8. Empty supplied data is valid. Ordinary reported acquisition failures remain Result errors. Allocation failure, fatal host traps and inability to instantiate are not recoverable EnvError promises; #30 still gates backend/host-pointer support. No Windows behavior is implied.

In rill test, skip actual host capture and use empty arguments/variables, regardless of developer/CI environment and runner flags. Tests needing data call fromParts explicitly; no host-inheritance testing switch is added. Build/runner options are not implicitly reclassified as application arguments; the API preserves only the input sequence actually supplied to the application. Flag parsing, dotenv/layered configuration, process spawning/mutation, executable-location discovery and an ambient-host opt-in remain outside this scope.

### Evidence and follow-through

[Prototype 20](../prototypes/20-environment-inputs.rill) and its [independent cases](../prototypes/20-environment-inputs.md) cover byte preservation, text ownership, validation priority, case/order/missing/empty semantics, error positions, explicit worker configuration and isolated tests. Examples are hand-checked, not compiler/runtime execution. Standard capture/validation/sorting/copying/decoding/default cleanup must bound call-stack use per #22; target limits and fatal allocation policy remain, with no silent truncation.

#28 must formalize conservative result/key dependencies, callee exits, opaque property/copy evidence, source construction origins, safe startup/embedding capture and immutable outcome publication, checked host count/size/read consistency, exactly-once cleanup and cancellation of partial copies/conversions. Host traps and arbitrary concurrent foreign mutation cannot be made safe by interface wording. #24's confirmed diagnostics model applies; #35 owns actual filesystem paths. Return an unworkable required checking/platform rule to the map; design approval is neither a safety proof nor final RFC acceptance.

## 13. Secure random bytes

[#37](https://github.com/Reilley64/rill/issues/37) / [ADR 0020](adr/0020-secure-random-byte-generation.md) add this explicitly confirmed secure-byte foundation without reopening #23 or changing #36's environment/test contract.

### Public surface and security boundary

`std::random` exports the following ordinary operations and opaque RandomError, plus public RandomErrorKind. No names enter the prelude, and no public provider, seed, global/environment override or deterministic mode is added.

```rill
randomBytes(length: u64): Result<Bytes, RandomError>
fillRandom(destination: &mut Bytes): Result<void, RandomError>
kind(error: &RandomError): RandomErrorKind
nativeCode(error: &RandomError): Option<i64>

enum RandomErrorKind {
  Unavailable,
  PermissionDenied,
  ProviderFailed,
}
```

randomBytes returns exactly the requested logical length of independently owned Bytes only after complete provider success. No partial owner escapes on error/cancellation. fillRandom replaces all bytes in the destination's existing logical extent on success, with no resize, append, spare-capacity access or writable borrowed subrange. Both forms use existing Bytes, not a new random-buffer/reader interface. Success is not a count/short fill.

On in-place error/cancellation, contents may be modified with no usable-prefix or rollback guarantee; logical length remains unchanged. Do not use the destination as successfully generated material after failure. Normal initialized-byte/memory-safety and ownership rules still hold; unspecified contents do not permit uninitialized memory or writes outside the extent. A later whole-buffer successful fill establishes its own complete result. No byte-generation operation promises uniqueness or that any particular byte is nonzero.

Within a running supported runtime, randomBytes(0) returns empty Bytes and filling an empty owner succeeds without consulting/initializing a provider. This is not a capability probe or a promise that missing Wasm imports instantiate. Normal scheduling/cancellation and fatal allocation/host behavior are not turned into Result success guarantees.

Use checked u64 requests and existing target addressability/Bytes capacity/allocation behavior, with no silent truncation/narrowing or artificial portable total-size quota. Unrepresentable capacity/allocation failures remain fatal, not recoverable RandomError variants. Callers bound untrusted sizes. Chunking/checkpoints are implementation details: check remaining lengths, offsets and native size_t/WASI u32 conversions against real provider limits. Standard traversal/cleanup bounds call-stack growth per #22; no full-request shadow buffer or zeroization protocol is required by the public contract.

### Errors and providers

RandomError owns safe category/optional native numeric code, never the generated bytes, destination contents or an automatic diagnostic buffer dump. It supplies canonical display and noBorrow/sendable/shareable evidence. Native codes are diagnostic platform-specific signed i64 observations, not a cross-platform branching protocol. Preserve representable native code values without wrapping; portable decisions use kind.

Unavailable identifies an unavailable source/operation; PermissionDenied covers reliably identified access restrictions; otherwise a reported source failure is ProviderFailed. Unknown statuses need not be falsely classified as permission or absence. Linux EINTR is retried rather than exposed as Interrupted, with normal checks between attempts. Nonempty zero progress produces ProviderFailed rather than spinning or returning an incomplete success; a runtime/provider bounds violation is not made safe or recoverable by an error label. Typed errors do not include fatal host traps, missing-import/instantiation failures, allocation failure or cancellation delivery. Error wrappers retain ordinary construction origins and explicit causes, not automatic propagation traces or implicit conversion.

| Target | Selected baseline |
| --- | --- |
| Linux x86-64/AArch64 | getrandom(..., 0), waiting for secure initialization; handle positive short counts, retry EINTR, no GRND_INSECURE/GRND_RANDOM/device-file or Rill-generator compatibility fallback. Report unavailable syscall or sandbox errors when the runtime receives them. |
| macOS x86-64/AArch64 | SecRandomCopyBytes(kSecRandomDefault, ...), checked status and size_t lengths; trusted runtime/system-framework linkage. No assumed short-count, rollback or bounded-latency guarantee. |
| wasm32-wasip1 | random_get only under an explicitly supported trusted host configuration providing cryptographically secure bytes; checked u32 ABI lengths/offsets and host-compatible bounded calls. Preview1 high-quality wording alone does not certify every host. |

No Windows support is implied. Provider trust is part of supported deployment, not something guest byte statistics can verify. A vetted securely seeded cryptographic generator inside an OS/host service is not intrinsically a weak fallback; predictable/weak/fixed/test/zero output must never be substituted for secure success. The chosen Linux baseline deliberately omits a compatibility backend even if another secure-source approach could be separately reviewed later.

### Blocking, cancellation and testing

Potentially blocking native provider work uses the runtime blocking facility instead of an ordinary scheduler worker. Small requests being usually fast is not a nonblocking guarantee. Provider calls use bounded chunks; positive counts advance only the confirmed region, retries cannot spin without normal cancellation/fairness opportunities, and errors never publish a partially complete owned result. There is no new public timeout/try-nonblocking API.

Retain backing owners and all exclusive-access restrictions until actual native completion or confirmed abortion. Requesting cancellation, dropping a handle or moving work to a blocking pool does not end native writes. Cleanup can suspend indefinitely; no hard return/cancel deadline, force kill, rollback or fatal-panic cleanup promise. Partial allocations/copies/owned output dispose exactly once only after source/native obligations end.

Wasm's synchronous import may stall its single-threaded scheduler. Offload/reentrancy/memory.grow safety needs backend validation: a guest offset is not a permanently valid host pointer. Host-owned staging with safe copy-back is a candidate, not an implemented or universally required mechanism. #30's existing support gates remain; unsupported integration must not be silently described as a working asynchronous entropy provider.

rill test keeps real secure provider behavior; it does not automatically seed/replace std::random, even though std::env intentionally supplies empty inputs in tests. Application consumers can explicitly accept fake callables in their own code. Failure/partial-write/retry tests of the standard adapter use structurally separate private harness/linkage, never a public production seed/provider override or new user-implementable secure source. Local aliases/shadowing do not change canonical identity or the standard support/security contract. Tests must not assert exact random values, all-nonzero output or guaranteed uniqueness; observations cannot certify unpredictability.

Output is ordinary Bytes, without locked-memory, secure erase or protection from explicit application printing. Seeded PRNGs, ranges/floats/shuffling, UUID/token formatting, cryptographic algorithms and special secret storage remain separate designs.

### Evidence and follow-through

[Prototype 21](../prototypes/21-secure-random.rill) and its [independent inventory](../prototypes/21-secure-random.md) demonstrate empty calls, ordinary production call shapes and explicit application fakes without a standard-provider override. They are hand-checked source examples, not live provider tests or entropy/security proof. ADR 0020 records exact primary-source contracts and differences; no historical Linux size limit, macOS getentropy limit or Wasmtime default limit becomes a universal API cap.

#28 must validate checked count/offset/chunk progress, source error mapping/origins and buffer-safe diagnostics, native blocking/retention/cleanup, cancellation/retries and non-escaping owned partial output. Specify/link the chosen macOS system framework and actual supported WASI provider configurations, including synchronous stalls, host traps and memory relocation/reentrancy. #38 must use a justified TLS entropy/provider configuration, not infer that this byte API alone establishes TLS security or authorizes entropy overrides/general FFI. Missing necessary checking/provider mechanisms return to the map; design approval neither implements the library nor accepts the future RFC.
