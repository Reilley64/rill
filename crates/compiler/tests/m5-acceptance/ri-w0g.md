# Interpolation over a guard owner, RFC 0007 O20 (ri-w0g, 2026-11-23)

This is ticket ri-w0g, the K3/O20 row that the owner brought into M5 on 2026-11-23 (owner decision through the chief of staff, recorded on city bead `ci-6ok`; review `ci-uaa1` SP-F2). The O20 module is `type Outer = { lock: LockGuard<i32>; value: i32; }` and ``function render(outer: Outer): string { return `${outer.value}-${outer.value}`; }``. At the parent, `7002855e`, `render` was `Incomplete` at the template, for the targeted query and the graph root. The ownership builder had no lowering for a template.

## What changed

A template is now lowered when every hole is an `i32` formatted by the selected primitive Show, and each hole, after its groups, is an owned literal, an owned local or a field of an owned local (`build/interpolation.rs`). Each hole is read in written order into its own scratch slot. A new MIR operation, `FormatI32Template { holes, into }`, ends every scratch slot and initializes the owned string. No other owner ends at the template, so the direct reads end before the next hole and the guard stays with `outer` until scope exit. The screen reads only facts already paid for. Its first charge comes after the last early return (rule 1), so other holes stop at the template at their old cost.

## Tests, both installed load modes

| Test | Result |
| --- | --- |
| `rfc_cases::o20_interpolation_reads_through_the_guard_owner_and_checks` (was `o20_transitive_guard_cleanup_is_incomplete_at_interpolation`) | `render` `Checked`, targeted query and graph root. Exact and one-short caps below. |
| `rfc_cases::o20::interpolation_reads_end_before_the_guard_is_released_once` | The whole straight path, the same for the targeted MIR and the graph root (rule 9): copy `outer.value` twice into scratch, format both, export the result, then end `outer.value`, release `outer.lock` once, end `outer`. The root's one edge is `ProtectedRelease`. |
| `rfc_cases::o20::incomplete_interpolation_caps_now_give_resource_limit` | The old caps give `ResourceLimit`, with no MIR and no finding (rule 11). |
| `rfc_cases::o20::admitted_interpolation_forms_check` | Grouped holes, a grouped template, a bound result, an expression statement, a loop-wrapped return and a local with a literal hole: each `Checked`, exact and one short. |
| `rfc_cases::o20::unsupported_holes_keep_their_parent_caps` | Bool, string, i32 arithmetic, call and shared-view field holes stay `Incomplete` at the template, at the caps measured at `7002855e` (rule 2). |

Red at the parent: the four admission tests above fail at `7002855e` with these test files copied into a `git worktree add` of the parent (the sequence helper's `FormatI32Template` arm removed, since the operation does not exist there). `unsupported_holes_keep_their_parent_caps` passes there in both modes. Log: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-mqsm/parent-7002855e-o20.log`.

## Caps

Rule 11, the ticket's mandated flip, named in the ticket before the diff:

| Query | Old (`Incomplete` at the template) | New (`Checked`) | Reason |
| --- | --- | --- | --- |
| `render` targeted | 2,230 / 81 / 3 | 2,499 / 215 / 4 | The two reads, the formatting and the cleanup are now lowered and flow-checked. Depth 4 is the hole expression under the template under the return. |
| O20 whole graph | 186,364 / 4,176 / 5 | 186,732 / 4,311 / 5 | D2 (b): the newly proved root `render`. |

Per-root attribution of the graph move: `render` is the only root that changes, and it accounts for all of the +368 work and +135 entries. The unsupported siblings load the same standard roots, and their whole-graph caps are unchanged from the parent. The +368 work is `render`'s own lowering, +269 (the targeted move), plus 99 for the scope-exit release verifier of ri-px1.8 on a guard field: 3 cleanup actions, 1 parameter, 8 protection scopes, 10 for the `core.rill` family key, 4 native representations, 70 for the `main.rill` record-header probe and 3 record fields. Depth stays 5, set by the i32 equality source root (ri-785.9). The row's `pre_equality_graph`, 186,048 / 4,042 / 3, still gives `ResourceLimit`. Each new cap is exact; each one-short cap gives `ResourceLimit`, and so do both old caps.

Admitted forms (new, measured on the candidate; nothing was pinned before):

| Form | Targeted | Graph |
| --- | --- | --- |
| grouped holes `${(outer.value)}-${((outer.value))}` | 2,511 / 218 / 6 | 186,924 / 4,314 / 6 |
| grouped template `` (`${outer.value}-${outer.value}`) `` | 2,504 / 216 / 5 | 186,797 / 4,312 / 5 |
| bound ``let text = `${outer.value}`; return text;`` | 2,552 / 245 / 4 | 186,786 / 4,341 / 5 |
| statement `` `${outer.value}`; `` (the string is dropped where the statement ends) | 2,417 / 175 / 4 | 186,530 / 4,271 / 5 |
| loop-wrapped ``while (again) { return `${outer.value}`; }`` | 2,680 / 320 / 6 | 187,156 / 4,416 / 6 |
| `` `${value}-${1i32}` `` over an `i32` parameter | 2,050 / 122 / 4 | 185,879 / 4,217 / 5 |

Groups are transparent (rule 5): the grouped template is one level deeper than the bare O20 form (5 against 4), the grouped holes deeper again (6), and one short of each depth gives `ResourceLimit`.

Unsupported siblings at their parent caps (targeted / graph; all `Incomplete` at the template): bool field 2,226 / 79 / 3 and 186,240 / 4,174 / 5; string field 2,226 / 79 / 3 and 186,240 / 4,174 / 5; `outer.value + outer.value` 2,232 / 82 / 3 and 186,426 / 4,177 / 5; `twice(outer.value)` 2,240 / 86 / 3 (with `twice` `Checked` at 1,987 / 83 / 3) and 186,950 / 4,251 / 5; a field of `outer: &Outer` 1,857 / 21 / 3 and 185,931 / 4,116 / 5.

Fixed caps unchanged, checked locally: `m5::legacy_caps`, `m5::graph::inventory` (its 17- and 18-source roots hold `${values[0u64]}`, an index hole that the screen rejects before its first charge), `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`, `m4_8_ownership` and `m4_8_callable_once`. Every other `rfc_cases` row is unchanged.

## Not established

- O10, O12 and O22, which this bead also names, are not covered by this change. They need a `List<&T>` storage route and family selection (O10), statement-local borrowed temporaries (O12) and a descriptor disposal family (O22); none of these is a template lowering. They were reported to the planner for the chief of staff, and their rows in `m5-status.md` are unchanged.
- Only `i32` holes with the primitive Show are admitted. Other primitive, text, user-Show and generic holes, and computed hole expressions, stay `Incomplete` at the template.
- The formatting is a compiler judgment over MIR. No Rill program ran, and the text the template produces is not checked.
