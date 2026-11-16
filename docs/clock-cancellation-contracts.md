# Rill v0 clocks, deadlines, waits and cancellation tokens

Human-confirmed [#33](https://github.com/Reilley64/rill/issues/33), [ADR 0022](adr/0022-jiff-time-and-explicit-cancellation.md), and [temporal value contract](time-contracts.md). This is a public design contract, **not implementation, executed Rill tests, cancellation/safety proof, target/provider validation or accepted RFC**. [Prototype 24](../prototypes/24-clocks-cancellation.rill) and its [inventory](prototypes/24-clocks-cancellation.md) are hand-checked. #28/#30 retain checking/backend gates.

**Confirmed networking integration:** [#34](networking-contracts.md) / [ADR 0023](adr/0023-tcp-addresses-resolution-and-controlled-networking.md) explicitly adds the identity-only `checkInstantDomain` name below. The concurrently approved [SQL consumer amendment](https://github.com/Reilley64/rill/issues/33#issuecomment-6892624943) names the same operation `validateInstantDomain`; preserve both as canonical aliases, not competing implementations. Its controlled TCP adapter and concrete token/deadline errors do not change `withDeadline` nesting, introduce `withCancellation`, or turn intrinsic cancellation into a recoverable result. #34 Q93 uses ordinary fields and binding-based mutability, not a new field modifier.

## 1. Existing boundaries and interview revisions

[ADR 0009](adr/0009-concurrency-safety-and-cooperative-cancellation.md), #22/#23/#29 still govern ownership, masking, commits, stopped routines and errors. Functions can suspend without async/await syntax. A cancellation request, delivered goroutine cancellation, timer eligibility, selected outcome and completed cleanup are **different events**.

- **Q53 is superseded:** public tokens are included, informed by [C#/.NET and Tokio-util research](research/0033-cancellation-token-models.md).
- **Q64 was explicitly dropped:** there is **no generic withCancellation** in v0 and no renamed combined token/scope wrapper. Existing withDeadline remains.
- **Q70 revises Q48:** new typed time waits always take an explicit token and use WaitError, not parallel token/no-token overload arities. Original millisecond compatibility APIs retain their shapes.
- **Q92:** explicit Inputs in systemTimeZone, detailed in the temporal contract, avoids violating #36's startup/test environment boundary.

No public callbacks, ExecutionContext/token inheritance, cancellation-mask controls, force kill, Task/future trait, public dyn/unsafe escape, implicit casts or source-owned CancelAfter/reschedule/reset API is added. Internal wakeups still require proven retention/publication/cleanup; privacy or a standard-library label is not a proof.

## 2. Public homes, values and errors

std::time exports Clock, ManualClock, Instant, ClockError, WaitError, DeadlineError and the time functions below. std::cancel exports CancellationSource, CancellationToken, CancelOnDispose and its functions. Imports are explicit. Existing std::stop/std::routine helpers and std::channel types retain canonical identity; locally unrelated same-spelled functions do not satisfy evidence. Family bodies use #41's **implement** syntax. Listed signatures are fixed-arity operation contracts, not a complete library source file or repeated conflicting declarations.

Clock and ManualClock are owned synchronized handles: noBorrow/sendable/shareable, not implicitly copyable. Explicit copy(&handle) retains the **same** service/domain/controller state. A ManualClock is control authority; a Clock obtained from it allows observation/waits, not time advancement. Releasing a handle does not cancel other users or invalidate timers. Registrations retain every required clock/token resource beyond their factory call; no borrowing into a destroyed caller's handle slot. Last manual-controller disposal does not advance time, fake success or force-close waits: ordinary cancellation can still stop them.

Instant is a public private-field nominal scalar value containing logical reading plus nonreused clock-domain identity, with implicit-copy/noBorrow/sendable/shareable metadata. Its logical coordinate is signed i128 **nanoseconds**, not a public wire/Unix integer or fixed memory layout. Ordering/difference/deadline use require the same domain. Canonical eq/hash compare domain and reading; cross-domain equality is false, not an ordering. Identities must not be reused while old scalar instants can be used; ID exhaustion cannot wrap. Independent runtimes/executions have no compatible serialized instant protocol.

```rill
export enum ClockError {
  Unavailable, Unsupported, DomainMismatch, NegativeDuration, Range,
  ReadFailure, InvalidReading, Regressed, TimerFailure,
}
export enum WaitError { Cancelled, ClockFailure }
export enum DeadlineError { Expired, ClockFailure }
```

These are small owned display/noBorrow/sendable/shareable payloads. ClockFailure retains the original Err<ClockError> through **explicit caused-by wrapping**, not fabricated payload copies, parsed messages or an automatic error conversion. Token-request cancellation is WaitError.Cancelled; it is not ClockError, IoError, fatal panic or intrinsic goroutine cancellation. Other foundation modules own their concrete token-aware error types; no universal generic cancellation-error wrapper or mandatory CancellationError type survives the dropped helper. Construction origins remain code locations, not time readings or parse offsets.

## 3. Clock semantics and operations

```text
systemClock(): Result<Clock, ClockError>
defaultClock(): Result<Clock, ClockError> // canonical alias of systemClock
manualClock(): ManualClock
clock(&ManualClock): Clock
now(&Clock): Result<Instant, ClockError>
validateInstantDomain(&Clock, Instant): Result<void, ClockError>
checkInstantDomain(&Clock, Instant): Result<void, ClockError> // canonical alias of validateInstantDomain
reportedResolution(&Clock): Option<SignedDuration>
advance(&ManualClock, SignedDuration): Result<void, ClockError>
shiftInstant(Instant, SignedDuration): Result<Instant, ClockError>
compareInstants(Instant, Instant): Result<Order, ClockError>
instantUntil(Instant, Instant): Result<SignedDuration, ClockError>
instantSince(Instant, Instant): Result<SignedDuration, ClockError>
deadlineAfter(&Clock, SignedDuration): Result<Instant, ClockError>
```

`validateInstantDomain` and its canonical alias `checkInstantDomain` compare only the clock's retained domain identity with the instant's identity, returning `DomainMismatch` for a mismatch. It does not read time, register a timer, expose a raw identity, test timeline/provider health or revive a failed service; a match alone can succeed even when a later `now` fails. The explicitly approved #34 and #40 consumer additions share one implementation/family identity under #17's alias rules; preserving both approved names adds no second policy or dispatch route. They support basic-domain validation before an already-requested-token outcome without private-field access or an unintended clock read.

One default system-clock service/domain per Rill runtime; repeated factory calls and copies share it, not fresh incompatible origins. A factory validates required capabilities; typed failure never means silently supplying a weaker clock. It does not reset a terminally failed default identity. This justified trusted service is not a named function capturing a public runtime let or an ambient application/request clock. Manual factories create fresh independent domains, initially coordinate zero.

The default elapsed clock **counts OS suspend**, but does not wake the machine. Ordinary descheduling counts too; this is not CPU time. It is monotonically nondecreasing, repeated readings allowed, with rate corrections permitted. Wall-clock steps do not reset/recalculate elapsed deadlines. No strict raw/steady rate, nanosecond physical accuracy, minimum scheduling fairness or maximum return latency is promised.

Logical arithmetic is exact/checked: shift can create a past (including negative-coordinate) marker, but cannot exceed i128. Differences require matching domains and the approved SignedDuration range, and may be negative. No saturation of reversed operands to zero. A full SignedDuration interval is not promised schedulable by every host; representability/read/registration failures are typed, not wrapping or a deliberate never-ready substitute. All u64 milliseconds can be widened/scaled safely without silently narrowing, but a particular scheduling request can still fail its clock/provider limits.

Convert exact host counters with checked widened integer/rational calculations; a sub-nanosecond remainder rounds **down to the earlier reading**. Invalid/nonfinite/unrepresentable data errors, not clamping. Browser double milliseconds do not become exact physical nanoseconds by conversion. Host timer intervals are separately rounded **up** when necessary, with rechecks; a host wake alone is not successful expiry.

reportedResolution returns known provider-reported granularity, or None if unavailable. Not an invented value from representation. Capture/report information without claiming accuracy/current precision/maximum delay. Manual clocks report one logical nanosecond. Host reports can themselves overstate physical precision; this contract does not make them measurement evidence.

advance is a checked atomic forward movement with respect to other controllers/readers. Zero valid; negatives and coordinate overflow error without changing time. No automatic idle advancement, wall progression, backward reset or public user clock trait/provider injection. It drives **both reads and registrations**, not fake reads with real timers. Due waiters become eligible before successful advance completes; their user code/cleanup need not have run. Tests observe/join work explicitly.

A detected backward reading or loss of the service's representable timeline makes it terminally failed: no decreasing Instant, silent new origin or identity reuse. Wake pending waits with clock failure after safe release; future handles/read/factory interactions cannot pretend that domain recovered. Ordinary isolated read/registration failures need not poison the timeline unless they establish that condition. Caller errors on a manual advance do not poison the manual clock.

[Primary clock research](research/0033-clock-and-zone-boundaries.md) shows why host capability checks matter: platform clock names do not imply the same suspend/rate behavior; browser/WASI contracts and actual versions differ. No Linux/macOS/browser/WASI/LLVM/Wasm support is proved here. Missing imports may prevent instantiation; fatal host traps/backing allocation are not a promised recoverable ClockError. Target/provider matrices belong to #30/implementation evidence, with honest unsupported status rather than wall-clock fallback.

## 4. Absolute deadlines and wall time

An absolute deadline is an Instant; use Option<Instant> for absence. No magic MAX/infinite duration or extra nominal Deadline type. deadlineAfter rejects negative intervals, reads once and checked-adds once; pass that **same** deadline through nested calls rather than restarting each layer's timeout.

No implicit wall Timestamp → Instant conversion, calendar duration → timeout, wall-clock alarm, automatic retargeting after wall steps or built-in wall-to-monotonic converter. Application sampling of separate wall/elapsed clocks is explicit and not atomic. Calendar recurrence/scheduling remains application-level. timestampNow():Result<Timestamp,TimeError> and zonedNow(&TimeZone):Result<Zoned,TimeError> read wall time without local-zone guessing or a separate public wall-clock handle. A manual elapsed clock does not control them; pure calendar functions use supplied values.

## 5. Typed waits and notification ownership

```text
sleepFor(&Clock, SignedDuration, &CancellationToken): Result<void, WaitError>
sleepUntil(&Clock, Instant, &CancellationToken): Result<void, WaitError>
afterFor(&Clock, SignedDuration, &CancellationToken): Receiver<Result<void, WaitError>>
afterUntil(&Clock, Instant, &CancellationToken): Receiver<Result<void, WaitError>>
```

All are std::time exports. No arity-overloaded two-argument forms or generic token scope. Use a named neverCanceledToken value for no external request. Relative waits calculate an absolute target once; early wakes/retries use that target, not a fresh timeout. A successful wait/notification checks the chosen clock reached it. Small positive intervals cannot become zero success by narrowing/down-rounding. Scheduling/throttling/native work/cleanup may delay it indefinitely.

Startup order:

1. Check basic arguments (negative relative interval; absolute domain mismatch).
2. If the explicit token is already requested, select Cancelled, including zero/expired cases, without starting the timer.
3. Otherwise read/calculate/register against the clock with checked arithmetic/capability checks.

Other races/read failures during setup use synchronized one-outcome bookkeeping; no broad priority is promised after the explicit preflight order. During a pending wait, expiry and token request race; simultaneously ready outcomes have no specified winner. Later request/failure does not retroactively replace selected success. Confirm unregister/abort/release before returning a failure; a native cancellation **request** is not release confirmation. No rollback of committed effects.

afterFor/afterUntil return a fresh ordinary receiver: one Ok(void) on expiry **or** one Err<WaitError>, then close. Validation/registration errors are already-eligible error messages, not a second nested factory Result. This does not promise the factory cannot suspend while safely establishing/disposing registrations. Retain owned clock/token state; the receiver remains borrow-free as the existing channel contract requires. There is no writable callback/captured user environment.

Last receiving-owner disposal withdraws an uncommitted timer/token subscription and waits for required release if necessary. Do not affect another independently created timer. Already-committed channel values follow ordinary disposal/ownership; canceling a consumer is not rollback/requeue. Masked cleanup still permits normal completion and required unregister joins.

### Compatibility calls

Existing std::time::sleep(milliseconds:u64):void and std::channel::after(milliseconds:u64):Receiver<void> preserve their locations, millisecond meaning, monotonic timing, zero immediately eligible/not forced yield, and after's one notification then close. They use the default suspend-inclusive clock with no external token. They cannot report ClockError without changing signatures: unsupported request/range/clock/timer failure is **fatal panic**, including an otherwise unreportable later timer failure, not false success, silent EOF or deliberate endless waiting. This clarifies previously unstated error boundaries rather than claiming every host supports those operations.

## 6. Scoped withDeadline, not withCancellation

```text
withDeadline<F,R>(&Clock, Instant, F): Result<R, DeadlineError>
  where callableOnce(F,[],R), sendable(F), sendable(R)
```

It owns F and runs it as scoped work. Valid caller-owned borrows are permitted; no blanket noBorrow(F/R). Concrete invocation/capture/result evidence is checked before monomorphization; an erased arrow cannot manufacture it. Results may retain valid caller dependencies but not child/owned-callable-local storage.

Validate clock identity/read first; invalid clock/read failure is ClockFailure with cause. Already-reached deadline selects Expired without invoking F. Owned captures still dispose normally, possibly with effects/indefinite cleanup. No atomic physical-time promise spans precheck and actual instruction execution.

After admission, operation completion, timer failure and deadline eligibility select one outcome. Completion and expiry both ready → unspecified winner (no strict completion timestamp ordering or guaranteed expiry bias). Do not reclassify after joining/return-time clock checks. If R is Result<T,E>, successful operation selection returns **Ok(Result<T,E>)**, including its original typed Err unchanged; no generic flattening or conversion. Clock failures use explicit caused-by DeadlineError.ClockFailure.

Select, request cancellation of losing scoped work, join **all execution/cleanup and native release**, then return. Selected R stays owned/valid through this phase. Helper cancellation also cancels/joins children with further delivery deferred. An external token affects only operations explicitly given it inside F; it does not become a hidden helper parameter/current token. If token-aware F returns its own cancellation result and completion wins, that remains nested under Ok. withDeadline's intrinsic child cancellation is independent of whether any token flag was requested.

No generic withCancellation, runUntilCancelled, combined renamed token/scope helper, public cancellation exception or force-kill path. stop(&Routine) remains the existing request-and-join operation; freeing Routine still does not stop/join. A later concrete need for more scoped helpers is a separate design, not implied here.

## 7. Sources, tokens and observation

std::cancel signatures:

```text
CancellationSource(): CancellationSource
token(&CancellationSource): CancellationToken
neverCanceledToken(): CancellationToken
cancel(&CancellationSource): void
isCancellationRequested(&CancellationToken): bool
waitForCancellation(&CancellationToken): void
cancellationSignal(&CancellationToken): Receiver<void>
childSource(&CancellationToken): CancellationSource
linkedSource(&List<CancellationToken>): CancellationSource
cancelOnDispose(CancellationSource): CancelOnDispose
disarm(CancelOnDispose): CancellationSource
```

All three exported types are nominal private-field values. Source and token are noBorrow/sendable/shareable **owned handles**, not references into a source's stack slot. No implicit copy. Explicit copy(&source) shares cancel authority/state; copy(&token) shares observation only. Token does not expose cancel authority or a source accessor. Child factories mint control only over fresh child state, never over parents. There is no ambiguous default token construction: fresh Source and named neverCanceledToken have different meanings.

State is permanently **one-shot**. Repeated requests are harmless, never uncancel/reset/reuse, and do not rerun user effects. Token contains a request state, not arbitrary exceptions/reasons/app context/deadline status. Never-canceled is a special stable identity; all such tokens equal. Other canonical token eq/hash use state identity, not current flag; child/independent states differ even if both canceled. Hash is process-local, not a serialized control identity. Source needs no separate eq/hash.

Ordinary disposal only releases the handle: it does not cancel, join work, invalidate surviving tokens, break needed live links or revoke another source copy. Root state may remain uncanceled after the last authority is released; a linked state can still be requested by its parents. Parent/link state must be retained for surviving tokens without dangling borrowed source/callback pointers. Release/unlink/in-flight propagation is protected exactly-once ownership work, not managed-GC reachability.

waitForCancellation observes persistent state; already requested is immediately eligible, no forced yield. It returns normally for the signal, not a CancellationError. A never token can wait forever but intrinsic goroutine cancellation still works. Abandoning and retrying observation does not consume/reset the request.

cancellationSignal creates one independent subscription/receiver, sends one void notification when requested then closes. Already requested → already-eligible notification. Multiple independent subscriptions all observe; copying/Sharing **one receiver** retains ordinary one-message channel consumption, not broadcast. Last receiver disposal unlinks that subscription safely. No unbounded retained dead waiters or lost wakeup window; registration and persistent-state recheck synchronize with cancel. No public callback order/context/reentrancy/registration type is introduced.

## 8. Child and OR links, propagation and guards

childSource creates a fresh state directed from one parent; linkedSource does the same for an OR list. Empty list = independent source; never parents contribute no request. Duplicate parents do not duplicate a state transition. Already-requested parent produces an already-requested child before construction returns. Creation racing cancellation either observes the request or participates in propagation, never misses it.

Parent request reaches descendants; child request never reaches parent/sibling branches. Children can outlive public parent/source values by retained state. There is no mutable attach/reparent operation, so callers cannot construct cycles by linking existing nodes backward. Shared descendants/fan-in are allowed. Internal links must avoid permanent ownership cycles and release dormant registrations when no longer needed; do not infer that copying Tokio's GC/Arc topology is a proof for Rill.

cancel publishes the local request and completes required descendant signal propagation **before return**, including repeated/concurrent requests overlapping an in-progress propagation. It may suspend/schedule for synchronization; it does not wait for user operations. During traversal observers may see parent requested before children; no atomic tree-wide snapshot. Late child construction from a requested parent returns requested. Affected waiters are eligible, not necessarily executed. Cancellation delivery cannot abandon a half-published required propagation/release step. Deep trees/OR graphs and their default disposal use bounded native call stacks/worklists, not recursive overflow or a force-kill escape.

CancelOnDispose uniquely owns a source and an armed required action. It is noBorrow/sendable/shareable under that ownership, not copyable and not goroutine-affine like a LockGuard. Explicit/automatic dispose requests cancellation if armed, then releases owned fields. disarm consumes it, disables the action and returns the source. No source borrow or dangling observer is fabricated. Protected actions/remaining-field cleanup prevent wrappers or orphan-specialized bodies from suppressing its required action. No public raw armed-bit mutation. This guard **does not join operations** and gives no permission to free other users' data; source disposal without this explicit guard still does not cancel.

## 9. Token requests versus intrinsic goroutine cancellation

Passing/capturing/observing a token is explicit. No ambient token, hidden application context, automatic token inheritance or automatic association with a goroutine. Named functions receive runtime values as parameters; arrows obey existing capture rules. Parent-child links do not create goroutines or imply ownership of their completion.

A token request affects only explicit observers/token-aware operations. A token-aware operation can cancel its own pending work and return its **ordinary typed outcome** after required release; it does not terminate its caller. Other modules (#32/#34/#35/#38/#39/#40) specify their own concrete token-aware errors and actual abortability; do not stuff intrinsic goroutine cancellation into IoError or silently change #23 signatures.

Runtime masking defers **goroutine cancellation delivery**, not ordinary token observations. Polling/signal waits remain truthful. Explicitly token-aware operations may complete with typed cancellation during cleanup, but only after unregister/confirmed-abort/native-source release; this is ordinary control flow, not an unwind. Cleanup that must ignore external requests uses neverCanceledToken. Compiler/runtime-required cleanup must still finish/dispose remaining initialized fields; it cannot abandon obligations based on a flag. No token can bypass a masked required native cleanup join. Ordinary intrinsic cancellation remains pending where required, is never converted to WaitError, and is not catchable fatal panic.

No public source-owned CancelAfter timer, rescheduling/uncancel API or callback facility. Applications can own an explicit timer/controller goroutine and call cancel, handling timer errors and joining it under existing rules; do not silently allocate unowned background tasks. A cancellation reason is not inferred from deadline/token races. Committed reads/writes/sends are not rolled back by any signal.

## 10. Evidence and cross-ticket obligations

Use the pinned [clock/zone](research/0033-clock-and-zone-boundaries.md) and [token](research/0033-cancellation-token-models.md) notes as facts, not an imported runtime. .NET callback disposal/repeated CancelAsync and Tokio future dropping are **not** Rill join guarantees. No runtime/driver/server or cancellation race was executed here.

#28 formalizes identity exhaustion, exact arithmetic/counter conversion, pre-mono callable/result dependencies, masking versus typed outcomes, publication/commit/unregister, concurrent propagation completion/helping, DAG ownership, default service lifetime and bounded partial/deep/canceled cleanup. #30 validates actual providers, hosts, Wasm pointer stability/imports/ABI/tail mechanisms. Return any unworkable required operation to map #1, not an asserted safety proof. Allocation/backing failure remains fatal; native unabortable work and cleanup can prevent return indefinitely.

#32/#34/#35/#38/#39/#40 consume the explicit clock/deadline/token contracts without assuming force kill, hard latency, replay, rollback or connection reuse. #36's safe Inputs/test isolation is preserved for zone discovery. #41 supplies implement/conversion evidence, not a token trait or lifetime erasure. #24 owns actual diagnostics. Closure of #33 records the confirmed design only.
