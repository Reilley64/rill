# C17b: the member-replacement check's over-rejection, pinned as a stated limit (2026-11-24, city bead ci-oqw9)

Ticket `ri-px1.11.14` for spec `ri-2s0`, items 1, 3 and 4 of C17 (item 2 went to `ri-px1.11.16`). Findings 2C-F1, 2C-F3 and 2C-F4 of the second focused re-review `ci-9qga` (`notes/m5-ownership-cleanup/review/re-review-3.md`). Authority: chief ruling 21 (3). Base `2e13bc6e`, branch `m5-ownership-cleanup/r4f-ri-px1.11.14`. There is no production change and no cap moved.

## The stated limit (2C-F1)

`member_replacement_overlap` rejects R0201 when a stored value holds a loan on a sibling member of the record being replaced. A value read from a member of another record carries the loans of that whole record (`Action::Reborrow` builds them through `carried_loan_root`). So when `other` also holds a reborrow of `holder.x`, the store `holder.y = other.x` rejects even though `holder.x` and `holder.y` point at different places afterwards. The check over-rejects: it fails closed, and each source below is sound.

Narrowing it needs a loan set that follows one member of a record, not the whole record. Ruling 21 (3) excludes that per-member tracking from M5, and the previous attempt (city bead ci-sqig) judged from reading `flow.rs`, without prototyping, that nothing narrower exists. The six stores from the review's 2C-F1 table are pinned at their current outcome as a conservative limit. The follow-up is `ri-qlh.12` (needs-triage, under `ri-qlh`, relates-to `ri-2s0`). When it lands, these pins go to `Checked`.

`o07::member_replacement_pins_an_over_rejection_of_sound_stores_from_another_record` (`OVER_REJECTION`), both load modes, typed `Checked` 346,577 / 51,865 / 12, graph `SourceRejected` 212,632 / 9,075 / 5. Every root is R0201 at the store, pinned exact and one short on each axis by `check`:

| Root | Store (site) | Related | Caps (work / entries / depth) |
|---|---|---|---|
| `storeFromBuilt` | `holder.y = other.x;` | `&mut *holder.x` | 3,990 / 820 / 5 |
| `storeFromBuiltThenWrite` | `holder.y = other.x;` then `*holder.y += 1i32;` | `&mut *holder.x` | 4,252 / 953 / 5 |
| `storeFromSetMember` | `holder.y = other.x;` after `other.y = &mut *holder.x;` | `&mut *holder.x` | 4,443 / 1,037 / 5 |
| `storeReborrowOfRead` | `holder.y = &mut *other.x;` | `&mut *holder.x` | 4,430 / 1,039 / 5 |
| `storeReborrowViaLocal` | `holder.y = r;` after `let r = &mut *other.x;` | `&mut *holder.x` | 4,605 / 1,115 / 5 |
| `storeSharedFromShared` | `h.second = k;` (a `Holder`, no `holder`) | `&*h.first` | 4,596 / 1,115 / 5 |

The outcomes, sites and related sites match the review's table.

## Call results (2C-F3)

`o07::member_replacement_pins_a_call_result_that_carries_every_argument_loan` (`CALL_RESULT_CARRIES_EVERY_LOAN`), typed `Checked` 326,878 / 48,110 / 12, graph `SourceRejected` 189,249 / 4,707 / 6. With `function pickFirst(a: &mut i32, b: &mut i32): &mut i32 { return a; }`, `storePickedFirst` (`holder.y = pickFirst(&mut c, &mut *holder.x);`) is R0201 at the store, related `&mut *holder.x`, 3,105 / 519 / 6; `pickFirst` is `Checked`, 2,080 / 128 / 3. The result points at `c`, but a call result carries every argument's loan (RFC 0007:28), so the result carries the reborrow of `holder.x`. It is conservative, and it is the same limit as above: the call does not say which argument the result keeps.

## The record (2C-F4)

`ri-px1.11.10.md` now carries a dated correction: the charge is `budget.step(staged.loans.len())`, so +1 per loan carried by the stored value, and the pinned roots there carry one. This ticket did not re-measure the review's +3 and +0 examples.

## Red first

The tests pin current behaviour, so the red check disables the fix under test: with the call to `member_replacement_overlap` commented out in `flow.rs` (restored afterwards; the diff of this branch has no `flow.rs` change), both new tests fail, as do the four C13 tests (`member_replacement_through_a_sibling_reborrow_rejects_r0201`, `a_record_holding_a_sibling_reborrow_rejects_at_the_replacement_before_it_is_taken`, `o07_forms::member_replacement_pins_its_sequence_and_twins`, `o07_forms::member_replacement_pre_sibling_check_caps_now_give_resource_limit`). The run was 14 passed, 6 failed. With the call disabled, all seven new roots lower `Checked` in both load modes (probe, `probe-check-disabled-sources.out` and `probe-check-disabled-pick.out`), so the pinned R0201 is the check's. That says the check is what rejects them; it does not say the sources were `Checked` at `d87e5dd2`, which the review states and this ticket did not rerun.

## Not established

- That the six sources and `storePickedFirst` are sound beyond the review's reading and the shape of each source (after each store, `holder.x` points at `a` and `holder.y` at `c`).
- That no narrower fix exists. The member-only loan set was judged from reading `flow.rs` (ci-sqig) and not prototyped here.
- Other sources the check over-rejects: the review's probes found these six and two `pickFirst` forms, and this ticket swept no more. The second `pickFirst` form (a three-`&mut` record) is not pinned.
- Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-oqw9/`.
