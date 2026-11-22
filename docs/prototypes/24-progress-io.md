# Prototype 24: channel wakes and single-owner I/O progress

Read [`prototypes/24-progress-io.rill`](../../prototypes/24-progress-io.rill).

The human-confirmed ri-w0c contract, [progress-I/O contract](../progress-io-contracts.md) and [ADR0027](../adr/0027-single-owner-progress-and-coalescing-wakes.md) are authoritative. This source is a compact syntax/ownership model, not a standard implementation. It does not add APIs beyond those contracts.

The file contains **11 intended-valid expectations**. They were counted and hand-reviewed only. No Rill parser, typechecker, compiler, runtime, channel, TCP or TLS implementation executed them. The local `ModelDriver` is application-owned deterministic state used to illustrate canonical family bodies and outcome movement; it is not `TcpProgress`, `TlsProgress`, a scheduler, a native job or a proof that user implementations are safe.

## Intended-valid source coverage

| Expectations | HandChecked intent |
| --- | --- |
| 1 | Buffered try-send commits and try-receive moves the same value. |
| 2 | A full bounded channel returns the second unsent value unchanged. |
| 3 | Capacity-zero try operations do not wait or reserve an absent counterpart. |
| 4 | Last-receiver loss returns `SendAttempt.Closed(T)`; last-sender close yields closed-and-drained receive. |
| 5 | Repeated notifier calls coalesce to one wake token; the next try-receive is empty. Multiplicity is not promised. |
| 6 | A one-shot receive watch wakes a shared destination, then the authoritative try-receive obtains the item. Explicit notifier copy/root ownership and disarm are visible. |
| 7 | A starting model emits `started`, accepts a positive read and transfers one owned data completion. |
| 8 | Zero read maximum is an effect-free `InvalidInput` rejection. |
| 9 | A rejected write returns its unchanged input. The miniature case uses `NotStarted`; the independent inventory covers concrete-size `LimitExceeded`. |
| 10 | Admitted owned write completes through `advance`, not admission. |
| 11 | Stop request is nonwaiting; one later step emits `stopFinished` exactly once and terminal advances stay Idle. |

`waitOnce` separately demonstrates explicit generic evidence for registration and withdrawal with the same `G`/`DE`, postfix forwarding of `DriverFailureSet<DE>`, and consuming guard withdrawal before the caller could regain mutable driver access. It is uncalled because no fake guard/native registration is fabricated.

The model implements every canonical admission/advance/stop family using `implement`, with independently explicit five error outputs on `ProgressStep`. It intentionally does not simulate failures, flush-behind-write ordering, native completion races or cleanup; those are independent cases below and delivery work in ri-hw5/#44.

## Independent rejection and adversarial inventory. NOT RUN

| Case | Required boundary |
| --- | --- |
| `trySend`/`tryReceive` on T lacking noBorrow or sendable | Reject missing channel-message evidence before monomorphization. No watch/wake path erases it. |
| Treat `Full`/`Empty` as transfer, discard returned T, or retry the whole value after Sent | Ordinary ownership applies; only Sent/Item commit. Cancellation does not roll back a committed transfer. |
| Capacity-zero watch counted as a waiting receiver/sender | It reserves nothing; try operation remains Full/Empty until a real counterpart waits. |
| Race a ready receive-select and try-receive | Exactly one receive commits; losing select cases consume nothing. No first-source priority guarantee. |
| Copy a `ChannelWatch`, share mutable arm/disarm, or retain sender/receiver counts through a watch | No supplied copy/share capability; exclusive mutation and protected endpoint accounting apply. |
| Arm an already eligible watch repeatedly without probing source state | Each arm can post a wake; coordinator misuse can loop. One-shot coalescing is not edge counting. |
| Drop the wake receiver and interpret watch retirement/`wake=false` as transport EOF or operation completion | Target loss only retires notification. Authoritative driver/channel state is separate. |
| Leave closed wake/timer/control receivers permanently in select | Closed receivers stay ready with None; remove/restructure cases to avoid a loop. |
| Call `advance` while a `registerProgressWait` guard still retains the driver | Reject conflicting mutable access. Consume withdrawal/disposal and quiesce first. |
| Move/return a wait guard beyond its driver, or assert noBorrow/sendable without evidence | Retained dependencies remain. There is no public pinning/lifetime annotation escape. |
| Generic coordinator omits one admission/advance/register/withdraw family or relies on an expected error to select it | Reject missing/ambiguous canonical evidence; no hidden ProgressDriver trait/associated output. |
| Install duplicate `advance` bodies for the same D that differ only by return errors | Reject at family collection; return type cannot distinguish ordinary implementations. |
| `admitRead(...,0)`, standard write over 16384 bytes, app admission before TLS started, second same-lane admission, write behind a flush barrier, close with a live lane | Return exact local rejection (`InvalidInput`, `LimitExceeded`, `NotStarted`, `Busy`) with no lower effect; write rejection owns the unchanged bytes. |
| Treat admission as completion, successful TLS write as ciphertext/peer receipt, or interruption as zero effect | Layer-relative milestones and unknown effects remain distinct; no rollback/replay inference. |
| Overwrite a retained completion or admit a new lane before its result transfers | Reject Busy and retain the original outcome/wake. Coalescing cannot erase it. |
| Convert a read data-plus-error into Data/End, clone a cause into dependent lanes, or let cleanup replace a primary | Preserve owned prefix and original Err once; dependent `Interruption` is classification only; use paired driver failures. |
| Register driver wait while Runnable/Idle or mutate after another select case wins without withdrawal | Follow schedule discipline and withdraw/quiesce before mutation. App/soft timer wins do not cancel retained I/O. |
| Treat executor/ring/full application queue as failure, or spin on Waiting | Temporary pressure is pending with a race-free one-shot registration. Persistent bounded-state impossibility alone selects LimitExceeded. |
| Stop request/drop/deadline frees native storage immediately | Request is not completion. Preserve loans/jobs/callbacks until `stopFinished`; cleanup may wait indefinitely. |
| Dispose a driver as graceful close or suppress its unreported cleanup effects | Disposal requests terminal stop, not notify/flush/delivery; explicit close/stop is required for observable outcomes. |
| Public TLS split/raw lower extraction, ordinary blocking adapter, Future/Poll/dyn/associated errors, variadic selector or native descriptor watch | No such v0 capability. Use the concrete single owner and named families. |
| Continue TLS1.2 application I/O after the fixed observer records a post-handshake renegotiation request | Terminal ProtocolFailure/Control and stop. The native reference demonstrates peer record failure under unsafe continuation. |

## Evidence boundary and handoffs

The [standalone OpenSSL reference](../research/0045-progress-reference/) is separate **ReferenceExecuted** evidence. Its current pinned macOS-arm64 matrix passes13/13, but it is in-memory C/OpenSSL, not this Rill source, TCP, runtime integration or full provider validation. In particular, its candidate four-direction staging ledger is not physically implemented.

ri-hw5 owns actual channel/compiler/runtime/TCP delivery. ri-ei7 owns OpenSSL progress, both renegotiation roles, asynchronous BIO/copy accounting, tickets/key-use/profile enforcement and target validation. Until those run, all Rill and live standard-driver claims here are HandChecked or NotChecked as labelled.
