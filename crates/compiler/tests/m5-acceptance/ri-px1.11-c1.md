# Review ci-0mdr fixes, part C1: `ResourceLimit` publishes nothing (2026-11-24, city bead ci-iilr)

Part of the single fix pass of review `ci-0mdr` (second review of `81d317b3`), split by `implementer-1` into children of `ri-px1.11`: C1 is this file (Q1, ST-5, `ri-grwx`); C2 `ri-px1.11.1` (Q3, SP-3, SP-4); C3 `ri-px1.11.2` (Q4, SP-7); C4 `ri-px1.11.3` (ST-2, ST-4, ST-9); C5 `ri-px1.11.4` (ST-3, ST-8, after C4); C6 `ri-px1.11.5` (test-only pins); C7 `ri-px1.11.6` (records, last). Base `81d317b3`, branch `m5-ownership-cleanup/review2-fix-ri-px1.11`. The records child consolidates the per-child files into the section "Review ci-0mdr fixes" of `ri-px1.11.md`.

## What changed

Chief ruling Q1 (2026-11-24 13:46 AEDT), mandated change under rule 11's third case: `analyze_ownership` withholds findings and root MIR when the graph completion is `ResourceLimit`.

- `src/analysis/ownership/graph.rs`: on a limit stop, `withhold_judgments` clears `findings`, drops every root's `ordinary_mir`, turns a root verdict `SourceRejected` back into `ResourceLimit`, and turns a `Rejected` delivery back into `Unresolved` (each cited an index into the cleared findings). It runs after the meter stops and charges nothing: no `step`, `reserve` or `charge_key` moved, and no published cap moved (below). Root MIR was already unpublished at a limit (it is stored only after the last funded pass); the finding list was the leak.
- `tests/support/limit.rs`: the one place that says what a `ResourceLimit` report publishes. `assert_graph_limit` (completion, no finding, no MIR in any root or active disposer), `assert_body_limit` (`lower_body` and `lower_root`: no finding, no MIR), `assert_typed_limit` (no receipt, no body finding), and `assert_typed_limit_keeping_body_findings` for five sites (below). `*_limit_flaws` return the list of what is wrong, for the legacy-caps replay that collects failures. Each failure prints the report's statistics, which identify the cap.
- Every `ResourceLimit` assertion on those three report kinds in `tests/m5.rs`, `tests/m5/**`, `m4_8_callable_once.rs`, `m4_8_match_boolean_snapshots.rs` and `m4_8_scoped_join_caps.rs` now calls a helper (graph, body and typed sites, about 390 in all; `legacy_caps.rs` through the `_flaws` forms), and the hand-written no-MIR and no-finding lines after them are gone. The sites ST-5 lists are all among them. The messages some sites passed (fixture and mode names) are gone: the helper prints the statistics, and the failing line is the call site (`#[track_caller]`).
- `docs/agents/compiler-metering.md` rule 8: the Check names the three helpers.

## Findings

| Finding | Status | Where |
|---|---|---|
| Q1, ST-5, `ri-grwx` | fixed | graph fix in `graph.rs`; test `scoped_join::fresh_result::plain_conflict_reports_r0201_at_the_second_capture` asserts `assert_graph_limit` one short on each axis of 194,574 / 4,692 / 7. Reverting only `graph.rs` fails it at `fresh_result.rs:540` ("a limit report publishes no finding"; log `red-plain-conflict.log`) |
| ST-5, typed sites | stopped (reason): `analyze_typed` publishes body findings at `ResourceLimit` | Five sites (`rfc_cases.rs` `old_typed_caps` helper, used by four tests; `list_contents.rs` `r1_caller_provided_exclusive_source_stops_typed`; `captures/borrowed_invocation.rs`; `m4_8_scoped_join_caps.rs`; `m4_8_callable_once.rs` one-short loop) fail the full typed assertion, so they assert completion and no receipt through `assert_typed_limit_keeping_body_findings`. Q1 names only `analyze_ownership`, so the typed producer is not changed. Found bead `ri-68mu` (needs-triage); the chief decides. Every other typed site passes the full assertion |
| Tests that pinned the `ri-grwx` behaviour | changed | `m5::graph` late exhaustion (`graph.rs` "late exhaustion must retain local findings", `graph/delivery.rs` "late exhaustion lost a finding" with its `Rejected` delivery matches, `graph/scalar.rs` two `findings().len()` checks, `graph.rs` `local_blocker_is_not_a_finished_dependency_verdict_on_exhaustion`'s finding, `scoped_join.rs` the old live-parent graph cap keeping its R0201) asserted that a limit report keeps findings, at the one-short caps that run out after a finding was recorded. Those assertions are gone, as Q1 reverses them; the helper asserts no finding there |

`ri-grwx` is fixed: the planner closes the bead. `ri-68mu` (filed, needs-triage) is its typed twin.

## Caps

No published cap moved. The graph, targeted and typed caps in the test files are unchanged and the filtered runs below pass without an edited expectation.

## Not established

- Typed ResourceLimit sites outside the three binaries the gate names (`m4_5`, `m4_7`, `m4_8*`, `m3`, `m2`) and `lower_body` sites in `m4_8_ownership.rs` still assert by hand; they predate M5 and the review does not list them.
- Where the filtered runs and the logs are: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-iilr/`.
