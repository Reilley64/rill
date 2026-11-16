# #43 external SQLite provider. Design and reference evidence

**Design confirmed; #43 OPEN for implementation/integration validation.** [Contract](../../docs/sqlite-provider-contracts.md), [ADR 0024](../../docs/adr/0024-external-sqlite-native-provider.md), [binding/wire specification](../../docs/sqlite-provider-binding-v1.md), [JSON schema](../../docs/sqlite-provider-binding-v1.schema.json) and [research](../../docs/research/0043-sqlite-native-provider.md). No std::sqlite, compiler/runtime hook, artifact consumer, generated product binding or external Rill SQLite driver is implemented here. #40 stays design-complete and #30's actual backend gates remain.

## Executed evidence. Deliberately separate layers

| Evidence | Result | What it does NOT establish |
| --- | --- | --- |
| Official archive/source identities | Matched upstream published SHA3/sourceID; see research. | Security or supply-chain certification. |
| Standalone selected SQLite C fixture | **98 checks passed**, plus object prefix/visibility audits on macOS arm64. | Product ABI/ownership/queue/control/runtime/driver support. |
| Object-derived namespace audit | 276 renamed global sqlite* definitions; no raw SQLite refs in private objects. | Complete product export/loader isolation or other toolchains. |
| Separate host/private engine coexistence | Basic live coexistence/query after private shutdown passed. | Host SINGLETHREAD reconfiguration: it returned SQLITE_ERROR and remains untested. |
| Python protocol/state/schema subset | **32 unittest methods passed**, including one declaration-only C-header syntax check. | Full binary result parser, command implementation, JSON Schema conformance engine, C ABI/Rill execution or concurrent ownership proof. |
| Other native cells, actual macOS product integration, WASI | NOT RUN. | No support inferred from the host reference. |

No aggregate score turns these into Rill tests. Existing conversion/streaming/filesystem/SQL prototypes retain 14/15/16/32 hand-checked expectations respectively; this directory adds **zero Rill-executed expectations**.

## Reproduce

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s experiments/sqlite-provider -p 'test_*.py' -v
python3 experiments/sqlite-provider/reference/run.py --report /tmp/rill-sqlite-provider-results.md
```

Reference runner downloads/verifies the pinned archive (or accepts `--archive PATH`, still hash checked), uses disposable /tmp storage and explicit timeouts, then writes [reference/results.md](reference/results.md). `--report PATH` writes a separate report. It is Darwin-toolchain-specific and explicitly rejects other platforms rather than claiming coverage. No engine/archive/generated object/binary is checked in. Native C fixture's trusted setup bypass is a test convenience, not the product internal-authority API.

[contract.h](contract.h) contains declarations ONLY. The unittest compiler invocation uses `-fsyntax-only`, not linking/implementation/ABI validation. [protocol_model.py](protocol_model.py) implements primitive 24-byte envelope, 32-byte token and five native-value encodings, a deliberately limited checker for the schema's used subset, and sequential job/wake/admission sketches. [test_protocol.py](test_protocol.py) uses synthetic MODEL-ONLY metadata; shape acceptance does not resolve a real canonical module, native library, authority, lease, ABI or build. No general-purpose JSON Schema validator is supplied. The model does not implement full requests/results/diagnostics, worker threads, privilege enforcement, native resource disposal or runtime adoption.

## Intended positive/rejected/adversarial inventory

| Boundary | Required case / result | Current evidence |
| --- | --- | --- |
| Envelope | Bad magic/version/kind/flags/reserved, truncated/overlong/overflowing length, unknown tags fail; no allocation from unchecked lengths. | Primitive model only. |
| Tokens | Wrong kind/service/connection/transaction/generation, stale/recycled slot; no raw pointer access, no live generation wrap. | Encoding subset/model; product registry NOT RUN. |
| Values | i64 extrema, f32 exact widening, wider integer range checks, NaN rejection, empty BLOB/TEXT vs NULL, embedded NUL data, raw invalid TEXT bytes, no getter/affinity precision shortcut. | Native observation + primitive model; product checked codecs NOT RUN. |
| Parameters | ?5 has maximum slot5, repeated names share, missing args not NULL-filled, reset bindings cleared. | Native references; full driver cardinality/ownership NOT RUN. |
| SQL | Native whole-tail validation, NUL SQL rejected, semicolons in trigger/string not counted, no step of valid prefix. | Required; native parser/product-tail policy NOT RUN. |
| Prepare effects | Unauthorized PRAGMA denied before its mutation; explicit-temp/schema/recovery effects not called pure; authorizer maintained through TVF/reprepare. | Selected source + bounded native cases; exhaustive view/VFS interactions NOT RUN. |
| Authority | App transaction/savepoint/attach/detach and unlisted/assignment PRAGMAs denied; internal fixed commands cannot leak privilege/cache into app mode. | Native reference callback subset; product gate NOT RUN. |
| Path | Absolute strict UTF-8/no NUL, no URI/magic/temp/relative coercion; explicit :memory: file remains a file; mode and lower rights honored. | Product NOT RUN, VFS source facts only. |
| Create/adopt | Failed-open handle owned; success resource offered; intrinsic stop before/during copy/registration/adoption/return; no gap/double owner. | Failed-open native references + sequential offer model; actual atomic adoption NOT RUN. |
| Queue | Count/aggregate-byte reservation, capacity distinct from SQLITE_BUSY; queued cancellation wins removal or worker claim wins and requires join. | Primitive budget/job model only. |
| Interrupt | Idle no-latch, before-entry gap, late request, old ticket vs next job, simultaneous completion, close/cleanup while interrupt pending, flag coalescing. | Native finite progress/synchronous interrupt + model, cross-thread protocol NOT RUN. |
| Wake | Completion before await, duplicates, unregister/recycle, runtime teardown; no Rill frame pointer retained. | Sequential model only; runtime NOT RUN. |
| Cleanup | Ordinary/cleanup queues full, reserved interrupt capacity, callback unregister, held resource/error owners, no early reuse/free, blocked VFS may never finish. | State model/native lifetime facts, actual executor NOT RUN. |
| Statement | ROW reset can complete/fail; finalize error consumes, no second finalize; partial binding reset/clear/finalize; actual metadata after reprepare. | Native subset; RETURNING completion/fault coverage NOT RUN. |
| Cache | Count/SQL budgets; successful reset+clear only, old schema/policy/connection, eviction+close failures; descriptor has no pointer. | Contract only; product NOT RUN. |
| Transactions | Deferred/default+serializable only, cached write blocked under query_only, automatic rollback invalidates guard, consume guard even queued-cancelled commit; late known commit outcome vs selected control error. | Native query_only/autocommit subset; Rill guard/outcome integration NOT RUN. |
| Close | BUSY retains valid connection, close_v2 zombie not complete, all tracked children/discarded offers dealt with; unaccounted ownership is internal fault. | Native references/model only; no allocator leak theorem. |
| Diagnostics | Copy before invalidation, original Err after construction, safe display, exact codes/offsets, byte prefix status and omitted cleanup count, continue cleanup after128/budget. | Native copy/code reference; aggregation/display/product NOT RUN. |
| Isolation | Prefix definitions+refs/globals, hidden exports, unrelated host SQLite lifecycle/state, target/version/header mismatch rejected. | One-host object audit + synthetic metadata; product ABI/toolchain NOT RUN. |
| Fatal behavior | No exceptions/longjmp/OS-thread killing across ABI, no cleanup promise after fatal faults. | Requirement only. |
| Support | Exact four native cells; separate WASI VFS/rights/memory/dispatcher profile; no source-only certification. | Only standalone host reference executed. |

## Next implementation gates (not replacement tickets)

1. Implement approved module/build admission, exact target C ABI generation, source/module/profile checks and actual export isolation.
2. Implement validated runtime temporary leases, bounded blocking executor/bootstrap/cleanup, epoch wakes and atomic adoption; preserve #30's live frame/native/Wasm reclamation obligations.
3. Implement the closed companion state machine and external Rill driver, copied values/errors, authorizer/internal-command boundary, resource graph, transaction/cache/limits/control semantics.
4. Execute this inventory through those real mechanisms on each claimed target; add fault/sanitizer/crash/security/unsupported evidence without calling timeouts abortion proofs.

Keep #43 OPEN until the missing implementation/integration validation is dealt with. Record honest negative/unavailable/untested rows. Return unworkable mechanisms to map #1; no silent general FFI, pinning, target drop, driver delivery, RFC acceptance or cleared #30 gate.
