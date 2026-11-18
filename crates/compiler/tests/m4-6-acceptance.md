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

## Completion boundary

`GenericReport` exposes partial arguments, captures, body and node facts, and issued goals. An ordinary result type or proof outcome under a hypothetical role does not supply a checked call. An output that remains open in an independent determination stays open there even if source context can fill a free slot and form separate requirements at that use.

The source checker still leaves some M4.6 paths incomplete. It does not pass a free output filled by source context back into a waiting determination chain that needs it as a new input. Ordinary callback projection covers individual arguments and direct tuple element paths, not arbitrary nested tuple paths. Distinct symbolic callable contracts with the same arity stay pending when they do not agree on one signature. These limits are not errors in the source language and are not evidence of absence. This record is not full M4.6 acceptance.

M4.7 owns conversion applicability and conversion-selected error types. M4.8 owns actual core and protection admission, integrated List/index/Option/Result/Err and iterator contracts, remaining visibility and aggregate prerequisites, and result use. No Rill program ran. No native runtime, hosted profiles, LLVM backend or product acceptance was verified for this revision. Workflows did not change, so `check:ci` was not run. No ownership, storage, copy execution, loan, cleanup or release gate is claimed.
