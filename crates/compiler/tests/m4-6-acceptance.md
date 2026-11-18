# M4.6 generic inference and symbolic evidence

This record covers [#63](https://github.com/Reilley64/rill/issues/63) in [draft PR #58](https://github.com/Reilley64/rill/pull/58), under [M4 #56](https://github.com/Reilley64/rill/issues/56). The owner [approved H1 through H4](https://github.com/Reilley64/rill/issues/63#issuecomment-6964769605) at `92d8454b65b5ee59760583178feaa5ce8bd199ff` before model or compiler implementation. The [design checkpoint](../../../docs/implementation-plan.md#m46-design-checkpoint) links the owning procedures. Their RFC statuses remain Draft.

## Independent model

`models/m4_6_inference.py` models readiness, correlated input projections, output equality and proof dependencies. Its tests compare finite projections with ground enumeration and compare proof outcomes with a least-closure oracle over all three-node dependency graphs. They also check permutations of default order, callback paths, filters, frozen assumptions, cycles and growth limits. The first run failed on the missing model module. Sixteen M4.6 model tests passed before compiler implementation at `58f04d492a742d99fb84960cd899a1a3355e0cf4`. They supply no core admission and did not execute Rill.

```sh
mise exec -- python -B -m unittest discover -s crates/compiler/tests/models -p test_m4_6_inference.py
```

## Public compiler assertions

The Rust tests use real M3 captures. They query the generic report, not checker internals. The fixtures use only the existing loader and prelude schema requirements. They add no trusted providers or role catalogue.

| File | Tests | Observations |
| --- | ---: | --- |
| `m4_6.rs` | 19 | Rigid bodies, independent anchors before written defaults, conditional ordinary arguments, uniform winner selection, exact outputs and assumption filters. Borrowed-map and tuple-join shapes use their own callback contexts. Expected results do not change the winner. |
| `m4_6_nominal.rs` | 16 | Shared substitutions for fields and payloads, defining-module defaults, value versus annotation anchors, rebuild identity, private members, nested default readiness, and inline-cycle witnesses after default expansion. |
| `m4_6_callables.rs` | 14 | Arrow signatures, block return targets, standalone function values, invocation through descriptors, per-capture paths and demands, and default readiness across nested calls and arrows. An unknown mode does not supply a checked invocation. |
| `m4_6_evidence.rs` | 16 | Symbolic callable assumptions, exact predicate evidence, capture versus projected storage, output-producing dependencies, active cycles versus growth, selected error equality, no fallback, canonical routes and bounded goal IDs. |

```sh
mise exec -- cargo test --locked -p compiler --no-default-features \
  --test m4_6 --test m4_6_nominal --test m4_6_callables --test m4_6_evidence
```

## Independent reviews and corrections

Both final reviews examined the exact code at `414c5505507986976119484c87cdc5132f42ce92` against the approved checkpoint. The diff covered 26 files with 9,634 additions and 216 deletions. Its SHA-256 was `584a15647eb900e10e4edb96f6f56af4971f54f16db5753b27d525042b478dbc`.

| Axis | Report and disposition |
| --- | --- |
| Standards | Arrow parameter mismatches and return coverage lost the ordinary premise. The extended declaration inventory dropped layout issues. The arrow context cloned an uncharged loop stack. Corrections preserve conditional diagnostics, expose retained declaration issues, and initialize arrow loops empty. The two identical mode checks now share one predicate. The reviewer ran 312 compiler tests, 16 M4.6 model tests, temporary public probes and an LLVM-free dependency check. |
| Spec | A borrowed field projection could falsely prove `noBorrow`. Open selected outputs did not follow their determining clauses. Rigid callable assumptions and flexible arrow clauses were not applied, and nested nominal receivers blocked ready defaults. Corrections retain captured storage, visit output dependencies before proof, use canonical callable clauses, and track nested mandatory reads. The reviewer ran all four required mise tasks, 16 M4.6 model tests and temporary public probes. |

The reviewers removed their probes. Local regressions reproduced the carried-reference proof, missing output, nested nominal default and callable-clause readiness failures before correction. The final tests also cover the conditional diagnostic and retained layout findings. The post-review changes were verified locally, not independently re-reviewed.

## Verified revision

Code revision [`f4765b82116fc29d45e392f89a3afbce7d7966e9`](https://github.com/Reilley64/rill/commit/f4765b82116fc29d45e392f89a3afbce7d7966e9) passed these checks on a clean macOS AArch64 worktree. The code diff from the approved checkpoint had SHA-256 `aa55499d5514df3f3bb3f4b16ea964695429fcbb1416b6887cc57cefebf7364c`.

| Check | Result |
| --- | --- |
| `mise run test:compiler` | 321 tests passed, including 65 M4.6 tests and 256 unchanged earlier tests. |
| `mise run test` | 329 tests passed across the task, including the compiler suite. |
| `mise run check` | Passed, with zero errors across 1,714 documentation destinations at the code revision. |
| `mise run build:scaffold` | Passed. |
| Independent Python models | 41 passed, including all 16 M4.6 tests. |
| Formatting, LLVM-free Clippy, dependency tree and diff | Passed. The LLVM-free compiler dependency tree contained only `compiler`. |

## Completion increment

The [previous publication](https://github.com/Reilley64/rill/issues/63#issuecomment-6966488734) left three procedures incomplete. The source checker now:

- Resumes selected output dependencies after source context or a ready default binds a free slot. A separate `Instantiate` goal records the use, its dependencies and defaults. The input-only `Determine` goal and its winner do not change. A pending output still cannot be bound by the source.
- Commits bracket category and tests arity before projecting needed callback contexts at nested tuple paths. It uses independent anchors from typed siblings and retains disagreement without selecting a winner.
- Matches symbolic callable assumptions against the argument vector. It supplies only agreed parameter contexts, rejects a settled mismatch, and does not use the expected result to select an assumption.

The new public tests failed first on missing output re-entry, missing nested callback context and unresolved same-arity invocations. They also cover dependent defaults, contextual family values and ambiguity despite an expected result.

Independent Standards and Spec reviews examined `2f5a21264640c23212ec767bedc26730e629895e` against `5a9b07f9b090ad029dae9892bad979c73c1589ef`: eight files, 946 additions and 339 deletions, SHA-256 `ae2d0f2bb7e171ae2bc608b4507e126d7e99859e2a7250eae84cbf31bddf674e`. Both ran 327 compiler tests and public probes. Standards also ran the documentation and scaffold checks, the full model suite and the LLVM-free dependency check. Spec ran the 16 M4.6 model tests.

| Axis | Findings and corrections |
| --- | --- |
| Standards | Exhaustion could expose hypothetical arity as an unconditional finding. Read-only invocation could lose the arrow's premise, and child projection could exceed its depth limit before checking it. The checker now emits conditional arity directly, discards unclassified findings on exhaustion, preserves read-only premises and checks depth before child projection. The suggested key-comparison cleanup was not required for correctness. |
| Spec | A bad nested arity could still produce a hypothetical winner. A compound default could bind just part of its term. A nested anchor was ignored, a settled symbolic mismatch stayed pending, and a cycle blocked unrelated output work. The checker now forms a tuple from its checked elements, applies only complete ready defaults, reads nested declared equations, reports settled invocation mismatches and resumes independent output work without resolving the cycle. |

The reviewers removed their temporary probes. The corrections and their public regressions were verified locally, not independently re-reviewed.

### Completion verification

Code revision `f7f96f71285ebb649da2837506cd48d071ec9967` passed on a clean macOS AArch64 worktree. Its diff from the partial publication has SHA-256 `be7a1dc06893eb75dd6316eab0b98dc628720dbf1bc6b4098f5df4ec8520fc60`.

| Check | Result |
| --- | --- |
| `mise run test:compiler` | 335 passed: 79 M4.6 tests (25 inference, 16 nominal, 14 callable, 24 evidence) and 256 earlier tests. |
| `mise run test` | 343 passed, including the compiler suite. |
| `mise run check` | Passed; zero errors across 1,716 documentation destinations at the code revision. |
| `mise run build:scaffold` | Passed. |
| Independent Python models | 41 passed, including all 16 M4.6 tests. |
| Formatting, LLVM-free Clippy, dependency tree and diff | Passed. The LLVM-free compiler dependency tree contained only `compiler`. |

## Completion boundary

`GenericReport` exposes partial arguments, captures, body and node facts, and issued goals. An ordinary result type or proof outcome under a hypothetical role does not supply a checked call. An output that remains open in an independent determination stays open there even if source context can fill a free slot and form separate requirements at that use.

This completes the approved M4.6 compiler slice. Unresolved contracts, ambiguous symbolic invocations and active cycles remain pending. Neither this slice nor its tests grant role admission or whole-source acceptance.

M4.7 owns conversion applicability and conversion-selected error types. M4.8 owns actual core and protection admission, integrated List/index/Option/Result/Err and iterator contracts, remaining visibility and aggregate prerequisites, and result use. No Rill program ran. No native runtime, hosted profiles, LLVM backend or product acceptance was verified for this revision. Workflows did not change, so `check:ci` was not run. No ownership, storage, copy execution, loan, cleanup or release gate is claimed.
