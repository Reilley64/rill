# Family-selected call contracts and O11's exclusive return dependency (ri-2s0.6, 2026-11-23)

This is ticket R-O11. Before it, the ownership builder stopped at every call to a family (`function f<L>(list: L): R;` resolved to an `implement`): `call_declared_once` admits only the private `callableOnce` wrapper. O11 (RFC 0007 line 487) therefore stayed `Incomplete` at `editAndView(&mut owned)` in both of its roots.

The planner's decision (comment on `ri-2s0`, 2026-11-23): a family call resolved to an `implement` is checked against the selected implementation's declared signature, which is the caller-visible contract (RFC 0007 line 204). The caller never reads the implementation body. The H2-authored O11 source is unchanged.

## What changed

- `ownership/build.rs`: `family_view_shape` (unmetered syntax and type screen) and `call_family_selected`. The admitted shape is one written `&mut name` argument, a selected `implement` in the same module, and a `&i32` result. The implementation's declared parameter and result types come from the family's captured facts (`captured_family`, probed with `charge_key` over the family count, then a charged scan of the family's implementations). They must equal the actual argument type (`&mut List<i32>`) and the call's result type. A `while`/`for` body sets `in_loop`, which keeps the loop form on the old path.
- `ownership.rs`: `CallResult::RetainsExclusiveInput` and `Operation::CallExclusiveView { callee, argument, into }`. The result slot keeps the staged exclusive loan, so a whole-List read before the view's last use conflicts (R0201) and one after it does not. `RetainsInputs` and `RetainsRecordInput` carry shared loans only; this is the exclusive twin.
- `ownership/flow.rs`: the new result kind retains its input loans like the other two and emits the new operation.
- `ownership/graph.rs`: `DependencyContract::DeclaredSignature`. The edge to the selected implementation is closed by its declared signature, as the once and shared wrappers' edges are closed by theirs. The implementation's own open source root stays separate (`editAndView`'s body `{ while (true) {} }` is `Incomplete` and is not read by the caller).
- `ordinary.rs`: `family_probes()`, the family map's size, for the `charge_key` charge.

## Tests

[`rfc_cases::o11_exclusive_return_view_rejects_whole_read_before_last_use`](../m5/rfc_cases.rs) (renamed from `o11_exclusive_return_view_contract_is_incomplete_at_its_call`), both installed load modes, targeted query and ordinary graph root:

- `conflict` is `SourceRejected`, R0201 at `&owned`, related `&mut owned`.
- `ordered` is `Checked` with MIR.
- Whole graph `SourceRejected`.

Caps (work / entries / depth); each exact cap is replayed and each independently one-short cap gives `ResourceLimit`.

| Query | Before (`7002855e`, `Incomplete` at `editAndView(&mut owned)`) | Now |
| --- | --- | --- |
| `conflict` targeted | 2,014 / 77 / 4 | 3,028 / 202 / 6 (`SourceRejected`) |
| `ordered` targeted | 2,032 / 83 / 4 | 3,316 / 336 / 6 (`Checked`) |
| Whole graph of this source | 189,575 / 4,219 / 5, `Incomplete` | 191,877 / 4,600 / 6, `SourceRejected` |
| Typed stats | 328,846 / 48,424 / 12 | unchanged |

Per-root attribution of the whole-graph change (D2 (b) and rule 11, third case: the blocker moved): `conflict` adds 1,014 work and 125 entries and `ordered` adds 1,284 work and 253 entries (2,298 and 378). The remaining 4 work and 3 entries are the graph's own bookkeeping for the extra rejected finding and dependency edges. `conflict` and `ordered` both reach depth 6, which sets the new maximum (the call frame, its staged borrow and the view read), from 4.

The old caps now report `ResourceLimit`: [`o11_pre_family_call_caps_now_give_resource_limit`](../m5/rfc_cases.rs) asserts it for `conflict` at 2,014 / 77 / 4, `ordered` at 2,032 / 83 / 4 and the whole graph at 189,575 / 4,219 / 5 in both modes. The row's `pre_equality_graph` now holds the 189,575 / 4,219 / 5 caps, which also report `ResourceLimit`.

## Unsupported siblings at their parent caps (rule 2)

[`o11_unsupported_family_call_siblings_stay_incomplete_at_their_calls`](../m5/rfc_cases.rs) pins four sources, each measured at the parent (the figures are the same before and after this change; the screen is unmetered). Each stays `Incomplete` at its call, with exact and one-short caps:

| Source | Blocker | Targeted | Whole graph |
| --- | --- | --- | --- |
| implementation returns `i32` (`editAndCount(&mut owned)`) | the call | 1,947 / 47 / 3 | 187,802 / 4,148 / 5 |
| shared input `&List<i32>` (`readView(&owned)`) | the call | 1,953 / 50 / 3 | 187,988 / 4,151 / 5 |
| grouped `editAndView((&mut owned))` | the call | 1,956 / 51 / 3 | 188,051 / 4,152 / 5 |
| call inside a `while` body | the call | 1,977 / 66 / 5 | 188,312 / 4,167 / 5 |

Before `in_loop` the loop form reached `Checked` at 3,210 / 306 / 8 (graph 189,547 / 4,409 / 8); the ticket keeps loop forms unsupported, so it does not.

## O9 and O13

The ticket expected O9 and O13 to reach a new blocker after family admission. They did not: neither calls the admitted shape (both pass two arguments or a `&mut &T` destination and return `void`), so they take the old path and keep their parent blockers and caps. `o09_retained_list_source_contract_is_incomplete_at_its_calls` and `o13_joined_retained_sources_reject_early_release` pass without an edited expectation. Nothing was re-pinned for them; they stay on `ri-ema`.

## Unchanged

No fixed cap moved. `m5::rfc_cases` (31 tests with `legacy_caps` and `graph::inventory`), `m5::legacy_caps`, `m5::graph::inventory`, `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`, `m4_8_callable_once` and `m4_8_ownership` (the M4 17-source baseline 372,246 / 54,059 / 12) pass with no edited expectation. The full M5 binary runs only in hosted CI.

## Not established

- Only the `&mut name` / `List<i32>` / `&i32` shape in the implementation's own module is admitted. Cross-module implementations, other element types, other result types and mutation of the whole List before the view's last use are not exercised; the shared-input, grouped, other-result and loop forms are pinned as unsupported.
- The implementation's body is not checked by this call; its own root stays `Incomplete`.
- This is compiler MIR evidence; no Rill program ran.
