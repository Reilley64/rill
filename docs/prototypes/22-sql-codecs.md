# #40 SQL/JSON codec, control and error evidence

Read the source examples in [`prototypes/22-sql-codecs`](../../prototypes/22-sql-codecs).

**Human-confirmed shared-interface design, not implementation.** [The contract](../sql-contracts.md) and [ADR 0021](../adr/0021-partial-standard-sql-contract.md) integrate #33 and Q58-Q77, replacing the partial checkpoint. [Research](../research/0040-standard-sql-and-unique-errors.md) remains source input. [#43](https://github.com/Reilley64/rill/issues/43) separately gates actual embedded-provider support, not #40 design closure/RFC drafting. The [JSON](../json-contracts.md) and [time](../time-contracts.md) contracts define the value behavior used here.

[main.rill](../../prototypes/22-sql-codecs/main.rill) has **20 intended-valid expectations** and loads [domain.rill](../../prototypes/22-sql-codecs/domain.rill). [control.rill](../../prototypes/22-sql-codecs/control.rill) is a separate entry with **12 control/temporal expectations**, for **32 total**. All are hand-checked, not Rill/clock/token/database/native/WASI/protocol/security execution. The memory argument encoder, immediate counter and cell fixtures are **not SQL drivers** and do not prove PostgreSQL/SQLite integration or race/abortion behavior.

## What the source actually demonstrates

- Email is an exported application-owned nominal type with private text, constructed by a checked conversion. Its fixture validation is **only nonempty**, not a real email grammar/security validator.
- Into<Parameter> represents Email as one text slot. TryInto<Parameter> for EmailInput preserves a distinct validation error. A memory-only canonical bind implementation tests explicit bindInto/tryBind wiring and unchanged argument count after conversion failure; no automatic conversion discovery.
- The same domain module explicitly encodes/decodes JSON Value.String. Owned and borrowed JSON sources have separate implementations; the latter copies observed text. parse/stringify and Number semantics are unchanged. No automatic reflection/record mapping or mandatory show(Value).
- A generic `tryInto<Email>(&C)` states `tryInto(string,&C,E)`. Its cell and validation error envelopes remain typed. The domain module imports no concrete driver. NetworkCell supplies checked UTF-8 byte decoding; EmbeddedCell supplies tagged text/null/integer distinctions. They test the seam with different data representations, not actual wire/native APIs.
- Explicit nullable EmbeddedCell conversion maps NULL to None and checks non-null T normally; nonnullable Email fails and an integer does not become formatted text. No automatic infallible-to-fallible adapter.
- Typed parameter NULL and signed/unsigned width preservation; exact decimal spelling, invalid leading zero, bounded token rejection without exponent expansion.
- Synthetic unique errors distinguish unavailable details from a declared ordered composite key. Raw native diagnostics deliberately contain sensitive text; default wrapper rendering must omit it while the original typed payload remains available.
- Expression/partial-key descriptions remain explicit, not ordinary columns, and their SQL is omitted from default diagnostics. DeclaredSchema provenance is a fixture assertion, not evidence about a real server snapshot.

These tests do not check actual field/row/frame limits, metadata lookup, cardinality, transaction outcome, cleanup, caches, native ABI, cryptography, or a borrow checker. Their required independent cases are below. Default diagnostics are not a parseable stable-string protocol; source tests only check that selected sensitive substrings are absent, not an exact message.

## Control/temporal source coverage and limits

control.rill uses actual #33 types plus the explicitly approved std::time::validateInstantDomain consumer amendment. It checks immediate-fixture admission with uncontrolled/requested/expired/mismatched controls, copied token/clock state, all four temporal parameter variants, retained temporal constructor errors and exact-microsecond rejection. No source code reads private clock/Instant fields. The counter only completes immediately; it is not a pending-I/O cancellation or timer-race implementation. Setup may read a manual clock to construct an Instant; a successful source expectation cannot prove the domain validator avoided a read internally. That needs a private call-count harness, not a public clock-provider override.

## Two actual adapter obligations (implementation sketches only)

The following roles are **not implemented types/native hooks**. Fixed public operation declarations are in docs/sql-contracts.md; the earlier deadline-independent sketches are superseded. #33 is confirmed, not an unresolved excuse for alternate arities.

### PostgreSQL-like network adapter

- PgConnector/config -> PgSession/PgError; ordinary input type selects the relationship, not a driver name registry.
- Session/transaction target -> owning PgArguments, PgPrepared descriptor, PgRows cursor, PgRow owner and PgCell type. Bind encodes ordered parameter values with type evidence; unsupported native values remain typed errors.
- Cursor owns/retains protocol and encoded-storage obligations while exclusively depending on its session/descriptor. Each returned row copies transient buffers into independent ownership. Common text decoding checks encoding; decimal/integer paths never detour through f64.
- Native/server statement cache resources belong to bounded session state. An owning descriptor is not a pointer into that cache; eviction/session mismatch/schema change causes checked reprepare/error, not stale-handle use.
- 23505 is classified from actual native evidence. Qualified optional object fields can be preserved; a complete composite list is not assumed. Explicit enrichment can be impossible in an aborted transaction and later metadata is not failure-time proof.
- EOF/local row close/cancel request do not establish reuse. Observe actual protocol completion/transaction status or discard. Commit reply loss can mean Unknown even if a later cleanup succeeds.
- COPY/notifications/extra result modes/codecs remain concrete capabilities, not a lowest-common-denominator lie.

### SQLite-like embedded adapter

- A typed connector would produce an opaque session via a **separately specified native provider**. No SQLite C pointer/public extern declaration/OS-thread-pinned public object is supplied here.
- Owning arguments/native binding storage, compiled statement state, stepping/reset/finalization and session closure need exactly-once retention through completion/confirmed abortion under worker migration. Native memory/cancellation limits require real provider validation.
- Public owning descriptors cannot outlive a closed session while secretly owning dangling sqlite3_stmt pointers. The session owns bounded native preparation state; values copied into a row survive step/reset/finalize.
- Extended codes distinguish primary/unique/rowid rejection where available, but there is no universal native structured failing-key identity/column list. Native diagnostic strings must be copied before connection error state changes, not parsed into guaranteed columns.
- Native INTEGER/FLOAT/TEXT/BLOB/NULL does not prove exact SQL decimal support. An explicit text-storage codec is possible; silently converting Decimal to float/text or NULL is not.
- Provider capability/reset/transaction-state differences stay explicit. No generic cancellation/close/disposal wording proves rollback or connection reuse.

An external builder/generator can select backend SQL/placeholder spelling and call the same ordinary families. It must state its native/codec/row error and output relationships before monomorphization, not assume traits, reflection, dynamic error assertions or a package manager.

## Independent invalid/typing cases

1. Two ordinary bind/cell-access implementations with identical parameter patterns but different result/error types are collection-time duplicates; get<T> does not create another return-only exception.
2. A generic binding/decoding helper missing canonical into/tryInto/bind/cellAt evidence fails before monomorphization. A same-spelled unrelated operation is not evidence.
3. Expected E cannot force a less-specific conversion/default. A generic body cannot normalize an overridable fallback error and later accept a differently typed specialization silently.
4. An unrelated glue module cannot register a conversion between foreign domain and foreign driver endpoints merely by importing them or defining a local error. Use an endpoint owner, owned wrapper or explicit adapter callable.
5. Into does not supply TryInto; a tryBind call needs the fallible pair explicitly. Loaded-codec count/later statements do not establish a missing target.
6. One custom parameter conversion does not expand a record into columns, concatenate SQL, discover fields or translate an ordinary closure to a query AST.
7. Borrowing a domain string does not authorize a bound argument secretly retaining it; common arguments must satisfy noBorrow. Owned values containing nested borrows are not automatically borrow-free.
8. A descriptor/default row/argument type without required noBorrow evidence cannot satisfy the owning standard role. A borrowed row cell/text view still prevents destroying its actual row owner while live.
9. SQL NULL to plain T fails; missing/ambiguous columns are not NULL; no silent zero/empty substitute or integer formatting into a text-domain decoder.
10. ColumnRef.Name is exact and duplicate matches are ambiguous; no implicit SQL identifier folding or qualified-name parsing.
11. Returning a decoded view/error into a helper's destroyed local row/cursor fails. Scoped noBorrow/output evidence is required there, not on every direct operation over caller-owned storage.
12. Using session/transaction/descriptor while an exclusive dependent cursor is live, committing with live rows, copying an owner or using a consumed transaction rejects under ordinary ownership rules.
13. Managed transaction control cannot safely coexist with ad-hoc BEGIN/COMMIT/ROLLBACK/savepoints through ordinary execution. No universal text parser is promised; unexpected native state is discarded.
14. A public C pointer/thread-pinning/broad FFI hook or invented standard deadline/calendar type is not supplied by these examples.
15. Primitive numeric casts, SQL coercions and JSON numeric semantics are distinct; a codec must not silently inherit the wrong policy from a same-spelled conversion.

## Independent value/limit cases

| Case | Required behavior |
| --- | --- |
| Integer near/outside native range | Exact supported encoding or checked range/unsupported error; no float intermediate or wrapped count. |
| NaN/infinity unsupported by backend | Error, not SQL NULL or replacement value. |
| Decimal `-0.00e+03` | Preserve original token; no float conversion/expansion. |
| Decimal `01`, `+1`, whitespace, NaN/Infinity | Invalid syntax under the approved grammar. |
| Small valid token with enormous exponent | Bounded lexical validation; backend rejects safely when out of range, not huge expanded allocation. |
| Decimal token exceeds configured cap | Structured limit error before a borrowed copy/scan where length is already known. |
| Typed NULL text vs empty text | Different value state; native type hint available even without a value. |
| Failed bind/conversion | Logical builder list unchanged; prior successful slots retained; consumed input ownership still follows its operation. |
| Unsupported custom/native type | Explicit error or explicit extra driver codec; no forced closed common Parameter ceiling. |
| Field/row/frame/metadata/count cap reached | First applicable limit error before controlled allocation where possible; safe resynchronization or discard. |
| Backend hard limit lower than configured limit | Error, not truncation/narrowing or an authority to exceed native limits. |
| Allocation/native-engine work | Payload caps are not exact allocator/server/native-total-memory bounds or recoverable allocation guarantees. |

Defaults are exactly those confirmed in docs/sql-contracts.md: parameter/field 16 MiB, row/frame 64 MiB, metadata/decimal token 1 MiB, 4,096 columns and 32,767 parameters. They are accepted safety defaults, not benchmarked optimal settings. Native statement caches must also be bounded/documented; no infinite collection/cache is implied.

## Independent uniqueness/provenance/privacy cases

- PK/unique/rowid categories use trustworthy native classification. Generic integrity code or English message text is not sufficient. A UUID convention alone does not prove which key failed.
- Missing details stay Unavailable, not an empty complete list. Composite ordered names describe one tuple constraint; included non-key columns are excluded.
- Expressions are explicit parts; partial conditions/provenance remain visible. Default formatting omits their SQL, native/codec messages, raw statements and duplicate parameter values.
- Preserve database/schema/table/key name components and per-name provenance. Quoted names, commas/periods and same-named constraints across schemas defeat string splitting/unqualified lookup.
- A metadata lookup is explicit optional I/O. Aborted transaction, permission failure, stale cache, renamed/reused key and wrong database must not overwrite the original failure or manufacture certainty. Observed metadata does not recreate the failure snapshot.
- Native Err<E> stays a typed field, not automatically an erased Cause. Recursive cause formatting must not bypass wrapper redaction. Raw inspection/explicit application cause wrapping is deliberate and may expose secrets.
- Verify safe rendering with secret duplicate values, SQL literals, expressions/predicates, control characters in identifiers and arbitrary codec errors. A fixture substring check is not a redaction/security proof.

## Independent transactions/reuse/cleanup cases

| Sequence | Required result |
| --- | --- |
| Commit acknowledged; later cleanup fails | Error can still carry ConfirmedCommitted; never replay. Preserve cleanup error. |
| Commit reply lost | Unknown unless positive evidence establishes an outcome; socket close/late rollback is not proof of noncommit. |
| Finalization fails, cleanup fails | Retain both typed envelopes; do not replace the original unique/statement/commit failure. |
| Active transaction disposed | Mark unusable and safely discard, not queue rollback and advertise reuse. No acknowledged rollback promise. |
| Partially consumed rows abandoned | Positive completed cleanup evidence or discard; no detached work followed by reuse. |
| EOF/local cursor release | Not alone proof of protocol synchronization or clean session state. |
| Reuse candidate returned to external pool | Validate/reset under documented policy; candidate is not clean state or future liveness. |
| Driver needs discard but operation might have executed | Discard does not authorize replay. SELECT can have side effects too. |
| Several owning prepared descriptors alive | No permanent exclusive session loans; each execution still excludes conflicts. |
| Cache eviction/old session/schema change | Validate/reprepare or error; never dereference freed native state. |
| queryOne/queryOptional sees second row | Structured cardinality failure, finalize local cursor and preserve cleanup errors. No unlimited collection. |
| Intrinsic cancellation while native work retains inputs | Retain every owner/loan/native registration until completion or confirmed abortion. No Result cancellation/outcome delivered to stopped code; explicit token outcomes are separate. |
| Cleanup cannot abort / expiry selected | Keep owners and wait indefinitely if necessary; integrated #33 control does not promise a hard return bound or prove native abortion. |

## Independent integrated-control and temporal cases

| Case | Required behavior |
| --- | --- |
| Deadline clock differs; token already requested | DomainMismatch retained as ClockFailure before token outcome; no clock reading/timer required by domain-only validation, no new SQL submitted. |
| Matching clock is terminally failed; token already requested | Domain compatibility can still validate; token preflight selects Cancelled without reading the failed timeline. No claim that validation tests clock health. |
| No deadline / never-cancelled token | No clock lookup/read; ordinary intrinsic cancellation still applies. |
| Explicitly copy control then request source | Both copies observe the same request; copying does not freeze/reset the flag or retarget the absolute Instant. |
| connect/begin succeeded; later original token requested | Does not asynchronously cancel the idle session/transaction. Each later operation receives its own explicit control. |
| Active next with buffered data and already-requested control | May terminate before another row is selected; observation is relative to admitted query, not latest fetch. Previously returned rows remain valid. |
| Terminal next, then token requested/clock fails | Still None; no second failure or retroactive outcome replacement. |
| Completion/expiry/request ready after admission | No guaranteed bias; preserve chosen outcome through cleanup, not a late return-time clock reclassification. |
| Control failure, mandatory native release blocked | Keep all owners/loans; use noncancelled internal cleanup and wait for completion/confirmed abortion. No hard return deadline. |
| Commit request not submitted | Does not prove prior transaction work rolled back; consuming guard cleanup and explicit outcome still apply. |
| Commit acknowledged after expiry selected | Failure can still report ConfirmedCommitted; never retry or replace selected control failure with success. |
| Failed/interrupted pool reset | Discard, not silent reconnect/replay; successful reset is documented baseline, not future liveness. |
| nanoseconds not divisible by 1,000 for microsecond backend | Precision error with original TimeError where used, not truncation. |
| Native invalid date/24:00/leap second/infinity/out-of-range epoch | Checked error, not repair/normalization. |
| Civil DateTime -> instant; Zoned -> instant; calendar interval -> elapsed | No implicit zone/context/unit/rules loss. Use explicit/schema-native mappings; common subset does not promise faithful zone/interval storage. |
| withDeadline around query creation | R stays nested Result; outer expiry is not a token request or rollback proof. A returned cursor needs its own explicit control for future advances. |
| Row-helper iteration error borrows local cursor | noBorrow(NE) required before retaining it across close; caller-owned-row get keeps valid borrowed outputs/errors. |
| Different query/next/close or finalization/cleanup errors | Preserve independent types and both envelopes; no expected-error-selected implementation or implicit unification. |

## Remaining implementation and validation work

The explicitly confirmed design permits #40 closure. Native embedded-provider work is tracked in #43 and is not fulfilled by a fixture; no general FFI/pinning or working adapter is claimed. #28 owes checker/candidate/dependency/redaction/limit/cleanup/race formalization and #30 real native/Wasm retention/pointer/scheduling validation. Merge the narrow validateInstantDomain amendment into #33's separate clock artifacts and reconcile JSON/time/random numbering without overwriting drafts. Any unworkable mechanism returns to map #1; design closure is not an accepted RFC or runtime/provider/security proof.
