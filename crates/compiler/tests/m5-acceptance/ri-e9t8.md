# O16 `joinedOrder` and its expectation (ri-e9t8, city bead ci-s0oq, 2026-11-23)

This ticket takes the Required row C4/O16 expectation into M5 (owner decision of 2026-11-23 19:18 AEDT). RFC 0007 case O16's `joinedOrder` and `expect joinedOrder();` were `Incomplete` at `pair[0] == 13i32 && pair[1] == 29i32`. Both are now `Checked` with MIR, in both installed modes, in the targeted query and as ordinary graph roots. These are compiler judgments over MIR. No Rill program ran; nothing observes that `joinedOrder` is true (M6).

## What changed

The ticket named two missing pieces. Probing the approved source found a third: the body's first statement, `let input: List<i32> = [13i32, 29i32];`, has no lowering either, because the backward builder reached the return statement first. All three are narrow proof paths:

- **`&&` on two i32 equalities.** M4 publishes `i32_equality_conjunction` for `left && right` typed `bool` whose operands, through groups, are `i32_equality` shapes. It is classified from slots already inferred and charges nothing (the 17-source baseline stays 372,246 / 54,059 / 12). M5 evaluates `left` into a scratch `bool`, branches, ends the scratch on both edges, and either evaluates `right` into the result or, on the false edge, initializes the result without reading `right`. A left operand with a statically decided outcome stays `Incomplete` (a dead edge grants nothing). `||`, `&& true`, chains of three and bool locals are not admitted.
- **Tuple component operands.** M4's `i32_operand` classification gains `TupleElement`: a normal `i32` index whose path is one tuple component of a local or parameter, owned access. A List index (`ListElement`, which can trap) is not. `i32_equality` then accepts it; M5 already lowered `pair[0]` as a projection read.
- **`List<i32>` literal.** `Action::CommitList` gains an i32 form and MIR `Operation::CommitListI32`: each integer literal element is staged in written order into a scratch `i32`, then all are committed through the linked `List::from_elements` in one step; the stages end. Admitted only when the context declares the type (`facts.expected == facts.ty`: an annotated binding or a typed argument) and every element is a plain literal. The unannotated `let values = [13i32, 29i32];` stays `Incomplete` so the published fixed caps of `legacy-caps/foreign-rc` (`unavailable`, 1,954 / 29 / 3) do not move.

Commits: see the branch `m5-ownership-cleanup/r-o16-ri-e9t8`.

## Tests

`m5::o16_join_order` (new), both installed modes, each pinned cap replayed exact and one short per axis:

- `joined_order_and_its_expectation_check_with_ordered_mir`: `joinedOrder` Checked (targeted and graph root) with one ordered `(operation, site)` list per edge. Shared prefix: `Initialize 13i32`, `Initialize 29i32`, `CommitListI32`, `Move input`, `Call joined(input)`, `Copy pair[0]`, `Initialize 13i32`, `EqualI32`, the branch. True edge: end scratch, `Copy pair[1]`, `Initialize 29i32`, `EqualI32`, export. False edge: end scratch, `Initialize` of the result, export, no read of `pair[1]`. The expectation is Checked and calls `joinedOrder()` once.
- `old_stop_caps_give_resource_limit`, `old_fixture_graph_caps_give_resource_limit`: the old caps below give `ResourceLimit`, no MIR, no finding.
- `unsupported_siblings_keep_their_parent_caps_and_blockers` (metering rule 2): 20 siblings keep the targeted and graph caps and blockers measured at the parent `7002855e` in a `git worktree add` (the candidate was measured separately and matched): `||`, `&& true`, `!=`, a three-operand chain, List index operands (alone and with `&&`), `while` and `if` conditions, a `&[i32, i32]` parameter, a nested tuple component, and List literals that are empty, hold a local, a sum, a group or a negative, are `List<i64>`, `List<bool>`, `List<string>`, returned, or unannotated.
- `admitted_shapes_check_at_exact_caps`: 17 newly admitted shapes and one finding, each at exact caps (scoped, `let ok = ...`, parameter pair, reversed, grouped, a mutable tuple, sums, calls, one- and two-element List literals, loop-wrapped, as an argument). Disposing a literal's list twice is `SourceRejected` with `R0200` at the second `dispose(input)`.
- `grouped_operands_add_depth_not_operations` (rule 5): `((a)) && ((b))` has the same operations as the bare form and depth 7 against 5.

Edited expectations (the ticket's flips and their consequences): `rfc_cases::o16_two_shared_jobs_check_through_the_join` now wants `Checked` for `joinedOrder` and `expect`; `graph::inventory` pins them as checked roots; `legacy-caps.tsv` and `scoped_join::fresh_result::shared_join_rejects_a_live_fresh_error_without_wrapper_selection` re-pin their whole graphs. `m5-status.md` row O16 is updated.

## Rule 11 dated note: mandated flips and whole-graph re-pins (2026-11-23)

Third case (ticket's mandated flip), RFC O16 source, targeted queries. Old caps give `ResourceLimit` in `o16_join_order`:

| Root | Old | New | Reason |
| --- | --- | --- | --- |
| `joinedOrder` | `Incomplete` at the `&&`, 2,392 / 139 / 3 | `Checked`, 36,060 / 21,717 / 10 | The body now lowers whole, including the call into `joined`'s frame, whose depth 10 it takes. |
| `expect joinedOrder();` | `Incomplete`, 4,431 / 219 / 3 | `Checked`, 38,099 / 21,797 / 10 | `lower_root` lowers `joinedOrder` too. |

Whole graph (D2 (b)): only `analyze_ownership` totals moved; no other targeted cap did. Per-root attribution, measured by adding the roots one at a time in the parent and on the candidate: `joinedOrder` adds **+1,313 work and +678 entries** to every graph that holds it (it was a cheap `Incomplete` root); the expectation adds **+0 / +0** (312 / 31 before and after); depth stays 10, set by `joined`, so the per-root figures add up to the change. The same figure holds for the O16 fixture and for the live-error fixture.

| Where | Old (historical) | New |
| --- | --- | --- |
| `rfc_cases` O16 graph | 225,554 / 25,180 / 10 | 226,867 / 25,858 / 10 |
| `legacy-caps.tsv` shared, alias | 243,522 / 26,094 / 10 | 244,835 / 26,772 / 10 |
| `legacy-caps.tsv` competing | 214,539 / 5,705 / 10 | 215,852 / 6,383 / 10 |
| inventory `shared` capture | 243,522 / 26,094 / 10 | 244,835 / 26,772 / 10 |
| `shared_join_rejects_a_live_fresh_error...` graph | 217,162 / 6,313 / 10 | 218,475 / 6,991 / 10 |

Inventory: `joinedOrder` and its expectation are checked application roots in the `shared` capture. Every other fixed cap is unchanged: the typed rows, every `body` and `root` row of `legacy-caps.tsv`, the 17-source baseline, and `foreign-rc`.

## What this did not establish

- No Rill execution. The truth value of `joinedOrder` is not observed.
- The full M5 binary, `m4_8_*` binaries beyond the two named in the report, and `mise run check` were not run locally (owner policy); the merger's CI on the integration branch is the gate.
- Unannotated `let values = [13i32, 29i32];`, List literals with non-literal elements, and `&&` beyond two equalities stay `Incomplete`.
