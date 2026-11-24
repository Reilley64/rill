# Retained contents for `&Record`: O2 and O8 reach the RFC outcome (ri-1up, 2026-11-24)

This is ticket R-RSB, ticket B of the retained-store sequence (city bead `ci-wliw`), after R-RSA (`ri-2s0.11`). Before it, RFC 0007 O2 and O8 stopped in typed at `*slot = source;` and `&mut view` (`Storage`). Now O2 is `Checked` with MIR and O8's `poison` is `SourceRejected` with R0202 at `*slot = &local;`, related `&local`, as a targeted query and as an ordinary graph root, in both installed load modes. Parent: `bc9bfdd5` (the base tip that contains `ri-2s0.11`).

## Plan (written before the build)

Probing the base with the typed `Storage` blocker relaxed (the `ci-e202` sizing) showed three ownership gaps beyond typed. One session was enough, built in two parts:

- **B1: a scalar field read through a named `&Record`** (`view.x`, `view: &Point`, a parameter or a local view). `p.x` on an incoming `&Point` was `Incomplete` everywhere, not only in O2. Free screen, no new lookup.
- **B2: `&mut &Record` retained contents.** Typed: admit the store `*slot = source;` through `&mut &Record` and the call argument `&mut view`. Ownership: the same retained parameter, destination and source contract as `&bool` and `&string` (`RetainedContract`, `CallRetaining`), the `&second` source argument to the call, and the loan transfer so that the caller's view carries the source's loan after the call. The callee exit check (R0202) already covered `*slot = &local;` once typed admitted it.

No child tickets: both parts are in this branch.

## What changed

- `analysis.rs`: `retained_record_candidate` (free: `&mut &R`, `R` a non-generic application nominal) and `retained_record_contents` (the candidate whose declaration is a record of `i32` fields; the lookup charged by the module path's length per comparison, rule 3, after the free screen). `retained_contents_type` (`&bool`, `&string`) is unchanged.
- `ownership.rs`: `retained_contents_of` (type-only first, then the record lookup); `retained_contract` and `retained_contract_of` take the contents. An exclusive source is still read only for `&bool` and `&string` contents.
- `ownership/build.rs`: a scalar field read through a named shared reference lowers to `Dereference` (`scalar_field_through_reference`); `bool_deref_descriptor_slot`, the `Borrow` `retained_target`, the pipe-retention probe and the destination probe use the general contents; a `&Record` argument beside a record destination is admitted as a source.
- Typed (`result_use.rs`, `result_use/calls.rs`): `caller_bool_storage` and the exact-descriptor staleness at a call also accept record contents.
- Tests: `tests/m5/rfc_cases.rs` (the O2 and O8 rows), `rfc_cases/record_contents.rs` (new), `rfc_cases/o12.rs` (`viewField`), `m5-status.md`.

## The O2 and O8 flips (mandated, rule 11 third case; whole graph D2 (b))

Named by the ticket and the chief of staff's ruling of 2026-11-23. There was no ownership result before (typed `Incomplete`, no receipt), so there is no old targeted cap and no whole-graph move; the old caps are the typed caps, which now give `ResourceLimit` (`o02_pre_record_contents_typed_cap_now_gives_resource_limit`, `o08_pre_record_contents_typed_cap_now_gives_resource_limit`).

| Row | Parent (typed) | Now (typed) | Roots now | Whole graph now |
| --- | --- | --- | --- | --- |
| O2 | `Incomplete` 325,943 / 47,994 / 12 | `Checked` 326,933 / 48,161 / 12 | `remember` 2,466 / 167 / 3; `selected` 6,373 / 855 / 6; `expect` 8,410 / 961 / 6, all `Checked` | `Incomplete` 190,814 / 5,004 / 6 (as O1: the standard library roots) |
| O8 | `Incomplete` 327,681 / 48,227 / 12 | `Checked` 328,745 / 48,394 / 12 | the O2 roots (2,494 / 181 / 3; 6,429 / 883 / 6; 8,494 / 1,003 / 6) and `poison` `SourceRejected` R0202, 2,815 / 302 / 4 | `SourceRejected` 192,713 / 5,271 / 6 |

Each row's `pre_equality_graph` (the whole graph less the expectation root: 182,404 / 4,043 / 6 and 184,219 / 4,268 / 6) gives `ResourceLimit`. Depth 6 is `selected`. The typed cost rises by 990 and 1,064 work because the stores and arguments are now checked instead of stopping at their first blocker.

## Other tests (`rfc_cases::record_contents`, `rfc_cases::o12`), both modes, exact and one short

| Test | Result |
| --- | --- |
| `record_view_reads_check` | B1. `viaParam`, `second`, `bound`, `grouped`, `looped`, `named` and `late` `Checked` (new; each was `Incomplete` at its read at the parent, e.g. `viaParam` 1,978 / 81 / 3). Whole graph 193,340 / 5,239 / 5. |
| `unsupported_record_reads_keep_their_parent_caps` | Rule 2. `&mut Point` view, `view.x + view.y`, `view.x == 1i32`, `*view` stay `Incomplete` at their parent caps (1,906 / 45 / 3; 1,905 / 45 / 3 twice; 2,262 / 104 / 3; typed 328,492 / 48,011 / 12; graph 188,245 / 4,241 / 5, equal at the parent). A `string` field read stays R0501 at 1,914 / 55 / 3 (parent equal); its module's graph moved 186,447 / 4,165 to 186,564 / 4,221 because `id` beside it is now admitted. |
| `stored_source_loans_reach_the_caller_view` | The loan transfer. A scope that ends before `view.x` is read: R0203 at the block, related `&second`. A grouped destination, a loop, the other field, a grouped read, a never-read view and a bound read check; the same store in dead code checks and rejects nothing (rule 6). |
| `callee_stores_of_locals_escape_and_of_parameters_do_not` | The callee exit check. `poison` R0202 at `*slot = &local;`; `forwardLocal` R0202 at `remember(slot, &local)`, related `&local`; forwarding a parameter, keeping a local view and storing two parameters check. |
| `caller_sequence_of_a_record_retaining_call` | Rule 9: the whole operation sequence of `selected` (init, borrow, exclusive borrow, source borrow, `retain view <- &second`, read, export, cleanup) for O2's body and for an empty body. |
| `empty_body_and_family_calls_with_record_contents` | See "Typed admissions". |
| `unsupported_record_contents_stay_incomplete` and `..._parent_caps_now_give_resource_limit` | See "Rule-1 exception". |
| `o12::read_through_a_stored_view_of_a_temporary_rejects_its_release` | See "`viewField`". |

## Rule-1 exception (chief of staff, 2026-11-24 07:19 and 11:35 AEDT)

The record screen's field-list lookup is charged and no paid fact carries the list. A string field, a nested record and an enum pay two lookups each (the callee's store, the caller's `&mut view`) and stay typed `Incomplete` at the same blockers. Typed work, parent `bc9bfdd5` to now:

| Sibling (as `remember(slot: &mut &T, source: &T)`) | Parent | Now | Blockers (unchanged) |
| --- | --- | --- | --- |
| `T` with a `string` field | 325,487 / 47,914 / 12 | 325,631 / 47,914 / 12 (+144) | `*slot = source;`, `&mut view` |
| `T` with a nested record field | 327,152 / 48,135 / 12 | 327,296 / 48,135 / 12 (+144) | the same |
| `T` an enum | 324,194 / 47,682 / 12 | 324,334 / 47,682 / 12 (+140) | `*slot = source;` |
| generic `Box<i32>` | 325,398 / 47,894 / 12 | unchanged | the same |
| `&mut &mut Point` | 325,413 / 47,912 / 12 | unchanged | the same |

The added cost exceeds the standing ruling's 20 units, so it was taken to the chief of staff, who granted it as built (no cache; no M4 fact in this ticket), listed by test name in the rule-1 note of `docs/agents/compiler-metering.md` and on `ri-qlh.4`, which also records "carry record field and enum variant lists as an M4 fact". These siblings never reach ownership, so there is no ownership cost.

## Typed admissions (chief of staff, 11:35 AEDT, accepted with conditions)

- An empty-body `remember(slot: &mut &Point, source: &Point): void { }` called as `remember(&mut view, &second)`: typed `Incomplete` at `&mut view` at the parent (328,191 / 48,344 / 12 with the test's `early`), now `Checked` (329,185 / 48,511 / 12). Ownership: `selected` `Checked` (6,269 / 845 / 6), with the whole sequence pinned (rule 9). The caller's view **still carries `&second`'s loan** after the call: `early`, with the call in a scope that ends before `view.x`, is R0203 (3,888 / 708 / 8). The call is checked against the declared signature and never the callee's body, which is how `&bool` and `&string` retained calls already behave (`retained_call_contract`: "do not use the callee body to narrow"). The condition "the caller's view carries no new source after the call" therefore does not hold for any body, the empty one included; the callee body is never read. Nothing is accepted that stores a reference unchecked: the call is conservative.
- A family call `remember(&mut view, &second)` to an `implement remember(slot: &mut &Point, source: &Point)`: typed `Incomplete` at `&mut view` at the parent (327,695 / 48,334 / 12), now `Checked` (328,618 / 48,501 / 12). Ownership: `selected` `Incomplete` at `remember(&mut view, &second)` (2,663 / 181 / 3, whole graph 189,426 / 4,282 / 5). The unmetered family screen reads the written formals' types for `&bool` and `&string` contents only, because a record needs the lookup.

Both old typed caps give `ResourceLimit` (`empty_body_and_family_calls_with_record_contents`).

## `viewField` (chief of staff, 2026-11-24 07:23 AEDT)

`let view = &shifted(Point { x: 1, y: 2 }); return view.x;` was `Incomplete` at `view.x` (1,986 / 86 / 3) in `rfc_cases::o12::unsupported_siblings_keep_their_parent_caps`. B1 lowers the read, and the borrowed temporary is released while `view` is still read: R0203 at `let view = &shifted(Point { x: 1, y: 2 });`, related `&shifted(Point { x: 1, y: 2 })`, 2,962 / 348 / 7 (the same code and site as `stored`); its module's graph is `SourceRejected` 188,579 / 4,666 / 7, typed 325,209 / 47,806 / 12. No free fact separates this view from O2's `let mut view = &first;`, so a rule-1 screen cannot keep it `Incomplete`. The old cap gives `ResourceLimit`. The remaining O12 siblings (`copyOut`, `bound`, `interpolated`, `number`, `owner`, `flag`) stay at parent caps; with `viewField` removed each root's cost fell by a few units (fewer declarations), so that test was re-measured at `bc9bfdd5` without it: typed 333,771 / 48,779 / 12; graph 191,338 / 4,565 / 5; `shifted` 2,862 / 279 / 3; `copyOut` 2,306 / 124 / 3; `bound` 1,972 / 75 / 3; `interpolated` and `flag` 1,958 / 71 / 4; `number` 1,952 / 65 / 4; `owner` 2,062 / 126 / 4. The candidate gives the same numbers.

## Fixed caps

Unchanged after merging the base tip `2c44d88d`: `m5::legacy_caps`, `m5::graph::inventory`, every `retained` test (`&bool`, `&string`, O13), `records`, `field_routes`, `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`, `m4_8_result_use` and `m4_8_ownership` (the M4 17-source baseline). The full M5 binary runs only in hosted CI.

## Metering rules

- Rule 1: every new path screens by type first. The record lookup is the one departure (above). The scalar read screen charges nothing before its first `step`.
- Rule 3: the lookup in `retained_record_contents` is charged by `(path length + 1) x probes` before `.get`.
- Rule 11: O2 and O8 (mandated), `viewField` (ruling of 07:23).

## Not established

- Only a flat non-generic record of `i32` fields is admitted. A `string` field, a nested record, an enum, a generic record and `&mut &mut Point` stay `Incomplete` (typed).
- A scalar field read through `&mut Point`, arithmetic or a comparison on such a read, and a whole `*view` copy stay `Incomplete`.
- The family-selected store with record contents stays `Incomplete` at its call.
- A cross-module implementation, a pipe and a field destination stay on the old path, as for `&string`.
- This is compiler MIR evidence; no Rill program ran. Hosted CI did not run for this branch.

## Logs

`~/.t3/evidence/rill/m5-build-2026-11-22/ci-wliw/`: `stage1-m5.log`, `stage2-record-contents.log`, `final-m5.log`, `final-m4_8_*.log`, `clippy.log`, `final-fmt.log`, `final-diffcheck.log`.
