# ri-pfxi: RFC 0007 O7, indirect overlap through record reference fields (R-O7), 2026-11-24

Ticket ri-pfxi (spec ri-2s0), city bead ci-jnns. Branch `m5-ownership-cleanup/r-o7-ri-pfxi`, parent `b6c93f30` (retained-store B, ri-1up, merged). The compiler judges O7 over MIR. It does not run it (M6).

## Plan (written before the code)

O7 needs three things (found by ri-kdi, city bead ci-mw88): an exclusive reference member in record storage, a conflict between loans staged into one construction (R0201), and a store through a field dereference. One session builds all three, in this order:

1. **Test first, before any `&mut` member is admitted (the chief's condition).** `rfc_cases::o07::overlapping_construction_rejects_r0201`: `Holder { first: &mut value, second: &value }` and the reversed order are `SourceRejected` with R0201, site the second borrow, related the first. It does not use the O7 text: that text uses the record afterwards (`*holder.first += 1i32;`), which build stops at before flow runs, so a probe that only admitted `&mut` members saw no R0201 there. The new source has no such use. The construction alone must reject. Red on the parent (`Incomplete` at `holder`), committed first.
2. **Admit the exclusive member and make the construction conflict.** `aggregate` (`ownership/build.rs`) admits an `&mut i32` field of a record without a custom disposer. The record is move-only. `Action::Init` of such a record uses every member slot (`flow.rs`, `liveness`), so the first member's loan is live at the second borrow and the existing `conflict` check rejects. The check is the sound part: without that use, the overlapping construction is `Checked`.
3. **Store through the field dereference.** `add_through_exclusive_i32` (`build/i32_arithmetic.rs`) admits `*record.field += literal` where the field is an `&mut i32` member of a local record, resolved to its member slot like a parameter descriptor. `*record.field` reads were already admitted.
4. **Flip the O7 row** (mandated flip, rule 11, third case) and update `m5-status.md`.

Split clause: not needed. All four steps fit one session, so there are no child tickets.

Rule 1: the screens for the new admission read facts already paid for (a type, a projection, an `Option`, a flag), before the first charge. No declaration lookup is added, so the standing rule-1 exception is not used. Rule 3: no new keyed probe.

## What changed

Commit A (`8abb590a`): the plan and `rfc_cases::o07::overlapping_construction_rejects_r0201`, red on the parent (`Incomplete` at `holder`, `parent-b6c93f30-o07-red.log`). Commit B (this one):

- `aggregate` (`ownership/build.rs`) admits an `&mut i32` field of a record with no custom disposer (`exclusive_i32_reference`, the projection is a record `Field`). The member's cleanup is `End`; the slot moves, so the record is move-only. The screen reads the type, the projection and the disposer argument, all already in hand: no charge, so a refused shape spends what it spent before. The root is flagged (`Local::exclusive_member`).
- `liveness` (`ownership/flow.rs`): `Action::Init` of a flagged record uses every member slot. The first member's loan is then live when the second borrow runs, and the existing `conflict` check rejects it with R0201 (site the later borrow, related the earlier). This is the sound part. With only the admission (the probe in ri-kdi), the construction `Holder { first: &mut value, second: &value }` is `Checked`; with the `Init` use removed, three of the five O7 tests fail (checked on this branch).
- `add_through_exclusive_i32` (`ownership/build/i32_arithmetic.rs`) admits `*record.field += literal` when the field is an `&mut i32` member of a local binding. Its descriptor is the member slot, resolved by the same `place` call a parameter descriptor uses, after every screen. `*record.field` reads were already admitted.
- `m5-status.md`: the O7 row.

## Tests (`m5/rfc_cases.rs`, `m5/rfc_cases/o07.rs`; both installed load modes, targeted query and graph root, exact and one short on each axis)

| Test | Result |
| --- | --- |
| `o07::overlapping_construction_rejects_r0201` | done-when 1: `overlapConstruct` (`Holder { first: &mut value, second: &value }`) R0201 at `&value`, related `&mut value`; `overlapReversed` (`&value` then `&mut value`) R0201 at `&mut value`, related `&value`; neither record is used afterwards |
| `o07_overlapping_reference_fields_reject_r0201` (the O7 row, was `..._are_not_yet_rejected`) | `overlap` (the RFC-prose module, with the write and the read) R0201 at `&value`, related `&mut value` |
| `o07_pre_admission_caps_now_give_resource_limit` | the old targeted and graph caps give `ResourceLimit` |
| `o07::staged_loans_conflict_unless_the_owners_are_distinct` | `Checked`: `disjoint` (`&mut a`, `&b`, written through and read back), `twice`, `endsBeforeSource` (the owner read after the holder's last use), `twoOwners` (two `&mut` of distinct owners), `groupedBorrows`, `looped`, `replaced`, `withText` (a record with a `string` member). R0201: `sameOwnerTwice` (two `&mut` of one owner), `readOwnerWhileHeld`, `secondHeldOverFirst` (a second record borrows what the first holds), `scopedOverlap`. `Incomplete` at the use: `assignThrough` (`*holder.first = 5i32`), `groupedTarget`, `movedOut` (`let moved = holder.first;`), `reborrowed` (`&mut *holder.first`) |
| `o07::unsupported_reference_members_keep_their_parent_caps` | `Incomplete` at `h`, at the caps measured at the parent: `&mut bool`, `&mut string`, `&mut i64` members and an `&mut i32` member of a record with a custom disposer (rules 1 and 2) |

Red at the parent `b6c93f30` (`git worktree add`, own target directory, `parent-b6c93f30-o07-red.log`): with the new test files only, `overlapping_construction_rejects_r0201` and `staged_loans_conflict_unless_the_owners_are_distinct` fail (`Incomplete`); the sibling test passes, so the admission adds no work to refused shapes. The `installed` child ran each test (`running 1 test`, asserted by `installed`).

## Caps

Targeted caps are work / entries / depth; each is exact, and one short on each axis gives `ResourceLimit` with no MIR and no finding (the harness asserts both). Whole-graph figures are `analyze_ownership`.

| Root | Old (parent, `Incomplete` at `holder`) | New |
| --- | --- | --- |
| O7 `overlap`, targeted | 2,233 / 88 / 1 | 2,653 / 282 / 5, R0201 |
| O7 whole graph | 187,028 / 4,183 / 5 `Incomplete` | 187,450 / 4,378 / 5 `SourceRejected` |
| O7 typed | 324,564 / 47,874 / 12 | unchanged |
| `overlapConstruct`, `overlapReversed` | new source | 2,462 / 203 / 5 each; graph `SourceRejected` 188,526 / 4,475 / 5; typed 327,083 / 48,063 / 12 |
| `LOANS` roots | new source | see `m5/rfc_cases/o07.rs` (`LOANS`): graph `SourceRejected` 228,054 / 11,538 / 7, typed 364,913 / 54,043 / 12 |
| `SIBLINGS` roots | the same, measured at the parent | `boolMember` 2,303 / 120 / 1, `textMember` and `wideMember` 2,301 / 119 / 1, `guarded` 2,310 / 121 / 1; graph `Incomplete` 190,674 / 4,432 / 5; typed 334,264 / 48,531 / 12 |

### Rule 11 note: the ticket's mandated flip, 2026-11-24

Named in ri-pfxi before the diff ("the O7 flip is a mandated flip under rule 11's third case"; chief ruling of 2026-11-23 22:29 AEDT, item 3).

- O7 `overlap`, targeted: old 2,233 / 88 / 1, new 2,653 / 282 / 5. Reason: the construction, the write through `holder.first` and the read through `holder.second` now lower, and the conflict is found in flow.
- O7 whole graph (D2 (b)): old 187,028 / 4,183 / 5, new 187,450 / 4,378 / 5, +422 work and +195 entries, depth 5 both. Per root: the graph of the module without `overlap` (only `type Holder`) measures 185,341 / 4,095 / 5 at the parent and on this branch, so no other root moved. `overlap` as a graph root adds 1,687 work and 88 entries at the parent, 2,109 and 283 now: +422 and +195. Depth 5 is the depth the rest of the graph already sets (the i32 equality root).
- `o07_pre_admission_caps_now_give_resource_limit` asserts both old caps give `ResourceLimit`. O7's `pre_equality_graph` (186,712 / 4,049 / 3) still does.
- No `legacy-caps.tsv` row and no `m5::graph::inventory` row moved. No other published cap moved: every filtered test below passes without an edited expectation.

No rule-1 exception is used and the standing ruling (record or enum field-list lookup) was not used: no declaration lookup is added.

## Local checks (logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-jnns/`)

| Check | Result | Log |
| --- | --- | --- |
| `--test m5 -- rfc_cases retained legacy_caps graph::inventory` | 106 passed | `m5-rfc-retained-legacy-inventory.log` |
| `--test m5 -- field_routes:: disposers:: records:: o15_arithmetic o05_field_arithmetic captures::` | 99 passed | `m5-field-disposers-records.log` |
| `--test m4_8_scoped_join_caps --test m4_8_match_boolean_snapshots` | 1 passed each | `m4_8.log` |
| `cargo fmt --all -- --check` | clean | `fmt.log` |
| `cargo clippy --locked --no-default-features -p compiler --tests -- -D warnings` | clean | `clippy.log` |

The full M5 binary and `mise run check` run in hosted CI only.

## Finish pass (city bead ci-81k1, 2026-11-24): the chief's conditions

Chief ruling of 2026-11-24 12:37 AEDT (`chief-rulings-9.md`): the `&mut` member admission is accepted on conditions. All tests are in `m5/rfc_cases/o07_forms.rs`, both installed modes, each root as a targeted query and as the ordinary graph root, exact and one short on each axis (the harness asserts `ResourceLimit` with no MIR and no finding), and each module's typed and whole-graph caps. A `Checked` root also asserts its whole MIR as one ordered list (rule 9; `Operation::None` join fillers are left out). No source change, no existing test or cap moved, no rule-1 exception used.

**Condition 1 and 2, by form** (`work / entries / depth`; sites are the finding's site, then its related site):

| Form | Test | Accepted root, caps | Rejecting twin, caps |
| --- | --- | --- | --- |
| `withText`: `Named { first: &mut a, name: "x" }`, then `*holder.first += 1i32` | `o07_forms::with_text_record_pins_its_sequence_and_the_overlap_twin` (typed 327,751 / 48,156 / 12; graph `SourceRejected` 189,430 / 4,774 / 5) | `withText` 3,029 / 486 / 5; MIR: init a, borrow mut a, init name, init holder, deref, add, install, export, `cleanup [drop holder.name, end holder.first, end holder, end a]` | `withTextOverlap` (`Named2 { first: &mut a, second: &a, name: "x" }`) R0201 at `&a`, related `&mut a`, 2,510 / 225 / 5 |
| the record parameter, read and passed to a call | `o07_forms::record_parameter_pins_its_sequences_and_the_source_use_twins` (typed 335,232 / 49,177 / 12; graph `SourceRejected` 199,346 / 6,361 / 5) | `readFirst` 2,532 / 236 / 3 (`*h.first`), `readSecond` 2,533 / 236 / 3 (`*h.second`), `forward` 5,524 / 585 / 5 (moves `h` into `readFirst(h)`), `callRead` 5,906 / 771 / 5 (builds the record from `&mut a`, `&b`, passes it; `cleanup [end b, end a]`) | with the record live, `readOwnerWhileLive` (`let seen = a;`) R0201 at `a`, related `&mut a`, 3,239 / 476 / 5; `borrowOwnerWhileLive` (`&a`) R0201 at `&a`, related `&mut a`, 3,241 / 476 / 5; `writeOwnerWhileLive` (`a = 5i32;`) **R0203** at `a = 5i32;`, related `&mut a`, 3,299 / 511 / 5 |
| the function returning such a record | `o07_forms::returning_function_pins_its_sequence_its_caller_and_the_local_twins` (typed 329,158 / 48,370 / 12; graph `SourceRejected` 191,732 / 4,961 / 5) | `make(p: &mut i32, q: &i32): Holder` 2,556 / 247 / 4 (move `p`, copy `q`, init, export, `cleanup [end q: &i32]`); its caller `useMake` is `Incomplete` at `make(&mut a, &b)`, 2,330 / 136 / 3 | `makeLocalMut` (`&mut` member borrows a body-local) R0202 at the `return`, related `a`, 2,550 / 252 / 5; `makeLocalBound` (the same through `let h`) R0202 at `return h;`, related `a`, 3,047 / 376 / 5 |

Why these codes: a read or a borrow of the owner conflicts with the live exclusive loan, which is the access conflict, R0201 (as for `readOwnerWhileHeld` in `LOANS`). Assigning to the owner replaces a borrowed source, which the existing rules give as R0203 (the write rule, as for any live loan). Returning a loan of a local is R0202.

Every twin was rejected on the first run. No accepted twin, no wrong acceptance. Probed beyond the tests with the same result: `Holder { first: &mut *p, second: &*p }` returned from a function that takes `p: &mut i32` is R0201 at `&*p`, related `&mut *p`; the record is consumed by the call in `callRead`, so a read of `a` after `readFirst(h)` checks.

**Not expressible.** The parameter form has no twin inside the callee: a write through either member of a parameter (`*h.first += 1i32`, `*h.second += 1i32`) is a typed `ReadOnly` finding (the parameter binding is not `mut`), so the typed pass stops it before ownership; probe observation, not a pinned test. The twins for the parameter are therefore the caller's, above.

**Condition 3** (both `Incomplete`, so a later change that admits them fails these tests):

| Shape | Test | Outcome and caps |
| --- | --- | --- |
| tuple with an `&mut` element, `[&mut a, &b]` | `o07_forms::tuple_with_an_exclusive_element_stays_typed_incomplete` | typed `Incomplete`, ownership never receives it: `finding R0100 @ &b`, `pending TypedInput @ [&mut a, &b]`, `pending Storage @ let t = [&mut a, &b];`; typed 321,947 / 47,478 / 12, exact and one short |
| nested reference members `&&i32` and `&mut &mut i32`; a record whose member is a record with an `&mut` member, bound (`Wrap { inner: inner, .. }`) or written in place | `o07_forms::nested_reference_shapes_stay_incomplete` (typed 333,580 / 48,935 / 12; graph `Incomplete` 192,245 / 4,517 / 5) | `refToShared` and `refToRef` `Incomplete` at `h`, 2,345 / 146 / 1 each; `nestedHolder` at `outer`, 2,707 / 204 / 1; `nestedLiteral` at `outer`, 2,343 / 142 / 1 |

## Not established

- No Rill program ran: these are compiler judgments over MIR (M6).
- Only an `&mut i32` field of a record without a custom disposer is admitted. `&mut bool`, `&mut string`, `&mut i64` and an `&mut` member of a custom-disposer record stay `Incomplete` (pinned at the parent caps). The tuple and the nested-reference shapes stay `Incomplete` and are pinned by the finish pass (condition 3, above).
- Only `*record.field += literal` writes through the member, on a local binding. `=` and other operators, a grouped target, moving the member out and reborrowing it stay `Incomplete` at the use (pinned: `assignThrough`, `groupedTarget`, `movedOut`, `reborrowed`).
- A record holding a `&mut i32` field next to a `string` field checks (`withText`); its cleanup order is the whole list pinned in `o07_forms`.
