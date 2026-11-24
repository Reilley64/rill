# ri-2s0.7.1: O10 typed `get` on `List<&T>`, 2026-11-24

Ticket ri-2s0.7.1 (city bead `ci-ofz5`), child 2 of 3 under ri-2s0.7 (the O10 split: extraction, this ticket, then the R0202 route ri-2s0.7.2). Branch `m5-ownership-cleanup/r-o10b-ri-2s0.7.1`, built on the base `942c8fc6` with child 1 (`m5-ownership-cleanup/r-o10-ri-2s0.7`) merged first; child 1 reached the base at `fedd91dc` before hand-back.

## Cause

`function slot(users: List<&User>): Option<&&User> { return get(&users, 0u64); }` stopped at `obligation FamilySelection @ get(&users, 0u64)`. The cause is not the `&T` element. On the first pass of `ordinary_call_step` the borrow `&users` has no type yet (`inputs = [None, None]`). `proven_context` (the List-get width context, which needs a typed receiver) is therefore false, and the `0u64` index gets `ordinary_context`, a hypothetical premise, instead of `establish`. The premise stays on the index and the call never reports `checked`. The same happens for any named owned List (`List<i32>`, `List<User>`, `List<&i32>`) with a literal index; `get(&users, i)`, `get(view, 0u64)` and `first(&users)` check. That wider case is filed as `ri-can9` and not fixed here.

## What changed

`crates/compiler/src/analysis/scalars/generics/ordinary_calls.rs`: in the receiving-context loop, when the index's receiver (argument 0) has no type, is `&name` (a direct `Prefix("&")` over a `ValueReference`, no group) and `name` has an established `List<&Record>` type (shared reference to an application record, no generics), the index keeps no context on this pass (`continue`). The call is revisited once `&users` is typed, takes the `proven_context` path and checks. The screen (`pending_list_reference_receiver`) reads syntax and slot types only and charges nothing (rule 1).

Row O10 source 1 is now a receipt source in both installed load modes: typed `Checked`. The graph is `Incomplete`, and the root `slot` is `Incomplete` at `get(&users, 0u64)` in both the targeted query and the ordinary graph root: no ownership lowering of the List `get` lookup exists, so that call is the blocker. This is the "different, more specific M5-only blocker" the ticket allows. Ri-2s0.7.2 needs that lowering before it can reject the reference to the destroyed slot. `m5-status.md` row O10 is updated.

## Tests

| Test (under `m5`) | Result | Caps |
| --- | --- | --- |
| `rfc_cases::o10_extracted_reference_checks_and_slot_get_stays_incomplete`, source 1 | typed `Checked`; graph `Incomplete`; `slot` `Incomplete` at `get(&users, 0u64)` (targeted and graph root) | typed 325,899 / 48,063 / 12; graph 186,123 / 4,117 / 5; `slot` 1,869 / 22 / 2. Exact and one short on each axis replayed by the harness |
| `rfc_cases::o10_pre_slot_get_typed_cap_now_gives_resource_limit` (new) | the old typed cap gives `ResourceLimit`, no receipt | old 325,319 / 47,875 / 12 |
| `rfc_cases::o10_get_siblings_keep_their_parent_caps` (new, rules 1, 2, 5) | each stays typed `Incomplete` at `obligation FamilySelection` plus the two `pending TypedInput` | `List<&mut User>` 325,319 / 47,875 / 12; `List<&i32>` 324,494 / 47,829 / 12; owned `List<User>` 325,250 / 47,838 / 12; grouped `get(&(users), 0u64)` on `List<&User>` 325,123 / 47,792 / 12 |

Red step: with source 1 expecting `Checked` and the unfixed source, `rfc_cases::o10_extracted_…` fails at the typed completion (`Incomplete`, expected `Checked`; log `red-on-base.log`; the child run shows the test ran).

The sibling caps were measured at the parent (`b7e58baf`, the merge of child 1 into the base) in a `git worktree add` with its own `CARGO_TARGET_DIR`: the new test file run on the unchanged parent source passes with these exact numbers (log `siblings-at-parent-b7e58baf6c1f9ec9e9d9a26aaf04bc617384c8e0.log`). So they are parent caps, not re-measured on the candidate.

## Cap changes (metering rule 11, mandated flip named in the ticket)

| Cap | Old | New | Reason |
| --- | --- | --- | --- |
| O10 source 1, typed | 325,319 / 47,875 / 12 (`Incomplete`) | 325,899 / 48,063 / 12 (`Checked`) | the flip the ticket names: `get` is selected on `List<&Record>` with result `Option<&&Record>` (+580 work, +188 entries) |

No other cap moved. `m5::legacy_caps`, `m5::graph::` (including `graph::inventory`), `m4_8_scoped_join_caps` and `m4_8_match_boolean_snapshots` pass unchanged. The graph row of source 1 is a new whole-graph pin of a new receipt source (D2 (b) does not apply: no existing graph is re-pinned, `legacy-caps.tsv` and the inventory have no row for it). `pre_equality_graph` of source 1, 185,807 / 3,983 / 3, is the graph minus the equality root's 316 work and 134 entries, as in the other rows with that field; it gives `ResourceLimit`.

## Local checks

Logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-ofz5/`: `rfc_cases` (46 passed), `legacy_caps`, `graph` (44), `iteration`, `retained`, `records`, `roots`, `generic`, `ordinary`, `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`, `m4_8_list`, `m4_8_result_use`, `fmt`, `clippy`. The full M5 binary runs in hosted CI only.

## Not established

- Only a named `List<&Record>` receiver written `&name` is admitted (application record, no generics, shared reference). A grouped receiver, a local-typed or computed receiver, `List<&mut T>`, `List<&i32>` and owned lists with a literal index keep their parent caps (`ri-can9`).
- The ownership graph does not lower `get` on a List, so the root is `Incomplete`. R0202 for the reference to the destroyed slot is ri-2s0.7.2.
- These are compiler judgments over typed facts. No Rill program ran.
