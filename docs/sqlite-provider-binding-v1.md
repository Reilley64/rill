# SQLite provider binding/wire specification v1. Design only

This specifies the closed native boundary selected in [ri-3id's contract](sqlite-provider-contracts.md). **No compiler artifact consumer, generated product bindings, C shim, runtime hooks or Rill driver implement this specification.** The C declarations and Python codec/state model are design/reference artifacts, not a supported ABI binary or proof. Revisions require explicit contract/profile review and version agreement; no permanent binary compatibility is promised.

The ordinary Rill-facing external driver implements std::sql; its internal provider functions/tokens are module-private. This document is not a public native-call facility. Binding records may provide only the approved internal implementations in the canonical defining module. Imported aliases never create another identity/authority. Public bodyless family declarations remain declarations, not native implementations by themselves.

## 1. Data-only binding artifact

The [JSON schema](sqlite-provider-binding-v1.schema.json) describes artifact version 1, contract `rill.sqlite.native/1`. A real build must resolve and verify the following, not merely parse them:

- Canonical defining module identity and source digest; exact fixed function/type role identities; ownership rules fixed by this contract.
- Official engine source/archive/sourceID/digest and exact build profile; generated API namespace, actual exported definitions and references.
- One concrete native target with actual OS/deployment/libc/compiler/linker identity and complete compiler/linker argument vectors. Cross-field checks require Linux/glibc or macOS/libSystem, x86_64/aarch64 and 64-bit pointers. No unclaimed minimum version becomes support from being written in JSON.
- Header/schema/profile digests, wire/C ABI version, approved entrypoint set and versions of runtime temporary leases, wake tokens, atomic adoption and blocking bootstrap/finalization.
- Effective budgets plus native hard caps. Overrides must remain checked finite host-representable values, not authorize weaker semantic rules or another function/module/plugin interface.

Reject unknown fields, versions and native roles. The artifact has no shell hooks, C source expressions, dynamic symbols or arbitrary type-property assertions. Exact source, generated header, signatures and selected outputs must agree with the approved contract. The compiler, not the document, owns permission to lower it. Resolve bindings/resourceTypes canonical identities to the eight fixed function and four resource roles. Verify signatures against the header and module authority. Never treat arbitrary identity strings as implementations. Compiler flags must preserve the specified numeric/ABI behavior (no fast-math or other weakening); recording flags does not approve them. Metadata/header/model tests cannot establish that missing consumer or implementation.

The [declaration-only C header](../experiments/sqlite-provider/contract.h) fixes shim roles. SQLite engine symbols use a generated private namespace; only the fixed shim exports are intended bridge symbols. Only one compatible provider contract/profile is admitted to a program initially; incompatible duplicates fail rather than resolving through a runtime registry.

## 2. Physical call and runtime ownership

C ABI arguments use fixed-width scalar integers, byte-span pointers/lengths and a runtime-private native context. The ABI permits no by-value aggregates, variadic calls, Rill record/enum/Err/string/list layouts, closures or cross-allocator ownership. The target C compiler/header and generated lowering must agree; no `reinterpret` layout assumption. Temporary input/output pointers remain valid exclusively/read-only as appropriate until the synchronous call returns, under the validated runtime lease. The C shim must copy before retaining asynchronous data. Pointer values never appear in frames/tokens.

The runtime context and fixed native notify function are the deliberate long-lived exceptions: retained native runtime infrastructure, never a raw Rill activation or application callback. Notify takes an opaque wake ID/generation and schedules continuation; it cannot run user code, inspect Rill storage or release its own lifetime prematurely. Register before dispatch; ready state persists; duplicate/revoked generation notifications are harmless. Context, worker code and leases survive all notifications and native calls.

service_start/stop are runtime-only bootstrap/finalization on the required blocking lane, not calls on ordinary Rill scheduler workers. Starting allocation/threads does not authorize opening SQLite before the control/admission policy permits it. Global SQLite configuration is established once before first engine initialization; unrelated host instances remain untouched. Shutdown cannot race other live service/library use. The other shim entries are short admission/flag/status/copy/handoff operations; actual SQLite work runs on the native executor. No exception/longjmp/OS-thread kill crosses the boundary.

### Shim statuses

`OK=0`, `PENDING=1`, `READY=2`, `CAPACITY=3`, `CLOSED=4`, `STALE=5`, `FAULT=6`, `BOOTSTRAP_FAILURE=7` are bridge statuses, **not SQLite codes**. Invalid caller token/state can return STALE/CLOSED where the contract permits stale capabilities; malformed trusted frames/inconsistent ABI output is FAULT. An ordinary SQLite failure is a READY result containing its real error record. A bootstrap error frame is optional, with explicit unavailable capture when no diagnostic is available. Never invent a native SQLite failure.

observe returns a stable result byte length only after the job is quiescent (native work, callback/interrupt references and required per-job state observation complete). copy_result requires that generation/length and sufficient capacity; it copies exactly the immutable frame, never truncates data to fit. Register/adopt new owners under the runtime intrinsic-delivery deferral region. adopt verifies the offered resource and registered runtime adoption receipt. release_job is legal only when quiescent and all resource offers adopted or disposed; it is not an implicit unsafe free. Unclaimed resources need a cleanup command and join first. A release/protocol error cannot orphan the job.

request_stop records a persistent per-job stop request and schedules generation-checked interrupt work on reserved capacity; it does not itself claim abortion or synchronously run SQLite on a scheduler worker. On a completed/revoked generation it is stale/no-op, not a request on the next operation. Native stop is not a Rill token flag. The Rill coordinator selects the typed control outcome, joins required completion, and preserves the logical operation observation across jobs.

## 3. Common binary envelope

All integer fields are unsigned little-endian unless explicitly signed. Decode via checked byte access, not alignment-dependent C casts. No padding, pointers, native-sized longs, sentinel-terminated strings or concatenated frames. The header is exactly 24 bytes:

| Offset | Width | Meaning |
| ---: | ---: | --- |
| 0 | 4 | ASCII `RSQL` |
| 4 | 2 | Wire version = 1 |
| 6 | 2 | Kind: 1 ServiceConfig, 2 Request, 3 Result, 4 BootstrapDiagnostic |
| 8 | 4 | Flags = 0 in v1 |
| 12 | 4 | Reserved = 0 |
| 16 | 8 | Exact body byte count |

Require exact total `24 + body_count == span_length`, checked against host size and configured frame budget before controlled allocation. Reject unknown kind/version/flags/reserved/trailing bytes. `bytes` means u64 byte count followed by that many bytes; UTF-8 fields require strict validation, diagnostic/raw text bytes do not silently become strings. List counts use u32 and checked aggregate bounds. Zero/absent fields are allowed only where explicitly specified. This is an in-process trusted interface with validation, not a network protocol or a general RPC engine. It makes no checksum/authentication claim.

The configuration body contains sixteen u64 fields in this order: ordinaryWorkers, interruptWorkers, ordinaryQueueEntries, ordinaryQueueBytes, cleanupQueueEntries, requestBytes, sqlBytes, cacheEntries, cacheSqlBytes, diagnosticRecords, parameterBytes, fieldBytes, rowBytes, metadataBytes, columns, parameters.

The defaults are 4,1,64,67108864,64,67108864,1048576,64,4194304,128,16777216,16777216,67108864,1048576,4096,32767. Workers, queue capacities and diagnosticRecords must be positive. Cache budgets may be zero to disable retention. Check host conversion and total arithmetic.

Require enough fixed protocol overhead and private fixed cleanup commands irrespective of application budget; reject impossible configuration, never make cleanup impossible by lowering a native SQL limit below required internal commands. Backend effective caps are min(requested, actual native) and must be reported. No metadata document changes the fixed source/profile security policy.

## 4. Capabilities and native values

A capability token is exactly **32 bytes**: service(u64), slot(u64), generation(u64), kind(u32), reserved(u32=0). The kinds are Connection=1, Statement=2, Job=3 and TransactionParticipation=4. Nonzero service/slot/generation are required for a live token. The all-zero token is absent only in specified request/result fields. A transaction token tracks logical explicit-transaction responsibility/epoch; it is not a SQLite pointer or proof that the native transaction remains active.

Private Rill constructors and move-only ownership plus native validation implement the capability role; token bytes are not cryptographic authentication. Check kind, service, owner graph, generation, current state and transaction association before indexing/access. Never wrap/reuse a live generation. User SQL/blob data containing token-looking bytes grants no authority. Parent close can invalidate idle child tokens without waiting for their descriptors to be dropped.

Native value encoding:

| Tag u8 | Payload |
| ---: | --- |
| 0 NULL | none; shared typed-NULL compatibility checked by driver before this lowering |
| 1 INTEGER | signed two's-complement i64, 8 bytes |
| 2 REAL | IEEE binary64 bits, 8 bytes; reject input NaN before SQLite bind |
| 3 TEXT | bytes; validated UTF-8 for binding, copied native raw bytes for checked decoding |
| 4 BLOB | bytes |

Unknown tags fail. Empty bytes remain non-NULL. f32/wider integer/common temporal/decimal mapping is explicit driver work, not more wire variants. Do not widen integer range through float, canonicalize native TEXT invalidity, or include a function pointer. Field/parameter/frame budgets apply to encoded lengths and controlled copies; native SQLite allocation is not globally bounded by these checks.

## 5. Closed request body

The prefix contains opcode(u16), reserved(u16=0), target(token32). The remainder below has no implicit padding. Prepare/Start/Advance/Finish carry transactionContext(token32) as listed, zero only for unguarded session work. Finalize uses cleanup authority and the statement's tracked association rather than an additional transaction-context field. The native graph rejects a missing/mismatched/lost transaction context rather than allowing an implicit transaction through an inactive guard.

| Opcode | Operation / target | Remaining fields and behavior |
| ---: | --- | --- |
| 1 | Open / absent | location(u8 Memory=0/File=1), mode(u8 ReadOnly=0/ReadWriteExisting=1/ReadWriteCreate=2; Memory uses2), foreignKeys(u8 bool), reserved(u8=0), path(bytes; empty for Memory). Absolute UTF-8 non-NUL file path; fixed FULLMUTEX/PRIVATECACHE/no-URI flags. |
| 2 | Prepare / Connection | transactionContext, sql(bytes). Validate one whole statement under policy before any step. Return owning metadata and a statement offer; temporary tail-validation statements are cleaned without execution. |
| 3 | Start / Statement | transactionContext, count(u32), native values. Validate exact maximum-slot count, apply complete arguments, first step, copy actual row/metadata/state. No partial public bind-list update. |
| 4 | Advance / Statement | transactionContext. Require active ROW state, perform the next step and copy owning row/state data. |
| 5 | Finish / Statement | transactionContext. Reset then clear bindings, retaining failures. Cache eligibility requires success. An error never permits reuse; finalize as needed. |
| 6 | Finalize / Statement | none. Consume a valid native statement even when reporting an error. Invalidate it in the graph exactly once. |
| 7 | Begin / Connection | isolation(u8 Default=0/Serializable=1), readOnly(u8 bool), reserved(u16=0). Protected setup and fixed BEGIN DEFERRED; failed setup restored/discarded. Successful transaction participation is an ownership offer. |
| 8 | Commit / TransactionParticipation | none. Fixed COMMIT, preserve acknowledged outcome, required rollback/restoration/discard on failure; no replay. |
| 9 | Rollback / TransactionParticipation | none. Fixed ROLLBACK or handling of positively known already-rolled-back participation; restore/discard and consume guard responsibility. |
| 10 | Close / Connection | none. Stop admission, join work/interrupts, dispose tracked children and complete actual close. Zombie acceptance is not completion. |
| 11 | State / Connection | none. Copy only fixed native state/effective-limit/profile observations under lane. |
| 12 | BuildInfo / absent | none. Exact linked source/profile/module identity and actual limits, not arbitrary native configuration access. |
| 13 | DiscardOffer / completed Job | offer(token32). Clean up the unadopted offered connection/statement/transaction responsibility. Join before releasing the original job. |

Finish/finalize/rollback/close/discard and required restoration have cleanup admission, not ordinary capacity failure. No arbitrary SQL can select cleanup privilege. Begin/commit aren't replayed or reclassified as arbitrary cleanup SQL. A queued cancelled finalizer that never dispatched still leaves its wrapper responsible for guard-consuming cleanup; dispatch acknowledgement alone never releases that responsibility.

Worker command handlers keep internal SQL authority scoped to the fixed command. No internal prepared statement can be returned/cached as an application statement. For commit/rollback, revoke external interruption before mandatory cleanup and preserve both native finalization and cleanup records; snapshots distinguish operation acknowledgement from later cleanup. The Rill wrapper remains responsible for selected control errors and for cleanup after a job cancelled before entry. Commands may consist of several native calls but no command carries an arbitrary native-call list or privileged SQL string.

## 6. Result body and bounded diagnostics

The result prefix contains echoed opcode(u16), outcome(u8 Success=0/Failure=1/NotDispatched=2), step(u8 None=0/Row=1/Done=2), offer(token32, zero if none), state snapshot, diagnostic aggregate, then operation-specific payload. NotDispatched applies to that job, not necessarily the enclosing SQL operation. The native companion does not select DeadlineExceeded/ClockFailure for a guessed native clock.

The state snapshot contains flags(u32), reserved(u32=0), transactionEpoch(u64), transactionOutcome(u8 NotApplicable=0/ConfirmedCommitted=1/ConfirmedRolledBack=2/Unknown=3), targetDisposition(u8 Live=0/Consumed=1/DiscardOnly=2), reserved(u16=0), nativeTxnState(u8 Unknown=0/None=1/Read=2/Write=3), reserved3(bytes3=zero).

The flags are bit0 autocommit-known, bit1 autocommit-value (only if known), bit2 query-only-known, bit3 query-only-value (only if known). Reject other bits. The snapshot is an observation with history constraints, not proof of commit/reuse. Automatic transaction loss keeps logical responsibility for cleanup but prevents ordinary commands under its token.

The diagnostic aggregate contains hasPrimary(u8 bool), an optional record, cleanupCount(u32), cleanup records, omittedCleanup(u64). Retain the primary before cleanup.

Each record contains these fields: origin(u8 SQLite=0/Provider=1/Host=2), phase(u16), code(i32), extended(i32), hostDomain(u8 None=0/Errno=1/Pthread=2), hasOffset(u8 bool), offset(u64 only if present), capture(u8 Complete=0/Truncated=1/Unavailable=2), message(bytes).

SQLite records have hostDomain=None. Provider record code is a contract-local reason, extended=0, hostDomain=None and no native offset; never a fabricated SQLite code. Host records preserve an actually available errno/pthread code with extended=0, the corresponding domain and no SQL offset; unavailable host diagnostics use Provider HostFailure with explicit unavailable capture, not a guessed errno.

Phase IDs 1 through 17 are, in order: Bootstrap, Admission, Prepare, Bind, Step, Copy, State, Reset, Clear, Finalize, Begin, Commit, Rollback, Restore, Close, Adoption, Compatibility.

Provider reason codes are 1 Capacity, 2 InvalidSqlSource, 3 Policy, 4 State, 5 Unsupported, 6 Limit, 7 Compatibility, 8 HostFailure, 9 DataDecode, 10 StoppedBeforeEntry; unknown phase/reason IDs fail v1 decoding. Native code pairs/offsets are original observations, not inferred from message text.

Cap total records at the configured default of 128 and the message/metadata budget. Unavailable capture has an empty message, and the omitted count is explicit. Continue resource cleanup regardless. Runtime-created Err envelopes are preserved after this capture boundary; no rewriting to fit. Normal packet/data-limit failures differ from inconsistent ABI output. Native UTF-8 data decoding failure is not automatically a bridge fault.

A BootstrapDiagnostic envelope contains this diagnostic aggregate only. Status BOOTSTRAP_FAILURE does not itself invent a SQLite/native code; preserve actual host initialization failures in their own domain when available.

Operation payloads:

- Open/State/BuildInfo: version/sourceID UTF-8 bytes and profile digest bytes (exactly 32 raw SHA3-256 bytes), followed by sixteen effective config/limit u64 values in configuration order. BuildInfo may have zero connection-specific observations explicitly marked unknown in snapshot; it cannot invent a measured cap from a desired default.
- Prepare: parameterCount(u32), that many optional names (present u8 + UTF-8 bytes), columnCount(u32), column names (UTF-8 bytes). NULL native name cannot distinguish anonymous slot from unused gap. Metadata budgets apply. Offer kind Statement.
- Start/Advance ROW: columnCount(u32), actual UTF-8 column names then native values in order. Own current metadata even after reprepare; do not use stale descriptor metadata. DONE has no row payload.
- Begin success: offer kind TransactionParticipation plus state; no extra payload.
- Finish/Finalize/Commit/Rollback/Close/DiscardOffer: no extra payload; all relevant consumption/outcome/cleanup info is in state/diagnostics.
- Failure/NotDispatched: no success-specific payload; no live newly created resource may be lost merely because the result is a failure. A resource still needing disposition remains graph/job-owned until cleanup or explicit ownership handoff.

A decoder requires full consumption and rejects trailing/overlong/count-inconsistent packets. Names, native cells, error bytes and codes have different semantics; do not run a generic unsafe deserializer or interpret arbitrary object layouts. ABI header/schema revision must change when these encodings/roles change.

## 7. Required state/event cases, not implemented machinery

The abstract job phases are reserved/queued, then claimed/running, completing, quiescent, result-copied, offers-adopted-or-disposed and released, in that order. Removal before claim is a coordinated transition, not freeing after a flag check. Stop requests are scoped to the current generation and can become no-op after revocation. Before quiescence, mandatory cleanup uses never-cancelled internal context and joins outstanding calls. Result readiness/adoption are distinct; C result token copying does not create an owner. Do not release while there is an unclaimed live resource, active interrupt or future notification.

Rill-side control preflight and whole-operation observation remain in the main contract. Required cases are a claim winning versus queue removal; a stop before native entry; ROW buffered then a token request; Native DONE versus deadline readiness; commit acknowledged while a control error is already selected; caller intrinsic stop during copy/adoption; a stale wake after slot recycling; and saturated ordinary/cleanup queues. Each is a separate case. A finalize error consumes the native statement, but a reset error does not. No model-only pass clears these real implementation obligations.

[Prototype model/tests](../experiments/sqlite-provider/README.md) intentionally implement only primitive envelope/value/token parsing, JSON contract checks and selected abstract transitions. They do not implement these command handlers, a full result parser, generated marshalling, worker dispatch, runtime ownership or platform C ABI. There is no claim of complete executable conformance coverage for this document.
