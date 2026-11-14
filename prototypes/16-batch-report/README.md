# Realistic Rill batch-report prototype (#25)

**Question:** does the current design remain coherent in a normal, non-toy workflow, and what ownership/copying friction becomes visible?

**Status:** explicitly accepted by the user as #25 prototype evidence for the RFCs. The two Rill files are **298 physical lines combined**, including **14 independent top-level expectations**. This is intended-valid source inspected by hand, not a working compiler, executed application/test suite, concurrency proof or backend validation. No JSON/HTTP/CLI API is invented. The confirmed #23 catalogue/ADR 0015 are this prototype's repository inputs, not a library implementation. Publishing these design artifacts does not establish compiler or backend support.

- [main.rill](main.rill): native file/UTF-8 work, scoped concurrency, notifications, mutex metrics, error context and entry startup.
- [records.rill](records.rill): application grammar, owned records, aggregation, generic rendering and pure expectations.
- Inputs: [north.tsv](north.tsv) (LF, final record unterminated), [south.tsv](south.tsv) (CRLF, final newline).
- [expected-report.txt](expected-report.txt): hand-computed successful output, not captured Rill output.
- Published primary evidence: [complete source snapshot/findings](https://github.com/Reilley64/rill/issues/25#issuecomment-6873277247) and [explicitly accepted verdict](https://github.com/Reilley64/rill/issues/25#issuecomment-6873344066).
- Authority: [library contracts](../../docs/standard-library-contracts.md), [ADR 0012](../../docs/adr/0012-enums-replace-unions-and-literal-overloads.md), [ADR 0015](../../docs/adr/0015-language-required-library-contracts.md), and the confirmed [#29 errors](https://github.com/Reilley64/rill/issues/29) / [#22 stack policy](https://github.com/Reilley64/rill/issues/22). #28 formalizes/checks the design; #30 gates backend mechanisms; #31 separately designs JSON.

## Workload and application policies

The entry driver reads two configured data files concurrently, parses labeled integer quantities, widens individual i32 quantities to i64, aggregates equal labels, and writes a numbered text report. A second scoped branch observes best-effort completion notifications or an idle timer. The report is derived only from actual loaded rows, never from notification delivery.

The intended future run would use this directory as the working directory and `main.rill` as the entry file. **There is no runnable Rill command today.** Module paths are entry-directory-relative source identities; OS data paths are ordinary configured file paths, not source-import paths. No command-line argument or process-exit API is assumed. At the command boundary an application failure is formatted, then explicitly panics; all scoped work has already been joined before that failure returns.

The deliberately small application grammar is not CSV/JSON:

- One nonempty UTF-8 label, exactly one ASCII TAB, then text accepted by strict `parseNumber<i32>`.
- Labels cannot contain TAB/CR/LF; quote characters have no escaping/quoting role. Other Unicode/control characters are ordinary data, not sanitized terminal output.
- No trimming, comments, embedded newline escaping, extra fields or inferred numeric widths.
- Accept LF and CRLF line endings, a final unterminated nonempty record, and an entirely empty file.
- Reject blank records, empty fields, lone CR, additional tabs and invalid/out-of-range numeric text.
- Report rows follow **first-seen input order**: left-file rows, then right-file rows. An application-owned label list supplies this order; Map iteration is never treated as ordered or sorted.
- The final numbered TOTAL line is a report footer, not an input-record parser feature. A data label may itself be TOTAL; position distinguishes the footer.
- Individual out-of-i32 inputs are recoverable parse errors. i64 aggregate/counter arithmetic remains checked and can panic on overflow; no unsupported recoverable tryAdd or saturation policy is invented.
- Filesystem output has the standard create/truncate/partial-write semantics, not atomic replacement or rollback.

For the supplied data, coffee = 3 - 1 = 2, tea = 2 + 4 = 6, cocoa = 5, five records, two files, grand total 13. Successful report bytes are those in expected-report.txt. Progress-log ordering and whether a timer/closure wins are intentionally not a golden output contract.

## Borrow and copy pressure

Reproducible source-site counts, excluding comments, string/char/template literals, expectation statements and the test-only sampleReport/numberHasCause/rowContext helpers:

- **60 `&` tokens**, including both reference-type annotations and explicit borrowing expressions.
- **12 of those are `&mut` sites**, not twelve additional ampersands or measured runtime loans.
- **8 explicit `copy(...)` sites**: four conditional error-path copies, the progress-path copy, the sender-owner copy, parser label materialization, and first-seen-order key ownership.
- **3 explicit `dispose(...)` sites**: two early guard releases and disposal of a recovered, undelivered progress message. These are ownership operations, not manual heap freeing.

A successful supplied-data run would reach eleven explicit copy invocations **by inspection**: five label materializations, three first-seen key copies, two progress-path copies and one Sender owner-count copy. Four conditional error-path source sites do not run on that path. This is not profiler output, allocated-byte accounting, an inferred-constraint count or an executable ownership diagnostic measurement. Compiler-generated interpolation reads and implicit scalar/record copies are not lexical copy sites.

The main costs are visible rather than hidden:

1. Parsing must copy a label that outlives its file buffer. Returning slices would change the result's lifetime contract, not eliminate the dependency.
2. Stable first-seen order alongside an unordered map needs another owned key representation. No sort/ordered-map API is assumed.
3. addExisting returns a non-reference bool, ending the lookup/element loan before a possible insertion. This avoids relying on branch-sensitive borrow precision inside a getMut/insert match; it is **not** a claim the final checker must reject every shorter spelling.
4. The generic renderBlock preserves F plus callableMut evidence. A single mutable formatter is explicitly reborrowed for body and footer, retaining its numbering state without cloning the closure.
5. Both guard releases are explicit because a guard otherwise lasts through scope exit, not merely its last mention. No guard is held while sending a potentially blocking notification or writing a file.

## Feature ledger

Use these categories literally: **exercised** means represented in intended-valid source; **inspection-only** means reasoned about without execution; **known gate** is unresolved implementation/checker validation; **not exercised** is not claimed by this program.

| Decision | Evidence | Category |
| --- | --- | --- |
| #2 functional-first syntax | Named/arrow functions, explicit returns, templates, immutable/mutable bindings, no methods/traits | exercised |
| #7 ownership; #15 mutation | Owning Bytes/strings/rows, logical views, partial-field moves, exclusive map/formatter access, explicit copies | exercised; checking is a known gate |
| #9/#18 primitive types | Explicit i32/i64/u64, base-range widening, Unicode byte offsets, checked arithmetic, strict numeric parsing | exercised |
| #10/#20 polymorphism | Canonical display family, variant specialization after match, generic callableMut requirement without a group alias | exercised |
| #11/#29 errors | Message-free error enums, three typed causal layers, unchanged forwarding, lazy wrapping and guarded cleanup | exercised/inspection-only |
| #12 pipes | Owned line list piped into collection join | exercised |
| #17 modules | Public records/functions, canonical display extensions, two file modules, separate startup/expectation execution | exercised/inspection-only |
| #19/#21 nominal data | Named RawRow/Row/Counts/Totals/Summary constructors, enum variants, exhaustive nested matches | exercised |
| #8/#16/#23 concurrency | Tuple all inside join, bounded Progress channel, sender copy, selected receive/closure/timer, failed-send recovery | exercised; scheduling/cleanup are known gates |
| #16 guards | Two child-safe metric updates through Mutex/LockGuard, explicit releases before sends/output | exercised; native protocol is a known gate |
| #23 collections | Consuming and borrowed map, hash lookup/update, first-seen list, scalar sum, repeated mutable callback | exercised |
| #23 I/O | Whole-file bytes, borrowed UTF-8 validation, typed filesystem errors and borrowed output byte view | exercised; native lifetime is a known gate |
| #22 standard stack policy | Iterative input/aggregation, finite user display dispatch, standard default cleanup/rendering obligations | inspection-only; #30 remains a known gate |
| #27 expectations | Twelve records-module and two main-module expectations with no startup-let capture | exercised, not executed |
| #3–#6/#13 research/backend choices | Ownership/runtime/LLVM traceability inputs, not source features that a program can prove | known gate / context |
| Remaining feature APIs | Shared/spawn/stop/sized spawn, every work helper, custom iterator extension, explicit record rebuilding, actual proper-tail transfer | not exercised here; earlier focused fixtures remain separate evidence |
| #31 JSON | No dependency; no parser/serializer or numeric-precision behavior is guessed | not exercised |

Using a decision does not mean testing every API it mentions. No claim of complete ownership/concurrency/stack coverage follows from this ledger.

## Error and cleanup inspection

### Invalid numeric record

For a file containing `tea<TAB>nope`, the semantic chain is:

```text
AppError.Rows { path }                 display: invalid records in <path>
  origin: wrapping operator at main.rill:54
caused by RowError.Number { line }     display: invalid amount on line <line>
  origin: wrapping operator at records.rill:53
caused by ParseError.InvalidSyntax     standard display text
  origin: its original construction inside the parseNumber operation
```

These labels explain **typed nodes**, not reflected runtime output. formatError uses each supplied display implementation, preserved construction origins and the complete explicit cause chain. Exact separators, paths/line/column presentation and standard ParseError text are not frozen here. The original ParseError source-map coordinates are deliberately symbolic; no unimplemented library source location is fabricated. parseRows' postfix ?, all's selected-failure forwarding, and run's loaded? add **no locations**.

- parseNumber constructs the underlying error; the infix operator in parseLine adds RowError.Number.
- parseRows uses postfix ? inside the push argument, forwarding that envelope unchanged. On failure push is not called; temporary argument loans end before owned accumulated rows are disposed.
- load adds AppError.Rows at its wrapping operator. Its path copy runs only on failure.
- all selects one AppError failure without fabricating a union or adding an origin; it cancels/joins the other child as required.
- join waits for both business and observer branches. run's loaded? forwards the same envelope, so no additional propagation node appears.
- On successful operations, each infix RHS is skipped: it neither constructs an outer value nor copies its path.

IoError and Utf8Error instead become Read/Decode causes; write failure becomes Write caused by IoError. Every retained payload here owns its diagnostic state and supplies display/noBorrow/sendable/shareable. Borrowed file/slice views are never hidden in a cause.

A parent-typed AppError needs its own display implementation. Its Rows arm establishes the variant before calling the more-specific display; a variant overload alone would not dynamically handle parent-typed errors. The two main expectations distinguish the direct variant and explicit parent-match paths.

### Notifications, cancellation and scope exit

- There are exactly two owned sender handles, copied **before** closure capture and each moved into one loader. No extra parent sender remains to keep the observer waiting forever.
- Each loader attempts at most one notification. Buffer capacity two avoids a producer depending on concurrent consumption even if the observer stops early.
- The observer owns its receiver through its scoped closure. Each select borrows that receiver and one named timer owner; selected handlers return owned WatchEvent values, not references to callback temporaries.
- If the timer wins, the observer may stop and drop the receiver. Later sends fail, and notify deliberately recovers/disposes the exact unsent Progress rather than failing the business job.
- Queued messages can be deliberately disposed on receiver shutdown. That is application abandonment, **not** select consuming a losing receive. Only one selection commits and only its handler runs.
- Closed/drained input is handled by returning, not selecting the permanently-ready None case forever. Simultaneous message/closure/timer readiness has no guaranteed winner.
- A canceled/failed loader may never notify. Completion is determined by scoped results and handle cleanup, not by waiting for two messages.
- all/join still wait for child execution and cleanup. Native reads, cancellation-deferred joins, locks or timers may delay completion indefinitely; the observer timer is not a timeout for run.
- Counts is protected through ordinary lock guards and copied only as scalar record data after all work is joined. Stats/notifications are side effects, not rolled back if another child fails.
- Input buffers/views remain local to load; independently owned rows escape. Rendered string ownership stays live through the borrowed write/native completion. If write fails, a partial/truncated report is permitted.
- The final command-boundary panic follows structured error rendering after run has already joined its children/released its guards. It is intentionally fatal and does not promise cleanup of the remaining error envelope.

These are source-level obligations, not observations from a scheduler or proof of native/Wasm publication, source retention, exactly-once cleanup or host-pointer stability.

## Friction, reconciliation and rejected alternatives

**Accepted verdict:** the core pipeline fits the confirmed interfaces by inspection without a new language rule or unavoidable API blocker. Explicit ownership is workable here, but manual scanning, key duplication for ordering, lookup/insert loan boundaries and callback annotations are real ergonomic costs.

- No split/lines/trim, sort, general string builder, tryMap/collectResults, CLI parser, HTTP or JSON API is invented. Local scanning is application code; an application with richer text needs may justify a separate library ticket.
- A fallible map callback's ? would return from that callback, not parseRows. The explicit parsing loop handles early return without nonexistent Result-list flattening.
- Historical `Ok()` convenience conflicts with #23's ordinary one-argument Ok signature. The final write yields a real void value and run passes **that expression** to Ok; no new zero-argument overload/void literal is invented. #28 must reconcile older examples with the current signature instead of silently adding optional arguments.
- No recoverable aggregate-overflow helper is in the required library. The prototype deliberately retains checked i64 panic behavior; requiring a typed total-overflow error would need application prechecks or an explicitly approved extra helper.
- A literal alias/fitting value does not change numeric conversion availability, copying or cause-retention evidence.
- Optional niceties are not v0 language blockers: richer line scanning, update/entry convenience for Map, and explicit checked-arithmetic helpers can be evaluated separately rather than quietly enlarging #23.

Rejected or gated edits to this program:

1. Return a label view into load's local file buffer: invalid source escape, even if wrapped in a moved record.
2. Keep a map value reference live, insert into the same map, then use the reference: conflicting structural mutation; no disjoint-key exception.
3. Retain a borrowed path error as an erased cause: fails noBorrow; primary borrowing and unchanged propagation are separate contracts.
4. Capture one non-copying sender into two jobs or copy it only after the first capture: ownership/capture failure; clone the owner first.
5. Pass the mutable formatter through &F or annotate it read-only: cannot strengthen invocation mode.
6. Return a reference to an owned Progress callback argument in WatchEvent: local message/callback storage would be destroyed.
7. Mix plain jobs into all or produce different error types without explicit adaptation: no return-only dispatch/inferred error union.
8. Assume renderBlock's generic predicates, tuple inference, callee-exit/temporary cleanup or source-vs-descriptor dependencies have an implemented checker: existing #28 gate, not demonstrated acceptance.
9. Hold a guard across send: not inherently a type error (#16 allows suspension), but an avoidable lock-lifetime/liveness risk, not something the runtime may fix by silently unlocking early.
10. Claim the 250 ms timer bounds return, losing receives are consumed, file writes are transactional, or LLVM/Wasm support is validated: contradicts the approved contracts/#30 gate.

An independent read-only review found no blocking source contradiction or invented API by inspection, reproduced the counts/report/parser reasoning, and requested the explicit symbolic origins now shown above. Whitespace, local Markdown links/fences, source delimiter/numeric-width/export checks and independent sample-data arithmetic passed. Those checks do not compile or execute Rill, validate native scheduling or prove the dependency checker.

The user explicitly accepted this prototype and its documented ergonomic costs as #25 input to the RFCs. This does not accept a new safety/semantic rule, the future feature RFCs, compiler execution, checker soundness or backend support. Any unavoidable rejection uncovered by the eventual checker returns to the map; #28/#30 gates remain.
