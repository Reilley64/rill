# Concurrency safety and cooperative cancellation

Issue ri-40y separates transfer safety from concurrent read-sharing, uses owned lock guards for synchronized mutation, and requires scoped work to finish before releasing borrowed sources. Native execution is parallel M:N with worker migration; Wasm retains its single-threaded implementation. Cooperative cancellation performs ownership-aware cleanup rather than using ADR 0005's fatal panic path.

**Status:** approved design direction, not an implemented checker/runtime or a safety proof. ri-8n7 must specify evidence-preserving library interfaces and ri-48g must supply explicit checking rules, cancellation/commit state machines, and adversarial examples. This resolves ADR 0007's pending cancellation/replacement policy without relaxing its borrow rules.

## Confirmed library interface refinements (ri-8n7)

[ADR 0015](0015-language-required-library-contracts.md) and the [catalogue](../standard-library-contracts.md) finalize std::mutex factories/forwarding, generic once-callable spawn/spawnWithStack, tuple-based scoped helpers with plain join separate from fallible all, explicit channel capacity, undelivered-message errors and receiver/handler select tuples. Copying explicitly borrows its input; canonical protected actions plus remaining-field cleanup preserve nested obligations. ri-636/#30 separately govern stack policy/backend validation; no compatible backend or safe host-pointer mechanism is inferred from these interfaces. Existing transfer, guard scope-exit, cancellation masking and cleanup-before-return rules remain.

## Confirmed explicit token and deadline extension (ri-j60)

[ADR 0022](0022-jiff-time-and-explicit-cancellation.md) and the [clock/cancellation contract](../clock-cancellation-contracts.md) add explicit source/token signals, links, observation and a unique request-on-disposal guard without changing stop's join or native-source retention. Tokens do not attach themselves to goroutines. New token-aware operations return ordinary typed outcomes only after safe release; polling/normal token outcomes remain visible during runtime cancellation masking, which still defers intrinsic delivery and cannot abandon required cleanup. Generic withCancellation was explicitly dropped. withDeadline uses ordinary scoped evidence and joins losing work; no hard return bound or force kill. Sources/token handles and the signal-only guard are not mutex guards or inferred completion owners. ri-48g/#30 still validate state/evidence/backend mechanisms.

## Transfer, sharing, and scheduling

`sendable(T)` and `shareable(T)` are compiler-recognized predicates, not user-implementable overloads. They are separate from `noBorrow(T)` and copyability; ri-tb9's exclusion of named constraint groups remains. Check generic bodies before monomorphization, using explicit individual requirements rather than assuming properties from eventual instantiations or erased callable annotations.

| Type | `sendable` | `shareable` |
| --- | --- | --- |
| Scalars and immutable strings | Yes | Yes |
| Record, tuple, list, enum | Corresponding property of every contained value/member | Corresponding property of every contained value/member |
| `&T` | Requires `shareable(T)` | Requires `shareable(T)` |
| `&mut T` | Requires `sendable(T)` | Requires `shareable(T)` for read-only access to the reference |
| `Shared<T>` | Requires `sendable(T)` and `shareable(T)` | Requires `sendable(T)` and `shareable(T)` |
| `Mutex<T>` | Yes under its payload requirements | Yes under its payload requirements |
| Channel endpoints | Yes under their message requirements | Yes under their message requirements |
| `Routine` | Yes: synchronized control handle | Yes: synchronized control handle |
| `LockGuard<T>` | No: ownership stays in the locking goroutine | Requires `shareable(T)` |

Derive callable properties from captures independently of read-only/mutable/consuming invocation mode. An erased arrow type does not invent environment evidence. Trusted runtime types need justified rules, including synchronization visibility/publication as well as mutual exclusion; owner counts alone do not grant mutable payload access. Ordinary records, wrappers, copies, or annotations cannot bypass a contained restriction. Sharing read-only access to an `&mut T` does not grant competing mutable access, permit moving it out, or duplicate its exclusivity. A synchronized handle is not automatically copyable.

Native goroutines can run in parallel and resume on different OS workers. Moving an execution context is not concurrent duplication or an ownership transfer to a second goroutine. No user-visible OS-thread-affine resource types or pinning interface in v0: integrations requiring affinity hide it behind operations and disposal valid from any worker. A lock guard is goroutine-affine, not OS-thread-affine. Budget-check yields, as well as suspending/waiting operations, are normal cooperative cancellation points.

Named functions do not implicitly capture enclosing or top-level runtime `let` bindings, mutable or immutable. Pass runtime state explicitly, or use checked arrow closures. Types and function declarations remain accessible under module rules; constant folding does not change a runtime binding's access rules. Apply this restriction through indirect calls and `dispose` overloads too. Trusted services such as logging need justified synchronization contracts; named function symbols cannot hide unchecked mutable environments.

## Shared mutation and lock guards

`Mutex<T>` payloads and channel messages require `noBorrow(T)` and `sendable(T)`, not `shareable(T)`. This intentionally restricts those library containers, not references in ordinary records/collections or scoped helpers. Channel endpoints synchronize sends, receives, closure, and cancellation; `Shared<Receiver<T>>` remains supported. A mutable `Shared<T>` handle binding never grants ordinary mutable payload access.

```ts
let mut guard = lock(&counter);
*guard += 1;
dispose(guard);
```

Acquisition parks the goroutine rather than blocking an OS worker. `lock` returns a non-copyable `LockGuard<T>` borrowing its mutex source, not a secretly cloned/owned handle. The source must remain valid through guard disposal. Returning a guard borrowing a caller-owned mutex can be valid; returning one borrowing a local mutex cannot. A guard carries a borrow and is not `noBorrow`.

The genuine standard-library guard has built-in dereference: read-only access can read the protected value, while a mutable guard binding/exclusive guard access can modify it. `&*guard` and `&mut *guard` produce borrows tied to the guard, not just the mutex. Existing borrow rules forbid moving/disposing the guard while such borrows are still needed, including through retained values or scoped child cleanup. Dereferencing does not permit moving out non-copying payloads. Replacement uses the normal ownership/disposal rules. No user-defined `Deref` traits or shape/name-based operator overloading are introduced.

[ADR 0010](0010-file-modules-explicit-exports-and-privacy.md) protects required built-in release/ownership operations from replacement by ordinary overloads, including nested-payload specializations otherwise allowed by the orphan rule. Private representation alone is not the protection; ri-8n7/#28 must specify the exact trusted-operation boundary. This does not impose a blanket outermost-type placement rule on normal copy/dispose defaults.

Guard disposal unlocks, not disposes the payload; absent explicit early `dispose`, its disposer runs at scope exit. It is not unlocked merely after its last explicit mention. No public raw unlock or callback-only `withLock` interface in v0. The interview's earlier callback result restriction and special callback-scoped compiler contract are superseded by ordinary guard-dependent borrowing; no `borrowMut` accessor is required.

Guards can remain held across suspension and worker migration. Ownership cannot transfer to another goroutine, even through wrappers. Read-only scoped sharing is allowed when the payload is shareable; protected payload loans follow their ordinary transfer/access rules and retain the guard until their uses finish. Mutexes are non-reentrant: reacquisition by the goroutine holding the lock is a bug/panic. Different goroutines/locks can still deadlock; no general cycle prevention or hard completion bound is promised.

Defer user-facing atomics/memory-ordering operations beyond v0. Runtime-internal atomics, including `Shared` owner counts, remain.

## Scoped work and stopping

Background `spawn` owns its zero-argument, void-returning callable and requires a borrow-free, sendable environment; mutable owned state is allowed. Scoped waiting helpers may use valid borrows under the appropriate invocation, transfer, sharing, and exclusivity contracts. Disjoint exclusive captures may run concurrently; conflicting captures may not. Results can retain valid caller-owned dependencies, but never references into destroyed child-owned storage. A returned dependency retains its ordinary restrictions after the helper returns.

`all`, `allSettled`, `race`, and `any` finish all child execution and cleanup before returning. For a winner or `all`'s early failure, select the result, cancel remaining children, join cleanup, then return. Result selection can be prompt while return waits indefinitely. Cancellation of the helper also cancels/joins children under deferred further cancellation, preserving all borrowed sources and any selected result until cleanup determines their ownership.

`stop(&routine)` requests cancellation and joins termination plus cleanup. Once the request is made, the join is cancellation-deferred. Already-finished targets are harmless; concurrent/repeated calls share termination state and cleanup runs once. Self-stop is a bug/panic, not a self-join. Freeing the handle still does not stop the goroutine. Wait cycles may deadlock. A timeout race does not promise a hard return deadline while losing work/cleanup remains.

## Cancellation, replacement, and operation ownership

Distinguish a cancellation request from delivery. Defer delivery during disposal and required cleanup joins while still allowing suspension, scheduling, and normal operation completion. Requests remain pending and do not unregister a masked cleanup wait or abandon its obligations. Dependent values and scoped children finish before guard disposal releases protected access. Cleanup can wait forever; there is no force-kill bypass.

For replacement:

1. Evaluate destination and RHS once with the old value alive. This phase is normally cancellable and cleans up the values actually owned.
2. Defer delivery from the start of old-value disposal through replacement installation. Keep the replacement owned throughout.
3. Finish disposal and installation, leaving a valid destination.
4. Deliver any pending cancellation afterward; normal cleanup then disposes the installed value exactly once.

Track initialized/disposed states rather than treating the old slot as still live. Dependencies needed by either value remain valid through its possible disposal. Masking never permits references into freed old storage.

Cancellation of an ordinary wait uses synchronized completion/cancellation bookkeeping only when delivery is enabled. Withdraw an uncommitted mutex/channel/timer/poller registration safely. If mutex acquisition wins the race, its release responsibility must remain owned through guard initialization or cleanup. Pending channel messages and committed receive results always have exactly one owner; a committed transfer is not rolled back. `select` has one winning commit across its registrations and withdraws the others. A separate `race` of receives may still consume losing messages; cancellation after a receive commits may dispose its result rather than put it back.

Native blocking work must cease accessing every borrowed source before cleanup invalidates it: buffers, resource state, callback data, and handles. Wait for completion or confirmed abortion; an abort request or moving work to a blocking pool does not end dependencies. Independently retained ownership must actually keep the required resource valid. Cancellation does not undo external effects already performed, and blocking work can delay stopping indefinitely.

Normal top-level completion retains ri-j33's program-exit behavior, without an implicit global join or guarantee of disposal in every background goroutine. Explicit stopping/waiting is needed when that completion matters. Panic remains ADR 0005's immediate whole-program termination without cleanup. Compiler-generated cooperative cancellation cleanup is a separate mechanism, not catchable panic or panic unwinding.

## Alternatives and validation

- **Channels only:** simpler public surface, but disallows useful synchronized shared state. Not adopted.
- **Callback-only `withLock`:** initially considered, then replaced by guards to reuse ownership/disposal and ordinary borrow dependencies instead of special callback escape/result rules.
- **Public raw unlock, escapable untracked views, or automatic shared mutation:** rejected; a guard owns the release responsibility and borrows control its views.
- **Immediate return after selecting a helper winner, abandoned native jobs, or force-kill cleanup:** rejected because borrowed sources and resource invariants must remain valid.
- **Public atomics, OS-thread pinning, or general deadlock prevention:** out of v0 scope.

`prototypes/09-concurrency-safety.rill` illustrates guards and scoped/background work; `prototypes/09-concurrency-safety.md` records expected accept/reject cases and runtime-state obligations. These are hand-checked design inputs, not executed tests. ADR 0010 records ri-owe's module visibility/scoping policy; ri-8n7 owns library module placement, factory spelling, complete generic/callable signatures, protected-operation interfaces, and evidence through interfaces; ri-48g owns explicit checking/cleanup/commit rules and adversarial validation. Those obligations remain before implementation readiness and do not constitute a safety proof or automatic RFC acceptance.
