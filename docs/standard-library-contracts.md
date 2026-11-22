# Rill v0 standard-library contracts

This is confirmed design input for ri-8n7, summarized by [ADR 0015](adr/0015-language-required-library-contracts.md). This is **not** an implemented library, accepted feature RFC, or checker/backend proof. ri-48g writes and validates the feature RFCs; ri-3t8 owns compiler diagnostics.

The core conversion interface direction (ri-48g, comment 6913810445) revises the separate-parser/decoder interface. [Draft RFC 0006](rfcs/0006-numbers-strings-and-primitive-operations.md#canonical-core-conversion-interface) specifies the audited conversion matrix. Its broader pair details and ScalarError are draft applications of the owner's predictability direction, not complete RFC acceptance. Numeric, parsing and UTF-8 value rules remain unchanged.

Separately approved application foundations extend the closed ri-8n7 core without reopening unrelated decisions. The std::env contract is recorded in section 12 / [ADR 0019](adr/0019-owned-startup-environment-and-arguments.md), and secure random bytes in section 13 / [ADR 0020](adr/0020-secure-random-byte-generation.md). JSON has its own [confirmed contract](json-contracts.md) from ri-1zg, and does not gate closing ri-8n7.

The newer decisions take precedence over historical examples: ri-a2c explicit widths/UTF-8; ri-kty enums, not unions/singleton types; ri-vtw origins and explicit causes, not propagation reports; ri-636 bounded stacks/proper tails. This contract refines ri-vtw's cause-retention boundary and ri-lql/#17's copy/disposal interfaces explicitly below. Use the full [error ADR](adr/0013-caused-by-errors-with-explicit-wrapping.md) and [stack ADR](adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) for those decisions.

ri-1wl's confirmed decision (comment 6886119679) defines the user-extensible conversions in section 9. It replaces the earlier closed-only conversion boundary and snake_case names. Family bodies use implement. ri-48g still owns the combined formal rules and validation.

[ri-dej's runtime-validation follow-up](research/0030-runtime-validation.md) completes the standalone mechanism-experiment stage, not Rill/library acceptance or production backend acceptance. References below to ri-dej identify that evidence/prerequisite; ri-48g retains complete ABI/generated-lowering/checker/runtime support acceptance and provider tickets retain their integration gates. No source API, target or experimental layout/headroom constant is newly approved.

## Reading the catalogue

[ri-1wl / ADR 0017](adr/0017-user-implementable-conversions.md) adds language-wide `implement` syntax for family bodies: `implement show(value: &Local): string { ... }` and `implement<T> show(value: &Box<T>): string where show(T) { ... }`. Body-less family declarations and standalone definitions retain `function`. Infer omitted family arguments from implementation signatures, not bodies. Implementations inherit family visibility. Only the canonical conversion families have the destination/source key exception described in §9; ordinary return-only dispatch remains forbidden.

Signatures below specify public operations, not complete source files. Repeated operation signatures describe implementations of one family, not multiple conflicting body-less declarations. Each overload family has one declaration. All ordinary implementations preserve its parameter count, reference structure as specialized through type parameters, and return pattern. No dispatch on return type alone, constraint truth, runtime value, import order, or dead-code elimination is added.

A loose declaration deliberately exposes its result type parameters when parameter specialization determines the result. For example:

```rill
function map<C, F, R>(from: C, transform: F): List<R>;
```

Its two input-pattern implementations are `(List<T>, F) -> List<R>` with `callableMut(F, [T], R)` and `(&List<T>, F) -> List<R>` with `callableMut(F, [&T], R)`. Determine the input pattern/element access before contextualizing the callback, including an overloaded name such as `show`. Do not guess callback capabilities by inspecting eventual monomorphized bodies. Ambiguous/missing inference requires an annotation; no look-ahead from later statements.

`where family(A, B)` identifies the canonical family's type arguments, including result-related arguments where present; it is not a structural name-based promise. Requirements retain the selected signature, invocation/access, property and dependency evidence. Constraint differences cannot distinguish duplicate parameter patterns. Public requirements name exported canonical declarations. Imported aliases preserve identity, but compiler metadata does not inject names into caller source.

The arity schemas below describe exactly the 2-, 3-, and 4-item implementations, not source-level variadic generics, ellipsis syntax, or optional parameters. Explicit tuples and List remain distinct types; candidate/contextual inference must not silently reinterpret an ambiguous literal based only on a desired return. Explicitly typed source expressions/annotations resolve such ambiguity; ri-48g must validate direct tuple callback literals as well as named inputs.

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
| `std::number` | ParseError; wrappingAdd/Sub/Mul, saturatingAdd/Sub/Mul |
| `std::bytes` | Bytes type, length, slice, get, getMut, append, clear, truncate, reserve |
| `std::string` | length, slice, append, decodeLossy, chars, charIndices, CharAt, Utf8Error |
| `std::char` | ScalarError, proposed under the core conversion audit |
| `std::shared` | Shared, shared, view |
| `std::mutex` | Mutex, LockGuard, mutex, lock |
| `std::channel` | Sender, Receiver, SendError, channel, send, receive, after, select, unsent, intoUnsent; ri-w0c SendAttempt, ReceiveAttempt, WakeNotifier, ChannelWatch, trySend, tryReceive, wakeChannel, wake, watchSendReady, watchReceiveReady, arm, disarm |
| `std::routine` | Routine, AllFailed, failures, join, all, allSettled, race, any |
| `std::time` | sleep; [ri-j60 temporal/clock/wait contract](time-contracts.md) and [cancellation integration](clock-cancellation-contracts.md); ri-dka's validateInstantDomain amendment below |
| `std::cancel` | [ri-j60 sources, tokens, links, guard and poll/wait/select observation](clock-cancellation-contracts.md) |
| `std::io` | IoError, IoErrorKind, kind, nativeCode, readStdin, writeStdout, writeStderr, flushStdout, flushStderr; read, write, flush, close, readExact, writeAll, readToEnd, IoFailure, ReadExactError, ReadToEndError, BufferedReader, BufferedWriter, bufferedReader, bufferedWriter, intoParts, BufferedFlushError, BufferedCloseError |
| `std::io::progress` | ri-w0c ProgressSchedule, admissions/rejections, read/write/operation/interruption outcomes, DriverFailurePhase/DriverFailure/DriverFailureSet, ProgressStep; admitRead, admitWrite, admitFlush, admitClose, requestStop, advance, registerProgressWait, withdrawProgressWait |
| `std::path` | Path type/factory, PathError, PathComponent, Components, pathBytes, pathText, isAbsolute, join, parent, fileName, components |
| `std::sql`, `std::sql::driver` | ri-dka design: Parameter/ParameterType/Decimal/ColumnRef/ExecSummary, SqlControl/uncontrolled, TransactionOptions/IsolationLevel/TransactionOutcome/TransactionFailure, ReuseDisposition/QueryFailure, DbError and structured uniqueness/codec errors, canonical connect/arguments/bind/prepare/execute/query/begin/commit/rollback/cellAt/resolveUnique/reuseDisposition/reset, explicit bindInto/tryBind/get/queryOne/queryOptional; see the SQL contract for full shapes |
| `std::fs` | readFile, writeFile; ReadFile, WriteFile, ReadWriteFile, WriteOptions, CreateMode, WriteBehavior, SeekFrom, openRead, openWrite, openReadWrite, seek, sync, syncDirectory; Metadata, FileKind, metadata, symlinkMetadata, readLink, createSymlink, currentDir; DirEntry, ReadDir, readDir, createDir, createDirs, removeFile, removeDir, rename; TempFile, TempDir, TempFileCloseError, tempFile, tempDir, tempPath, keep; uses canonical std::io::IoError |
| `std::net` | [Address values, TCP owners/halves, resolution/dialing, NetError, NetControl, controlled canonical I/O and ri-w0c TcpProgress](networking-contracts.md) |
| `std::tls` | [Explicit trust/CRL/credential snapshots, owned client/server handshakes, canonical I/O and ri-w0c TlsProgress](tls-contracts.md); [private provider boundary](tls-native-boundary.md) |
| `std::http` | [ri-n5i owning methods/statuses/URI/headers/heads/plans/limits/controls/body/completion/error values](http-contracts.md) |
| `std::http::h1` | [ri-n5i strict transport-independent HTTP/1 request/response decoders and encoders](http-contracts.md) |
| `std::http::client`, `std::http::server` | [ri-n5i explicit connection driver/endpoint pairs and consuming runners](http-contracts.md) |
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

Generic bodies invoke F using only established access and requirements. Concrete capture evidence survives keeping F generic. An erased arrow annotation cannot invent environment evidence. Returning/storing an erased interface conservatively loses evidence not promised by it. Calls/joins/records/enum payloads preserve borrowed dependencies and exclusive restrictions before monomorphization.

Canonical declarations:

```rill
function copy<T>(value: &T): T;
function dispose<T>(value: T): void;
```

Write `copy(&value)`, not the historical `copy(value)` convenience. Copy returns the type's explicit copy result; it never silently moves an owner through a read-only borrow or erases embedded dependencies. Strings/Bytes copy their observed extent into independent ownership. List copies require `copy(T)`; Map copies require `copy(K), copy(V), eq(K), hash(K)`. Shared/Sender copy adds an owner of the same resource without copying its payload. No standard copy is supplied for guards, receiver ownership, retained causes, Err or Result. Copying an error value deliberately through a custom user operation is not a guarantee that the standard error envelope is clonable.

Retain the existing implicit-copy scalar/read-only-view/record/tuple classification. Extend it structurally to enum variants and parents: a parent needs every possible payload to qualify, not just its current runtime variant. Strings, owned collections, counted handles and exclusive references gain no implicit-copy exception. A static unit variant can qualify independently of a wider non-copying parent enum.

### Remaining-field cleanup and protected operations

This is a newly confirmed ri-8n7 rule, not a claim that ri-owe already specified it. A custom disposer performs extra cleanup; afterward the compiler disposes remaining initialized owned fields in normal established cleanup order. Moved/explicitly disposed fields are not disposed twice. The outer disposer is not recursively invoked on its own consumed parameter. An empty custom wrapper disposer cannot suppress a contained guard/owner/native obligation. Partial initialization, moves, branches and replacement slots must be tracked. Fatal panic still promises no cleanup; normal cancellation stays deferred through required disposal/installation/join work.

Protected identities are compiler/trusted metadata attached to genuine runtime types and operations, not their spelling or module path. Required guard release, count-preserving Shared/Sender copying/release and channel endpoint ownership, and outstanding-native-operation source retention are closed ownership actions. Private fields prevent construction, not interception.

During family collection normalize aliases/type parameters and reject any non-trusted implementation that could replace/tie a required protected action, including `dispose(LockGuard<Payload>)` in Payload's module and nested-payload specializations of counted owners. Negative capabilities such as guard copying cannot be enabled through a generic adapter or new specialization. Less-specific ordinary defaults remain usable only when they cannot displace the protected action. Apply checks even without calls and preserve canonical actions through expected function values, generic requirements and implicit cleanup. Broad orphan permissions and ordinary unprotected default specialization remain; do not seal every std family/type or impose an outermost-type orphan rule.

Standard wrapper layouts contain owned, non-forgeable resource state whose required action is preserved by field cleanup. Trusted native registrations own their borrowed-source obligations until completion/confirmed abort, even after cancellation/handle drop; cancellation requests are not release confirmations. Exactly-once state transitions, publication, replacement initialization and Wasm host-pointer stability still require ri-48g/#30 validation. Private fields or this registry alone do not prove safety.

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

Map insertion without an equal key stores both inputs and returns None. Otherwise retain the old key, dispose the incoming unused key, replace the value and return the old value. Removal returns the value and disposes the removed key. None means ordinary absence/out-of-range, not an exceptional error. No mutable stored-key access; changing a key requires removal/reinsertion. The minimum interface does not include recovering a whole removed entry or panicking access conveniences.

Map/transform callbacks run once per element, sequentially in list order. They may suspend or have effects. The operation owns F; reuse requires explicit `&callback` or `&mut callback`. Results may retain valid caller/source borrows, but not destroyed owned-element storage or the operation's local callback environment. A once-only callback cannot implement repeated mapping. Borrowed map does not copy its elements; in-place mutation is traversal/getMut, not a third map meaning. There are no persistent snapshots or implicit storage sharing. Allocation failure is fatal, not a recoverable allocation Result. Public u64 sizes do not guarantee target addressability/capacity.

### Iteration

```rill
// std::iter, also used canonically by for ... of
function iterate<C, I>(source: C): I;
function next<I, T>(iterator: &mut I): Option<T>;
```

`for ... of` evaluates its source once, constructs an iterator and repeats next until None; user-defined containers may implement the same canonical families under ordinary checking/orphan rules. The compiler loads these identities for syntax without binding their names in source. User extensions/generic requirements import them explicitly. Standard nominal iterator types and yielded types are:

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

Exclusive yielded access must reach last use before advancing; no accumulating multiple exclusive references or arbitrary disjoint-index inference is promised. Structural modification conflicts with a live collection iterator. Shared yields/owned elements with embedded references retain the conservative dependencies of ri-lql; ri-48g must explicitly validate transitive last use, source versus iterator-slot storage, callee exits and caller-dependent outputs. Iteration syntax is not an exemption from those rules.

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

The retained-cause boundary is a new ri-8n7 refinement of ri-vtw. E may carry checked borrows in a primary typed failure, and unchanged forwarding preserves them. Erasing/retaining E as a heterogeneous cause requires noBorrow(E), sendable(E), shareable(E), display(E). This applies to Err(value,cause) and infix wrapping.

It is a compile-time requirement, not a dynamic flag test or an assumption from the outer type. A borrowed/affine error can be propagated directly or explicitly converted into a suitable owned error, but not hidden behind an outer error that appears safe.

Cause is a sealed, uniquely owned, non-copyable representation preserving each retained value, origin, next cause, formatter and disposal metadata. It is not a public dyn/trait/downcast facility or counted Report. Its invariant provides noBorrow/sendable/shareable evidence independently of the visible outer E. Private runtime metadata/canonical identities must enforce construction and cleanup, including through generic/erased interfaces. SourceLocation is opaque; formatLocation is its minimal public observation operation, not a stable machine-readable path/line/column record or a new coordinate-format contract. It identifies the original source construction site using the compiler's source map. SourceLocation owns its required diagnostic data (or uses justified immutable compiler metadata), never a borrowed input/temporary disguised as borrow-free state. No standard Err/Result copy is supplied merely because E/T are copyable; an optional Cause can hold non-copyable state.

`display` receives a borrow, returns an owned string and may suspend or have effects. Supply it for primitive error values, default Error and every library error; custom E needs its own canonical implementation, not reflected field printing. Standard show(Err<E>) bridges to formatError with display(E); show(Cause) bridges to formatCause. Ordinary show identity/orphan/reference-forwarding rules remain, not a sealed formatting family. Construction records origins only. Returns, postfix ?, map and andThen do not append history. Full standard rendering visits every explicit cause, using bounded-stack traversal per ri-636; no promised automatic full backtrace or eliminated-tail reconstruction. User formatters/disposers remain callbacks outside the standard recursion bound. ri-3t8 owns compiler-diagnostic presentation; runtime error rendering here must not require users to parse a fixed text format.

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

One `lock<M,T>(source:M):LockGuard<T>` family supplies &Mutex<T> and &Shared<Mutex<T>> input implementations, requiring noBorrow(T), sendable(T). These are explicit forwarding implementations, not general auto-dereference. A guard borrows/retains its source, cannot copy/send ownership, is read-shareable when T is shareable, and has only the approved built-in dereference. Mutable guard access can modify the payload; protected views prevent premature guard/source release. Dispose unlocks; it does not destroy T. No callback-only withLock, raw unlock, borrowMut or user-defined Deref protocol is introduced.

Trusted primitive state retains active waiters/native borrowers through actual completion/confirmed abort; explicit/automatic disposal cannot bypass its release state machine. Owner counts, lock publication, acquire-to-guard handoff and channel commits must remain correct across suspension/cancellation and all code paths. Source ownership alone does not establish pointer stability across Wasm memory.grow. ri-dej remains an explicit backend gate, not established support.

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

ri-636's source/build capacity contract is preserved: u64 usable addressable bytes; zero/below-minimum/excessive/unrepresentable/effective-alignment-invalid dynamic settings panic, build settings reject; reservation/backing failure is fatal. The runtime publishes actual target limits and checked upward alignment, not a portable recursion-depth guarantee. `--stack-size` sets program default, `--entry-stack-size` entry only, with equivalent run/test compile settings; no manifest/environment/optional/named parameter mechanism. Ordinary/scoped children use program default, not parent overrides. Native default usable capacity is 1 MiB; Wasm addressable and saved budgets default separately to 1 MiB per routine. `--saved-stack-size` selects the per-routine Saved capacity program-wide, is rejected on native, and does not set an aggregate quota or host-engine limits. Handle drop/cancel requests do not authorize reclamation while borrowers/cleanup remain. Full support remains gated by ri-48g's production/formal integration requirements after ri-dej's completed standalone mechanism stage. [ADR 0006](adr/0006-whole-program-monomorphization-on-llvm-23.md#confirmed-portable-wasm-lowering-direction-30) records the human-approved explicit-record/iterative-dispatch direction for portable Wasm, not an implemented ABI/runtime or closed support gate; the old tail-call/Asyncify composition is not the selected mechanism.

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

There is no hidden runtime dispatcher based on callback result types. The required v0 interface includes no variadic generics or dynamic-sized work-list helpers. Nested fixed groups and explicit worker queues remain possible. Cleanup/join latency is not bounded; timeout races are not hard deadlines. Worklists/explicit error data may grow; that is not hidden activation growth or a total-memory quota.

## 7. Channels, selection and timers

```rill
channel<T>(capacity: u64): [Sender<T>, Receiver<T>]
  where noBorrow(T), sendable(T)
send<T>(to: &Sender<T>, value: T): Result<void, SendError<T>>
  where noBorrow(T), sendable(T)
```

Zero capacity is unbuffered rendezvous; positive capacity is bounded. Capacity is explicit. There is no no-argument/default-argument constructor and no unbounded mode. Unsupported allocation/capacity does not produce a recoverable out-of-memory result; runtime target limits/checks still apply. Last Sender closes the channel; buffered messages drain before None. Last receiver ownership release makes pending/future sends fail and disposes undelivered buffered ownership exactly once. Sender copy preserves the same channel/count; receivers are shared through Shared, not duplicated by a standard receiver copy.

One `receive<S,T>(from:S):Option<T>` family implements &Receiver<T> and &Shared<Receiver<T>>, with noBorrow(T), sendable(T). This is explicit forwarding. It moves a received message or yields None once closed/drained. A pending send owns its message; success commits one transfer, failure returns the undelivered T in opaque SendError<T>. `unsent<T>(&SendError<T>):&T` inspects; `intoUnsent<T>(SendError<T>):T` recovers ownership. display(SendError<T>) needs no display(T). Cancellation never rolls back a committed transfer; abandoning an uncommitted pending operation disposes its remaining message after unregister/confirmed abort. Deferred waits remain registered through required cleanup.

Confirmed [ri-w0c](progress-io-contracts.md) additionally supplies atomic nonwaiting `trySend`/`tryReceive`, owned `SendAttempt<T>`/`ReceiveAttempt<T>`, one-token `wakeChannel`/`WakeNotifier`, and one-shot send/receive `ChannelWatch<T>` guards. These preserve the same message evidence and rendezvous atomicity. A watch reserves/consumes nothing and does not retain application endpoint counts; closure is readiness and the later try-operation is authoritative. Wake tokens coalesce hints over retained state, not result payloads or event counts. Existing blocking send/receive/select contracts and fixed select arities remain unchanged. ri-hw5 retains runtime/race/target validation.

```rill
// std::time / std::channel
sleep(milliseconds: u64): void
after(milliseconds: u64): Receiver<void>
```

Timers use monotonic time, not calendar time. after produces one void notification then closes. Zero is immediately eligible, not a forced yield. Scheduling/native work/cleanup can delay completion indefinitely; checked duration/deadline representation must not silently wrap. There is no portable maximum scheduling-latency or hard-deadline promise. [Confirmed ri-j60](clock-cancellation-contracts.md) makes the default clock suspend-inclusive, preserves these millisecond shapes and defines fatal unsupported/failure handling for their error-free signatures. New typed waits take an explicit token and return/deliver WaitError; explicit token cancellation is not intrinsic goroutine cancellation.

### select without string tags

One `select<C,R>(cases:C):R` family implements 2/3/4-item tuples whose items are `[&Receiver<Ti>, Fi]`. It requires noBorrow(Ti), sendable(Ti), callableOnce(Fi,[Option<Ti>],R). For a Shared receiver use its explicit view; select need not erase arbitrary source/container types. Handlers run in the selecting goroutine after the winning commit. Local selection alone adds no extra sendability/noBorrow constraint on F/R. Captures/results still obey dependencies and cleanup.

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

Bytes owns a contiguous byte sequence; &Bytes is logical read access to a whole/subrange, not necessarily a pointer to an owning header. &mut Bytes exclusively accesses a whole owner. As with ri-a2c string views, &mut &Bytes modifies a descriptor slot, not underlying byte permissions; generic dereference/reborrow/copy cannot fabricate an owner from a view. Source/input/temporary dependencies persist through nested values, iterators, suspension and native use.

| Home/operation | Signature |
| --- | --- |
| List to bytes | into<Bytes>(List<u8>):Bytes; consumes source; reuse optional, never assumed |
| bytes length | length(&Bytes):u64 |
| bytes slice | slice(&Bytes,u64,u64):Option<&Bytes> |
| bytes get/getMut | get(&Bytes,u64):Option<u8>; getMut(&mut Bytes,u64):Option<&mut u8> |
| bytes append | append(&mut Bytes,&Bytes):void |
| bytes reuse (ri-fnk) | clear(&mut Bytes):void; truncate(&mut Bytes,u64):void; reserve(&mut Bytes,u64):void |
| byte/List conversions | into<List<u8>>(Bytes):List<u8>; into<Bytes>(&List<u8>):Bytes; into<List<u8>>(&Bytes):List<u8> |
| string length | length(&string):u64, counts UTF-8 bytes |
| string slice | slice(&string,u64,u64):Option<&string> |
| string append | append(&mut string,&string):void; valid UTF-8 preserved |
| string bytes | into<&Bytes>(&string):&Bytes, no mutable escape |
| strict borrowed validation | tryInto<&string>(&Bytes):Result<&string,Utf8Error> |
| strict copying decode | tryInto<string>(&Bytes):Result<string,Utf8Error> |
| lossy copying decode | decodeLossy(&Bytes):string |
| scalar traversal | chars(&string):Chars; charIndices(&string):CharIndices |
| scalar conversion | into<u32>(char):u32; tryInto<char>(u32):Result<char,ScalarError> |

Slices use half-open byte bounds; reject reversed/out-of-bounds ranges, and for text reject non-scalar endpoints. Empty valid ranges succeed. Byte get may observe continuation bytes. Append cannot bypass conflicting loans/self-aliasing. Decoding errors own their diagnostic data; `Utf8Error` is a named record exposing `validUpTo:u64` and `errorLength:Option<u64>` (None for incomplete trailing input), with display. Strict borrowed success retains the original byte storage; copying success does not. Lossy decode replaces invalid input with U+FFFD rather than dropping it. Concrete invalid-sequence grouping follows the strict UTF-8 decoder's maximal valid-prefix/error-span contract and must be tested in ri-48g, not confused with a view-cast.

[ri-fnk / ADR 0018](adr/0018-streaming-and-buffered-io.md) adds Bytes reuse: clear sets length to zero while retaining reusable allocation; truncate shortens or does nothing when already shorter; reserve ensures room for at least the requested additional bytes without changing length. No exact capacity/allocator footprint or public uninitialized memory is exposed. Checked length/addressability/reservation failure follows existing fatal allocation policy; no native-size wrapping or bypass of conflicting views. Streaming reads append into a whole exclusively borrowed owner, not a new writable subrange API.

CharAt is a named record `{ offset:u64; value:char }`, exported from std::string. Chars/CharIndices are opaque source-retaining iterators, yielding scalars/CharAt through canonical next. The proposed tryInto<char>(u32) pair accepts Unicode scalars, excluding surrogates/out-of-range values, not merely printable/assigned characters. Its ScalarError has one InvalidScalar unit variant with standard display/eq. The distinct scalar payload does not broaden numeric ConversionError. No grapheme indexing, normalization, locale collation, writable UTF-8 bytes, mutable substrings, implicit List contiguity or fabricated &List<u8> temporary headers.

String literals are expressions of string type, not singleton types. Uninterpolated literal expressions can be rematerialized into fresh logical owners/static backing under ri-a2c, but a named string binding moves unless explicitly copied. Optimizer knowledge does not grant implicit copying, noBorrow to a view, capture access or a safe numeric conversion. `copy(&owner)` copies owned text; `copy(view)` already passes a &string and copies the observed extent. To copy the descriptor itself use its read-only implicit-copy rules or the appropriately typed borrowed descriptor, preserving dependencies. These are not duplicate freeable owners or hidden reference counts.

## 9. Conversion, numeric compatibility, parsing and formatting

[ri-1wl / ADR 0017](adr/0017-user-implementable-conversions.md) supersedes the closed-only conversion boundary from ri-a2c/#23:

```rill
// std::convert canonical user-extensible families
export function into<To, From>(value: From): To;
export function tryInto<To, From, E>(value: From): Result<To, E>;
// from aliases into; tryFrom aliases tryInto, same destination-first argument order.
// Historical try_into/try_from spellings are superseded, not extra implementation sets.

implement into<UserId>(value: string): UserId { /* body */ }
implement tryInto<UserId>(value: &string): Result<UserId, InvalidUserId> { /* body */ }

// Standard concrete pairs, described in RFC 0006; these are calls, not new declarations.
// tryInto<i32>(&text) selects ParseError. All twelve numeric destinations have a pair.
// tryInto<string>(&bytes) selects Utf8Error and returns independent owned text.
```

Only these canonical conversion families distinguish implementations by `(To, From)` rather than ordinary parameter patterns. E is the selected implementation's output evidence, never a selection key. Infer source/error family arguments from registration signatures. Calls use `into<To>(value)` / `tryInto<To>(value)` or an immediate unique concrete expected destination (including a Result success type). Do not choose a destination from the number of available implementations, later uses or a default. Source widths must be independently established. Concrete function values require resolved endpoints and the exact selected signature/error; there is no erased runtime registry.

Canonical requirements are `into(To, From)` and `tryInto(To, From, E)`; aliases preserve identity. Text parsing uses `tryInto(T, &string, ParseError)` for the standard numeric relationship, not a dedicated parseNumber(T) predicate. Generic bodies are checked before monomorphization using symbolic selected-signature/property/dependency evidence. Both endpoints must be named in requirements; unlike a call, no value supplies the source. E can be inferred at a concrete call, but exact equality is checked against the winning implementation. A generic body cannot assume an overridable fallback's concrete error and later substitute a differently typed specialization. Additional properties such as display(E) remain explicit. For example, `stringify<T>(value:T):string where into(string,T)` accepts any owned T with the conversion; a borrowed variant requires `into(string,&T)`.

Collect loaded implementations once. Reject identical canonical pair patterns regardless of error/constraints; aliases and generic renaming do not create distinct patterns. Choose the unique most-specific applicable source/destination pattern, allowing concrete specializations of generic defaults. Incomparable applicable patterns are ambiguous; constraints, expected errors and import order do not rank candidates. An expected-error mismatch cannot force fallback selection. The implementation module must own the family or an endpoint's outer nominal type after peeling references. Local wrappers count; foreign wrappers containing local types, bare generic parameters, local errors, constraints and aliases do not confer ownership. Ordinary families retain their existing orphan/duplicate rules and declaration-wide implementation visibility.

Built-in numeric pairs remain protected. Standard `into<T,T>` identity takes precedence for equal endpoints even over overlapping generic wrappers; explicit user infallible identity registrations are rejected. Identity preserves moves/copies/reference dependencies, not backing-storage cloning. No user infallible-to-fallible adapter or universal fallible identity is generated. Numeric safe/identity pairs retain standard fallible ConversionError entries; other fallible identity entries require explicit registration with a concrete E.

Owned T, &T and &mut T sources are distinct contracts. Calls explicitly move/borrow/copy; no implicit borrowed adapters. Destinations and errors preserve nested dependencies and exclusive access, reject local-source escape, and cannot fabricate noBorrow/sendable/shareable or extend lifetimes. Custom errors use canonical display for Err construction; exact-E propagation, explicit mapErr and existing cause/origin rules are unchanged. Infallible implementations may have ordinary effects or fatal panics; infallibility does not promise purity or losslessness.

No as, fallback width, arbitrary ordinary return-only implementations, source guessing or conversion/parser trait. No implicit casts, conversion chaining, structural mapping, automatic display/serialization/JSON/database registration or SQL-expression capture. Applications can register explicit conversions with their own semantics/errors; ri-1zg JSON numeric behavior stays distinct. ri-dka's [SQL contract](sql-contracts.md) explicitly adds bindInto/tryBind/get helpers invoking declared canonical conversions, not automatic conversion discovery/chaining. User JSON Value conversions remain explicit around parse/stringify; no new Serializable trait or structural mapping is added. Intended-valid and independent rejection cases are in [prototype 11](prototypes/11-conversions.md).

Preserve ri-a2c's semantics. Infallible integer pairs require full base-range inclusion. Integer/float and float/float destinations use IEEE rounding and may lose precision or overflow to infinity. Float-to-integer truncates first, then checks range/non-finiteness; try supports safe pairs too and keeps Result shape. ConversionError is an enum with OutOfRange and NotFinite unit variants, supplied display/eq implementations and owned origin handling through Err. No rejected infallible call becomes a panic-catching helper.

The owner confirmed strict text parsing and owned UTF-8 decoding through tryInto/tryFrom. The broader core matrix additionally proposes borrowed views, byte/List representation changes and scalar conversion through those same families. Each pair retains its specified move/copy/dependency contract and selected E. No extra source form, infallible-to-fallible adapter, numeric source width or conversion chain is inferred.

ParseError has InvalidSyntax and OutOfRange unit variants with standard display/eq. Parsing consumes the whole text, no whitespace trimming/separators/suffix/base guessing; integer parsing does not truncate fractions. Float parsing accepts ri-a2c's canonical NaN/inf/-inf/-0 and decimal grammar. Finite rounded overflow produces an error; underflow may produce zero. The standard &string-to-numeric entries cover the 12 established primitives with E=ParseError. Application endpoints retain explicit user registration and their own selected errors. Explicit radix parsing is outside the required interface.

Six ordinary same-type parameter families in std::number provide wrappingAdd/Sub/Mul and saturatingAdd/Sub/Mul for the ten integer primitives. Each declaration has `(T,T):T` shape; implementations specialize parameter types. Primitive floats have no supplied implementation. These helpers do not change checked ordinary arithmetic, bit/shift rules, constant-trap timing or introduce a general unsafe arithmetic mode. Numeric sum has the ordinary same-type arithmetic behavior, including checked integer overflow and input-order float evaluation; no reassociation promise is added.

Public parseNumber/decodeUtf8 and the replaced core helpers are not retained as compatibility aliases. Private implementation helpers remain possible. decodeLossy stays named; no default into<string>(&Bytes) lossy pair is registered. Queries, mutation, traversal and context-dependent protocols are not silently converted. Historical prototype/report spellings are preserved with migration guidance in the prototype index.

Canonical std::format::show has `show<T>(&T):string`; output is owned, inputs borrowed. Supply primitive bootstrap renderers (not self-recursive interpolation), standard read-only/exclusive-reference observation forwarding, and error-wrapper bridges. Generic interpolation needs canonical show(T), formats each expression once immediately left-to-right, and retains temporary/resource obligations through suspension. There is no local namesake/reflected record fallback or implicit show source binding. Keep ri-a2c's exact primitive formatting/parse rules; special formatting can use explicit functions, not a template mini-language.

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

Stdin may return a short nonempty read; None means EOF, not temporarily unavailable input. A zero request returns Some(empty Bytes) without consuming input. A request is an upper bound, not a promise the target can allocate maxBytes; implementations may perform smaller reads without narrowing/wrapping the public u64. Writes attempt the complete sequence; errors/cancellation can leave a committed prefix and are not rollback or safe whole-buffer-retry promises. Flush does not imply filesystem crash durability. Shared standard-stream state is trusted synchronized runtime state, not named functions capturing exported runtime lets. Native operations retain buffers/path sources until completion or confirmed release, including deferred cleanup; Wasm pointers need ri-dej validation. Owner retention alone is insufficient.

readFile returns owned bytes. writeFile creates/truncates and may leave partial output after failure/cancellation, with no atomic replacement, rollback, durable-sync or file-locking promise. Embedded-NUL/invalid platform paths return InvalidInput rather than unsafe path truncation. No general streaming-file/reader-writer abstraction, networking, seeking or directory traversal is required by this historical ri-8n7 baseline. Confirmed ri-fnk streaming, ri-vno networking and ri-dzr filesystem extensions supply their own additional contracts without changing these convenience signatures. Text I/O composes bytes and explicit UTF-8 handling.

IoError is an opaque owned diagnostic type with `kind(&IoError):IoErrorKind` and `nativeCode(&IoError):Option<i64>`. IoErrorKind has NotFound, PermissionDenied, AlreadyExists, InvalidInput, Other plus ri-dzr's Unsupported, NotDirectory, IsDirectory, DirectoryNotEmpty variants; unmapped platform failures use Other. Optional native codes are platform-specific, not portable branching contracts. Its display uses owned diagnostic context; callers do not parse human-readable messages. IoError, ConversionError, ParseError and Utf8Error provide display and noBorrow/sendable/shareable evidence. Underlying native-code translation must not make intrinsic routine cancellation a recoverable I/O error or fabricate successful completion. Explicit token requests are distinct: ri-j60 approves ordinary per-operation outcomes and ri-vno defines the concrete controlled-networking errors, without retrofitting these ri-8n7 convenience signatures.

Root log/panic each use one `(S)` declaration shape with string and &string input implementations. Log writes text plus newline, returns void, synchronizes whole messages (same stream synchronization as output operations), and has unspecified concurrent message order. Failure is fatal; fallible I/O is the alternative. It is text-only: show/interpolation is explicit, not generic reflection or implicit owner copying.

Source panic requires importing std::panic and returns never. Compiler-generated arithmetic/allocation/stack failures use canonical runtime metadata without a lexical panic binding. User shadowing cannot redirect them. Rill/runtime allocation/reservation failure is fatal, not ubiquitous recoverable allocation Results. ri-vno explicitly distinguishes a documented native operation's reported resource shortage (which may become NetErrorKind.ResourceLimit) from failure of Rill allocation, native-operation bookkeeping or error construction itself; reporting such an error can still fail fatally. ri-636 overflow uses bounded best-effort emergency reporting without user formatters/full trace/required allocation/unwind; host failure may prevent a report. Other fatal panic remains uncatchable, with no cleanup/global-exit guarantee.

## Confirmed time and cancellation extension (ri-j60)

[ADR 0022](adr/0022-jiff-time-and-explicit-cancellation.md), the [temporal contract](time-contracts.md) and [clock/token contract](clock-cancellation-contracts.md) record the explicitly confirmed expansion. Jiff-derived validated values/immutable zones, explicit source limits, exact arithmetic/text boundaries, suspend-inclusive clocks/manual controls and token-aware waits extend this baseline. Sources request one-shot state, and tokens observe it. Stop still joins. There is no generic withCancellation, public callback registry, ambient token or source CancelAfter. withDeadline preserves its R and joins losing work/cleanup. Explicit token outcomes can complete normally during masked cleanup only after safe resource release; intrinsic delivery remains deferred. ri-7ap supplies explicit Inputs for system-zone TZ, never live environment reads. The temporal and clock/cancellation examples are hand-checked, not compiler/runtime/provider proof; ri-48g/#30 retain gates.

## Confirmed networking extension (ri-vno)

[ADR 0023](adr/0023-tcp-addresses-resolution-and-controlled-networking.md), the [complete networking contract](networking-contracts.md) and [prototype 22](../prototypes/22-networking.rill) / [inventory](prototypes/22-networking.md) record explicitly confirmed Q1-Q93. Private-representation numeric values, strict pure parsers/canonical show, scoped SocketAddress and separate native scope lookup keep value identity distinct from system resolution and authenticated service identity. The lookup grammar, numeric-looking guard, owned system-ordered candidates and explicit family filters are fixed by that contract, not delegated to a permissive native literal parser.

Owned noncopyable TCP resources and split halves reuse ri-fnk's canonical std::io read/write/flush/consuming-close and IoFailure progress. Raw flush is a no-op. Disposing a BufferedWriter queue never flushes it; releasing its owned raw TcpWriteHalf still attempts the explicitly approved local send-direction release without joining the read half/routine. Native borrowers and shared state remain valid through true completion/release. A close error never authorizes retrying an unowned/recycled descriptor.

NetControl and configuration use ordinary public fields under ri-lql binding/exclusive-access rules, with stable captured value snapshots and retained dependencies; no per-field mut/readonly syntax or implicit owned handle copies. ControlledTcp borrows its source, has appropriate canonical I/O capabilities but no consuming close, and never silently closes that source merely to abort controlled work. Explicit TokenCancelled/DeadlineExpired/ClockFailure outcomes report final actual prefixes, including late effects; intrinsic cancellation remains outside Result. The clock contract explicitly gains identity-only checkInstantDomain, a canonical alias of the concurrently SQL-approved validateInstantDomain, while withDeadline preserves nested R and joined cleanup.

Dialing resolves one batch, interleaves families, uses at most two live attempts including cleanup, defaults to 250 ms spacing/ten-second attempt caps, forwards one optional overall absolute deadline, and retains the chosen winner through loser cleanup. Defaults/options, typed errors/ordered original failure envelopes and preflight/selection rules are fully specified in the linked contract. No hidden replay, protocol probes, IDNA, provider/FFI selection or stronger return-time guarantee is implied.

Live v0 networking is native Linux/macOS; portable values and typecheck-visible APIs remain on wasm32-wasip1, with typed Unsupported live operations rather than new imports/preview2/inherited-socket handoff. The 32 intended-valid expectations and all real-native matrix rows are hand-checked/NOT RUN, not Rill execution or support/security proof. ri-48g/#30 and the TLS/HTTP/SQL consumers retain their formalization/provider/protocol gates.

### Streaming and buffering extension (ri-fnk)

Q36 confirms a scoped restriction. Local probes/internal adapter buffers and consuming-close flush failures require the scoped noBorrow evidence listed below.

The user explicitly approved this conservative rule after artifact validation; [ADR 0018](adr/0018-streaming-and-buffered-io.md#scoped-borrow-free-errors-confirmed-after-artifact-validation-q36) records its reason and limits. This is design approval, not a completed checker or soundness proof.

[ri-fnk / ADR 0018](adr/0018-streaming-and-buffered-io.md) adds these ordinary canonical families in std::io; it leaves the preceding stdio/whole-file convenience signatures unchanged. Their existing partial-effect warnings do not retroactively become reliable progress counts. No global standard-handle ownership scheme, traits, methods, public dyn, return-only dispatch or new prelude binding is introduced.

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

Read appends at most maxBytes and preserves existing contents. Positive requests succeed with Some(n), 1 <= n <= maxBytes, or None at EOF with no appended bytes. Short reads are not EOF or message/UTF-8 boundaries. Zero request immediately returns Some(0) without consumption or EOF testing. Write accepts a prefix of its supplied logical extent. Nonempty success makes positive progress; empty input makes zero progress. Calls suspend the routine while unavailable, not return WouldBlock; backend execution must not simply block the scheduler thread. Counts are public u64 upper bounds, never implicit native-size wrapping/allocation promises.

IoFailure.transferred exactly reports appended/accepted bytes from this operation, including failure; no rollback, unreported destination bytes or whole-buffer retry promise. Acceptance is relative to the layer, not peer receipt/durable storage. Helpers check successful zero progress, bounds and read length/count/EOF consistency before arithmetic/slicing; impossible reports are fatal adapter bugs, not synthetic E or infinite loops. Existing-prefix preservation is required without promising byte-for-byte validation of every adapter.

Exact/all helpers loop with bounded stacks; empty exact/all operations do not touch the stream. readExact succeeds only after appending the requested count. Its errors retain the obtained prefix and cumulative progress. writeAll accepts the entire input or reports cumulative failure; it does not flush/close or create framing. Rebase IoFailure progress with metadata-preserving mapping, retaining both the I/O envelope and nested original Err<E>; new context envelopes have normal new construction origins.

readToEnd requires a maximum additional-byte budget and returns the appended count.

At the limit, probe separately for one byte. EOF succeeds. A byte yields LimitExceeded with exactly one owned extra byte, never appended beyond the bound. A simultaneous error is retained in probeFailure with its original probe-relative count; the top-level transferred is destination-relative. Probe failure without a byte is Read with cumulative progress. Zero budget still probes. Existing destination length is excluded; subtract it when enforcing a total-size budget. No unlimited variant or silently discarded lookahead.

BufferedReader/Writer own their wrapped values and have explicit nonzero u64 capacities bounding logical pending bytes, not exact allocation footprint. Zero capacity is a fatal programmer error; allocation/reservation failures retain fatal policy. No borrowed constructors, hidden background work, unbounded queues, public writable views or peek/consume API. Capabilities require wrapped evidence rather than being fabricated by construction. Reader may read ahead up to capacity; a partial refill failure reports immediately with caller-relative progress and retains the unread remainder. There is no deferred-error queue. Writer drains when necessary before accepting more input, with no implicit underlying flush merely to create space. Current-input progress may be zero when older output partially commits. Remove only confirmed prefixes and retain unsent suffixes on recoverable failure. Continued transport usability is implementation-specific.

intoParts consumes without I/O, returning the wrapped value and all unread/pending bytes in order. No silent buffer loss; pending output must precede later output if the caller preserves ordering. Reader close discards unread input and delegates underlying close, not draining to EOF.

BufferedWriter flush requires write(W,WE), flush(W,FE), display(WE), display(FE), noBorrow(WE): drain local pending output first, then underlying flush. Drain errors retain suffixes/skip underlying flush; underlying-flush errors leave local pending empty. Retry cannot replay confirmed bytes. Flush is layer handoff, not durability/peer acknowledgement. Consuming buffered close additionally requires close(W,CE), display(CE), noBorrow(FE), attempts safe underlying close even after flush failure, and preserves flush-only/close-only/both errors. The caller no longer owns the resource after either recoverable outcome. Required cleanup completes. Flush while retaining ownership first when recovery is wanted. Disposal instead discards unsent output and does not flush or perform graceful protocol shutdown; remaining-field cleanup cannot double-close a moved/consumed resource.

Operations exclusively borrow their stream; reads also exclusively borrow destination and writes retain read-only source access. Suspension/cancellation must retain buffers, callback/native state and handles until completion or confirmed abortion, not merely an abort request. No fabricated lifetimes/properties or overlapping whole-stream operations. Separate directional handles are ri-vno's decision. Intrinsic goroutine cancellation is outside Result and may leave partial external effects without returning progress to stopped code. ri-j60's explicit token-aware consuming operations can instead define typed outcomes; this does not silently change these untimed I/O signatures. Cleanup can suspend indefinitely with cancellation deferred, including after close's cancellable flush phase. No force-kill, fatal-panic cleanup or global-exit delivery promise.

ri-j60 owns concrete deadlines/context/races; no timeout parameter is added here or hard return promised by racing a timer. ri-vno/#35/#38 own actual socket/file/TLS handles, shutdown and fatal states; ri-n5i/#40 own body/driver abandonment and connection reuse. Disposal does not acknowledge input or guarantee remote rollback. ri-dej still gates native/Wasm pointer stability and scheduling/cleanup machinery. [Prototype 12](../prototypes/12-streaming-io.rill) and its [case inventory/composition sketches](prototypes/12-streaming-io.md) are hand-checked evidence, not implementations or validation proofs.

### Native paths and filesystem extension (ri-dzr)

[ri-dzr / ADR 0019](adr/0019-paths-files-and-filesystem-operations.md) confirms the filesystem extension. These are public contracts, not an implemented filesystem/provider or security proof. Ordinary families retain canonical identity/parameter-pattern collection; implementations use `implement`. No methods, traits, return-only dispatch or new prelude bindings.

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

For join, an absolute child replaces the base, and an empty child/base preserves the other. Insert a separator only when needed, preserving spelling. Components retain root slash count and explicit `.`/`..`, skip empty interior/trailing components, and preserve source dependencies through canonical iterate/next. fileName ignores trailing separators and returns only a final ordinary name. parent removes the last component and preceding separator run while preserving the root run: `a/b/` -> `a`, `a` -> empty, `/a` -> `/`, `//a` -> `//`, `a/.` and `a/..` -> `a`; roots/empty have no parent. No lexical operation is symlink resolution, filesystem identity or confinement; exact-two-leading-slash native behavior is not normalized away.

ReadFile implements read/close; WriteFile implements write/flush/close; ReadWriteFile implements both directions/flush/close. All have seek and borrowed-handle metadata; only write-capable handles have sync. TempFile delegates the same read/write/seek/flush/sync/metadata capabilities to its owned ReadWriteFile but has its own close error below. File primitive errors are IoError; read/write errors are IoFailure<IoError>. Handle owners are not implicitly copied; no duplication/shared-cursor wrapper. Generic file requirements are seek(F)/sync(F) with fixed IoError; metadata uses distinct &Path/&ReadFile/&WriteFile/&ReadWriteFile/&TempFile input patterns. Access modes are static, but object seekability is not; filesystem pipes/devices can fail Unsupported and directory byte I/O is IsDirectory.

WriteOptions fields are mandatory. Read-only never creates. ExistingOnly requires existence; IfMissing does not truncate unless explicitly requested; NewOnly atomically fails AlreadyExists on any existing final entry including a dangling link. Preserve starts at zero without shortening; Truncate is destructive; Append obeys native append regardless of seek. Every combination is meaningful. Unsupported faithful exclusivity/append must fail, not be emulated by prechecks. Each underlying append is distinct; writeAll has no whole-record atomicity. Seek checks negative/native-range errors without wrapping, returns absolute u64 position and does not extend/write merely by seeking beyond EOF. Separate handles do not isolate contents from external changes. Open/create/truncate may leave effects after later failure/cancellation; finish safe cleanup, not rollback.

Unbuffered file flush has no Rill-owned output buffer and does not sync. Explicit file sync requests platform data/required-metadata synchronization; syncDirectory separately requests directory metadata synchronization. There is no universal hardware/crash transaction, silent skip or transparent buffered seek/sync. Flush/decompose adapters explicitly and account for unread/pending bytes; Current uses the actual advanced underlying cursor.

Metadata size is Some only for regular files. Pending ri-j60 coordination, there are no timestamps. This contract adds no ownership/ACL management, cached entry-kind/identity or exists helper. Path metadata follows the final link; symlinkMetadata/readLink inspect it but intermediate links can still resolve. Relative link targets resolve from the link parent. Ordinary opens/directory lookup follow native traversal. removeFile/rename operate on the final link; removeDir does not follow a link to recursively delete a target. No hard-link/all-components-no-symlink API or portable sandbox is implied. Native currentDir observes process state; no process-wide setter, and external embedding changes can still affect relative paths.

ReadDir uses canonical iterate/next: next(&mut ReadDir) yields Option<Result<DirEntry,IoError>>. Owned one-component entry names exclude `.`/`..`; order is unspecified, not a snapshot or recursive walk. EOF or one yielded enumeration error makes next terminal. Remaining enumeration state stays owned until close(ReadDir):Result<void,IoError> or disposal, exactly once. createDir creates one entry and errors on existence; createDirs iteratively creates missing ancestors and accepts existing directories, with possible partial effects. removeDir only removes empty directories; no recursive deletion helper. Rename uses native compatible-target replacement, not portable no-replace or copy-and-delete fallback; cross-filesystem/unsupported cases fail and success is not durability.

Ordinary creation requests conventional native permissions filtered by host umask/ACL/capability policy; temporaries request owner-only access. There is no process umask mutation or general chmod/chown/ACL API. TempFile/TempDir require an explicit parent, fresh exclusive names and a finite collision-retry policy, not env/global temp selection or secrecy tokens. Missing faithful private temporary-creation support is Unsupported. tempPath borrows the guard; keep consumes/disarms deletion without renaming/syncing/closing. TempFile close closes then attempts removal even after close failure, preserving close-only/remove-only/both envelopes. TempDir close attempts empty-directory removal with IoError. Both consume on failure; disposal attempts best-effort nonrecursive removal and required native cleanup without a recoverable error/deletion guarantee. External name replacement affects path-based cleanup; moved initialized fields cannot be closed twice. Standard display for TempFileCloseError preserves inspectable owned typed errors.

IoError adds Unsupported/NotDirectory/IsDirectory/DirectoryNotEmpty; authority denial is PermissionDenied, invalid paths/encoding are InvalidInput. Native codes remain optional/nonportable. WASI absolute paths choose the longest component-boundary preopen guest-name match, relative paths require `.`; no first-preopen guess, ambient access or broader-capability fallback after denial. Leave `..` for host-enforced resolution after capability binding. Ambiguous/unusable mappings fail explicitly. Missing namespace/operation support is Unsupported; currentDir is Unsupported on WASI v0. Provider/private-creation/namespace guarantees require actual support, not reduced-success fallbacks. ri-48g must formalize root/mount spelling and errors; host mechanisms are not validated here.

All path/buffer/handle/native dependencies survive suspension until actual completion or confirmed abortion. Cancellation is outside Result, may leave effects and cleanup can wait indefinitely. Do not blindly retry interrupted native close across descriptor reuse. Existing ri-fnk Q36 scoped requirements remain; standard IoError already satisfies them. ri-dej gates actual native/Wasm scheduling, pointers and reclamation. Process spawning/watching, general ACL management, migration/static-file frameworks and new copyFile helper are excluded. [Prototype 13](../prototypes/13-filesystem.rill) and its [case inventory](prototypes/13-filesystem.md) cover lexical checks and bounded copy/config/certificate/directory/temp workflows by inspection, not execution/security proof.

### Standard SQL interface (ri-dka)

ri-dka / [ADR 0021](adr/0021-partial-standard-sql-contract.md) records the explicitly confirmed shared-interface design, including Q58-Q77 integration after ri-j60 closed. [The SQL contract](sql-contracts.md) owns fixed callable/control/error shapes, generic requirements, temporal mappings and defaults. This supersedes the partial checkpoint, not implementation/RFC/provider validation. ri-3id separately tracks the still-unfulfilled embedded native-provider prerequisite; it does not block shared-interface design closure or RFC drafting.

- Canonical driver-facing families live under std::sql::driver, with identity-preserving std::sql exports/helpers. Concrete typed connectors select external drivers; concrete pools, dialect/builders/ORM/migrations remain external. No global registry, driver-name lookup, universal connection-string parser, public dyn, reflection, associated types or general FFI.
- Common Parameter is an owning subset (typed NULL, bool, signed/unsigned widths, f32/f64, text, bytes, Decimal, ri-j60 Date/Time/DateTime/Timestamp), not the only driver argument type. Default arguments/rows/prepared descriptors provide noBorrow evidence, not implicit send/share or blanket error constraints. Borrowed views retain their actual owners.
- Direct bind encodes driver-supported values; bindInto explicitly invokes into<Parameter>; tryBind invokes tryInto<Parameter> and preserves conversion/binding errors separately. get<T> is one generic cell-access-plus-canonical-conversion helper, not return-only ordinary implementations. Source/destination/error evidence remains explicit before monomorphization; Into does not create TryInto. User modules can provide SQL and JSON Value conversions without importing every driver or adding a serialization trait. One SQL conversion occupies one slot; record/table mapping remains explicit/external.
- Checked extraction rejects incompatible NULL/type/range/precision/encoding; Option<T> preserves nullability. No silent numeric float intermediate, text replacement, unsupported-float-to-NULL or decimal text fallback. Parameter limits do not audit coercions performed by SQL/columns/triggers. Decimal validates/preserves a finite JSON-number-style token without float conversion/exponent expansion or a new arithmetic library; explicit token-budget override and borrowed decimalText are available.
- DbError<E> has first-class uniqueness/native and local argument/decode/cardinality/state/unsupported/limit cases. Unique details name ordered direct key columns when established, distinguish expressions/partial conditions, preserve qualified identity/provenance, and explicitly say unavailable rather than fabricate a universal column list. Metadata enrichment is explicit optional I/O, not an error accessor side effect or failure-time snapshot proof. Original native and codec Err payloads stay typed fields, not automatic erased causes. Default diagnostics omit raw SQL/values/native/codec/expression text; no recursive native-cause rendering bypass.
- Raw SQL/arguments are consumed; prepared descriptors are borrowed and independent of bounded session-owned native caches. Session use is exclusive, rows own data, cursors retain dependencies, iteration yields Option<Result<Row,DbError<E>>>. One statement/result stream, exact zero-based/name lookup with ambiguity errors, strict queryOne/queryOptional cardinality, no unlimited collector or silent batch-prefix execution.
- Transactions exclusively retain sessions; commit/rollback consume guards and distinguish confirmed committed/rolled-back/unknown outcomes from cleanup failures/reuse state. Abandoned transactions discard; early row abandonment requires actual safe cleanup evidence or discard. External pools validate/reset candidates. No automatic replay or cleanup-as-rollback guarantee. Unsupported isolation/read-only requests fail rather than substitute; managed transaction control is not mixed with ad-hoc commands or enforced by an imaginary SQL parser.
- Accepted configurable defaults: parameter/field payload 16 MiB each, row/frame 64 MiB each, metadata/decimal token 1 MiB each, 4,096 columns and 32,767 parameters. These are safety defaults, not benchmarked optimal settings, exact allocator/native/server-memory bounds or permission to exceed backend limits. Check before controlled allocation where possible; safe resynchronization or discard on limit errors. Source/native retention and ri-dej backend gates remain. Intrinsic stopping is outside Result; explicit SQL controls may return typed cancellation/expiry/clock errors after required release. Cleanup still can wait indefinitely.

Controlled connect/prepare/execute/query/begin/commit/rollback/resolveUnique/reset consume SqlControl. It owns Option<[Clock,Instant]> plus CancellationToken; uncontrolled uses no deadline/neverCanceledToken with no clock read. Explicit copy preserves clock/token state and target, not a restarted timeout. A cursor retains query control for active next, including buffered rows; an exhausted/failed stream stays terminal without later control errors. Sessions/idle transactions do not inherit connect/begin controls. Value-only helpers and canonical close keep their existing shapes; mandatory cleanup ignores external requests. Validate basic/domain arguments, then an already-requested token, then clock/deadline; post-admission races have no guaranteed winner and later requests cannot replace selected success.

DbError adds Cancelled/DeadlineExceeded observations (NotSubmitted or MayHaveExecuted), ClockFailure retaining Err<ClockError>, optional typed native diagnostics and Temporal retaining Err<TimeError>. Observations are relative to the requested operation/query, not prior transaction work/cleanup. Phase errors/outcomes/disposition remain separate; TransactionFailure and QueryFailure retain primary and cleanup/close failures without unifying native types. Reset is optional controlled baseline restoration, not hidden reconnect/replay. Exact temporal mappings reject range/precision/sentinel loss; civil values, instants, zones/rules and calendar/elapsed intervals are not interchanged. ri-j60's withDeadline preserves nested Results and joins cleanup; it is not an automatic SQL token/context wrapper.

The approved ri-j60 consumer amendment adds std::time::validateInstantDomain(&Clock,Instant):Result<void,ClockError>, which checks immutable domain compatibility only. It performs no clock read, timer operation, ID exposure or service-health check. This lets external SQL drivers obey preflight without bypassing clock privacy. It is recorded back to ri-j60 rather than overwriting separate drafts.

[Prototype 16](../prototypes/16-sql-codecs/main.rill), [domain codecs](../prototypes/16-sql-codecs/domain.rill), [control/temporal entry](../prototypes/16-sql-codecs/control.rill) and [inventory](prototypes/16-sql-codecs.md) supply 20 + 12 = **32 hand-checked expectations** and independent adapter/transaction/privacy/race/invalid traces. Memory cells/counter/encoder are not drivers or pending-I/O implementations. [Research](research/0040-standard-sql-and-unique-errors.md) cites backend evidence; no Rill/time/database/provider/security proof is claimed. The JSON, time and random examples remain separate. ri-48g/#30 and ri-3id retain formal/native/provider gates; closing ri-dka accepts design input, not future RFCs or runtime support.

### External SQLite provider design (ri-3id)

ri-3id / [ADR 0024](adr/0024-external-sqlite-native-provider.md) records explicitly confirmed Q1-Q58 in the [provider contract](sqlite-provider-contracts.md) and [binding/wire specification](sqlite-provider-binding-v1.md). The concrete driver/provider/engine dependency stays external: **no std::sqlite or general FFI**. The narrowly approved runtime/build mechanism is missing. It must implement fixed versioned metadata/C shim, copied values/errors, resource offers/adoption, bounded native queues, generation-scoped interrupts/wakes and actual cleanup. Selected 3.54.1 profile, protected authorizer/Unix VFS, transaction/cache/reuse restrictions and effective limits are design input, not another set of standard families.

[SQLite provider experiment](../experiments/sqlite-provider/README.md) separates 98 standalone native fixture checks and one-host prefix/visibility/coexistence evidence from 32 Python model/schema/header checks. Native reference caps are lower: 2,000 columns / 32,766 slots. These do not change shared maxima or prove an implemented driver. Host SINGLETHREAD reconfiguration is unavailable; actual C shim/Rill artifact consumer/runtime integration and other target cells are NOT RUN. **ri-3id remains OPEN** with design recorded and implementation/integration validation outstanding; ri-dej gates remain. No RFC/support/security clearance, hidden reset/replay or new native authority follows.

## Confirmed resumable I/O progress extension (ri-w0c)

[ADR0027](adr/0027-single-owner-progress-and-coalescing-wakes.md), the [complete progress contract](progress-io-contracts.md), [prototype24](../prototypes/24-progress-io.rill) and its [case inventory](prototypes/24-progress-io.md) record the human-confirmed single-owner progress foundation. `std::io::progress` owns explicit admission/advance/wait/stop families and typed schedule/lane/driver outcomes. Generic code names concrete D/G and independently selected RE/WE/FE/CE/DE evidence; there is no trait, Future/Poll, associated output, public dyn or automatic capability from canonical I/O.

`advance` performs bounded local work and reports Runnable/Waiting/Idle without waiting for external progress. One retained read, write, flush barrier and close/stop lifecycle keep completions bounded and non-overwritable. Driver registrations borrow their source, coalesce result-free wakes and must quiesce before mutation. Coordinator-owned token/deadline/application policy remains outside the driver; cancellation/drop/wake is never release confirmation or rollback. Standard operations use owned bytes and 16 KiB read/write units. Executor/ring/app pressure is pending; inability to retain required bounded state is typed terminal failure. There is no finite liveness/cleanup promise.

`std::net::TcpProgress` consumes a whole stream and privately permits one read and one write job under one owner. `std::tls::TlsProgress<D>` consumes an eligible lower progress driver before handshake, retains TlsError at every TLS layer and uses the fixed private profile/amendments in the TLS documents. No raw recovery, TLS split or controlled overload is added. Names stay typecheck-visible across targets while live standard providers remain native Linux/macOS scope. Prototype declarations are HandChecked; the separate OpenSSL matrix is ReferenceExecuted; ri-hw5/#44 retain all Rill/runtime/TCP/provider support gates. Closing ri-w0c is design-only and unblocks ri-n5i design, not implementation.

## Confirmed TLS extension (ri-irw)

[ADR 0025](adr/0025-explicit-tls-trust-and-private-provider-boundary.md), the [TLS contract](tls-contracts.md), [closed native boundary](tls-native-boundary.md) and [prototype 23](prototypes/23-tls.md) record consolidated Q1-Q80. `std::tls` owns eligible canonical streams, immutable explicitly copied snapshots and consumed handshake inputs. Independently selected read/write/flush/close error types each need display/noBorrow/sendable/shareable for unique retained causes; this does not change primitive I/O or impose global noBorrow(S). Native opacity is not property evidence.

TLS 1.3 by default and hardened explicit TLS 1.2, explicit CA roots and strict offline full-path CRLs, SAN reference verification, controlled SNI/ALPN, checked wall time and typed Unsupported Wasm provider operations are deliberate policy. No OS/bundled trust, online/soft-fail revocation, encrypted key containers, TLS split, insecure override, general FFI or implicit dial/replay. NetControl, actual plaintext prefixes and joined native release remain explicit; admitted failure poisons, TCP EOF is not close-notify, disposal is not graceful close. ri-n5i must not assume independent duplex TLS drivers or zero-configuration HTTPS trust.

Controlled private OpenSSL 3.5 LTS is a conditional provider direction. It has not passed current conformance or compiler/runtime integration. ri-ei7 gates delivery/TLS support/v0 readiness; ri-dej/#43 stay independently OPEN and ri-48g drafting is not acceptance. All prototype/source/reference checks are hand-checked or static, not Rill/native/security tests. No actual layouts/generated bindings, target support or public native-extension authority follows from the semantic operation catalogue.

## Confirmed standard HTTP foundation (ri-n5i)

[ADR0028](adr/0028-strict-http1-codec-and-explicit-connection-drivers.md), the [complete HTTP contract](http-contracts.md), [prototype25](../prototypes/25-http.rill) and its [case inventory](prototypes/25-http.md) record the human-confirmed direct-origin HTTP foundation. Shared owning values preserve encoded URI/authority spelling and ordered repeated byte headers; private-field heads prevent bypassing method/target validation. The public `std::http::h1` codec is transport-independent, strict CRLF/octet parsing with explicit consumed prefixes/suffix ownership and plan-bound incremental encoding.

HTTP1.1 is initial with close-only1.0 compatibility. Framing rejects repeated/comma Content-Length and every TE+CL combination, supports only actual chunked coding, separates informational/final heads and late trailers, and repairs no obs-fold/bare-LF ambiguity. Valid4xx/5xx stays data. No HTTP2/3/QUIC/proxy/successful CONNECT/101, browser URL behavior, automatic retry/decompression/cookies/JSON or raw transport escape.

Client/server constructors consume a ri-w0c progress driver and return one explicit runner plus bounded application endpoint. Callers scope/spawn/join the runner; it invokes no handlers. One exchange is active; distinct noncopy endpoints allow explicit duplex app work. HttpControl is owning/absolute per exchange. Exchange completion uniquely owns lower typed errors, cleanup, layer-relative observations and conservative reuse. Drop/cancel/deadline is not completion/nonexecution/replay safety.

All prototype25 expectations are HandChecked. Standards research and ri-w0c's ReferenceExecuted OpenSSL matrix are not HTTP execution. ri-19d retains codec/runner/adversarial/conformance/security work; ri-hw5/#44 retain lower delivery. Closing ri-n5i is design-only and supplies ri-48g input, not accepted RFC/support.

## 11. Validation and follow-through

[The library examples](../prototypes/09-library-contracts.rill) and [case inventory](prototypes/09-library-contracts.md) contain hand-checked intended-valid and rejected cases. They are not compiler-executed tests or a demonstration of stack, callback, native, or borrow-checker soundness. Earlier fixtures whose no-arg channels, copy(value), variadic work calls, plain all, string tags, implicit Error construction or propagation reports conflict are historical in those respects; do not count them as v0-valid without migration.

ri-48g must reconcile these interfaces with all source tickets, formally specify generic requirement/candidate inference (including direct tuple literals/overloaded callbacks), source/iterator/callback dependencies, callee exits, mutable storage and transitive last use, enum joins/widening, property evidence and sealed cause invariants. Validate protected overlaps/negative capabilities/remaining-field cleanup, exactly-once native/channel state, cancellation masks, standard bounded-stack traversal/default release, numeric/parser/UTF-8 edge cases, alias identity/public contracts, and both accepted/rejected adversarial examples. A missing consistent required rule is returned to the map, not concealed by a conservative-sounding safety claim. ri-dej separately gates pinned backend/ABI/Wasm mechanisms; no target compatibility proof is asserted here.

ri-3t8 owns diagnostics; ri-2yn supplies the accepted realistic-program evidence; ri-1zg records the separate [JSON contract](json-contracts.md). ri-1wl owns the requested user-extensible camelCase conversion/error interface, including reconciliation of the earlier closed numeric declarations above; it is not settled by the JSON ticket. ri-9o5 owns final RFC organization/status. These design inputs neither implement the standard library nor accept the future feature proposals.

## 12. Environment and argument inputs

ri-7ap / [ADR 0019](adr/0019-owned-startup-environment-and-arguments.md) add this explicitly confirmed application foundation after ri-8n7. This section is the current std::env contract, not a live-process configuration API.

### Public API and ownership

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

Validate lookup names even on empty Inputs. Invalid queries yield InvalidName/LookupName, not absence. Missing valid names produce Ok(None); present-empty produces Ok(Some(empty bytes/text)). getBytes never validates Unicode. Text get validates only the selected value; unrelated non-UTF-8 keys/values do not break a valid lookup. Bulk args/vars conversion stops on its first invalid UTF-8 item without skipping or replacing it. Conversion follows argument order for args and canonical variable order with name before value for vars. Returned text is owned; explicit lossy conversion remains existing byte/string composition.

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

Capture once at a controlled runtime point before Rill workers. No later operation consults mutable process-global state or guest-libc environment state. Publish an immutable owned outcome containing captured data or safe acquisition/validation failure information. Each inputs call creates its own owned Inputs or EnvError result under ordinary construction-origin rules, without cloning unique Err/Cause state, executing user formatters at bootstrap or terminating early merely for malformed input. There is no publicly mutable runtime global or module initializer; named functions still receive application state explicitly.

An embedding host must supply owned inputs or establish a genuinely safe capture point; an internal mutex cannot coordinate foreign native mutation. Synthetic/host-supplied pairs use the same validation rules. Native Linux/macOS and wasm32-wasip1 share the API; WASI sees only instance-provided data, not ambient host inheritance, and a host may restrict injection to UTF-8. Empty supplied data is valid. Ordinary reported acquisition failures remain Result errors. Allocation failure, fatal host traps and inability to instantiate are not recoverable EnvError promises; ri-dej still gates backend/host-pointer support. No Windows behavior is implied.

In rill test, skip actual host capture and use empty arguments/variables, regardless of developer/CI environment and runner flags. Tests needing data call fromParts explicitly; no host-inheritance testing switch is added. Build/runner options are not implicitly reclassified as application arguments; the API preserves only the input sequence actually supplied to the application. Flag parsing, dotenv/layered configuration, process spawning/mutation, executable-location discovery and an ambient-host opt-in remain outside this scope.

### Evidence and follow-through

[Prototype 14](../prototypes/14-environment-inputs.rill) and its [independent cases](prototypes/14-environment-inputs.md) cover byte preservation, text ownership, validation priority, case/order/missing/empty semantics, error positions, explicit worker configuration and isolated tests. Examples are hand-checked, not compiler/runtime execution. Standard capture/validation/sorting/copying/decoding/default cleanup must bound call-stack use per ri-636; target limits and fatal allocation policy remain, with no silent truncation.

ri-48g must formalize conservative result/key dependencies, callee exits, opaque property/copy evidence, source construction origins, safe startup/embedding capture and immutable outcome publication, checked host count/size/read consistency, exactly-once cleanup and cancellation of partial copies/conversions. Host traps and arbitrary concurrent foreign mutation cannot be made safe by interface wording. ri-3t8's confirmed diagnostics model applies; ri-dzr owns actual filesystem paths. Return an unworkable required checking/platform rule to the map; design approval is neither a safety proof nor final RFC acceptance.

## 13. Secure random bytes

ri-w9z / [ADR 0020](adr/0020-secure-random-byte-generation.md) add this explicitly confirmed secure-byte foundation without reopening ri-8n7 or changing ri-7ap's environment/test contract.

### Public API and security boundary

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

Use checked u64 requests and existing target addressability/Bytes capacity/allocation behavior, with no silent truncation/narrowing or artificial portable total-size quota. Unrepresentable capacity/allocation failures remain fatal, not recoverable RandomError variants. Callers bound untrusted sizes. Chunking/checkpoints are implementation details. Check remaining lengths, offsets and native size_t/WASI u32 conversions against real provider limits. Standard traversal/cleanup bounds call-stack growth per ri-636; no full-request shadow buffer or zeroization protocol is required by the public contract.

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

Potentially blocking native provider work uses the runtime blocking facility instead of an ordinary scheduler worker. Usually fast small requests do not establish a nonblocking guarantee. Provider calls use bounded chunks; positive counts advance only the confirmed region, retries cannot spin without normal cancellation/fairness opportunities, and errors never publish a partially complete owned result. There is no new public timeout/try-nonblocking API.

Retain backing owners and all exclusive-access restrictions until actual native completion or confirmed abortion. Requesting cancellation, dropping a handle or moving work to a blocking pool does not end native writes. Cleanup can suspend indefinitely; no hard return/cancel deadline, force kill, rollback or fatal-panic cleanup promise. Partial allocations/copies/owned output dispose exactly once only after source/native obligations end.

Wasm's synchronous import may stall its single-threaded scheduler. Offload/reentrancy/memory.grow safety needs backend validation. A guest offset is not a permanently valid host pointer. Host-owned staging with safe copy-back is a candidate, not an implemented or universally required mechanism. ri-dej's existing support gates remain; unsupported integration must not be silently described as a working asynchronous entropy provider.

rill test keeps real secure provider behavior; it does not automatically seed/replace std::random, even though std::env intentionally supplies empty inputs in tests. Application consumers can explicitly accept fake callables in their own code. Failure/partial-write/retry tests of the standard adapter use structurally separate private harness/linkage, never a public production seed/provider override or new user-implementable secure source. Local aliases/shadowing do not change canonical identity or the standard support/security contract. Tests must not assert exact random values, all-nonzero output or guaranteed uniqueness; observations cannot certify unpredictability.

Output is ordinary Bytes, without locked-memory, secure erase or protection from explicit application printing. Seeded PRNGs, ranges/floats/shuffling, UUID/token formatting, cryptographic algorithms and special secret storage remain separate designs.

### Evidence and follow-through

[Prototype 15](../prototypes/15-secure-random.rill) and its [independent inventory](prototypes/15-secure-random.md) demonstrate empty calls, ordinary production call shapes and explicit application fakes without a standard-provider override. They are hand-checked source examples, not live provider tests or entropy/security proof. ADR 0020 records exact primary-source contracts and differences; no historical Linux size limit, macOS getentropy limit or Wasmtime default limit becomes a universal API cap.

ri-48g must validate checked count/offset/chunk progress, source error mapping/origins and buffer-safe diagnostics, native blocking/retention/cleanup, cancellation/retries and non-escaping owned partial output. Specify/link the chosen macOS system framework and actual supported WASI provider configurations, including synchronous stalls, host traps and memory relocation/reentrancy. ri-irw must use a justified TLS entropy/provider configuration, not infer that this byte API alone establishes TLS security or authorizes entropy overrides/general FFI. Missing necessary checking/provider mechanisms return to the map; design approval neither implements the library nor accepts the future RFC.
