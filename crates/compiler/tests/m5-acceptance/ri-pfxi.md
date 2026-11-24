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

## Not established

- No Rill program ran: these are compiler judgments over MIR (M6).
- Only an `&mut i32` field of a record without a custom disposer is admitted. `&mut bool`, `&mut string`, `&mut i64` and an `&mut` member of a custom-disposer record stay `Incomplete` (pinned at the parent caps). A nested reference was not probed. A tuple with an `&mut` element is typed `Incomplete` (`pending TypedInput`, R0100 on the shared element), so M5 never reaches it: probe observation, not a pinned test.
- Only `*record.field += literal` writes through the member, on a local binding. `=` and other operators, a grouped target, moving the member out and reborrowing it stay `Incomplete` at the use. Probe observations, not pinned tests: a parameter of record type with an `&mut` member can be read (`*h.first`, `*h.second`) and passed on to a call that reads it, and writing through it is a typed `ReadOnly` finding; a function that returns such a record (`make(p, q): Holder`) checks, and a caller of it stays `Incomplete` at the call.
- A record holding a `&mut i32` field next to a `string` field checks (`withText`); its cleanup order against the loan is the existing aggregate order, not separately pinned beyond that test.
