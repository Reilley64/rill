# Re-review ci-b4b2 fixes, part C13: member replacement through a reborrow of a sibling member rejects R0201 (2026-11-24, city bead ci-51qr)

Ticket `ri-px1.11.10` for spec `ri-2s0`. Findings D-F1, D-F2 and C-4 of the focused re-review `ci-b4b2` (`notes/m5-ownership-cleanup/review/re-review-2.md`). Authority: chief ruling 11 Q3 ("member replacement that creates the overlap REJECTS R0201 at the replacement, as construction does ... one state, one outcome") and chief ruling 18 (pin the reborrow form and keep a valid twin `Checked`). Base `d87e5dd2`, branch `m5-ownership-cleanup/r3f-ri-px1.11.10`.

## What changed

The rule: a member replacement whose incoming value holds a loan on another member of the same record rejects R0201 at the replacement. A reborrow `&mut *holder.x` or `&*holder.first` of a member of a record with an `&mut i32` member carries a `Suspension` loan on that member's slot (`Action::Reborrow` with `suspends`), and the loan travels with the value held directly, through a local (`let r = ...; holder.y = r;`) or through a call's result. The reborrow is a child of the sibling's loan, so the borrow check at its creation sees no overlap with the sibling; the store does. Construction rejects the same state (`Pair { x: &mut *r, y: &mut *r }`).

- `src/analysis/ownership/flow.rs`: `member_replacement_overlap`, called at `Action::Install` before the incoming value moves into the member. It acts only when the target is a member (`owner != target`) of an `exclusive_member` record, the same gate as the `Action::Displace` liveness arm of C2. It rejects (`Conflict::Access`, R0201) when a loan of the incoming value has a source in the record's member range other than the target. Site: the replacement statement. Related: the reborrow. The replaced member's own reborrow is not a sibling (`holder.y = &mut *holder.y` keeps its parent outcome R0203, from the `Displace` release, unchanged by this ticket).
- Metering: one `budget.step(loans.len())`, charged before the loans are read (rule 1: the loans of the staged value are the only fact that carries the reborrow, so no earlier screen exists).
- A record parameter's members hold no loans at entry (`h: Pair`, `reborrowParam`), but the reborrow itself carries the `Suspension` loan on `k.x`, so the check needs no new loan seeding.

## The six D-F1 outcomes (both load modes)

Every source was `Checked` at `d87e5dd2` (old numbers measured with the call to `member_replacement_overlap` disabled, same module); each is now R0201 at the replacement. Pinned as roots of `o07::REBORROW`, exact and one short on each axis.

| Root | Replacement | Old (d87e5dd2) | New outcome, related | New caps |
|---|---|---|---|---|
| `reborrowDirect` | `holder.y = &mut *holder.x;` | Checked 3,410 / 651 / 5 | R0201, `&mut *holder.x` | 3,230 / 577 / 5 |
| `reborrowThenWrite` | same, then `*holder.y += 1i32;` | Checked 3,958 / 914 / 5 | R0201, `&mut *holder.x` | 3,446 / 684 / 5 |
| `reborrowLocal` | `let r = &mut *holder.x; holder.y = r;` (site `holder.y = r;`) | Checked 3,542 / 710 / 5 | R0201, `&mut *holder.x` | 3,342 / 625 / 5 |
| `reborrowCall` | `holder.y = same(&mut *holder.x);` | Checked 5,982 / 1,036 / 6 | R0201, `&mut *holder.x` | 3,325 / 629 / 6 |
| `reborrowParam` | `function reborrowParam(h: Pair)`: `let mut k = h; k.y = &mut *k.x;` | Checked 3,590 / 642 / 4 | R0201, `&mut *k.x` | 3,392 / 545 / 4 |
| `reborrowShared` | `holder.second = &*holder.first;` (`Holder`) | Checked 3,410 / 651 / 5 | R0201, `&*holder.first` | 3,230 / 577 / 5 |

The module's typed totals are 355,194 / 52,171 / 12 and its graph is `SourceRejected` 215,061 / 8,614 / 6 (216,698 / 9,318 / 6 without the check). The new roots lower the cost, so no old cap is exceeded; these are new pins, not moved caps.

## D-F2 and the neighbours

- D-F2, pinned as `siblingReplacedAfter` (`holder.y = &mut *holder.x; holder.x = &mut c; *holder.x += 1i32; *holder.y += 1i32;`): R0201 at the first replacement, 3,898 / 908 / 5. It was R0203 at `holder.x = &mut c;` (4,018 / 974 / 5). It is no longer an over-rejection reached late: the first replacement is the overlap, so it is not conservative any more, it is the ruled outcome.
- The review's rejecting neighbours all contain the replacement now ruled R0201, so the replacement is the first rejection: `...; *holder.x += 1i32;` was R0201 at `*holder.x`, now R0201 at the replacement; `let seen = a; ...` was R0201 at `a`, now at the replacement; the loop form was R0201 at the second round's `&mut *holder.x`, now at the replacement; `...; return take(holder);` was R0203 at `holder`, now R0201 at the replacement. None of these was pinned before; the codes R0201 are unchanged except `take(holder)` (R0203 to R0201), which follows from the same one-state-one-outcome ruling. Construction `Pair { x: &mut *r, y: &mut *r }` is unchanged R0201 (not touched).
- `holder.y = &mut *holder.x` inside a record update (`Pair { ...holder, y: &mut *holder.x }`) stays `Incomplete` at the update, as before.

## Valid twins (ruling 18), pinned `Checked` in `o07::REBORROW`

Reborrows of a sibling member that are not stored into another member: `reborrowKept` (`let r = &mut *holder.x; return 0i32;`, 3,316 / 611 / 5), `reborrowRead` (`return *r;`, 3,331 / 613 / 5), `reborrowPassed` (`return peek(&mut *holder.x);`, a call that does not return it, 5,762 / 933 / 6) and `reborrowSharedKept` (`let r = &*holder.first; return *r;`, 3,332 / 613 / 5). Each is unchanged from `d87e5dd2` (the check is not reached). The chief's example `let r = &mut *holder.x; *r += 1i32;` is not available: a write through a local reborrow is `Incomplete` at `*r += 1i32;` (2,306 / 131 / 3 in a probe module), at `d87e5dd2` too, and is not pinned.

## C-4 pins and the record update (`o07_forms::MEMBER_REPLACEMENT_SIBLINGS`, checked by `member_replacement_pins_its_sequence_and_twins`)

A second form beside `MEMBER_REPLACEMENT`, so the six existing roots' module is unchanged and their moved caps are attributable (the numbers of a root depend on its module). The review's `d87e5dd2` numbers (3,606 / 760 / 5 and 3,848 / 897 / 6) were measured in C2's module; these are the numbers in this form's module.

| Root | Outcome | Caps | Sequence |
|---|---|---|---|
| `exclusiveOverlap` | Checked | 2,991 / 452 / 5 (was 2,990) | pinned, whole MIR |
| `blockLocalNoUse` | Checked | 3,233 / 589 / 6 (was 3,232) | pinned, whole MIR |
| `recordUpdateOverlap` (`let next = Pair { ...holder, y: &mut a };` over `x: &mut a`) | `Incomplete` at `Pair { ...holder, y: &mut a }`, not `Checked` | 2,733 / 217 / 3 (unchanged) | none |

Typed 330,429 / 48,732 / 12; graph `Incomplete` 192,946 / 5,221 / 6 (192,944 before).

## Caps moved (rule 11, one named charge: the sibling check)

The check costs one work unit for each store of a member of an `exclusive_member` record that reaches it (`staged.loans.len()` is 1). Entries and depth do not move. The old caps below are asserted to give `ResourceLimit` in `o07_forms::member_replacement_pre_sibling_check_caps_now_give_resource_limit`, through `crate::limit`.

| Case | Old (d87e5dd2) | New |
|---|---|---|
| `MEMBER_REPLACEMENT` whole graph | 204,371 / 7,771 / 6 | 204,376 / 7,771 / 6 (+5: `replaceMember`, `replaceNoUse`, `sameOwnerAgain`, `sharedReplace`, `localReferent` +1 each; depth set by `localReferent`, unchanged) |
| `replaceMember` | 3,834 / 884 / 5 | 3,835 / 884 / 5 |
| `replaceNoUse` | 3,283 / 602 / 5 | 3,284 / 602 / 5 |
| `sameOwnerAgain` | 3,691 / 805 / 5 | 3,692 / 805 / 5 |
| `sharedReplace` | 3,906 / 898 / 5 | 3,907 / 898 / 5 |
| `localReferent` | 3,491 / 735 / 6 | 3,492 / 735 / 6 |
| `MEMBER_REPLACEMENT_SIBLINGS`: `exclusiveOverlap`, `blockLocalNoUse`, graph | 2,990 / 452 / 5, 3,232 / 589 / 6, 192,944 / 5,221 / 6 | 2,991, 3,233, 192,946 (new roots; old numbers asserted `ResourceLimit` too) |

`overlapThenUse` (3,122 / 530 / 5) and `recordUpdateOverlap` do not reach the check and keep their caps. No other pin moved: the targeted runs of `rfc_cases`, `retained`, `legacy_caps`, `graph::inventory`, `flow` and the three `m4_8` binaries pass with no other edited expectation, and `m5::legacy_caps` and the M4 baselines are unchanged.

## Red first

With the call to `member_replacement_overlap` removed (flow.rs otherwise as committed), `o07::member_replacement_through_a_sibling_reborrow_rejects_r0201` fails (six roots `Checked`), `member_replacement_pre_sibling_check_caps_now_give_resource_limit` fails (the old caps still hold) and `member_replacement_pins_its_sequence_and_twins` fails on the pinned numbers. The C-4 and record-update pins record behaviour that does not change with the fix except by the one work unit. Log: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-51qr/red-fix-reverted.txt`.

## Not established

- `holder.y = &mut *holder.y` (a reborrow of the replaced member) and `holder.y = same(&mut *holder.y)` are R0203 at `d87e5dd2` and now; not touched.
- Tuples, nested records and custom disposers stay `Incomplete` before the replacement, as in `ri-px1.11.1.md`.
- A write through a local reborrow of a sibling (`*r += 1i32;`) is `Incomplete`, so a valid use of such a reborrow cannot be pinned `Checked`.
- Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-51qr/`.
