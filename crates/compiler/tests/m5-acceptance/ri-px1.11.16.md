# C19: a record whose member borrows its own storage is copied or moved while the loan is live; and 2C-F2 (2026-11-24, city bead ci-v4s6)

Ticket `ri-px1.11.16` for spec `ri-2s0`. Authority: chief rulings 21 and 22 (`notes/m5-ownership-cleanup/review/chief-rulings-21.md`, `chief-rulings-22.md`). Found by city bead `ci-sqig` (C17); its probes are in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-sqig/`. Base `2e13bc6e`, branch `m5-ownership-cleanup/r4f-ri-px1.11.16`. Logs and probe sources: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-v4s6/` (`probes/` holds the probe sources, outputs and the bisect script).

## 1. The wrong acceptance

`type Own2 = { v: i32; r: &i32 };`, `let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;`. At `2e13bc6e`, both load modes, `take2(h)`, `let g = h; return 0i32;` and `let g = h; return *g.r;` were `Checked`.

**Why they were accepted.** `Own2` has only an `i32` and a shared reference, so a whole-record use is a copy, not a move (`Local::moves` is false). A copy is `Action::Read`, which asks `conflict` for `Access::Observe`, and `Observe` blocks only exclusive loans. The loan `h.r` holds on `h.v` is shared, so nothing objected. A record that moves (a `string`, or an `&mut i32` member) takes `Access::Exclusive`, which sees `h.r`'s loan overlap the moved storage and rejects. I probed `Named2 = { s: string; n: i32; r: &i32 }` with `t.r = &t.n; return takeT2(t);`: R0203 at `t`, related `&t.n`, at `2e13bc6e`. So the gap is the copy path of records with only copyable members.

### (a) Where it came from

- **`origin/main` (M4) has no ownership analysis.** `7332ca88` and the merge-base `216e4a8c` have no `crates/compiler/src/analysis/ownership*` and `git grep` finds neither `lower_body` nor `R0203` under `crates` there. Main's typed analysis checks the program and makes no ownership judgment, so main does not accept or reject it as an ownership error: the program is rejected only where the M5 analysis exists. I did not run a probe on main (nothing to run).
- **The gap was admitted on the M5 branch, by `1e609933`, "fix(compiler): end a sibling view before its backing field (#68)", 2026-11-20, a pre-cleanup commit (GitHub #68, no `ri-` ticket).** It added `can_end_record_in_reverse_order` and let `cleanup_boundary` schedule a record whose reference member borrows an earlier member of the same record. Before it, `h.r = &h.v;` could not end its scope and the body was `Incomplete`; after it the body is `Checked`.
- How I measured it: a first-parent bisect of `216e4a8c..7002855e` (468 commits) on a throwaway worktree, with a probe of two functions: `h.r = &h.v; return 0i32;` and the same followed by `return take2(h);`. `470a7299` is `Incomplete` for both (blocked); `1e609933`, its child, is `Checked` for both. The bisect assumes one transition along the first-parent chain; I confirmed both commits by name, not every commit between. Script and logs: `probes/bisect-step.sh`, `probes/bsearch.py`, `probes/bis.*.log`.
- Probed at the revisions the brief names (the forms of `probes/a1.txt`): `81d317b3` and `7002855e` give `Checked` for `take2(h)`, `let g = h` and `let g = h; return *g.r;`, with the same caps as `2e13bc6e` (5,849 / 744 / 5, 3,414 / 551 / 5, 3,445 / 557 / 5). `return h;` is R0202 at `return h;` at both. Output: `probes/a1.81d317b3.out`, `probes/a1.7002855e.out`.
- Other self-borrowing shapes (`probes/a5.txt`, measured at `2e13bc6e` and at the fix, identical except where stated): a reference member borrowing through a nested record (`h.r = &h.n.w`) or a list element (`h.r = &h.l[0u64]`), and an `&mut` member borrowing through a nested record, are `Incomplete` at `h` before any move. A tuple member (`t: [i32, i32]`) is typed `Incomplete` (no receipt). So those shapes share no gap. `&mut` direct (`OwnM`, `Own` with `h.m = &mut h.v`) was R0201 at the replacement at `2e13bc6e`; see 2C-F2 below for what it is now.

### (b) The rule

A copy of a whole record rejects when one of its reference members holds a local-storage loan whose source lies in the record's own storage. The code and the site are the existing convention for moving a borrowed local: **R0203 (`Conflict::Release`) at the moved operand, related the borrow.** The matching pinned case is `direct_owned_nested_record_leaf_loans_respect_overlap_and_last_use` in `crates/compiler/tests/m5/records.rs`, rows `sharedWholeMove` and `exclusiveWholeMove` (`let taken = saved;` is R0203 at `saved`). A record that moves already took this path through `conflict`; the new function covers the copy.

Code: `record_borrows_itself` in `src/analysis/ownership/flow.rs`, called from the `Action::Read` / `Action::Move` arm. It is at the move itself, not a per-shape test: every path above that reaches a `Checked` or rejecting outcome (call argument, `let`, branch, reassignment, loop) does so through `Action::Read`; I did not audit the other `Action` variants for a whole-record use.

- It runs only for a whole record (`slots[from].members != 0`) that does not move, and only on reference members (a free fact of the slot).
- It does not run when the copy is the returned value (the next instruction is `Export` of the copy's destination). That return already rejects R0202 at `return h;` (the escape), and the brief and chief keep that outcome. Checked: `moveReturn` is R0202 at `return h;` with unchanged caps 3,505 / 606 / 5.
- The charge: `budget.step(loans held by a reference member)`, once per such member. It is the only new work.

### Move paths (chief ruling 22 (2)), both load modes

| Path | Source | Outcome at the fix |
|---|---|---|
| call argument | `return take2(h);` (`moveCall`) | R0203 at `h`, related `&h.v` |
| `let` binding | `let g = h; return 0i32;` (`moveLet`), `... return *g.r;` (`moveLetRead`) | R0203 at `h`, related `&h.v` |
| branch | `if (true) { let g = h; }` (`moveBranch`) | R0203 at `h` |
| reassignment | `g = h;` (`moveAssign`) | R0203 at `h` |
| loop | `while (again) { let g = h; ... }`, `... take2(h) ...` (`moveLoop`, `moveLoopCall`) | R0203 at `h` |
| return | `return h;` (`moveReturn`) | R0202 at `return h;`, unchanged |
| struct-literal field | `Wrap { o: h }` as a `let` and as a return (`moveStructField`, `moveStructReturn`) | `Incomplete` at `w` / `Wrap { o: h }` |
| member store | `w.o = h;` (`moveMemberStore`) | `Incomplete` at `w` |
| record update | `Own2 { ...h, v: 2i32 }` (`moveRecordUpdate`) | `Incomplete` at the update |
| tuple element | typed `[Own2, i32]` as a `let` and as a return (`moveTuple`, `moveTupleReturn`) | `Incomplete` at `t` / `[h, 1i32]` |
| list element | `List<Own2>` as a `let` and as a return (`moveList`, `moveListReturn`) | `Incomplete` at `l` / `[h]` |
| return through a wrapper | `Option.Some(h)`, `Result.Ok(h)` (`moveWrapperReturn`, `moveResultReturn`); `let o = Option.Some(h);` (`moveWrapperLet`) | `Incomplete` at the call / `o` |
| `match` scrutinee | `match (h)`, `match (take2(h))`, `match (Option.Some(h))` (`moveMatch`, `moveMatchCall`, `moveMatchWrapper`) | `Incomplete` at the `match` |
| arrow | `() => h.v` called (`moveArrowCall`); `() => h` as a `let` | `Incomplete` at `f()`; typed `Incomplete`, no receipt (`pending Containment @ () => h`) |

The `Incomplete` rows are not acceptances: each stops before the move can be judged, and each has the same blocker and caps at `2e13bc6e` (measured, `probes/m3.2e13bc6e.out` against `probes/m3.out`; the same module). They are pinned so that a later change cannot admit them silently, and they are the paths where the fix has no test that can reject; if one of them is admitted later, it reaches `Action::Read` and the rule applies.

### (c) Pins

`rfc_cases/o07.rs`, each through `check` (both modes, targeted query and ordinary graph root, exact and one short on each axis for every root and the whole graph):

| Test | Pins |
|---|---|
| `a_whole_record_use_with_a_self_borrowing_member_rejects_r0203` (`SELF_BORROW_COPY`) | the seven rejecting forms above; `moveReturn` R0202; the write twin `moveWrite` (R0203 at `h.v = 2i32;`); `take2` Checked. Typed 348,956 / 51,350 / 12, graph `SourceRejected` 213,177 / 8,919 / 7. |
| `copying_a_record_that_borrows_nothing_of_itself_stays_checked` (`SELF_BORROW_TWINS`) | **the valid twin** `deadLoan`: `let k = &h.v; return take2(h);`, `Checked` 5,734 / 685 / 5; and `plainCopy`, `plainLet` (a copy with no self-borrow). |
| `other_whole_value_paths_of_a_self_borrowing_record_stay_incomplete` (`SELF_BORROW_UNREACHED`) | the fifteen `Incomplete` roots, with blockers and caps. |
| `an_arrow_capturing_a_self_borrowing_record_stays_typed_incomplete` | the arrow capture, typed `Incomplete`. |
| `the_member_replacement_check_covers_only_sibling_reference_members` (`SELF_BORROW_MEMBERS`) | 2C-F2, below. |

Caps (`SELF_BORROW_COPY`, old = `2e13bc6e` with the same module, new = this change):

| Root | Old | New |
|---|---|---|
| `moveCall` | Checked 6,465 / 1,052 / 5 | R0203 3,519 / 615 / 5 |
| `moveLet` | Checked 3,722 / 705 / 5 | R0203 3,509 / 613 / 5 |
| `moveLetRead` | Checked 3,749 / 709 / 5 | R0203 3,532 / 617 / 5 |
| `moveBranch` | Checked 3,883 / 795 / 5 | R0203 3,628 / 683 / 5 |
| `moveAssign` | Checked 4,612 / 1,028 / 5 | R0203 4,223 / 855 / 5 |
| `moveLoop` | Checked 4,416 / 1,074 / 5 | R0203 3,959 / 867 / 5 |
| `moveLoopCall` | Checked 7,312 / 1,506 / 7 | R0203 4,044 / 909 / 7 |
| graph | `SourceRejected` 215,532 / 9,926 / 7 | `SourceRejected` 213,177 / 8,919 / 7 |

The rejection is cheaper than the acceptance was, so the old caps still hold and now reject R0203, not `ResourceLimit`. `self_borrowing_record_forms_keep_their_old_caps_and_now_reject` asserts that for the seven roots and the graph (`Parent::Rejects`). `take2`, `moveReturn` and `moveWrite` did not change.

## 2. 2C-F2 (ruling 21 (2))

One condition in `member_replacement_overlap`: a loan counts only when `slots[loan.source].reference_member` is true. Results, both modes (`SELF_BORROW_MEMBERS`, typed 353,793 / 51,619 / 12, graph `SourceRejected` 213,613 / 8,865 / 5):

| Root | Source | `2e13bc6e` | Now |
|---|---|---|---|
| `replaceOwn` | `Own = { v; r: &i32; m: &mut i32 }`, `h.r = &h.v;` | R0201 at the replacement, 3,212 / 575 / 5 | **Checked** 3,411 / 658 / 5 |
| `replaceOwn2` | `Own2`, `h.r = &h.v;` | Checked 3,147 / 541 / 5 | Checked, unchanged |
| `replaceOwnM` | `OwnM = { v; m: &mut i32 }`, `h.m = &mut h.v;` | R0201 at the replacement, 3,014 / 480 / 5 | **Incomplete** 3,107 / 527 / 5, blocked at the function body |

The brief names the outcomes `Checked`, `Checked`, `Incomplete` for `d87e5dd2`. I did not run `d87e5dd2`; the three outcomes above match what the brief names.

**Rejecting twins**, while `h.r` is live: all reject, for `Own` and `Own2`.

| Twin | `Own` | `Own2` |
|---|---|---|
| (i) write `h.v = 2i32; let n = *h.r;` | R0203 at `h.v = 2i32;`, related `&h.v` (`ownWrite`) | same (`moveWrite`) |
| (ii) `return h;` | R0203 at `h`, related `&h.v` (`ownReturn`) | R0202 at `return h;`, related `&h.v` (`moveReturn`) |
| (ii) `return take(h);` | R0203 at `h` (`ownTake`) | R0203 at `h` (`moveCall`) |
| (ii) `let g = h; return *g.r;` | R0203 at `h` (`ownLet`) | R0203 at `h` (`moveLetRead`) |

`Own` moves (it has an `&mut` member), so its `return h;` takes the move's R0203; `Own2` copies, so its return keeps the escape's R0202. Neither was Checked. Not asked, also pinned: the `&mut` self-borrow followed by a move, `ownMTake` (`OwnM`, `h.m = &mut h.v; return takeOwnM(h);`) and `ownMutMember` (`Own`, `h.m = &mut h.v; return takeOwn(h);`), R0203 at `h`, related `&mut h.v`; they were R0201 at the replacement at `2e13bc6e`. `replaceOwnM` itself, with no move after, is `Incomplete`.

Old caps: `self_borrow_member_parent_caps_now_give_resource_limit` asserts `ResourceLimit` for the old caps of eight roots (`replaceOwn`, `replaceOwnM`, `ownWrite`, `ownReturn`, `ownTake`, `ownLet`, `ownMTake`, `ownMutMember`) and the old graph cap 213,012 / 8,571 / 5: each outcome moved from an early R0201 to a later one, which costs more.

## 3. Caps moved (chief ruling 22 (4), metering rule 11, third case)

The one mandated charge: `record_borrows_itself` charges the number of loans each reference member of a copied whole record holds (work only; entries and depth do not move). It makes a `Checked` copy of such a record cost one more per held loan. Nothing else in the targeted suites moved: with the change, `rfc_cases`, `retained`, `legacy_caps`, `graph::inventory`, `flow`, `records::`, `patterns::`, `field_routes::` and the three `m4_8` binaries pass with no edited expectation, and `m5::legacy_caps` and the M4 baselines are unchanged.

New pins for `SELF_BORROW_TWINS` (old = `2e13bc6e`, same module):

| Case | Old | New |
|---|---|---|
| `deadLoan` | 5,733 / 685 / 5 | 5,734 / 685 / 5 |
| `plainCopy` | 5,607 / 627 / 5 | 5,608 / 627 / 5 |
| `plainLet` | 3,209 / 443 / 5 | 3,210 / 443 / 5 |
| graph | 193,543 / 5,512 / 5 | 193,546 / 5,512 / 5 |

`self_borrow_twin_parent_caps_now_give_resource_limit` asserts `ResourceLimit` for the old caps of all three roots and the graph, through `crate::limit`. The charge is not screened by a cheaper earlier fact: whether a reference member holds a loan is only in the state.

## Red first

With `git apply -R` of the two source changes, the new tests failed: `a_whole_record_use_with_a_self_borrowing_member_rejects_r0203`, `copying_a_record_that_borrows_nothing_of_itself_stays_checked`, `self_borrowing_record_forms_keep_their_old_caps_and_now_reject`, `self_borrow_twin_parent_caps_now_give_resource_limit`, `the_member_replacement_check_covers_only_sibling_reference_members` and `self_borrow_member_parent_caps_now_give_resource_limit`. `other_whole_value_paths_of_a_self_borrowing_record_stay_incomplete` and the arrow test passed without the fix: they pin behaviour that does not change. The child logs show the failing tests ran (`1 failed`, 0.09 s). Log: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-v4s6/red-fix-reverted.txt`.

## Not established

- **A live shared loan from another local.** `let k = &h.v; let g = h; return *k;` (and the `take2(h)` form with `k` used afterwards) is `Checked` before and after for `Own2`, because the copy is an observation and `k`'s shared loan does not conflict with it. I measured it (`probes/a1.out`, `local_loan_let`, `local_loan_call`: 3,335 / 504 / 5 and 5,880 / 757 / 5 at the fix, one more than at `2e13bc6e`) and did not pin it. The brief's wording ("a loan on one of its members is live") and its remark that `k` is dead in the valid twin could be read to want this rejected too. For a record that moves it already is (`mut_move_let`, `text_move`: R0203 at `h`, related `&h.v`). Rejecting it for a copy would reject every read of a copyable value while it is shared-borrowed, which is wider than the self-borrow; it is a ruling for the chief. I did not widen the rule.
- A self-borrow stored through a nested record, tuple or list stays `Incomplete` before the move, so none of the move paths could be tested against it.
- I did not run `d87e5dd2`, `216e4a8c` or `main` probes (the bisect tested `7002855e`'s ancestors only through the two-function probe named above).
- Hosted CI, the full M5 binary and `mise run check` were not run, by policy.
- Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-v4s6/` (`targeted-final.txt`, `o07-green.txt`, `targeted-records-patterns-field_routes.txt`, `m4_8_*.txt`, `fmt.txt`, `clippy.txt`, `red-fix-reverted.txt`, `probes/`).
