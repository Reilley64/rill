# External SQLite native-provider contract. #43

The user explicitly confirmed Q1-Q58 and the consolidated recording plan. The design is recorded; implementation/integration validation is outstanding. [#43](https://github.com/Reilley64/rill/issues/43) stays OPEN. This is not a working provider, an accepted RFC, or a safety/security/target-support claim. The checkout has no implementation of the required compiler/build/runtime bridge. [#40](https://github.com/Reilley64/rill/issues/40)'s [shared SQL interface](sql-contracts.md) remains design-complete; [#30](https://github.com/Reilley64/rill/issues/30) retains actual backend/lease/scheduling/reclamation gates. Its newer Wasm dispatcher direction is not superseded by this native-only profile.

[ADR 0024](adr/0024-external-sqlite-native-provider.md) records placement/trust tradeoffs. [Primary-source evidence](research/0043-sqlite-native-provider.md), [binding/wire specification](sqlite-provider-binding-v1.md), [metadata schema](sqlite-provider-binding-v1.schema.json), and [prototype 26 inventory](../experiments/sqlite-provider/README.md) are coordinated design inputs. Native reference results and Python protocol/state models are explicitly separate from product integration.

## 1. Placement and authority

The SQLite driver, provider, engine dependency, release/update policy and backend codecs belong to an **external package**, not `std::sqlite`. The external driver implements the canonical `std::sql::driver` families, re-exported unchanged by std::sql. Concrete pooling, query ASTs/dialects, ORMs and migrations stay external. The provider is not a second shared SQL interface.

The runtime/build mechanism must explicitly admit this one narrowly approved provider contract. It is absent today. Versioned, data-only JSON metadata identifies the canonical defining module, exact closed entry points, ownership contracts, source/profile/target/ABI identity and runtime-hook versions. Generated bindings and a fixed C shim implement it. Neither a bodyless Rill family declaration nor an arbitrary metadata document authorizes native execution. No new public `extern` syntax, general FFI, arbitrary native package registration, dynamic symbol lookup, runtime driver registry, reflection or public pointer/thread-pinning escape is approved.

The native companion, generated code, symbol generator and runtime integration code are trusted code. Rill cannot prove their bodies safe. Metadata cannot assert arbitrary Rill properties; only the fixed resource and marshalling roles in the approved contract may be trusted. Ordinary module identity, privacy, family/orphan rules, generic selected-output evidence and source dependencies remain unchanged. Implementation review and real validation are required in addition to shape/version checks.

## 2. Source, build and library isolation

Initial reference pin: **SQLite 3.54.1**, official archive `https://sqlite.org/2026/sqlite-amalgamation-3540100.zip`.

- Archive bytes: 2,990,966; SHA3-256 `92281c86e2957f9b9305890f9ac04204caa7b677cc868dc326d942689df28e30`.
- SQLITE_SOURCE_ID: `2026-10-16 14:45:20 555e07ef33f69a0957d24d8e555b702464a131d6f1607f4c3c1e4551e8e6ca76`.
- sqlite3.c SHA3-256: `e2b869d3003b33c942b7dd7063604fbc53553e1b66609570df39b9b7b558ee81`.

Downloaded identities were checked against upstream published values. They identify source, not its security or suitability. Historical 3.50.4 source findings are not silently attributed to this pin. There is no automatic upgrade/host fallback. The external provider maintainer reviews applicable notices, source/profile changes and revalidation. An applicable unresolved defect returns the pin/profile decision to the human before support claims.

Required initial profile:

| Concern | Requirement |
| --- | --- |
| Threading | `SQLITE_THREADSAFE=1`; establish SERIALIZED before initialization, FULLMUTEX and PRIVATECACHE connections; verify actual initialization/open procedure, not just sqlite3_threadsafe(). |
| Loading/cache | `SQLITE_OMIT_LOAD_EXTENSION`, `SQLITE_OMIT_SHARED_CACHE`. No load/enable API, custom collations, Rill-defined SQL functions or user VFS callbacks. |
| Schema | `SQLITE_TRUSTED_SCHEMA=0`; verify trusted-schema OFF, defensive ON. Foreign keys ON unless an explicit creation option requests OFF. |
| Modules | Do not enable FTS3/4/5, RTree/Geopoly, dbpage/dbstat or additional registered extensions. Inventory core functions, JSON and built-in pragma virtual tables. Do not remove all virtual-table support merely to omit optional modules. |
| Mappings | `SQLITE_MAX_MMAP_SIZE=0` removes database-file mmap. WAL shared-memory/index mapping is separate and still needs validation. |
| Batch atomic | Leave `SQLITE_ENABLE_BATCH_ATOMIC_WRITE` **undefined**. Defining it as 0 still enables the presence-tested feature. |
| Contention | No busy handler/timeout/retry initially. SQLite BUSY and provider queue saturation are distinct failures. |
| Files | Native Unix VFS, FULL synchronous setting on file-backed connections, no implicit WAL conversion or synchronous downgrade. |

3.54.1 predates the November 2026 batch-atomic fallback journal-sync fix. Excluding that feature excludes that particular affected path, not all durability defects. The specific listed source switches did not require parser regeneration in source inspection; arbitrary OMIT combinations are not thereby approved. Actual macros, generated code, linked identity and behavior must be checked per build.

Static linking alone is not isolation. Generate a complete private namespace for SQLite's externally linked definitions/references, including globals, from the pinned build's symbols. Combine this with hidden visibility and a fixed shim export allowlist. Audit unexpected exports/unprefixed references, actual identity and coexistence with unrelated host SQLite. Do not change that host instance's global configuration. Unknown or incomplete isolation fails validation, with no silent substitution. Toolchain mechanisms differ on ELF and Mach-O. The host reference's object-derived prefix map is an experiment, not an implemented product generator or an exhaustive loader proof.

## 3. Targets and explicit input

Initial validation is intended to cover Linux/glibc x86-64 and AArch64, and macOS x86-64 and AArch64. Each actual profile records OS/libc/deployment versions, compiler/linker, ABI, flags and runtime identity. No minimum platform version or universal Linux/macOS support is claimed from one host. Musl, 32-bit, Windows and WASI require separate profiles; Rill's broader target commitments are not dropped. SQLite's browser/Wasm bundle is not a WASI promise. No new WASI imports, native descriptor escape or ambient rights are authorized here.

Location is explicit, either an isolated in-memory database or a file-backed database. File-backed mode is explicit ReadOnly / ReadWriteExisting / ReadWriteCreate; creation is never inferred. Initial file input must be an absolute Path representable as valid UTF-8, with no NUL. Reject relative/unrepresentable paths; callers can explicitly resolve/convert before connecting. No replacement bytes, hidden working-directory resolution, magic connection strings, URI/VFS/cache options or empty-filename temporary database. Disable URI interpretation at initialization and opening. An absolute file named `:memory:` stays a file; only the explicit Memory variant selects SQLite memory mode. No named/shared memory mode initially.

This is not canonicalization or symlink containment. Native Unix VFS file/locking/journal/WAL/temp/random/time behavior is inside the trusted implementation, not routed automatically through std::fs or its future capability machinery. It may consult native environment variables for temporary-file behavior. Document/audit that behavior; do not claim environment-free execution or a sandbox. Read-only open may encounter sidecar/recovery permissions and fail. ATTACH is not a way to bypass explicit location policy. In-memory journaling differs from file-backed behavior.

New file-backed databases use the selected SQLite rollback-journal default; opening an existing supported WAL database does not convert it. FULL synchronization follows the actual VFS/OS/filesystem/hardware assumptions, not a universal power-loss guarantee. Explicit journal-mode changes would require a separately audited typed option; none is added to version 1. No native engine function signature promises hard return latency.

## 4. Managed SQL policy and whole-statement validation

Install the provider-owned native authorizer **before prepare** and keep it during step/reprepare. Deny application BEGIN/COMMIT/ROLLBACK/END, savepoints, ATTACH/DETACH and all unlisted PRAGMAs using SQLITE_DENY, not IGNORE. Native action codes, not keyword filters, enforce categories. Internal transaction/configuration work has dedicated closed commands with fixed templates/values; no public privileged-SQL flag or arbitrary raw internal SQL. Internal statements/authority cannot enter the application cache or leak into application reprepare.

Only these PRAGMA forms are initially allowed:

| Names | Argument rule |
| --- | --- |
| table_list, table_info, table_xinfo, index_list, index_info, index_xinfo, foreign_key_list | Object/filter argument as defined by the pinned built-in; absent arguments may return no/all metadata, not settings mutation. |
| database_list | No argument. |
| foreign_keys, query_only, trusted_schema, schema_version, journal_mode, synchronous, compile_options | No argument only. |

Match built-in names case-insensitively, safely handle NULL arguments, preserve identifier contents and distinguish NULL from an explicit value. Schema selection is limited to the connection's main/temp schemas; no attachments. Apply policy when table-valued pragmas prepare underlying SQL during evaluation. The selected source's hidden argument shapes differ: table_list has only a name filter; object inspectors have object then optional schema; journal_mode/synchronous have schema; listed other status tables have no hidden arguments. Do not invent a schema parameter for every form. Compile-option diagnostics do not prove all effective settings.

These are inspections, not no-effect operations. They may reset internal counters, invoke VFS controls, load/resolve schema/views/virtual tables, lock, open temp or perform recovery. database_list explicitly returns paths. Native schema resolution can occur before PRAGMA authorization; selected-source view-column derivation temporarily suppresses some underlying authorization. This is NOT a row/column ACL, complete nested authorization or arbitrary-SQL/database-file sandbox. Required absent extensions, fixed trusted callbacks and exact source/VFS profile are part of the boundary. Do not repair malformed/unsupported schema or present incomplete metadata as complete.

Application SQL is valid UTF-8 source, at most its configured byte cap, with embedded NUL rejected before native submission. Bound text/blob NUL is different and retains explicit length. Establish exactly one statement using native parsing and validated tails, with no step of a valid prefix before discovering another statement. Empty/comment-only source is rejected. No semicolon counting; triggers/quoted literals need native parsing. Preparation itself is not a dry run: the restricted policy must prevent prohibited prepare-time settings changes, but rejected input may still have native effects (temp/schema/recovery/allocation). Do not infer NotSubmitted after native preparation. No parser proof is supplied by the reference probes; full adversarial tail/policy interaction remains an integration gate.

## 5. Resource graph, jobs and adoption

Each connection owns/tracks its native resource graph: statements, active transaction participation and in-flight work. Rill-visible provider tokens have private constructors, are noncopying capabilities, never pointers. Validate service/kind/slot/generation/state and connection/transaction association before native access. Reused slots cannot alias a live/stale generation; exhaustion fails rather than wrapping. A stale/cross-connection token returns checked state failure. Encoding token bytes is not ownership transfer.

The provider serializes complete native sequences, including bind/start, type inspection and owning row copy, immediate diagnostic capture and required state observation. SQLite FULLMUTEX is only a second defense. Sequential calls can migrate between workers where validated; native code/callbacks cannot retain an invalidated stack/buffer or assume permanent OS-thread affinity. One active high-level session operation, cursor and transaction loans remain #40's ordinary Rill dependencies.

The native job/result owns newly created connection/statement/transaction participation until explicit atomic adoption into a registered Rill owner. Register and commit the handoff within the privileged runtime adoption region; defer intrinsic delivery through it. There must be no gap or independent double-owner interval. Failure/cancellation before adoption disposes the unclaimed offer with required joins. Native result storage stays immutable until copied/released; resource offers must be adopted or safely disposed before freeing their job/result. Joined work, interrupts, callback state and wake notifications must be quiescent before their owners go away.

Finalize invalidates a valid statement token once even if SQLite reports failure. Reset does not destroy the statement and does not clear bindings. Finish must reset and clear; any error makes it ineligible for cache, and required finalization/errors still follow. Never retry a consumed finalizer. A resource graph close blocks new admission, joins active work/interrupts and closes tracked children; it does not wait merely for idle caller tokens to be dropped. Those tokens become unusable. Close success requires required SQLite destruction to have completed, not close_v2 zombie acceptance. It does not certify every hidden OS/VFS failure or durability assumption.

Valid API failures remain typed errors. Unexpected surviving resources after an apparently empty graph, malformed ABI output, lost ownership or illegal bridge transitions are trusted-implementation faults. Stop affected admission/reuse and retain known owners while safe cleanup is possible. Follow internal fatal-fault policy if ownership cannot be accounted for. Do not free resources with uncertain ownership, label infinite quarantine as successful close, or retry normally. A corrupt database reported as SQLITE_CORRUPT is not automatically a bridge fault. No native exception/longjmp/async OS-thread cancellation crosses the ABI. Fatal termination has no cleanup guarantee.

## 6. Service, executor and control

A runtime-owned service owns provider initialization/executors. Configuration is explicit through the approved future build/program settings, not a Rill module initializer/global binding/environment worker count. No public global SQLite shutdown/reconfiguration. Library-global initialization is controlled before first use and never shut down while other relevant service/resources remain. Normal runtime shutdown stops admission, safely resolves queues, joins native work/interrupts/notifications, closes the graph and only then destroys the service. Bootstrap/finalization must also use the required blocking-runtime mechanism.

Potentially blocking SQLite operations run on the private bounded executor; Rill callers suspend. Interruption has separately reserved capacity, never simply queued behind the blocked ordinary call it must interrupt. Callbacks are fixed provider-owned native code, not user Rill/functions/collations/VFS code. SQL triggers and built-in functions are not excluded; they do not become inspectable Rill closures.

The Rill wrapper is the one control coordinator. Consume/copy #40 SqlControl and use #33 actual clock/token operations and public validateInstantDomain. Validate basic/domain inputs before token and clock checks, then admit work. No private clock inspection, native guessed epoch/timeout or generic withCancellation. Native callback control is an owned stop flag associated with a specific operation ticket; native work does not reinterpret Rill Clock/Instant/Err layouts.

Track the logical operation across multiple native jobs. If preparation ran, a later undispatched bind job does not make that entire query NotSubmitted. Active next checks retained control even with buffered rows; terminal next stays None. Session/idle transaction does not inherit connect/begin control. Post-admission completion/control races have no guaranteed winner. Preserve selected success/error; a late native commit acknowledgement can establish outcome without replacing a chosen control error.

A request ticket/interrupt lease is scoped to its generation. Revoke delivery and join in-flight interrupt calls before reuse, close or noncancelled cleanup. Idle interrupt is not a latch; near-complete work can succeed; progress callbacks need not run promptly. Cover the pre-native-entry gap with persistent operation control and actual coordinator observations, not an assertion that interrupt always aborts. Native VFS work may remain blocked indefinitely. Intrinsic stopping stays outside Result, does not set token flags and joins required cleanup. Explicit cancellation/expiry/clock failures return only after required release.

Native completion signals a pre-registered generation-checked runtime wake token, never a raw suspended frame or user callback. Ready state survives completion before await; stale/duplicate wakes cannot target a recycled waiter. Runtime ownership remains until notifications are impossible. The runtime hook schedules continuation rather than executing user code on the native worker. These lease/wake/adoption/blocking mechanisms are NOT implemented/proven by metadata or Python state models.

Ordinary queue saturation is a distinct provider Capacity failure, not SQLITE_BUSY, and is not automatically retried. Accepted queued requests retain deadline/token; removal before worker claim must be acknowledged before returning NotSubmitted. Dispatch winning requires conservative observation and completion join. Mandatory cleanup uses bounded reserved admission with uncancelled waiting and priority between completed native calls. It cannot fail merely because ordinary admission is saturated, leak, or advertise reuse. It cannot preempt blocked C; unbounded wait remains possible. No unbounded native cleanup queue replaces the bounded ordinary queue.

## 7. Budgets, values and preparation cache

The initial defaults are configurable and are not measured optimal settings:

| Budget | Default |
| --- | ---: |
| Ordinary native workers / reserved interrupt workers | 4 / 1 |
| Ordinary queued requests / aggregate queued payload | 64 / 64 MiB |
| Reserved cleanup descriptors | 64 |
| Native request frame / application SQL bytes | 64 MiB / 1 MiB |
| Cache entries / retained SQL per connection | 64 / 4 MiB |
| Native diagnostic records per logical operation | 128 |

Retain #40's 16 MiB parameter/field, 64 MiB row/frame, 1 MiB metadata/decimal token, 4,096-column and 32,767-parameter caps. Actual backend limits can be lower. The reference build reports **2,000 columns and 32,766 parameter slots**; report/enforce these lower caps, not the shared maxima. Compute encoded total/aggregate sizes with checked arithmetic and reserve queue budget before controlled payload allocation. Headers/metadata count toward frame budgets; a maximal payload is not permission to exceed its frame. Cleanup descriptors contain bounded internal metadata/references, not arbitrary SQL/duplicate large payloads. Waiting frames, caller-owned arguments, total SQLite memory/stack/native allocations and peak reprepare cost are not thereby globally bounded.

Use native transient copies initially, with no public zero-copy views/custom destructors. Preserve NULL versus empty BLOB/TEXT (a null native pointer can mean NULL even at zero length); explicit lengths preserve embedded NUL data. Logical bind validates/constructs before appending one slot, leaving earlier arguments unchanged on failure. Failed application of an owned argument list may leave partial native bindings. Reset/clear or finalize before reuse. Never expose partial native bindings as a successful argument list.

Native values are NULL/i64 INTEGER/binary64 REAL/TEXT/BLOB. Smaller/wider signed/unsigned integers bind only if exactly representable as i64; f32 widens exactly. Reject NaN before double binding, which otherwise becomes NULL. Do not silently float/stringify larger integers, assume DECIMAL affinity is exact decimal storage, or infer time/date storage from column spelling. Decimal/civil/timestamp values require explicit schema codecs or fail; original checked TimeError survives in #40's Temporal wrapper. Infinity/other floats require faithful native representation and checked extraction, not a blanket precision promise about SQL.

Read original column storage class before coercing accessors; copy data before next/reset/finalize or conversion invalidates it. Validate UTF-8, ranges, exact numeric extraction and NULL Option semantics; no replacement text, implicit text-number parse or silent precision loss. Native TEXT bytes can be invalid even though application SQL string is valid. SQLite affinity, casts, triggers, functions and storage transformations are outside the driver's bind-time fidelity claim. No ORM mapping or JSON numeric rounding is imported.

Argument zero maps to native slot one. Repeated names share a slot; ?5 alone requires five supplied slots, including unused positions. Count is maximum index, not textual occurrences. No implicit NULLs or SQL placeholder rewrite. Clear bindings between uses; reset alone retains them.

Owning preparation descriptors contain SQL/owned metadata, not native pointers or permanent exclusive session borrows. The native cache belongs to the session and has both entry and retained-SQL budgets. Only successfully reset/cleared entries qualify. Validate association/schema/policy and current read-only restriction during reuse and reprepare. Hash alone is not full SQL identity. Reprepare can change column metadata; copy actual metadata with owning rows. Cache limits do not bound every native byte. Eviction/close consumes tracked statements exactly once and preserves failures.

## 8. Transactions and reuse

Initial standard isolation accepts BackendDefault and Serializable. It rejects explicit ReadUncommitted/ReadCommitted/RepeatableRead rather than silently substituting. Private caches do not implement dirty reads by enabling a PRAGMA. Use BEGIN DEFERRED; no implicit IMMEDIATE/EXCLUSIVE or separate savepoint API. Later lock/snapshot-upgrade failures are ordinary failures, never automatic restarts.

readOnly=true establishes protected query-only for the guard lifetime, including cached writes; stmt_readonly is not a security classifier. Only dedicated internal finalization/config commands get necessary authority. Verify setup/restoration, clean up failed setup, and discard if baseline cannot be established. readOnly=false does not override read-only file opening. Read-only is a database restriction, not pure execution/containment; COMMIT/checkpoint/internal maintenance may still occur.

Observe native autocommit/txn_state and operation history under exclusive access. Automatic rollback makes the logical guard inactive for ordinary work; subsequent SQL cannot accidentally start an implicit transaction through it. Consume/finalize it to restore state. Known rollback requires positive operation/history evidence; otherwise the outcome is Unknown. Autocommit alone is not commit history, absence of all unfinished statements, or reuse proof.

Commit/rollback consume guards through #40's independent phase errors/outcome. Native successful commit confirms commit, not universal physical durability. A failed commit that leaves a transaction active is not replayed; required rollback/discard runs with external control ignored. Preserve original finalization and cleanup errors, including cleanup-only failure after confirmed outcome. A transaction participation/resource offer abandoned before Rill adoption also needs rollback/restoration or discard.

The initial driver does not implement optional whole-session std::sql::reset. Native statement reset remains mandatory and is different. Temp objects, counters and session history are not claimed fresh. Generic pools without a justified baseline/validation reset must discard between borrowers. Normal sequential owner use/cache remains valid. No reconnect/replay/tenant-isolation promise is hidden in reuseDisposition.

## 9. Typed errors and diagnostic ownership

Concrete provider errors distinguish actual captured SQLite failures from legitimate provider-local capacity/policy/input/state/compatibility failures. Native codes exist only for native failures. Generic std helpers cannot manufacture arbitrary E; the concrete external provider can construct its own owned local variants. Use standard DbError categories where meaningful, otherwise preserve concrete Driver(E); do not mislabel SQL-source/queue failures using an unrelated standard limit kind or fake SQLITE_BUSY. This clarifies #40 without adding standard families/arity or forcing every backend failure into one new universal enum.

Snapshot code/extended code/available byte offset/owned diagnostic bytes under the same serialized boundary before invalidation. Real UNIQUE/PRIMARYKEY/ROWID extended codes permit UniqueViolation classification; stock SQLite supplies no guaranteed structured failing key/columns. Retain unavailable/provenance semantics; no English message classifier or silent catalog I/O. Explicit enrichment is not a failure-time schema proof.

Capture has KnownComplete / Truncated / Unavailable status. Bound text without splitting it into silently repaired UTF-8; diagnostic bytes can be explicitly decoded by the caller. Retain at most 128 records per logical operation within metadata budget, primary first, explicit omitted-cleanup count. Continue cleanup after capture capacity ends. Native snapshot/aggregation enforces limits before Rill error-envelope creation; never rewrite/drop an already-created Err to fit. This limits diagnostics, not cleanup or resource accounting. Codes/phase/state remain honest when text is absent.

Copied provider diagnostics have no borrowed native buffers; Rill envelopes retain origin/cause unchanged through driver/helpers. Default rendering emits safe category/code/observation only. It does not emit raw SQL, parameter values, paths, native/codec messages or recursive typed-field/cause dumps. Raw details require explicit inspection. Invalid stored text is a data Decode failure; malformed ABI tags/length relationships from trusted code are a bridge defect. No fake native error is required for preflight clock/token/local errors.

## 10. Validation and completion boundary

The design inputs now exist, but the required mechanisms do not. #43 remains OPEN, with design recorded and implementation/integration validation outstanding. #40 stays closed and #28 can use this contract for RFC drafting without asserting native support. #30 backend validation and #43 provider/runtime gates are not cleared.

The [reference report](../experiments/sqlite-provider/reference/results.md) has 98 passing countable checks on macOS 27.0.1 arm64/Apple Clang 21, plus source and object prefix/hidden-reference audits. 276 global sqlite* symbols were renamed; basic separately compiled host/private coexistence passed. The host's SINGLETHREAD reconfiguration returned SQLITE_ERROR, so that discrimination remains untested. This is not the pinned LLVM23 Rill runtime, complete loader isolation, or proof of the proposed artifact consumer. Native caps and NaN/prepare/reset behavior are facts, not product implementations of the accepted restrictions.

The [protocol model](../experiments/sqlite-provider/protocol_model.py) and tests check only the explicit framing/value/token model and selected abstract state transitions. They are not generated bindings, a real C ABI, Rill-native cancellation or ownership checking. Refer to the inventory for exact executed commands/counts; no aggregate chart or single pass count substitutes for the per-layer matrix.

| Layer / target | State |
| --- | --- |
| Official source identity / selected-source audit | Reference facts were checked. This is not security clearance. |
| Standalone selected engine, one macOS arm64 host | Reference checks PASS as reported, with explicitly unavailable host configuration case. |
| Python framing/state/metadata model | Reference model only; see exact report. |
| Product artifact consumer / generated marshalling / C shim | UNIMPLEMENTED / NOT RUN. |
| Runtime leases, wake epochs, adoption, queue/interrupt/cleanup races | UNIMPLEMENTED / NOT RUN. |
| Rill driver/copy/transaction/cache/limits/control end-to-end | UNIMPLEMENTED / NOT RUN. |
| Linux/glibc x86-64 / AArch64; macOS x86-64 | NOT RUN. |
| Actual macOS AArch64 product/runtime integration | This is NOT RUN despite standalone host passes. |
| WASI, relocation/termination/VFS/rights | Separate profile; NOT RUN, no new imports/support. |
| Crash/fault/sanitizer/security/power-loss evidence | NOT RUN; no certification. |

Required adversarial cases include all failure boundaries of open/bind/step/copy/adopt/reset/finalize/close; cancellation before/after claim/entry/row selection/completion/adoption; idle/stale interrupts; callback unregister; recycled wake/resource generations; saturating ordinary/cleanup/interrupt queues; failed-clock/domain ordering; buffered-row/terminal stability; distinct phase errors; malformed UTF-8/frames/lengths; schema/reprepare/virtual-table policy; close BUSY/zombies; native defaults/limits; and positive/unknown transaction outcomes. Any mechanism that cannot express these guarantees returns to map #1, not a silent weakening or assumed proof.

### Decision trace

Q1-4 placement/targets/source: sections 1-3. Q5-9 trust/serialization/interruption/copy/callbacks: 1,5-6. Q10-14 resource/phase/diagnostic/compatibility: 2,5,9. Q15-20 SQL/VFS/slot/location/value/source policy: 3-4,7. Q21-26 executor/isolation/build/settings: 2,6. Q27-32 transaction/cache/reset: 7-8. Q33-39 metadata/admission/service/budgets/pin/cleanup/journaling: 2-3,6-7. Q40-45 selected features/inspection/namespace/ABI/coordinator/targets: 2-6 and binding specification. Q46-51 adoption/errors/JSON/path/unwind/framing: 1,3,5,9 and binding specification. Q52-58 closed commands/wakes/faults/cleanup/diagnostics/reference/open gate: 5-10. No implementation/RFC acceptance or commit/push was authorized by design confirmation.
