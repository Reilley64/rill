# ri-2s0.5 (R-O1a): RFC 0007 O1 and O6, 2026-11-23

City bead `ci-vca2`, branch `m5-ownership-cleanup/r-o1a-ri-2s0.5`, base `7002855e`. The compiler judges O1 and O6 over MIR. It does not execute them: the expectation values (3) are not observed (M6).

## What changed

- **Typed (M4), `typed/result_use.rs`, Assignment arm.** A `+=` through a plain exclusive dereference whose target type is `i32` is admitted. It is screened from the node's own facts (the checked fact, operator, `[Dereference]` path and the target type) before any charge. `-=`, `*=`, `=`, other primitive types, shared `&i32` and record fields keep `pending Storage`. The `boolean_storage` exception in `typed/result_use/calls.rs` already covers every `TypeTerm::Primitive` referent, so it did not need widening: `&mut i32` and `&mut moved.x` actuals leave no `Storage` blocker today.
- **Ownership (M5).** `Action::InstallScalarThrough` (`ownership.rs`, `flow.rs`): store a scratch `i32` through an exclusive incoming descriptor, checked with `Access::Exclusive`, ending the scratch. It publishes the existing `Operation::InstallThrough`. `add_through_exclusive_i32` (`build/i32_arithmetic.rs`) lowers `*parameter += literal` as dereference read, literal, `CheckedI32Add`, scratch ends, install. It admits only a bare `*name += literal` on an `&mut i32` parameter, screened from written shape and paid-for facts (`value()` reads) before the first charge.
- The reborrowing call (`change(value)`) and the `&mut moved.x` field actual already lowered on the base (measured: `callNoop`, `reborrow`, `fieldArg` in a probe at the parent). No lowering was added for them.
- **`call().field == i32`.** `scalars.rs` classifies a field written on a call result, through groups, as an `i32_equality` operand. `call_field_operand` in `build/i32_arithmetic.rs` lowers it through the existing `temporary_projection` and ends the projected call result after the comparison.

## Tests (`m5/rfc_cases.rs`), all green

Run with `cargo test --locked --no-default-features -p compiler --test m5 -- rfc_cases` (31 passed). On a clean worktree of the parent the six new or changed tests fail and `o01_unadmitted_exclusive_writes_keep_their_parent_caps` passes (log `red-on-parent.log`).

- `o01_shifted_point_checks_through_exclusive_deref_storage` and `o06_reborrowing_calls_check_through_exclusive_deref_storage`: typed `Checked`, every root `Checked` with MIR in the targeted query and as the ordinary graph root, both installed modes, exact and one-short caps.
- `o01_old_typed_caps_give_resource_limit`, `o06_old_typed_caps_give_resource_limit`: the parent's typed caps now report `ResourceLimit` with no receipt.
- `o01_unadmitted_exclusive_writes_keep_their_parent_caps`: `&mut i64`, shared `&i32`, plain store, `-=`, `*=` and a record-field write keep their parent typed caps and blockers.
- `o01_exclusive_i32_write_siblings_keep_their_screens`: grouped target, operand and descriptor, a computed operand and a non-parameter descriptor stay `Incomplete` at the caps measured with only the typed admission applied (a `git worktree add` at the parent). `scoped`, `looped` and `dead` are `Checked` controls.
- `o01_call_field_equality_operands_check_and_siblings_keep_their_screens`: bare, grouped, two-sided and local-versus-field operands check. A bound field read and a nested field keep the parent caps.

## Caps

Mandated flips (rule 11, third case; the old caps assert `ResourceLimit`, not the parent's `Incomplete`):

| Row | Old (typed, parent) | New (typed) | New graph | Reason |
| --- | --- | --- | --- | --- |
| O1 | 325,375 / 47,829 / 12 (`Incomplete`, `pending Storage @ *value += 1i32;`) | 326,220 / 47,997 / 12 `Checked` | 190,759 / 5,183 / 6 | the typed admission and three lowered roots |
| O6 | 325,245 / 47,754 / 12 (`Incomplete`, same blocker) | 326,089 / 47,923 / 12 `Checked` | 189,091 / 4,699 / 6 | as O1 |

Per-root attribution, work / entries / depth, targeted: O1 `increment` 2,182 / 183 / 3, `shifted` 5,806 / 722 / 6, expectation 8,913 / 1,139 / 6. O6 `change` 2,190 / 187 / 3, `twice` 4,412 / 394 / 3, `result` 6,615 / 587 / 6, expectation 8,642 / 688 / 6. Depth 6 comes from `shifted` and the expectation (O1) and from `result` and the expectation (O6). The old rows had no receipt, so there is no old graph; their `pre_equality_graph` field is the new graph with the expectation root's work and entries removed, which gives `ResourceLimit`. The sibling source `EXCLUSIVE_WRITE_SIBLINGS` has no equality root: its `pre_equality_graph` is its graph minus the `control` root (190,538 / 4,622 / 5), and `CALL_FIELD_EQUALITY` uses the real graph before the field-of-call operand was admitted (193,367 / 4,510 / 5).

The scoped (`{ ... }`) and loop-wrapped `+=` forms lower through the ordinary block and loop paths and are `Checked` controls, not held at parent caps.

Left unchanged, rerun on the final tree: `m5::legacy_caps`, `m5::graph::inventory` and the other `m5::graph` tests, `m5::o15_arithmetic`, `records`, `roots`, `field_routes`, `tuples`, `--test m4_8_scoped_join_caps`, `--test m4_8_match_boolean_snapshots`, `--test m4_8_result_use`, `--test m4_8_ownership`. No other `rfc_cases` row moved. The M4 17-source baseline (372,246 / 54,059 / 12) is not edited anywhere in this diff.

Rule 1: the one sibling that first moved (`viaLocal`, 2,008 / 101 / 2 to 2,010 / 101 / 3) was caught by the sibling test and fixed by screening on `value()` facts before the depth and flow charges.

## Not established

Execution of O1 and O6 (M6); `*slot = source` through `&mut &Point` (O2 and O8, ticket ri-1up); a write through a non-parameter descriptor; `-=`, `*=` and `=` through `&mut i32`; a later `let moved = value;` that transfers the reborrowed descriptor. The full M5 binary and `mise run check` run in CI on the integration branch, not here.
