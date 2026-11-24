# Re-review ci-9qga fixes, part C15b: rule 8 round 3 and a wrong acceptance in the withheld typed inventory (2026-11-24, city bead ci-5n7s)

Ticket `ri-px1.11.12` for spec `ri-2s0`, from the second focused re-review `ci-9qga` of `2e13bc6e` (`notes/m5-ownership-cleanup/review/re-review-3.md`, "Subagent B"). Authority: chief rulings 12, 17 to 19, 21 ("2B-F2 (my error)", conditions (a) and (b)) and 23 (`notes/m5-ownership-cleanup/review/chief-rulings-*.md`); brief `briefs/build/review/R4F-C15.md` and `R4F-C15b.md`. The first builder (`ci-sw67`) stopped at condition (a) with nothing committed; this ticket reuses its branch `m5-ownership-cleanup/r4f-ri-px1.11.12` from `2e13bc6e`.

Publication only. No `step`, `reserve` or `charge_key` moved: every change runs after the meter stops (`withhold_findings`, the `publishes_nothing_at_limit` guards and the test support). No published cap and no pinned outcome moved: the pinned binaries named under "Checks" pass without an edited expectation.

## The wrong acceptance (ruling 21 (a))

**Cause.** `analyze_typed` at `ResourceLimit` called `OrdinaryAnalysis::withhold_findings`, which emptied the inventory's findings and issues but left `completion()` at `Incomplete`. The public readers of an `&OrdinaryAnalysis` (`select_inputs`, `analyze_generics`, `analyze_conversions`, which also reads it through `extend_nominal_types`) check the completion and read the findings. They saw a complete inventory with no declaration finding. The bodies of the typed report (`GenericReport`) had the same gap: findings emptied, completion left at the stage's own value.

**Window.** From C8 `6f35705e` (the merge that added `withhold_findings`) to `33eaab59`, the first commit of this ticket (the fix). Fifty merges on the integration branch lie between C8 and `2e13bc6e`.

**What a public caller could get in the window** (measured by `ci-sw67` at `2e13bc6e` with `probe_c15.rs.txt`, log `probe-a.log`, and reproduced here by the red log `red-c15b-wrong-acceptance.log`):

- `analyze_generics(report.inventory(), ..)` reads `BodyCompletion::Checked` for a body the exact cap leaves unchecked. Source `type Gone = { x: Missing }; enum E { Left, Right } function g(): bool { return true; } function f<A>(a: A): bool; implement f(a: E): bool { return true; }`: exact cap 1,066 / 312 / 2; work caps 823 to 1,065 give `g` `Checked`. `type P = { x: bool; x: bool }` (work caps 885 to 1,138) and `type Loop = { a: Loop }` (825 to 1,073) do the same.
- `select_inputs(report.inventory(), ..)` returns a `Winner` where the exact cap gives the blocker `DeclarationPrerequisite` (the review's reproducer, `Gone` and `Loop`, work caps 866 to 1,055).
- In this ticket's red run the conversion path read the same: `analyze_conversions(report.inventory(), ..).bodies()` had one body `Checked` and a `ConversionReport` that was not `ResourceLimit`.

**What it could not get: a typed `Checked`, a receipt or MIR.** `TypedReport::receipt()` is `Some` only when the typed completion is `Checked`; a limited typed report has completion `ResourceLimit`. `lower_body`, `lower_root` and `analyze_ownership` take a `&TypedReceipt`, whose `report` field is private and which only `receipt()` builds, so none of them can run on a limited typed report. `limited_typed_reports_are_never_more_permissive_*` asserts `receipt().is_none()` and typed completion `ResourceLimit` at every limited cap of each source, and that `typed_checked` and `receipt` never exceed the exact cap's. The pipeline (the ownership and MIR stages) is unaffected: they are unreachable from the inventory, which is a `&OrdinaryAnalysis`, not a receipt.

**Fix.** `OrdinaryAnalysis::withhold_findings` and `GenericReport::withhold_findings` set `completion` to `ResourceLimit` where they empty the value (ruling 23, item 1).

Tests (`crates/compiler/tests/m4_8_limit_readers.rs`, new; red log `red-c15b-wrong-acceptance.log`, green `green-c15b-inventory.log`):

| Test | Asserts |
|---|---|
| `limited_typed_reports_are_never_more_permissive_with_a_missing_type` | at every work cap from 0 to one short, and one short on entries and on depth, a limited typed report has no receipt, its inventory and bodies read `ResourceLimit`, and `select_inputs`, `analyze_generics`, `analyze_conversions(..).bodies()` and the typed bodies are no more permissive than at the exact cap |
| `..._with_a_duplicate_field`, `..._with_a_self_recursive_type`, `..._with_a_missing_and_a_recursive_type` | the same, for `type P = { x: bool; x: bool }`, `type Loop = { a: Loop }` and the review's reproducer |
| `a_limited_typed_inventory_reads_resource_limit` | one work short of the exact cap, the inventory reads `ResourceLimit` and `assert_typed_limit` holds |

Swept: the four sources above, every work cap, entries minus one and depth minus one, with readers `select_inputs`, `analyze_generics`, `analyze_conversions` (and `bodies()` over it) and the typed `bodies()`. Not swept: sources outside these four.

## Ruling 21 (b): the audit of withheld values

Every value a rule-8 fix empties, and every public consumer that takes it as an input and could read empty as "nothing there". A consumer that takes only a `&TypedReport` reads its `completion()`, which is `ResourceLimit`; the table lists consumers that take the value apart from its owner.

| Withheld value | Consumer taking it as input | Checks completion first | One-short test |
|---|---|---|---|
| typed `inventory()` (`OrdinaryAnalysis`: findings, issues, header findings) | `select_inputs`, `analyze_generics`, `analyze_conversions`, `extend_nominal_types` | yes, since this ticket (the completion is `ResourceLimit`) | the sweep above, readers of all four |
| typed `bodies()` (`GenericReport`: findings, conditional findings, issues, annotations, conversion uses, goals, ordinary calls and values) | none takes a `GenericReport` | its own completion is `ResourceLimit` since this ticket | `..never_more_permissive..` asserts it at every limited cap |
| `ConversionReport` facts, headers and `family` | `bodies()`, `query_pair` | `query_pair` refuses on `ResourceLimit` (`pairs.rs`); `bodies()` reads the input's completion | `conversion_report_publishes_no_finding_list_at_resource_limit`, the pair sweep in `pair_queries_keep_destination_bound_variables_exact_and_filter_receivers`, the conversion cases of the readers sweep |
| coverage verdicts, overlaps, Result discards, registration issues of the typed report | none takes them apart from the typed report | read through `TypedReport`, whose completion is `ResourceLimit`; its `receipt()` is `None` | `assert_typed_limits_below` and the one-short pins in `m4_8_ownership`, `m4_8_primitives` |
| `ConversionUse`, goals, ordinary calls and values | none takes them apart from the `GenericReport` | the report's completion is `ResourceLimit` | `limited_uses`, `rejected_registrations`, `numeric`, `user_dispatch` in `m4_8_installed_conversions.rs` |

Not audited: consumers outside `crates/compiler` (the CLI and the loader read none of these values; `grep` for the accessors found none). The audit read each public function that takes `&OrdinaryAnalysis`, `&TypedReceipt`, `&GenericReport` or `&ConversionReport` (`select_inputs`, `analyze_generics`, `analyze_conversions`, `lower_body`, `lower_root`, `analyze_ownership`).

## Items

| Item | Finding | Outcome | Where |
|---|---|---|---|
| 1 | 2B-F1, widened | withheld; red first | below |
| 2 | 2B-F4 | one-level exhaustive destructures | below |
| 3 | 2B-F3 | `SelectionReport` and `PairReport` have a guard and a helper | below |
| 4 | 2B-F2 | the wrong acceptance, above | above |
| 5 | 2B-F7 | `Overlap` and `Pending` pinned for primitive coverage | below |
| 6 | 2B-F5 | recorded in the rule-8 note | `docs/agents/compiler-metering.md` |

### Item 1: conversion uses and goals

A `ConversionUse` with no premise (`premise.is_none()`) discloses a finding through `outcome` `NoMatch`, `NumericGap` or `Ambiguous`, `error_agreement` `Disagrees` (R0313), `result_agreement` or `descriptor_agreement` `Some(false)`, and `blockers` carrying `Registration` (a rejected registration). At a limit each reads as undecided: `outcome` `None`, `error_agreement` `NotDetermined`, the agreement `None`, and no `Registration` in `blockers`. Why these: they are the states `conversion_failure` turns into a finding, so a reader who sees them has the finding. Why not another state: `None`, `NotDetermined` and "no blocker" are what the same use reads before the checker decides; the report's completion (`ResourceLimit`) says it is not complete.

Goals: a goal with no premise whose state is `Unresolved(ConversionPair(Registration))` reads `Unresolved(ResourceLimit)`; the goal of a conversion use with no premise whose state is `Disproved`, or whose `conversion_outcome` is one of the three above, reads `Unresolved(ResourceLimit)` and no outcome. `OutputAgreement::Disagrees` on `OrdinaryCall` and `OrdinaryValue` reads `NotDetermined`: a disagreement is a type mismatch, conditional under a premise, and both finding lists are withheld.

Red: `red-item1-conversions.log` (the withholding disabled): `["publishes a conversion use error disagreement", "publishes a conversion use agreement that is a mismatch", "publishes a conversion use outcome that is a finding", "publishes a conversion use goal verdict that is a finding", ..]` at the `numeric` source, and the Registration goal state at `rejected_registrations`. Green: `green-item1-conversions.log`.

Tests (`m4_8_installed_conversions.rs`): `rejected_registrations` (`Registration` blocker and goal state, exact and one short, `assert_typed_limits_below`), `numeric` (`NumericGap` and `Disagrees`), `user_dispatch` (`Disagrees`), new `limited_uses` (`NoMatch`, `Ambiguous` and a false `result_agreement`, each pinned at the exact cap and swept). Helper: `limit::conversion_use_flaws` and `assert_typed_limit_uses`; the variants are listed at the head of `tests/support/limit.rs`.

**Swept and not withheld, with the reason:**

- `GoalState::Disproved` of a goal that is not a conversion use's own: a candidate's eliminated requirement, which is not a finding (a program can select another candidate). Withholding it would change an observable that is not a finding, which ruling 23 (item 4) sends to the chief first.
- `FamilyFacts::duplicate_keys`: a fact of the inventory, pinned by `m4_5` and `m4_7` (`ri-q9lj.md` recorded the same); the finding is `DuplicateImplementation` in the finding list.
- `descriptor_agreement` `Some(false)`: withheld, but no source I tried (`narrow` as a descriptor with a wrong error type) reaches it, so no test pins it at the exact cap.
- `ConversionUse` `InsufficientContext` (no `destination` with an `actual_source`): derived from two fields that are facts; not withheld, and not swept.
- `OrdinaryCall::outcome` `NoMatch` or `Ambiguous` (`NoApplicableImplementation`, `AmbiguousImplementation` at the use when the callee has an installed ordinary scope): a selection verdict that also describes candidates; not withheld, not swept. Recorded for the planner.

### Item 2: one level down

`PrimitiveProtection`, `OwnershipProtection`, `CollectionProtection`, `ConversionFamily`, `HeaderMetadata`, `PartialFacts`, `GoalFacts`, `ConversionUse`, `OrdinaryCall` and `OrdinaryValue` each have a `publishes_nothing_at_limit` that destructures the struct without `..`; the report-level guards call them. `red-item2-new-field-fails-to-compile.log`: a new field on `HeaderMetadata` fails with E0027 at the destructure. The enum variants that disclose a finding are listed at the head of `tests/support/limit.rs`, beside the accessor list.

### Item 3: the report-kind list

Every type whose completion can be `ResourceLimit`, its guard and its helper:

| Type | Guard | Helper |
|---|---|---|
| `GraphReport` | `publishes_nothing_at_limit` | `assert_graph_limit` |
| ownership `Report` | `publishes_nothing_at_limit` | `assert_body_limit` |
| `TypedReport` | `publishes_nothing_at_limit` | `assert_typed_limit`, `assert_typed_limit_uses`, `assert_typed_limits_below` |
| `OrdinaryAnalysis` | `publishes_nothing_at_limit` | `assert_ordinary_limit` |
| `GenericReport` | `publishes_nothing_at_limit` | `assert_generics_limit` |
| `ConversionReport` | `publishes_nothing_at_limit` | `assert_conversion_limit` |
| `SelectionReport` (new) | `publishes_nothing_at_limit` | `assert_selection_limit` |
| `PairReport` (new) | `publishes_nothing_at_limit` | `assert_pair_limit` |

The hand-written sites `m4_5.rs` (the exhausted-inventory selection), `m4_7.rs` (`low` and `growing`) call the helpers. `SelectionReport` is consistent with `PairReport`: `SelectionReport::outcome` is assigned only at the two returns of `query`, after its last metered step, so a limited selection never has one. `a_limited_selection_names_no_outcome` sweeps every work cap on a winner and a no-match source and the helper holds at each; it was green before the guard, so the guard pins the state and changes no code path. `PairReport` clears `outcome`, `selected_signature` and `selected_error` at a limit already; the pair sweep (every work cap below the exact one) holds for `error_agreement` and the `Registration` blocker as well, so no clearing of those was added.

### Item 5: primitive coverage

`m4_8_primitives::installed_primitive_providers` (`protected_primitive_replacement`) now pins `PrimitiveCoverage::Overlap` at the exact cap and `Pending` one work short with no overlap, as `m4_8_ownership` does for ownership. `red-item5-primitive-coverage.log`: with the `Overlap` to `Pending` withholding removed, the sweep and the pin fail.

## Checks

Logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-5n7s/`. See the report for the list and results.

## What it did not establish

- The full M5 binary and the full gate run in hosted CI, not here.
- Sources beyond the four of the readers sweep and the conversion cases named above.
- The items listed under "Swept and not withheld".
