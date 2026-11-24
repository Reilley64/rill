# Borrowed temporary and static string view, RFC 0007 O12 (ri-2s0.8, 2026-11-24)

This is ticket ri-2s0.8, split from ri-w0g by the owner's decision of 2026-11-23 (city bead `ci-6ok`). The O12 module is `shifted(point: Point): Point`, `within(): i32 { return (&shifted(Point { x: 1, y: 2 })).x; }`, `stored(): void { let view = &shifted(Point { x: 1, y: 2 }); let later = view; }` and `literal(): &string { return &"static"; }`. At the parent, `4f7a8032`, all three of `within`, `stored` and `literal` were `Incomplete` for the targeted query and the graph root.

## Plan (written before the build)

Reading, settled from RFC 0007 lines 173, 216, 481 and 489 and RFC 0006 lines 481, 483 and 637:

- `within`: a borrowed temporary lives through its enclosing statement. The call result gets a backing slot; the shared borrow reads one scalar field; the statement ends the descriptor, then the backing.
- `stored`: the backing ends with the statement while `view` is still needed by `later = view`. That is a release while needed, R0203, at the binding with the borrow as related item: the rule the existing `field_routes::later_use_requires_a_named_scalar_owner` already pins for a `bool` temporary. R0202 is the exit check for a body-local owner and does not apply.
- `literal`: a direct borrow of an uninterpolated quoted, raw or backtick literal has program-lifetime static backing. The route needs no owner, no cleanup and no loan. A named owner (`let text = "x"; return &text;`) keeps R0202.

Files: `ownership/build.rs` (the `&call(..)` binding route widened from `bool` to a flat record of scalars; a new `(&call(..)).field` return route; a static-literal arm in the `&` expression), `ownership/flow.rs` and `ownership.rs` (one new action and MIR operation, `BorrowStatic`), tests `tests/m5/rfc_cases.rs` and `tests/m5/rfc_cases/o12.rs`, and the O12 row of `m5-status.md`. No M4 (typed) change. One session of work: no child tickets.

## What changed

- `BorrowStatic { holder }` (new MIR operation): initializes a descriptor over a string literal's static backing. It adds no loan and nothing ends with it but the named holder. Admitted only for `&` over a `string` literal or a template of text pieces only (no hole), after groups. `&mut`, `&1i32`, interpolated templates and named owners are not admitted.
- A `&call(..)` binding of a flat application record of scalars (no text, no custom disposer, no guard) now takes the temporary route that `bool` had: `CallIntoTemporary`, `Borrow`, then the statement's cleanup ending the backing. A later use of the descriptor then rejects with R0203 through the existing flow.
- `(&call(..)).field` in a `return` (scalar field of such a record): `CallIntoTemporary`, `Borrow`, a `BindBorrowedMember` read (`CopyScalar`) into the return slot, then one cleanup ending the descriptor and then the backing, then `Export`.
- Each screen reads written syntax and facts already paid for before its first charge (rule 1). The record-field screen is the one charge that cannot be avoided: see Caps.

## Tests, both installed load modes

| Test | Result |
| --- | --- |
| `rfc_cases::o12_borrowed_temporary_lives_through_its_statement_and_static_view_checks` (was `o12_borrowed_temporary_is_incomplete_past_its_producer`) | `within` `Checked`, `stored` R0203 at `let view = &shifted(Point { x: 1, y: 2 });` related `&shifted(Point { x: 1, y: 2 })`, `literal` `Checked`, `shifted` unchanged; each for the targeted query and the graph root. Exact and one-short caps below. |
| `rfc_cases::o12::borrowed_temporary_ends_with_its_statement_and_static_view_has_no_owner` | The whole sequence of `within`, a never-read `&call(..)` binding, `&"static"` and a bound static view, the same for the targeted MIR and the graph root (rule 9). `within` ends the descriptor, then the temporary's fields and header, in its statement, then exports. The static view has no `end` but the named holder's. |
| `rfc_cases::o12::incomplete_borrowed_temporary_caps_now_give_resource_limit` | The old targeted caps and the old whole-graph cap give `ResourceLimit`, with no MIR and no finding (rule 11). |
| `rfc_cases::o12::admitted_forms_check_and_a_feasible_store_rejects` | Grouped receiver and borrow, grouped literal, a parameter argument, the other field, loop-wrapped return and loop-wrapped literal, never-read binding, raw and backtick literals, a bound static view, a store in a dead branch (`Checked`, no finding) and the same store in a feasible branch (R0203). Exact and one short. |
| `rfc_cases::o12::unsupported_siblings_keep_their_parent_caps` | `*(&shifted(..))`, a read through a stored view, `let x = (&shifted(..)).x`, an interpolated template, `&1i32`, a `bool` template, and the named owner (R0202) at the caps `4f7a8032` measured (rule 2). |
| `rfc_cases::o12::text_record_results_stay_incomplete` | A call result with a `string` field stays `Incomplete` at its blocker. Caps are measured on the candidate; see below. |

Red at the parent: with these test files copied into a `git worktree add` of `4f7a8032` (the two arms for operations that do not exist there removed from the sequence helper), five of the six fail. `unsupported_siblings_keep_their_parent_caps` passes there. Log: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-hxmu/parent-4f7a8032-o12.log`.

## Caps

Rule 11, the ticket's mandated flip, named in the ticket before the diff:

| Query | Old (`Incomplete`) | New | Reason |
| --- | --- | --- | --- |
| `within` targeted | 1,899 / 39 / 3 | 5,914 / 661 / 9 (`Checked`) | The call, the temporary backing, the borrow, the read and the statement cleanup are now lowered and flow-checked. Depth 9 is the argument construction under the call under the borrow under the field under the return. |
| `stored` targeted | 1,932 / 52 / 4 | 2,989 / 361 / 7 (R0203) | The temporary route and the flow check that finds the release. |
| `literal` targeted | 1,900 / 39 / 4 | 1,949 / 66 / 4 (`Checked`) | One `BorrowStatic`, no cleanup. |
| O12 whole graph | 188,807 / 4,385 / 5 `Incomplete` | 191,182 / 5,092 / 9 `SourceRejected` | D2 (b): the three newly decided roots; `stored` keeps the graph finding. |

Per-root attribution of the graph move, measured by loading the O12 module's `Point` and `shifted` with one root at a time at the candidate: the graph with `shifted` alone is 186,588 / 4,320 / 5, and adding `within` gives 188,686 / 4,709 / 9 (+2,098 work, +389 entries, depth 9), `stored` 188,513 / 4,662 / 7 (+1,925, +342), and `literal` 187,159 / 4,361 / 5 (+571, +41). Those three add to +4,594 work and +772 entries, which is exactly the whole O12 graph, 191,182 / 5,092 / 9, above `shifted` alone. `shifted` is unchanged in the row (2,810 / 253 / 3). Against the parent's `Incomplete` O12 graph the move is +2,375 work and +707 entries; the three roots were already counted there as `Incomplete`. The depth 9 of `within` sets the graph depth. Each new cap is exact and each one short gives `ResourceLimit`; the old caps and the O12 row's `pre_equality_graph`, 188,491 / 4,251 / 4, give `ResourceLimit`.

Admitted forms (new, measured on the candidate; nothing was pinned before), targeted: grouped `within` 6,358 / 881 / 11 (against `second`, the same shape ungrouped, 6,355 / 881 / 9: groups are transparent and add depth, rule 5); grouped literal 2,171 / 176 / 6 (bare `literal` 2,169 / 176 / 4); `viaParam` 6,794 / 976 / 8; loop-wrapped 6,703 / 1,084 / 11 and 2,355 / 278 / 6; never-read binding 6,285 / 845 / 7; bound static view 2,262 / 222 / 4; raw and backtick literals 2,169 / 176 / 4 and 2,170 / 176 / 4. Whole graph of that source: 206,060 / 7,452 / 11, `SourceRejected` for the one feasible store. The dead-branch store is `Checked` at 2,148 / 167 / 4, with no finding: dead code grants nothing and suppresses nothing (rule 6).

Unsupported siblings at their parent caps (targeted; all `Incomplete` at the blocker named in the test unless noted): `copyOut` 2,330 / 136 / 3, `viewField` 1,986 / 86 / 3, `bound` 1,996 / 87 / 3, `interpolated` 1,982 / 83 / 4, `number` 1,976 / 77 / 4, `flag` 1,982 / 83 / 4, `owner` 2,086 / 138 / 4 (R0202 at `return &text;`, related `text`); whole graph 192,243 / 4,594 / 5. The test source and its caps are the same at `4f7a8032` and at the candidate.

### A departure from rule 1, not avoided

A call result that is a record with a `string` field stays `Incomplete` at the same blocker as before, but it now costs 3 more work: the screen has to read the record's fields (one charge of 1 and one of the field count) to see that it is not a flat record of scalars, and no paid fact carries the field list. `textRead` was 1,873 / 26 / 3 at `4f7a8032` and is 1,876 / 26 / 3; `textBind` was 1,889 / 33 / 4 and is 1,892 / 33 / 4; their whole graph was 187,421 / 4,277 / 5 and is 187,427 / 4,277 / 5. No published or pinned cap contains such a shape: every fixed cap held (below). `text_record_results_stay_incomplete` pins the candidate's numbers, not the parent's.

Ruling (chief of staff, 2026-11-24 06:50 AEDT, `chief-rulings-4.md` item 2): rule-1 exception GRANTED for these two siblings, on the terms of the O7 items. Short-circuiting to the same `Pending` blocker was tried and cannot match the parent exactly: it gave 1,873 / 26 / 2 and 1,888 / 33 / 4 (graph 187,420), and adding the depth charge back gave 1,874 / 26 / 3. No paid fact carries the record's field list, so an earlier screen needs a new M4 fact; the site is recorded in `ri-qlh.4` (the text-record screen in `ownership/build.rs` that reads the record's field list). `text_record_results_stay_incomplete` pins the new caps exact and one short (1,876 / 26 / 3 and 1,892 / 33 / 4; graph 187,427 / 4,277 / 5), and asserts in both load modes that each parent cap (1,873 / 26 / 3; 1,889 / 33 / 4; graph 187,421 / 4,277 / 5) gives `ResourceLimit` with no MIR and no finding. Both roots are named by test in the dated rule-1 exception note in `docs/agents/compiler-metering.md`.

Fixed caps unchanged, checked locally after merging the base tip `1fb84233`: `m5::legacy_caps`, `m5::graph::inventory`, every other `rfc_cases` row, `field_routes`, `m4_8_scoped_join_caps` and `m4_8_match_boolean_snapshots`. Typed statistics are unchanged for every source (no M4 change), so the M4 17-source baseline holds.

## Not established

- Only a flat record of scalars is admitted as the borrowed temporary, with a scalar field read. A record with a text field or a guard, a computed receiver other than a call, `*(&call(..))`, a read through a stored view, and `&mut` of a temporary stay `Incomplete`.
- Only a string literal or a text-only template gets the static view. `&1i32` and other primitives stay `Incomplete`.
- RFC 0007's reading that a stored reference "is invalid when the temporary would end" is implemented as R0203 at the binding. RFC 0007 names no diagnostic code for this case. R0203 ("Required source released", `docs/diagnostic-codes.md`) was chosen by the chief of staff's ruling of 2026-11-24 (`chief-rulings-4.md` item 3), R0202 ("Escaping dependency") being kept for callee exit checks; the existing `field_routes::later_use_requires_a_named_scalar_owner` uses R0203 for the same shape with a `bool` temporary. The chief reports the choice to the owner, since the RFC text is the owner's.
- The result is a compiler judgment over MIR. No Rill program ran.

Dated pointer (2026-11-24, ri-1up): the `viewField` root of `unsupported_siblings_keep_their_parent_caps` (a read through a stored view of a borrowed temporary) left that test when `ri-1up` admitted a scalar field read through `&Record`. It is now R0203 and is pinned by `rfc_cases::o12::read_through_a_stored_view_of_a_temporary_rejects_its_release` (chief of staff ruling of 07:23 AEDT). The siblings test was re-measured at `bc9bfdd5` without it; see [`ri-1up.md`](ri-1up.md).
