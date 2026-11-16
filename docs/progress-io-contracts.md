# Resumable single-owner I/O progress

This document records the explicitly confirmed #45 design. It extends the channel, streaming-I/O, TCP and TLS contracts without changing ordinary `read`, `write`, `flush` or consuming `close`. It is a public design contract, not an implemented compiler, runtime, TCP driver or TLS provider.

## 1. Scope and module boundary

`std::io::progress` owns the reusable progress families and shared outcome types. `std::channel` owns nonwaiting channel attempts and coalescing wake/watch values. `std::net` and `std::tls` own their concrete progress drivers and constructors.

A progress driver is one exclusive application-I/O owner that retains admitted operations across bounded local advances and external waits. It is not a Future/Poll trait, public dynamic object, associated-type interface, raw readiness descriptor or automatic adapter from canonical blocking I/O. Generic code names every canonical family and output type it needs before monomorphization.

Initial standard drivers are whole native TCP and TLS over an eligible lower progress driver. No standard file, stdio, UDP or TCP-half driver is supplied. External frameworks own their connection loops, bounded queues and protocol policies; the [#39 HTTP contract](http-contracts.md) consumes this foundation without moving HTTP into `std::io`.

## 2. Atomic channel attempts

`std::channel` adds:

```rill
export enum SendAttempt<T> {
  Sent,
  Full(T),
  Closed(T),
}

export enum ReceiveAttempt<T> {
  Item(T),
  Empty,
  Closed,
}

export function trySend<T>(to: &Sender<T>, value: T): SendAttempt<T>
  where noBorrow(T), sendable(T);

export function tryReceive<S,T>(from: S): ReceiveAttempt<T>
  where noBorrow(T), sendable(T);
```

`tryReceive` has the same supplied source patterns as `receive`: `&Receiver<T>` and the explicit view of `&Shared<Receiver<T>>`. One attempt commits atomically or returns immediately. `Full`/`Empty` are normal coordination states, not errors; `Closed` distinguishes receiver loss from temporary send pressure and closed-and-drained input from temporary emptiness. Returned unsent `T` is the original owned value. Default formatting does not require or reveal `T`.

Capacity-zero channels participate. `trySend` commits only with an already-waiting receiver; `tryReceive` commits only with an already-waiting sender. A waiting `select` receive is one such receiver, but existing one-winner/losers-consume-nothing semantics remain. A watch is never a waiting endpoint or reservation. Cancellation cannot roll back a committed transfer.

## 3. Coalescing wake channels

`std::channel` additionally exports opaque owned `WakeNotifier`, opaque `ChannelWatch<T>`, and:

```rill
export function wakeChannel(): [WakeNotifier, Receiver<void>];
export function wake(notifier: &WakeNotifier): bool;

export function watchSendReady<T>(
  source: &Sender<T>,
  notifier: WakeNotifier,
): ChannelWatch<T>
  where noBorrow(T), sendable(T);

export function watchReceiveReady<S,T>(
  source: S,
  notifier: WakeNotifier,
): ChannelWatch<T>
  where noBorrow(T), sendable(T);

export function arm<T>(watch: &mut ChannelWatch<T>): void;
export function disarm<T>(watch: &mut ChannelWatch<T>): void;
```

A wake state holds at most one pending void token. `wake` never waits. It returns true while receiving ownership exists, including when a token is already pending and the new hint coalesces; it returns false after receiving ownership is gone. Explicit `copy(&notifier)` shares one state and notification authority. `WakeNotifier` is non-implicitly-copyable, noBorrow, sendable and shareable. Last-notifier disposal closes the ordinary receiver after a pending token drains. There is no notifier-to-receiver accessor or reset.

Notification racing token consumption leaves the condition represented by the consumed hint or another pending hint. Multiplicity is deliberately lost: callers retain authoritative work/results elsewhere and recheck them. This mechanism is unsuitable for an edge that has no retained state and requires counted delivery.

Watch construction consumes one notifier and starts disarmed. `arm` synchronizes registration with a current eligibility check. An eligible source posts/coalesces one hint and leaves the watch disarmed; otherwise it stays armed until eligibility, target loss or withdrawal. `disarm` is idempotent and returns only after observer withdrawal/quiescence. Disposal performs the same required withdrawal. No user callback runs.

Send readiness means a send could commit or discover receiver closure. Receive readiness means a receive could commit or discover closed-and-drained state. Another routine can win first, so `Full`/`Empty` after a wake is valid. Watches reserve nothing and promise no priority, ordering, fairness or starvation prevention.

A watch retains synchronized channel state, not application endpoint counts. Loss of the watched-side endpoint ownership retires it and cannot manufacture an endpoint. Opposite-side closure wakes a still-existing operation endpoint. Target receiver loss silently retires the watch. `ChannelWatch<T>` owns no T; it is noBorrow and sendable, but noncopyable/nonshareable and exclusively armed/disarmed. Moving an armed guard preserves registration.

A coordinator retains one root notifier and passes explicit copies to watches/driver registrations. It disarms/releases registrations before dropping the root, then drains/removes the ordinary receiver. A closed wake receiver is not transport EOF or operation completion and must not remain as a permanently ready select case.

Managed channel allocation/bookkeeping failure remains fatal. These APIs do not add a recoverable allocation model or public unregister token.

## 4. Admission and public outcomes

`std::io::progress` exports:

```rill
export enum ProgressSchedule {
  Runnable,
  Waiting,
  Idle,
}

export enum AdmissionReason {
  Busy,
  NotStarted,
  Closing,
  Stopping,
  Terminal,
  InvalidInput,
  LimitExceeded,
}

export enum Admission {
  Admitted,
  Rejected(AdmissionReason),
}

export type WriteRejection = {
  reason: AdmissionReason;
  input: Bytes;
};

export enum WriteAdmission {
  Admitted,
  Rejected(WriteRejection),
}

export type ReadFailure<E> = {
  data: Bytes;
  error: Err<E>;
};

export enum Interruption {
  ReadFailed,
  WriteFailed,
  FlushFailed,
  CloseFailed,
  DriverFailed,
  StopRequested,
}

export enum ReadOutcome<E> {
  Data(Bytes),
  End,
  Failed(ReadFailure<E>),
  Interrupted(Interruption),
}

export enum WriteOutcome<E> {
  Completed,
  Failed(IoFailure<E>),
  Interrupted(Interruption),
}

export enum OperationOutcome<E> {
  Completed,
  Failed(Err<E>),
  Interrupted(Interruption),
}

export enum DriverFailurePhase {
  Startup,
  TransportProgress,
  WaitRegistration,
  Stop,
  Release,
}

export type DriverFailure<E> = {
  phase: DriverFailurePhase;
  error: Err<E>;
};

export type DriverFailureSet<E> = {
  primary: DriverFailure<E>;
  cleanup: Option<DriverFailure<E>>;
};

export type ProgressStep<RE,WE,FE,CE,DE> = {
  schedule: ProgressSchedule;
  started: bool;
  stopFinished: bool;
  read: Option<ReadOutcome<RE>>;
  write: Option<WriteOutcome<WE>>;
  flush: Option<OperationOutcome<FE>>;
  close: Option<OperationOutcome<CE>>;
  driverFailures: Option<DriverFailureSet<DE>>;
};
```

Full/empty/busy/pending are not fabricated error payloads. Default diagnostics for admission/completion aggregates reveal safe phases/classifications/counts only, not byte contents, native handles, request data or recursively formatted arbitrary errors. Explicit field/variant access exposes owned bytes and original errors. Rendering a contained E needs explicit `display(E)` evidence; construction/transport does not.

## 5. Canonical progress families

```rill
export function admitRead<D>(driver: &mut D, maximum: u64): Admission;
export function admitWrite<D>(driver: &mut D, input: Bytes): WriteAdmission;
export function admitFlush<D>(driver: &mut D): Admission;
export function admitClose<D>(driver: &mut D): Admission;
export function requestStop<D>(driver: &mut D): void;

export function advance<D,RE,WE,FE,CE,DE>(
  driver: &mut D,
): ProgressStep<RE,WE,FE,CE,DE>;

export function registerProgressWait<D,G,DE>(
  driver: &D,
  notifier: WakeNotifier,
): Result<G,DriverFailureSet<DE>>;

export function withdrawProgressWait<G,DE>(
  guard: G,
): Result<void,DriverFailureSet<DE>>;
```

One body for a concrete D selects its independently typed RE/WE/FE/CE/DE outputs; return types cannot install competing bodies. A generic coordinator explicitly requires all eight admission/stop/advance/register/withdraw families shown above, including the same concrete G and DE relationship. It separately names only display/noBorrow/sendable/shareable evidence required by its actual storage, channels and diagnostics. There is no hidden ProgressDriver trait, associated projection, method surface or runtime capability lookup.

Admission validates/retains state only: no application I/O, readiness wait, executor-capacity wait or native-job completion. Basic/state rejection is effect-free and non-poisoning. `Busy` includes an active same lane, a flush barrier blocking a later write, or a retained untransferred completion. Positive read maximum is required; zero is `InvalidInput`. A larger maximum is an upper bound, not eager allocation. Concrete write limits use `LimitExceeded` and return the unchanged input. After `Admitted`, the driver owns the input and failure does not recover it.

There is at most one logical read and one logical write. A flush barrier can queue behind the current write, blocks later writes and has one slot. It completes after preceding output reaches the concrete lower flush boundary; it does not cover future output. Graceful close admits only with quiescent read/write/flush lanes; `Busy` leaves the driver unchanged. Empty write is admitted and completes through `advance`.

`requestStop` forbids new application admission immediately and marks retained operations for truthful terminal disposition. It performs no external wait/native I/O and is harmless while already stopping/terminal. The caller keeps advancing/waiting until `stopFinished`; intrinsic routine stopping is separate.

## 6. Advance, results and lifecycle

One advance performs bounded local work and returns rather than waiting for socket readiness, application/channel capacity, executor capacity or native-job completion. A native operation that can block is retained through bounded job machinery where required. This is a semantic obligation, not a hard wall-time/CPU guarantee for an individual native call.

Scheduling states mean:

- `Runnable`: bounded local work remains; check other bounded sources, then advance again.
- `Waiting`: admitted work exists but needs an external driver wake.
- `Idle`: no admitted/startup/close/stop work remains; wait only for application/control admission or owner shutdown.

Scheduling state says nothing by itself about health, EOF, success or reuse. A standard coordinator checks application/control sources between runnable advances; no source-order/fairness/starvation guarantee follows.

A step transfers at most one new outcome per field, but several fields can complete together. `started` and `stopFinished` are one-time events, not persistent flags. TCP begins started and emits no synthetic started event. TLS emits started only after complete publication. `stopFinished` means all lanes, native work, observers, jobs and required release are quiescent, even if failures were returned; it never means success.

Each lane has one retained completion slot. Native completion between advances stays there, wakes the coordinator and blocks conflicting admission until transferred. Advance transfers every currently available field once. The driver then retains only bounded terminal classifications needed to explain later interruptions, never a duplicate arbitrary error.

Read returns a positive owned prefix, real End, or a valid owned prefix paired with the original error. It does not perform another read merely to fill a maximum or prove EOF. Write targets the whole admitted buffer; the driver retains its internal cursor and retries only unaccepted staging suffixes. Completed write means full current-input acceptance at that layer, not flush/peer receipt. `IoFailure.transferred` is exact accepted current-input prefix. It does not report native TLS processing, ciphertext handoff or peer effects.

Interruption carries no copied error. The actual typed cause is transferred once by its owning lane/driver failure field; later classifications refer to terminal history. Driver failure preserves primary plus optional cleanup error, phases and original wrappers. A cleanup failure cannot replace the selected operation failure. Provider E may itself be a structured typed aggregate for one provider operation; this layer does not flatten it.

Post-admission standard TCP/TLS lane or driver failure begins terminal cleanup automatically. New admissions reject Stopping/Terminal; dependent lanes become Interrupted. A second stop request is harmless. Stop supersedes active graceful close as `Interrupted(StopRequested)` unless close had already won; ready ties are unspecified. Stop/disposal is not graceful close or delivery.

Read End closes the input direction semantically. It does not automatically terminate output where the concrete protocol permits it. Later reads can complete immediately with End without native I/O; applications stop admitting reads after observing EOF.

## 7. Wait registration and controls

A driver registration retains a scoped dependency on its driver, posts/coalesces at most one wake, then is inactive. Setup synchronizes with current state: work becoming runnable before/during setup yields a valid guard plus immediate wake, not a lost transition. One registration privately owns whatever read/write/job-capacity interests the current concrete state permits; callers cannot select a WANT direction or native descriptor.

G is noncopyable. No blanket noBorrow/sendable requirement applies; movement follows its concrete dependencies/evidence. A live guard prevents mutable advance/move. After a wake—or when another select case wins—the coordinator consumes `withdrawProgressWait`, confirms callback/registration quiescence, then mutates/advances. Withdrawal never cancels admitted I/O merely because another source woke.

Registration/withdrawal failures use the same DE as advance, return once as DriverFailureSet and move standard drivers toward terminal cleanup. The driver retains classification, not a clone of a returned cause. Guard disposal is fallback: it still quiesces and records recoverable withdrawal failure in the borrowed driver's retained slot for next advance. Cleanup is masked and can wait indefinitely.

Register only after Waiting. Runnable work is advanced after checking other sources; Idle has no driver registration. Standard loops normally select among the shared wake receiver, an optional timer, cancellation signal and at most one direct command/control receiver using existing fixed 2/3/4 receive-select. Body/channel interests feed the shared wake through watches. Closed cases are removed.

Low-level drivers do not attach mutable exchange deadlines/tokens to lanes. Coordinators own explicit absolute controls. Application activity and soft timers withdraw/re-enter scheduling without stop; a selected true token/deadline outcome withdraws first, then requests stop. Preserve selected control and any withdrawal failure independently. No timeout/cancellation is a progress yield or hard return bound.

Executor saturation is Waiting. A queued-not-started job may be withdrawn during stop; an executing unabortable job is joined. There is no public executor handle, queue position, worker count or priority. Peer behavior, lower/app backpressure, refusal to consume, starvation, native work and cleanup can delay completion indefinitely. Manual clocks control their registrations, not these other sources.

## 8. Standard TCP progress

`std::net` exports opaque `TcpProgress`, opaque `TcpProgressWait` and:

```rill
export function tcpProgress(stream: TcpStream): TcpProgress;
```

Construction consumes the whole stream and installs managed ownership state without a recoverable result. Rill allocation/bookkeeping failure remains fatal; native errors occur in later outcomes. There is no split-half implementation, raw descriptor, copy, extraction or conversion back. `TcpProgress` is noBorrow and sendable, noncopyable/nonshareable; moving preserves admitted jobs. A live wait dependency prevents conflicting movement.

Supplied bodies bind RE=WE=FE=CE=DE=`NetError` and G=`TcpProgressWait`. They use `NetOperation.Progress` only when failure is not naturally Read/Write/Flush/Close. `TcpProgressWait` exposes no interests/descriptors.

At most one private native read and one private native write job can run concurrently under the one public owner. Same-direction work is serialized. Jobs retain stable runtime-owned storage, operation/generation identity, immediate native error capture and mandatory joins/release. This is a private amendment to #34, not public overlapping canonical I/O or a split.

Standard write admission ceiling is 16384 bytes; larger input returns LimitExceeded unchanged. Read returns at most min(caller maximum,16384) bytes. Graceful drained FIN is End; reset/native failure is Failed with actual prefix. Raw flush is an ordered successful no-op after preceding write acceptance, never a health probe/packet push/ACK. Graceful close requires quiescence. Terminal stop may destructively wake native work but returns only after joined truthful prefixes/errors. No generic half-close is added.

Native Linux/macOS are the initial live scope. Names/types remain typecheck-visible on wasm32-wasip1 consistently with #34, but standard WASI has no live TcpStream construction path and no hidden preview2/import/blocking fallback. User-defined drivers can supply the generic families on other targets.

## 9. Standard TLS progress

`std::tls` exports opaque noncopyable/nonshareable `TlsProgress<D>`, opaque `TlsProgressWait<D,G>`, and:

```rill
export function clientProgress<D,G,RE,WE,FE,CE,DE>(
  lower: D,
  name: ServerName,
  config: ClientConfig,
): Result<TlsProgress<D>,TlsError>;

export function clientProgressWithInput<D,G,RE,WE,FE,CE,DE>(
  lower: D,
  name: ServerName,
  config: ClientConfig,
  initialCiphertext: Bytes,
): Result<TlsProgress<D>,TlsError>;

export function serverProgress<D,G,RE,WE,FE,CE,DE>(
  lower: D,
  config: ServerConfig,
): Result<TlsProgress<D>,TlsError>;

export function serverProgressWithInput<D,G,RE,WE,FE,CE,DE>(
  lower: D,
  config: ServerConfig,
  initialCiphertext: Bytes,
): Result<TlsProgress<D>,TlsError>;
```

Each declaration requires the complete lower family set from section5 with the same G/RE/WE/FE/CE/DE relationship. Every lower error additionally needs display/noBorrow/sendable/shareable because TLS retains original unique causes. There is no global noBorrow(D) or sendable(G); `TlsProgress<D>` gains properties only from actual D/dependencies. Outer wait G is `TlsProgressWait<D,LowerG>` and quiesces its lower registration before releasing the outer borrow.

Construction consumes all inputs and selects the progress/provider profile before handshake. Immediate invalid/over-limit prefix or provider-setup failure consumes and terminally stops/joins lower D before returning TlsError; preserve setup plus cleanup causes. Ordinary handshake/verification/negotiation/final-flight errors occur later as Startup driver failures. Application admissions reject NotStarted until a one-time started event.

Started requires #38 authentication, identity/status/policy and actual final-handshake output handoff, not provider handshakeDone alone. No NetControl forms: coordinators own controls. No conversion from TlsStream, extraction, raw lower recovery, independent TLS directions or ordinary-blocking lower adapter.

All TLS progress RE/WE/FE/CE/DE are TlsError. Lower causes remain original under #38. Errors use existing kinds/phases plus `TlsOperation.Progress` for wait/background/stop/release work not owned by Read/Write/Flush/Close/ClientHandshake/ServerHandshake. Pending is scheduling state, never WouldBlock/CancelledYield. No new error kind is added.

Standard write admission/owned read completion are capped at 16384 bytes. Public exact write progress remains current-input plaintext accepted into bounded TLS-owned staging. Native `SSL_write_ex` zero/WANT/failure does not reveal a cumulative plaintext prefix; ciphertext size/private counters never replace it. Peer effects stay unknown.

### 9.1 Progress provider profile

The private OpenSSL3.5.9 progress path sets before I/O:

- `SSL_MODE_ENABLE_PARTIAL_WRITE` on;
- `SSL_MODE_ACCEPT_MOVING_WRITE_BUFFER` off;
- `SSL_MODE_AUTO_RETRY` off;
- maximum application fragment/native attempt 16384 bytes;
- one record pipeline;
- no configured outbound record padding;
- 16384-byte ingress and egress BIO-pair rings.

A WANT retry preserves original native pointer/content/length. Positive partial success completes that native operation; a fresh native operation uses only the suffix. No mode supplies failure-prefix counts or bounds one native call's duration.

Each ring is charged inside the existing #38 65536-byte ciphertext-in/out ceiling, not added to it. Remaining capacity covers copied drains, lower-job payload/tails and real copy overlap; moving a byte does not uncharge it while another representation retains it. Existing 65536-byte plaintext-in/out ceilings remain. Initial prefix shares ingress. These caps do not bound every OpenSSL allocation. Concrete async partitions, leases/pointer validity and copy-overlap proof remain #44/#47 delivery gates; ordinary short/full BIO pressure is Waiting, not allocation failure.

If required authenticated/control/ciphertext state cannot be preserved/progressed within approved bounds/work counters, return typed TlsError LimitExceeded and stop. Never discard authenticated bytes, overwrite outcomes, grow hidden queues or call pressure EOF.

### 9.2 Retry/interleaving and renegotiation

OpenSSL retry rules are stateful. Write-WANT_WRITE permits BIO drain and same-write retry, not another SSL I/O call. Write-WANT_READ permits only source-justified serialized read/control progress while retaining the write identity. Pending read/handshake/control state uses a reviewed state policy; previous WANT direction alone is insufficient. BIO feed/drain is not a second SSL application operation. Classify each SSL result immediately on the same native thread with clean error queue before any intervening OpenSSL call/query; capture diagnostics before migration.

A fixed private passive observer detects TLS1.2 post-handshake `HelloRequest` at clients and renegotiating `ClientHello` at servers. It performs no user callback/reentrant OpenSSL call. After immediate current-call classification, detection selects terminal `TlsErrorKind.ProtocolFailure`, Control phase, owned by the active Read/Write/Flush when natural or driver-wide Progress otherwise. Dependent lanes are Interrupted. Perform no further application SSL I/O or SSL_shutdown; preserve real prefixes/handoffs and stop/release.

This final policy supersedes the provisional continue-after-rejection choice. The pinned reference showed why: warning callback and local fresh-write success can occur while warning ciphertext is not handed off; sender record state advances and the peer then fails record processing. #44 must validate both client HelloRequest and server renegotiating ClientHello. TLS1.3 KeyUpdate remains required valid control traffic and must not match this rule.

Fatal SSL/provider/transport results, admitted abandonment and bounded-state exhaustion poison as #38 already requires. Safely retained read state does not alone forbid a source-justified opposite-direction operation. No universal duplex liveness follows.

### 9.3 Closure and targets

TLS End means authenticated close-notify after prior plaintext. Raw lower EOF/reset/truncation is failure. TLS1.2 peer notify ends both application directions under #38; TLS1.3 can leave local output open until local closure. No generic progress half-close is added; existing TLS shutdownWrite remains outside this foundation.

Graceful progress close follows #38 local flush/notify/handoff/lower release without waiting for peer notify. Stop/disposal skips graceful protocol, joins retained work and can wait indefinitely. No recovery/replay/ordinary-stream extraction.

Names/signatures stay typecheck-visible on wasm32-wasip1. Without a live selected provider, otherwise-valid construction returns deterministic typed Unsupported after normal validation and consumes/releases lower D. There is no native capability probe, hidden fallback or claim that an arbitrary lower D supplies Wasm TLS.

## 10. Effects, disposal and diagnostics

Layer-relative milestones remain distinct: command admission, TLS-owned staging acceptance, native processing, ciphertext handoff, lower transport acceptance and peer effects. Success/failure/interruption establishes no remote receipt/execution/nonexecution, rollback or replay safety. Higher protocols can add explicit confirmation; this layer never retries/replays.

Temporary lower/ring/executor/app pressure is pending. Lasting peer refusal to consume can legitimately block. Deadline/token/drop does not establish native completion. A selected outcome is not rewritten by later cleanup; actual late prefixes are finalized before return where required.

Driver disposal before graceful close/stopFinished implicitly requests terminal stop and drives/joins release under masked cleanup. It is not graceful, can wait indefinitely and cannot return recoverable cleanup errors; failures are truthfully classified and released, not logged with arbitrary contents or called success. Explicit stop is preferred for observable outcomes. Fatal panic/allocation promises no unwind.

Protected endpoint counts, watcher withdrawal, native-job leases, result adoption and release cannot be replaced through ordinary family extension. User-defined drivers own their implementation correctness but cannot intercept standard owners' protected actions.

## 11. Evidence and delivery gates

Evidence labels follow [RFC conventions](rfcs/README.md#examples-and-evidence):

- The declarations/scenarios in prototype24 are **HandChecked**, not compiler/runtime execution.
- [`research/0045-progress-reference/`](research/0045-progress-reference/) is **ReferenceExecuted**: a pinned OpenSSL3.5.9 in-memory macOS-arm64 build currently reports13/13. It includes both-version/direction transfers, fragmented input, same-argument retry, partial success, bounded stall/resume, KeyUpdate cases, close-vs-truncation and unsafe TLS1.2 renegotiation continuation. It is not TCP, Rill or production provider execution.
- Source/configuration research is supporting evidence, not protocol execution.
- Channel/runtime/TCP and complete TLS target support are **NotChecked** here.

[#47](https://github.com/Reilley64/rill/issues/47) owns shared compiler/runtime/channel/TCP delivery. [#44](https://github.com/Reilley64/rill/issues/44) owns OpenSSL/private-shim/profile/asynchronous staging and target validation. Passing the finite reference matrix does not approve the candidate complete four-budget ledger, solve ticket discard/key-use enforcement, prove security/conformance or close either ticket. The [#39 HTTP design](http-contracts.md) consumes these interfaces without claiming those implementations exist; #48 retains HTTP delivery.

Required delivery evidence includes rendezvous and receive-select races; notifier coalescing/target loss; register/recheck/stale wake/withdrawal quiescence; full queues with independent terminal wake; retained errors and stop/disposal joins; real TCP backpressure/FIN/reset/prefixes; both TLS renegotiation roles; fragmented/control/backpressure/error/copy-overlap matrices; actual Rill metadata/ABI/leases and Linux/macOS target cells. Contradiction returns to the map/design rather than weakening general streaming or silently expanding budgets.
