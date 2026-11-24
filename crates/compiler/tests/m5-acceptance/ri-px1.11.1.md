# Review ci-0mdr fixes, part C2: member replacement rejects R0201 (2026-11-24, city bead ci-gtmo)

Part of the single fix pass of review `ci-0mdr`, split into children of `ri-px1.11` (see `ri-px1.11-c1.md` for the list). C2 is this file (`ri-px1.11.1`: Q3, SP-3, SP-4). Base `da5bec50` (the integration branch with C1 merged), branch `m5-ownership-cleanup/r2f-ri-px1.11.1`. The records child consolidates the per-child files into the section "Review ci-0mdr fixes" of `ri-px1.11.md`.

## What changed

Chief ruling Q3 (`chief-rulings-11.md`), a mandated flip under rule 11's third case: a member replacement that creates an overlap (`holder.y = &mut a;` while `holder.x` borrows `a`) is `SourceRejected` R0201 at the replacement with no later use, as construction is.

- `src/analysis/ownership/flow.rs`, `liveness`: at `Action::Displace` of a member of a record with an `&mut i32` member (`exclusive_member`), the record's other members are marked live at the store, as `Action::Init` already marks all members of such a record live. The loans of the other members then reach the borrow that stages the new loan, and the existing conflict check rejects at `&mut a` (related: the loan of `a` in the construction). The replaced member itself is left out: the store releases its old loan, so `holder.y = &mut b;` over a loan of `b` does not overlap itself (`sameOwnerAgain`). A whole-record replacement, a scalar and a record without an `&mut i32` member are not touched (the arm needs `slots[target].owner != target` and `exclusive_member`). No `step`, `reserve` or `charge_key` was added: the new arm reads two facts the build already holds, and the extra work in the numbers below is the other members staying live.
- Rule 1 (the cost is not avoidable by a screen): the work is the liveness of the other members, which is what "the store uses the record's other members" means. No earlier screen can know that the replacement overlaps without those loans.

Tests (all run both installed modes, exact and one short on each axis through `crate::limit`):

- `rfc_cases::o07::member_replacement_overlap_without_later_use` (SP-4, Q3): `overlapNoUse` (`holder.y = &mut a;`, no later use), `replacesFirst` (`holder.x = &mut b;`), `sharedMember` (`holder.second = &a;` over `first: &mut a`), `sharedFirst` (`holder.first = &a;` over `second: &mut a`), `looped` (the replacement in a `while`): each R0201, with site and related as in the table of the test.
- `rfc_cases::o07_forms::member_replacement_pins_its_sequence_and_twins` (SP-3, Q3): `Checked` roots with the whole sequence: `replaceMember` (`holder.y = &mut c; *holder.x += 1i32;`), `replaceNoUse` (the valid twin of `overlapNoUse`: `holder.y = &mut c;`, no later use), `sameOwnerAgain`, `sharedReplace`. Rejecting twins: `overlapThenUse` (R0201 at `&mut a` for `holder.y = &mut a; *holder.x += 1i32;`), `localReferent` (R0203 at the block that ends `c` while `holder.y` is read later, related `&mut c`). The sequence helper learned `Operation::Install` (printed `replace <member> <- <incoming>`).
- `rfc_cases::o07::construction_twins_the_review_found_unpinned` (SP-3 twins the review found correct but unpinned): `movedTwice` R0200 at `h` (related `h`), `memberAndReferent` R0201 at `&mut a` (related `&a`), `dupParam` R0200 at `p` (related `p`), and `refFirst` `Checked`. These needed no source change: they passed at the parent (their caps below are unchanged).
- `rfc_cases::o07::member_replacement_overlap_gives_r0201_at_its_parent_caps` and `rfc_cases::o07_forms::member_replacement_parent_caps_now_give_resource_limit`: the parent caps below, replayed on the same sources.

Red check: with `flow.rs` restored to `da5bec50`, the four member-replacement tests above fail (the replacement is `Checked`). With only the replaced-member exclusion removed, `member_replacement_pins_its_sequence_and_twins` fails on the whole-graph cap (203,877 / 7,511 / 6 against 204,371 / 7,771 / 6), because `sameOwnerAgain` rejects.

## Findings

| Finding | Status | Where |
|---|---|---|
| Q3 | fixed | `flow.rs` liveness arm; `o07::member_replacement_overlap_without_later_use` |
| SP-4 | fixed (outcome chosen by Q3: reject) | `o07::member_replacement_overlap_without_later_use`, valid twin `o07_forms::member_replacement_pins_its_sequence_and_twins` (`replaceNoUse`) |
| SP-3 | fixed | `o07_forms::member_replacement_pins_its_sequence_and_twins` (Checked sequences, R0201 and R0203 twins) |
| Q3 conditions: record moved twice, member and referent in one call, `&mut` parameter duplicated | fixed (pinned, no source change) | `o07::construction_twins_the_review_found_unpinned` |

`m5-status.md` O7 row now says replacement rejects as construction does; its "Not established" list is unchanged.

## Caps

Every pinned cap in the diff is a new root's. Measured at the parent `da5bec50` in a `git worktree add` with its own target directory (the parent runs the same sources; `o07_forms.rs` there carried a throwaway probe), then on this branch. No pre-existing pin moved: the targeted runs of `rfc_cases`, `retained`, `legacy_caps`, `graph::inventory` and the three `m4_8` binaries pass without an edited expectation.

`REPLACEMENT` (`o07.rs`), the flip: the cost falls, so the old caps still hold and give the new outcome R0201 (`member_replacement_overlap_gives_r0201_at_its_parent_caps`). The reason for the lower cost: the body stops at the replacement instead of lowering the rest.

| Root | Parent outcome and caps | New outcome and caps |
|---|---|---|
| `overlapNoUse` | Checked 3,061 / 491 / 5 | R0201 2,828 / 385 / 5 |
| `replacesFirst` | Checked 3,056 / 490 / 5 | R0201 2,823 / 384 / 5 |
| `sharedMember` | Checked 3,061 / 491 / 5 | R0201 2,828 / 385 / 5 |
| `sharedFirst` | Checked 3,056 / 490 / 5 | R0201 2,823 / 384 / 5 |
| `looped` | Checked 3,679 / 789 / 6 | R0201 3,073 / 514 / 6 |
| whole graph | Incomplete 198,404 / 6,418 / 6 | SourceRejected 196,876 / 5,724 / 6 |

`MEMBER_REPLACEMENT` (`o07_forms.rs`): the other members stay live, so the cost rises and the parent caps now give `ResourceLimit` (`member_replacement_parent_caps_now_give_resource_limit`).

| Root | Parent caps (outcome) | New caps (outcome) |
|---|---|---|
| `replaceMember` | 3,832 / 884 / 5 (Checked) | 3,834 / 884 / 5 (Checked) |
| `replaceNoUse` | 3,271 / 600 / 5 (Checked) | 3,283 / 602 / 5 (Checked) |
| `sameOwnerAgain` | 3,689 / 805 / 5 (Checked) | 3,691 / 805 / 5 (Checked) |
| `sharedReplace` | 3,904 / 898 / 5 (Checked) | 3,906 / 898 / 5 (Checked) |
| `overlapThenUse` | 3,120 / 530 / 5 (R0201) | 3,122 / 530 / 5 (R0201) |
| `localReferent` | 3,489 / 735 / 6 (R0203) | 3,491 / 735 / 6 (R0203) |
| whole graph | 204,349 / 7,769 / 6 (SourceRejected) | 204,371 / 7,771 / 6 (SourceRejected) |

Also measured, in other forms probed with the same change (not pinned): `exclusiveOverlap` (`holder.first = &mut a;` over its own loan of `a`, with `second: &b`) 3,594 / 758 / 5 to 3,606 / 760 / 5, still `Checked`; `blockLocalNoUse` (the `localReferent` block with no later use) 3,819 / 889 / 6 to 3,848 / 897 / 6, still `Checked`.

`TWINS` (`o07.rs`): typed 330,147 / 48,293 / 12 and graph 191,771 / 4,990 / 7 at the parent and now; each root unchanged (`refFirst` 2,430 / 190 / 3, `movedTwice` 3,344 / 409 / 5, `memberAndReferent` 2,498 / 223 / 7, `dupParam` 2,461 / 205 / 4).

## Probe note (chief ruling item (b))

The re-review must re-run probe 2 on the replacement forms, in both load modes, for a wrong acceptance or a wrong rejection. The forms this change touches: overlap (`overlapNoUse`, `overlapThenUse`), swap of the first member (`replacesFirst`), a shared member over an exclusive one and the reverse (`sharedMember`, `sharedFirst`), the loop form (`looped`), a block-local referent (`localReferent`), replacing a member with a loan of the owner it already borrows (`sameOwnerAgain`) and replacing the exclusive member with a loan of its own owner while the shared member borrows another (`exclusiveOverlap`: `Checked`, judged sound because the store releases the old loan). `blockLocalNoUse` stays `Checked` (a referent released while the member is never read again); it is judged sound by liveness, as SP-4 was, and is not pinned. Probe 2 itself was not re-run as the review ran it: these are the forms found by hand on this branch (about 20 functions, targeted query only; graph roots and `test_mode` only through the pinned tests).

## Not established

- A record with a custom disposer, a nested record member and a tuple stay `Incomplete` before the replacement is reached; the change does not touch them.
- Record update with `..base` forms (the second `Displace` site in `build.rs`) were not probed.
- Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-gtmo/`.
