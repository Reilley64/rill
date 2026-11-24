# `ri-px1.11.8` (C11): rule 8 residue at `ResourceLimit`, and helpers that cover every finding accessor (2026-11-24, city bead ci-8j91)

Child of `ri-px1.11`, spec `ri-2s0`. Source: focused re-review `ci-b4b2` of `d87e5dd2` (findings B-F1, B-F2, B-F4, A-F2 / C-2). Authority: the standing ruling in `chief-rulings-12.md` (any entry point found publishing at `ResourceLimit` gets the same fix) and `chief-rulings-18.md` (16:33 AEDT: enumerate the accessors in one place in `limit.rs`; corrected rule-8 sentence). Base `d87e5dd2`, branch `m5-ownership-cleanup/r3f-ri-px1.11.8`. Fix commit `57704d2d`. Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-8j91/`. No Rill program ran. **No published cap moved**; no `step`, `reserve` or `charge_key` was added or reordered (every production change runs after the meter stops, or is an accessor).

## Findings

| Finding | Outcome |
|---|---|
| B-F1 `analyze_typed` publishes discards, registration issues and overlaps | fixed (`57704d2d`), `m4_8_result_use::resource_limit_publishes_no_result_discard`; sweeps in `m4_8_ownership` (overlap, Guard overlap, nested-payload copy, `R0311` registration gap), `m4_8_primitives`, `m4_8_list` |
| B-F2 `ConversionReport::family()` publishes issues | fixed (`57704d2d`), `m4_7::conversion_family_publishes_no_issue_at_resource_limit` |
| B-F3 "no facts" sentence | not mine to scope; the chief ruled (`chief-rulings-18.md`). Rule 8's dated note carries the corrected sentence and keeps the ruling-17 sentence struck through as superseded. Ordinary and typed facts are not withheld |
| B-F4 `graph.rs` lost `assert_graph_limit(&short)` | fixed (`57704d2d`), `m5::graph::...::local_blocker_is_not_a_finished_dependency_verdict_on_exhaustion` calls the helper again for the entries - 1 run |
| A-F2 / C-2 hand-written `ResourceLimit` assertions | fixed (`57704d2d`), see "Sites switched" |

## What each entry point withholds (publication only, after `completion::finish` / after the budget stops)

`analyze_typed` (`TypedReport::withhold_findings`, `src/analysis/typed.rs`), on top of C8's lists:

- `result_use().discards()` (R0103): `ResultUseReport::withhold_findings`. `pending()` stays: it is a blocker list, not a finding.
- `into_registration_issues()` / `try_into_registration_issues()` (R0310 to R0312): `ConversionCore::withhold_findings` empties `ConversionFamily::issues` in the typed report's stored families.
- `ownership_overlaps()` and `collection_overlaps()` (R0500): emptied.
- `primitive_protection()` `overlaps` (R0500): emptied. This accessor was not in the review; reading every public accessor found it.
- The `Overlap` coverage verdict on `primitive_protection()`, `ownership_protection()` and `collection_protection()`: reads `Pending` at `ResourceLimit` (`Pending` is the existing state for "a prerequisite or contender is unresolved"). A reader could otherwise learn that an R0500 finding exists. Every in-crate reader (`ownership/scalar.rs`, `closed_release.rs`, `build.rs`) accepts a region only when it is `Clear`, so no outcome changes; they need a receipt, which a limit report never has.

`ConversionReport::family()` (`src/analysis/conversions.rs`): returns `Ok(None)` at `ResourceLimit` instead of the family. I withheld the family rather than only its issues, as `header_metadata` withholds the header (C10): one rule, and a family is an inventory fact the chief said a `ConversionReport` does not publish at the limit. The stored families keep their issues, because `ConversionReport::bodies` reads them (`analyze_with_conversions`) and withheld issues are still metered. `Ok(None)` is ambiguous (no family also gives it): documented on the accessor and in rule 8.

Other entry points: I read every public accessor of `TypedReport` and `ConversionReport`. None of the remaining ones carries a finding: roles, providers, `core_blockers()` and `result_use().pending()` are facts or blockers, and `conversion_provider` reads `registered`, not issues. No further entry point found.

## Helpers

- `typed_limit_flaws` checks every list above (and the lists C8 added). `conversion_limit_flaws` also checks `family(..)` issues for every family declaration of the graph.
- The accessors are enumerated in one place: the module comment at the head of `tests/support/limit.rs`, one list per report kind (graph, body, typed, ordinary, generics, conversion), with what is deliberately not checked. Rule 8's new dated note in `docs/agents/compiler-metering.md` repeats it as a table, one row per helper.
- New `assert_typed_limits_below(graph, full)`: one short on entries and depth, 64 evenly spaced work caps and the last 64 work caps, all through `assert_typed_limit`. It also checks that the exact cap replays the run.

## Red first

The helper extension and the new tests went in before the `src/` change; every log below is a run with `src/` unfixed (the fix reverted, per-list for the last row).

| Log | Fails with | At |
|---|---|---|
| `red-result-use.log` | `["publishes a Result discard"]` | work 127,873 (the full run is the test's source with both a tuple and a record destructure) |
| `red-m4_7.log` | `limited.family(id).unwrap().is_none()` (the `Ok(None)` pin) | `m4_7.rs:1263` |
| `red-m4_7-helper.log` | `["publishes a registration issue"]` (pin removed, helper alone) | work 74 / entries 33 / depth 1, the review's first leaking cap |
| `red-sweep-m4_8_ownership.log` | `["publishes an ownership overlap", "publishes an overlap verdict"]` | `m4_8_ownership.rs:74`, work 13,651 |
| `red-sweep-m4_8_primitives.log` | `["publishes a primitive overlap"]` | `m4_8_primitives.rs:443`, work 11,291 |
| `red-sweep-m4_8_list.log` | `["publishes a collection overlap", "publishes an overlap verdict"]` | `m4_8_list.rs:569`, work 32,627 |
| `red-registration.log` | `["publishes a registration issue"]` (only `ConversionCore::withhold_findings` removed) | `m4_8_ownership.rs:1017`, work 181,978 |

The reviewer's reproducer of B-F1 gave two discards on the `tuple` source alone; on the installed harness the test's tuple-and-record source gives its own count, read from the full run (no literal), so the test pins "every work cap below exact publishes none" rather than a count.

## Sites switched to the helper (A-F2 / C-2)

`m4_7.rs` (the `max_work: 0` conversion report, now `assert_conversion_limit`), `m4_8.rs` (the two one-short typed runs), `m4_8_ownership.rs` (the six sites in the brief, and a seventh the brief missed, in the probe's last check: it asserted `ResourceLimit` through a `use ...::{Incomplete, ResourceLimit}`), `m4_8_installed.rs` (two), `m4_8_installed_result.rs` (the work - 1 run), `m4_8_primitives.rs`, `m4_8_list.rs`, `m4_8_installed_conversions.rs`. Each site keeps the assertions it made beside the completion (`core_blockers()`, `option_role()`, statistics bounds). Duplicate hand lines removed at `m5/graph/inventory.rs` (`findings().is_empty()` and the MIR check; `inventory_complete()` stays, it is not in the helper). `m4_8_installed_result.rs:1448` is `assert_ne!(.., ResourceLimit)` and stays; so do the `Err` failure partials of `analyze_declaration_types`, `check_declarations`, `check_scalars` and `check_data` (ruling 15, confirmed in ruling 18).

No site failed under the helper: no new leak surfaced at a switched site. The leaks showed only under the new sweeps.

## Caps

None pinned, none moved: the filtered runs below pass without an edited expectation. `m5::legacy_caps`, `graph::inventory`, `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots` and `m4_8_callable_once` pass.

## Checks (logs in the evidence directory)

- `green-m5.log`: `--test m5 -- graph:: rfc_cases retained legacy_caps` (this covers `graph::inventory`): 193 passed.
- `green-narrow.log`: `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`, `m4_8_callable_once`, `m4_7`, `m4_8`, `m4_8_result_use`, `m4_8_ownership`, `m4_8_installed`, `m4_8_installed_result`, `m4_8_primitives`, `m4_8_list`, `m4_8_installed_conversions`: all pass.
- `fmt.log`, `clippy.log` (`-D warnings`), `docs.log` (`mise run check:docs`): clean. `git diff --check` clean; `git status --short | grep target/` empty.

## Not established

- Whether an `Err` partial should carry findings at all (`ri-qlh.9`, which the chief widened to cover the partial facts).
- The red logs run the new tests with `src/` unfixed; I did not revert each fix list one at a time except registration issues (the others fail first, in the same test, before any later list is reached).
