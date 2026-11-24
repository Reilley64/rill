# O9: nested storage under rule R1 (ri-ema, city bead ci-xjff, 2026-11-24)

This is ticket R-O9. After this ticket RFC 0007 O9 reaches its RFC outcome: `forwarded` is `Checked` with MIR and `local` is `SourceRejected` with R0202, as a targeted query and as an ordinary graph root, in both installed load modes. Rule R1 is the owner's amendment of 2026-11-23 (RFC 0007, calls section; ri-2s0.12). Parent: `b6c93f30` (the base tip that contains ri-1up).

## Plan (written before the build)

One session; no child tickets. R1 is one more retained-contents kind, not a parallel path:

1. **Contents screen** (`analysis.rs`, `ownership.rs`): `&mut List<&R>`, `R` a non-generic application nominal, holds retained contents `&R` (the List's element type, not the referent `List<&R>`). The type screen is free (it needs the List role, so it sits in `retained_contents_of`, which has the typed report). The record-of-`i32`-fields lookup is the same charged lookup as for `&mut &Record` (rule 3), after the free screen (rule 1). It is split out of `retained_record_contents` and shared; the typed passes keep calling `retained_record_contents` and are unchanged.
2. With the contents known, `lower()` registers `RetainedContents::OneHop` for a `&mut List<&R>` parameter: the callee exit check (R0202 for a body-local source), the destination route `CallStorage::ThroughParameter` and the loan transfer into the destination's contents reuse the `&mut &Record` machinery.
3. **Source admission**: `record_destination` in `call` also admits a `&R` argument beside a list destination.
4. **Family routing**: O9 calls through a family. `family_retaining_shape` accepts a destination written as a plain name of a retained parameter when its written type is the list candidate, and decides the contract from the element type alone (unmetered). The `&mut &Point` family form stays as it was (ri-1up: "stays Incomplete at its call").
5. **Tests first** (`rfc_cases.rs`, `rfc_cases/list_contents.rs`): the O9 row renamed; the old root and graph caps give `ResourceLimit`; R1's known conservative rejection; direct-function form; siblings that keep their outcomes.

## What changed

- `analysis.rs`: `retained_i32_record` is the record-of-`i32`-fields lookup of `retained_record_contents`, split out so that a list element shares it (same charges, same order; the typed passes are unchanged).
- `ownership.rs`: `retained_list_candidate` (free: `&mut List<&R>`, `R` a non-generic application nominal; the contents are the element `&R`) and `retained_destination_contents` (`retained_contents_of` plus the list). `lower()` registers the list parameter's contents (`RetainedContents::OneHop`, as for `&mut &Record`), so the callee exit check, `CallStorage::ThroughParameter` and the loan transfer are the existing ones. The screen that looks for a retained parameter by its descriptor contents (`lower()`'s first loop) keeps `retained_contents_of`: it can never name a list, and a list parameter pays its lookup once.
- `ownership/build.rs`: `retained_contents` uses `retained_destination_contents`; `record_destination` admits a `&R` argument beside a list destination; `family_retaining_shape` accepts the plain name of a parameter whose written type is the list candidate (a written `&mut name` keeps its `&bool`/`&string` screen, and `&mut &Record` family calls keep their old path); a family call whose list element is not a record of `i32` fields stops at the call node, where it stopped before.
- Tests: `tests/m5/rfc_cases.rs` (the O9 row, renamed; `o09_pre_list_contents_caps_now_give_resource_limit`), `rfc_cases/list_contents.rs` (new), `rfc_cases/record_contents.rs` (`checked`, `incomplete`, `rejected` made `pub(super)`), `m5-status.md`, `docs/agents/compiler-metering.md` (the rule-1 note).

## The O9 flip (mandated, rule 11 third case; whole graph D2 (b))

Named by the ticket. Parent `b6c93f30`, typed unchanged at 329,382 / 48,563 / 12 (`Checked`).

| | Parent | Now |
| --- | --- | --- |
| `forwarded` | `Incomplete` at `retain(dst, source)`, 1,930 / 55 / 3 | `Checked` with MIR, 3,330 / 243 / 5 |
| `local` | `Incomplete` at `retain(dst, &user)`, 2,292 / 118 / 3 | `SourceRejected`, R0202 at `retain(dst, &user)`, related `&user`, 3,726 / 332 / 6 |
| whole graph | `Incomplete`, 189,463 / 4,237 / 5 | `SourceRejected`, 192,306 / 4,641 / 6 |

Per-root attribution of the graph move: `forwarded` +1,400 work, +188 entries; `local` +1,434 work, +214 entries; the sum is +2,834 work and +402 entries, and the graph moved by +2,843 and +404. The remaining +9 work and +2 entries are not attributed to a root; the targeted queries are the per-root figures. Depth 6 is `local` (the exit check). The old caps (each root and the graph) give `ResourceLimit` (`rfc_cases::o09_pre_list_contents_caps_now_give_resource_limit`). `pre_equality_graph` of the row is unchanged and still gives `ResourceLimit`. Both modes, targeted query and ordinary graph root, exact and one short on each axis.

## Other tests (`rfc_cases::list_contents`), both modes, exact and one short, old caps give `ResourceLimit`

| Test | Result |
| --- | --- |
| `r1_known_conservative_rejection_of_a_callee_that_stores_nothing` | The chief's condition. An `implement retain(dst: &mut List<&User>, source: &User): void { }` stores nothing; the caller `local` is valid, and R1 rejects it, R0202 at `retain(dst, &user)`, related `&user`, 3,696 / 317 / 6 (parent `Incomplete` 2,262 / 103 / 3; graph 188,363 / 4,204 / 5 to `SourceRejected` 189,799 / 4,419 / 6). It is the witness for R2 (an explicit storage contract). |
| `direct_list_destination_checks_parameters_and_rejects_locals` | The direct function form (no family): `retain` (empty body), `forwarded`, `twice` and `keep` `Checked`; `local` R0202. At the parent `retain` and `keep` were `Checked` (2,066 / 121 / 1; 2,584 / 260 / 4) and `forwarded`, `local`, `twice` `Incomplete` at `dst`: the parameter now has retained contents (a slot, one lookup). |
| `family_list_destination_forms` | Two calls, a call in a loop, a pair of sources, a lone destination and a source read after the call `Checked`; a grouped destination `(dst)` stays `Incomplete` at its call. |
| `local_sources_escape_through_every_route_to_the_list` | Soundness. A body-local owner stored through an alias, a local view, a branch, a later call, or forwarded by a callee is R0202 at the call; in a loop body that ends first it is R0203; in dead code it checks and rejects nothing (rule 6); forwarding a parameter checks. |
| `list_destination_siblings_keep_their_outcomes_and_blockers` | Rule 1. See "Rule-1 exception". |

## Rule-1 exception (the standing ruling of 2026-11-24 07:19, amended 11:35 AEDT)

The list candidate is a free type screen (the List role, an element `&` of an application nominal with no type arguments). The record's field list needs a charged lookup and no paid fact carries it. A sibling that passes the screen and fails the lookup keeps its outcome and its blocker site (a family call stops at the call node; a direct call at `dst`, as before). Each pays two lookups: the caller's own parameter and the call. Work, parent `b6c93f30` to now:

| Sibling (as `retain(dst: &mut List<&T>, source: &T)` forwarded) | Parent | Now | Blocker (unchanged) |
| --- | --- | --- | --- |
| `T` with a `string` field | 1,904 / 42 / 3 (graph 188,126 / 4,143 / 5) | 2,052 / 45 / 3 (+148; graph 188,274 / 4,146 / 5) | `retain(dst, source)` |
| `T` with a nested record | 1,906 / 43 / 3 (graph 188,192 / 4,144 / 5) | 2,054 / 46 / 3 (+148; graph 188,340 / 4,147 / 5) | the same |
| `T` an enum | 1,900 / 40 / 3 (graph 188,002 / 4,141 / 5) | 2,044 / 43 / 3 (+144; graph 188,146 / 4,144 / 5) | the same |
| an extra `bool` beside the source | 1,917 / 49 / 3 (graph 188,322 / 4,150 / 5) | 1,991 / 52 / 3 (+74, the parameter's lookup only; graph 188,396 / 4,153 / 5) | `retain(dst, source, flag)` |
| generic `Box<i32>`, owned `List<User>`, `List<&mut User>`, shared `&List<&User>`, a caller-owned list `&mut users` | unchanged | unchanged | the same |

All four are under 150 work and at most two lookups, and the parent caps give `ResourceLimit`. They are listed by test name in the rule-1 note of `docs/agents/compiler-metering.md`. The siblings never reach a different outcome, so (a) to (d) hold.

`lower()`'s first loop (which looks for a retained parameter to read `exclusive_source` from) keeps `retained_contents_of`, which never names a list. A list is never a destination that loop could use (its sources are never exclusive), so it would only pay a third lookup.

## Outcomes beyond the two mandated roots (for the chief of staff)

New forms of the same call that were `Incomplete` and now check, as a consequence of R1 and of registering the list parameter's contents, with no pinned expectation behind them: a direct `retain(dst, source)`, two calls, a call in a loop, a pair of `&User` sources (`retain(dst, a, b)`), a lone destination (`retain(dst)`, "the contents keep every source they already held", as `&mut &bool`) and a source read after the call. None is accepted unsoundly: the escape tests above reject every body-local source. `keep`-like roots that already checked (a list parameter and no call) moved: 2,584 / 260 / 4 to 2,760 / 305 / 4 (one lookup, a contents slot). There is no other pinned test with a `&mut List<&R>` parameter (`rg "mut List<&"`).

## Fixed caps

Unchanged: `m5::legacy_caps`, `m5::graph::inventory`, every `retained`, `records`, `field_routes`, `list` and `scoped_join` test, `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`, `m4_8_ownership` (the M4 17-source baseline) and `m4_8_result_use`. The full M5 binary runs only in hosted CI.

## Metering rules

- Rule 1: the list candidate reads the type alone; the one lookup is the record screen's (above).
- Rule 3: `retained_i32_record` charges `(path length + 1) x probes` before `.get`, as before.
- Rule 11: O9 (mandated). Whole graph D2 (b), attribution above.

## Not established

- Only a `&mut List<&R>` destination that is a parameter name (a plain `dst`) and a flat non-generic record of `i32` fields. A caller-owned list (`&mut users`), a grouped destination, another container, `&mut List<&mut R>`, an owned element and a generic or non-`i32` record stay `Incomplete`.
- No callee exit check for a store through the list itself: Rill has no list store operation in these tests (the implementation body `while (true) {}` stores nothing). The exit check is established for a body-local source reaching `dst` through a call (R0202), as for `&mut &Record`.
- R1 reads the declared signature only (never the implementation's body), so it is conservative (see its witness above).
- RFC 0007's conformance table (line 542, "NotChecked. No compiler evidence exists for O9 yet.") is the owner's text and is not edited here.
- This is compiler MIR evidence; no Rill program ran. Hosted CI did not run for this branch.

## Logs

`~/.t3/evidence/rill/m5-build-2026-11-22/ci-xjff/`: `parent-probe.txt` and `candidate-probe.txt` (the measurements, sources in `probe-sources*/`), `m5-targeted.log`, `m5-extra.log`, `m4_8.log`, `m4_8_ownership.log`, `clippy.log`, `fmt.log`.
