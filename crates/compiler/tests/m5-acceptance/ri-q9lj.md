# `ri-q9lj`: `analyze_ordinary` and `analyze_generics` publish no finding at `ResourceLimit` (2026-11-24, city bead ci-jr0z)

Review-2 fix pass, child of `ri-px1.11`; twin of `ri-68mu` (`analyze_typed`, `ri-68mu.md`) and `ri-grwx` (C1). Authority: the chief of staff's standing ruling of 2026-11-24 14:20 AEDT (`chief-rulings-12.md`): any further analysis entry point found publishing at `ResourceLimit` gets the same fix, publication only, stopping if a published cap moves. Base `6f35705e` (the integration branch with C8 merged), branch `m5-ownership-cleanup/r2f-ri-q9lj`.

## What changed

- `src/analysis/ordinary.rs`: `analyze_ordinary` withholds, at `Completion::ResourceLimit`, `type_facts().findings()`, `declaration_issues()` and every declaration header's `header_metadata(..).type_metadata().findings()` (the probe showed all three published at the limit; the header lists were the "not probed" item of `ri-68mu`). `OrdinaryAnalysis::withhold_findings` (C8's mutator, which `analyze_typed` calls on its inventory) now also clears the header lists, so the typed report's inventory publishes none either.
- `src/analysis/scalars/generics.rs`: `analyze_with_conversions`, the one body of `analyze_generics` and `ConversionReport::bodies` (the same `GenericReport`), withholds at `ScalarFailureKind::ResourceLimit` with C8's `GenericReport::withhold_findings`: `findings()`, `conditional_findings()`, `declaration_issues()` and `annotations().findings()`. The integrated path (`analyze_typed`) is unchanged.
- No `step`, `reserve` or `charge_key` moved. One consequence needed care: `analyze_generics` meters each declaration issue of its ordinary input (`reserve(1 + extra)`) before it notices that input is `ResourceLimit`. Clearing the issues would have lowered that report's statistics by 2 work and 2 entries (402 ordinary work gave 404 work and 92 entries, now would have given 402 and 90). So `OrdinaryAnalysis` keeps the withheld issues in a private list (`withheld_issues`), and the generics loop meters `metered_issues()` (published plus withheld). The pre-fix and post-fix statistics of `analyze_generics` over every ordinary `ResourceLimit` input of two sources (806 rows) are identical: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-jr0z/chain-pre.txt` and `chain-post.txt` (the first run of the post-fix probe, before this list, differed on rows 402 to 426; `withheld_ordinary_issues_are_still_metered_by_generics` pins the value).
- `tests/support/limit.rs`: `ordinary_limit_flaws` / `assert_ordinary_limit(graph, report)` and `generics_limit_flaws` / `assert_generics_limit(report)`, with `header_finding(graph, report)` (the graph names the headers). `docs/agents/compiler-metering.md` rule 8 names both entry points (and `ConversionReport::bodies`).

## Lists withheld

| Entry point | Withheld at `ResourceLimit` |
|---|---|
| `analyze_ordinary` | `type_facts().findings()`, `declaration_issues()`, each header's `type_metadata().findings()` |
| `analyze_generics` (and `ConversionReport::bodies`) | `findings()`, `conditional_findings()`, `declaration_issues()`, `annotations().findings()` |
| `analyze_typed` (inherits) | the inventory's header `type_metadata().findings()` (new); every list C8 withheld is unchanged |

## Sites switched to the helper

- `m4_5.rs`: the one-short loop and the 200-entries-short partial in `exhausted_products_keep_known_headers_but_cannot_supply_negative_input_evidence`, and the partial in `wide_admission_products_charge_copies_to_work_with_entries_to_spare` (`assert_ordinary_limit`; each was `completion() == ResourceLimit`).
- `m4_6_evidence.rs`: `type_growth_exhausts_a_limit_instead_of_reporting_a_cycle_or_absence` (max depth 10) and the one-short loop in `retained_goal_work_and_entries_have_independent_exact_boundaries` (`assert_generics_limit`).
- `m4_6_nominal.rs`: `a_repeating_default_is_pending_but_a_growing_application_exhausts_depth` (depth 24).
- Not switched, by design: `m4_1.rs`, `m4_2.rs` (`analyze_declaration_types`, `check_declarations`: `Err` with a partial, pinned, with the chief of staff), `m4_3.rs`, `m4_4.rs` (`check_scalars`, `check_data`: `Err` with a failure; not an `Ok` report, not in the ruling), `m4_7.rs`'s `select_inputs` and `analyze_conversions` (`ConversionReport::completion`, a different report).

## Red first

Tests added, each failing with only the `src/` changes reverted (`red.log`, `red-header.log`, `red-typed-header.log`, `fix-src.patch` in the evidence directory):

- `m4_5::resource_limit_publishes_no_finding_list`: "publishes a type finding" at 57 work; with only the header clearing reverted, "publishes a header metadata finding" at 177 work; with the ordinary sweeps skipped, the typed inventory header finding at typed work 441.
- `m4_6_evidence::resource_limit_publishes_no_finding_list`: "publishes a declaration issue" at 429 work.
- `m4_6::resource_limit_publishes_no_conditional_finding`: "publishes a conditional finding" at 219 work.
- `m4_7::conversion_bodies_publish_no_finding_at_resource_limit`: "publishes a body finding" at 99 work.

Each test replays the exact cap (statistics equal the full run, completion not `ResourceLimit`, the finding lists published: type facts 2, declaration issue 1 for the first source, header finding, annotation findings at least 2, the conditional `Arity` finding, the body finding), then every work, entries and depth cap below exact (the helper, so the boundary is pinned on both sides). The existing sites above were switched in the same commit; they pass without an edited expectation. `withheld_ordinary_issues_are_still_metered_by_generics` passes before and after (a guard, not a red test).

## Caps

No published cap moved. The filtered runs below pass with no edited expectation; `m5::legacy_caps` passes; the three `m4_8_*` cap binaries pass; the 806-row statistics comparison is identical.

## Not established

- The 806-row statistics comparison swept `max_work` only (entries and depth at default); the entries and depth axes hold by construction (`metered_issues()` meters the same list as before) but were not swept.
- `withheld_issues` is state C8 did not have; it exists only to keep `analyze_generics`' metering of an ordinary `ResourceLimit` input unchanged, and nothing publishes it.
- `ConversionReport::type_facts()` and `header_metadata()` read the ordinary input, which is a separate report with its own completion; they were not probed at a conversion `ResourceLimit`.
- `check_scalars` and `check_data` failure partials, `analyze_declaration_types` and `check_declarations` partials: out of scope (see above).
- Finding lists of `FamilyFacts` (`duplicate_keys`, `comparisons`) are facts, not findings, and were not withheld.

Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-jr0z/`.
