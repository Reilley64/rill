# Diagnostic, report and LSP contract examples

Read the source examples in [`prototypes/21-diagnostics`](../../prototypes/21-diagnostics).

These examples follow confirmed [#24](https://github.com/Reilley64/rill/issues/24), [ADR 0016](../adr/0016-structured-diagnostics-and-complete-openidea-lsp.md), the [report contracts/schema](../diagnostic-contracts.md) and the [LSP matrix](../lsp-contracts.md). The [batch-report program](10-batch-report.md) is a separate application example.

No Rill compiler emitted these reports, and no example ran as Rill. The report files are hand-authored protocol examples. Mechanical checks cover only coordinates, hashes and ordinary data calculations. There is no Rill compiler, runtime or server yet. [#42](https://github.com/Reilley64/rill/issues/42) owns full-scope LSP delivery and acceptance; #28/#30 retain the requirements for checker and backend evidence. Static checks do not prove inference, borrow safety, snapshot noninterference or client behavior.

## Files and intended outcomes

| File | Intended status, not execution evidence |
| --- | --- |
| `valid.rill` | 8 intended-valid, intended-passing expectations cover explicit borrowed generic copy, enums, a named record, short-circuit calls, minimum i8, a shift and owned UTF-8. |
| `expect-failures.rill` | Source-valid with 3 intentionally false expectations. probe is called once, skipped is never called, and an ordinary false result permits execution to continue. |
| `locations.rill` | Intentional CRLF/leading tab/astral text: 2 source-valid expectations, one false; not part of the passing fixture |
| `rejections/*.rill` | 9 separate intended-rejection programs; never concatenate them and claim compiler execution |
| `check-report.jsonl` / `.txt` | Illustrative one-use-after-move report and completed rejected front-end check |
| `expect-report.jsonl` | Illustrative 3 failed results, short-circuit marker and summary; not recorded runtime values/logs |
| `tool-failure-report.jsonl` | Controlled internal failure, untrusted/incomplete analysis, no fake source error or passed check |
| `empty.rill` / `empty-suite-report.jsonl` | Zero local expectations; illustrative zero-test program assumes no expectations in other loaded/prelude modules |
| `warning.rill` / `warning-report.jsonl` | Source-valid unreachable executable initializer; warning-only front-end success, not Werror |
| `coordinate-map.json` | Exact original byte/human scalar/LSP UTF-16 ranges for the two astral string tokens |
| `check-fixtures.py` | Python JSON/data/source-coordinate checks only, not Rill implementation |

Report counts describe the authored fixture context, not an assertion about an eventual toolchain's additional loaded standard-module expectations. Source declaration counts are local to the named files.

Reports retain their original fictitious prefix, `file:///workspace/rill/prototypes/17-diagnostics/`. The checker resolves each suffix relative to its current 21-diagnostics directory. Renumbering changes filesystem paths, not the authored reports, source hashes or coordinates. Real integration must use properly encoded host/client URIs rather than copying fixture paths into an editor configuration.

### Rejection contract inventory

| Independent file | Primary intended kind |
| --- | --- |
| `use-after-move.rill` | R0200: general owned string used after consume; no singleton/literal copy exception |
| `missing-width.rill` | R0003: no annotation/suffix/known numeric context |
| `non-bool-expect.rill` | R0101: no truthiness |
| `nested-expect.rill` | R0102: module top level only |
| `nominal-mismatch.rill` | R0100: matching fields do not equate Left/Right |
| `borrowed-spawn.rill` | R0302: owned spawn environment contains a borrow; later stop does not waive noBorrow |
| `incomparable-overloads.rill` | R0303: different specific parameter positions, not a return/constraint tie-break |
| `duplicate-body.rill` | R0304: renamed type parameter does not distinguish bodies, even with no calls |
| `protected-overlap.rill` | R0500: genuine guard release cannot be displaced through nested Payload ownership |

The later confirmed [#41](https://github.com/Reilley64/rill/issues/41#issuecomment-6886119679) requires implement / implement<T> name for family bodies. The 3 affected rejection files use that syntax. Standalone functions and declarations without bodies retain function. The ordinary nested-payload orphan permission remains relevant to protected-overlap; conversion-specific endpoint permission does not replace it.

Future conversion cases must cover duplicate detection, overlap and orphan selection for canonical (To, From) pairs. They must also cover protected numeric/identity behavior, camelCase aliases and selected symbolic E equality without choosing a fallback by its error type. No new conversion implementation is executed here.

Additional compiler messages/cascades are not asserted absent. Exact source/rule checking is future #28 work. The registry freezes problem meanings, not these precise English strings. Removed literal types/unions, visibility/import ambiguity, closed numeric availability, inner-cause display/noBorrow/sendable/shareable, noncapturing named functions and unsupported eligible-tail lowering also need full later suites; these 9 programs are representative, not exhaustive coverage.

## Reproduce the static checks

Use an isolated Python validator environment without changing project dependencies or the lockfile:

```sh
uv run --no-project --with jsonschema==4.25.1 python prototypes/21-diagnostics/check-fixtures.py
```

The validator checks the Draft 2020-12 schema, typed records and negative data mutations. It also checks lossless decimal fields, sequence numbers and counters, snapshot hashes, original UTF-8 locations and token substrings. It independently recomputes Unicode-scalar and UTF-16 mappings. For Rill source, it checks only declaration counts, rejection headers and delimiters. It does not parse or type-check Rill or implement an emitter. The JSONL records were authored from the contract. No final summary in a real stream means completion was not observed; an external tool cannot infer success from the last diagnostic alone.

## Required future expectation-observation tests

1. Compare callback/log counters with and without instrumentation. The original call or comparison must run exactly once, skipped branches must remain uncalled, and instrumentation must not retry. `probe()` has one log effect by inspection; no log trace was observed here.
2. Use custom show/display/copy/iterator/disposer bodies that increment counters or panic. Reporting must not call them. An explicit such operation inside the original expression still runs normally once. Generic expectations acquire no added formatting/copy requirements.
3. Test owned/noncopyable values, borrowed text extents, exclusive handles, private/opaque records, unique Err/Cause and remaining initialized fields. Observation must not move an owner, retain a user loan, unlock early or late, or add disposal. Current #23 copy(&value) and retained-cause boundaries apply.
4. Take a snapshot before releasing or mutating the original value where safe. Otherwise, record an unavailable marker. Do not keep the source alive just to print it after failure. Never inspect Mutex/Shared/guard/native internals or lock a value for diagnostics.
5. Test deep, cyclic and huge aggregates, many Boolean operands, long text/bytes and type names. Bound observation, work and storage before capturing the whole graph. Defaults are 4 levels, 8 children and a shared limit of 4096 rendered bytes. Distinguish truncated, unavailable and not-evaluated values explicitly. Do not expose addresses, sort whole maps or produce unbounded diffs. Ordinary captures can be truncated; standard runtime explicit cause-chain rendering remains full and separate.
6. Test float widths, signed zero, NaN, i128/u128 extrema, UTF-8 boundaries, combining/astral text and invalid bytes. Previews must describe exact typed values without f64 coercion or invalid JSON nonfinite numbers. Literal minima remain syntactically valid; the formatter must not change their spelling to invalid syntax.
7. A false expectation continues after normal cleanup. Actual panic, overflow or required-worklist allocation failure remains fatal, without unwinding or a guaranteed final report. Run fatal cases in separate processes/instances, not as assertions inside the passing fixture or through a new panic-catching helper.
8. Test canonical module/test dedup, deterministic source order, duplicate expression text at different sites, empty suites and the absence of startup execution. There is no hidden global join, background isolation or filesystem rollback. Deliberately parallel user work retains ordinary effects and has no hard latency promise.

## Required future client/driver tests

The [complete LSP matrix](../lsp-contracts.md#7-versioned-acceptance-matrix) is NOT RUN throughout. Test stale pull/push/resolve responses across dependency, context, open-close and restart races. Also test rich-message fallback, eager ranges/hints, hierarchy source re-preparation, alias/coupled-namespace rename, graph-preserving imports, meaningful refactor workflows and adverse non-atomic edits. Wiring a menu to source code or mocking an RPC acknowledgment does not clear #42.

Test human stderr separately from the clean JSONL sink, and verify that raw application stdout/stderr remain unchanged. Distinguish run's build-phase summary from application exit. Cover invalid/report-I/O/internal/cancel statuses, partial output without a summary, source/destination alias refusal, omitted diagnostics/counters and color/control-character safety. check/LSP must not automatically execute tools or user code. These tests add no manifest/notebook/Windows/Run-Test-DAP scope, and front-end success does not certify a backend.
