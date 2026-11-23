# Retained contents generalised to `&string`: O13's joined sources reject early release (ri-2s0.11, 2026-11-24)

This is ticket R-RSA, ticket A of the retained-store sequence (city bead `ci-2ed6`). Before it, a store into a caller's `&mut &T` descriptor was checked only for `T = bool`, and a family call stopped at its call. RFC 0007 case O13 (line 491) therefore stayed `Incomplete` at `&mut slot` in its second source. Now the second source rejects early release like its direct-assignment source does: `joined` is `SourceRejected` with R0203 at `dispose(left)` (related `&left`), and `clear` is `Checked`.

## Plan

One session was enough, so there are no child tickets. Three gaps, in the order built:

1. **Contents (`&string`).** `retained_bool_contents_type` (`analysis.rs`) becomes `retained_contents_type`, which accepts `&mut &bool` and `&mut &string`. The bool-only machinery keeps a narrow `retained_bool_contents_type` (a filter over the general one): the two-hop route (`retained_two_hop_bool_type`, `terminal_two_hop_store`), the `Option<bool>` handoff and the Bool gate (`retaining_gate`). `RetainedBoolContract` becomes `RetainedContract`, `retained_bool_contract` becomes `retained_contract`; they are the same code, generalised in place. A string destination takes no independent Bool. The general sites: `lower` (the callee's retained slot and exit check), `bool_deref_descriptor_slot`, the `Borrow` `retained_target`, the pipe-retention probe and the destination probe in `call`.
2. **A call that passes the destination alone** (`clear(&mut slot)`). `retained_contract` admits one formal and zero sources. The call adds no source and ends none: the destination keeps every source it held, so `dispose(left)` after it still rejects. New MIR operation `CallRetainingNoInput { callee, destination }`; `CallDestination.input` is an `Option`. The other `CallRetaining*` operations and their `&bool` tests are unchanged.
3. **A family call to the retaining void shape.** `family_retaining_shape` (an unmetered syntax and type screen) admits a call whose written first argument is `&mut name` of a retained descriptor, whose implementation is in the same module, and whose written argument types satisfy the retained contract (`retained_contract_of`, the same code the metered check runs, with a step that charges nothing). The formals come from `selected_signature`, the lookup `call_family_selected` (O11) already did, now shared. The call then takes the existing retained-call path, naming the selected implementation as callee. `graph.rs` closes its edge with `DependencyContract::DeclaredSignature`, as for O11, so the implementation's own `{ while (true) {} }` root stays separate and open.

## What changed

- `analysis.rs`, `ownership.rs`, `ownership/build.rs`, `ownership/flow.rs`, `ownership/graph.rs` as above.
- Not renamed: `retains_exclusive_bool` and `incoming_exclusive_bools` (the first now also covers a `&mut &string` destination's exclusive source; the second stays bool-only).

## Tests

All in `tests/m5/rfc_cases.rs`, both installed load modes, targeted query and ordinary graph root, exact and independently one-short caps (`check_receipt`). `Incomplete`, `ResourceLimit` and `SourceRejected` are asserted apart, rejections with code, site and related.

- `o13_joined_retained_sources_reject_early_release` (row edited, second source): `clear` `Checked`, `joined` `SourceRejected` R0203 at `dispose(left)`, related `&left`; whole graph `SourceRejected`. The first source (direct assignment) is untouched.
- `o13_pre_retained_string_caps_now_give_resource_limit`: the old caps give `ResourceLimit` for `clear`, `joined` and the whole graph, in both modes.
- `o13_plain_string_retained_contract_rejects_early_release` (new source): the same sources through plain functions, with a `{ }` body for `remember` (a store `*slot = source;` is still a typed `Storage` blocker, ticket B / `ri-1up`). `remember` and `clear` `Checked`; `joined` and `loops` R0203 at `dispose(left)`, related `&left`; `released` (the release after the slot's last use) `Checked`.
- `o13_family_store_in_a_loop_rejects_and_after_the_last_use_checks` (new source): the family form of `loops` (R0203) and `released` (`Checked`).
- `o13_unsupported_family_store_siblings_stay_incomplete_at_their_calls` (rule 2): see below.

The `&bool` retained tests (`m5::retained`, and the `&mut &bool` cases in `m5::cancellation`, `m5::tuples` and `m5::graph`) pass with no edited expectation.

## Caps

Whole rows, `(work / entries / depth)`.

### O13 second source (mandated flip, rule 11 third case; whole graph D2 (b))

The ticket names the flip (chief ruling of 2026-11-23, item 1; ticket body "Caps"). Typed stats are unchanged at 332,133 / 48,966 / 12. The row's `pre_equality_graph` (189,687 / 4,102 / 3) is unchanged and still gives `ResourceLimit`.

| Query | Old (`1fb84233`) | New |
| --- | --- | --- |
| `clear` targeted | 1,993 / 88 / 1, `Checked` | 2,047 / 114 / 1, `Checked` |
| `joined` targeted | 2,015 / 97 / 3, `Incomplete` at `&mut slot` | 5,150 / 931 / 8, `SourceRejected` R0203 |
| Whole graph | 190,003 / 4,236 / 5, `Incomplete` | 193,195 / 5,097 / 8, `SourceRejected` |

Reason, `clear`: its `&mut &string` parameter now holds retained contents (a contents slot, its entry state and its exit check), as a `&mut &bool` parameter already did: +54 work, +26 entries. Reason, `joined`: it now lowers its calls and rejects at `dispose(left)` instead of stopping at `&mut slot`. Rule 1 cannot move either: `clear` has to establish the contents it is checked against, and `joined` has to do the work it now rejects after.

Per-root attribution of the whole-graph change, by building the module with each root alone at the parent `1fb84233` (a temporary worktree, removed) and at this change:

| Module | Parent | Now | Delta |
| --- | --- | --- | --- |
| `remember` family and implementation only | 187,021 / 4,110 / 5 | 187,021 / 4,110 / 5 | none |
| plus `clear` | 187,538 / 4,153 / 5 | 187,593 / 4,179 / 5 | +55 work, +26 entries |
| plus `joined` (the row) | 190,003 / 4,236 / 5 | 193,195 / 5,097 / 8 | +3,137 work, +835 entries over the previous row |

The deltas add up: 55 + 3,137 = 3,192 and 26 + 835 = 861, the change of the totals. `joined` sets the new depth 8 (targeted and graph); `clear` stays at depth 1. The implementation root stays `Incomplete` and is not read by the caller.

### New sources (no parent measurement; not moves)

| Source | Typed | Whole graph | Roots |
| --- | --- | --- | --- |
| plain `O13_PLAIN` | 337,065 / 49,293 / 12 | 203,549 / 7,569 / 8, `SourceRejected` | `remember` 2,234 / 202 / 1; `clear` 2,183 / 182 / 1; `joined` 3,746 / 999 / 8; `loops` 3,791 / 1,018 / 8; `released` 9,199 / 1,887 / 8 |
| family `O13_FAMILY` | 337,785 / 49,820 / 12 | 202,579 / 6,570 / 8, `SourceRejected` | `loops` 4,491 / 983 / 8; `released` 8,406 / 1,630 / 8 |

The `pre_equality_graph` field of these new rows holds the exact graph cap one work short.

### Unsupported siblings at the parent caps (rule 2)

Each was measured at `1fb84233` and passes at this change with the same numbers: the screen is unmetered, so the call is still the blocker at its old cost. Each is `Incomplete` at its call, exact and one-short caps.

| Source | Blocker | Typed | Targeted | Whole graph |
| --- | --- | --- | --- | --- |
| a Bool beside the string destination | `remember(&mut slot, &left, flag)` | 328,486 / 48,489 / 12 | 1,969 / 73 / 3 | 188,672 / 4,174 / 5 |
| three sources | `remember(&mut slot, &left, &middle, &right)` | 331,252 / 49,199 / 12 | 1,992 / 85 / 3 | 189,179 / 4,186 / 5 |
| grouped destination | `remember((&mut slot), &left)` | 328,296 / 48,419 / 12 | 1,957 / 66 / 3 | 188,476 / 4,167 / 5 |

O9 (`o09_retained_list_source_contract_is_incomplete_at_its_calls`, a `&mut List<&User>` destination) and the O11 rows pass unchanged: no cap moved.

### Fixed caps unchanged

`m5::legacy_caps`, `m5::graph::inventory`, `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`, `m4_8_ownership` (the M4 17-source baseline 372,246 / 54,059 / 12) and `m4_8_callable_once` pass with no edited expectation. The full M5 binary runs only in hosted CI.

## Metering rules

- Rule 1: `family_retaining_shape` and the contract screen it runs charge nothing; the first charge is `selected_signature`, reached only for a call the contract admits.
- Rule 3: `selected_signature` probes the family map with `charge_key` over `family_probes()` as `call_family_selected` did; no other keyed lookup was added or touched.
- Rule 11: the O13 flip above; old caps asserted `ResourceLimit`.

## Not established

- The callee-side store `*slot = source;` through a `&mut &string` stays a typed `Storage` blocker, so no `&string` store body was checked here; the O13 implementation body is `{ while (true) {} }`. Ticket B (`ri-1up`, O2 and O8) covers stores, and the reference-to-record contents.
- Only a same-module implementation, a void result and a written `&mut name` destination are admitted; a cross-module implementation, a pipe, a default argument and a field destination stay on the old path.
- The two-hop route, the `Option<bool>` handoff and the Bool gate stay `&bool` only. A Bool beside a string destination stays `Incomplete` at the call.
- O9 (`&mut List<&User>`) stays on `ri-ema`.
- This is compiler MIR evidence; no Rill program ran.

## Hosted CI

Not run for this branch (rill owner policy of 2026-11-22; `common.md`). The rill merger watches CI after the merge.

## Logs

`~/.t3/evidence/rill/m5-build-2026-11-22/ci-2ed6/`: `baseline-m5.log` (the parent), `stage1-m5.log`, `stage2-m5.log`, `stage2-m5-other.log`, `final-m5.log` (after merging the base tip), `final-m4_8_*.log`, `final-fmt.log`, `final-clippy.log`, `final-diffcheck.log`.
