# ri-vgz.8: ri-vgz closeout audit, 2026-11-23

Ticket ri-vgz.8 (gap-analysis C6), umbrella ri-vgz (rows P1 to P7 of §1.3). This is an audit at the integration tip `5471035b`, which holds every ri-vgz ticket: A3 (ri-vgz.1), A5 (ri-vgz.2), C1 (ri-vgz.3), C2 (ri-vgz.4), C3 (ri-vgz.5), C4 (ri-vgz.6) and C5 (ri-vgz.7). It changes no source. It adds one test, [`propagation/closeout.rs`](../m5/propagation/closeout.rs), and this file.

## Replay

Every test below runs the installed compiler in both load modes (`probe` and `installed` rerun the test in a child per mode), and asserts the targeted query and the ordinary graph root. Where a cap is named, the test also asserts exact work, entries and depth, and one short on each axis independently giving `ResourceLimit` with no MIR and no finding. The older tests (before A3) derive the exact cap from the measured statistics and then assert one short; the C-tickets pin the numbers in their own files.

Replayed at `5471035b` with the filters and results (logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-u2dt/`):

| Filter | Result | Log |
| --- | --- | --- |
| `propagation::` | 74 passed | `p1-p5-propagation.log` |
| `iteration` (`iteration::`, `iteration_result_use::`) | 41 passed | `p6-iteration.log` |
| `patterns::commitment records::interrupted records::flat_rebuild tuples::` | 23 passed | `p5-patterns-records.log` |
| `legacy_caps rfc_cases graph::inventory` | 27 passed | `fixed-caps.log` |
| `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots` | 1 passed each | `m4_8_*.log` |

No expectation was edited. Nothing is red.

## Obligations

Caps are targeted work / entries / depth. Graph caps are in the ticket's own file or test.

| Row | Obligation | Test (under `m5`) | Outcome | Cap |
| --- | --- | --- | --- | --- |
| P1 | Postfix on owned `Option`/`Result` scalars: once-only operand, payload transfer, failure exported before cleanup | `propagation::option_postfix_returns_through_initialized_cleanup`, `result_postfix_forwards_existing_failure_before_cleanup`, `shared_option_postfix_uses_the_static_variant_edges`, `exclusive_option_postfix_moves_the_carrier_and_descriptor`, `exclusive_option_postfix_checks_reached_moves_and_loans`, `known_some_postfix_moves_the_exclusive_carrier_without_a_tag_test` | `Checked`; reached `R0200`/`R0201`/`R0203` give `SourceRejected` | exact and one short asserted in each test |
| P2 | Infix wrapping, including an RHS that returns | `propagation::scalar_infix_wraps_only_failure_and_preserves_the_cause`, `returning_wrap_rhs_forwards_instead_of_wrapping`, `wrapping_rhs_keeps_ordinary_and_unadmitted_budgets`, `closeout::wraps_after_rhs_has_no_graph_root_mir_only_because_of_the_retained_display_edge` | `Checked` targeted for `wrapsAfterRhs`, `terminalRhs`, `successfulRhs`, `pending`, scalar wrap. The graph root of `wrapsAfterRhs` has no MIR: see below | `wrapsAfterRhs` 12,574 / 2,472 / 6; literal wrap 4,080 / 801 / 4; `terminalRhs` is a graph root with MIR |
| P3 | `Option<string>` | `propagation::option_text_propagation_moves_or_drops_the_owned_payload`, `live_option_text_cleanup_drops_only_the_initialized_carrier_payload`, `fresh_option_text_propagation_stages_the_constructor_once` | `Checked` | exact and one short asserted in each test |
| P3 | `Result<string, i32>` success payload | `propagation::result_text::owning_text_result_postfix_moves_ok_and_forwards_err_before_cleanup`, `owning_text_result_cases_keep_their_outcomes_at_exact_caps` (C2) | `Checked` (bound), `R0200`, `R0203`, `Incomplete` controls | bound 4,605 / 715 / 4, graph 188,540 / 4,676 / 4 |
| P3 | Non-scalar `Err` (`string`) transfer and destruction | `propagation::text_err::string_error_is_forwarded_unchanged_and_the_held_error_is_retired_once`, `string_error_cases_keep_their_outcomes_at_exact_caps` (C3) | `Checked`: one `DropResultFailure` after the export; `R0200`, `R0203`, three `Incomplete` controls | bound 3,521 / 522 / 4, graph 187,456 / 4,483 / 4; retire 4,794 / 668 / 4 |
| P3 | Heterogeneous wrap with canonical display | none | **Deferred** (D7) | bead `ri-ink` |
| P4 | `Option<&T>`, same body | `propagation::owned_option_postfix_preserves_shared_text_backing`, `shared_option_copies_keep_the_named_carrier_and_its_backing` | `Checked` | exact and one short asserted in each test |
| P4 | `Option<&mut T>`, same body | `propagation::exclusive_option_postfix_moves_the_carrier_and_descriptor`, `exclusive_option_does_not_copy_or_propagate_through_a_borrowed_whole` | `Checked`; the borrowed whole stays unadmitted | exact and one short asserted |
| P4 | Shared `Option<&T>` across an ordinary caller | `propagation::option_caller::admitted_option_relay_callers_keep_their_caps`, `unsupported_option_relay_siblings_keep_parent_caps`, `selected_shared_option_call_preserves_the_caller_backing`, `call_result_keeps_local_backing_until_the_descriptor_dies` (A3) | `Checked`; unsupported siblings `Incomplete` at parent caps | `relay(input)?` 9,516 / 1,677 / 4, graph 198,554 / 6,624 / 4; grouped 9,536 / 1,685 / 8 |
| P4 | Exclusive `Option<&mut bool>` across an ordinary caller | `propagation::exclusive_option_caller::*` (5 tests, C1) | `Checked`; `R0200`, `R0201`, `R0202`, `R0203` | `callerPostfix` 9,404 / 1,651 / 4, graph 195,872 / 6,109 / 4 |
| P4 | Dependency through `Ok` and a primary `Err` (borrowed `Result` payloads) | none | **Deferred** (D7) | bead `ri-ijq` |
| P5 | Propagation inside call arguments and pipes | `propagation::an_early_call_argument_exit_cleans_only_the_staged_prefix`, `composed_arguments_before_a_propagating_outer_step_keep_all_exit_prefixes`, `option_pipe_propagates_the_staged_call_output_once`, `propagating_pipe_input_runs_before_written_arguments`, `two_propagating_arguments_clean_their_own_staged_prefixes`, `wrapping_and_pipe_arguments_use_the_same_early_exit_cleanup` | `Checked` | exact and one short asserted in each test |
| P5 | Record initializers | `records::interrupted_record_initialization_cleans_only_the_reached_fields`, `patterns::commitment::interrupted_record_commits_binders_only_on_success` | `Checked` | exact and one short asserted |
| P5 | Tuple initializers and one ordinary continuation (C4) | `propagation::staging::interrupted_tuple_initializer_cleans_only_the_reached_prefix`, `condition_postfix_continues_on_the_ordinary_branch` | `Checked`; `R0200`; `Incomplete` siblings at parent caps | tuple 8,617 / 2,053 / 7; condition 4,072 / 821 / 6 |
| P5 | A List literal source | `iteration::source::installed_list_source_can_propagate_before_cursor_commitment` | `Checked` | exact and one short asserted |
| P6 | Consuming `for`, scalars and owning items, early exits | `iteration::installed_consuming_list_iteration`, `consuming_iteration_checks_each_items_lifetime_before_advancing`, `text::installed_consuming_string_iteration`, `text::linked_string_suffix_disposes_only_remaining_values_in_reverse` | `Checked` | `onBreak` (strings) 2,837 / 457 / 6 |
| P6 | Propagation with a live cursor | `iteration::text::postfix::installed_postfix_with_live_string_cursor`, `postfix_stamps_only_a_reached_loop_with_all_text_formals_covered` | `Checked` | exact and one short asserted |
| P6 | Propagation from a yielded owning carrier (A5) | `iteration::option_text::review_counterexamples_keep_their_outcomes_at_exact_caps`, `consuming_list_transfers_the_actual_option_tag`, `an_ordinary_option_loop_keeps_the_reached_move_finding` | `Checked`; counterexamples `Incomplete` at `xs` or `R0200` | `&item` 1,855 / 19 / 0; sibling 1,864 / 23 / 0; `List<bool>` sibling 1,866 / 24 / 0 |
| P6 | Record-valued items (C5) | `iteration::record::record_yield_cleans_the_partial_item_then_the_suffix`, `linked_record_suffix_releases_whole_remaining_records_in_reverse`, `string_on_break_keeps_its_cap` | `Checked`; `R0200`; `Incomplete` controls | `onBreak` 5,605 / 1,356 / 7; whole 5,318 / 1,209 / 5 |
| P6 | One borrowed iteration | none | **Deferred** (D7) | bead `ri-9z0` |
| P7 | Cancellation while propagation is pending or a cursor is live | none | **Deferred** (D7) | bead `ri-xat` |

The existing `cancellation::` tests (function entry, the `while` backedge) are M4 checkpoints, not P7: no checkpoint exists at a `for` or at `?`.

## `wrapsAfterRhs` graph root: recorded as deferred against `ri-ink`

`wrapsAfterRhs` (`let flag = input ? (marker()?);`) has targeted MIR (`returning_wrap_rhs_forwards_instead_of_wrapping`, 12,574 / 2,472 / 6) and no ordinary graph-root MIR. The new test `closeout::wraps_after_rhs_has_no_graph_root_mir_only_because_of_the_retained_display_edge` pins the whole picture on a smaller fixture and shows why:

- the targeted query is `Checked` at 11,666 / 2,019 / 6 (this fixture; exact and one short per axis);
- the graph root is `Incomplete`, with no MIR and no finding, blocked at the wrap `input ? (marker()?)`;
- `scalarWrap` (`input ? 3i32`, no RHS call) stops at the same kind of site, `input ? 3i32`: the cause is the wrap, not the RHS call;
- the control `terminalRhs` (an RHS that always returns, so no new wrapper) is `Checked` with graph-root MIR;
- the whole graph is `Incomplete` at 203,035 / 7,345 / 6, and one short on work, entries or depth gives `ResourceLimit`.

A new outer wrapper emits `WrapResultFailure`, whose canonical cause display is the `RetainedErrorDisplay` provider edge, and the graph keeps that edge pending (`graph.rs`, `Operation::WrapResultFailure`). Giving `wrapsAfterRhs` graph-root MIR is therefore the same work as the heterogeneous display of P3, which D7 defers (`ri-ink`). It is not a missing-edge defect in the RHS: fixing it would admit a provider edge for display, which is a new proof, not a graph-root correction. No source changed in this ticket. If `ri-ink` is taken, this test must flip: it would then assert `Checked` for `wrapsAfterRhs` and `scalarWrap`.

## Caps

No fixed cap moved. `legacy_caps`, `rfc_cases`, `graph::inventory` (17-source graph 202,698 / 4,871 / 3; 18-source 243,206 / 25,960 / 10), `m4_8_scoped_join_caps` and `m4_8_match_boolean_snapshots` pass unedited. The only new cap is the fixture above.

## Found

`ri-o62c` (from C5, needs-triage): a grouped List literal of records `(([Entry{..}, ..]))` loses the M4 receipt, while the ungrouped form checks. It bounds P6 record-valued items to the ungrouped literal.

## Not established

- Rill execution: every result is LLVM-free host ownership analysis.
- Heterogeneous display and `wrapsAfterRhs` graph-root MIR (`ri-ink`); borrowed `Result` payloads (`ri-ijq`); borrowed iteration (`ri-9z0`); cancellation at a pending `?` or a live cursor (`ri-xat`).
- The full M5 binary and `mise run check`: hosted CI only (owner policy). See the hosted run in the handoff.
- The older P1, P3 (`Option<string>`), P5 and P6 rows hold exact caps in their tests, not in this file; this audit replays them and records the outcome. It does not re-measure each number.
