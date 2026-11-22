# ri-n5i: single-owner duplex progress and TLS retry constraints

## Status, authority and scope

**Research only. No API/interface, family arity, Rill source syntax, provider amendment, implementation, conformance result or security proof is selected here.** No compiler/runtime exists for executing this design. No tests or probes were run. Reading provider source establishes constraints, not a working Rill integration. Open alternatives below are historical research inputs; the later human-confirmed [progress](../progress-io-contracts.md) and [HTTP](../http-contracts.md) contracts govern selected behavior.

The task's confirmed direction is **HTTP/1.1 initially, HTTP/2 and HTTP/3 deferred; general streaming clients and servers**, not a restricted write-all/read-later profile. The missing single-owner bidirectional progress capability is to be designed explicitly. That direction does **not** authorize concurrent mutable TLS application drivers, TLS split/reunite, raw owner escape, timeout-as-yield, or assumed polling/native hooks.

Authority examined directly:

- Exact ri-irw public snapshot, comment 6903320873 (comment 6903320873), especially §§7–10 and §12: eligible generic `S` supplies canonical read/write/flush/close; one exclusive TLS application driver; no TLS split, raw transport extraction or independent application drivers. One operation can pump both directions, but this does not promise general duplex scheduling. Admitted interruption/abandonment poisons the session.
- Exact ri-irw private snapshot, comment 6903320800 (comment 6903320800), especially boundary §§2–6: selected OpenSSL **3.5 LTS**, inspected **3.5.9**, commit **`45e844fa2a14ec92d146bd8f5778ac130b6625fb`**. The commit's [VERSION.dat][version] independently confirms 3.5.9. Private memory-BIO feed/drain, stable owned retry storage, serialized engine steps, managed transport I/O outside C, and quiescent release are already design obligations—not newly delivered capabilities.
- [Networking contracts](../networking-contracts.md), §§5–6, 8, 10: whole TCP owners are exclusively borrowed during I/O; **explicit TCP split** permits independent lower directional owners. It does not give arbitrary generic `S` a split/readiness capability or make TLS directions independent. Native Linux/macOS support remains unvalidated; live standard WASI preview1 networking is unsupported.
- [Clock/cancellation contracts](../clock-cancellation-contracts.md), §§3–7: checked domain-tagged elapsed clocks and timers, select-once control, retained registrations and cleanup; no wall/elapsed reinterpretation.

The snapshot comments are the reference even if their named local TLS documents are absent in this checkout. No later issue discussion or speculative API is substituted for them.

## 1. Why HTTP/1.1 needs more than serialized blocking calls

[RFC 9112 §9.5][http11] says a client sending a body **SHOULD monitor the connection for an error response while transmitting**. If the response says the server does not want the body and is closing, the client should cease body transmission and close its side. [RFC 9110 §10.1.1][http] allows a final response to `Expect: 100-continue` before sending the request body, and requires an origin server to respond with a final status or immediate 100 when the stated conditions apply, rather than waiting for content.

Two concrete scheduling problems follow (design deductions, not claims that every streaming application avoids its own deadlock):

1. A large client upload can wait on outbound backpressure while the peer's early final response waits inbound. A server can likewise need to write a response while a client is still uploading. Waiting for one complete blocking direction before considering the other is not a general streaming solution.
2. Even without `Expect`, a streaming application can depend on received bytes to produce its next outgoing bytes. Merely having simultaneous TCP directions is insufficient if one exclusive application operation holds the TLS owner indefinitely.

Small plaintext writes and bounded queues reduce how much can be committed ahead; they do not bound a write/flush/read's wait or make a blocked exclusive borrow available. A mutex serializes access but does not solve an owner held across a suspending read. Filling a queue and calling blocking flush can reproduce the same problem.

The HTTP monitor obligation is also **not** a promise of instantaneous stopping: data already accepted by TLS, handed to the transport or received by the peer cannot be recalled. A final response is not proof of request nonexecution or safe replay. Persistence independently requires complete, unambiguous framing: [RFC 9112 §9.3][http11] requires a server to read the request body or close after responding, and a client to read the response body to reuse the connection.

## 2. OpenSSL's retry contract at the selected revision

All OpenSSL sources below are pinned to the selected commit. These are ordinary TLS/non-QUIC facts; QUIC event handling and QUIC concurrency APIs in the same documentation are **not** evidence for this HTTP/1.1 transport design.

### 2.1 Completion, retry identity and partial writes

| Fact from selected provider | Consequence for a single-owner design |
| --- | --- |
| [`SSL_write_ex`][write] normally reports success only after the entire supplied chunk has been written to the SSL connection. A WANT result can follow partial internal processing. | WANT is not a successful short write and is not permission to advance the native retry slice. Preserve the original in-flight operation and cursor. |
| A write returning WANT_READ or WANT_WRITE must be retried with the **same arguments**. `SSL_MODE_ACCEPT_MOVING_WRITE_BUFFER` permits a different pointer, but the **same bytes and length** are still required. [Write manual][write], [mode manual][mode]. | Without that mode, address as well as content/length must remain valid. With it, movement alone is relaxed; replacement data, changed length or a different queued application write is not allowed. This note does not select the mode. |
| `SSL_MODE_ENABLE_PARTIAL_WRITE` permits successful completion after a message block (documented as 16 KiB). After successful partial completion, a **new** write handles only the remaining suffix. It cannot be enabled halfway through an incomplete write. [Write/mode manuals][write]. | Distinguish a completed partial write from a suspended retry. Neither mode permits abandoning the pending write on WANT. Mode choice is not a substitute for retaining owned buffers. |
| [`SSL_read_ex`][read] succeeds with application bytes; it can require several transport reads for one complete record, return buffered record data, or return WANT after processing non-application traffic. It can also need output. | Keep a stable owned receive destination and continuation when pausing the native read; do not expose a live native borrow into caller `Bytes`. The write-specific moving-buffer mode says nothing about read storage. |
| The [nonblocking guide][guide] instructs retrying exactly the original operation. The read manual does not spell out an identical-buffer rule as explicitly as the write warning. | Do not claim a general read-buffer-changing guarantee from this evidence. Retaining the original private read buffer/count is the conservative design basis; any relaxation needs its own source justification. |

The [record-layer implementation][records], `tls_write_check_pending` and `ssl3_write_bytes`, tracks `wpend_tot`, `wpend_buf`, record type and `wnum`; invalid pending retries can produce `SSL_R_BAD_WRITE_RETRY` or `SSL_R_BAD_LENGTH`. This supports the documented obligation; the implementation's particular checks are **not** permission to weaken the documented same-data/same-length rule.

Rill's public progress accounting is separate. The public snapshot counts plaintext accepted into bounded **TLS-owned staging**, not the return count from OpenSSL, ciphertext emitted, lower transport acceptance, acknowledgement or peer execution. A driver may still owe a native retry for bytes already publicly accepted. It must not accept the same input twice or charge earlier acceptance again when that retry completes. Conversely a native retry can generate output without appending any public plaintext. Public §8 (ri-irw, comment 6903320873), private §§3–4 (ri-irw, comment 6903320800).

### 2.2 Which other application operation can run between retries?

This is **asymmetric**, not an unconditional “alternate read and write after every WANT” rule.

The selected [`SSL_get_error` manual][error], WANT_READ/WANT_WRITE section, explicitly states:

> It is safe to call SSL_read() or SSL_read_ex() when more data is available even when the call that set this error was an SSL_write() or SSL_write_ex().

It then requires retrying the original write to continue sending its data, and adds:

> If you get SSL_ERROR_WANT_WRITE from SSL_write() or SSL_write_ex() then you should not do any other operation that could trigger IO other than to repeat the previous SSL_write() call.

Read together:

- **Write → WANT_READ:** a serialized read when more data is available is explicitly permitted; the original write obligation survives. Save its state independently of the result of the intervening read.
- **Write → WANT_WRITE:** retry that write before another SSL I/O-triggering operation. In a memory-BIO arrangement, draining already-generated ciphertext and making lower transport progress are necessary driver actions, not a license to call another SSL application I/O operation.
- **Read → WANT_READ or WANT_WRITE:** the read manual requires satisfying the BIO need and repeating the call. These sources do not furnish an exhaustive interleaving matrix for every read/handshake/control state. In particular, the permission for read-after-write-WANT_READ must not be silently promoted into permission for arbitrary opposite operations in all states. A proposed scheduler needs a source-backed transition policy for its actual TLS 1.2/1.3 profile and chosen BIOs; blanket read-then-write alternation is not justified here.
- WANT can change direction on retry; the manuals promise no fixed upper bound on retries before application-visible progress. Disabled renegotiation/PHA in Rill does not eliminate all post-handshake control work or this retry discipline. [Read][read], [error][error], public §6 (ri-irw, comment 6903320873).

Thus **one native call at a time is necessary but not sufficient**: serial calls can still violate a pending write retry. Conversely preserving a pending operation is not the same thing as holding an active foreign frame while waiting: a memory-BIO SSL call can return with its state retained for later continuation.

OpenSSL [thread guidance][threads] says most objects are unsafe for simultaneous use, and thread support is not cancellation safety. In-progress calls cannot generally be abandoned by thread cancellation, forced termination, exceptions or other non-local exit. Rill's private boundary already requires joined execution, no managed suspension/re-entry inside foreign steps, and error classification on the same native thread immediately after the SSL operation, before unrelated OpenSSL calls or migration. A serialized driver must preserve those rules, not merely place an SSL pointer behind a lock. Private §§4–6 (ri-irw, comment 6903320800), [error][error].

**Liveness consequence:** if a pending write returns WANT_WRITE, draining its ciphertext may itself meet lower outbound backpressure while the peer is sending inbound application data. The driver cannot simply invoke SSL_read in disregard of that restriction. It may need bounded lower ingress retention and a carefully constrained scheduling policy. Finite ingress can fill; neither the sources nor these design sketches prove arbitrary bidirectional application traffic deadlock-free. This is a real analysis gate, not a reason to invent unlimited queues or claim the general goal has already been met.

## 3. Memory BIOs: local TLS state, not transport readiness

### 3.1 Feed/drain and bounded output

A memory BIO replaces provider-owned socket access with driver-controlled ciphertext input/output. It does **not** make `S` nonblocking, supply a readiness subscription, or automatically enforce Rill's memory bounds.

- [`BIO_s_mem`][mem] stores bytes written for later read; ordinary writable memory BIOs grow as long as memory is available. [`mem_write`][memsource] calls `BUF_MEM_grow_clean`. There is no stream-memory-BIO write-buffer capacity promise analogous to the datagram-specific option. Checking the size **after** an SSL step can discover overshoot, but cannot prove it never exceeded the limit.
- Empty writable memory BIO reads can be configured as retry rather than EOF (`BIO_set_mem_eof_return`, normally negative for retry). Temporary lack of ciphertext must not become clean TLS EOF. `BIO_new_mem_buf` is read-only and **does not copy** supplied storage; its bytes must remain unchanged until release. Pointers obtained from memory BIO getters are not owned output values. [Memory BIO manual][mem].
- A [`BIO_s_bio` pair][pair] has bounded buffers: empty reads and full writes request retry; oversized writes can be short. The documented default is 17 KiB, not Rill's selected queue size. It offers capacity/read-request observations but no managed transport notification or synchronization; both halves normally use the same application thread because internal data structures are not locked.
- The pair manual explicitly warns that **WANT_READ can coexist with pending output**. Drain/send generated output before waiting only for input, or the peer may wait for a request still sitting locally. Under lower backpressure, “drain first” cannot mean block forever on a write and forget all other eligible work.

A bounded BIO pair is therefore an existing provider mechanism worth evaluating, **not a selected change to the private boundary**. Other proposals would need a reviewed bounded native BIO strategy or a justified pre-step bound on maximum possible output. No custom BIO callback/native hook is invented here. The private boundary's operation catalogue does not itself establish that a stock writable memory BIO enforces the promised ceilings.

The public snapshot fixes **65,536 bytes each** for pending plaintext input/output and ciphertext input/output, an initial ciphertext prefix charged to ingress staging, and additional reassembly/work/record limits. An eventual design must account for where bytes live: TLS internal buffers, BIOs, native retry storage, managed transport tails, app queues, and copies during handoff. Moving a tail from a BIO to a second queue is not “free space” unless all accounting remains true. These limits are not a total transient allocation/CPU/stack proof. Public §10 (ri-irw, comment 6903320873), private §4 (ri-irw, comment 6903320800).

### 3.2 Reading can generate control output; receiving can hit app backpressure

The [read manual][read] expressly allows reads to require writes, including after the initial handshake; the [mode manual][mode] explains processing non-application records and `SSL_MODE_AUTO_RETRY` (on by default). With nonblocking BIOs AUTO_RETRY can process control records until input exhausts or application data appears. Without it a WANT_READ may be a processing boundary while data remains locally buffered. It is not an application scheduling yield contract or a per-call CPU/work bound.

The driver must retain authenticated plaintext encountered while driving write/flush/control needs and must arrange output handoff even when the initiating request was a read. A full inbound plaintext queue cannot be resolved by dropping bytes or decrypting indefinitely into new queues. If the application consumer is stopped, continued TLS progress can legitimately become impossible within bounds. Rill currently specifies typed limit failure/poison for unsupported progress within its limits, not silent loss or unlimited buffering. Determining when to wait for consumer capacity versus fail requires explicit policy; protocol processing cannot generally skip earlier application records to reach arbitrary later control records. Public §§8–10 (ri-irw, comment 6903320873), [read][read], [records][records].

### 3.3 `SSL_pending` and `SSL_has_pending` are not a readiness API

At this revision, [`SSL_pending`][pending] counts **processed, buffered application bytes available immediately**. It can be zero while unprocessed records are already buffered, even with no more lower BIO/socket bytes readable. `SSL_has_pending` includes processed and unprocessed buffered data, but can be true for a partial record or control record whose processing yields **no application data**; repeated calls can remain true without a successful app read.

Neither query reports writability, wake registration, cancellation, remote closure, or the readiness of arbitrary `S`. BIO pending counts concern yet another local queue. Socket-readable is not synonymous with a whole decryptable TLS record; socket-writable is not completion of a TLS write. A correct wait policy considers local actionable work, input need, output capacity, consumer capacity and the pending retry before sleeping, and avoids spinning merely because `SSL_has_pending` remains true. [Pending][pending], [read][read], [pair][pair].

## 4. The missing capability is below and around TLS

There are at least four distinct states of progress:

1. **OpenSSL operation state:** a native call returned, possibly with a retry obligation. Engine state, buffers and operation identity survive; no foreign frame is waiting on `S`.
2. **Ciphertext transport state:** reading/writing/flushing `S` may suspend indefinitely. A WANT from a memory BIO describes that BIO's needs/capacity, not whether `S` would complete now.
3. **HTTP protocol state:** parse informational/final responses, decide which body bytes may still be admitted, preserve framing, schedule local output, and determine closure/reuse separately from TLS success.
4. **Application-body state:** a source may be waiting for a file, channel, computation, remote service or incoming response; a consumer may be waiting for storage or application processing.

The current generic evidence only says `S` can perform canonical blocking/suspending I/O. Once an exclusive `S.read` is admitted, the caller cannot assume it can safely withdraw it, release its borrow, issue `S.write`, or receive a nonterminal “not ready” result. A scheduler worker being free while the routine suspends does not free that borrow. The private TLS catalogue's `Advance`/Feed/Take operations solve none of these managed `S` scheduling questions. Public §7 (ri-irw, comment 6903320873), private §3 (ri-irw, comment 6903320800), [networking §§6,8](../networking-contracts.md).

The **new transport capability to be designed** would need some justified means to preserve in-flight state while allowing the owner to schedule both directions: for example, genuinely nonblocking attempts plus owned waiting, or an ownership-safe lower duplex execution arrangement. It must specify progress, exclusive access, stable storage, notification, error/control and cleanup semantics. These are capability requirements, **not names for existing standard functions**. A readiness query followed by an arbitrary blocking generic read is not enough: readiness can change and a wrapper may need more data or have its own buffering.

TCP split can be used only where explicitly available and deliberately consumed. It could support owned lower ingress/egress execution around a *single* TLS engine owner, but it is not a generic transformation of any `S`, not a split of `TlsStream`, and not evidence that HTTP can recover the already-owned transport from TLS. Such an arrangement needs a designed ownership boundary before construction and cannot bypass ri-irw by reaching through the owner.

## 5. What a resumable single-owner design would have to retain

“Step + wait” below is descriptive, not an API recommendation. Likewise “owner task” is an architectural possibility, not an already implemented runtime service.

| Required state/obligation | Why it survives a pause |
| --- | --- |
| Exactly one authority for SSL mutation and protocol scheduling | Prevent overlapping native calls, illegal retry interleavings, and competing app drivers; serialization includes intervening control operations. |
| TLS engine, underlying transport ownership/dependencies, retained configuration | A pause does not release the connection or invalidate borrowed `S`/clock/token dependencies. Transfer of ownership needs actual sendability evidence. |
| Stable native write bytes, original count/address as required, cursor and retry kind; private read destination | The next invocation may have to repeat the original operation, long after the initiating public call accepted bytes. Managed copies must be made before native use, not pointers into resizable caller storage. |
| Ordered owned ciphertext ingress and outgoing short-write suffixes; authenticated plaintext tails | Partial lower I/O must resume exactly once, preserve order and distinguish ciphertext diagnostics from public plaintext progress. |
| Public acceptance/delivery counters, pending fault owner and HTTP framing state | Pausing/retrying cannot double-count acceptance, clone an original unique error or lose already-authenticated data. |
| Explicit read/write/flush demand and local buffer/work budgets | Output can exist on WANT_READ; inbound work can require output. A wait must not hide actionable local work or turn capacity exhaustion into unbounded buffering. |
| Actual managed/native I/O jobs, leases, timer/token/transport wake registrations and generation identities | Their storage must outlive actual use; a cancellation request is not completion. Registration/recheck must avoid missed wakes; stale/duplicate wakes cannot resume reused state. |
| A withdrawal/unregister/quiescence path and exactly-once result/release selection | At a genuine pause, retire temporary waits safely when replacing them, or retain them with a continuing owner. At final abandonment, unregister/join all uses before reclaiming storage; never free retry buffers on a mere timeout. |
| Bounded app queues and explicit producer/consumer execution ownership | Avoid blocking the only TLS scheduler while waiting for a body source or sink; avoid detached body jobs holding invalid borrows or work that cannot be joined. |

These obligations follow from private §§3–6 (ri-irw, comment 6903320800), public §§8–10 (ri-irw, comment 6903320873), networking/clock cleanup contracts, and the provider retry facts above. Concrete layouts, wake/lease ABI, registration primitives, scheduling fairness and checker evidence remain unestablished.

A **pause** must mean the same admitted logical operation remains alive with all its required state; it is not cancellation followed by a fresh operation. There are two materially different lifetime models:

- Keep the original exclusive call/activation alive while it internally coordinates all relevant work, preserving its borrows and pending native buffers.
- Design an explicit owner-retaining continuation or persistent owner that holds the continuation independently of the initiating call's stack. This requires defined ownership transfer/copying and dependencies; it cannot be achieved by quietly letting a borrowed caller buffer escape.

Neither model can resume after true admitted abandonment as though healthy. Dropping an in-flight continuation, canceling its owner routine, or terminating a controlled call still encounters ri-irw's poison/cleanup rules unless the semantics are separately designed and confirmed. Even when stopping transmission after an early final response is appropriate, pending accepted/native/lower bytes require an explicit disposition; no rollback, immediate physical release or healthy reuse follows.

## 6. Application-body scheduling is a second design problem

Suppose the TLS/network part can pause perfectly. If that owner then calls a blocking body producer while holding the only protocol-driving activation, it cannot read an early response until the producer returns. The reverse holds for a blocking consumer: it can prevent outgoing progress and control output. The source may even wait for the response that this owner has stopped reading. Network readiness alone cannot break that dependency.

Plausible ways to handle this, requiring explicit decisions:

- Body production/consumption participates in a designed resumable progress protocol. The scheduler must distinguish “no item yet” from EOF/error and have a notification path. This is additional capability, not supplied by ordinary canonical read/write.
- Run body producers/consumers in explicit owned/scoped tasks with bounded queues, while the single connection owner drives TLS/HTTP. Define admission/backpressure, exactly-once item/error ownership, end-of-body/trailers, queue closure, cancellation, and joining. Queue bounds include item sizes and outstanding work, not only item counts. No hidden detached tasks or borrowed-buffer escape.
- Keep a blocking-body convenience adapter, but explicitly establish its coordinator/worker arrangement or document its limitations; merely accepting an arbitrary blocking source does not establish the general streaming goal.

Bounded queues trade decoupling for finite buffering. Once full, the owner must stop admission, wait for capacity or take an explicitly designed failure/closure path. It cannot promise progress when the application refuses to consume or defines a cyclic dependency. Reserving control capacity and fairly scheduling producer, consumer and transport work can be evaluated, but are not a general liveness/security proof.

## 7. First-party comparisons: the missing mechanics are explicit elsewhere

These are **comparison facts only**, pinned examples rather than claims about the latest releases. No Rust traits, `Future`, `Poll`, pinning, cancellation safety or Go concurrent-connection guarantee is imported into Rill.

### Rust / Tokio / hyper

- Rust [1.90.0 `Poll`][rustpoll] defines Pending as requiring the current task to be scheduled for wakeup when progress is possible. It is a protocol with the executor, not a renamed timeout/error.
- Tokio [1.48.0 `AsyncRead`][tokioread] and [`AsyncWrite`][tokiowrite] explicitly provide pending-return/waker behavior; read/write/flush/shutdown have separate polling contracts. This is the mechanism that lets an owner stop trying one direction without blocking the calling thread. Those traits alone do not prove a particular TLS adapter's retry/cancellation properties.
- hyper [1.7.0 runtime I/O traits][hyperio] have their own poll-based read/write/flush/shutdown boundary (not simply the Tokio trait names). Its [HTTP/1 dispatcher `poll_loop`][hyperdispatch] tries read, write and flush in one serialized connection loop and limits looping to avoid starvation. `poll_write` separately polls outgoing body frames, and `poll_read` checks inbound body-consumer readiness. The [`http-body` 1.0.1 `Body`][hyperbody] protocol itself yields frames via `poll_frame`.

The instructive feature is **both transport and bodies can return control while retaining continuation/wake state**. Copying hyper's order of calls over Rill's current blocking/suspending `S` would not reproduce this behavior, and polling a TLS wrapper still must honor the selected provider's operation interleaving rules.

### Go

- Go [1.25.0 `net.Conn`][gonet] explicitly permits simultaneous method calls from multiple goroutines. That is a stronger/different concurrency contract than one exclusively borrowed Rill TLS owner.
- Go's [HTTP/1 transport][gotransport] launches separate `readLoop` and `writeLoop` goroutines, with coordination for expectations and responses. It is not evidence that two Rill operations can concurrently borrow `TlsStream` mutably or that blocking one connection-owner routine drives both directions.
- Go's [`ResponseController.EnableFullDuplex`][goduplex] explicitly changes HTTP/1 server handling: by default the server consumes unread request body before writing the response; the option permits interleaved/concurrent request reads and response writes. Its [server source][goserver] documents why write-all/read-later clients can deadlock against an early streaming response. This is an explicit behavioral tradeoff, not a suitable implicit restriction for the confirmed general Rill goal.

## 8. `100-continue`: soft scheduling timer versus request cancellation

[RFC 9110 §10.1.1][http] allows a client to proceed with content without having received 100 and discourages indefinite waiting. A bounded expectation wait can therefore mean **“body sending becomes eligible now”**, not “request failed.” Go's [transport][gotransport] illustrates this directly: `ExpectContinueTimeout` starts after request headers, and `waitForContinue` returns permission to send on timer expiry; connection closure is a different branch. This is comparison policy, not Rill's selected duration/default.

For a possible Rill design:

- Soft expiry changes HTTP scheduling state while the outstanding response read/TLS retry remains alive and healthy. It must not be implemented by wrapping the response read in an expiring controlled TLS operation and then reusing the poisoned stream. Nor does a timer magically release that operation's exclusive borrow.
- A final response, a 100, soft expiry, a true request token/deadline and clock failure need distinct transitions. Decide the ready-tie and early-final-body policy; “one event selected” must not hide already-accepted bytes. A late 100 cannot be mistaken for a final response.
- A true request cancellation/deadline remains terminal under existing admitted-operation rules: select once, retain real final prefixes, poison where required, and join cleanup. It is not a recoverable scheduling yield.
- Use existing checked `Clock`/`Instant`/`SignedDuration` semantics and the same domain for associated scheduling; ManualClock must control readings **and registrations**. Keep the request's absolute deadline separate from a relative soft interval and never restart its budget for TLS retries. Clock acquisition/read/arithmetic/registration failure cannot silently disable the policy or become permission to send.
- Do not implement soft expiry by reinterpreting `Timestamp`, native timer counters, OpenSSL certificate time or an arbitrary integer as an `Instant`. Certificate validation remains separate checked wall time. There is no new clock/provider interface or native timestamp conversion selected here. [Clock contract §§3–5](../clock-cancellation-contracts.md), public §§4,8 (ri-irw, comment 6903320873).

Expectation handling alone does not solve general streaming: after 100 or soft expiry, the client must still be able to notice a later final response while producing/sending its body.

## 9. Plausible design approaches, deliberately unselected

These approaches are not interchangeable fixes at the same layer. All retain one TLS engine/app-driving authority and still require a justified provider interleaving policy and bounded body scheduling.

### A. Explicit resumable single-owner progress with owned wait state

Design a capability through which the owner performs finite progress, retains incomplete operations, identifies unmet needs, and waits without terminating them. The HTTP coordinator can schedule local TLS work, response parsing, output admission and soft timers at genuine continuation boundaries.

**Tradeoffs:** most direct control over bounds and scheduling, potentially composable beyond HTTP; introduces a new generic transport/progress contract and concrete continuation/registration obligations. If TLS steps return but lower `S.read` still blocks, this approach is incomplete. A public “step” label does not guarantee finite crypto work, fair scheduling, safe cancellation or a valid native ABI. Source-backed interleaving and owned retry state are prerequisites, not afterthoughts.

### B. Persistent single-owner connection driver with bounded commands/data

Move/retain the whole connection in one explicit driver lifetime. Application tasks exchange owned body chunks and protocol requests/results with it. The driver owns retries across soft-timer and application waits; no second task drives SSL or borrows its raw owner.

**Tradeoffs:** centralizes retry identity and can isolate blocking body work; adds command/queue bounds, shutdown/fault delivery, lifetime, task joining and resource-limit obligations. **A persistent task alone is insufficient over current arbitrary `S`:** if it enters a blocking read, it still cannot service outgoing commands. It needs approach A's lower scheduling capability or an explicitly supported ownership-safe lower directional arrangement. TCP halves could serve such an arrangement at the ciphertext layer only if deliberately designed; no generic split or TLS escape is inferred. Queue endpoints would not be independent TLS application drivers.

### C. An HTTP-exchange-scoped coordinating owner

Keep orchestration scoped to an exchange/connection operation rather than exporting a reusable progress facility or long-lived command service. It internally coordinates body jobs, lower I/O and all TLS state, with explicit joined lifetimes.

**Tradeoffs:** can hide HTTP-specific complexity and avoid forcing application code to drive TLS, but does not eliminate the missing transport capability. Its eligibility requirements, continuation ownership, body scheduling and cleanup must still be specified; it cannot claim support for every canonical-I/O `S` without evidence. Making it merely “write body, flush, read response” would abandon the confirmed goal rather than implement this approach.

A and B can be combined, and C can use either internally. Choosing their ownership/lifetime boundary is a user design decision. No family declarations or arities are inferred from these architectural descriptions.

## 10. Remaining decisions and gates

### Decisions needed before an API recommendation

1. **Capability boundary:** reusable transport progress facility, explicit persistent connection owner, exchange-scoped coordinator, or a combination? Which eligible transports can actually supply its semantics, beyond existing canonical I/O?
2. **Continuation lifetime:** preserve the original scoped activation or transfer work into an explicit persistent owner? Which buffers/dependencies are copied or retained, and who owns abandonment/cleanup?
3. **Provider scheduling:** exact allowed transition/interleaving policy for pending read/write/control operations; mode choices; how bounded BIO output and pending write-WANT_WRITE cases are handled without forbidden SSL operations. The documented asymmetry is established; a complete scheduler/liveness argument is not.
4. **Body execution:** resumable producers/consumers or explicit tasks/adapters; queue and chunk limits, faults, cancellation, trailers and joins. No hidden guarantee for arbitrary blocking body callbacks.
5. **Early responses and reuse:** when to stop new body admission, disposition of already-accepted output, drain-versus-close policy and framing checks. Any persistence claim must separately survive HTTP framing, TLS state and actual cleanup.
6. **Soft expectation timing:** opt-in/default interval, eligibility start point, tie handling, clock failure and relationship to the request's unchanged absolute deadline. No timeout-as-yield.
7. **Capacity/fairness:** account for copies/internal buffering and bounded work; define legitimate consumer backpressure versus limit failure. A finite-memory service cannot promise progress despite an indefinitely stalled app consumer.

### Unknown implementation/platform gates, not design facts to assume

- Native **Linux/macOS × x86-64/AArch64**: actual lower notification/completion mechanism, race-free registration and retry-after-wake, nonblocking configuration, simultaneous lower direction ownership if any, local SIGPIPE policy, target ABI, native worker migration/error capture, safe unregister/join and stable addresses all need delivered compiler/runtime/provider evidence. No epoll/kqueue/poll API is selected or presumed available to source-level Rill.
- **WASI:** current contracts expose portable values/names but no live standard preview1 TLS/network provider. Neither memory BIOs nor host `poll_oneoff` in isolation establishes live connection creation, a selected TLS build, secure entropy, safe integration or usable generic duplex transport. Preview2/custom host capabilities would require a separate decision, not an automatic target upgrade.
- ri-dej's executor/activation/lease/wake machinery, ri-ei7's complete TLS delivery and provider-profile gaps remain open. Existing queue ceilings and native source inspection are not enforcement, target acceptance, cancellation safety, a full-duplex liveness proof or a security audit.

**Bottom line:** OpenSSL can return TLS continuation needs while a single owner retains its engine and retry buffers. That fact is valuable but does not supply a resumably duplex generic transport or schedulable application bodies. The confirmed general HTTP/1.1 goal needs those missing capabilities explicitly designed, with provider retry asymmetry preserved—not concurrent TLS drivers, timed cancellation used as yielding, raw escape, unlimited buffering, or a silently restricted streaming profile.

## Primary source index

OpenSSL links use the full selected revision; rendered `.pod` files are the first-party manual source shipped with that revision. Comparison sources use fixed first-party release tags, not unpinned `main`/`latest` documentation.

[version]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/VERSION.dat
[write]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/doc/man3/SSL_write.pod
[read]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/doc/man3/SSL_read.pod
[mode]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/doc/man3/SSL_CTX_set_mode.pod
[error]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/doc/man3/SSL_get_error.pod
[pending]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/doc/man3/SSL_pending.pod
[guide]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/doc/man7/ossl-guide-tls-client-non-block.pod
[threads]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/doc/man7/openssl-threads.pod
[mem]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/doc/man3/BIO_s_mem.pod
[memsource]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/crypto/bio/bss_mem.c
[pair]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/doc/man3/BIO_s_bio.pod
[records]: https://github.com/openssl/openssl/blob/45e844fa2a14ec92d146bd8f5778ac130b6625fb/ssl/record/rec_layer_s3.c
[http]: https://www.rfc-editor.org/rfc/rfc9110.html#section-10.1.1
[http11]: https://www.rfc-editor.org/rfc/rfc9112.html
[rustpoll]: https://github.com/rust-lang/rust/blob/1.90.0/library/core/src/task/poll.rs
[tokioread]: https://github.com/tokio-rs/tokio/blob/tokio-1.48.0/tokio/src/io/async_read.rs
[tokiowrite]: https://github.com/tokio-rs/tokio/blob/tokio-1.48.0/tokio/src/io/async_write.rs
[hyperio]: https://github.com/hyperium/hyper/blob/v1.7.0/src/rt/io.rs
[hyperdispatch]: https://github.com/hyperium/hyper/blob/v1.7.0/src/proto/h1/dispatch.rs
[hyperbody]: https://github.com/hyperium/http-body/blob/v1.0.1/http-body/src/lib.rs
[gonet]: https://github.com/golang/go/blob/go1.25.0/src/net/net.go
[gotransport]: https://github.com/golang/go/blob/go1.25.0/src/net/http/transport.go
[goduplex]: https://github.com/golang/go/blob/go1.25.0/src/net/http/responsecontroller.go
[goserver]: https://github.com/golang/go/blob/go1.25.0/src/net/http/server.go
