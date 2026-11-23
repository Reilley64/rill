# ri-2s0.7: O10 extract a contained &User from an owned List<&User>, 2026-11-24

Ticket ri-2s0.7 (city bead `ci-u1xm`), owner decision of 2026-11-23 22:17 AEDT. Branch `m5-ownership-cleanup/r-o10-ri-2s0.7`, built on `4f7a8032`. This file records child 1 of 3. The ticket stays open until children ri-2s0.7.1 and ri-2s0.7.2 land.

## Reading and plan

RFC 0007 line 485 names two distinct operations on an owned `List<&User>`: extracting the stored `&User` by value (valid while its incoming source stays valid), and referencing the List's element slot, which is destroyed with the List (R0202). The row's two sources are exactly these: `extract` (consuming `for`) and `slot` (`get(&users, 0u64)`, `Option<&&User>`). Both stopped in M4: `extract` at `pending Storage`, `slot` at `obligation FamilySelection`.

That is more than one session, so the ticket is split:

1. **Child 1 (this branch): consuming `for` over a `List<&Record>` parameter, returning the yielded reference.** Files: `typed/result_use/loops.rs` (admit a shared application-record reference yield on the owned scalar iteration path), `scalars/iteration.rs` (a `Some(user)` construction in the body of such a loop is ordinary), `ownership/build.rs` (a `List<&Record>` parameter has the canonical List disposal and no element drop; `Option<&Record>` is an admitted shared reference option), `ownership/build/iteration.rs` (the consuming iteration admits that List and yield, and skips the i32 element check). No new MIR operation: the existing `OpenOwnedListI32`/`AdvanceOwnedListI32` carry the iteration, because a shared reference has no element drop action. The yielded `&User` is a copy of the stored reference and carries no loan of the List's slot, so it is a distinct value from a reference to the slot.
2. **ri-2s0.7.1 (not built): typed `get` on `List<&T>`.** Done when source 1 types.
3. **ri-2s0.7.2 (not built, depends on ri-2s0.7.1): the R0202 route** for returning a reference to the destroyed element slot.

The parent cannot be blocked by its own children in the tracker, so ri-2s0.7 stays open as their parent.

## What changed

Row O10 (renamed `o10_extracted_reference_checks_and_slot_get_stays_incomplete`, was `o10_owned_list_reference_extraction_has_no_receipt`) source 0 is now a receipt source. In both installed load modes the typed report is `Checked`, and `extract` is `Checked` with MIR for the targeted query and the ordinary graph root. Source 1 is unchanged (typed `Incomplete`, 325,319 / 47,875 / 12). `m5-status.md` row O10 is updated.

## Tests

| Test (under `m5`) | Result | Caps |
| --- | --- | --- |
| `rfc_cases::o10_extracted_reference_checks_and_slot_get_stays_incomplete` | `extract` `Checked` (targeted and graph root); source 1 unchanged | typed 324,131 / 47,845 / 12; `extract` 3,539 / 513 / 6; graph `Incomplete` 188,037 / 4,612 / 6 (pre-equality 187,721 / 4,478 / 3 gives `ResourceLimit`); source 1 325,319 / 47,875 / 12. Exact and one short on each axis are replayed by the harness |
| `rfc_cases::o10_extraction_siblings_keep_their_parent_caps` (new, rules 1 and 2) | `List<&mut User>` and `List<&i32>` consumed by `for` stay typed `Incomplete` at `pending Storage` | measured at the parent `4f7a8032` in a `git worktree add` with its own target directory, unchanged: `&mut User` 323,289 / 47,677 / 12; `&i32` 322,461 / 47,631 / 12 (log `siblings-at-parent-4f7a8032.log`) |
| `rfc_cases::o10_pre_extraction_typed_cap_now_gives_resource_limit` (new) | the old typed cap gives `ResourceLimit` with no receipt | old 323,289 / 47,677 / 12 |

Red step: with the new expectation and the base sources, `rfc_cases::o10_…` fails at the typed completion (`Incomplete`, expected `Checked`; log `red-on-base-4f7a8032.log`; the child run shows the test ran).

## Cap changes (metering rule 11, mandated flip named in the ticket)

| Cap | Old | New | Reason |
| --- | --- | --- | --- |
| O10 source 0, typed | 323,289 / 47,677 / 12 (`Incomplete`) | 324,131 / 47,845 / 12 (`Checked`) | the flip the ticket names: the loop now checks and the receipt is built (+842 work, +168 entries) |

No other cap moved. The admission screens on the yield type from facts already paid for (`iteration.yielded`) before any new charge, and the new `List<&Record>` parameter proof is reached only for that type. `m5::legacy_caps`, `m5::graph::` (including `graph::inventory`), `m4_8_scoped_join_caps` and `m4_8_match_boolean_snapshots` pass unchanged. The graph row of O10 source 0 is a new whole-graph pin of a new receipt source, not a re-pin of an existing graph; `legacy-caps.tsv` and the inventory have no row for it.

## Local checks

Logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-u1xm/`: `rfc_cases` (31 passed, before the final rename), `legacy_caps`, `graph::`, `iteration`, `roots`, `records`, `field_routes`, `retained`, `fresh_result_storage`, `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`, `m4_8_list`, `m4_8_result_use`, `m4_8_ownership`. The full M5 binary runs in hosted CI only.

## Not established

- `get` on `List<&User>` and the R0202 rejection of a reference to the destroyed element slot: children ri-2s0.7.1 and ri-2s0.7.2. The done-when of ri-2s0.7 is not met until they land.
- Only a `List<&Record>` parameter of an application record (no generics) and a shared reference yield are admitted. A local `List<&T>`, `List<&mut T>` and `List<&i32>` stay `Incomplete`.
- These are compiler judgments over MIR. No Rill program ran.
