# Review ci-0mdr fixes, part C5: charge the two new path-keyed probes; reserve the scratch Vec (ST-3, ST-8) (2026-11-24, city bead ci-h68b)

Child `ri-px1.11.4` of the single fix pass `ri-px1.11` for review `ci-0mdr`. Base `origin/implementation/m5-ownership-cleanup` at `eabe14ad` (it holds C1 `ead19c6a` and C4 `eabe14ad`). Branch `m5-ownership-cleanup/r2f-ri-px1.11.4`. Every "old" number below is what the base measured: the tests were written first and were red on the base (`red-5.log`: the graph statistics of each new test differ from the pinned ones); the new numbers were computed from the rule before the fix and matched the measurement. Logs are in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-h68b/`.

## Findings

| Finding | Status | Where |
|---|---|---|
| ST-3, `self.locals.get(path.root)` in `scalar_field_through_reference` (`ownership/build.rs`) | fixed | The fixed `budget.step(1)` is replaced by `graph::charge_key(&path.root.module, probes, ..)`, `probes = 1 + ilog2(locals.len().max(1))`, the form `arithmetic_local_of` already uses. Test `rfc_cases::field_through_reference_charges_its_local_probe`, red on the base |
| ST-3, `self.locals.insert(&parameter.declaration.id, slot)` for a `List<&Record>` parameter (`ownership/build.rs`) | fixed | `reserve(1)` stays (the entry); `charge_key(&parameter.declaration.id.module, probes, ..)` is added before the insert, with the same `probes`. Test `rfc_cases::list_contents::list_reference_parameter_charges_its_insert`, red on the base |
| ST-8, `scratch: Vec<Slot>` in `i32_equality` (`ownership/build/i32_arithmetic.rs`) | fixed | `budget.reserve(n)` before the collect, `n` = the number of projected call operands (0, 1 or 2). A plain `i32 == i32` reserves 0. Test `rfc_cases::call_field_equality_reserves_its_scratch`, red on the base |

Both screens come after every check the code makes before the site (rule 1): the new charges sit where the old `step(1)` and `reserve(1)` sat, after the shape tests. Only the old `local()` sites and the other `locals` probes stay uncharged, deferred to `ri-qlh.3` (rule 3).

## The new tests (exact, and one short on each axis through `check_receipt`; the old cap through C1's helpers)

Typed statistics are unchanged by this ticket. `m` below is `main.rill`, so one probe costs `m.len() + 1 = 10`.

| Test | Root | Old (base) | New | Reason |
|---|---|---|---|---|
| `field_through_reference_charges_its_local_probe` | `viaParam(view: &Point)`: `return view.x;` | 1,995 / 88 / 3 | 2,004 / 88 / 3 | one local, one probe: 10 instead of 1 (+9) |
| | `afterLocals` (`view` and three `let`s) | 2,278 / 232 / 3 | 2,307 / 232 / 3 | four locals, three probes: 30 instead of 1 (+29) |
| | whole graph | 187,294 / 4,397 / 5 | 187,332 / 4,397 / 5 | +9 + 29 |
| `list_reference_parameter_charges_its_insert` | `keep(users: List<&User>)` | 1,912 / 44 / 1 | 1,922 / 44 / 1 | empty map, one probe: +10 |
| | `keepThree(a, b, c)` | 1,985 / 68 / 1 | 2,025 / 68 / 1 | probes 1, 1, 2: +40 |
| | whole graph | 186,623 / 4,198 / 5 | 186,673 / 4,198 / 5 | +10 + 40 |
| `call_field_equality_reserves_its_scratch` | `same`: `a().x == b().x` | 7,894 / 754 / 4 | 7,896 / 756 / 4 | two scratch slots: +2 work, +2 entries |
| | `one`: `a().x == 2i32` | 4,972 / 437 / 4 | 4,973 / 438 / 4 | one: +1, +1 |
| | `a`, `b`, `plain` (`n == 2i32`) | 2,407 / 176 / 4 (both), 2,110 / 152 / 4 | unchanged | no projected call operand |
| | whole graph | 191,477 / 5,125 / 5 | 191,480 / 5,128 / 5 | +3, +3 |

The old graph and root caps of the three tests above are asserted `ResourceLimit` (no MIR, no finding) in `list_contents::old_root_caps` (called from each test), both load modes. The scratch reserve is independently one short: the `check_receipt` replay of `same` at 755 entries gives `ResourceLimit`.

## Caps that moved in existing tests (rule 11, third case: the ticket mandates the charge)

The brief names the charge. Every move is accounted for by these three sites and by nothing else: each root's work moved by `10 x probes - 1` for a field read (+9, +19, +29), by `10 x probes` for a list-parameter insert (+10), or by the scratch reserve (+1 / +2 work and entries); each whole graph moved by the sum of its roots' moves (the roots that are not pinned are in the sum, as the totals match). Entries, depth and typed statistics did not move except by the scratch reserve. The old cap of each is asserted `ResourceLimit` in the test named in the last column (the root caps in the test's `old_root_caps` row, the whole graph in the same row).

| Test (`rfc_cases::`) | Root | Old | New | Reason | Old cap asserted by |
|---|---|---|---|---|---|
| `list_contents::family_list_destination_forms` (`AFTER_RETAIN`) | `reuse` | 3,489 / 322 / 5 | 3,508 / 322 / 5 | `source.id`, two locals, two probes (+19) | `list_contents::list_roots_pre_charge_caps_now_give_resource_limit` |
| | graph | 193,669 / 4,807 / 5 | 193,688 / 4,807 / 5 | +19 | same |
| `list_contents::r1_source_read_after_the_call_checks_with_its_sequence` | `reuse` | 3,457 / 306 / 5 | 3,476 / 306 / 5 | +19, as above | same |
| | graph | 189,806 / 4,408 / 5 | 189,825 / 4,408 / 5 | +19 | same |
| `list_contents::r1_stored_source_outlives_the_call_loan` | `viewReadThenWrite` | 2,761 / 346 / 4 | 2,780 / 346 / 4 | view read, two probes (+19) | same |
| | graph | 198,597 / 5,538 / 6 | 198,616 / 5,538 / 6 | +19 | same |
| `o01_call_field_equality_operands_check_and_siblings_keep_their_screens` | `bare` | 5,192 / 547 / 4 | 5,193 / 548 / 4 | scratch reserve (+1, +1) | `o_rows_pre_charge_caps_now_give_resource_limit` |
| | `grouped` | 5,197 / 547 / 5 | 5,198 / 548 / 5 | +1, +1 | same |
| | `both` | 5,705 / 687 / 4 | 5,707 / 689 / 4 | +2, +2 | same |
| | `viaLocal` | 5,282 / 593 / 4 | 5,283 / 594 / 4 | +1, +1 | same |
| | `looped` | 5,493 / 720 / 6 | 5,494 / 721 / 6 | +1, +1 | same |
| | graph | 197,917 / 5,950 / 6 | 197,923 / 5,956 / 6 | +6, +6 (1 + 1 + 2 + 1 + 1) | same |
| `o01_shifted_point_checks_through_exclusive_deref_storage` | `expect` (`shifted(..).x == 3i32`) | 8,913 / 1,139 / 6 | 8,914 / 1,140 / 6 | scratch reserve (+1, +1) | same |
| | graph | 190,759 / 5,183 / 6 | 190,760 / 5,184 / 6 | +1, +1 | same |
| `o02_remembered_slot_carries_the_stored_source_to_the_caller` | `selected` | 6,373 / 855 / 6 | 6,392 / 855 / 6 | `view.x`, two probes (+19) | same |
| | `expect` | 8,410 / 961 / 6 | 8,429 / 961 / 6 | +19 (the same `selected` read, in the expectation's call) | same |
| | graph | 190,814 / 5,004 / 6 | 190,833 / 5,004 / 6 | +19 | same |
| `o08_poisoned_slot_rejects_the_escape_of_a_callee_local` | `selected` | 6,429 / 883 / 6 | 6,448 / 883 / 6 | +19, as O2 | same |
| | `expect` | 8,494 / 1,003 / 6 | 8,513 / 1,003 / 6 | +19 | same |
| | graph | 192,713 / 5,271 / 6 | 192,732 / 5,271 / 6 | +19 | same |
| `o10_extracted_reference_checks_and_slot_reference_is_rejected`, source 0 | `extract` (`List<&User>` parameter) | 3,539 / 513 / 6 | 3,549 / 513 / 6 | insert probe (+10) | same |
| | graph | 188,037 / 4,612 / 6 | 188,047 / 4,612 / 6 | +10 | same |
| same, source 1 | `slot` | 3,059 / 267 / 5 | 3,069 / 267 / 5 | +10 | same |
| | graph | 187,315 / 4,363 / 5 | 187,325 / 4,363 / 5 | +10 | same |
| same, source 2 | `indexed` | 3,081 / 278 / 5 | 3,091 / 278 / 5 | +10 | same |
| | graph | 187,398 / 4,374 / 5 | 187,408 / 4,374 / 5 | +10 | same |
| `o10_lookup_siblings_keep_their_parent_caps` | `computed` | 2,009 / 93 / 2 | 2,019 / 93 / 2 | insert probe (+10) | same |
| | `grouped`, `head` | 2,003 / 89 / 2 | 2,013 / 89 / 2 | +10 | same |
| | `bound` | 2,002 / 89 / 1 | 2,012 / 89 / 1 | +10 | same |
| | `discarded` | 2,011 / 89 / 3 | 2,021 / 89 / 3 | +10 | same |
| | graph | 191,091 / 4,248 / 5 | 191,141 / 4,248 / 5 | +50 (five roots) | same |
| `o12::read_through_a_stored_view_of_a_temporary_rejects_its_release` | `viewField` | 2,962 / 348 / 7 | 2,971 / 348 / 7 | `view.x`, one probe (+9) | the same test (`old_root_caps`) |
| | graph | 188,579 / 4,666 / 7 | 188,588 / 4,666 / 7 | +9 | same |
| `record_contents::record_view_reads_check` | `viaParam`, `second`, `grouped` | 2,093 / 137 / 3 | 2,102 / 137 / 3 | +9 | `record_contents::record_roots_pre_charge_caps_now_give_resource_limit` |
| | `bound` | 2,206 / 193 / 3 | 2,225 / 193 / 3 | +19 | same |
| | `looped` | 2,153 / 170 / 5 | 2,162 / 170 / 5 | +9 | same |
| | `named` | 2,699 / 312 / 4 | 2,718 / 312 / 4 | +19 | same |
| | `late` | 3,240 / 460 / 4 | 3,259 / 460 / 4 | +19 | same |
| | graph | 193,340 / 5,239 / 5 | 193,433 / 5,239 / 5 | +93 (9 x 4 + 19 x 3) | same |
| `record_contents::unsupported_record_reads_keep_their_parent_caps`, `NAMED_READS` | `id` | 1,983 / 82 / 3 | 1,992 / 82 / 3 | +9 | same |
| | graph | 186,564 / 4,221 / 5 | 186,573 / 4,221 / 5 | +9 | same |
| `record_contents::stored_source_loans_reach_the_caller_view` | `grouped`, `other`, `groupedRead`, `looped`, `early`, `dead` | 7,066 / 1,199 / 7; 7,061 / 1,199 / 6 (both reads); 7,201 / 1,277 / 8; 4,196 / 862 / 8; 3,049 / 491 / 4 | 7,085; 7,080 (both); 7,220; 4,215; 3,068 (entries and depth unchanged) | +19 each | same |
| | `released` | 7,195 / 1,271 / 6 | 7,224 / 1,271 / 6 | +29 (three probes) | same |
| | graph | 215,791 / 9,533 / 8 | 215,934 / 9,533 / 8 | +143 (19 x 6 + 29) | same |
| `record_contents::empty_body_and_family_calls_with_record_contents`, `EMPTY_BODY` | `selected` | 6,269 / 845 / 6 | 6,288 / 845 / 6 | +19 | same |
| | `early` | 3,888 / 708 / 8 | 3,907 / 708 / 8 | +19 | same |
| | graph | 193,577 / 5,558 / 8 | 193,615 / 5,558 / 8 | +38 | same |
| same, `FAMILY` | `selected` (`Incomplete` at `remember(&mut view, &second)`) | 2,663 / 181 / 3 | 2,682 / 181 / 3 | +19 (the same `view.x` charge at two probes, which the stopped root also pays) | same |
| | graph | 189,426 / 4,282 / 5 | 189,445 / 4,282 / 5 | +19 | same |

Outcomes and blocker sites did not change in any row: every `Checked`, `Incomplete`, `SourceRejected` and blocker assertion of these tests passes unedited.

## Fixed caps left unchanged

`m5::legacy_caps` and `m5::graph::inventory` pass unedited (the M4 17-source baseline 372,246 / 54,059 / 12 is not touched), as do `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots` and `m4_8_callable_once`. `rfc_cases`, `retained`, `o05_field_arithmetic`, `field_routes`, `o15_arithmetic` and `records` pass.

## What this does not establish

The list of moved caps comes from running `--test m5 -- rfc_cases retained legacy_caps graph::inventory`, the neighbours above and a read of the other sources for `&Record` parameters; the full M5 binary runs only in hosted CI, which is the backstop for a root outside those filters.
