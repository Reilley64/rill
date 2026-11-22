# ri-w0c: existing family, channel, control and wait seams

Facts-only fresh-context investigation for ri-w0c. No new API, policy choice, compiler/runtime implementation or support claim is proposed here. Sources were read, not executed. **Approved below means confirmed language/library design, not accepted RFC, implemented checker or working runtime.** Public ri-w0c signatures remain undecided; ri-n5i interviews continue.

## Source scope and precedence

- Local baseline: `20aa273175c20b82e67510722a812ab7a5f982c3`. The contracts explicitly distinguish design from implementation: [library catalogue](../standard-library-contracts.md), lines 1–25; [clock contract](../clock-cancellation-contracts.md), lines 1–16; [networking contract](../networking-contracts.md), lines 1–15. ri-48g owns formalization; ri-dej retains actual native/Wasm validation. No tests were run for this investigation.
- Later confirmed decisions supersede historical examples. In particular, [historical prototype04](https://github.com/Reilley64/rill/blob/20aa273175c20b82e67510722a812ab7a5f982c3/prototypes/04-goroutines-channels.rill) contains no-argument channels, old `copy(value)`, string-tag selection and variadic scoped calls. These are **not** current syntax. The catalogue explicitly calls out obsolete fixtures ([standard-library-contracts.md](../standard-library-contracts.md), lines 13–25, 258–293 and 588–592); use the consolidated library prototype and newer contracts instead.
- **TLS availability checked:** this checkout has no tracked `docs/tls-contracts.md`, `docs/tls-native-boundary.md`, ADR0025 or prototype27. That is checkout lag, not absence of approved TLS design. ri-irw's publication record (ri-irw, comment 6903669836) supersedes its earlier “local/uncommitted” status. The published public and private files at merge `15632ac1f88a7d1ca69cea66d083f6d61b897fb3` were retrieved read-only through GitHub for this investigation. They are cited below as **published sources**, not invented local links. Original authoritative snapshots: public (ri-irw, comment 6903320873), private/ADR (ri-irw, comment 6903320800).
- ri-w0c's body and research-input pointer (ri-w0c, comment 6904286500) explicitly say ordinary I/O does not yet supply the new progress/wait capability. ri-n5i research snapshots are standards/framing (ri-n5i, comment 6904280519) and selected-provider retry/duplex (ri-n5i, comment 6904280793), not finalized public APIs.

## 1. Canonical ordinary families and generic evidence

### Exact approved syntax and output parameters

One bodyless declaration owns a family; an exported family uses `export function`. Bodies extending it use `implement`, with generic binders **before the name**. Standalone definitions retain `function`. Current examples are:

```rill
implement show(value: &Local): string { ... }
implement<T> show(value: &Box<T>): string where show(T) { ... }
```

These are spelling illustrations, not executable bodies here. Ordinary implementations preserve declaration parameter count, specialized reference structure and return pattern. A result parameter may be exposed in the declaration when input specialization determines it; the catalogue's exact example is `function map<C, F, R>(from: C, transform: F): List<R>;`. `where family(A, B)` names that **canonical family's type arguments**, including result-related parameters. It is not a structural promise about matching names. Evidence includes the selected signature, invocation/access, properties and retained dependencies, checked before monomorphization. Public requirements must name publicly exported declarations; metadata does not inject source bindings. Sources: [catalogue](../standard-library-contracts.md), lines 9–25; [networking declaration convention](../networking-contracts.md), lines 13–15; [public requirement visibility](../adr/0010-file-modules-explicit-exports-and-privacy.md), lines 59–65.

For ordinary I/O the current complete family shapes are:

```rill
function read<R,E>(source: &mut R, destination: &mut Bytes, maxBytes: u64)
  : Result<Option<u64>, IoFailure<E>>;
function write<W,E>(destination: &mut W, source: &Bytes)
  : Result<u64, IoFailure<E>>;
function flush<S,E>(stream: &mut S): Result<void,E>;
function close<S,E>(stream: S): Result<void,E>;
type IoFailure<E> = { transferred: u64; error: Err<E> };
```

The requirements are **`read(R,E)`, `write(W,E)`, `flush(S,E)`, `close(S,E)`**. E is independently selected output evidence for each input implementation; it is not an argument selecting another body. One S may have different read/write/flush/close errors. `display(E)` and other properties are additional evidence, not implicit consequences. Sources: [catalogue](../standard-library-contracts.md), lines 408–459. Concrete canonical imports and bodies appear in the [current consolidated streaming prototype](../../prototypes/12-streaming-io.rill); its generic helpers state evidence explicitly. The local helper named `send` there is not the channel family.

### Identity, aliases, extension and duplicates

- Import/re-export aliases preserve canonical identity; equal spelling does not merge unrelated families. A family's declaration controls visibility of all bodies; bodies are not individually exported/hidden. Importing an extension does not automatically re-export its private operation binding.
- Every body in a loaded module contributes. Ordinary orphan placement requires the implementation module to own **the declaration or a type occurring in its parameter types**. Aliasing/importing grants no ownership. This is not a universal outermost-nominal-only rule.
- Same canonical parameter patterns are duplicates at collection, even without calls; normalize aliases and generic renaming. Return type and constraint truth cannot distinguish them. Concrete specialization may beat a generic pattern; incomparable applicable patterns are errors, never import-order choices. Expected errors do not authorize fallback selection.
- Required ownership actions have a separate protection rule: nested local payloads do not authorize intercepting genuine guard release, owner counts or native-retention actions. Custom disposal is followed by cleanup of remaining initialized fields; moved/disposed fields are not released twice.

Sources: [ADR0010](../adr/0010-file-modules-explicit-exports-and-privacy.md), lines 67–77; [catalogue](../standard-library-contracts.md), lines 79–95. **Do not import conversion-specific dispatch into ri-w0c:** only canonical conversions use `(To,From)` keys, selected E and reference-peeled outer nominal endpoint ownership; ordinary families keep the broader rule ([catalogue](../standard-library-contracts.md), lines 337–351).

## 2. Ordinary I/O ownership, prefixes, EOF and closure

Approved contract, not a cooperative single-step interface:

| Operation/fact | Existing meaning |
| --- | --- |
| Read | Exclusively borrows stream and whole destination Bytes; appends at most maxBytes without changing its prior prefix. Positive request: `Some(n)` with `1 <= n <= maxBytes`, or `None` with no append at EOF. Short data is not EOF/framing/UTF-8 completion. Zero request: immediate `Some(0)`, no consumption or EOF test. |
| Write | Exclusively borrows stream, retains read-only source access. Success accepts a current-input prefix, positive for nonempty input, zero for empty. Acceptance is layer-relative, not peer receipt/durability. |
| Availability | Read/write **suspend the routine** while unavailable, not return WouldBlock. Avoiding a blocked scheduler worker does not keep the calling coordinator available for unrelated work. |
| Failure | `IoFailure.transferred` is exact current-call append/acceptance, including failure after a full prefix. Preserve original `Err<E>` and outer envelope/origin/cause; no rollback, full-buffer replay or automatic conversion to another E. Helpers validate impossible counts/zero-progress successes as fatal adapter bugs. |
| Flush | Layer handoff, not durability/peer ACK. Buffered flush drains confirmed prefixes then calls underlying flush; failure retains only the unaccepted suffix. Drained data is not replayed if underlying flush fails. |
| Close | Consumes its stream even on recoverable error. Buffered writer close tries required underlying closure even after flush failure and can preserve both failures. No subsequent retry using the consumed owner. |
| Dispose | Discards pending output rather than implicitly flushing/gracefully shutting down. Remaining-field cleanup must not close a moved owner again. Scope exit does not guarantee delivery, body acknowledgement or remote rollback. |

Sources: [catalogue](../standard-library-contracts.md), lines 459–477; [streaming case traces](../prototypes/12-streaming-io.md). `readExact`/`writeAll` rebase cumulative prefixes without replacing the original error envelope; empty helpers do not call the stream. `readToEnd` has a bounded destination budget plus a retained one-byte overflow probe, not an unlimited read ([catalogue](../standard-library-contracts.md), lines 420–425, 465–467).

No blanket borrow-free/error-transfer rule exists. Approved scoped requirements are `noBorrow(E)` for readToEnd and BufferedReader read; `noBorrow(WE)` for BufferedWriter write/flush; consuming buffered close additionally `noBorrow(FE)`. No blanket noBorrow on primitives, caller-buffer exact/all helpers, buffered-reader close or underlying close error CE; no implied sendable/shareable. This matters if ri-w0c retains errors while mutating or consuming state. Sources: [catalogue](../standard-library-contracts.md), lines 457–459; [streaming evidence and rejections](../prototypes/12-streaming-io.md).

All suspended/native accesses retain stream, buffer, callback and registration dependencies until completion or **confirmed** abort. Abort request alone is not release; exclusive access cannot be duplicated to run ordinary read/write concurrently on one whole stream. Intrinsic cancellation is outside Result and may leave external prefixes without returning progress to stopped application code. Required cleanup can suspend indefinitely under deferred cancellation ([catalogue](../standard-library-contracts.md), lines 475–477).

## 3. Channels: actual capacity, transfer, selection and cleanup

Current fixed forms, not prototype04's older surface:

```text
channel<T>(capacity: u64): [Sender<T>, Receiver<T>]
  where noBorrow(T), sendable(T)
send<T>(&Sender<T>, T): Result<void,SendError<T>>
  where noBorrow(T), sendable(T)
receive<S,T>(S): Option<T>
select<C,R>(C): R
```

`receive` supplies **&Receiver<T> and &Shared<Receiver<T>>** implementations. `select` supplies **only tuples of 2, 3 or 4** cases; each case is `[&Receiver<Ti>, Fi]`, with `noBorrow(Ti), sendable(Ti), callableOnce(Fi,[Option<Ti>],R)`. For Shared receivers use their explicit view. Sources: [catalogue](../standard-library-contracts.md), lines 255–293.

- Capacity zero is rendezvous; positive capacity is bounded. No default/no-argument constructor, unbounded mode or recoverable allocation API. Channel capacity zero must not be confused with invalid zero **buffered-I/O adapter** capacity.
- Sender `copy(&sender)` retains another counted endpoint for the **same channel**, not a fresh queue. No standard Receiver copy: use Shared ownership. Channel endpoint synchronization/transfer does not require shareable(T) for messages. `Shared<Receiver<T>>` is competing consumption, not broadcast. Endpoint sendability/shareability is justified under the message requirements ([ADR0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md), lines 19–31, 37–39).
- Last sender closes; drain buffered messages before `None`. Last receiver ownership release makes pending/future sends fail and disposes undelivered buffered values once. A pending send owns its T; success commits transfer, failure returns undelivered T inside opaque SendError. `unsent<T>(&SendError<T>):&T` observes; `intoUnsent<T>(SendError<T>):T` recovers. Rendering SendError does not require display(T).
- Cancellation never rolls back a committed transfer. Uncommitted abandoned messages are disposed only after unregister/confirmed abort. A committed receive whose routine is cancelled may dispose its result rather than requeue it. Masked cleanup waits stay registered ([catalogue](../standard-library-contracts.md), lines 264–266; [concurrency traces](../prototypes/04-concurrency-safety.md)).
- Selection commits exactly one receive and executes only its handler in the selecting routine; nonwinning cases consume nothing and are withdrawn. Handlers/results gain no gratuitous noBorrow/sendable requirement merely for local dispatch, but retained dependencies still count. Source/handler state must survive registration, commit, handler and losing-case cleanup.
- Closed/drained cases remain ready with `None`. Ready ordering is unspecified; no source priority or starvation guarantee. Remove/restructure permanently closed cases. Racing independent receives is not an implementation of this atomic losing-case guarantee.

Sources for channel ownership/selection: [catalogue](../standard-library-contracts.md), lines 264–293. The [current consolidated library prototype](../../prototypes/09-library-contracts.rill) illustrates drain-after-close, unsent recovery and one winning commit; its quiet sender is explicitly retained until selection completes. These are hand-checked examples, not tests executed here.

**Absent from the inspected approved channel export list and contracts:** `trySend`, nonblocking send, tryReceive, send-select cases, capacity/space-ready notification, capacity reservation/permit, public unregister handle and arbitrary descriptor selection. Only receiver cases are specified. The export list is [catalogue line 47](../standard-library-contracts.md); the complete channel section is lines 255–293. An OS nonblocking send discussed in networking research is not a Rill channel capability. Therefore a bounded `send` by the sole coordinator cannot be assumed to return promptly when its receiver is stalled. Existing internal unregister obligations are not an application-accessible registration API.

## 4. Scoped work, returns, borrowed captures and masking

Exact current helper shapes ([catalogue](../standard-library-contracts.md), lines 225–253):

| Family | 2-item schema; only fixed 2/3/4 implementations |
| --- | --- |
| `join<W,R>(W):R` | `[F1,F2] -> [R1,R2]` |
| `all<W,R,E>(W):Result<R,E>` | jobs return Result<Ti,E>; result Result<[T1,T2],E> |
| `allSettled<W,R>(W):R` | jobs return Result<Ti,Ei>; tuple preserves distinct result/error types |
| `race<W,R>(W):R` | same R; first completion, including Err if R is Result |
| `any<W,T,E>(W):Result<T,AllFailed<E>>` | first success; otherwise preserve all original Err<E> in input order; also display(E) |

Each takes **one tuple of owned callable values**, not variadic parameters or a dynamic List. Each Fi needs `callableOnce(Fi,[],Ri), sendable(Fi), sendable(Ri)`; Fi can itself be an explicit borrow. No blanket noBorrow(Fi/Ri). Results can retain caller-owned dependencies, not destroyed child/callable-local storage. `&T` transfer needs shareable(T); `&mut T` transfer needs sendable(T). Disjoint exclusive captures are permitted; conflicting ones are not. Source: [ADR0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md), lines 19–35, 59–69. The [current consolidated library prototype](../../prototypes/09-library-contracts.rill) records current tuple calls.

Every scoped helper joins **all execution and cleanup** before return/cancellation completes. Winner/early-error selection is separate from return; losers are cancelled/joined, losing results disposed. None offers bounded join latency. `spawn<F>(F):Routine` instead requires `callableOnce(F,[],void), noBorrow(F), sendable(F)`; Routine has no typed task-result join. `stop(&Routine):void` requests and joins; handle drop neither stops nor joins. Repeated/concurrent stop shares termination, self-stop panics. Thus a scoped helper can contain borrowed application body work, but it does not make an arbitrary source/sink callback cooperative or permit returning while it still borrows state.

Intrinsic delivery is deferred during disposal/required joins/replacement installation; scheduling and completion continue. A pending request does not unregister a masked wait. No public masking controls or force kill. Native work that cannot abort can delay cleanup forever. Explicit token polling/typed cancellation remains truthful while intrinsic delivery is masked; cleanup that must ignore external requests uses neverCanceledToken. Sources: [ADR0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md), lines 61–84; [clock contract](../clock-cancellation-contracts.md), lines 162–172.

## 5. Clocks, tokens and receiver-backed subscriptions

**Owned reusable handles:** Clock/ManualClock are noBorrow/sendable/shareable, not implicitly copied; `copy(&handle)` retains the same domain/service/controller. `manualClock():ManualClock`, `clock(&ManualClock):Clock`, `now(&Clock):Result<Instant,ClockError>`, `advance(&ManualClock,SignedDuration):Result<void,ClockError>`. Manual domains start at zero, advance atomically forward and drive actual registrations as well as readings; due waits are eligible before successful advance returns, not necessarily executed. Dropping the last controller neither advances nor closes waits. Instant is a copyable domain-tagged scalar; ordering/deadlines require the right domain. `validateInstantDomain`/`checkInstantDomain` are canonical aliases doing identity-only validation, not a clock read or health guarantee. Sources: [clock contract](../clock-cancellation-contracts.md), lines 20–24, 39–76.

**Wait shapes and receiver ownership** ([clock contract](../clock-cancellation-contracts.md), lines 84–106):

```text
sleepFor(&Clock, SignedDuration, &CancellationToken): Result<void,WaitError>
sleepUntil(&Clock, Instant, &CancellationToken): Result<void,WaitError>
afterFor(&Clock, SignedDuration, &CancellationToken): Receiver<Result<void,WaitError>>
afterUntil(&Clock, Instant, &CancellationToken): Receiver<Result<void,WaitError>>
```

Relative waits compute one absolute target, not a refreshed timeout. Validate basic/domain arguments, then already-requested token, then read/calculate/register. Later expiry/request races have no guaranteed winner. Receiver factories retain owned service/token state; they do not borrow factory caller slots. Each receiver emits one Ok or Err then closes; setup errors are already-eligible messages, not nested factory Result. Factory establishment/disposal is not guaranteed nonsuspending. Last receiving-owner release withdraws uncommitted subscriptions and waits for required release; already committed messages follow channel ownership. Separate timer subscriptions are independent. Compatibility `sleep(u64):void` / `after(u64):Receiver<void>` use default suspend-inclusive elapsed time and fatal unsupported/failure handling, not silent EOF. Timer eligibility is not proof of timely return.

**Tokens** ([clock contract](../clock-cancellation-contracts.md), lines 128–172): Source and CancellationToken are owned noBorrow/sendable/shareable handles, not references into source slots. `token(&source)` returns a token owner. Explicit copies retain authority/state (source) or observation only (token); ordinary disposal neither cancels nor joins. Request is persistent, one-shot, no reset. `isCancellationRequested(&token):bool`, `waitForCancellation(&token):void`, `cancellationSignal(&token):Receiver<void>` are the observation paths. Each signal receiver is a separate one-shot subscription; sharing one receiver does not broadcast. Last receiving-owner disposal safely unlinks it. Registration/recheck synchronizes with cancel; no retained dead waiters or lost wake window is permitted, but no public callback/registration handle is supplied. Child/OR links retain necessary states and finish required propagation before cancel returns, without waiting for user operations. CancelOnDispose requests cancellation, **not join**.

`withDeadline<F,R>(&Clock,Instant,F):Result<R,DeadlineError>` uses scoped sendable callable/result evidence, can borrow caller state, preserves nested Result and joins losers; it is not a hard timeout or generic cancellation-as-yield facility ([clock contract](../clock-cancellation-contracts.md), lines 108–124). No generic `withCancellation`, token inheritance, public Future/Poll, async trait or public mask API is approved (lines 8–16, 124).

## 6. Borrowed NetControl versus owned messages

Exact current definition ([networking contract](../networking-contracts.md), lines 274–284):

```rill
export type NetControl = {
  clock: &Clock;
  deadline: Option<Instant>;
  token: &CancellationToken;
};
```

It is **not noBorrow**. Ordinary public fields are editable through mutable access; an operation/view captures a stable by-value descriptor snapshot that retains clock/token/resource dependencies. Copying this descriptor does not own another handle or remove its borrows. The chosen token continues observing later requests. `controlledTcp<S>(&mut S,NetControl):ControlledTcp<S>` is a noncopyable borrowing view; constructor stores dependencies without preflight. Supplied canonical evidence depends on whole/read-half/write-half capability; no close on the view. Dropping it releases the view, not the TCP owner. Its exclusive dependency blocks conflicting I/O/split/close/dispose. Mandatory close/dispose is not externally abortable ([networking contract](../networking-contracts.md), lines 299–315).

Owned Clock and CancellationToken handles **can** be retained in noBorrow/sendable command/control records when all other fields meet the same rules; borrow NetControl temporarily from that retained owned state rather than treating NetControl itself as a channel message. This follows the handle properties above, not an invented owning NetControl overload. An already approved precedent is SqlControl's `Option<[Clock,Instant]>` and CancellationToken, consumed by controlled operations, explicit copy retaining the same domain/token/absolute target ([SQL contract](../sql-contracts.md), lines 74–84; [catalogue](../standard-library-contracts.md), line 574).

For ri-n5i specifically, this investigation treats the task's accepted **working** HttpControl shape (`deadline: Option<[Clock,Instant]>`, `cancellation: CancellationToken`) as an interview input, not a finalized exported declaration. Unlike borrowed NetControl, anything sent through ordinary channels must establish noBorrow and sendable. Retaining owned control does not imply arbitrary stream ownership is noBorrow, erase a caller borrow or renew the absolute request deadline.

## 7. Published TLS constraints, not an available public progress API

Published sources: [public TLS contract at 15632ac](https://github.com/Reilley64/rill/blob/15632ac1f88a7d1ca69cea66d083f6d61b897fb3/docs/tls-contracts.md), [private semantic boundary at 15632ac](https://github.com/Reilley64/rill/blob/15632ac1f88a7d1ca69cea66d083f6d61b897fb3/docs/tls-native-boundary.md).

- Public lines 161–183 require independently selected `read(S,RE), write(S,WE), flush(S,FE), close(S,CE)`; each error needs display/noBorrow/sendable/shareable for retained unique causes. No global noBorrow(S). Owned handshake inputs, no TLS split/reunite, raw transport escape or independent concurrent application drivers. One operation can pump both directions.
- Public lines 185–229 retain canonical exclusive I/O arities. Reads return authenticated plaintext/close-notify EOF, not raw TCP EOF; truncation/fault tails preserve real prefix accounting. Writes count current-input plaintext accepted into bounded TLS-owned storage, not ciphertext. Flush can need opposite-direction traffic. NetControl remains borrowed. Post-admission failure/true abandonment poisons; effect-free pre-admission rejection preserves health. Mandatory release must still complete; close consumes on failure and does not wait for peer notify. Disposal is not graceful notify.
- Private lines 24–55 contain `FeedCiphertext`, `AcceptPlaintext`, `Advance`, `TakeCiphertext`, `TakePlaintext`, adoption/release and saved failure facts. **These names are private semantic operations, not public Rill families, native opcodes available to applications or a completed ABI.** Managed arbitrary S I/O remains outside C. WANT is continuation with stable original buffer identity/content/cursor, not public readiness or EOF.
- Private lines 59–83 require joined admitted native work, same-thread classification/error capture, retained leases, real operation/wake generations, stale/late event rejection, atomic adoption/loser release and callback/registration quiescence before reclamation. These are obligations, not proof or an existing public continuation runtime.

The ri-w0c body explicitly requires a new amendment for its bounded BIO-pair/budget-partition direction and a source-backed retry/interleaving policy; ri-irw's private catalogue alone does not authorize arbitrary alternation or a generic resumable transport. ri-ei7 gates actual TLS delivery/support/v0 readiness; ri-dej retains runtime/ABI/lease/target gates. Source contracts and provider source observations do not discharge either.

## 8. Missing reusable abilities and constraints on a new seam

These are gaps/requirements to resolve in ri-w0c, **not chosen signatures or new permissions**:

1. **Finite/cooperative progress entry:** ordinary read/write/flush can suspend indefinitely with exclusive borrows. No approved public finite-step/resume result separates temporary pending from terminal EOF/error while preserving single ownership. A readiness hint followed by arbitrary ordinary I/O, a mutex or a long-lived task does not supply that boundary ([catalogue](../standard-library-contracts.md), lines 461, 475; ri-w0c body).
2. **Retained operation/continuation:** no reusable public contract yet specifies stable owned pending buffers/native jobs, re-entry identity, independent selected error outputs and exact late/current-layer prefixes across resumptions. Private TLS semantic staging is not that generic contract. Caller scratch may not outlive its permitted borrow interval (sections 1–2, 7 above).
3. **Application-safe outbound backpressure:** bounded channels can carry owned chunks/commands but expose no try-send, send-capacity wait or reservation. Arbitrary application sources/sinks can block too. A sole coordinator must not enter those operations or a full-queue send and lose all other progress. Scoped/owned body work must be explicitly retained and joined; no detached cleanup or unbounded buffering follows (sections 3–4; ri-w0c body).
4. **Terminal notification independent of full body queue:** last-sender close still drains preceding queued values before None; send of a terminal value can block. Existing channels do not promise an out-of-band terminal slot/priority path. ri-n5i's requirement that terminal notification not await freeing the full body queue therefore needs explicit accounting, not an assumption about ordinary closure (section 3; task's ri-n5i working inputs).
5. **Composable race-free waits:** fixed receiver select, timer receivers and persistent cancellation subscriptions are reusable *facts*, but no arbitrary-descriptor/variadic selector, public registration ticket, transport readiness/capacity receiver or generic unregister/join interface exists. A new seam needs explicit register/recheck, stale-wake handling, retirement, losing-case withdrawal and real release while fitting actual fixed 2/3/4 selection rather than pretending it already generalizes (sections 3, 5, 7; ri-w0c body).
6. **Nonterminal wake versus real abandonment:** app activity/soft protocol timer expiry must not silently mean token cancellation, delivered intrinsic stop or admitted TLS abandonment. Real cancellation selects a terminal outcome and performs actual cleanup; timer/app wakes may merely resume retained work. No cancellation-as-yield or timeout-as-progress shortcut is justified by withDeadline or TLS poisoning rules (sections 4–5, 7; ri-w0c body).
7. **Transport/provider handoff:** TCP's real halves do not imply TLS halves. Preserve one TLS owner, documented WANT/retry restrictions, bounded buffers, authenticated EOF and poison/closure behavior; amend ri-fnk/#34/#38/#44 explicitly where new semantics exceed current contracts. No raw/native handles, dyn/associated outputs, Poll/Future/async traits or generic withCancellation may be assumed (sections 1–2, 5–7; ri-w0c body).
8. **Runtime and liveness evidence:** admitted native jobs, pointer stability, executor admission, generation-safe wakes, loan retention, publication/adoption, masked joins and reclamation remain actual ri-dej/#44 work. Neither design closure nor manual clocks imply hard cancellation/progress latency. An application refusing consumption can legitimately stop body progress; no universal liveness claim is available (sections 2–5, 7).

The accepted ri-n5i working scope is HTTP/1.1 bidirectional bodies with **one active exchange, no pipelining**, a single owner/coordinator, transport-independent codec and bounded application channels. Those are consumer constraints for this investigation, not finalized public signatures. This note does not select any missing API or revise those choices. ri-w0c design confirmation and later actual ri-dej/#44 support evidence remain separate completion events.
