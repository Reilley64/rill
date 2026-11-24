# C19b: copy versus move of a record that borrows its own storage, pinned at the base; and 2C-F2 (2026-11-24, city bead ci-xwi2)

**Superseded by C19b. C19's copy rejection was dropped after probe ci-0xlg, ruling 25.** This file replaces the C19 record (city bead `ci-v4s6`, branch `m5-ownership-cleanup/r4f-ri-px1.11.16`, feature commit `7b3020eb`, never merged). Ticket `ri-px1.11.16`, spec `ri-2s0`. Authority: chief of staff ruling 25 (`notes/m5-ownership-cleanup/review/chief-rulings-25.md`), on the probe `ci-0xlg` (`notes/m5-ownership-cleanup/review/probe-c19-copy.md`; sources and results in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-0xlg/`). Base `b082e6ac`, branch `m5-ownership-cleanup/r4f-ri-px1.11.16b`. Logs, probe sources, measuring harness and outputs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-xwi2/`.

## 1. The withdrawn finding

C19 reported a wrong acceptance: `type Own2 = { v: i32; r: &i32 }` with `h.r = &h.v;` was `Checked` for `take2(h)`, `let g = h;` and `let g = h; return *g.r;`, so a record whose member borrows its own storage "could be moved while the loan is live". C19 added `record_borrows_itself`, which rejects every whole-record **copy** of such a record with R0203.

**The finding is withdrawn.** It was not a wrong acceptance:

- `Own2` (an `i32` and a shared `&i32`) is implicitly copyable: RFC 0007 line 107, "Use the existing implicit-copy classification for scalars, read-only views and eligible aggregates. Tuples and records require eligible contained values." The `string` of `OwnS` is not: line 109, "Strings, owned collections, counted handles and exclusive references gain no implicit-copy exception." So `let g = h;` and `take2(h)` are copies of `Own2` (MIR `Copy`), and moves of `OwnS` (MIR `Move`).
- A copy keeps its dependency: line 113, "Copying a read-only descriptor preserves its source dependencies." and line 190, "Copying its contained read-only reference produces the stored reference's dependencies. It does not produce a reference to the slot merely because the copy read it." The copy `g` of `h` therefore still depends on `h.v`, and the base (`b082e6ac`) already rejects every write or escape through it, in both load modes: P1, P2, P6 (R0203), P3, P15 (R0202), P18 (R0203).
- C19's rule rejected five **valid** programs (P7, P8, P9, P10, P17: copies that are only read, one with the copy dead before the write) and added no true rejection. That is why it was dropped.

The one real wrong acceptance of the review loop was the withheld inventory (C8, fixed in `33eaab59`), not this.

**Origin.** `1e609933` (#68, "fix(compiler): end a sibling view before its backing field", 2026-11-20) made `h.r = &h.v;` analysable: before it the body was `Incomplete`. It is not the origin of a bug. C19's bisect to it (C19 evidence, section 1 (a)) found where the program became analysable, not where a wrong acceptance began.

## 2. What this ticket changes

**One source change**, 2C-F2 (ruling 21 (2)): `member_replacement_overlap` in `src/analysis/ownership/flow.rs` counts a loan only when it goes through a sibling **reference** member (`slots[loan.source].reference_member`). `record_borrows_itself` is not brought over. Nothing else in `src/` changes.

Tests: `crates/compiler/tests/m5/rfc_cases/o07_self_borrow.rs` (new; registered in `rfc_cases.rs`). It reuses `Form` and `check` of `o07_forms.rs` (made `pub(super)`: no other change there). Every source is loaded in both installed modes; each root is pinned through its targeted query and its ordinary graph root, exact and one short on each axis for every root and the whole graph. Each `Checked` root pins its whole MIR sequence, which asserts `copy h` (Own2) or `move h` (OwnS).

## 3. Pins at base outcomes (probe ci-0xlg, column B)

Every outcome equals the probe's B column (code, site, related). Both load modes identical. Caps are work / entries / depth of the targeted query.

| Probe | Root | Outcome | Caps | Test |
|---|---|---|---|---|
| P1 | `p1WriteAfterCopy` | Rejected R0203 at `h.v = 3i32;`, related `&h.v` | 3,584 / 647 / 5 | `a_copy_keeps_its_dependency_on_the_original_so_a_write_or_scope_end_rejects_r0203` |
| P2 | `p2ReplaceAfterCopy` | Rejected R0203 at `h = Own2 { v: 9i32, r: &e };`, related `&h.v` | 4,202 / 837 / 5 | `a_copy_keeps_its_dependency_on_the_original_so_a_write_or_scope_end_rejects_r0203` |
| P6 | `p6WriteAfterCallCopy` | Rejected R0203 at `h.v = 3i32;`, related `&h.v` | 3,668 / 698 / 5 | `a_copy_keeps_its_dependency_on_the_original_so_a_write_or_scope_end_rejects_r0203` |
| P18 | `p18InnerScopeDies` | Rejected R0203 at `{ let mut h2 = Own2 { v: 1i32, r: &e }; h2.r = &h2.v; g = h2; }`, related `&h2.v` | 5,332 / 1,324 / 6 | `a_copy_keeps_its_dependency_on_the_original_so_a_write_or_scope_end_rejects_r0203` |
| twin (i) | `writeWhileMemberLive` | Rejected R0203 at `h.v = 2i32;`, related `&h.v` | 3,097 / 521 / 5 | `a_copy_keeps_its_dependency_on_the_original_so_a_write_or_scope_end_rejects_r0203` |
| P3 | `p3ReturnTheCopy` | Rejected R0202 at `return g;`, related `&h.v` | 3,748 / 615 / 5 | `a_copy_keeps_its_dependency_on_the_original_so_an_escape_rejects_r0202` |
| P15 | `p15ReturnTheCopysReference` | Rejected R0202 at `return keepR(g);`, related `&h.v` | 3,855 / 674 / 5 | `a_copy_keeps_its_dependency_on_the_original_so_an_escape_rejects_r0202` |
| twin (ii) | `returnTheRecord` | Rejected R0202 at `return h;`, related `&h.v` | 3,215 / 461 / 5 | `a_copy_keeps_its_dependency_on_the_original_so_an_escape_rejects_r0202` |
| P11 | `p11MoveIntoCall` | Rejected R0203 at `h`, related `&h.v` | 3,375 / 547 / 5 | `a_move_of_a_self_borrowing_record_rejects_r0203` |
| P12 | `p12MoveIntoLet` | Rejected R0203 at `h`, related `&h.v` | 3,389 / 549 / 5 | `a_move_of_a_self_borrowing_record_rejects_r0203` |
| P13 | `p13MoveWhileBorrowed` | Rejected R0203 at `h`, related `&h.v` | 3,248 / 485 / 5 | `a_move_of_a_self_borrowing_record_rejects_r0203` |
| P7 | `p7CopyIntoLet` | Checked, MIR `copy h` | 3,607 / 638 / 5 | `a_copy_of_a_self_borrowing_record_is_valid_when_nothing_writes_or_escapes` |
| P8 | `p8CopyIntoCall` | Checked, MIR `copy h` | 6,181 / 910 / 5 | `a_copy_of_a_self_borrowing_record_is_valid_when_nothing_writes_or_escapes` |
| P9 | `p9CopyDeadBeforeWrite` | Checked, MIR `copy h` | 3,836 / 780 / 5 | `a_copy_of_a_self_borrowing_record_is_valid_when_nothing_writes_or_escapes` |
| P10 | `p10CopyWhileSharedBorrowed` | Checked, MIR `copy h` | 3,737 / 703 / 5 | `a_copy_of_a_self_borrowing_record_is_valid_when_nothing_writes_or_escapes` |
| P17 | `p17CopyIntoReferenceReturningCall` | Checked, MIR `copy h` | 6,333 / 977 / 5 | `a_copy_of_a_self_borrowing_record_is_valid_when_nothing_writes_or_escapes` |
| C19 valid twin | `deadLoanBeforeCopy` | Checked, MIR `copy h` | 6,085 / 861 / 5 | `a_copy_of_a_self_borrowing_record_is_valid_when_nothing_writes_or_escapes` |
| P14 | `p14MoveAfterLoanDied` | Checked, MIR `move h` | 5,793 / 722 / 5 | `a_move_of_a_self_borrowing_record_is_valid_when_the_other_loan_is_dead` |
| P4 | `p4ReturnThroughPass` | Incomplete at `pass(h)` | 2,727 / 218 / 3 | `calls_returning_a_reference_bearing_record_stop_incomplete_and_are_unanalysed` |
| P5 | `p5WriteAfterPass` | Incomplete at `pass(h)` | 2,764 / 228 / 3 | `calls_returning_a_reference_bearing_record_stop_incomplete_and_are_unanalysed` |
| P16 | `p16StoreIntoBox` | Incomplete at `store(h)` | 2,724 / 217 / 3 | `calls_returning_a_reference_bearing_record_stop_incomplete_and_are_unanalysed` |

`Checked` rows are valid programs. The MIR of each: P7 `copy h -> g`, P8 `copy h -> h` then `call take2`, P9 `copy h -> g` then cleanup of `g`, then the write, P10 `copy h -> g` with `k` live, P17 `copy h -> h` then `call keepR`; P14 `move h -> h` then `call takeS`. None is a `move` of `Own2`.

Ruling 21 (2) twins, as corrected by ruling 25: (i) writing `h.v` while `h.r` is live rejects: `writeWhileMemberLive` (`Own2`) and `ownWrite` (`Own`, below). (ii) `return h;` of `Own2` rejects R0202 (`returnTheRecord`); a move of a non-copyable record rejects (`OwnS`: P11, P12, P13); copies of copyable records are Checked (P7 to P10, P17, `deadLoanBeforeCopy`).

Test module sizes and whole-graph meters:

- `a_copy_keeps_its_dependency_on_the_original_so_a_write_or_scope_end_rejects_r0203`: typed 340,266 / 50,285 / 12, graph SourceRejected 205,171 / 7,678 / 6
- `a_copy_keeps_its_dependency_on_the_original_so_an_escape_rejects_r0202`: typed 330,741 / 48,662 / 12, graph SourceRejected 195,358 / 5,863 / 5
- `a_move_of_a_self_borrowing_record_rejects_r0203`: typed 331,503 / 48,782 / 12, graph SourceRejected 194,670 / 5,698 / 5
- `a_copy_of_a_self_borrowing_record_is_valid_when_nothing_writes_or_escapes`: typed 340,859 / 49,993 / 12, graph Incomplete 205,562 / 7,723 / 5
- `a_move_of_a_self_borrowing_record_is_valid_when_the_other_loan_is_dead`: typed 326,036 / 47,952 / 12, graph Incomplete 189,159 / 4,795 / 5
- `calls_returning_a_reference_bearing_record_stop_incomplete_and_are_unanalysed`: typed 333,980 / 48,973 / 12, graph Incomplete 195,006 / 5,054 / 5
- `the_member_replacement_check_covers_only_sibling_reference_members`: typed 353,793 / 51,619 / 12, graph SourceRejected 213,613 / 8,865 / 5
- `other_whole_value_paths_of_a_self_borrowing_record_stay_incomplete`: typed 365,714 / 54,164 / 12, graph Incomplete 216,715 / 6,116 / 5

## 4. Routes that are unanalysed, and are not evidence of soundness

**These pins record where the analysis stops. They do not show the programs are sound.** A later change that makes the analysis reach them must flip the pin, and a `Checked` verdict there needs a review.

- **P4, P5, P16** (`pass(h)`, `store(h)`): `Incomplete` at the call, targeted and graph root (test `calls_returning_a_reference_bearing_record_stop_incomplete_and_are_unanalysed`). Found bead **ri-qlh.13**, "Calls that return a reference-bearing record stop Incomplete (pass(h), store(h))" (needs-triage, parent `ri-qlh`, relates-to `ri-2s0`).
- **Ruling 25 condition 3, the extra probe**: the copy `g` outliving `h` through a callee that stores `g.r` into the caller's storage. Both forms stop before ownership: the typed analysis is `Incomplete` with no receipt (`pending Storage`). `put(slot: &mut Box1, o: Own2)` with `slot.r = o.r;` and the caller's `put(&mut b, h)` gives `pending Storage @ slot.r = o.r;` and `pending Storage @ &mut b` (typed 328,018 / 48,155 / 12); `putRef(slot: &mut &i32, o: Own2)` with `*slot = o.r;` gives `pending Storage @ *slot = o.r;` and `pending Storage @ &mut b` (326,434 / 48,022 / 12). A `List<&i32>` with `push` is not expressible (`MissingField`). Variants with `slot` a parameter of the caller, the result bound with `let`, and a plain `&i32` slot were all typed `Incomplete` (`pending Storage`) as well (probe8, probe9, probe12, probe14 to probe16). Test `a_callee_storing_a_copys_reference_into_caller_storage_stops_typed_incomplete`, listed under bead ri-qlh.13. **None was `Checked`, so no wrong acceptance was found.**
- **P19** (`dispose(h)` of a record): typed `Incomplete`, no receipt, for any record with or without a borrow (`obligation FamilySelection @ dispose(h)`, `pending TypedInput @ dispose(h)`; 325,474 / 47,894 / 12 for P19, 323,897 / 47,604 / 12 for the `Plain = { v: i32 }` control). Test `dispose_of_a_record_has_no_typed_receipt`. Found bead **ri-qlh.14**, "dispose of a record has no typed receipt" (needs-triage, parent `ri-qlh`, relates-to `ri-2s0`).
- The arrow that captures the record (`let f: () => Own2 = () => h;`): typed `Incomplete`, `pending Containment @ () => h` (324,232 / 47,806 / 12), as at C19. Test `an_arrow_capturing_a_self_borrowing_record_stays_typed_incomplete`.
- C19's other move paths: fifteen roots, each `Incomplete` before the whole-record use is judged. Re-measured at the base; every blocker and every cap equals C19's pin (test `other_whole_value_paths_of_a_self_borrowing_record_stay_incomplete`; module typed 365,714 / 54,164 / 12, graph Incomplete 216,715 / 6,116 / 5):

| Root | Outcome | Caps |
|---|---|---|
| `moveStructField` | Incomplete at `w` | 3,226 / 462 / 1 |
| `moveStructReturn` | Incomplete at `Wrap { o: h }` | 3,229 / 471 / 2 |
| `moveMemberStore` | Incomplete at `w` | 3,226 / 462 / 1 |
| `moveRecordUpdate` | Incomplete at `Own2 { ...h, v: 2i32 }` | 3,262 / 479 / 3 |
| `moveTuple` | Incomplete at `t` | 3,230 / 469 / 2 |
| `moveTupleReturn` | Incomplete at `[h, 1i32]` | 3,239 / 477 / 2 |
| `moveList` | Incomplete at `l` | 2,866 / 404 / 1 |
| `moveListReturn` | Incomplete at `[h]` | 2,875 / 412 / 2 |
| `moveWrapperReturn` | Incomplete at `Option.Some(h)` | 2,875 / 412 / 2 |
| `moveResultReturn` | Incomplete at `Result.Ok(h)` | 2,875 / 412 / 2 |
| `moveMatch` | Incomplete at `match (h) { x => 0i32 }` | 2,897 / 420 / 3 |
| `moveMatchCall` | Incomplete at `match (take2(h)) { 0i32 => 0i32, _ => 1i32 }` | 2,897 / 420 / 3 |
| `moveArrowCall` | Incomplete at `f()` | 2,887 / 419 / 3 |
| `moveWrapperLet` | Incomplete at `o` | 2,866 / 404 / 1 |
| `moveMatchWrapper` | Incomplete at `match (Option.Some(h)) { Option.Some(x) => 0i32, Option.None => 1i32 }` | 2,897 / 420 / 3 |

No path differs from C19's pin, so none is left unpinned. C19's `plainCopy` and `plainLet` pins existed only to pin its one-unit charge and are not brought over; C19's valid twin `deadLoan` is `deadLoanBeforeCopy` above.

## 5. 2C-F2 (ruling 21 (2))

The Q3 member-replacement check covers loans through a sibling **reference** member only. Both load modes (test `the_member_replacement_check_covers_only_sibling_reference_members`; typed 353,793 / 51,619 / 12, graph SourceRejected 213,613 / 8,865 / 5):

| Root | Outcome at the fix | Caps |
|---|---|---|
| `replaceOwn` | Checked | 3,411 / 658 / 5 |
| `replaceOwn2` | Checked | 3,147 / 541 / 5 |
| `replaceOwnM` | Incomplete at `{\n  let mut a = 1i32; let mut h = OwnM { v: 1i32, m: &mut a }; h.m = &mut h.v;\n  return 0i32;\n}` | 3,107 / 527 / 5 |
| `ownWrite` | Rejected R0203 at `h.v = 2i32;`, related `&h.v` | 3,494 / 708 / 5 |
| `ownReturn` | Rejected R0203 at `h`, related `&h.v` | 3,756 / 729 / 5 |
| `ownTake` | Rejected R0203 at `h`, related `&h.v` | 3,812 / 760 / 5 |
| `ownLet` | Rejected R0203 at `h`, related `&h.v` | 3,825 / 762 / 5 |
| `ownMTake` | Rejected R0203 at `h`, related `&mut h.v` | 3,541 / 624 / 5 |
| `ownMutMember` | Rejected R0203 at `h`, related `&mut h.v` | 3,815 / 760 / 5 |

`Own = { v: i32; r: &i32; m: &mut i32 }` with `h.r = &h.v;` is Checked, `Own2` is Checked, and `OwnM = { v: i32; m: &mut i32 }` with `h.m = &mut h.v;` is `Incomplete` at its body, its `d87e5dd2` outcome (not itself run: it is the outcome the brief names). `Own` moves (an `&mut` member), so its `return h;` is R0203 where `Own2`'s is R0202; neither is Checked.

**Red first.** With the `reference_member` condition removed from the source (`flow.rs` restored from `flow.rs.fixed` afterwards), `the_member_replacement_check_covers_only_sibling_reference_members` and `member_replacement_parent_caps_now_give_resource_limit` fail: `replaceOwn` and `replaceOwnM` (and the roots after them) give R0201 at the replacement, and the whole graph costs 213,012 / 8,571 / 5 against the fix's 213,613 / 8,865 / 5. The other ten new tests pass with and without the condition, because they pin base outcomes. Child log shows `1 failed`, ran in under 0.1 s. Log: `red-condition-removed.txt`.

## 6. Caps

No published cap moves except as ruling 25 and ruling 21 (2) say. The moved cost is the base's own: the eight roots of the members source that rejected R0201 at the replacement at `b082e6ac` now run on, which costs more. `member_replacement_parent_caps_now_give_resource_limit` asserts, in both modes, that the base's measurements (whole graph 213,012 / 8,571 / 5; `replaceOwn` 3,212 / 575 / 5; `replaceOwnM` 3,014 / 480 / 5; `ownWrite` 3,419 / 664 / 5; `ownReturn` 3,709 / 705 / 5; `ownTake` 3,762 / 734 / 5; `ownLet` 3,775 / 736 / 5; `ownMTake` 3,502 / 604 / 5; `ownMutMember` 3,763 / 734 / 5) now give `ResourceLimit`. These sources are new in this branch; no existing source moved. C19's moved caps are not on the base, so there is nothing to undo. With the change, `rfc_cases`, `retained`, `legacy_caps`, `graph::inventory`, `flow`, `records::`, `patterns::` and `field_routes::` (216 tests) and the three `m4_8` binaries pass with no edited expectation.

## Not established

- Soundness of any `Incomplete` route above (section 4).
- Execution: these are compiler judgments over MIR, not Rill execution.
- Hosted CI, the full M5 binary and `mise run check` were not run, by policy.
