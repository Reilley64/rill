# RFC 0010: Diagnostics and reporting

| Field | Value |
| --- | --- |
| ID | 0010 |
| Status | Draft |
| Owner | Reilley64. [#28](https://github.com/Reilley64/rill/issues/28) tracks this draft. |
| Source inputs | [#24 diagnostic decision](https://github.com/Reilley64/rill/issues/24#issuecomment-6885970775), [ADR 0016](../adr/0016-structured-diagnostics.md), [report contract](../diagnostic-contracts.md) and [source reconciliation](#source-reconciliation). Repository inputs use commit `f33dcf4cf3f815824eeb897135775d7884f65c1a`. The proposed annex snapshots appear [below](#proposed-normative-annexes). |
| RFC dependencies | [RFC 0001's accepted snapshot](https://github.com/Reilley64/rill/issues/28#issuecomment-6907867211), SHA-256 `ee4980216dd2ed051cb6153600bf8b5f6c039145c083abfeeb37119d027bd954`, owns expectations. Draft RFCs [0002](0002-modules-and-program-loading.md) through [0009](0009-user-implementable-conversions.md) propose analysis/typing/ownership/error rules. Their full texts are not accepted. Required [companions](#required-companion-rules) remain incomplete or missing. |
| Replaces / superseded by | None. This draft does not amend RFC 0001 or accept another draft. |
| Decision | Not yet decided |
| Reviewed revision | Not yet reviewed |

## Summary and scope

This RFC proposes one diagnostic model, source/snapshot locations, terminal and versioned JSONL reporting, driver outcomes and expectation-report adapters. It owns reporting contracts, not the language checks that produce their facts.

The glossary defines [diagnostic, analysis context, error wrapper and expectation](../../GLOSSARY.md#language). A compiler diagnostic is not an application error wrapper. A controlled tool failure and an expectation result also have separate record kinds.

RFC 0001's accepted snapshot owns expectation execution, capture restrictions and preview limits. This RFC maps those results into reports without adding observation, cleanup or capture rules. Its unfinished bounded-capture procedure remains owned by RFC 0001.

The owner confirmed occurrence grouping/order, summary accounting and safe report-file opening. The [policy record](https://github.com/Reilley64/rill/issues/28#issuecomment-6928487474) identifies their exact approval scope. Full driver state transitions, source mapping and emitter integration still need work. This document remains Draft, not implemented reporting or a safety proof.

Runtime error rendering remains with RFC 0008. Editor protocols are outside this RFC.

## Motivation

An editor, terminal and automation need the same source facts without sharing one presentation format. Stable codes identify problem meanings while messages can improve. A source error must not look like a compiler defect or incomplete analysis.

Source bytes, human columns and editor positions measure different things. Mixing them can highlight the wrong token or produce an unsafe suggested edit. Reports must identify the exact source snapshot they describe.

Limits and missing records can conceal failures if summaries imply more than the producer checked. Reporting can also overwrite source files or mix with application output. Those are driver failures, not successful validation.

## Guide-level explanation

Case D1 is a complete intended-rejection module:

```rill
function consume(value: string): void {}
function invalidUse(): string {
  let original = "owned";
  consume(original);
  return original;
}
```

The intended diagnostic is R0200. It identifies the later use and relates it to the ownership transfer. Exact English wording is not a parsing contract. No Rill compiler has emitted a report for this example.

The following is a proposed invocation, not an executed command:

```sh
rill check main.rill --report-format jsonl --report-file diagnostics.jsonl
```

Check performs front-end analysis only. It does not execute startup, expectations or user formatters. Its success would not certify linking or native/Wasm support.

Case D2 is a complete illustrative JSONL stream for a controlled internal failure. It contains authored data, not emitted output:

```jsonl
{"schemaVersion":1,"runId":"guide-internal-failure","sequence":"1","kind":"toolFailure","context":null,"failure":{"code":"R9000","message":"Example analysis state failure","phase":"frontEnd","analysisTrusted":false}}
{"schemaVersion":1,"runId":"guide-internal-failure","sequence":"2","kind":"summary","context":null,"summary":{"operation":"check","phase":"frontEnd","outcome":"incomplete","analysisComplete":false,"reportComplete":true,"countsExact":false,"counts":{"errors":"0","warnings":"0","expectationsDiscovered":"0","expectationsEvaluated":"0","expectationsPassed":"0","expectationsFailed":"0","omittedPrimaryReports":"0"},"reason":"Analysis is untrusted; zero counts are lower bounds, not proof of no source errors."}}
```

Under the confirmed policy, the producer can emit all available reports while analysis remains incomplete. The zero counters do not establish successful checking. A controlled internal failure uses status 3, not an ordinary source-rejection status.

Case D3 is a separate complete module:

```rill
function finished(): void {
  return;
  let unreachable = 1i32;
}
```

The initializer is an executable statement and is unreachable, R0700. The warning is advisory. The source still undergoes checking; warnings do not change literal validity or runtime trap behavior.

## Syntax and reference semantics

### Proposed normative annexes

This RFC proposes including these exact snapshots in its normative review boundary:

| Annex | Exact artifact and scope | SHA-256 |
| --- | --- | --- |
| A | [Diagnostic/report schema v1](../diagnostic-report.schema.json), complete JSON record shapes and field/value constraints. | `3f590f8c0d954088a9e68e4e95b681c080e51c9235bcd65e1c302ed92c5fe0c2` |
| B | [Diagnostic code registry v1](../diagnostic-codes.md), complete assigned problem meanings and compatibility rules. | `f9f4ad400dbab701ad2c160c8fd426dcdc510b68f7cce9312b554a896f228105` |

The links identify maintained files; the digests identify the proposed annex bytes. Later edits do not silently update this review boundary. Acceptance would need to identify the RFC revision and these annex snapshots explicitly.

The existing report contract, ADRs and fixtures remain supporting inputs. This RFC does not include them wholesale as normative annexes. The annexes themselves remain unaccepted with this Draft.

Annex A checks record shapes, not cross-record or semantic truth. The rules below supply additional sequence, range, scope and accounting requirements. Matching a JSON Schema alone cannot establish successful analysis or correct observations.

### Common diagnostic model

A diagnostic contains code, severity, message, optional primary labeled location, related labeled locations, notes, help and structured suggestions. It also identifies the relevant entry/target/options and source/dependency snapshot context.

Severity is error or warning in this schema version. Codes use the R namespace independently of severity. A code's documented meaning remains stable; it is not an exact message, compiler-stage or English-text identifier.

Errors block the requested operation. Warnings are advisory. Reusing a published code for a different meaning requires a new assignment instead. The registry is not an exhaustive catalogue of every future rejection.

Primary can be null when no truthful source location exists. Configuration, destination and internal failures must not fabricate a source span at line zero. Context can likewise be null when the driver cannot establish it.

Notes, related locations and help belong to a primary diagnostic. They are not additional independent source errors. A toolFailure record is not an ordinary diagnostic mislabeled as a source mistake.

The common model is not a serialized AST, HIR/MIR graph or transport message. Adapters preserve its meaning while using their own coordinate and presentation conventions. They must not recover structured facts by parsing message text.

### Analysis and source identities

An analysis context identifies the selected entry, target and relevant compiler options. Its snapshot includes the relevant loaded dependency/source versions. A workspace index or one open document version does not establish that context.

Attach sourceSnapshotId to every source location. URI plus offsets without the snapshot cannot safely identify a suggestion target. Treat run/context/snapshot identifiers as opaque equality tokens, not encoded paths or timestamps.

Source evidence must refer to the snapshot actually checked or executed. Formatting cannot reread a changed file and claim those new bytes produced the old error. Known stale facts cannot be relabeled as a current successful check.

Loaded app/std origins and canonical identities follow RFC 0002. Human path rendering must distinguish otherwise identical origin-relative names. Source identity does not use inode/content equality merely because files share storage.

JSONL context contains id and snapshotId, with entryUri, target and compilerOptions when established. Null context is honest missing context, not an invitation to invent entry/configuration facts.

### Source ranges and coordinate projection

Locations use original-source UTF-8 byte offsets, with exclusive ends. Require `0 <= startByte <= endByte <= snapshot byte length`. A zero-width span can identify an insertion point or EOF.

Human positions use one-based source lines and Unicode-scalar columns. A tab is one source scalar, not its terminal display width. Combining scalars and astral scalars retain their actual source positions.

Terminal alignment separately expands tabs and accounts for display widths.

Preserve mappings through physical CRLF and literal decoding/normalization. A decoded literal offset is not the original token's source offset. Do not split encoded scalars. Do not use floating-point numbers for large byte offsets.

For unavailable decoded coordinates, omit both start and end projections rather than clamp or invent them. Retain a truthful original-byte range when available. Invalid source bytes do not justify fabricated Unicode or editor coordinates.

The [confirmed coordinate procedure below](#proposed-original-byte-coordinate-procedure) specifies line-ending boundary and invalid-source projection rules. Unrepresentable positions must disclose the limit rather than wrap into another location. This RFC adds no public fields to runtime SourceLocation or parser for formatLocation text.

Machine locations use canonical encoded URIs. Human display text does not replace that identity. Source/path text remains untrusted data in every adapter.

<a id="proposed-original-byte-coordinate-procedure"></a>

#### Original-byte coordinate procedure

The owner [confirmed this file-level mapping policy](https://github.com/Reilley64/rill/issues/28#issuecomment-6929923825) with the lexical/source contract. It includes invalid-source positions. It does not change runtime SourceLocation or accepted expectation identity.

Build physical lines from the saved raw bytes, not normalized literal content. Recognize CRLF as one ending, then lone CR or LF as one ending. Lone-CR handling is confirmed by the lexical decision; CRLF preservation follows the earlier source decision.

Line zero starts at byte zero. Every physical ending creates the next line at its first following byte. A trailing ending therefore creates an empty final line. An empty file has one empty line.

For an offset b, first require `0 <= b <= source length`. An offset between CR and LF of the same pair has no decoded coordinate. Do not map it to either neighboring position.

Otherwise choose the line with the greatest start not exceeding b. Strictly decode the bytes from that line's start up to b. Failure or a partial scalar makes the projection unavailable at b.

A valid prefix supplies these coordinates:

```text
human line = zero-based line index + 1
human column = Unicode scalar count in prefix + 1
UTF-8 prefix length = encoded prefix byte length
UTF-16 prefix length = prefix UTF-16 code-unit count
UTF-32 prefix length = prefix Unicode scalar count
```

Tabs contribute one scalar, one UTF-8 byte and one UTF-16 unit. Astral scalars contribute one scalar, their actual UTF-8 bytes and two UTF-16 units. Combining scalars remain separate columns; terminal display alignment is a different calculation.

A BOM in a snapshot contributes to its scalar and code-unit counts. Omitting it changes the snapshot bytes.

This procedure can project a later valid line after invalid UTF-8 on an earlier line. It cannot guess scalar counts after invalid bytes within the same line. Line boundaries remain identifiable from their ASCII bytes.

Project span endpoints independently. Include the decoded start/end pair only when both projections exist in the requested coordinate system. Otherwise retain the original byte range and omit both decoded fields.

Reject out-of-bounds or reversed byte ranges rather than clamp them. Report an unrepresentable location without wrapping or substituting a nearby range. Human adapters can show byte ranges and safely escaped source without guessed columns.

The illustrative Y1 source contains these bytes:

```text
source scalars: A, TAB, é, U+1F642, CR, LF, B
hex bytes:     41 09 C3 A9 F0 9F 99 82 0D 0A 42
```

| Case | Offset or condition | Human line/column | Zero-based line / UTF-16 prefix units |
| --- | --- | --- | --- |
| Y1a | Byte 2, after tab | 1 / 3 | 0 / 2 |
| Y1b | Byte 4, before astral scalar | 1 / 4 | 0 / 3 |
| Y1c | Byte 8, before CRLF | 1 / 5 | 0 / 5 |
| Y1d | Byte 9, inside CRLF | Unavailable | Unavailable |
| Y1e | Byte 10, next line | 2 / 1 | 1 / 0 |
| Y1f | Byte 11, EOF | 2 / 2 | 1 / 1 |
| Y1g | Byte 3, inside é | Unavailable | Unavailable |
| Y2 | Bytes `41 0D 42`, offset 2 | 2 / 1 | 1 / 0 |
| Y3 | Bytes `FF 0A 42`, offset 2 | 2 / 1 | 1 / 0 |
| Y4 | Empty file, EOF at byte 0 | 1 / 1 | 0 / 0 |
| Y5 | Literal CRLF decodes to one LF | Original-byte spans do not shrink. | Map original endpoints, not decoded-content offsets. |

These are authored mapping expectations, not executed Rill source-map results. Historical coordinate fixtures remain unchanged. Whole-context freshness and edit preconditions still need their own complete procedures.

### Explanations and suggested edits

Lead with the failed operation and the relevant source rule. Show move/borrow origins and why later use, retained storage, cleanup or native work still needs the source. Keep noBorrow, sendable, shareable, copy and callable mode distinct.

Separate ordinary parameter mismatch, ambiguous inference, missing selected-body evidence, incomparable patterns and collection duplicates. Conversion failures additionally distinguish pair absence, duplicate pair, protected overlap, endpoint permission and selected-error mismatch.

RFC 0009's confirmed applicability policy remains conversion-specific. Unknown evidence is not proof of an absent pair. An expected E is not a reason to select another implementation.

Explain inner retained-cause requirements under RFC 0008, not merely outer properties. Exact forwarding and a fresh wrapping origin remain distinct. A final-looking call with pending cleanup is not a silently eligible tail transfer.

A suggestion contains title, intent, applicability, snapshotId and proposed edits. Each edit includes a source location and replacement text. This is data, not an apply command or transaction promise.

Intent is mechanical, behaviorChoice or scaffold. Applicability is verified, requiresChoice or unavailable. A verified suggestion must have the source, type, visibility, effect and ownership evidence its claimed change needs.

Do not suggest unsupported lifetimes or forbidden copies. Do not suggest an inaccessible constructor. Do not suggest moving a value that still carries the invalid borrow. Do not mark incomplete/generated scaffold text as a finished implementation. An explicit conversion or error adaptation can require user choice and additional evidence.

Application must revalidate source and dependency preconditions. Reporting an edit does not apply it or promise atomicity.

<a id="proposed-occurrence-grouping-and-deterministic-limits"></a>

### Occurrence grouping and deterministic limits

Identify a diagnostic occurrence by context/snapshot and the originating failed check instance. Keep that identity private; v1 adds no required public diagnostic-id field. Repeated dependency paths to one established failed check can share its primary report.

Independent failed operations remain distinct even when their code, span or message text matches. Never deduplicate by English text alone. Establish causal grouping from actual checking facts rather than visual similarity.

Attach derivative consequences to their established root. Suppressing those cascades must not hide an independent failure. Recovery may use poisoned/unavailable facts, but must not invent a successful type or candidate.

For independent compiler primaries, preserve the source decision's ordering components:

1. Order by canonical analysis context, not worker completion or random context tokens.
2. Put errors before warnings within the context.
3. Compare origin-tagged canonical display-path keys as unsigned UTF-8 bytes.
4. Compare original start/end byte positions, then code bytes.
5. Break remaining ties with a stable originating-check key.

The display-path key uses origin identity and canonical origin-relative spelling, not terminal escapes. Null context/location uses a fixed before-known sentinel at its applicable ordering position. Complete context-key/check-key construction remains authoring work.

Apply presentation limits after grouping and ordering. Parallel discovery must not change which primary reports survive the limit. Notes/related locations do not consume extra primary-report slots.

Human output defaults to 20 primary reports. JSONL has no default count limit. An explicit max-diagnostics caps diagnostic and failed-expectation primary reports, not test execution or passed result records.

A limited JSONL primary is omitted as a whole record. Count the omission and mark reportComplete false. Internal failure/pass counts remain separate from emitted-record counts.

Expectation results keep RFC 0001's execution order rather than compiler severity order. Do not reorder or retry expectations to fill a report quota. No reporting limit changes source validity or completed Boolean outcomes.

### Human output, warnings and offline help

Human compiler and test reports go to stderr. Show a concise severity/code headline, relevant source and labeled related spans, then necessary notes/help. Exact glyphs and prose can change without changing problem meaning.

Support color auto, always and never. JSONL contains no styling or interleaved progress text. Escape source/path control characters and keep source-derived Markdown inert.

Bound excerpts and type explanations. Disclose omitted detail. Do not dump the environment or heap. Do not upload reports automatically. Do not promise automatic redaction. Source/value text can contain secrets.

Default warnings remain narrow: established unreachable executable statements and optionally safely proven runtime traps. Declarations are not unreachable runtime statements merely because they follow return. An ordinary let initializer is executable.

Unexecuted source still undergoes static checks. A valid integer arithmetic trap remains fatal only if evaluated; IEEE float zero division is not that trap. No required general constant evaluator or v0 warnings-as-errors flag is added.

Unused Result remains a hard language error, R0103. Do not replace it with an unused-binding warning. Broad unused-import/binding cleanup must not discard resource or module-loading effects.

`rill explain <code>` supplies offline meanings/examples from the registry and packaged documentation. Unknown codes are driver invocation errors, R0603. It does not fetch documentation or execute code from a supplied string.

Exact structured-explain payload and option applicability still need a driver decision. Annex A's explain operation value alone does not supply an explanation record. This gap does not reduce the required offline human command.

### CLI reporting and phase scope

The proposed reporting options preserve #24's spellings:

```text
--report-format human|jsonl
--report-file <path|->
--color auto|always|never
--max-diagnostics <positive-count>
```

Human is the default. Report-file selects the JSONL sink; a literal dash selects stdout. The confirmed sink policy requires explicit JSONL mode for report-file. JSONL check/build default to stdout. Run/test require an explicit non-stdout sink so application output stays raw.

Max-diagnostics accepts a positive count, not zero, negative, wrapped or silently clamped input. Unsupported values/settings are invocation errors. The option limits reports, not checking, expectation evaluation or cleanup.

Check shares front-end analysis without LLVM generation, executable output or user-code execution. Build performs later work and reports its actual scope. Front-end success cannot certify an unavailable backend or executable.

Run's structured scope ends at its build/driver phase. Application stdout/stderr, Err formatting, panic text and raw exits are not automatically JSONL records. A successful build summary is not application success.

Test first checks the canonical loaded graph. Front-end rejection means no expectation execution. Valid testing reports actual outcomes in accepted order; empty completed discovery reports zero tests, not invented passing cases.

The schema supports operation and phase separately. A summary must identify the achieved phase, not imply later phases ran. Complete per-command failure precedence and execution-stop transitions remain driver/runtime authoring work.

<a id="proposed-jsonl-sequence-counters-and-completion"></a>

### JSONL sequence, counters and completion

Emit UTF-8 JSON objects, one record per line, with LF from the writer. Escape embedded newlines inside JSON strings. Transport chunks and partial objects are not records; a complete final JSON object can be read at EOF without LF.

Each invocation uses one runId for its stream. Emitted sequences start at string "1" and increase by one without gaps. Omitted reports do not consume sequence numbers.

Emit at most one controlled final summary, as the last record. Do not emit intermediate summary records that resemble completed operations. Records after that summary or conflicting repeated summaries violate this stream contract.

The four record kinds are diagnostic, expectationResult, toolFailure and summary. Use Annex A's required fields and exact enums. Unknown optional fields can be ignored; unsupported schemaVersion or incompatible required meanings cannot be treated as v1.

Offsets, coordinates, sequence numbers, preview budgets and counters use canonical decimal strings where the schema requires them. Do not round through JSON floating point or replace unknown counts with claimed exact zeroes. Identifiers and descriptive previews are not program-value serialization.

The confirmed summary meanings are independent:

| Field | Required interpretation |
| --- | --- |
| analysisComplete | The reported validation phase completed its required decision without known missing checks, cancellation or unavailable analysis. It does not certify later phases. |
| reportComplete | The producer emitted all available requested reports/details without known omission. It does not acknowledge downstream delivery, complete analysis or durable storage. |
| countsExact | Counters are exact totals for the stated phase/scope, including tracked omitted occurrences. Otherwise they are reliable observed lower bounds. |

A completed source rejection can have analysisComplete true. Invalid regions need not acquire invented type facts or cascading errors. Exhausted/unfinished required analysis instead reports incomplete, even when some real source errors are already known.

A controlled passed summary requires analysisComplete and countsExact true, with zero errors and failed expectations. Known warning-only presentation omissions may leave reportComplete false without changing completed analysis into a source failure. State those omissions explicitly.

Canceled/incomplete summaries require analysisComplete false. Internal tool failure also makes the affected analysis untrusted. A zero lower bound with countsExact false is not evidence that no errors exist.

Count primary diagnostic occurrences after justified grouping, including known records omitted by presentation limits. ToolFailure is a separate kind, not a fabricated source diagnostic. Related notes are not additional error/warning counts.

Count expectationsEvaluated only after an ordinary Boolean outcome completes its required cleanup. Require passed plus failed to equal evaluated, and evaluated not to exceed discovered. Started, canceled or fatally terminated evaluations are not invented passes or false results.

Completed ordinary expectations phase requires evaluated equal discovered. Non-execution phases record no evaluated/pass/fail events. Discovered counts describe the actual loaded graph, not one file's textual declarations.

Count omittedPrimaryReports for omitted diagnostic/failed-expectation primary records. Preview-budget truncation stays separately marked by previewIncomplete/observation status. It alone need not make the record stream incomplete.

If no primary records are omitted and countsExact is true, available JSONL event counts must agree with summary counters. When known records are omitted, emitted counts can be lower but cannot exceed the exact totals. Inexact counts must not contradict already observed records.

No final summary means completion was not observed. A valid summary still does not guarantee sink durability, application success or trust in an externally failed transport. Later known write/flush/close failure remains nonzero despite an earlier completed analysis phase.

<a id="proposed-safe-report-file-opening"></a>

### Safe report-file opening

Validate options before execution and before destructive file operations. The confirmed v0 policy limits path sinks to regular files, plus the explicit stdout mode above. Reject named pipes, devices and other unsupported path sink kinds without blocking on them.

Protect selected entry/source inputs, known source-lookup paths and the active read-only toolchain roots. Preserve both canonical path and available physical file identities. Diagnostics must not overwrite a source through a symlink or hard-link alias.

This physical alias check is not module deduplication. RFC 0002 can treat distinct canonical hard-link paths as distinct modules. A report sink still cannot safely truncate their shared storage.

Resolve and check a destination's existing ancestry before creating a new file. Parent directories must already exist. Existing regular files must open without truncation first. Verify the actual opened object against protected identities before truncation or writing.

Check a regular-file stdout sink against protected input identities. A shell can truncate files before launching Rill. The driver cannot undo those earlier effects.

For run/test, also reject a destination that aliases application stdout or stderr. A non-dash spelling alone does not prove a separate sink. Do not capture/rewrite application I/O to repair that conflict.

Only after successful admission may an existing report file be truncated for the new stream. A reused report file is not an append-only history. These rules add no atomic-replacement, cross-process locking, durability or filesystem-rollback promise.

Human mode with report-file is an invocation error, R0603/status 2. Unsupported path sink kinds and protected aliases also use R0603/status 2. Filesystem permission/open/write/flush/close failures are driver I/O failures, R0604/status 1. Error reporting must not recursively write prose into a broken promised JSONL stream.

If source/protection discovery cannot establish safety, do not truncate first and validate later. Use a non-destructive failure path with best-effort stderr explanation. Do not use a known protected fallback stream for that explanation. Do not add unstructured fallback text when stderr aliases the JSONL sink. Do not create a successful report by overwriting the evidence being checked.

The source decision's native host baseline is macOS/Linux on x86-64/AArch64. Validate descriptor/identity behavior separately on each supported host. Wasm application targets do not turn JSONL into a runtime logging API.

Supported platform implementations need race-aware descriptor/identity checks. This is not a sandbox against unrelated hostile filesystem writers. Complete admission/teardown procedures and active-execution failure handling remain implementation/specification gates.

### Driver statuses and trust boundaries

| Status | Required meaning |
| --- | --- |
| 0 | The requested reported operation/phase completed successfully within its stated scope. |
| 1 | A valid request failed through source/build/expectation/environmental operation failure. |
| 2 | Invocation/configuration is invalid, including incompatible reporting destination modes. |
| 3 | A controlled internal tool failure prevents trusted analysis. |
| 130 | The operation was interrupted under the host convention. |

These meanings do not normalize arbitrary application exits, signals, OOM or abrupt host termination. No report or final status mapping is guaranteed after fatal termination. A controlled incomplete/canceled operation cannot report successful validation.

A known sink failure remains nonzero with best-effort stderr explanation. Do not print success after detecting it. Existing Rill interruption, cleanup and fatal rules govern active execution; this RFC adds no panic catcher or global join.

### Expectation adapter without an amendment

Use RFC 0001's accepted context/snapshot/URI/span identity and execution rules. An actual expectationResult needs its established context, not null in place of that identity. The expectationResult fields are report data, not new runtime callbacks. Identical expression text at different sites remains separate declarations.

Its id is an opaque representation of that identity within the run/context. Status is passed or failed only for an ordinary completed Boolean outcome. Panic, cancellation and missing execution are not new expectation statuses invented by this adapter.

Map only trusted diagnostic-owned observations into Annex A. Preserve captured, notEvaluated, unavailable and truncated distinctions. Captured/truncated observations carry descriptive type/rendered data; skipped/unavailable observations have no invented rendered value.

The accepted limits remain four levels, eight children per expansion and a shared 4096 rendered UTF-8 bytes. This RFC neither changes their accounting nor supplies missing capture/work/cycle budgets. Reported previewBudget states the applied contract, not permission for extra traversal.

Trusted reporting adds no user show/display/copy/conversion/iterator/disposer calls, loans, moves, locks or changed source/guard lifetime. It does not serialize a retained Err/Cause or invoke full runtime error formatting. Ordinary operations explicitly present in the original expression still execute normally once.

A false result continues after normal cleanup under RFC 0001. Adapter/output failure is not a source Boolean result. Capture-policy amendments must return to RFC 0001's explicit amendment process, not appear here as reporting precision.

### Required companion rules

| Rule owner | Required contribution | Current dependency state |
| --- | --- | --- |
| [Accepted RFC 0001](0001-top-level-expectations.md) | Expectation behavior, trusted observations and preview limits. | Accepted snapshot identified in metadata. Complete bounded capture remains unfinished there; this RFC does not amend it. |
| [Draft RFC 0002](0002-modules-and-program-loading.md) | Canonical origins, source snapshots, loaded graph and analysis context. | Identity/order policies are confirmed. Complete snapshot/source-map and resolver rules remain incomplete. |
| Draft RFCs [0003](0003-expressions-bindings-and-control-flow.md), [0004](0004-functions-overloads-and-generic-requirements.md) and [0005](0005-types-records-enums-and-inference.md). | Actual grammar, flow, generic/type facts and failure provenance. | Their precision policies are confirmed. Full checking procedures remain incomplete. Reporting cannot invent missing facts. |
| Draft RFCs [0006](0006-numbers-strings-and-primitive-operations.md) and [0007](0007-ownership-borrowing-mutation-and-cleanup.md). | Literal/source maps, scalar preview data, moves, dependencies and cleanup. | Their policy approvals do not prove safe capture or full ownership. Complete judgments remain incomplete. |
| Draft RFCs [0008](0008-errors-propagation-and-caused-by-wrapping.md) and [0009](0009-user-implementable-conversions.md). | Error/cause and pair/evidence distinctions. | Their precision policies are confirmed. Full sealed/selection/evidence rules remain incomplete; runtime rendering is separate. |
| Driver and runtime | Actual sink state transitions and execution-stop behavior. | Complete procedures remain missing. |

## Valid/invalid examples and diagnostics

D1 through D3 appear in the guide. Later cases are independent. Source/runtime outcomes are expectations, not emitted compiler results. Authored JSON validation does not establish the truth of its claimed analysis.

Case D4 describes context identity. The same file checked for two targets has distinct analysis contexts. A report from an old dependency snapshot cannot become current merely by retaining the same entry URI. Null context is preferable to fabricated target/options facts.

Case D5 describes coordinates. An astral token has different scalar and UTF-16 offsets; a leading tab has a separate visual width. Preserve original CRLF bytes and snapshot ranges. The historical coordinate-map fixture exercises token boundaries, not all invalid-source projection cases.

Case D6 describes invalid/unrepresentable locations. Keep truthful byte-only ranges when decoded positions are unavailable. Reject malformed/out-of-bounds location claims in producer validation. Do not split a multibyte scalar, clamp large values or synthesize line zero.

Case D7 describes distinct occurrences. Two independent failed operations can share code/span/message text. Under the confirmed policy, they remain separate originating checks. A repeated dependency path to one actual failed check can share its root instead.

Case D8 describes causal grouping. One unresolved import can produce dependent unknown-call/type symptoms. Attach justified consequences rather than inventing many unrelated primaries. Preserve a separate ownership failure whose proof does not depend on that import.

Case D9 describes deterministic limits. Parallel workers discover the same independent diagnostics in different arrival orders. Group and sort before choosing the first 20 terminal primaries. Neither worker order nor object addresses can choose which errors remain visible.

Case D10 describes warning-only truncation. Complete analysis finds 100 warning primaries and no errors; a limit emits 20. Under the confirmed policy, counts stay exact at 100 with 80 omitted primaries and reportComplete false. The completed check can still pass with status 0. Presentation omission is not incomplete analysis or Werror.

Case D11 describes incomplete zero counts. D2 reports zero lower bounds with countsExact false and analysisComplete false. It cannot be interpreted as a passed source check. A controlled internal failure remains status 3 even if some earlier source diagnostics looked valid.

Case D12 describes stream validity. Duplicate/out-of-order sequence strings, a second summary or records after the terminal summary violate the confirmed protocol policy. A partial final object supplies no summary. Unknown optional fields differ from unsupported schemaVersion or incompatible required meanings.

Case D13 describes preview truncation. All expectation result records arrive, but one contains a truncated observation and previewIncomplete true. Under the confirmed accounting policy, that alone need not set reportComplete false. It does not claim a complete value or truncate normal runtime causes.

Case D14 describes false and fatal outcomes. A false expectation records one ordinary failure after cleanup and continues. A panic cannot become a failed Boolean record with a guessed preview. No final report is guaranteed after fatal termination.

Case D15 describes evidence explanations. Ordinary missing selected-body requirements and conversion applicability are separate rules. A selected E mismatch is R0313, not a request to choose a fallback. An unresolved conversion goal must not become a fabricated R0314 absence proof.

Case D16 describes suggestions. An unavailable copy or private constructor cannot be proposed as a verified fix. A valid explicit conversion/error adaptation can require behaviorChoice and its own evidence. No suggestion text runs as a command merely because the client displays it.

Case D17 describes stale edits. A suggestion was verified against one source/dependency snapshot. Changed inputs require revalidation before application. The stored verified label neither updates its preconditions nor promises an atomic client edit.

Case D18 describes protected sink aliases. The selected report path is a symlink or hard link to loaded source. The confirmed admission policy refuses it before truncation, status 2/R0603. Inode alias checks for output safety do not merge distinct modules.

Case D19 describes stdout aliases and special files. Run/test cannot bypass the dedicated-sink rule through another path to application stdout or stderr. Under the confirmed v0 policy, FIFO/device report paths are unsupported invocation modes, not a reason to block during validation. A separate admitted regular file is valid in principle.

Case D20 describes report I/O failure. A valid sink later fails write/flush/close. Return nonzero and explain best-effort on stderr, without adding prose to the JSONL stream. A previously completed analysis or parseable summary does not prove durable output.

Case D21 describes front-end-only checking. Check loads/checks the selected context without emitting LLVM or executing startup, expectations, conversions or formatters. An absent parser/checker cannot be reported as a successful empty check. A front-end pass does not establish backend support.

Case D22 describes run separation. A build-phase JSONL summary passes, then the application writes raw text and exits nonzero. That application result is not overwritten by the earlier summary or converted into a compiler diagnostic. Report scope remains explicit.

Case D23 describes warning boundaries. D3's initializer can receive R0700. A later function/type declaration is not an unreachable executable statement. Invalid literals still reject in unreachable code; a valid trapping expression retains evaluated-only fatal behavior. Unused Result stays hard R0103.

Case D24 describes observation effects. Reporting a failed expectation must not call user display/show/copy/conversion/iterator/disposer bodies. A primary typed Err or opaque resource stays observationally restricted. Reports retain diagnostic data, not sources needed for later user formatting.

Case D25 describes JSON types. Sequence/counter/offset strings cannot be replaced with lossy JSON numbers or leading-zero spellings. A notEvaluated observation has no rendered value. Source ranges and count relationships require validation beyond schema shape.

Case D26 describes code/help/security boundaries. Known codes keep Annex B's meanings; English prose can improve. Unknown explain codes fail as invocation errors. Source/path/Markdown/control data remains inert, without automatic environment dumps, uploads or remote documentation execution.

| Case / artifact and revision | Context | Expected outcome | Owning rule | Evidence label / observed result |
| --- | --- | --- | --- | --- |
| D1/D3/D15/D21/D23, this draft | Complete source modules and checker/evidence scenarios. | Explain real failures and preserve checking/warning/phase rules. | [Model](#common-diagnostic-model), [explanations](#explanations-and-suggested-edits), [CLI](#cli-reporting-and-phase-scope). | NotChecked. No Rill source checker or emitter. |
| D2/D10/D11/D12/D13/D25, this draft | Authored JSONL and accounting/transport scenarios. | Apply proposed sequence/completeness/count rules without invented success. | [JSONL](#proposed-jsonl-sequence-counters-and-completion). | NotChecked for producer/runtime behavior. Authored-data ToolChecked evidence appears below; no stream was emitted by Rill. |
| D4/D5/D6/D7/D8/D9, this draft | Context, coordinates, causal grouping and deterministic ordering. | Preserve source identity and proposed occurrence/limit semantics. | [Locations](#source-ranges-and-coordinate-projection), [grouping](#proposed-occurrence-grouping-and-deterministic-limits). | NotChecked. Historical coordinate checks cover only their named data cases. |
| D16/D17/D18/D19/D20/D26, this draft | Edits, safe sinks, code lookup and untrusted text. | Preserve preconditions, reject destructive aliases and report failure honestly. | [Suggestions](#explanations-and-suggested-edits), [sink opening](#proposed-safe-report-file-opening), [human output](#human-output-warnings-and-offline-help). | NotChecked. No safe-opening implementation or client application test. |
| D14/D22/D24, this draft | Runtime/fatal/output and expectation-observation boundaries. | Preserve accepted expectation semantics and separate application/driver results. | [Adapter](#expectation-adapter-without-an-amendment), [statuses](#driver-statuses-and-trust-boundaries). | NotChecked. No noninterference, interruption or runtime validation. |

Codes above use Annex B's assigned meanings, not invented emitted messages. Exact producer failure precedence and source-location error handling remain authoring work. No checker result follows merely from a structurally valid report.

## Feature interactions

### Existing language and runtime owners

RFC 0001 owns reached expectation behavior and trusted capture. This RFC proposes transport/accounting, not a replacement assertion API or capture algorithm. Semantic changes there require its amendment discipline.

RFCs 0002 through 0009 own the facts diagnostics explain. Their policy approvals do not complete all grammar, inference, evidence, ownership or runtime checks. This RFC cannot fill missing facts with a plausible message.

RFC 0008 owns runtime Err/Cause formatting and opaque SourceLocation. Compiler coordinates are original-byte/source-map facts, not parsed runtime output. Preview limits cannot truncate its full explicit cause-chain contract.

Driver status is separate from application exits, Rill panic and intrinsic cancellation. Required source retention, cancellation masking and native completion remain with their actual semantic/runtime owners. Reporting cannot shorten them for convenience.

### Source reconciliation

| Input | Disposition |
| --- | --- |
| [#24 decision](https://github.com/Reilley64/rill/issues/24#issuecomment-6885970775), [ADR 0016](../adr/0016-structured-diagnostics.md), report contract | Preserve the rich model, stable codes, coordinates, narrow warnings, flags, statuses and separate report adapters. The owner confirmed occurrence/accounting/sink precision. Detailed procedures remain unfinished. |
| [RFC 0001 acceptance](https://github.com/Reilley64/rill/issues/28#issuecomment-6907867211) | Preserve Boolean outcomes, sequence, source identity, no added user effects and 4/8/4096 capture limits. Do not complete missing capture accounting through an implicit amendment. |
| [Diagnostic schema](../diagnostic-report.schema.json) and [code registry](../diagnostic-codes.md), exact annex digests | Propose their explicit normative adoption with this RFC, not automatic latest-file authority. Cross-record truth and operational procedures still need the body rules and validation. |
| #9/#15/#16/#17/#19/#21 and Draft RFCs 0002 through 0007 | Preserve nominal/static facts, actual source dependencies, initialized cleanup, private access and noncapturing declarations. No source rule changes through warning text or suggestions. |
| [RFC 0008 policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6917170418), [RFC 0009 policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6918885645) | Preserve exact errors, independent cause predicates and conversion-specific selection/equality. Ordinary families keep their distinct no-fallback behavior. |
| #22/#23/#29 and runtime/backend refinements | Preserve fatal no-unwind limits and normal full runtime cause rendering. A host crash, allocation failure or report failure need not produce a final stream. |
| [Diagnostic fixture inventory](../prototypes/21-diagnostics.md), source revision | Preserve illustrative streams, original URI prefix, source bytes/hashes and coordinate calculations. Tool-checked data is not emitted output or Rill execution. |

## Compatibility and migration

Use stable codes and versioned structured data instead of parsing human messages. Existing authored messages are examples, not frozen prose. Annex changes require explicit revision/adoption rather than silent replacement.

New codes can use an explicitly adopted registry revision without changing v1 record shapes. Consumers must not invent meanings for unfamiliar codes.

The pinned schema permits unknown optional fields. New required meanings, incompatible field changes or different version handling require a version change. A parser must not silently treat unsupported records as successful v1 validation.

Preserve original byte/snapshot identities when converting human/editor coordinates. Historical fictitious fixture URIs remain historical; real adapters need actual encoded host/client locations. Do not rewrite authored reports to resemble a current tool run.

The owner [confirmed the three precision policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6928487474). The full RFC and its annex adoption remain undecided. No released reporter exists. The Rust CLI scaffold still reports unavailable product commands; these proposed flags/status contracts do not claim current implementation.

## Security/resource implications

Report/source/value text may contain secrets or terminal/Markdown control data. Keep that text inert. Escape adapter-specific controls. Do not promise redaction, upload safety or a sandbox merely because data has a schema.

Source snapshots, causal graphs, sorting, output and pending diagnostic data can consume resources. Analysis and reporting completeness are separate. A resource limit must disclose its effect rather than convert missing checks into success.

Safe destination admission must precede destructive writes. Canonical paths alone miss hard links; identity checks alone do not define read-only-root containment. Neither supplies cross-process transaction or durability guarantees.

Accepted observation constraints remain mandatory across all resource states. Fatal exhaustion can prevent reports or cleanup. Do not add user callbacks or retain program data to improve a damaged report.

## Rationale, alternatives and prior art

A single semantic model prevents terminal and JSONL adapters from explaining different type or ownership rules. Stable codes support automation without freezing English. Snapshot-aware locations support safe interpretation without parsing rendered paths.

Text-only deduplication could hide independent failed checks. Private originating-check identity permits justified root grouping without changing the public schema. Stable order before limiting avoids worker scheduling choosing visible errors.

One pass/fail bit cannot describe incomplete analysis, omitted reports and exact counters. Separate flags permit honest partial output. A warning-only display cap should not invent source invalidity or silently truncate checking.

Opening output with truncation before alias checks can destroy the input being diagnosed. Verified regular-file admission is the confirmed v0 policy boundary. Other sink kinds could be reviewed later without claiming current support.

The pinned JSON Schema supplies structural constraints, not evidence of semantic truth. Existing coordinate/report examples justify mechanical checks only. No client behavior or noninterference proof follows from their format.

## Implementation implications

The compiler needs stable context/source identities, causal failure provenance and a common diagnostic-owned model. Recovery must preserve trustworthy facts without invented source success. Warnings cannot change static acceptance or evaluation.

Adapters need lossless offset conversion, stable ordering, explicit omissions and safe escaping. Sequence/counter logic must reflect actual emitted versus detected events. A summary needs the real phase/completeness state rather than a default success template.

The driver needs non-destructive option/sink preflight, physical/path alias checks and honest I/O failure handling. Coordinate and output procedures require supported platform implementations. No atomic client edits or global filesystem transaction is implied.

Expectation reporting requires RFC 0001's actual trusted capture mechanism, which remains incomplete. This RFC defines no fallback user formatter. This scaffold does not establish complete compiler, runtime or driver behavior.

## Evidence and checking plan

The 2026-11-17 lexical completion adds authored coordinate cases Y1 through Y5. These are NotChecked by a Rill source mapper. Existing fixture and data checks do not validate the mapping procedure.

### Actual evidence

| Artifact / revision | Evidence label | Rule/question exercised | Checker/tool and command/environment | Expected / observed outcome | Limits, negative/unavailable results and gate links |
| --- | --- | --- | --- | --- | --- |
| #24 decision, accepted RFC 0001 and source base `f33dcf4cf3f815824eeb897135775d7884f65c1a` | HandChecked | Common reporting model, execution/observation separation and authority. | Drafting agent, 2026-11-16. Read current decision, contracts, schema, registry and accepted expectation rules. | Preserved accepted behavior and separated the proposed precision/annex review. | Provenance review is not an emitter, complete driver contract or capture proof. |
| D1 through D26, this draft | NotChecked | Reporting, source identity, counters, sink safety and adapter semantics. | No Rill checker/reporter/runtime or safe-opening implementation is available. | Intended outcomes appear above. No source program or live report emission executed. | Mechanical validation of authored data remains separate. No case is RillExecuted. |
| Confirmed grouping/accounting/sink policies | NotChecked | Causal identity, summary truth, filesystem safety and state transitions. | No formal model, operational emitter or native/client acceptance test ran. | No soundness, race-free opening or noninterference result is claimed. | Complete procedures and actual platform/driver/runtime validation remain required. |
| Frozen annexes and historical diagnostic fixtures | ToolChecked | Schema, source hashes/ranges, counters and illustrative coordinate mappings. | Drafting agent, 2026-11-16. `mise exec -- uv run --project tools/python --locked --no-sync python prototypes/21-diagnostics/check-fixtures.py`. | Schema v1, six negative data cases and five illustrative streams passed. Checks also covered eight positive declarations and nine rejection fixture headers. | Declaration/header checks do not parse/type-check/execute Rill. Golden data and original URI prefixes remain unchanged. |
| D2 authored JSONL and proposed data invariants | ToolChecked | Record shapes, frozen digests and limited sequence/counter conditions. | Same environment. Temporary `/tmp/rill-diagnostics-data-check.py` through the locked tools/python environment. | Two D2 records and both annex digests passed. Eight negative schema/stream/accounting mutations were rejected. | These data assertions do not prove claimed analysis truth, opening safety or runtime observation. |
| RFC documents, register and glossary, local working draft | ToolChecked | Structure, writing limits, links and preserved earlier rules/artifacts. | Drafting agent, 2026-11-16. Temporary Python checks; `mise run check:docs`. | Ten IDs/register rows and required metadata/headings passed. Sentence/paragraph, fence, link and whitespace checks passed. | Document checks do not execute Rill, validate an emitter or audit the formal ASD-STE100 dictionary. |

### Planned checks. Not executed evidence

- Validate exact annex digests, schema shapes and stable code assignments.
- Check source/snapshot bounds, UTF-8 scalar boundaries and lossless decimal fields.
- Test CRLF, tabs, astral scalars, invalid bytes and unavailable projections.
- Keep independent occurrences distinct while grouping justified cascades.
- Vary worker arrival order without changing ordered/limited primary results.
- Validate complete/incomplete analysis separately from report omission and preview truncation.
- Reject sequence gaps, conflicting summaries and impossible event/counter claims.
- Test zero suites, warning-only caps and source rejection without invented execution.
- Reject symlink/hard-link/stdout aliases before truncation on supported hosts.
- Exercise destination modes, source lookup failure and later write/flush/close failure.
- Verify check/run/test phase boundaries and unchanged application I/O.
- Compare actual expectation effects with and without trusted observations.
- Test source/snapshot preconditions and inert structured suggestions.

These require the relevant producer, checker, runtime and target/client profiles. A schema check cannot clear their acceptance gates.

## Open/deferred questions

<a id="review-choices"></a>

### Confirmed policies

The project owner [approved all three policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6928487474) on 2026-11-17. The pre-recording snapshot has SHA-256 `040642dbac4b8c7a7eb9aadd9a3fa69ae6411edf9d2a7c4122d7daf53f40b25f`. This approval does not accept the full RFC or adopt the proposed annexes.

Occurrence identity uses the originating failed check and context/snapshot, not message text. Causal grouping preserves independent failures. Deterministic ordering precedes report limits; accepted expectation order remains unchanged.

Sequence, counter and summary rules distinguish complete analysis, complete reporting and exact totals. Warning-only presentation omissions need not fail completed checking. Preview truncation remains separate from missing report records.

Safe sink admission checks protected paths/physical identities before truncation. It requires explicit JSONL mode, existing parents and supported regular-file/stdout destinations. Run/test reject application stdout/stderr aliases. No atomic replacement, hostile-filesystem isolation or durability guarantee follows.

Approval does not supply future unwritten procedures. The remaining authoring and validation work below stays open.

### Remaining in-scope authoring work

The [coordinate procedure](#proposed-original-byte-coordinate-procedure) is confirmed with the lexical/source groups. Complete canonical context/check ordering keys. Finish verified suggestion criteria without adding unavailable type/ownership facts.

Complete per-command driver state tables, combined-failure precedence and structured-explain option/payload rules. Formalize counters at interrupted discovery/execution boundaries without invented completed outcomes.

Complete race-aware sink admission, source-protection discovery and active-execution report-failure handling on the supported hosts. Keep source snapshots, paths and physical identities distinct.

RFC 0001 still needs its own bounded capture/work/cycle procedure and amendment review. This document cannot supply it through a reporting annex.

These gaps prevent the [canonical readiness review](README.md#canonical-reviewready-checklist). Required inconsistent contracts return to [map #1](https://github.com/Reilley64/rill/issues/1), not silent source-rule or tooling-scope reduction.

### Implementation/validation gates and future scope

Implement the diagnostic model, adapters, driver and source mapping. Validate actual reports, filesystem handling, trusted observations and client workflows. Structural data checks alone are not those implementations.

This RFC adds no application error codes, automatic panic parsing, environment uploads, Werror, runtime value serialization or report-driven command execution. It adds no Windows, DAP, IDE Run/Test or notebook scope beyond the existing decisions.

## Decision and change history

### Readiness record

No canonical readiness review has occurred. This document remains Draft. The drafting agent checked sentence/paragraph limits and applied unslop as the final prose pass. Structure and link checks passed.

These checks cannot supply missing operational rules or trusted capture. Formal ASD-STE100 dictionary compliance remains unverified.

### Human decisions and amendments

On 2026-11-17, the owner requested completion of the lexical/source contract. The owner then [confirmed its three groups](https://github.com/Reilley64/rill/issues/28#issuecomment-6929923825), including the original-byte coordinate procedure. This is separate from the earlier reporting-policy approval. Existing reporting policies and proposed schema/code annex status remain unchanged.

The project owner requested this reporting RFC on 2026-11-16. That request authorized drafting, not policy approval, annex adoption or full RFC acceptance. The owner later [confirmed the three precision policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6928487474). Source decisions and earlier RFC approvals retain their own scopes.

| Date / actor | Kind and scope of change | Reviewed revision or snapshot | Decision link / rationale |
| --- | --- | --- | --- |
| 2026-11-16, drafting agent | Created substantive Draft RFC 0010, register and partial source coverage. | Source base `f33dcf4cf3f815824eeb897135775d7884f65c1a`, current #24 decision and the exact proposed annex digests above. This is not a reviewed RFC snapshot. | User request to draft diagnostics and reporting. No new policy, annex, full RFC or implementation acceptance is inferred. |
| 2026-11-17, project owner | Approved occurrence grouping, summary accounting and safe report-file opening. The full RFC remains Draft. | The three recommendations presented in chat. Pre-recording SHA-256: `040642dbac4b8c7a7eb9aadd9a3fa69ae6411edf9d2a7c4122d7daf53f40b25f`. No complete RFC revision was accepted. | [Policy approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6928487474). Annex adoption, complete source/driver/sink procedures, readiness and actual implementation/target/client validation remain open. |
| 2026-11-17, project owner | Approved the coordinated lexical/source groups, including original-byte coordinate projection. | Pre-recording SHA-256: `f97e1e2e983bb3f7f01f8d9576a8941758fc00a24108b0f4c8759c764878f706`. The shared decision identifies the other owner snapshots. | [Lexical policy approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6929923825). Full RFC/schema/code-annex acceptance, client snapshot/fallback handling, readiness and implementation remain open. |
