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

Per-root attribution of the graph move (D2 (b)), measured from the graph, not from the targeted queries, with the module cut down to the roots named (`attr-parent.txt`, `attr-candidate.txt` in the logs; the parent is a `git worktree add` at `b6c93f30` with its own target directory, removed after). The base module (`type User`, the family declaration and `implement retain`, no other root) is 187,149 / 4,112 / 5 at the parent and now: the implementation root moves nothing. A root's share is the graph with it less the graph without it.

| Root | Parent | Now | Move |
| --- | --- | --- | --- |
| `forwarded` | 914 / 30 | 2,321 / 219 | +1,407 work, +189 entries |
| `local` | 1,400 / 95 | 2,836 / 310 | +1,436 work, +215 entries |
| sum | 2,314 / 125 | 5,157 / 529 | +2,843 work, +404 entries |

The sum is the move of the whole graph, 189,463 / 4,237 / 5 to 192,306 / 4,641 / 6, exactly. Depth is a maximum: `local` sets the new depth 6 (the module with only `forwarded` is depth 5; with only `local`, 6). The old caps (each root and the graph) give `ResourceLimit` (`rfc_cases::o09_pre_list_contents_caps_now_give_resource_limit`). `pre_equality_graph` of the row is unchanged and still gives `ResourceLimit`. Both modes, targeted query and ordinary graph root, exact and one short on each axis.

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

All four are under 150 work and at most two lookups, and the parent caps (each root's and the graph's, both asserted by `old_root_caps`) give `ResourceLimit`. They are listed by test name in the rule-1 note of `docs/agents/compiler-metering.md`. The siblings never reach a different outcome, so (a) to (d) hold.

`lower()`'s first loop (which looks for a retained parameter to read `exclusive_source` from) keeps `retained_contents_of`, which never names a list. A list is never a destination that loop could use (its sources are never exclusive), so it would only pay a third lookup.

## Outcomes beyond the two mandated roots (for the chief of staff)

New forms of the same call that were `Incomplete` and now check, as a consequence of R1 and of registering the list parameter's contents, with no pinned expectation behind them: a direct `retain(dst, source)`, two calls, a call in a loop, a pair of `&User` sources (`retain(dst, a, b)`), a lone destination (`retain(dst)`, "the contents keep every source they already held", as `&mut &bool`) and a source read after the call. None is accepted unsoundly: the escape tests above reject every body-local source. `keep`-like roots that already checked (a list parameter and no call) moved: 2,584 / 260 / 4 to 2,760 / 305 / 4 (one lookup, a contents slot). There is no other pinned test with a `&mut List<&R>` parameter (`rg "mut List<&"`).

## The chief of staff's four conditions (ci-ue8s, ruling of 2026-11-24 12:35 AEDT)

The chief accepted the extra forms R1 admits (below) with four conditions. All tests are in `rfc_cases::list_contents`, both installed load modes, the targeted query and the ordinary graph root, exact and one short on each axis (`check_cases`); the sequence tests also compare the whole MIR of the targeted query and of the graph root's ordinary MIR, block by block, against one list (`pin_form`, `sequence`; rule 9). No cap moved: these are new pins of the built branch (no source change), and the fixed caps are unchanged.

**Condition 1: every newly admitted form is pinned by name, with its MIR.** Caps are work / entries / depth of the root; each module's typed and whole-graph caps are in the test.

| Form | Test | Root caps |
| --- | --- | --- |
| direct function `retain(dst, source)` | `r1_direct_function_retain_checks_with_its_sequence` | `retain` 2,102 / 102 / 1, `forwarded` 4,527 / 292 / 5 |
| two calls | `r1_two_calls_check_with_their_sequence` | `twice` 4,683 / 448 / 5 |
| a call in a loop | `r1_call_in_a_loop_checks_with_its_sequence` | `looped` 3,476 / 320 / 7 |
| a pair of `&User` sources | `r1_pair_of_user_sources_checks_with_its_sequence` | `both` 3,490 / 312 / 5 |
| the lone destination `retain(dst)` | `r1_lone_destination_checks_with_its_sequence` | `forwarded` 3,133 / 156 / 5 |
| a source read after the call | `r1_source_read_after_the_call_checks_with_its_sequence` | `reuse` 3,457 / 306 / 5 |
| `keep`: a `&mut List<&User>` parameter, no call | `r1_keep_list_parameter_without_a_call_is_pinned` | `keep` 2,760 / 305 / 4 (module of `direct_list_destination_checks_parameters_and_rejects_locals`; `local` there rejects R0202, 3,011 / 360 / 6) |

The sequences read as follows. A call is a mutable reborrow of `dst`, a copy of the source into a stage, then one retention into the destination's contents (`retain through dst <- source`); two calls repeat that for each source; the pair stages both sources before one retention; the lone destination reborrows and retains `nothing`; the read after the call is `deref source` then `export source.id` after the retention, with `source` ending only at the function's exit; the loop's last retention jumps back to the loop head and the MIR has no return. Nothing is retained for `keep`: it only borrows its own local.

**Condition 2: the loop form's rejecting twin** (`r1_loop_body_local_source_retained_into_dst_rejects`, module typed 332,728 / 49,168 / 12, graph `SourceRejected` 196,274 / 5,223 / 8). `looped` (a parameter source) checks, 3,546 / 355 / 7. A body-local `User` created in the loop and retained into `dst` rejects, and the code depends on how the body ends:

| Root | Result | Caps |
| --- | --- | --- |
| `loopLocalReturn` (`return` after the call, inside the loop) | R0202 at `retain(dst, &user)`, related `&user` | 3,936 / 441 / 8 |
| `loopLocal` (the body ends and the loop goes round) | R0203 at the loop body block, related `&user` | 3,960 / 444 / 8 |

The ruling names R0202. The loop that goes round is R0203 (the scope's end releases a loan the destination still needs, as for `early` in `stored_source_loans_reach_the_caller_view`), which is also what `local_sources_escape_through_every_route_to_the_list`'s `loopLocal` has pinned since the build. Both reject; the R0202 form is the `return` variant. The chief should confirm that R0203 for the round-going loop is the wanted code.

**Condition 3: the stored source outlives the call loan** (`r1_stored_source_outlives_the_call_loan`, module typed 336,144 / 49,622 / 12, graph `SourceRejected` 198,597 / 5,538 / 6). After `retain(dst, &user)` the call's own loan is over; `dst` still holds the source until the function ends.

| Root | Result | Caps |
| --- | --- | --- |
| `writeAfter`: `retain(dst, &user); user.id = 2;` | R0203 at `user.id = 2;`, related `&user`: the write needs the source exclusively while `dst` still holds it | 3,924 / 431 / 6 |
| `readAfter`: `retain(dst, &user); let n = user.id;` | R0202 at the call: a read does not conflict, only the escape rejects | 3,909 / 422 / 6 |
| `writeBefore`: `user.id = 2; retain(dst, &user);` | R0202 at the call: no loan exists at the write | 4,006 / 472 / 6 |
| `viewReadThenWrite`: `let view = &user; let n = view.id; user.id = 2;` | `Checked`: an ordinary view's loan ends at its last use, so the same write is fine | 2,761 / 346 / 4 |

`writeAfter` against `writeBefore` and `viewReadThenWrite` is the judgment: the write rejects only when a retention precedes it, so the retained source outlived the call.

Stated limit (also in the O9 row of `m5-status.md`): this judgment is reached through a body-local owner only, whose store R0202 rejects anyway. A use that would need a caller-provided source released or exclusively borrowed while `dst` is live is not expressible today: a `&mut User` source is typed `Incomplete` (probe: `retain(dst, &*source)` and `retain(dst, source)` with `source: &mut User` stop typed), `*source` stops the ownership pass at `*source`, and a caller-owned list (`retain(&mut users, source)`) is `Incomplete` at `users` (`list_destination_siblings_keep_their_outcomes_and_blockers`). The `&mut User` source is pinned by `r1_caller_provided_exclusive_source_stops_typed` (typed `Incomplete` at 326,581 / 48,099 / 12 for `retain(dst, &*source)` and 326,114 / 47,888 / 12 for `retain(dst, source)`, no ownership receipt, one short `ResourceLimit`; the typed report exposes no blocker site, so none is asserted). The `*source` and caller-owned list stops are pinned or observed as described in condition 4.

**Condition 4: the admitted forms and the forms that stay `Incomplete`.**

Admitted (`Checked` with MIR, or `SourceRejected` for a body-local source) for a `&mut List<&R>` destination that is a plain parameter name, `R` a non-generic record of `i32` fields:

- the O9 forwarded call through a family (`o09_retained_list_source_checks_forwarded_and_rejects_local`) and its direct-function form;
- two calls, a call in a loop, a pair of `&R` sources, the lone destination `retain(dst)`, a source read after the call, a call through a callee that forwards the parameter (`forward`);
- a `&mut List<&R>` parameter with no call (`keep`, whose caps moved 2,584 / 260 / 4 to 2,760 / 305 / 4);
- every body-local source, rejected: R0202 at the call (directly, through an alias, a local view, a branch, a later call, a forwarding callee, and a loop that returns) and R0203 for a loop body or branch that ends first; the same store in dead code rejects nothing (rule 6);
- an implementation that stores nothing is rejected the same way (the known conservative rejection, R2's witness).

Stay `Incomplete` at their blocker (pinned in `list_destination_siblings_keep_their_outcomes_and_blockers` and `family_list_destination_forms`): a caller-owned list (`retain(&mut users, source)`, at `users`), a grouped destination `retain((dst), source)` (at the call), an element that is a generic record, an owned `List<User>`, a `List<&mut User>`, a shared `&List<&User>` destination, a record with a `string` field, a nested record or an enum, and a call with an extra `bool` argument. A `&mut User` source stops the typed pass (`r1_caller_provided_exclusive_source_stops_typed`). Observed by probe and not pinned: a whole-record copy `*source` (ownership `Incomplete` at `*source`).

**For the second review:** probe R1's admissions for a wrong acceptance. R1 reads the declared signature only and assumes a `&R` input can flow into `dst`'s elements, so it errs towards rejecting; the danger is a form that checks and should reject. The forms to try first are the ones above that check: a source whose owner ends or is mutated while `dst` is live, a `&R` source that is itself derived from a local (a view of a local, a call result), a destination reborrowed or aliased across a branch or loop, and a call in a loop whose source is created in the body. None has been found to check; every body-local source above rejects.

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

`~/.t3/evidence/rill/m5-build-2026-11-22/ci-ue8s/` holds the logs of the conditions work (ci-ue8s). `~/.t3/evidence/rill/m5-build-2026-11-22/ci-xjff/`: `parent-probe.txt` and `candidate-probe.txt` (the measurements, sources in `probe-sources*/`), `m5-targeted.log`, `m5-extra.log`, `m4_8.log`, `m4_8_ownership.log`, `clippy.log`, `fmt.log`.
