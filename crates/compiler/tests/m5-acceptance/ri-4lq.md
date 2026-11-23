# ri-4lq: K2 non-primitive fields and the written field disposal edge, 2026-11-23

Ticket ri-4lq (review ci-uaa1 SP-F3; ri-px1.7 #2), brought into M5 by the owner's decision of 2026-11-23 19:18 AEDT. Branch `m5-ownership-cleanup/r-k2-ri-4lq`, built on `7002855e`.

## What changed

Row K2 needed a non-primitive field: a custom disposer over a record whose field is itself an owned record. It also needed the active body's written `dispose(value.field)` to be a verified graph edge. Before this ticket, both stopped at `Incomplete`: the nested layout at its parameter, the written field disposal in the graph. Three source changes:

- `ownership/build.rs`, `nested_record_region`: a record with a nested owned record field may carry its outer custom disposer. The region's root records the disposer, as a flat record's does. `ownership.rs` builds the active disposer's epilogue for such a parameter by copying the proved region (`clone_region`). A flat record's epilogue is built as before.
- `ownership/build.rs`, written `dispose`: an operand with field projections resolves through `read_value_place`, which knows nested record regions. An operand without projections resolves exactly as before.
- `ownership/graph/closed_release.rs`, `written_operand`: in an active disposer body, a written `dispose` of a field (`value.field`, or the nested `value.child.text`) of type `string` is a verified `BuiltinDispose` edge. The written path must name the same fields as the MIR operand. A field read has no `binding`, so its root is matched to the operand's slot by the existing declaration search. The edge is screened before the first charge, from the place and the field's own declaration: the field is declared in this module and written `string`. Outside an active disposer, and for any other field type, the edge stays unverified as before.

## Tests

All in both installed load modes, targeted query and ordinary graph root. Every new positive and negative pins exact work / entries / depth, and one short on each axis gives `ResourceLimit` with no MIR and no finding.

| Test (under `m5`) | Result | Caps |
| --- | --- | --- |
| `disposers::owning_text::nested_record_disposer_runs_once_then_drops_the_remaining_fields` (new, SP-F3) | `nested`: one outer disposer run. The body moves `moved` to `saved`, disposes `manual` (the verified edge) and drops `saved`. The epilogue then drops `tail`, `child.text` (ending `child`) and `head`, skipping `moved` and `manual`. The active body and the graph root check with the targeted MIR. `leaf` (`dispose(value.child.text)`), `bare` and `grouped` (`dispose((value.manual))`) check the same way. `dead` grants no edge and drops `manual` in the epilogue. `twice` is R0200, `outside` is R0501. Boundaries: `explicit` and `canonical` are `Incomplete` at `value`; `ordinaryField` checks in the targeted query but its graph root is `Incomplete` at `dispose(value.child.text)` | `nested` 6,810 / 985 / 3; `leaf` 6,422 / 807 / 3; `bare` 6,423 / 807 / 3; `grouped` 6,424 / 807 / 3; `dead` 6,324 / 766 / 4; `twice` 6,334 / 781 / 3; `outside` 3,043 / 336 / 3; `explicit`, `canonical` 2,961 / 287 / 3; `ordinaryField` 3,125 / 374 / 3; graph `SourceRejected` 217,159 / 8,459 / 5 |
| `disposers::owning_text::written_field_disposal_siblings_keep_their_parent_caps` (new, rules 1 and 2) | A `bool` field in an active disposer, bare and grouped, and a `string` field written in an ordinary function (a parameter and a local) stay `Incomplete` at the call in the graph root | measured at the parent `7002855e` and unchanged: `flatFlag` 5,384 / 476 / 3; `groupedFlag` 5,385 / 476 / 3; `projected` 2,439 / 186 / 3; `local` 2,512 / 222 / 4; graph `Incomplete` 193,291 / 5,257 / 5 |
| `disposers::owning_text::a_written_field_disposal_is_a_verified_graph_edge` (was `a_written_field_disposal_keeps_the_graph_root_open`) | the mandated flip: `manual`'s graph root and its active body are `Checked`, the body's one edge `BuiltinDispose` | targeted 5,487 / 518 / 3, unchanged |
| `disposers::owning_text::outer_disposer_runs_once_then_drops_the_remaining_text_fields` | whole-graph re-pin, below | graph 198,767 / 5,963 / 5 |
| `records::nested_owned_records_clean_initialized_leaves_on_exit` | `customRoot` (custom disposer, nested `child: Inner`) moves from the `Incomplete` list to `Checked`: one outer disposer run; the epilogue drops `sibling` then `child.text` | 5,817 / 559 / 1 |
| `records::named_nested_record_cleanup_after_one_edge_child_move` | `outerDisposer` was `Incomplete` at `saved`. With the nested layout admitted, moving `saved.child` out of a `CustomOuter` outside its active disposer is R0501 (related `saved`), as the flat `outsideLocal` already was. `innerDisposer` is unchanged | 4,009 / 607 / 5 |
| `rfc_cases::o14_replacement_reaches_the_old_disposer_and_a_stale_view_rejects` | O14 `replace`: the graph root is `Checked` (it was `Incomplete` at `dispose(value.text)` in the reached disposer). `m5-status.md` row O14 and note 4 updated | `replace` 6,922 / 882 / 4, unchanged; graph re-pin below |
| `rfc_cases::o14_pre_field_edge_graph_cap_now_gives_resource_limit` (new) | the O14 graph cap before ri-4lq gives `ResourceLimit` | old 192,578 / 5,361 / 5 |

Red step: these tests run against the parent `7002855e` sources. `nested_record…` fails with `nested` `Incomplete`. `a_written_field_disposal_is_a_verified_graph_edge`, the two re-pinned graphs, `customRoot`, `outerDisposer` and both O14 tests also fail. The sibling test passes there (log `red-on-parent-7002855e.log`).

## Cap changes (metering rule 11, D2 (b), 2026-11-23)

The flip is the ticket's mandated flip, named in the ticket before the diff. Two whole-graph expectations move, and only those. No targeted cap moved.

| Graph | Old | New | Root that adds the difference |
| --- | --- | --- | --- |
| `owning_text` `SOURCE` | 197,911 / 5,963 / 5 | 198,767 / 5,963 / 5 | `manual`, through its active body's `dispose(value.second)`: +856 work, +0 entries |
| O14 | 192,578 / 5,361 / 5 | 193,432 / 5,361 / 5 | `replace`, through `implement release(value: Owner)`'s `dispose(value.text)`: +854 work, +0 entries |

Depth stays 5 in both. It is set by the i32 equality root (ri-785.9), not by these roots. Each old cap now gives `ResourceLimit`, asserted in `outer_disposer_runs_once…` and `o14_pre_field_edge_graph_cap_now_gives_resource_limit`. The earlier historical totals (197,595 / 5,829 / 3; O14's `pre_equality_graph`) are still asserted. `legacy-caps.tsv` has no row for either graph; `m5::legacy_caps` and `m5::graph::inventory` pass unchanged.

## Local checks at this branch

Logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-1cxp/`: `check-iteration.log` (41 passed), `check-cancellation::.log` (15), `check-disposers::.log` (11), `check-records::.log` (16), `check-graph::.log` (41, including `graph::inventory`), `check-legacy_caps.log` (2), `check-rfc_cases.log` (27), `check-roots::.log` (6); `fmt.log`; `clippy-compiler.log` (`-p compiler --all-targets -D warnings`). The full M5 binary runs in hosted CI only.

## Not established

- Explicit `release(value)` and canonical `dispose(value)` of a whole nested record with a custom disposer stay `Incomplete` at `value`. Only the normal edge runs the outer disposer.
- A written `dispose(x.field)` outside an active disposer is still not a verified graph edge. Four graph tests use that shape as their unresolved edge: `graph::aliases_share…`, `graph::local_blocker…`, `graph::delivery::independent_delivery…` and `graph::closed_release::written_guard_release…`.
- A record with an `Option` field (`Option<string>` or `Option<bool>`) is still `Incomplete` at its parameter, with or without a custom disposer. K2's non-primitive field is witnessed by the nested record, the first alternative SP-F3 names.
- `disposers::active_disposer_cleans_only_remaining_fields` and `custom_record_rebuild` have the same written field disposal but still assert only the targeted query.
- These are compiler judgments over MIR. No Rill program ran.
