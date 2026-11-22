# M4.7 conversion analysis

The owner approved V1 through V4 (ri-lqn, comment 6967953617) at `f0b261a887dec3bf6252c40da928335e29bfc602`, including exact source capture for a binder fixed by the destination. [RFC 0009](../../../docs/rfcs/0009-user-implementable-conversions.md#proposed-m47-conversion-checking-procedure) owns this procedure. Issue ri-lqn remains part of M4 ri-b83 in [draft PR #58](https://github.com/Reilley64/rill/pull/58). RFC 0009 remains Draft.

## Model and public tests

The eight Python model tests passed before the Rust implementation at model revision `f2c1b8c457ec43948641c73644555adbfca9def8`. The oracles enumerate ground pairs, possible completions of unresolved applicability states, and closed sets for error-determination routes. Separate assertions cover exact source filters, the identity gate, and unclassified clauses. The model does not interpret Rill source or establish core admission.

The 15 public tests in `m4_7.rs` use real M3 captures and these interfaces:

| Interface | Observations |
| --- | --- |
| `analyze_conversions`, `family` | Captured export identities, alias routes, pair keys, exact signatures, destination ownership, duplicate/protected registrations, and error-determination routes. An invalid hypothesized family signature remains a registration issue. |
| `query_pair`, `goal` | Exact destinations, fixed outer source admission, destination-bound variable capture, receiver filters, protected identity, and the distinction between a missing numeric provider and a specified gap. An unrelated pending clause does not hide a disproved candidate. A possibly maximal pending candidate blocks selection. A separate error demand does not affect the winner. |
| `bodies`, `conversion_use`, `conditional_conversion_node` | Immediate destination and explicit source context, no guessed literal width, diagnostics in unused implementation bodies, and the selected callable descriptor without parameter variance. Exact selected errors feed waiting callers, including requirements on standalone and ordinary calls. Conditional node and ordinary selection facts retain their premises; an independent arithmetic error stays unconditional. |
| `goals` | Nested error determination, cycles versus growth exhaustion, rigid relations versus implementation selection, and filter equivalence per candidate. A disproved partial specialization does not stop a uniform result. Foreign identities and goals from another report do not supply facts. |

```sh
mise exec -- python -B -m unittest discover -s crates/compiler/tests/models -p test_m4_7_conversions.py
mise exec -- cargo test --locked -p compiler --no-default-features --test m4_7
```

## Independent code reviews and corrections

Standards and Spec independently reviewed `846bdcf3e735a06e0e426c785a97e09882a8fb57` against the approved checkpoint. The 24-file diff had 3,561 additions, 109 deletions, and SHA-256 `fce9e1c279e5101e12d4f1ee4a6a51e0c86d98fffcba2a9eb73f97c1cce54e5f`.

| Axis | Findings and disposition |
| --- | --- |
| Standards | An ordinary call could lose an input's conversion premise. A unary role hypothesis crashed a family requirement. Inventory preflight and final pending-use scans missed work charges. The corrections combine premises, guard role shape, charge both scans, share protected-pair classification, and name the waiting error term separately from the source filter. The reviewer ran all four mise tasks, the eight M4.7 models, and public probes. |
| Spec | Reproduced the invalid-role crash. An open standalone caller error could not use a conversion requirement. A disproved partial specialization still blocked a uniform result. A rigid error demand blocked a ground pair query. A sibling conversion could qualify an independent arithmetic error. The corrections keep the pair and error demand separate, follow ready conversion-output equations, test partial intersections for disproof, and track which source nodes depend on a conversion premise. The reviewer ran all four mise tasks, workspace tests, the 49-model suite, and public probes. |

The reviewers removed their probes. Local regressions reproduced the reported failures before correction, including the lost ordinary premise. The post-review code was verified locally. No second independent review is claimed.

## Verified code

Code revision [`0b388175bfd8482645df5366904133df47008750`](https://github.com/Reilley64/rill/commit/0b388175bfd8482645df5366904133df47008750) passed on a clean macOS 27.0.1 AArch64 worktree. The code diff from the approved checkpoint has SHA-256 `2e71b5b6f1a7eafcd56e38ab48854d93c75d229035300c325015dfcf4d0cc8e2`. The correction diff from the reviewed revision has SHA-256 `37999a728994a34def46e22c9a2be8186c81015f8102a22ea7df752929880b4b`.

| Check | Result |
| --- | --- |
| `mise run test:compiler` | 350 passed, including 15 M4.7 tests. |
| `mise run test` | 358 passed, including the compiler suite. |
| `mise run check` | Passed. 1,740 documentation destinations, zero errors at the code revision. |
| `mise run build:scaffold` | Passed. |
| Python models | 49 passed, including eight M4.7 models. |
| Formatting, LLVM-free Clippy, dependency tree and diff | Passed. The LLVM-free compiler dependency tree contained only `compiler`. |

## Completion boundary

The report carries role hypotheses. It does not admit a core role, provider, Result representation, or protected operation. Pair comparison cannot issue a checked call or prove unconditional absence. A selected error can feed a conditional equation in the current expression, but that does not establish result use through named storage or aggregates. Active cycles and exhausted work cannot supply an error or authority.

This completes the scoped M4.7 compiler analysis. M4.8 owns actual core admission and typed-stage completion, including integrated core operations and result use. The tests did not execute Rill. This record does not verify hosted runners, native runtime support, LLVM, ownership, or product acceptance. Workflows did not change, so `check:ci` was not run.
