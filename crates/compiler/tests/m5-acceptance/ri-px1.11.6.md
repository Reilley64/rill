# Review ci-0mdr fixes, part C7: records, the last pins and the final section (2026-11-24, city bead ci-ru4p)

Last child of the single fix pass of review `ci-0mdr` (second review of `81d317b3`), `ri-px1.11.6` under `ri-px1.11`. Base `origin/implementation/m5-ownership-cleanup` at `61f22ae3` (it holds C6, `34d39c4e`, and C9, `61f22ae3`); branch `m5-ownership-cleanup/r2f-ri-px1.11.6`. Chief rulings 11 to 16 (`review/chief-rulings-11.md` to `-16.md` in the city notes) win over the ticket body. Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-ru4p/`. No Rill program ran. This file's findings are consolidated with the others in [`ri-px1.11.md`](ri-px1.11.md), "Review ci-0mdr fixes".

## Findings

| Item | Status | Where |
|---|---|---|
| ST-4 (ruling 13) | recorded, pinned | `rfc_cases::o12::disposer_record_results_pay_the_disposer_lookup_and_keep_their_outcomes` |
| ST-9 at `i32_arithmetic.rs:202` (ruling 13) | recorded, pinned | `o05_field_arithmetic::local_descriptor_write_through_another_binding_pays_one_work`, `o05_field_arithmetic::local_descriptor_write_through_a_reborrow_pays_one_work` |
| Standing ruling: `ConversionReport::type_facts` and `header_metadata` at `ResourceLimit` | fixed (publication only) | `m4_7::conversion_report_publishes_no_finding_list_at_resource_limit`; `src/analysis/conversions.rs` |
| Ruling 15 (rule 8 and `Err` partials) | recorded | `compiler-metering.md`, note under rule 8 |
| Q2, ST-14 | recorded | `compiler-metering.md`, note under rule 11 |
| ST-18, R-2 | recorded, transcript only | [`ri-px1.11.6-d95ae9e/`](ri-px1.11.6-d95ae9e/README.md) |
| ST-19 | recorded, dated supersession lines | `ri-2s0.4.md`, `ri-px1.11.md`, `ri-px1.11.3.md`, the ledger (H1, `pre` and `computed`, `ri-2s0.13`), `compiler-metering.md` rule 3. RFC 0007 line 542 left alone |
| ST-20 | recorded | the final record says where logs live |
| SP-8 | recorded, `ri-qlh.11` | the planner filed the bead (mail `ci-wisp-iuocp2` and its reply); the ledger's K3 bullet points to it |
| Item (a), the MUST KILL sites | recorded, found bead `ri-qlh.10` | below |
| Consolidation of C1 to C9, final record | recorded | `ri-px1.11.md`; the last section of `m5-acceptance.md` |

Name corrections to the brief: C1's evidence file is `ri-px1.11-c1.md` (no `ri-grwx.md`); the others are as named.

## ST-4 and ST-9 pins (chief ruling 13, 2026-11-24 14:47 AEDT)

Both are pins of current behaviour and pass on the base, so rule 10's red-first does not apply. Each root keeps its outcome (`Incomplete`) and blocker, both load modes, targeted and graph root, exact and one short on each axis (`check_cases`, `check_incomplete_rows`). Each old cap gives `ResourceLimit` through `crate::limit::assert_body_limit` or `assert_graph_limit`. Current numbers were measured on this base; the parents in a `git worktree add --detach` of `7002855e` (ST-4) and from C4's probe at `f24d825b` (ST-9, `ci-e8rg/probes/parent2.out`). Every number matches the ruling; none differs.

| Test | Root | Old cap (parent) | Current cap | Blocker |
|---|---|---|---|---|
| `rfc_cases::o12::disposer_record_results_pay_the_disposer_lookup_and_keep_their_outcomes` | `within`, `return (&make()).x;` | 1,875 / 27 / 3 | 2,252 / 90 / 6 | `(&make()).x` |
| | `bound`, `let v = &make();` | 1,891 / 34 / 4 | 2,263 / 97 / 4 | `make()` |
| | whole graph (`Incomplete`) | 187,649 / 4,260 / 5 | 188,398 / 4,386 / 6 | |
| | `make` (`Checked`, control) | | 2,339 / 137 / 4 | |
| | typed (`Checked`) | | 327,240 / 47,914 / 12 | |
| `o05_field_arithmetic::local_descriptor_write_through_another_binding_pays_one_work` | `viaOther` | 2,245 / 103 / 2 | 2,246 / 103 / 2 | `*d += 1i32;` |
| | whole graph | 186,741 / 4,198 / 5 | 186,742 / 4,198 / 5 | |
| `o05_field_arithmetic::local_descriptor_write_through_a_reborrow_pays_one_work` | `viaReborrow` | 2,249 / 105 / 2 | 2,250 / 105 / 2 | `*d += 1i32;` |
| | whole graph | 186,865 / 4,200 / 5 | 186,866 / 4,200 / 5 | |

At `7002855e` the two ST-9 bodies are typed `Incomplete` (no ownership cap exists there); their parent is `f24d825b`, where the route did not exist. The `Tracked` source of ST-4 is `type Tracked = { x: i32 }; implement dispose(value: Tracked): void {}` with `make`, `within` and `bound`. Probe sources and outputs: `ci-ru4p/probes/` (`parent-7002855e.out`, `base.out`). The type and literal checks that review 2 named at `i32_arithmetic.rs:202` cannot fire there: recorded in [`ri-px1.11.3.md`](ri-px1.11.3.md). The site of ST-4 and a link to `ri-zee8` are in a comment on `ri-qlh.4` ("carry a type's custom disposer as an M4 fact"). Both exceptions are named in the rule-1 note of `compiler-metering.md`.

Helper change: the closure body of `o05_field_arithmetic::keeps_parent_caps` is now `check_incomplete_rows`, called by it and by the new tests; behaviour unchanged.

## `ConversionReport::type_facts` and `header_metadata` at a conversion `ResourceLimit`

The standing ruling (chief, 14:20 AEDT, `chief-rulings-12.md`) named a candidate C9 did not probe. Both accessors read `self.input()`: the caller's own ordinary report, or the extended report that `extend_nominal_types` builds when the input has a generic-default or type-argument obligation. Neither carries the conversion's completion, so a conversion `ResourceLimit` still showed the input's findings.

Probe (a throwaway sweep over `analyze_conversions`, then the committed test): two sources, each with a missing type and a self-referential type or a missing value, one with a generic default (extended input), one without. `max_work`, `max_entries` and `max_depth` each swept from 0 to one below the full run's statistics (721 / 178 / 2 and 37 / 17 / 1), the ordinary input analysed at default limits.

| Source | Limited reports | Published a type finding or a header finding |
|---|---|---|
| 0: missing field type, inline cycle, missing value, `function g<T = Absent>(x: Nope): T;` and an `into` implementation | 901 | 808 (up to 2 type findings and 1 header finding) |
| 1: missing field type and an `into` implementation | 55 | 55 (1 type finding) |

Fix, publication only, in `src/analysis/conversions.rs`: at `Completion::ResourceLimit`, `type_facts()` returns an empty `PartialFacts` the report owns and `header_metadata(id)` returns `Ok(None)` (after validating the id). `bodies()` and the other internal readers use `self.input()` directly, so no statistic moves: `m4_7` passes unedited and no `step`, `reserve` or `charge_key` was touched. Red first: `red-conversion.log` (with the test and the helper, before the fix) fails with `["publishes a type finding", "publishes a header metadata finding"] at Statistics { work: 0, entries: 0, max_depth: 0 }`. `tests/support/limit.rs` gains `conversion_limit_flaws`, `assert_conversion_limit` and `conversion_header_finding`; the test replays the exact cap (the findings are published there: 2 type findings and a header finding for source 0, 1 and none for source 1) and every cap below it on each axis. Rule 8 in `compiler-metering.md` names the helper.

The fix withholds more than findings: at `ResourceLimit` these two accessors publish no declarations, uses or obligations either, because a `ConversionReport` borrows its input and cannot copy the input without its findings (no `Clone` on the report types). Nothing in the repository reads them at a limit. Recorded as a stated limit in the final record. No published cap moved.

## Item (a): the MUST KILL sites

Bead `ri-qlh.10` (rill store, needs-triage, parent `ri-qlh`, relates to `ri-2s0`). C4 restructured none of the four sites of review 2's Comments section, and none of the later fixes did. Lines at `61f22ae3` (the review's lines at `81d317b3` in parentheses):

- `ownership/build.rs:3041-3042` (3028-3029): the disposer note in `binding_temporary_borrow`.
- `ownership/build.rs:7978-8002` (7951-7975): the screen and charge prose in `family_retaining_shape`.
- `ownership/build.rs:8296-8298` (8271-8273): the written-destination note in `call`.
- `ownership/build/i32_arithmetic.rs:192-194` (191-197): the zero-count note.

The bead also names the review's fifth MUST KILL site, the retained-contents split in `analysis.rs:77-133` and `ownership.rs:2100-2132`, which the brief's "four" does not count and which no fix restructured either.

## Notes added

- `docs/agents/compiler-metering.md`: two named rule-1 exceptions (ST-4, ST-9); rule 3's superseded `ri-30q` line (ST-19); rule 8's `assert_conversion_limit` and the ruling-15 note; rule 11's Q2 note.
- Evidence files: `ri-px1.11.3.md` (ruling 13 update), `ri-2s0.4.md` (supersession line), `ri-px1.11.md` (supersession lines, ST-18, the section "Review ci-0mdr fixes").
- `m5-acceptance.md`: supersession lines (H1 table, `pre` and `computed`, `ri-2s0.13`), the ST-18 note, the K3 pointer to `ri-qlh.11`, the update line on the R-D6b record and the final section.

## Checks (local, targeted; logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-ru4p/`)

At the branch before the base merge (the base tip is still `61f22ae3`): `m5-filters.log` (`--test m5 -- rfc_cases retained legacy_caps graph::inventory o05_field_arithmetic`: 156 passed); `m4_7.log` (17), `m4_5.log` (18), `m4_6.log` (26), `m4_6_evidence.log` (26), `m4_8.log` (20), `m4_8_scoped_join_caps.log`, `m4_8_match_boolean_snapshots.log`, `m4_8_callable_once.log` (each 1); `fmt.log` and `clippy.log` clean (`cargo fmt --all -- --check`, `cargo clippy --locked --no-default-features -p compiler --tests -- -D warnings`); `docs.log` for `mise run check:docs`. `git diff --check` and `git status --short | grep target/` are empty.

## Not established

- The two new ST-4 and ST-9 pins pass on the base; they pin behaviour, they do not reproduce a regression.
- Probe 2 and the focused re-review of the fix range are not part of this child.
- The `d95ae9e` numbers are a transcript, not a re-run.
