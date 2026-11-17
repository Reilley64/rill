# RFC 0001: Top-level expectations and test execution

| Field | Value |
| --- | --- |
| ID | 0001 |
| Status | Accepted |
| Owner | Reilley64. [#28](https://github.com/Reilley64/rill/issues/28) tracks this draft. |
| Source inputs | Start with the [confirmed expectation decision](https://github.com/Reilley64/rill/issues/27#issuecomment-6852094134). [Source reconciliation](#source-reconciliation) lists the other inputs. Repository inputs use commit `f33dcf4cf3f815824eeb897135775d7884f65c1a`. The [control-flow and preview decisions](https://github.com/Reilley64/rill/issues/28#issuecomment-6907785727) supplement those inputs. |
| RFC dependencies | Required companion RFCs are missing. See [required companion rules](#required-companion-rules). These gaps remain open after acceptance. |
| Replaces / superseded by | None. This RFC does not replace another accepted RFC. |
| Decision | [Accepted by the project owner on 2026-11-16](https://github.com/Reilley64/rill/issues/28#issuecomment-6907867211). |
| Reviewed revision | Human-accepted [published snapshot](https://github.com/Reilley64/rill/issues/28#issuecomment-6907867211). SHA-256: `ee4980216dd2ed051cb6153600bf8b5f6c039145c083abfeeb37119d027bd954`. The canonical readiness review remains incomplete. |

## Summary and scope

This RFC proposes the module-level test declaration `expect <expression>;`. The expression must have type `bool`. `rill test` evaluates expectations instead of ordinary startup statements. Ordinary executables exclude expectations and their test-only executable code. The compiler still checks that source.

This RFC owns expectation placement, test discovery, test execution and failure observations. It uses an already resolved module graph. It does not define the full expression grammar, module resolver, ownership checker, scheduler or diagnostic protocol. Companion RFCs must supply those rules before ReviewReady.

This RFC preserves the confirmed source decisions. The current CLI reports `rill test` as unavailable. This proposal does not implement a test runner.

## Motivation

A programmer needs to test a small function beside its declaration. This should not require a separate test function, registration API or entry file. For example, an addition test needs ordinary calls and Boolean comparisons.

The runner must discover tests in imported modules without executing their startup code. Starting an application must not execute its tests. These separate modes let a file contain application startup and independent tests.

A failed comparison should identify the expression and relevant values. It must not evaluate the expression again. A second evaluation could repeat I/O, consume an owner or observe different state.

## Guide-level explanation

This complete example has two expectations that should pass under `rill test`.

```rill
function add(a: i32, b: i32): i32 {
  return a + b;
}

expect add(2, 3) == 5;
expect add(-1, 1) == 0;
```

`expect` is a keyword, not an imported function. An expectation passes when its expression produces `true`. It fails when the expression produces `false`. There is no truthiness conversion. A function can return the Boolean that an expectation uses. The expectation declaration cannot appear inside that function.

Expectations use declarations and helpers. They cannot capture values from ordinary startup statements. A test must construct its required inputs in an expression or helper and pass them explicitly. `rill test` does not execute the entry file's startup as setup.

After a false result, the runner completes normal cleanup and continues. A panic stops execution instead. Run fatal cases in separate programs or processes. There is no panic-catching assertion. Production validation uses the ordinary `guard`, `Result` and `panic` rules.

The examples show proposed behavior. No Rill compiler has compiled or executed them.

## Syntax and reference semantics

### Declaration and checking context

The declaration production is:

```text
expect-declaration = "expect" expression ";"
```

`expression` uses the ordinary expression grammar and operator precedence. The semicolon is required.

An expectation belongs directly in a file module's top-level declaration sequence. The compiler rejects it inside a function, closure or nested block. The expression must type-check as `bool`. The compiler does not implicitly unwrap a `Result` or convert another type to Boolean. R0102 identifies invalid placement. R0101 identifies a non-Boolean expectation.

The compiler checks loaded startup statements and expectation bodies in both ordinary and test modes. Code that will not execute must still pass these checks. This includes literal, name, type, ownership and evidence checks. Ordinary typing rules establish numeric widths. `expect` supplies no numeric default.

An expectation can use available declarations and non-capturing helpers. It cannot capture runtime `let` bindings from ordinary startup. An import used only by an expectation still belongs to the static program graph.

### Control flow

An expectation must produce `bool`. It does not create an implicit function that returns `Result` or `Option`. A `return` or postfix `?` cannot exit the expectation. These transfers are compile errors when they would leave the expectation.

Handle fallible work explicitly within the expression or call a helper. A helper or nested closure keeps its ordinary return and propagation rules. A transfer within that callable does not exit the expectation itself.

The [confirmed policy decision](https://github.com/Reilley64/rill/issues/28#issuecomment-6907785727) fixes this boundary. Compiler lowering must preserve it. Companion rules must still define expression grammar and ordinary callable checking.

### Discovery and module identity

The compiler first resolves and checks the selected entry's loaded graph. Module rules determine its prelude, transitive imports and re-exports. Imports used only by tests also belong to this graph. Discovery does not scan unrelated files. A workspace search index is not a program graph.

Each canonical loaded module contributes its expectations once. Aliases, re-exports and repeated paths do not duplicate a module's tests. This also applies to loaded standard-library modules. A local file's test count is not necessarily the complete suite count.

Expectations run sequentially in canonical module-identity order. Within each module, they run in source-position order. Filesystem traversal, import order, alias spelling and later dead-code elimination cannot change this order. The module RFC must define the exact identity comparison.

Static family collection includes test-only imports before executable-code elimination. Removing test-only executable code must not change overload selection. It must not suppress loaded duplicate or protected-operation errors in an ordinary build.

### Startup and execution modes

Ordinary execution runs the entry file's startup statements under the module rules. It does not execute expectations. Imported ordinary startup is not implicit module initialization.

Test execution skips ordinary startup statements. It evaluates the discovered expectations. There is no implicit setup, retry, filesystem rollback or per-expectation process isolation. Sequential evaluation does not serialize goroutines that the program starts. It does not add a global background join.

Ordinary ownership, cleanup, required joins, cancellation and explicit background-work rules still apply. The runner cannot release a retained source early or suppress required disposal. It cannot stop unrelated work to simplify reporting. Companion RFCs must own these rules.

### Boolean, rejected and fatal outcomes

For each expectation that execution reaches:

1. Evaluate its expression once with ordinary evaluation order and short-circuiting.
2. Complete the normal cleanup required for that evaluation.
3. Record a pass for `true` or one failure for `false`.
4. Continue to the next expectation after an ordinary Boolean outcome.

A false result is not a panic or an error wrapper. It does not cause unwinding, rollback or another evaluation for diagnostics.

A panic, managed stack overflow or fatal resource exhaustion ends current Rill execution. The fatal-runtime rules apply. The runner does not catch these failures as `false`. It does not promise unwinding or a final report. A driver that observes termination does not make the failure catchable in Rill.

A front-end rejection means expectations did not run. If checking completes and the graph contains no expectations, the runner reports zero discovered and evaluated tests. It does not invent a pass.

Successful testing requires complete checking and execution with no failed expectations. Interruption, unavailable implementation and incomplete checking cannot count as success. The driver/report RFC must own status numbers and protocol fields.

### Result identity and observations

A result identifies its analysis context, source snapshot, canonical source URI and declaration span. Expression text alone is not an identity. Identical text at two sites denotes two declarations.

Source offsets are original UTF-8 byte offsets with exclusive ends. Human positions use one-based lines and Unicode-scalar columns. These meanings must agree with the companion diagnostic RFC. Display coordinates cannot replace them.

A failure report identifies the location and bounded expression text. It includes available expression and operand observations. Reports label operands by their source expression or left/right position. They do not guess actual/expected roles. Logical operators distinguish skipped operands from operands whose values were unavailable. A helper call can expose its Boolean result, not invented internal observations.

Trusted observation captures diagnostic data at the original evaluation points. The following restrictions apply:

- Do not evaluate an expression or comparison again.
- Do not add user `show`, `display`, `copy`, iterator or disposer calls.
- Do not add generic requirements or move an owner.
- Do not retain a program value or reference for later printing.
- Do not introduce or extend a program loan.
- Do not change a guard or source lifetime, or alter ordinary cleanup.
- Do not lock a value or read hidden native or resource state for a preview.

Explicit operations in the original expression still execute normally once. Only diagnostic data survives to reporting. A delayed callback over the original value is not permitted. Discarding preview data after a pass must not cause user cleanup effects.

### Preview limits and opacity

The default expansion limit is four levels. Count each preview's root as level one. Do not traverse beyond level four. Mark omitted descendants as `truncated`. Each expanded preview contains at most eight children or elements.

An expectation's operand previews share a limit of 4096 rendered UTF-8 bytes. Allocate this budget in ordinary evaluation order. A later observation uses only the remaining budget. A skipped operand has no captured value. Mark observations as `truncated` when a limit prevents further capture.

Count rendered preview bytes, not source characters or Unicode scalars. Text and byte prefixes must still satisfy the encoding-safety rules below. Shared accounting must not change expression evaluation or cause another traversal for a better allocation.

Capture must bound work and storage before traversal. This includes metadata and cycle tracking, not only rendered text. Stop capture when a limit prevents further observation. Do not construct the full value graph and truncate it afterward.

The [confirmed policy decision](https://github.com/Reilley64/rill/issues/28#issuecomment-6907785727) establishes these limits and allocation rules. A complete bounded capture procedure remains authoring work before ReviewReady.

Trusted capture can describe intrinsic scalars, text, bytes and visible record, tuple or enum data. It can also describe safe standard prefixes. It must not call source-level iterators. It must respect access at the expectation site. Private or opaque representations and resource or synchronization internals remain opaque.

Cycle handling must not print memory addresses. Reporting cannot sort an entire map, compute unbounded deep differences or traverse callbacks for better output.

The observation states are `captured`, `notEvaluated`, `unavailable` and `truncated`. These states are not program values. Captured scalars preserve width and value, including signed zero and float special values. Portable NaN payloads are not guaranteed. Text and byte truncation must be explicit and encoding-safe. The report must not guess missing data.

Preview-budget exhaustion permits a minimal report with the location and false result. Actual allocation or host exhaustion can still be fatal. These limits do not shorten complete explicit error-cause rendering.

### Required companion rules

[Draft RFC 0002](0002-modules-and-program-loading.md) proposes the module rules. [Draft RFC 0003](0003-expressions-bindings-and-control-flow.md) proposes expression and control rules. [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md) proposes function and generic rules. [Draft RFC 0005](0005-types-records-enums-and-inference.md) proposes types, records and enums. [Draft RFC 0006](0006-numbers-strings-and-primitive-operations.md) proposes numeric/text rules. [Draft RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md) proposes ownership and cleanup.

[Draft RFC 0008](0008-errors-propagation-and-caused-by-wrapping.md) proposes typed errors, propagation, causes and runtime rendering. [Draft RFC 0009](0009-user-implementable-conversions.md) proposes canonical conversions and selected-error evidence.

[Draft RFC 0010](0010-diagnostics-and-reporting.md) proposes diagnostic/report adapters. It does not amend accepted expectation or capture rules.

These drafts are not accepted or included in this RFC's acceptance. Other companions remain unassigned. Source documents are supporting inputs, not normative annexes.

| Missing rule owner | Rules required for joint review | Supporting input at the source revision |
| --- | --- | --- |
| Expressions, typing and control flow | Define expression boundaries, precedence, types and enclosing-control context. Distinguish static rejection from runtime traps. | [Draft RFC 0003](0003-expressions-bindings-and-control-flow.md), not yet accepted. [Draft RFC 0005](0005-types-records-enums-and-inference.md) proposes type rules. [Draft RFC 0006](0006-numbers-strings-and-primitive-operations.md) proposes numeric/text rules, not yet accepted. [ADR 0011](../adr/0011-explicit-numeric-widths-and-utf8-string-views.md) remains supporting input. |
| Module identity and program loading | Define canonical identity, ordering and graph collection. Define positions, visibility, non-capturing declarations and startup checking. | [Draft RFC 0002](0002-modules-and-program-loading.md), not yet accepted. [ADR 0010](../adr/0010-file-modules-explicit-exports-and-privacy.md) remains supporting input. |
| Ownership, cleanup and concurrency | Define temporary and retained storage rules. Define normal and fatal cleanup, cancellation, required joins and explicit background work. | [Draft RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md) proposes ownership/cleanup, not yet accepted. Concurrency/runtime companions remain missing. [ADR 0005](../adr/0005-panics-end-the-program-without-unwinding.md), [ADR 0007](../adr/0007-explicit-mutation-with-conservative-borrow-dependencies.md) and [ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md) remain supporting inputs. |
| Diagnostics and driver reports | Define shared source identities and coordinates. Define trusted data, report destinations, limits, completion, statuses and versioned JSONL. | [Draft RFC 0010](0010-diagnostics-and-reporting.md), not yet accepted. [Diagnostic contracts](../diagnostic-contracts.md), [code registry](../diagnostic-codes.md) and [schema](../diagnostic-report.schema.json) remain source inputs. Its proposed annex adoption does not amend this RFC. |
| Standard environment and entropy | Define test-mode input acquisition and real secure-provider behavior. Do not add general deterministic or isolated execution. | [Catalogue sections 12 and 13](../standard-library-contracts.md#12-environment-and-argument-inputs). |

## Valid/invalid examples and diagnostics

The guide example is case E1. Each block below is an independent program. Do not combine rejection or fatal cases with a passing suite.

Case E2 is a complete module. The first declaration fails. After normal cleanup, execution continues to the second declaration. These outcomes exclude any additional tests in the loaded prelude.

```rill
expect false;
expect true;
```

Case E3 is a complete intended-rejection module. Both build and test checking must reject the non-Boolean expectation.

```rill
expect 1u8;
```

Case E4 is a complete intended-rejection module. The declaration is nested even if the program never calls `bad`.

```rill
function bad(): void {
  expect true;
}
```

Case E5 is a complete intended-rejection module. An expectation cannot capture a startup binding.

```rill
let startupReady = true;
expect startupReady;
```

Case E6 is a complete module. Short-circuiting skips the right operand. Execution and reporting must not evaluate the trapping division to obtain a value.

```rill
expect false && (1i32 / 0i32 == 0i32);
```

Case E7 is a complete isolated fatal-case module. Its well-typed integer division causes a panic when evaluated. This is not a Boolean-failure fixture.

```rill
expect 1i32 / 0i32 == 0i32;
```

Case E8 is a complete module. The helper returns `false`. A report cannot invent its internal operand observations.

```rill
function checkValue(): bool {
  return false;
}

expect checkValue();
```

| Case / artifact | Context | Expected outcome | Owning proposed rule | Evidence label / observed result |
| --- | --- | --- | --- | --- |
| E1, guide module | Known `i32` parameters supply widths. | Both local declarations pass. | [Declaration/checking](#declaration-and-checking-context), [Boolean outcomes](#boolean-rejected-and-fatal-outcomes). | HandChecked against these rules. No Rill execution occurred. |
| E2 | Two top-level Boolean declarations. | The first fails. Normal cleanup completes. The second executes. | [Boolean outcomes](#boolean-rejected-and-fatal-outcomes). | HandChecked. No cleanup or continuation trace was observed. |
| E3 | The expression has type `u8`. | Both modes reject it with R0101. | [Declaration/checking](#declaration-and-checking-context). | HandChecked against the code meaning. No diagnostic was emitted. |
| E4 | `expect` appears inside a function. | The compiler rejects it with R0102. | [Declaration/checking](#declaration-and-checking-context). | HandChecked. No parser or checker executed. |
| E5 | The expression uses a startup `let`. | The compiler rejects startup capture. This RFC assigns no additional code or message. | [Declaration/checking](#declaration-and-checking-context). | HandChecked against #17 and #27. Diagnostic cascades remain unspecified. |
| E6 | The left operand is false. The right operand would trap. | The expectation fails. The right operand is `notEvaluated`. No division panic occurs. | [Boolean outcomes](#boolean-rejected-and-fatal-outcomes), [observations](#result-identity-and-observations). | HandChecked against numeric and short-circuit rules. The program did not execute. |
| E7 | Execution reaches the trapping expression. | Execution ends fatally. There is no catchable false result or guaranteed report. | [Boolean outcomes](#boolean-rejected-and-fatal-outcomes). | HandChecked. Isolated execution remains planned. |
| E8 | A helper returns Boolean. | The result is false. The report does not invent helper internals. | [Observations](#result-identity-and-observations). | HandChecked. No report was generated. |
| `prototypes/21-diagnostics/expect-failures.rill` at source revision | Three false declarations use an effectful probe, a skipped call and astral text. | The probe executes once. The skipped call does not execute. Three local failures result if execution completes. | [Observations](#result-identity-and-observations), [preview limits](#preview-limits-and-opacity). | HandChecked source and authored reports. See separate Python checks below. |
| Seven-file `prototypes/07-modules/` graph at source revision | Multiple paths reach canonical modules. | Discover fifteen application declarations once, excluding loaded standard tests. Do not execute startup in test mode. | [Discovery](#discovery-and-module-identity), [execution modes](#startup-and-execution-modes). | Supporting source inventory only. Actual discovery is NotChecked. |

The following cases specify the newly confirmed policy. They are test descriptions, not complete Rill programs or observed compiler results.

| Case | Context | Expected outcome | Owning rule | Evidence |
| --- | --- | --- | --- | --- |
| C1 | A return or postfix propagation would exit the expectation. | Reject the transfer. No new diagnostic code is assigned here. | [Control flow](#control-flow). | HandChecked against the confirmed policy. Compiler checking is NotChecked. |
| C2 | A helper or nested closure handles fallible work under its own return type. | Apply ordinary callable rules. The expectation must still produce `bool`. | [Control flow](#control-flow). | HandChecked against the confirmed policy. Compiler checking is NotChecked. |
| P1 | A preview has data nested beyond four levels. | Count the root as level one. Do not traverse level five. Mark omitted data as `truncated`. | [Preview limits](#preview-limits-and-opacity). | HandChecked policy boundary. Capture is NotChecked. |
| P2 | An earlier operand uses part of the shared byte budget. | The next observation receives only the remainder of 4096 bytes. Use evaluation order. | [Preview limits](#preview-limits-and-opacity). | HandChecked policy boundary. Accounting is NotChecked. |
| P3 | An operand is skipped or a capture limit is reached. | Preserve `notEvaluated` for the skipped operand. Use `truncated` for limit-omitted data. Do not inspect omitted data. | [Observations](#result-identity-and-observations), [preview limits](#preview-limits-and-opacity). | HandChecked marker meanings. Runtime behavior is NotChecked. |

The drafting agent hand-checked E1 through E8 on 2026-11-16 against the named source rules. This was a manual rule comparison, not execution or a readiness review. The source registry fixes R0101 and R0102 meanings, not exact English diagnostics.

## Feature interactions

The ordinary checker applies in both modes. An invalid individual literal remains a static error in a skipped operand. A well-typed division by zero traps only if evaluated. Warnings cannot change these rules.

Test-only imports can contribute implementations to canonical families. Import cleanup and executable-code elimination must preserve the checked graph. An editor cannot discard expectations or change family collection when removing an apparently unused import. This does not permit editor requests to execute tests or application code. #42 owns full LSP delivery.

Test mode is not a general sandbox or deterministic runtime. `std::env` must skip actual host argument and environment capture and supply empty inputs. Tests construct synthetic inputs with `fromParts`. Secure randomness retains real provider behavior. There is no automatic seed, provider replacement or guaranteed uniqueness or nonzero output. Standard-library companion RFCs must own these API rules.

Diagnostic observation must preserve borrowing, custom disposal, protected actions, unique error causes and user-created goroutines. It cannot copy an error envelope or retain an exclusive handle for later display. It cannot delay guard release. Companion RFCs must supply the checking and lowering rules. An interface or fixture does not prove them.

### Source reconciliation

Repository inputs use commit [`f33dcf4cf3f815824eeb897135775d7884f65c1a`](https://github.com/Reilley64/rill/tree/f33dcf4cf3f815824eeb897135775d7884f65c1a). Issue links identify recorded decision comments. Mutable issue bodies supply context, not unnamed normative annexes.

| Source decision/input | Disposition in this proposal |
| --- | --- |
| [Confirmed RFC 0001 choices](https://github.com/Reilley64/rill/issues/28#issuecomment-6907785727) | Specify the [control-flow boundary](#control-flow) and [preview allocation policy](#preview-limits-and-opacity). This approves those choices, not the complete RFC. |
| [#27 confirmation](https://github.com/Reilley64/rill/issues/27#issuecomment-6852094134) | [Reference semantics](#syntax-and-reference-semantics) include the declaration, typing, execution and observation rules. The former `06-expect.rill` now belongs to `01-basics.rill`. |
| [#17 confirmation](https://github.com/Reilley64/rill/issues/17#issuecomment-6866705217), ADR 0010 | Preserve discovery, separate startup and graph checking. Canonical module identity, order and resolution still need companion rules. |
| [#18 confirmation](https://github.com/Reilley64/rill/issues/18#issuecomment-6869848603), ADR 0011 | Preserve explicit widths and the distinction between static invalidity and runtime traps. Numeric operations remain outside this RFC. |
| [#24 confirmation](https://github.com/Reilley64/rill/issues/24#issuecomment-6885970775), diagnostic contract section 5 | Preserve sequential execution, Boolean and fatal outcomes, trusted observations and preview limits. The later decision fixes root depth and shared-budget order. Bounded capture details remain authoring work. Common protocol and driver rules remain separate. |
| [#36 confirmation](https://github.com/Reilley64/rill/issues/36#issuecomment-6887766966), catalogue section 12 | Preserve empty test inputs and no startup capture. The environment API needs a companion RFC. |
| [#37 confirmation](https://github.com/Reilley64/rill/issues/37#issuecomment-6889691308), catalogue section 13 | Preserve real secure entropy and explicit application fakes. Provider validation remains separate. |
| `prototypes/01-basics.rill`, module and diagnostic groups | Preserve source and fixture bytes. The [consolidation record](../prototypes/consolidation.md) explains retired sources. [Diagnostic notes](../prototypes/21-diagnostics.md) explain historical report URIs. |
| ADR 0005 and current cleanup/concurrency contracts | Preserve fatal behavior without unwinding and ordinary cleanup obligations. Do not restore earlier reporting or timing assumptions. Detailed rules need companion RFCs. |

## Compatibility and migration

There is no released Rill implementation to migrate. The CLI's exit-2 response means testing is unavailable. Implementation requires a real checker, runner and reporter. A successful empty suite cannot replace that response.

Current examples already use top-level `expect`, semicolons and explicit widths. Use the indexes to resolve historical `06-expect.rill`, `10-modules/` and `17-diagnostics/` paths. Those numbers do not identify this RFC. Do not create duplicate old files. Preserve authored report URIs and hashes as historical evidence.

Production assertions, startup capture and duplicate imported tests conflict with the confirmed decisions. Use independent expressions, helpers and canonical discovery instead. Preserve ordinary production error handling.

## Security/resource implications

An expectation executes ordinary user code. It can cause external effects or start work that overlaps later tests. The runner does not guarantee rollback, isolation, background cancellation, deadlines or bounded cancellation latency. Tests need explicit resource ownership and controlled inputs.

Empty standard environment inputs reduce accidental secret exposure through that API. They do not sanitize explicit strings, logs, source text or previews. They do not prevent filesystem or network access. Real entropy does not become predictable for tests.

Diagnostic observation must bound its data and work. Truncation cannot change program values or turn a failure into a pass. Private and resource representations remain opaque. Reports can still expose sensitive source text or explicitly observed values. There is no automatic redaction guarantee or security proof.

Fatal allocation and overflow handling require compiler and runtime validation. Safe observation of owned or borrowed values also needs validation. A schema, manual source check or finite native experiment does not establish these guarantees.

## Rationale, alternatives and prior art

[#27](https://github.com/Reilley64/rill/issues/27) adopts Roc-style expectation notation with Rill's required semicolon. This identifies the source of the notation. It does not import Roc's module system, testing implementation or runtime rules.

An ordinary assertion function would use familiar call syntax. It would not establish module-only placement, test discovery or production exclusion. A test declaration specifies these rules directly. Production code can still use ordinary guards and errors.

Separate test functions and files can organize larger tests. Requiring them for each Boolean check would add registration and setup machinery. Helpers already let an expectation delegate complex work. This RFC adds no competing registration mechanism.

Repeated evaluation or user formatting could provide richer reports with less compiler support. It could also repeat effects or change ownership and cleanup. Trusted observations require compiler and runtime support. They can report `unavailable` instead of a value. They must preserve the tested behavior.

## Implementation implications

The Rust workspace implements CLI help, version output and unavailable-command responses only. There is no Rill parser, typed graph, expectation runner or snapshot lowering. [Development status](../development.md) records the actual checks.

The compiler must retain expectations during graph construction and checking. It must collect canonical declarations and implementations before selecting ordinary or test execution. Intermediate representations and lowering algorithms remain implementation choices. They must preserve the [confirmed control-flow boundary](#control-flow).

Observation lowering must not introduce source-visible calls or ownership dependencies. It must preserve evaluation points, short-circuit control flow, cleanup and cancellation. Optimization cannot remove an evaluated trap or evaluate a skipped expression. Return an implementation conflict to #1 and #28. Do not weaken the proposed behavior.

The driver needs source and reporting support. JSONL test output requires a destination other than stdout. Application I/O must remain raw. The driver/report RFC must own options, destination failures, limits, versions and statuses. An LLVM C-API smoke test does not test expectations.

## Evidence and checking plan

### Actual evidence

Source inputs use revision `f33dcf4cf3f815824eeb897135775d7884f65c1a`. Structural document checks are not a readiness review.

| Artifact / revision | Evidence label | Rule/question exercised | Checker/tool and command/environment | Expected / observed outcome | Limits and remaining gates |
| --- | --- | --- | --- | --- | --- |
| E1 through E8 in this draft | HandChecked | Placement, typing, startup context, short-circuiting, fatal behavior and helper observations. | The drafting agent compared the cases with source rules on 2026-11-16. | Expected outcomes agree with the in-scope source rules. | No Rill execution or readiness review occurred. Companion rules remain missing. |
| Diagnostic fixtures at source revision | ToolChecked | Authored schema, data, ranges, counts and source coordinates. | `mise run check:fixtures` on macOS 27.0.1 AArch64. Python 3.13.18 and locked jsonschema 4.25.1. | The schema, six negative data cases and five illustrative reports passed. | No compiler emitted reports or executed capture. Other TLS and SQLite checks do not validate this feature. |
| Prototype/module cases at source revision | NotChecked | Compilation, discovery, module deduplication and execution. | No relevant implementation is available. | Expected outcomes appear above. No Rill outcome was observed. | `test:language` is unavailable. |
| Ownership-safe bounded observations | NotChecked | Source retention, cleanup, masking, budgets and unchanged program behavior. | No compiler or runtime implementation is available. | No implementation result is claimed. | Requires ownership and checker rules, then executable backend and runtime tests under #28. |

### Planned checks. Not executed evidence

- Parse complete modules. Reject nested declarations and missing semicolons. Check non-Boolean and startup-capture cases in both modes. Include unused functions and skipped operands.
- Construct graphs with aliases, re-exports, cycles, test-only imports and unrelated neighboring files. Check canonical order, deduplication and unchanged family collection. Check that startup does not execute. Include loaded standard-module expectations in totals.
- Compare effects with and without observation. Include calls, short-circuiting, user comparisons, formatting, copying, iteration and disposal. Include generic and erased interfaces. Observation must add no calls, requirements or resource-lifetime changes.
- Exercise owned values, exclusive references, opaque types, guard/native dependencies, unique causes and remaining-field cleanup. Check continuation after false and normal joins and cancellation. Do not add a global join or early release.
- Test depth, child and byte limits with many operands, cycles and long text or bytes. Include integer widths, signed zero and nonfinite floats. Check markers and bounded capture before full traversal or allocation.
- Run fatal arithmetic, panic, overflow and exhaustion cases separately. Check non-success and absence of false recovery. Do not require a report after fatal termination.
- Check empty suites and identical expression text at different sites. Check report limits, omissions, source identity and dedicated JSONL destinations. Distinguish rejection, incomplete runs and complete Boolean results. Preserve raw program I/O.
- Check empty standard inputs through actual acquisition paths. Check explicit synthetic inputs. Test secure providers separately. Do not infer deterministic bytes or security from sample outputs.

## Open/deferred questions

### In-scope semantic blockers

1. Supply the required companion RFCs. Identify exact sections and revisions for joint review. Supporting contracts do not replace rule owners.
2. Complete the bounded capture specification under the approved policy. Specify byte charging and finite metadata, cycle-tracking, work and storage bounds. Reconcile these details with the companion report rules.

The user resolved the control-flow boundary, root level and shared-budget order in the [recorded decision](https://github.com/Reilley64/rill/issues/28#issuecomment-6907785727). Those policy questions are no longer open. The remaining items are authoring work, not requests for another decision on those choices.

These items prevent ReviewReady. A proposed change to a confirmed guarantee needs design review through #1 and #28.

### Implementation/validation gates

The parser, checker, graph, ownership model, runner, trusted observation and report emitter remain unimplemented. Their execution and target tests have not passed. #28 retains formalization and production acceptance. #42 retains full applicable LSP delivery. It is not a prerequisite for drafting.

Historical #30 mechanism experiments are supporting evidence only. They do not implement expectations or certify backend support.

### Future/out-of-scope work

This RFC adds no registration API, automatic parallel testing, retries, panic-catching assertions or setup/teardown hooks. It adds no implicit isolation or IDE test integration. These exclusions do not weaken ordinary cleanup, effects or reporting requirements.

## Decision and change history

### Readiness record

The project owner [accepted the published snapshot](https://github.com/Reilley64/rill/issues/28#issuecomment-6907867211) before completion of the readiness checklist. Status is Accepted. The drafting agent has not performed a canonical readiness review.

Source comparisons, manual case checks and structural validation do not complete that review. Missing dependencies and capture details remain open under the [canonical checklist](README.md#canonical-reviewready-checklist). Acceptance does not complete implementation or validation. Material additions require amendment review. This decision does not accept unwritten text.

### Human decisions and amendments

| Date / actor | Kind and scope of change | Reviewed revision or snapshot | Decision link / rationale |
| --- | --- | --- | --- |
| 2026-11-16, drafting agent under #28 | Initial substantive draft. Registered as RFC 0001. No source-rule departure or acceptance is claimed. | Inputs use `f33dcf4cf3f815824eeb897135775d7884f65c1a`. The RFC is not yet reviewed. | The user authorized RFC drafting. Source approvals do not accept this document. |
| 2026-11-16, drafting agent under #28 | Revised the prose to use Simplified Technical English. Preserved rules and code examples. | Editorial revision of the local draft. No readiness review or dictionary compliance audit occurred. | The user required this style for all RFCs. Formal ASD-STE100 compliance is not claimed. |
| 2026-11-16, project owner | Approved the control-flow boundary and preview-budget policy. This does not accept the complete RFC. | The two recommendations presented in chat. No complete RFC revision was accepted. | [Recorded agreement](https://github.com/Reilley64/rill/issues/28#issuecomment-6907785727). Status remains Draft. |
| 2026-11-16, project owner | Accepted RFC 0001 with its recorded open work. This does not complete readiness or implementation checks. | [Published pre-status-update snapshot](https://github.com/Reilley64/rill/issues/28#issuecomment-6907867211). SHA-256: `ee4980216dd2ed051cb6153600bf8b5f6c039145c083abfeeb37119d027bd954`. | The user stated: "ok I approve the rfc". Status is Accepted. Future material additions need amendment review. |
| 2026-11-16, drafting agent under #28 | Added an editorial reference to Draft RFC 0002. No accepted semantic rule changed. | The accepted snapshot remains unchanged. RFC 0002 is a separate proposal. | Linking the module draft does not accept its rules. |
| 2026-11-16, drafting agent under #28 | Added an editorial reference to Draft RFC 0003. No accepted semantic rule changed. | The accepted snapshot remains unchanged. RFC 0003 is a separate proposal. | Linking the expression draft does not accept its rules. |
