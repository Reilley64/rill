# `ri-68mu`: `analyze_typed` publishes no finding at `ResourceLimit` (2026-11-24, city bead ci-1c5f)

Review-2 fix pass, child of `ri-px1.11`; typed twin of `ri-grwx` (fixed in C1, `ri-px1.11-c1.md`). Chief ruling (`chief-rulings-12.md`, 2026-11-24 14:20 AEDT): `analyze_typed` withholds its findings at `ResourceLimit`, the five `keeping_body_findings` sites switch to `assert_typed_limit`, the variant is deleted, and the seven assertions C1 removed come back as exact-cap pins. Base `a4e56651` (the integration branch with C1 merged), branch `m5-ownership-cleanup/r2f-ri-68mu`.

## What changed

- `src/analysis/typed.rs`: after `completion::finish`, when the completion is `ResourceLimit`, `analyze_typed` withholds every finding list its report publishes: `bodies().findings()`, `conditional_findings()`, `declaration_issues()`, `annotations().findings()`, `public_interface_findings()`, and the inventory's `type_facts().findings()` and `declaration_issues()`. The receipt was already withheld (it exists only when `Checked`). The block runs after the meter stops and charges nothing: no `step`, `reserve` or `charge_key` moved, no completion changed, no published cap moved. Mutators: `GenericReport::withhold_findings`, `OrdinaryAnalysis::withhold_findings`, `PartialFacts::withhold_findings` (publication only).
- `tests/support/limit.rs`: `assert_typed_limit_keeping_body_findings` and its partial `flaws` function are deleted. `typed_limit_flaws` checks every list above, so one helper states rule 8. `docs/agents/compiler-metering.md` rule 8 names no variant; its typed parenthesis now reads "no receipt and no finding in any list it publishes".
- The five sites call `assert_typed_limit`: `rfc_cases.rs` `old_typed_caps` (used by `o01_unadmitted_exclusive_writes_keep_their_parent_caps`, `o07_forms::tuple_with_an_exclusive_element_stays_typed_incomplete`, `o17_exclusive_copy_and_repeated_once_reject`, `o23_generic_no_borrow_evidence_rejects_r0302`), `rfc_cases/list_contents.rs` `r1_caller_provided_exclusive_source_stops_typed`, `captures/borrowed_invocation.rs` `borrowed_once_and_unproved_bodies_do_not_issue_receipts`, `m4_8_scoped_join_caps.rs` `unsupported_result_joins_keep_parent_caps_and_blockers`, `m4_8_callable_once.rs` `private_once_checks_the_actual_shared_pair`.

Entry point fixed: `analyze_typed` (the five sites' producer). The standing ruling's search found no other entry point that I fixed; see "Found" below.

## Red first

With only the four `src/` files reverted (no stash; patch in `fix-src.patch`):

- `m4_8_callable_once::private_once_checks_the_actual_shared_pair` fails at `m4_8_callable_once.rs:382`, "publishes a body finding" (`red-m4_8.log`). The four other sites fail the same way.
- `m4_8::resource_exhaustion_publishes_no_lexical_finding` fails ("publishes a body finding" at 524 work) and `m4_8::resource_exhaustion_publishes_no_finding_list` fails ("publishes a declaration issue", "publishes a public-interface finding", "publishes an inventory declaration issue" at 1,579 work) (`red-m4_8-main.log`).

## The seven replaced assertions

Every cap below except the scoped-join literals is measured by the test from the full run's `statistics()` (exact) and lowered one axis at a time (one short); none is a literal. At exact the finding (or rejected delivery) **is** published; one short gives `ResourceLimit` with nothing published (`assert_graph_limit`, from C1).

| # | File, test | Exact (publishes) | One short |
|---|---|---|---|
| 1 | `tests/m5/graph.rs` `one_budget_leaves_unfinished_work_unproved` | the `SourceRejected` run's statistics: one finding, `R0200` (before: `findings().len() == 1`) | work, entries, depth each `- 1` (already in the test): helper |
| 2 | `tests/m5/graph.rs` `local_blocker_is_not_a_finished_dependency_verdict_on_exhaustion` | new exact replay: `SourceRejected`, one `R0200` at `again = v` related to `first = v` | new: each axis `- 1`, plus the old entries `- 1` run: helper |
| 3 | `tests/m5/graph/delivery.rs` `check_limits` (callers: the two graph-cap tests at lines 280 and 366) | `at` replays the exact cap: findings equal the full run; when it has findings, `nestedCaller` `forwarded_call()` and `bad` `selected_call()` are `Delivery::Rejected { finding_id: 0, via_call: Some(_) }` | each axis `- 1`: helper, and no root has a `Rejected` delivery (new) |
| 4 | `tests/m5/graph/scalar.rs` (`R0201` conflict test, was `findings().len() == 1`) | exactly one finding, `R0201` | `one_short`: helper |
| 5 | `tests/m5/graph/scalar.rs` (widths test, was `findings().len() == 2`) | the exact run's finding codes equal the full run's (`R0200`, `R0203`) | `one_short`: helper |
| 6 | `tests/m5/scoped_join.rs` `installed_scoped_join_checks_actual_child_frames_and_caller_successors`, live parent graph | exact 213,499 / 5,736 / 10 (rule 11, `ri-785.9`) replay publishes one `R0201` | `budget_caps` short: helper; the old 213,183 / 5,602 / 10 stays `ResourceLimit` with nothing published |

The ticket's seven are these sites: `graph.rs` (rows 1, 2), `delivery.rs` `check_limits` (row 3: the count check, the two `Rejected` matches and the else branch), `scalar.rs` (rows 4, 5) and `scoped_join.rs` (row 6).

## Beyond the ticket

- `tests/m4_8.rs` `resource_exhaustion_does_not_erase_independent_lexical_findings` pinned the opposite of Q1 (a `ResourceLimit` typed report kept a `MissingValue` finding at `max_entries: 123`). It is replaced by `resource_exhaustion_publishes_no_lexical_finding`: exact publishes the finding, each axis one short and the old 123-entry cap use `assert_typed_limit`. This reverses an M4.8 expectation on the chief's Q1/ri-68mu ground; no cap moved.
- New `m4_8::resource_exhaustion_publishes_no_finding_list`: three sources (R0401 public-interface findings plus an inline cycle; a rejected header; a cycle next to a missing type). Each pins exact (the same finding count as the full run), one short on each axis, and 64 work caps below exact, all through `assert_typed_limit`.

## Caps

No published cap moved: the filtered runs below pass without an edited expectation, the three `m4_8_*` cap binaries pass, `legacy_caps` passes. No `step`, `reserve` or `charge_key` was added.

## Found, not fixed

Filed as a needs-triage bead (see the handoff report): `analyze_ordinary` and `analyze_generics` return reports with a completion, and both publish findings at `ResourceLimit` (probe over work caps 0 to 700 of one source: ordinary published type findings and declaration issues; generics published declaration issues and annotation findings). `analyze_declaration_types` and `check_declarations` return `Err` with a `partial` that carries findings, pinned on purpose by `m4_1.rs` and `m4_2.rs`. Fixing them edits M1 to M4 contracts, beyond the same fix.

## Not established

- Other lists on `TypedReport::inventory()` (header metadata findings, family facts) were not probed.
- Typed `ResourceLimit` sites in M5 modules I did not run (`o12`, `patterns` and the like) pass by construction (the helper only got stricter where the producer now publishes less), and CI runs them.

Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-1c5f/`.
