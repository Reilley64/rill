# Rill diagnostic and expectation-report contracts

ri-3t8 confirms this design; [ADR 0016](adr/0016-structured-diagnostics.md) records it. These are approved design inputs, not implemented diagnostics, accepted feature RFCs or a checker proof. [Language server contract](lsp-contracts.md) and tooling issue ri-bv2 cover editor support; ri-48g/#30 retain checking/backend validation. [Prototype 21](prototypes/21-diagnostics.md) is hand-authored, not emitted by a compiler.

## 1. Current source semantics

The later confirmed ri-kty decision replaces primitive literal types/general unions with enums. ri-vtw origins/explicit causes replace propagation histories. ri-8n7 refines copying to `copy(&value)`, uniquely owned Err/Cause and retained-cause `display(E), noBorrow(E), sendable(E), shareable(E)`, plus cleanup of remaining initialized fields after custom disposal. Primary typed borrowed errors may propagate with their dependencies. Instrumentation must not restore old copying behavior or erase these obligations. Later confirmed ri-1wl (comment 6886119679) uses language-wide `implement name(...)` / `implement<T> name(...)` for family bodies; body-less declarations and standalone functions retain `function`. Canonical into/from and tryInto/tryFrom are user-extensible with (To, From) keys and selected E evidence for tryInto(To, From, E). CamelCase replaces historical snake_case try names. Numeric pairs and infallible identity remain protected; numeric semantics do not become user-overridable. Preserve ordinary parameter-pattern dispatch/orphan rules outside these explicit conversion exceptions.

## 2. One model, separate adapters

A diagnostic contains a stable code, severity, a concise message, an optional primary location/label, related locations/labels, notes, help and structured suggestions. It also contains relevant entry/target/configuration/snapshot context. Terminal/JSONL/LSP adapt this model; it is not an LSP object or serialized HIR/MIR.

Codes identify documented problem kinds, independent of severity; see the [registry](diagnostic-codes.md). Published meanings are not recycled. Prose and explanatory detail may improve. Notes/help belong to their root diagnostic, not extra errors. Expectation results and internal tool failures are separate report kinds. Application errors/panics do not automatically use compiler codes.

Driver/configuration failures need no fake file/line. The LSP uses supported operational error notification/logging for non-source failures, not a fabricated source diagnostic at line zero. Source mistakes, incomplete analysis and internal tool failure remain distinct. A failed tool must not claim successful checking.

### Locations

Canonical ranges are original-source UTF-8 byte offsets with exclusive ends and source snapshot identity. Human lines and Unicode-scalar columns are one-based. Terminal visual alignment separately expands tabs and accounts for displayed widths. The [language server contract](lsp-contracts.md#diagnostics-and-positions) owns projection to protocol positions. Preserve maps through CRLF and literal decoding. Do not split encoded scalars, wrap/clamp unrepresentable positions or mislabel bytes as editor columns; report a tooling limit if necessary. Invalid source encoding can retain a byte location while unavailable decoded coordinates are omitted.

Human paths are context-relative where useful; machine locations are canonical encoded URIs with original source identity. This does not add public coordinate fields to ri-8n7's opaque runtime SourceLocation or parse `formatLocation` text into tool data.

### Explanations

Lead with the programmer's failed operation, then relevant origins and dependencies, with useful rule notes and evidence-backed suggestions. Ownership reports show move/borrow origin and the later use, retained slot, guard, cleanup or native operation still requiring the source. `noBorrow`, sendable, shareable, copy and callable mode are distinct. Guard unlock follows disposal, not last mention. Do not suggest unsupported lifetimes, copying forbidden resources or moving an already-borrowing closure as an escape.

Separate no parameter match, incomplete/ambiguous inference, selected-implementation requirement failure, incomparable bodies and collection-time canonical duplicates. Show static argument types, canonical family/signatures and concise rejection reasons, not an unexplained constraint dump. Returns and constraint truth do not distinguish ordinary bodies. Canonical conversion pair keys are a specific ri-1wl exception, not general return-only dispatch. E is selected output evidence, never a ranking key or reason to fall back. Distinguish pair duplicates, protected numeric/identity overlap, endpoint ownership and selected-error mismatch; generic requirements preserve symbolic E and explicit endpoint evidence. Whole-enum calls do not get an implicit dispatcher from variant coverage. Private metadata/navigation does not grant source access.

Explain missing widths rather than choose a numeric default. Invalid literals/type-invalid code reject even when unexecuted; otherwise valid constant traps remain panic-if-executed. Do not warn that IEEE division by zero is an integer trap. Explain missing inner retained-cause evidence, not merely outer error properties. New Err origin differs from unchanged propagation; explicit wrapping can owe post-call work. Final-looking ordinary calls are not errors; genuinely eligible unsupported tail lowering is an actionable target/call failure, not ordinary-call fallback.

## 3. Human reports, severity and limits

Human compiler/test reports go to stderr. The headline includes severity/code where applicable, a primary source excerpt and labeled related spans, followed by necessary notes/help. Exact glyphs/prose are not a stable interface. Default color is automatic for suitable terminals, with always/never controls; JSONL contains no terminal styling. Escape source/path control characters. Do not automatically dump environments/heaps, upload reports or promise secret detection. Source/value reports may contain sensitive text.

The default terminal limit is 20 primary reports (compiler diagnostics or detailed failed expectations); notes are not additional errors. Bound excerpts/type explanations; explicitly disclose omissions. Group derivative errors under their root cause and order independent compiler diagnostics by context, severity (errors first), canonical display path, position and code. Expectation results follow test order. JSONL streams available records by default; explicit limits/resource truncation must disclose incomplete coverage/omissions. Limits/cancellation cannot yield a passed check.

Errors block the requested operation. Warnings are advisory, not broad default stylistic linting. Initial warning meanings are unreachable executable statements established by flow and optionally safely proven runtime traps, without a required general constant evaluator. Function/implementation/type/import declarations are not unreachable executable statements merely because their textual position follows return; an ordinary let initializer is executable. Unexecuted code is still checked. No default unused-binding/import cleanup warnings that ignore resources/module loading, and no v0 warnings-as-errors option. The existing unused-Result rule is still a hard language error, not an advisory lint. Warnings never change acceptance/trap/folding rules.

`rill explain <code>` supplies offline documented meaning and examples. Unknown codes produce a driver error rather than executing/fetching anything. Source-derived text remains inert in terminal/JSON/Markdown adapters; documentation may be formatted but not interpreted as executable commands.

## 4. CLI and structured reports

`rill check <entry>` uses selected target/compiler options and shared front-end checks only. It produces no executable and performs no LLVM generation, startup or expectation execution. Success applies to that analysis phase, not linking, backend support or soundness. `build` performs its later stages; report scope must state which checks ran.

Reporting spellings for v0:

- `--report-format human|jsonl` (human default).
- `--report-file <path|->`: JSONL destination; `-` is stdout. JSONL check/build default to stdout. Run/test JSONL require an explicit non-stdout sink; no capture/rewrite of application stdout/stderr. Human reports remain on stderr.
- `--color auto|always|never` controls terminal presentation. `--max-diagnostics <positive-count>` sets an explicit primary-report limit: human default 20; JSONL has no default count limit. Disclose omitted records/coverage, and distinguish output limits from incomplete checking; presentation limits do not redefine validity.

Human stderr remains the default without a report sink. Progress/logging cannot be interleaved into the structured stream. Validate destination/options before execution; do not truncate loaded source or read-only toolchain files as a report sink. A report-sink failure is a driver I/O failure, nonzero with best-effort stderr explanation; do not print success or plain text into a promised JSON stream.

Run's structured scope ends at its build/driver phase; application stderr/Err/panic output is not automatically serialized. A phase-completion record is not successful completion of the application. Test reporting covers front-end rejection or actual expectation results as appropriate; compiler failure means tests did not run. No IDE test integration is implied.

### Driver status

| Status | Meaning |
| --- | --- |
| 0 | The requested reported check/build/test operation completed successfully. Check remains front-end-only. |
| 1 | A valid request failed because of a source, build, false-expectation or environmental operation failure. |
| 2 | The invocation/configuration is invalid, including inapplicable settings/report destination mode. |
| 3 | A controlled internal tool failure occurred. |
| 130 | The operation was interrupted under the host convention. |

Raw application exits/signals, OOM or abrupt host termination are not normalized into guaranteed driver statuses. No completion record is guaranteed after fatal termination. A controlled incomplete/canceled check is not successful validation. Empty checked test suites report zero discovered/evaluated tests, not invented passes.

### Compilable programs

`build` and `run` compile a program only when it is a compilable program: its entry startup and every body reachable from it are `Checked`, although the whole graph has no complete state, every edge the walk meets is one it follows, and every construct in those bodies is one the backend lowers (the `Shape` list in `compiler::analysis::compilable::capability`, which the backend is tested to lower in full). The compiler decides this once (`compiler::analysis::compilable::compilable_program`); neither the backend nor the CLI decides it. A valid program that is not compilable fails with driver status 1 and R0605, which names the first blocking span: a body that is not `Checked`, an edge of a kind the walk does not follow, or the first construct the backend does not lower. The backend never decides this; driver status 3 from `build` or `run` means a backend bug, never an unsupported program. A source with several findings reports all of them, in source order. R0605 is not a source error and is never reported for a finding or for an exhausted resource limit: a source finding keeps its own code, and a resource limit is reported as a limit. `check` reports R0605 for the same programs, without generating code. A program whose typed analysis cannot finish also reports R0605, at the first site that analysis publishes (`ProgramAnalysis::compilability` makes both decisions). A resource limit during `check`, `build` or `run`, in the analysis or in the rule itself (which takes its own budget from the same limits), exits with status 3, names the exhausted limit and says analysis is incomplete: it is neither R0605 nor a pass.

### Version 1

[The JSON Schema](diagnostic-report.schema.json) defines typed JSONL records: `diagnostic`, `expectationResult`, `toolFailure`, `summary`. Each has schemaVersion, a run id, a decimal-string sequence starting at 1 and increasing by one, and kind. Context/snapshot data associates facts with their input. Byte offsets/counters are canonical decimal strings, not lossy floats. Optional decoded line/column coordinates are likewise decimal strings; LSP ranges are separately encoded, not copied from these human coordinates. Previews are typed descriptive strings, not JSON serialization/reflection of program objects.

A final summary explicitly names operation/phase, outcome, validation scope, analysis/report completeness, counts and omissions. A completed rejected check differs from canceled/incomplete work. An absent final summary means report completion was not observed. Byte equality of example messages is not an emitter requirement. Schema v1 may add optional fields with unchanged meaning; consumers ignore unknown optional fields. Incompatible changes or new required event meanings need a version change. Sequence/order/range/count consistency needs checks beyond JSON Schema. Suggested edits carry source/snapshot preconditions; reporting one is not applying it or promising client atomicity.

## 5. Effect-preserving expectation reports

`expect` remains top-level/test-only/bool, checked along with loaded startup in both modes, without capturing startup lets. Executables omit test-only code. `rill test` discovers/checks its canonical loaded graph first, deduplicates modules, orders by canonical module identity/source position and evaluates expectations sequentially. No retries, filesystem rollback, hidden per-expect process isolation or new global background join. Normal ownership cleanup, cancellation masking, required joins and explicit background semantics remain.

Identify results by context/source snapshot/URI/span, not expression text. A true result passes; false finishes ordinary expression cleanup, reports one failed expectation and continues. Panic/managed overflow/fatal resource exhaustion ends current Rill execution without catch/unwind/guaranteed final formatting. Fatal scenarios belong in separate programs/processes. An external driver observing a crash does not make panic a catchable Boolean failure.

Report location, bounded expression and operand observations. Label left/right by source expression, not assumed actual/expected. Logical operators show evaluated operands and short-circuit `notEvaluated` markers. A Boolean helper can report its result, not invented helper internals/propagation traces. Do not rerun the expression to populate a report.

### Trusted snapshots

Capture at original evaluation points without consuming or retaining program values/loans. No added user show/display/copy/iterator/comparison/disposer calls, generic requirements, owner moves, early release or extended resource/guard lifetime. Explicit original operations execute once as normal; diagnostic code adds no callback execution to obtain values. Retain only diagnostic data. Passing expectations can discard previews without user cleanup effects.

The default shared per-expectation budget is 4 expansion levels, 8 children/elements per expanded value and 4096 UTF-8 bytes of rendered preview. Bound capture/work/storage before constructing the full graph, not merely output afterward. Intrinsic scalar/text/bytes/visible record/tuple/enum and safe standard-prefix observations may be supported without source iterators. Respect opacity/private access at the expectation site; resource/synchronization internals stay opaque, with no hidden locking/native reads. Detect cycles without printing addresses. Never perform unbounded deep diff, entire-map sorting or callback traversal for prettier output.

Statuses are `captured`, `notEvaluated`, `unavailable`, `truncated`; distinguish incomplete display from an actual program value. Scalars retain exact width/value (including float special/signed-zero distinctions without a NaN-payload portability promise); text/byte truncation is explicit and encoding-safe. Unknown values are not guessed. Budget exhaustion permits a minimal false/location report; allocation/host exhaustion may still be fatal. ri-48g must validate trusted observation and interaction with remaining-field cleanup/loans/masking. These bounded previews do not weaken ri-8n7's complete explicit runtime cause-chain rendering, whose payload display remains a separate operation.

## 6. Evidence and handoff

ri-3t8 approves this design only. ri-48g specifies exact grammar/inference/identity/cleanup/observation/lowering and adversarial tests. ri-dej validates pinned backends. ri-bv2 tracks standards-based editor support when the shared analysis can support it. Current fixtures are independently checked text/data calculations, not emitted diagnostics, Rill execution, safety proofs or client acceptance.
