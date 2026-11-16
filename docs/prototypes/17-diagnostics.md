# Diagnostic, report and LSP contract examples

Read the source examples in [`prototypes/17-diagnostics`](../../prototypes/17-diagnostics).

Confirmed [#24](https://github.com/Reilley64/rill/issues/24), [ADR 0016](../adr/0016-structured-diagnostics-and-complete-openidea-lsp.md), [report contracts/schema](../diagnostic-contracts.md) and [LSP matrix](../lsp-contracts.md). The [batch-report program](16-batch-report.md) is a separate application example.

**Nothing here was emitted by a Rill compiler or executed as Rill.** The report files are hand-authored protocol examples; only coordinates/hashes/ordinary data calculations are mechanically checked. There is no Rill compiler/runtime/server yet. [#42](https://github.com/Reilley64/rill/issues/42) owns actual full-scope LSP delivery/acceptance; #28/#30 retain checker/backend evidence. Static checks are not inference, borrow safety, snapshot noninterference or client proof.

## Files and intended outcomes

| File | Intended status, not execution evidence |
| --- | --- |
| `valid.rill` | 8 intended-valid, intended-passing expectations: explicit borrowed generic copy, enums, named record, short-circuit calls, minimum i8, shift and owned UTF-8 |
| `expect-failures.rill` | Source-valid with 3 intentionally false expectations; probe called once, skipped never called, ordinary false continues |
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

All report URIs use the **fictitious normalized fixture prefix** `file:///workspace/rill/`. The checker maps it to this repo for byte/hash checks. Real integration must use real properly encoded host/client URIs, not copy these paths into an editor configuration.

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

Later confirmed [#41](https://github.com/Reilley64/rill/issues/41#issuecomment-6886119679) requires implement / implement<T> name for family bodies. The 3 affected rejection files use that syntax; standalone functions and body-less declarations retain function. The ordinary nested-payload orphan permission remains relevant to protected-overlap; conversion-specific endpoint permission does not replace it. Future conversion cases must cover canonical (To, From) duplicate/overlap/orphan selection, protected numeric/identity behavior, camelCase aliases and selected symbolic E equality without fallback-by-error selection. No new conversion implementation is executed here.

Additional compiler messages/cascades are not asserted absent. Exact source/rule checking is future #28 work. The registry freezes problem meanings, not these precise English strings. Removed literal types/unions, visibility/import ambiguity, closed numeric availability, inner-cause display/noBorrow/sendable/shareable, noncapturing named functions and unsupported eligible-tail lowering also need full later suites; these 9 programs are representative, not exhaustive coverage.

## Reproduce the static checks

Use an isolated Python validator environment (no project dependency or lockfile change):

```sh
uv run --no-project --with jsonschema==4.25.1 python prototypes/17-diagnostics/check-fixtures.py
```

This validates the Draft 2020-12 schema, typed records/negative data mutations, lossless decimal fields, sequence/counters, snapshot hashes/original UTF-8 locations, token substrings and independently recomputed Unicode-scalar/UTF-16 mappings. It checks declaration counts, rejection headers and delimiters only; it does not parse/type-check Rill or implement an emitter. The JSONL records are explicitly authored from the contract. No final summary in a real stream means completion was not observed; an external tool cannot infer success from the last diagnostic alone.

## Required future expectation-observation tests

1. Compare callback/log counters with and without instrumentation; exactly one original call/comparison, skipped branches uncalled, no retries. `probe()` has one log effect by inspection; no log trace was observed here.
2. Custom show/display/copy/iterator/disposer bodies increment counters or panic: reporting must not call them. An explicit such operation inside the original expression still runs normally once. Generic expectations acquire no added formatting/copy requirements.
3. Owned/noncopyable values, borrowed text extents, exclusive handles, private/opaque records, unique Err/Cause and remaining initialized fields: no moved owner, retained user loan, early/late unlock or extra disposal. Current #23 copy(&value) and retained-cause boundaries control.
4. Snapshot before original value release/mutation where safe; unavailable marker where not. Do not keep the source alive just to print it after failure. Never peek through Mutex/Shared/guard/native internals or lock a value for diagnostics.
5. Deep/cyclic/huge aggregates, many Boolean operands, long text/bytes and type names: bound observation/work/storage before whole-graph capture; 4 levels/8 children/shared 4096 rendered bytes by default. Explicit truncated/unavailable/not-evaluated distinctions; no addresses, whole-map sorting or unbounded diff. Ordinary captures can be truncated; standard runtime explicit cause-chain rendering remains full and separate.
6. Float widths/signed zero/NaN, i128/u128 extrema, UTF-8 boundaries/combining/astral text and invalid bytes: descriptive exact typed previews, no f64 coercion or invalid JSON nonfinite numbers. Literal minima remain syntactically valid; formatter must not change literal spelling to invalid syntax.
7. False continues after normal cleanup; actual panic/overflow/required-worklist allocation failure remains fatal without unwind/guaranteed final report. Fatal cases are separate processes/instances, not assertions inside the passing fixture or a new panic-catching helper.
8. Canonical module/test dedup, deterministic source order, duplicate expression text at different sites, empty suites, startup not executed, no hidden global join/background isolation/FS rollback. Deliberately parallel user work keeps ordinary effects and no hard latency promise.

## Required future client/driver tests

The [complete LSP matrix](../lsp-contracts.md#7-versioned-acceptance-matrix) is **NOT RUN** throughout. In particular test stale pull/push/resolve across dependency/context/open-close/restart races, rich-message fallback, eager ranges/hints, hierarchy source re-preparation, alias/coupled-namespace rename, graph-preserving imports, meaningful refactor workflows and adverse non-atomic edits. No source-wired menu or mocked RPC acknowledgment clears #42.

Test human stderr versus clean JSONL sink, raw app stdout/stderr unchanged, run build-phase summary versus application exit, invalid/report-I/O/internal/cancel statuses, partial output without a summary, source/destination alias refusal, omitted diagnostics/counters and color/control-character safety. No automatic tool/user-code execution for check/LSP, no new manifest/notebook/Windows/Run-Test-DAP scope, and no backend certification from front-end success.
