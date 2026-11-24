# Final record and PR statement (ri-px1.11, D6, 2026-11-23)

Ticket D6 (gap-analysis row D6), umbrella ri-px1. This ticket changes no compiler source, test or cap. Under the planner's decision of 2026-11-23 on `ri-2s0` it writes the record; the full gates run in hosted CI only, and the paired Standards and Spec reviews follow as `implement-spec` step 7.

## What changed

- [`m5-acceptance.md`](../m5-acceptance.md): "Open acceptance obligations" is replaced by "Final record: deferred obligations (2026-11-23, ri-px1.11)". It lists every §1.6 row with its bead, the ticket-index deferrals, the found beads still open (state checked 2026-11-23) and points to the three closeout audits.
- [`m5-status.md`](../m5-status.md): owners that named closed tickets now name the bead that owns the gap. O14: `ri-4lq` (the graph root of a written `dispose(value.field)`), O18: none (D2 closed), O20: `ri-w0g` only (D3 closed), O17 and O23: none (B5 audit closed).
- Body of draft PR #73: a section states that the M4 Result-use work stays in this PR (owner decision D6 (a)). The PR stays draft.

## Superseded record line

D4's file [`ri-px1.9.md`](ri-px1.9.md) says `ordinary_only_list_loops_keep_the_pre_delivery_caps` "still holds at 191,781 / 4,867 / 6". B6 (`ri-785.9`) re-pinned that whole-graph cap to 192,097 / 5,001 / 6 ([`ri-785.9.md`](ri-785.9.md), rows `fixed-list` and `cancellation.rs` ordinary list loops), each old cap asserting `ResourceLimit`. The D4 line is superseded by the B6 re-pin; D4's file is not edited. D5's audit ([`ri-px1.10.md`](ri-px1.10.md)) already names B6's re-pin.

## Not established

- The hosted checks on the final head and the paired reviews are not part of this record; they follow in `implement-spec` step 7. `ri-px1.11` stays open until then.
- No Rill program ran.

## Review ci-uaa1 fixes (2026-11-23, city bead ci-paai)

This section answers review `ci-uaa1` of `7002855e` under the owner's decisions of 2026-11-23 on `ri-2s0` (O1 to O8), the chief of staff's conditions (`rill/staff.chief-of-staff`, same date) and the owner's later override of O4 and O5: the Required rows of O4 and C12 become M5 tickets, so this section defers none of them. Branch `m5-ownership-cleanup/review-fix-ri-px1.11`. All results are LLVM-free host ownership and typed analysis in both installed load modes. No Rill program ran.

### Metering standard (O1, O2, R-1, S2-F6)

[`compiler-metering.md`](../../../../docs/agents/compiler-metering.md) rule 11 gains the third case, "A ticket's mandated flip or charge", with the chief's four checks, and names the chief of staff or the owner as the authority for any other cap movement. It names the three moves the owner ratified: A2's O15 `counter` (2,504/329/6 to 2,562/329/6, graph 187,485 to 187,543), B6's O15 expectation (`Incomplete` 1,893/32/1 to `Checked` 4,577/425/6, with O3 and O5) and C1's `callerExclusive` (`Incomplete` 3,089/284/4 to `Checked` 9,466/1,677/4). The D3 (b) example now cites `e022c80d` and its old-cap test. Rule 3 has a dated note: the existing path-keyed probes are known non-conforming and deferred to `ri-qlh.3`; new or touched probes must conform.

### Base-matrix rows (O3, ri-px1.1 #1)

The ten `base-matrix.tsv` queries are active rows of [`legacy-caps.tsv`](../m5/legacy-caps.tsv) in both modes, at today's values. Nine moved since `68f3788`; `infeasibleOuter` reproduces its `68f3788` cap 10,495/2,458/6 (the H1 table's 10,887/2,609/6 no longer holds). [`legacy_caps::base_matrix_caps_give_resource_limit`](../m5/legacy_caps.rs) asserts that each `68f3788` cap gives `ResourceLimit` with no MIR (and no finding for a root).

| Fixture and query | `68f3788` | Today |
| --- | --- | --- |
| `retaining-rc` whole graph | `SourceRejected` 246,818 / 14,458 / 11 | `SourceRejected` 247,377 / 14,656 / 11 |
| `retaining-rc` `nestedGoodWithOwner` | `Checked` 12,151 / 3,338 / 6 | `Checked` 12,838 / 3,591 / 6 |
| `retaining-rc` `nestedCaller` | `Checked` 10,348 / 2,397 / 6, root `Checked/Checked` | `SourceRejected` 10,472 / 2,446 / 6, root `SourceRejected/Checked` |
| `safe-rc` whole graph | `Incomplete` 199,807 / 6,904 / 11 | `Incomplete` 200,280 / 7,079 / 11 |
| `safe-rc` `nestedGoodWithOwner` | `Checked` 9,679 / 2,102 / 6 | `Checked` 10,366 / 2,355 / 6 |
| `foreign-rc` whole graph | `SourceRejected` 193,281 / 5,176 / 6 | `SourceRejected` 193,683 / 5,333 / 6 |
| `foreign-rc` `rejected` | `Checked` 7,463 / 895 / 6, root `Checked/Checked` | `SourceRejected` 7,587 / 944 / 6, root `SourceRejected/Checked` |

The whole-graph rows include the i32 equality source root (ri-785.9, rule 11 D2 (b)). The owner decided that these are the intended cost of the forwarding proof.

The two flips are correct rejections, not false ones (chief's O3 condition). `nestedCaller` calls `bad`, and `rejected` calls `left::bad`; each `bad` calls `unsafeBody`, which stores `&flag` through `slot` and then loops on `enter`. RFC 0007 applies exit checks to cancellation exits: "A planned later overwrite cannot excuse invalid outward state at a possible delivery point." The manifest already published `unsafeBody` and `bad` as `SourceRejected`. Each root now reports that callee's witness, `R0202` at `*slot = &flag;`, related `&flag`; the test asserts it. `foreign-rc`'s function is named `rejected`.

### Manifest rows for D3's graphs (ri-px1.8 #1)

New fixtures `d3-implicit-guard` and `d1-mixed-wrapper` copy the sources of `graph::closed_release::implicit_guard_release_closes_the_ordinary_root` and `disposers::mixed_wrapper`. Their graph rows hold 185,562 / 4,127 / 5 and 198,520 / 5,684 / 5 (unchanged from those tests), with the D3 roots `Checked/Checked`. The typed rows are first pins: 322,181 / 47,506 / 12 and 342,614 / 49,553 / 12. `legacy_caps` now has 36 fixtures and 300 cap triples (276 before this section).

### Tests and records with no owner decision

| Finding | Change | Test |
| --- | --- | --- |
| ri-px1.5 #1 | `installed()` fails unless the child ran exactly one test. Before the change, every test-name literal in `tests/m5.rs` and `tests/m5/` resolved against the binary's `--list`. | `m5::installed_fails_when_the_child_runs_no_test` (should panic) |
| ri-px1.10 #1b, SP-F8 | `ordered` and `reinitialized` compare the whole cleanup at exact caps 3,011/590/4 and 3,106/648/4, one short on each axis. `reinitialized` drops `owner` twice (the replaced value, then the live one). The rank witness `let mut a = "a"; let b = "b"; a = "c";` drops `a` (replaced), then `b`, then `a`, at 2,073/131/3. | `m5::ownership_from_installed_source`, `m5::replacement_keeps_the_declaration_rank` |
| S2-F7 | Each pair row names its exact `GoalState` (`Proved`, `Disproved` or `Unresolved(CallableEnvironment)`). | `m4_8_ownership` |
| R-5, R-7 | The graph old-cap assertions also assert no finding and no root MIR. | `rfc_cases::o15_pre_charge_counter_caps_now_give_resource_limit`, `option_caller::exclusive_relay_caller_exceeds_its_parent_caps` |
| R-6 | `callerExclusive` asserts `Move` → `CallIntoTemporary` → `TestVariant`, then `TakeOptionPayload` on `Some` and `EndEmptyOption` on `None`, with no copy, in the target and the graph root. | `option_caller::admitted_option_relay_callers_keep_their_caps` |
| R-3 | The capture conflict with no unsupported parent: `R0201` at `flag`, related `&mut flag`, target 3,916/530/7, graph 194,574/4,692/7 (first pins, own fixture so no published cap moves). | `scoped_join::fresh_result::plain_conflict_reports_r0201_at_the_second_capture` |
| S2-F4 | A scoped reference pair reserves each slot's three goal handles before it builds the slot, and the later reserve drops from 8 to 2. The total is unchanged: typed 427,606/49,151/12, one short gives `ResourceLimit` with no receipt. | `scoped_join::reference_pair_reserves_before_it_builds_each_slot` |
| ri-vgz.3 #3 | `*view = true` after `forward(input)?`: M4 issues no receipt; typed `Incomplete` with a pending `Storage` at `*view = true;`, 328,076/48,338/12, as RFC O1/O2 (`ri-1up`). | `exclusive_option_caller::write_through_a_relayed_view_has_no_typed_receipt` |
| ri-vgz.6 #3 | Dead variants: the postfix tuple and condition after `return` emit no test, branch, stage or cleanup, and their second use of a disposed string is not `R0200`. `deadTuple` 3,358/431/4, `deadCondition` 3,313/407/4, graph 193,518/4,955/5. | `propagation::staging::dead_postfix_staging_grants_nothing` |
| ri-px1.8 #4 | Two same-provider scope-exit releases: `two(a, b)` releases `b` then `a`, target 1,933/45/1, graph 185,803/4,142/5; `pair(value: Pair)` releases `second` then `first`, target 2,296/105/1, graph 186,516/4,202/5. Against `36f1cf15` (185,387 and 185,814) the equality root adds 316, and verification adds 100 and 386 work (24 for one release): the per-edge recharge is quadratic in the releases. | `graph::closed_release::two_guard_{parameters,fields}_release_in_reverse_and_close_the_root` |
| ri-px1.9 #3 | `pass(again, lock): LockGuard<i32>` returning its guard after a loop stops at `lock` with no delivery proof, at the `3da1d5a3` caps 1,888/25/2, graph 186,018/4,120/5. | `cancellation::guard_returned_after_a_loop_keeps_its_parent_caps` |

### Rule-1 sibling costs (O7)

| Finding | Result |
| --- | --- |
| ri-785.1 #3 | No change needed. An arrow body's `first * 10i32 + second` never reaches the charged local read: `arrowInvoked` stops at `f()` (1,906/50/3) and `arrowUninvoked` at the arrow (1,919/51/4), both the `a2d8e05` caps. Graph 187,125/4,173/5 (`a2d8e05`: 186,809/4,039/4 plus the equality root). Test `o15_arithmetic::arrow_body_arithmetic_keeps_its_parent_caps`. |
| ri-vgz.5 #2 | Fixed in code. A `string` error wrap now screens its RHS for a postfix (after the paid RHS fact and grouping) before the retained goals, display and `?` token are charged; an `i32` wrap keeps its order. The witness `input ? (label(source?))` reaches the branch: `Incomplete` at `(label(source?))`, 4,095/435/4, graph 189,339/4,600/5. Before the fix: 4,107/435/4 and 189,351. At `a95e04e8` it stopped at its parameter (1,878/30/0); the parameter has been admitted since C3. Row in `text_err::string_error_cases_keep_their_outcomes_at_exact_caps`. |
| ri-vgz.3 #2 | Fixed in code. The relay screen rejects a callee named through a public binding, or declared inside the caller's body, before the first charge. `callerExport` and `callerScoped`, shared and exclusive, keep the parent targeted caps (3,115/297/4 and 3,118/298/4; `cb94538f` for shared, `b5e361de` for exclusive). The graph moves only by the equality root. Before the fix each paid 500 more work. Test `option_caller::exported_and_scoped_relay_callees_keep_parent_caps`. A callee with another signature is still told apart only by its charged header. |
| ri-px1.8 #3b | No change needed. `wide(lock: LockGuard<i64>)`, `generic(value: Holder<i32>)` and a flat record with a `LockGuard<i64>` field stop at their parameter, so they never reach the scope-exit verification. Targeted 1,841/12/0 and 1,849/12/0, graph 186,005/4,113/5 (`36f1cf15` plus the equality root). Test `graph::closed_release::implicit_guard_siblings_keep_their_parent_caps`. |
| ri-785.7 #1 | Partly. The owned once wrapper whose body is `return other();` gets no M4 receipt (`Containment` at `job: F`, 324,801/47,618/12 at `a95e04e8` and today), pinned in `declared_shared_siblings_keep_their_parent_caps`. **Stopped:** `ignored` (`skip(&f)`, +5 over 1,917/55/3) and the lent wrapper `return other();` (`ignoredOther`, 1,899/46/3 at `a95e04e8`, 2,395/46/3 today, +496) keep paying. The wrapper's body is found only by the charged ordered-declaration probe (+5), and telling `other()` from the formal `job()` needs the charged header or node-fact probes. The `return other();` sibling stays unpinned (no candidate-cap pin without a ruling). |
| ri-vgz.7 #1 | **Stopped.** `mixed` (+3) and `custom` (+372 work, +61 entries) are told apart by the yielded record's field types and its disposer. Those are only in the typed declaration facts (a keyed lookup, rule 3) and the disposer selection; no paid fact on the loop carries them. The current pins stay. |
| ri-785.5 #1 | **Stopped.** The formal actual (+1) is rejected by the one charged fact probe of its operand. The `Result` capture (+16/+3) is rejected after the charged residence reads that find the captured `Result`. `shared-unclosed` (+464) is rejected only after the wrapper's other caller closes, when the unclosed `run(job)` is found. No M4 fact published before these charges decides any of them. The current pins stay. |

### Re-derivations (R-2)

The three A7 old numbers reproduce at `d95ae9e`, in both modes, in a scratch worktree (`git worktree add`, the `live-parent-err` fixture, `lower_body` and `analyze_ownership` at default limits): `liveParentError` `Incomplete` at `pair`, 3,449/320/3; `retiredParentError` `Incomplete` at the `join(...)` call, 3,457/326/3; whole graph `Incomplete` 197,064/4,761/5. The measuring test and script are in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-paai/measure/` (`measure_snippet.rs`, `measure.sh`, `lpe-d95ae9e.txt`). The same procedure measured every parent cap in this section.

**Update (2026-11-24, review `ci-0mdr` ST-18, R-2).** Transcript only, not reproduced: those files are outside the repository. Copies of the source, the measuring snippet, the command and the output at `d95ae9e` are committed in [`ri-px1.11.6-d95ae9e/`](ri-px1.11.6-d95ae9e/README.md), and the three numbers above are marked there as a transcript of the original run. Nothing was re-run for this record.

### Superseded record lines (SP-F6)

[`ri-vgz.8.md`](ri-vgz.8.md) (tip `5471035b`) predates B6's equality source root (`ri-785.9`, rule 11 D2 (b): +316 work and +134 entries, depth at least 5 in every whole graph). Its graph triples are now the pre-equality caps, each asserted to give `ResourceLimit`. Current values: `result_text` bound 188,856 / 4,810 / 5 (was 188,540 / 4,676 / 4); `text_err` bound 187,772 / 4,617 / 5 (was 187,456 / 4,483 / 4); `relay(input)?` 198,870 / 6,758 / 5 (was 198,554 / 6,624 / 4); `callerPostfix` 196,188 / 6,243 / 5 (was 195,872 / 6,109 / 4); the closeout fixture 203,351 / 7,479 / 6 (was 203,035 / 7,345 / 6); the 17-source graph 203,014 / 5,005 / 5 and the 18-source graph 243,522 / 26,094 / 10 (were 202,698 / 4,871 / 3 and 243,206 / 25,960 / 10). Its "No fixed cap moved" was true at its tip only. [`ri-785.10.md`](ri-785.10.md)'s graph triples are its tip's post-equality values. Three of its figures are not pinned as literals in any test: `local_exclusive_capture_transfers_the_loan_until_discard` graph 193,822 / 5,407 / 5, `invokes_a_known_once_pair_and_transfers_its_contents` typed 359,132 / 52,801 / 12, and `erased_read_only_pair` typed 322,016 / 47,461 / 12 ("default"). Read them as that audit's measurements, not as pins.

### Host-model evidence (SP-F7)

`runtime/collection/tests/storage.rs::owned_option_iterator_distinguishes_empty_yields_and_retires_the_suffix_in_reverse` (`e8d3423f`) tests the host model of the runtime collection crate. It is not M5 compiler or MIR evidence, and no M5 row cites it.

### Beads filed

- `ri-qlh.3`: "Charge path-keyed lookups per rule 3" (O2). It lists every site of S1-F1..F4, S2-F1..F3, S2-F5 and ri-vgz.7 #2, the charged wrapper design, the review's baseline smells (O8) and that its re-pins fall under rule 11's third case. Parent `ri-qlh`, related to `ri-2s0` and `ri-30q`.
- `ri-8qlm` (K3/O21) and `ri-fmsk` (O5 field arithmetic), filed under O4. The owner's override makes them M5 tickets, so they are not in the deferral record.
- `ri-grwx` (found): `analyze_ownership` keeps findings published with `ResourceLimit` (metering rule 8), from the R-3 test.

### Not established

- The full M5 binary, `mise run check` and the native workflow run in hosted CI only.
- The `ri-785.7 #1`, `ri-vgz.7 #1` and `ri-785.5 #1` siblings above still pay before they reject. They are reported to the planner, not given an exception.

**Superseded (2026-11-24, review `ci-0mdr` ST-19).** The three "Stopped" rows of "Rule-1 sibling costs" and the "not given an exception" line above are superseded by the chief of staff's rulings of 2026-11-23 (`chief-rulings-2.md`, item 4), which granted all of them as the rule-1 exception list in [`compiler-metering.md`](../../../../docs/agents/compiler-metering.md) (`ignored` and `ignoredOther`, `mixed` and `custom`, the three `m4_8_callable_once` rows), each pinned exact and one short with its parent cap giving `ResourceLimit` ([`ri-2s0.10.md`](ri-2s0.10.md)). The lines above are not edited.

## Final record after the owner override (2026-11-24, R-D6b)

Records only: no compiler source, test or cap changed. Base `c6b2cc98` of `implementation/m5-ownership-cleanup`, which holds the last build merge (K3 on a propagation exit, `ri-2s0.13`). Branch `m5-ownership-cleanup/r-d6b-ri-px1.11`. The review-fix section above answers review `ci-uaa1`; this one rewrites the record for what merged after it, under the owner's override of 2026-11-23 19:18 AEDT (the deferred Required rows are built in M5).

- [`m5-acceptance.md`](../m5-acceptance.md): the section "Final record: deferred obligations (2026-11-23, ri-px1.11)" is replaced by "Final record (2026-11-24, ri-px1.11, R-D6b)". It lists every row built since the first record with its bead and evidence file (so no deferral row remains for a built row), the deferrals that still hold with the state of each bead, the ten open found beads, the rule exceptions in force and the stated limits recorded by tickets. The first record's `ri-qlh.3` row moved to the open found beads, and `ri-30q` is listed as closed (superseded).
- [`m5-status.md`](../m5-status.md): every owner cell names an open bead, or "none: <ticket>" where the row is built. O1, O10, O14, O15 and O16 changed. O10 names `ri-can9` for the receivers it does not cover and states the untested `LookupListElement` MIR form. Notes 1 and 2 carry a dated update, because they described the state of 2026-11-22.
- [`compiler-metering.md`](../../../../docs/agents/compiler-metering.md): a short paragraph states two M5 rules from the retired wiki that no file held (the chief of staff's note of 2026-11-24 12:36 AEDT on `ri-2s0`): evidence covers both installed load modes, and MIR tests are not Rill execution.
- Pointers use `docs/agents/` paths. No `.wiki/` link is added.

The second review is next, at the fixed point `origin/main`. `ri-px1.11` stays open until it is clean and hosted CI is green on the final head. The hosted checks on this head did not run. No Rill program ran.

## Review ci-0mdr fixes (2026-11-24, city bead ci-ru4p, C7)

This section consolidates the per-child evidence of the fix pass for review `ci-0mdr` (second review of `81d317b3`; findings in the chief of staff's triage `review-2-triage.md`). One line per finding: fixed (commit, test), recorded, or stopped (reason). The per-child files stay: C1 [`ri-px1.11-c1.md`](ri-px1.11-c1.md) (the brief names it `ri-grwx.md`; no such file exists), C2 [`ri-px1.11.1.md`](ri-px1.11.1.md), C3 [`ri-px1.11.2.md`](ri-px1.11.2.md), C4 [`ri-px1.11.3.md`](ri-px1.11.3.md), C5 [`ri-px1.11.4.md`](ri-px1.11.4.md), C6 [`ri-px1.11.5.md`](ri-px1.11.5.md), C8 [`ri-68mu.md`](ri-68mu.md), C9 [`ri-q9lj.md`](ri-q9lj.md) and this child's [`ri-px1.11.6.md`](ri-px1.11.6.md). Merge commits on `implementation/m5-ownership-cleanup` (`git log --merges --first-parent`): C1 `da5bec50`, C2 `a4e56651`, C4 `eabe14ad`, C8 `6f35705e`, C5 `d58380d1`, C3 `2052c240`, C6 `34d39c4e`, C9 `61f22ae3`. Test paths are under `crates/compiler/tests/`. No Rill program ran.

*Added 2026-11-24 (re-review `ci-b4b2` C-1, `ri-px1.11.11`).* C7's merge is `4ab0a82c` (the list above has no C7 entry). C10, `ri-px1.11.7` (`ci-1lo8`), merged as `d87e5dd2` under chief ruling 17 (16:02 AEDT): at a conversion `ResourceLimit` `type_facts()` is empty and `header_metadata()` is `Ok(None)`; conditions (1) the rule-8 sentence, (2) a test pinning empty facts at the limit and facts at the exact cap, (3) `Ok(None)` is not confusable with "no header": `header_metadata` already gives `Ok(None)` for a source without a header, said in the rule-8 note, callers check `completion()` first. Ruling 18 (16:33 AEDT) scoped the sentence, so it is superseded as the final record after the re-review says. The re-review `ci-b4b2` of `d87e5dd2` and its fix children C11 to C14 (`ri-px1.11.8` to `ri-px1.11.11`) are recorded in `m5-acceptance.md`, "Final record after the re-review fix pass".

### Standards

- **ST-1** (`grouped` unlisted): fixed, `e46e8f38`, `m5::rfc_cases::list_contents::list_destination_siblings_keep_their_outcomes_and_blockers` (`GROUPED`, +74 work, +3 entries); the fifth use is in `compiler-metering.md` rule 1 ([`ri-px1.11.5.md`](ri-px1.11.5.md)).
- **ST-2** (O11 family view over a non-list): fixed, `44d13d20`, `rfc_cases::o11_family_view_over_a_non_list_stays_at_its_parent_cap`.
- **ST-3** (two path-keyed probes at fixed width): fixed, `295070c2`, `rfc_cases::field_through_reference_charges_its_local_probe` and `rfc_cases::list_contents::list_reference_parameter_charges_its_insert`.
- **ST-4** (slot allocated before a disposer record is rejected): recorded. C4 stopped it (`44d13d20`, `8aee6b3e`); the chief granted it as a named rule-1 exception (ruling of 14:47 AEDT). Pinned exact and one short by `rfc_cases::o12::disposer_record_results_pay_the_disposer_lookup_and_keep_their_outcomes` (C7): `within` 1,875 / 27 / 3 to 2,252 / 90 / 6, `bound` 1,891 / 34 / 4 to 2,263 / 97 / 4.
- **ST-5** (`ResourceLimit` assertions that omit part of rule 8): fixed, `ead19c6a` (graph, body and typed helpers in `support/limit.rs`, about 390 sites; `analyze_ownership` withholds), `a0a3dfab` (`analyze_typed`, `ri-68mu`), `e2b0c80f` (`analyze_ordinary`, `analyze_generics`, `ConversionReport::bodies`, `ri-q9lj`) and C7 (`ConversionReport::type_facts` and `header_metadata`). Tests: `scoped_join::fresh_result::plain_conflict_reports_r0201_at_the_second_capture`, `m4_8::resource_exhaustion_publishes_no_finding_list`, `m4_5::resource_limit_publishes_no_finding_list`, `m4_7::conversion_report_publishes_no_finding_list_at_resource_limit`.
- **ST-6** (admitted O16 shapes proved by `mir().is_some()`): fixed, `448fa595`, `o16_join_order::admitted_shapes_check_at_exact_caps` and `o16_join_order::grouped_operands_add_depth_not_operations`.
- **ST-7** (O1 sibling caps on a hybrid base): recorded, `ed837d8a`; accepted by the chief (ruling of 15:28 AEDT) as the rule-2 witness. Test `rfc_cases::o01_exclusive_write_siblings_typed_parent_cap_now_gives_resource_limit`; the patch is [`ri-px1.11.5-hybrid.patch`](ri-px1.11.5-hybrid.patch); the rule-2 note is in `compiler-metering.md`.
- **ST-8** (scratch `Vec` without a reserve): fixed, `295070c2`, `rfc_cases::call_field_equality_reserves_its_scratch`.
- **ST-9** (`i32_arithmetic.rs:326`, a decided conjunction operand): fixed for the right operand, `44d13d20`, `rfc_cases::decided_conjunction_right_keeps_its_parent_cap` (red on the base); the left operand is not reproducible and is pinned at its parent cap by `rfc_cases::decided_conjunction_left_keeps_its_parent_cap`. At `i32_arithmetic.rs:202` (`self.place(target)`): recorded; granted by the chief (ruling of 14:47 AEDT), `viaOther` 2,245 to 2,246 and `viaReborrow` 2,249 to 2,250 work, pinned by `o05_field_arithmetic::local_descriptor_write_through_another_binding_pays_one_work` and `o05_field_arithmetic::local_descriptor_write_through_a_reborrow_pays_one_work` (C7); the type and literal checks the review named cannot fire there ([`ri-px1.11.3.md`](ri-px1.11.3.md)).
- **ST-10** (blocker site not asserted): fixed, `e46e8f38` and `657ec901`, `rfc_cases::list_contents::r1_caller_provided_exclusive_source_stops_typed`.
- **ST-11** (no parent hash on the sibling constants): fixed, `cad220d5`, doc lines on `O10_SIBLINGS`, `O10_LOOKUP_SIBLINGS`, `O10_GET_SIBLINGS`, `O11_SIBLINGS` in `rfc_cases.rs`.
- **ST-12** (O11's pre-equality total lost): fixed, `cad220d5`, `rfc_cases::o11_pre_family_call_caps_now_give_resource_limit`.
- **ST-13** (`pre_equality_graph` values that are not measurements): fixed, `cad220d5`, `Outcome::Receipt.pre_equality_graph` is `Option<Triple>`; derived rows of other sources are unchanged ([`ri-px1.11.5.md`](ri-px1.11.5.md)).
- **ST-14** (rule 11 asks for manifest rows): recorded. Rule 11 gains the dated note of the chief's ruling Q2 (an in-test cap is pinned in its test; `legacy-caps.tsv` holds fixture files). No manifest row added.
- **ST-15** (`hold` and `replace` checked with `position`/`any`): fixed, `f4c2b9f0`, `cancellation::installed_guard_release_on_budget_delivery` and `disposers::protected_guard_cleanup_follows_the_initialized_owner`.
- **ST-16** (R-4, the counter's +58 only a comment): fixed, `12a8660d`, `o15_arithmetic::rfc_counter_caps_include_the_charged_local_reads`.
- **ST-17** (S2-F4, a test that cannot fail): fixed, `0c880e83`, `scoped_join::reference_pair_reserves_before_it_builds_each_slot` (entries caps in a narrow window separate the two reserve orders; red with `pairs.rs` reverted).
- **ST-18** (R-2, the `d95ae9e` re-derivation): recorded as "transcript only, not reproduced". Source, snippet, command and output are committed in [`ri-px1.11.6-d95ae9e/`](ri-px1.11.6-d95ae9e/README.md) and linked from the ledger.
- **ST-19** (stale record lines): recorded, dated supersession lines (the old lines are not edited) at `ri-2s0.4.md` ("needs a ruling"), `ri-px1.11.md` ("Rule-1 sibling costs", Stopped), the ledger's H1 table (against `legacy-caps.tsv`), the ledger's `pre` and `computed` rows, the ledger's "`ri-2s0.13` still reads ready-for-agent" and `compiler-metering.md` rule 3 (the `ri-30q` lint). **RFC 0007 line 542** ("No compiler evidence exists for O9 yet") is the owner's text and is left alone; it is listed in the ledger for U2.
- **ST-20** (evidence logs outside the repository): recorded. The final record says where they live: `~/.t3/evidence/rill/m5-build-2026-11-22/<city bead>/`. No file moved.
- **Unverified** (`retained_contents` siblings): measured, `bc6fcd2e` and `ba6dac20`; list forms pinned under the standing ruling by `rfc_cases::list_contents::list_parameter_siblings_pay_their_lookup_and_keep_their_outcomes`; the two `&mut &R` forms over bound (c) are a named exception (ruling of 15:28 AEDT), `rfc_cases::record_contents::forwarded_record_slot_roots_pay_three_lookups_and_keep_their_outcomes`, found bead `ri-zee8`.

### Spec

- **SP-1** (R1 old contents): fixed, `e46e8f38`, `rfc_cases::list_contents::r1_old_contents_survive_a_later_retain`.
- **SP-2** (owned parameter and temporary sources): fixed, `e46e8f38`, `rfc_cases::list_contents::r1_owned_parameter_and_temporary_sources_do_not_escape`. A temporary source stays `Incomplete` (stated limit, no list destination for a temporary).
- **SP-3** (member replacement unpinned): fixed, `15ab11af`, `rfc_cases::o07_forms::member_replacement_pins_its_sequence_and_twins` and `rfc_cases::o07::construction_twins_the_review_found_unpinned`.
- **SP-4** (member replacement overlap with no later use): fixed (outcome chosen by Q3, reject R0201), `15ab11af`, `rfc_cases::o07::member_replacement_overlap_without_later_use`; `src/analysis/ownership/flow.rs` liveness.
- **SP-5** (no rejecting twin for the pair and the lone destination): fixed, `e46e8f38`, `rfc_cases::list_contents::r1_pair_with_one_body_local_source_rejects` and `rfc_cases::list_contents::r1_lone_destination_keeps_a_stored_local_rejected`.
- **SP-6** (R0202 site is the last retention): fixed (pinned as the rule gives it), `e46e8f38`, `rfc_cases::list_contents::r1_local_then_parameter_reports_the_storing_call`.
  - *Superseded (2026-11-24, `ri-px1.11.15` (city bead `ci-d5gm`, re-review `ci-9qga`), 2D-3).* "SP-6 fixed" is wrong. The renamed test `rfc_cases::list_contents::r1_blocker_site_is_the_last_retention_not_the_storing_call` pins the compiler's R0202 site as the last retention before the exit, not the storing call, as a stated limit, bead `ri-i0kj` (C14, `ri-px1.11.11`). The line above and its test name are not edited.
- **SP-7** (C12 siblings pinned `SourceRejected`): fixed (Q4), `60dc10d4` with `c4c3715f` and `363c3083`, `m4_8_match_boolean_snapshots::installed_boolean_matches` (`pre: mutable binder`, `pre: mutable source` `Checked`; a third admission, `pre: immutable local source`, accepted by the chief's ruling of 14:50 AEDT).
- **SP-8** (three propagation gaps pointed at `ri-qlh.8`): recorded. `ri-qlh.8` does not cover them; the planner filed `ri-qlh.11` and the ledger's K3 bullet points there (C7).
- **SP-9** (the record's base `c6b2cc98`): the planner's. The final record now names the base of this pass.

### Decisions, carried-over findings and the Comments axis

- **Q1** (`ri-grwx`): fixed, `ead19c6a`; the planner closed `ri-grwx`. No published cap moved.
- **Q2**: recorded (see ST-14). **Q3**: fixed (see SP-4). **Q4**: fixed (see SP-7). **Q5**: recorded; the standing bound (c) reads "at most two charged declaration lookups per sibling module, under 150 work per sibling plus the entries of those lookups" (`compiler-metering.md`, `common.md`).
- **R-2**: see ST-18. **R-4**: see ST-16. **S2-F4**: see ST-17.
- **Comments axis** (about 929 comment lines, four MUST KILL sites): exempt on this branch by `docs/agents/comments.md`. No fix restructured the MUST KILL sites; `ri-qlh.10` names them (the four of the review and the retained-contents split) and is listed in the ledger's open found beads.
- **Standing ruling on `ResourceLimit` entry points** (chief, 14:20 AEDT): `analyze_ownership` (C1), `analyze_typed` (C8, `ri-68mu`), `analyze_ordinary`, `analyze_generics` and `ConversionReport::bodies` (C9, `ri-q9lj`), `ConversionReport::type_facts` and `header_metadata` (C7: published findings, withheld; [`ri-px1.11.6.md`](ri-px1.11.6.md)). `analyze_declaration_types`, `check_declarations`, `check_scalars` and `check_data` return an `Err` partial and are outside rule 8 (ruling of 15:12 AEDT; `ri-qlh.9`).
