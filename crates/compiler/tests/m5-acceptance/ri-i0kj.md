# ri-i0kj: R0202 is reported at the call that stores the body-local source (2026-11-24, city bead ci-gs1f)

Ticket `ri-i0kj` (found in review `ci-0mdr` as SP-6, re-review `ci-b4b2` C-3), spec `ri-2s0`. Owner decision 2026-11-24: fix before M5 merges. Base `d85303bd` (`origin/implementation/m5-ownership-cleanup`), branch `m5-ownership-cleanup/r0202-site-ri-i0kj`. Logs: `~/.t3/evidence/rill/ci-gs1f/`.

This file supersedes the SP-6 line of "Final record after the re-review fix pass" in [`m5-acceptance.md`](../m5-acceptance.md) ("The compiler reports R0202 at the last call that retains into `dst` before the exit ... Pinned as current behaviour by `rfc_cases::list_contents::r1_blocker_site_is_the_last_retention_not_the_storing_call`"), and the stated limit recorded for it in [`ri-px1.11.md`](ri-px1.11.md) and [`ri-px1.11.5.md`](ri-px1.11.5.md). Those lines are not edited. The test was renamed back to `r1_local_then_parameter_reports_the_storing_call`.

## What changed

One source change, in `src/analysis/ownership/flow.rs`.

- Cause: `check_retained` reported the exit finding at `Held::origin`, which each retaining call overwrote with itself. A call that retained a valid parameter after the storing call, or a lone-destination call, became the site.
- Fix: a `Held` for outward contents carries `local_store`, the retaining call that first stored a body-local source into those contents and that source's borrow. A retaining call sets it only when the contents hold no body-local loan yet and its inputs add one; otherwise it keeps the earlier value. `check_retained` reports the loan that `local_store` names, at that call. `origin` and `RetainedAtDelivery::last_store` are unchanged (still the last store). A strong update into caller contents (`InstallThrough` into a caller target, and `InstallThroughTwoHop`) sets `local_store` to the assigning statement and the first body-local borrow the installed value holds, and clears it when the value holds none (review `ci-o92c` F2, ruling of 2026-11-24 23:24 AEDT): a later retaining call keeps it, and a later strong update replaces it with the whole `Held`. `assigned_local_store` reads the loans of the incoming value, which the action consumes at once, so each loan set is read once, after it was charged where it was built; no `budget.step` is added and no cap of the assignment roots moved.
- `Held` grew from 208 to 232 bytes, which trips `clippy::large_enum_variant` on `State` (the lint fires above a 200-byte gap). `State` carries `#[allow(clippy::large_enum_variant)]`, as other files of the crate allow `too_many_arguments`. Boxing `Held` would touch every state site; sharing the bytes of `origin` would drop `last_store`, which `RetainedAtDelivery` publishes.
- Which store when there are two: the first. RFC 0007 line 218 has the callee's exit check reject "a body-local source that flows" to `dst`; the earliest store is where the contents first hold one, and the analysis reports one finding per root, so it names one. The related site is that call's own borrow, in either declaration order (`secondThenFirst`).
- The two scans in `Action::Call` (`old.loans.iter().any(..)` and `loans.iter().find(..)`) carry no `budget.step`. This is deliberate: both read loan sets that `include_loans` and `loan_set` already charged on the same path, and a step would move the caps of every list-retain root and bring in rule 11. `check_retained` charges one step per loan it reads, as before; it reads further only when the loan that `local_store` names is not first in `Loan` order (review `ci-o92c` F3, pinned by `secondThenFirst`, 5,786 against 5,785).
- Outcomes: every program stays `SourceRejected` with R0202 (or `Checked`); only the site moved. No published cap moved: every pin that existed before this ticket passed with its caps unchanged (listed under Checks), so no old cap of a published test needed a `ResourceLimit` assertion. The caps of programs that no test pinned did move, and some of them are `Checked` (next section).

## Cost: states split by `local_store` (review `ci-o92c` F1)

`Held` derives `Eq` and a join (`KnownStates::add`) reuses a block input only on whole-state equality, so `local_store` is part of the key and no determination is reused across different values (rule 7). Two paths that store the same borrow at different calls therefore stay two states after a later common store, where the parent merged them (same loans, same origin). The work is metered and bounded: the multiplier is the number of distinct first-store calls, times the tail to the next strong update or the exit. The split is what names the right call per path. The chief of staff accepted it on 2026-11-24 at 23:24 AEDT as the mandated cost (rule 11, third case; no published cap moved), on the condition that the roots below are pinned exact and one short, each parent triple asserts `ResourceLimit`, and this paragraph states the cost. An earlier version of this file said the extra reads happen only when the named loan is not first in `Loan` order; that covers `check_retained` alone and missed the split.

Work / entries / depth, `lower_body`, both modes alike. Parent `d85303bd`; the numbers are those of the pinned modules, which differ from the review's probe modules where the module has other roots.

| Root | Outcome | Parent | Now |
|---|---|---|---|
| `c1_splitThenRepair` (view stored on both branches of an `if`, then a valid call, then `*slot = input;`) | `Checked` | 11,439 / 3,030 / 7 | 12,798 / 3,765 / 7 |
| `c2_split3ThenRepair` (three-way split) | `Checked` | 13,264 / 3,976 / 9 | 16,847 / 5,686 / 9 |
| `c3_noSplitControl` (no split) | `Checked` | 11,529 / 3,074 / 7 | unchanged |
| `v23_viewBranches` | `R0202` at `retain(dst, view)` | 7,198 / 1,162 / 7 | 7,303 / 1,228 / 7 |
| `v37_viewsOfTwo` | `R0202` at `retain(dst, v)` | 11,991 / 2,576 / 7 | 12,545 / 2,888 / 7 |

Whole graph of the two modules: the split module (`Incomplete`, both) 214,952 / 12,549 / 9 to 220,698 / 15,030 / 9; the view module (`SourceRejected`, both) 207,342 / 7,754 / 7 to 208,001 / 8,132 / 7. At the parent's exact cap the `Checked` roots now give `ResourceLimit`. They stay `Checked` at default limits.

Tests: `list_contents::r1_split_stores_that_repair_cost_more_and_their_parent_caps_give_resource_limit` (exact and one short on every axis for `c1`, `c2`, `c3`; the parent triples of `c1`, `c2`, `v23`, `v37` and the two parent graphs asserted to give `ResourceLimit` through `assert_body_limit` and `assert_graph_limit`, in both modes) and `list_contents::r1_view_stored_on_two_branches_reports_the_first_branch_call` (`v23`, `v37` exact and one short).

## Strong update through `InstallThrough` (review `ci-o92c` F2, F4)

An earlier version of this file and the handoff of `ci-gs1f` said no program does a strong update through `InstallThrough` that stores a body-local source. That was wrong. A body-local source stored by assignment (`*slot = &flag;`) and then followed by a valid call (`set(slot, input);`) reported at the valid call, at the parent and at `eb749491`, with the borrow as the related site. `local_store` was `None` after the assignment. Fixed above. The two-hop assignment forms (`**slot = &flag; keep(slot, input);`) stop typed at `Storage` before the analysis reads them (`retained::original_two_hop_relay_checks_reached_bodies_and_preserves_backing` with such a root: `Incomplete`, pending `Result` use at `**slot = &flag;`), so no test can reach the `InstallThroughTwoHop` line of the fix; it is the same call to `assigned_local_store` and is there for the day `Storage` admits the form.

| Root (`ASSIGNED_STORES`) | Parent and `eb749491` site | New site | Caps |
|---|---|---|---|
| `o3_assignLocalThenCall` | `set(slot, input)` | `*slot = &flag;` | 2,716 / 436 / 5 |
| `r4_assignLocalThenCall` (`&mut &Point`) | `remember(slot, a)` | `*slot = &p;` | 3,702 / 596 / 5 |
| `o8_branchAssign` | `set(slot, input)` | `*slot = &flag;` | 3,157 / 670 / 6 |
| `o5_callLocalAssignLocalCall` (call, then assignment of a second local) | `set(slot, input)` | `*slot = &other;` (the assignment holds the local now) | 3,264 / 706 / 6 |
| `o7_assignLocalCallLocalCall` (assignment, then a call that stores a second local) | `set(slot, input)` | `*slot = &flag;` (the first store) | 3,276 / 708 / 6 |
| `oa_assignLocalOnly` (control) | `*slot = &flag;` | unchanged | 2,398 / 290 / 4 |

All six unchanged in caps (typed 341,282 / 49,757 / 12; graph `SourceRejected` 202,445 / 7,014 / 6). Test: `list_contents::r1_assignment_stores_report_the_assigning_statement`, exact and one short on every axis. Red first (`red-pre.log`, the fix not applied): the test fails on the site of `o3`.

Rule 7, premise `local_store`: `list_contents::r1_the_storing_call_is_a_premise_of_the_state`. `repaired` stores at `set`, overwrites with a valid source, stores the same view at `keep`: the site is `keep`, not the stale `set`. `joinedSetFirst` and `joinedKeepFirst` differ only in which call each branch makes; the first branch's call is the site in both and the valid call after the join is not. The roots hold one local view throughout, so only the storing call changes between two uses of the same loans. Whether the key holds the premise shows in the cost of `c1` and `c2` above (a join that dropped `local_store` from `Eq` would cost the parent's numbers), not in a site, because the analysis reports the first rejecting path in either case.

## Tests

Red first (`red-site.log`, the source change reverted): three tests fail on the site alone and the new test fails.

| Test | Roots | Old site | New site |
|---|---|---|---|
| `list_contents::r1_local_then_parameter_reports_the_storing_call` (renamed) | `localThenParam` | `retain(dst, a)` | `retain(dst, &user)` |
| `list_contents::r1_old_contents_survive_a_later_retain` | `reversed`, `reversedViaCall` | `retain(dst, a)`, `forward(dst, a)` | `retain(dst, &user)` for both |
| `list_contents::r1_lone_destination_keeps_a_stored_local_rejected` | `loneAfterLocal` | `touch(dst)` | `retain(dst, &user)` |
| `retained::original_two_hop_relay_checks_reached_bodies_and_preserves_backing` | `relayTwice` | `keep(slot, input)` | `keep(slot, &flag)` |

The last three rows are the same defect, found by running the tests whose pins named a later call; the related site of each is unchanged (`&user`, `&flag`). `relayTwice` asserted that the related borrow precedes the site; it now asserts that the borrow is inside the site call.

New: `list_contents::r1_two_local_stores_report_the_first_storing_call` (`TWO_LOCALS`; both load modes; each root and the graph pinned exact and one short on every axis through `assert_body_limit` and `assert_graph_limit`). Typed 343,618 / 51,567 / 12; graph `SourceRejected` 210,460 / 7,464 / 8.

| Root | Site | Related | Caps |
|---|---|---|---|
| `firstThenSecond` | `retain(dst, &one)` | `&one` | 5,785 / 778 / 6 |
| `secondThenFirst` (locals declared `one`, `two`; stored `two` first) | `retain(dst, &two)` | `&two` | 5,786 / 778 / 6 |
| `twoThenParam` | `retain(dst, &two)` | `&two` | 7,309 / 1,087 / 6 |
| `loopLocalThenParam` | `retain(dst, &user)` | `&user` | 6,065 / 1,037 / 8 |

The loop forms already pinned (`loopOwned`, `loopBreak`, `oneBranch`, `r1_loop_body_local_source_retained_into_dst_rejects`, `r1_call_in_a_loop_checks_with_its_sequence`) pass with their caps unchanged.

## Checks run, first pass (`ci-gs1f`; targeted; no full suite)

- `cargo test -p compiler --test m5`, selected by name: `list_contents` (25 tests), then `retained::`, `graph::`, `field_routes`, `record_contents`, `legacy_caps`, `tuples`, `patterns`, `propagation` and `rfc_cases::` (333 tests, one failure: `relayTwice`, fixed above; `green-targeted.log`), then `cancellation`, `records`, `scoped_join`, `scoped_jobs` and `captures` (129 tests, all pass; `green-targeted-2.log`), then `roots::`, `disposers::`, `fresh_result_storage` and the three top-level tests of `m5.rs` that name R0202 roots (`ownership_from_installed_source`, `replacement_keeps_the_declaration_rank`, `installed_multi_module_borrow_join_and_early_exit`; 24 pass; `green-targeted-3.log`), and `retained::` plus `list_contents::r1` again on the final tree (42 pass). Together 486 of the 551 tests of the `m5` binary. Not run: `iteration*`, `o05`, `o15`, `o16` (none names R0202 or a retaining call into a parameter list). No other test file names R0202 (`m4_8_*`, `native_ownership` and the rest were searched).
- `cargo fmt -p compiler -- --check`, `cargo clippy --locked --no-default-features -p compiler --all-targets -- -D warnings` and `mise run check:docs`: clean on the final tree.

## Checks run, second pass (`ci-l4zc`; targeted)

Logs: `~/.t3/evidence/rill/ci-l4zc/` (`red-pre.log`, `green-final.log`, `green-wide.log`, `clippy.log`; probes and outputs under `probe/`, `o92c-p`, `o92c-q`, `o92c-r`).

- Red first: with the fix not applied, `r1_assignment_stores_report_the_assigning_statement` fails on the site of `o3` (`red-pre.log`); the F1 and premise tests pin parent behaviour and pass there, as they should.
- Two tests that existed before pinned the later call for a body-local source stored by assignment, the same defect, and were re-pinned: `retained::moved_incoming_access_keeps_the_original_caller_destination` (`escapeAfterMove`: `remember(access, input)` to `*slot = &flag;`) and `retained::retaining_option_call_preserves_old_and_possible_new_backing_in_caller` (`oldBacking`: `ignore(slot, seed, gate)` to `*slot = &flag;`). Outcome (`SourceRejected`, `R0202`), related site and caps are unchanged.
- Outcomes: the review's probe rows (87 in `q`, 48 in `p`, 7 in `r`, roots and whole-graph rows) and the 28 of this ticket's modules, both load modes, at the parent `d85303bd` and at the fixed tree: 0 changed completion, MIR presence or blocker. Against the review's probe of `eb749491`, only `o3`, `r4`, `o8`, `o5` and `o7` changed (site only, no cap).
- `cargo test -p compiler --test m5`, selected by name: `list_contents` and `retained::` (55 pass), then every module that names R0202 (`cancellation::`, `records::`, `graph::`, `patterns::`, `tuples::`, `scoped_join::`, `legacy_caps::`, `scoped_jobs::`, `propagation::`, `rfc_cases::`, `field_routes::`, `captures::` and the three top-level tests of `m5.rs` that name R0202 roots): 444 pass, 0 fail.
- `cargo fmt -p compiler -- --check`, `cargo clippy --locked --no-default-features -p compiler --all-targets -- -D warnings` and `mise run check:docs`: clean on the final tree.

## Not established

- The 65 `m5` tests named above as not run, the other test binaries and the hosted matrix did not run locally; CI runs them.
- Which of several body-local stores the RFC prefers is not stated in it; the first is a choice, recorded above.
