# ri-2s0.7.2: O10 R0202 for a reference to the destroyed List element slot, 2026-11-24

Ticket ri-2s0.7.2 (city bead `ci-lf1k`), child 3 of 3 under ri-2s0.7 (the O10 split: extraction `ri-2s0.7`, typed `get` `ri-2s0.7.1`, this ticket). Branch `m5-ownership-cleanup/r-o10c-ri-2s0.7.2`, built on `63b9b3e3` (the base with children 1 and 2 merged).

## Reading and design

Row O10's `slot` returns `get(&users, 0u64)`, an `Option<&&User>`, from a function that owns `users`. The payload refers to the List's element slot, and the slot is destroyed when `users` is: RFC 0007 R0202. Child 1 yields the stored `&User` by value from a consuming `for`. This route yields a different value, a reference to the slot, so the two stay distinct in the model.

After child 2 the root stopped at `get(&users, 0u64)` because ownership had no lowering of a List `get`. The cause was two guards, not a missing proof of R0202: `return` refused any call returning a reference option (`build.rs`, "no checked caller-backing contract") and `slot()` has no storage for `Option<&&Record>`.

## What changed

- `ownership/build/list_get.rs` (new): `list_element_lookup` screens `get(&name, <key>)` on a named owned `List<&Record>`, returning `Option<&&Record>`, from syntax and typed facts only (rule 1): no `step`, reservation or allocation. The `get` family is matched against the fixed array of List operations (`collection_protection`, 13 entries, constant). The key is a `u64` literal or a plain `u64` local or parameter. `list_element_option_slot` gives the return storage (a shared reference option with `StorageCleanup::End`). `call_list_element_reference` emits one action.
- `ownership/build.rs`: `return` allocates that storage and skips the "no contract" refusal only when the screen passes; `call` reaches the route only into such a slot. `admitted_option` is unchanged, so no other `Option<&&T>` is admitted.
- `ownership/flow.rs`, `ownership.rs`: `Action::LookupListElement` and `Operation::LookupListElement { list, index: ElementIndex, into, lookup }`. The action initializes `into` with a shared loan of the List's slot of a new `LoanKind::ElementSlot`, plus the List's own contained loans as `Contents`. It conflict-checks the List as an observed read. `ElementSlot` counts as local storage (`borrows_local_storage`), so the existing `Export` exit check rejects it. Its related site is the lookup itself (`loan.site`), not the List's declaration; the related site of every other loan kind is unchanged (the O11 `escaped` root still reports `owned`). A new kind, not `Backing`, because the special routes that match `Backing` exactly (list views, scoped joins, retained destinations) must not accept a reference to a slot.
- `m5-status.md` row O10.

ri-can9 (the `FamilySelection` stop of `get` on other owned Lists) is not needed here: child 2's receiver screen already types this shape `Checked`. Not fixed.

## Tests

Red step: with source 1 expecting `SourceRejected` and the unchanged source, `rfc_cases::o10_extracted_…` fails at the graph completion (`Incomplete`, expected `SourceRejected`; log `red-on-base.log`; the child run shows the test ran).

| Test (under `m5`) | Result | Caps |
| --- | --- | --- |
| `rfc_cases::o10_extracted_reference_checks_and_slot_reference_is_rejected` (renamed, was `…slot_get_stays_incomplete`), source 1 | `slot` `SourceRejected`, R0202, site `return get(&users, 0u64);`, related `get(&users, 0u64)`, targeted and graph root, both modes; `extract` stays `Checked` | `slot` 3,059 / 267 / 5; graph `SourceRejected` 187,315 / 4,363 / 5 (pre-equality 186,999 / 4,229 / 5 gives `ResourceLimit`); typed 325,899 / 48,063 / 12 unchanged. Exact and one short on each axis replayed by the harness |
| same test, source 2 (new): `indexed(users, index: u64)` returning `get(&users, index)` | `SourceRejected`, R0202, site `return get(&users, index);`, related `get(&users, index)` | typed 325,821 / 48,013 / 12; root 3,081 / 278 / 5; graph 187,398 / 4,374 / 5 (pre-equality 187,082 / 4,240 / 5) |
| `rfc_cases::o10_pre_slot_lookup_caps_now_give_resource_limit` (new) | the old `slot` root cap and the old graph cap give `ResourceLimit`, no MIR, no finding | old root 1,869 / 22 / 2; old graph 186,123 / 4,117 / 5 |
| `rfc_cases::o10_lookup_siblings_keep_their_parent_caps` (new, rules 1, 2, 5) | each types `Checked` and stays `Incomplete` in ownership at its parent blocker | see below |

Siblings of the lookup (rule 2), all in one module, measured at the parent `63b9b3e3` in a `git worktree add` with its own target directory (log `lookup-siblings-at-parent-63b9b3e3.log`; the new test file ran there unchanged and the candidate gives the same numbers): typed 342,897 / 50,941 / 12; graph 191,091 / 4,248 / 5.

| Root | Blocker | Caps |
| --- | --- | --- |
| `computed`: `get(&users, index + 1u64)` | `get(&users, index + 1u64)` | 2,009 / 93 / 2 |
| `grouped`: `get(&users, (0u64))` (grouped key; depth is not admitted) | `get(&users, (0u64))` | 2,003 / 89 / 2 |
| `view`: `get(users, 0u64)` on `&List<&User>` | `get(users, 0u64)` | 1,992 / 89 / 2 |
| `head`: `first(&users)` | `first(&users)` | 2,003 / 89 / 2 |
| `bound`: `let found = get(&users, 0u64); return found;` | `found` | 2,002 / 89 / 1 |
| `discarded`: `get(&users, 0u64);` | `get(&users, 0u64)` | 2,011 / 89 / 3 |

The typed-level siblings (`List<&mut User>`, `List<&i32>`, an owned `List<User>` and the grouped receiver `get(&(users), 0u64)`) stop in the typed pass, so ownership never runs on them. `o10_get_siblings_keep_their_parent_caps` pins their parent caps and passes unchanged.

## Cap changes (metering rule 11, mandated flip named in the ticket)

| Cap | Old | New | Reason |
| --- | --- | --- | --- |
| O10 `slot` root | 1,869 / 22 / 2 (`Incomplete` at `get(&users, 0u64)`) | 3,059 / 267 / 5 (`SourceRejected`, R0202) | the flip the ticket names: the lookup is lowered and the exit check runs (+1,190 work, +245 entries, depth 2 to 5) |
| O10 source 1 whole graph | 186,123 / 4,117 / 5 (`Incomplete`) | 187,315 / 4,363 / 5 (`SourceRejected`) | the `slot` root only (+1,192 work, +246 entries: its targeted cost plus 2 and 1 of graph bookkeeping); the maximum depth stays 5 (the `slot` root now reaches 5 itself; the pre-equality cap keeps depth 5 so that its work and entries, 316 and 134 short, are what give `ResourceLimit`). D2 (b) does not apply: source 1's graph was pinned as new by child 2 and no legacy graph is re-pinned. |

The old root and graph caps give `ResourceLimit` (`o10_pre_slot_lookup_caps_now_give_resource_limit`). The cost is not avoidable by a rule-1 screen: the root is the proof that now runs. No other cap moved. `m5::legacy_caps`, `m5::graph::` (including `graph::inventory`), `m5::iteration`, the whole of `rfc_cases`, `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots` and `m4_8_list` pass unchanged.

## Local checks

Logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-lf1k/`: `rfc_cases` (53 passed), `legacy_caps` (3), `graph` (44), `iteration` (41), `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`, `m4_8_list`, `fmt`, `clippy`. The full M5 binary runs in hosted CI only.

## Not established

- Only a named owned `List<&Record>` parameter (application record, no generics) read as `&name`, with a `u64` literal or plain `u64` local key, in a `return` of the call, is lowered. Every other shape keeps its parent blocker and caps (table above).
- The lookup always reaches R0202 on this route: the List is a parameter and is destroyed at the exit that the `return` exports through. No checked root exercises `Operation::LookupListElement`, so its MIR form is not tested beyond compiling and the shared flow checks.
- `ElementSlot` is a new loan kind. A later route that borrows the result (`&found`) would turn it into `Contents` and report the List, not the lookup, as the related site.
- These are compiler judgments over MIR and typed facts. No Rill program ran.
