# Compiler metering standard

Apply this page to every diff under `crates/compiler/src/analysis/` and every test that pins work, entries or depth. Authors follow it while writing; Standards reviewers walk the eleven rules against the diff and cite the rule number. A rule is met when its **Check** can be answered from the diff alone.

Vocabulary: a **cap** is one `(work, entries, depth)` limit in `AnalysisLimits`. **Exact** is the cap equal to the measured statistics; **one short** lowers one axis by one. A **targeted** query is one function (`lower_body`); a **graph** query is the whole receipt (`analyze_ownership`). The **ledger** is [`m5-acceptance.md`](../../crates/compiler/tests/m5-acceptance.md). Each rule cites one commit that shows the fix.

M5 evidence covers both installed load modes. MIR tests are not Rill execution.

## 1. Screen before you charge

A new proof path decides eligibility from facts already paid for, before its first `budget.step`, `budget.reserve`, `charge_key` or allocation. Otherwise an unsupported sibling that used to stop at `Incomplete` now runs out of budget at its old cap and reports `ResourceLimit`.

- **Check:** in every new path, no `step`, `reserve`, `charge_key` or allocation precedes the last early return that rejects an unsupported shape.
- **Example:** `335da9f` preflights both child shapes, environments and premises before allocating isolated Result-use checks.

**Exceptions (2026-11-23, chief of staff, under rule 11's authority line).** Eight unsupported siblings in four tests pay before they reject, because no paid fact can reject them earlier; an earlier screen needs new M4 facts. Each keeps its current cap, pinned exact and one short, and each parent cap is asserted to give `ResourceLimit`. They stay `Incomplete` (or `SourceRejected`) at default limits. The evidence is [`ri-2s0.10.md`](../../crates/compiler/tests/m5-acceptance/ri-2s0.10.md); the restructuring is one open bead. No other sibling has an exception, except as added below.

- `captures::declared_shared::declared_shared_siblings_keep_their_parent_caps`: `ignored` (`skip(&f)`, +5 work) and `ignoredOther` (a lent `return other();` wrapper, +496 work).
- `iteration::record::record_yield_cleans_the_partial_item_then_the_suffix`: `mixed` (+3 work) and `custom` (+372 work, +61 entries).
- `m4_8_callable_once::private_once_checks_the_actual_shared_pair`: `shared-formal-actual` (+1 work), `shared-result-capture` (+16 work, +3 entries) and `shared-unclosed` (+464 work, +14 entries).
- `m4_8_match_boolean_snapshots::installed_boolean_matches`: `pre: copy of another parameter` (+14 work), before `R0103`. `pre: mutable binder` and `pre: mutable source` left this list on 2026-11-24 (chief ruling Q4 of review `ci-0mdr`, [`ri-px1.11.2.md`](../../crates/compiler/tests/m5-acceptance/ri-px1.11.2.md)): a mutable local that nothing writes now checks with a receipt, so neither rejects.

**Addition (2026-11-24, chief of staff, ruling of 06:50 AEDT, ri-2s0.8).** Two more siblings pay before they reject, on the same terms: the screen in `ownership/build.rs` that reads a call result's record field list costs +3 work in each root of `rfc_cases::o12::text_record_results_stay_incomplete`, `(&named()).x` (`textRead`) and `let view = &named();` (`textBind`) (whole graph 187,421 to 187,427). Both stay `Incomplete` at the same blocker, pinned exact and one short, and each parent cap (1,873 / 26 / 3; 1,889 / 33 / 4; graph 187,421 / 4,277 / 5) is asserted to give `ResourceLimit` with no MIR and no finding. The evidence is [`ri-2s0.8.md`](../../crates/compiler/tests/m5-acceptance/ri-2s0.8.md); an earlier screen needs a new M4 fact carrying the field list, recorded in `ri-qlh.4`.

**Addition (2026-11-24, chief of staff, rulings of 07:19 and 11:35 AEDT, ri-1up).** Three more siblings pay before they reject, on the same terms: the screen for `&mut &Record` retained contents (`retained_record_contents` in `analysis.rs`) reads the record's field list through a path-charged lookup (rule 3), after a free type screen (application origin, no type arguments, `&mut &Nominal`), and no paid fact carries the list. Each unsupported sibling pays it twice in typed work (the callee's store, the caller's `&mut view`) and stays typed `Incomplete` at the same blockers, pinned exact and one short, with each parent cap (`bc9bfdd5`) asserted to give `ResourceLimit` with no receipt: a record with a `string` field (325,487 to 325,631, +144), a nested record (327,152 to 327,296, +144) and an enum (324,194 to 324,334, +140), all of `rfc_cases::record_contents::unsupported_record_contents_stay_incomplete` and `rfc_cases::record_contents::unsupported_record_contents_parent_caps_now_give_resource_limit`. A generic record and `&mut &mut Point` are refused by the free screen and keep their parent caps. The evidence is [`ri-1up.md`](../../crates/compiler/tests/m5-acceptance/ri-1up.md); an earlier screen needs a new M4 fact carrying record field and enum variant lists, recorded in `ri-qlh.4`.

**Addition (2026-11-24, the standing rule-1 exception, ri-ema).** Four more siblings pay before they reject: the contents screen for a `&mut List<&Record>` destination (`retained_list_candidate` in `ownership.rs`, rule R1) is a free type screen (the List role, an element `&` of an application nominal with no type arguments), and then reads the record's field list through the same path-charged lookup (`retained_i32_record`, rule 3). No paid fact carries the list. Each pays it twice in ownership work (the caller's own parameter and the call; a screen that looks for a retained parameter skips list destinations, so the parameter pays once), stays `Incomplete` at the same blocker, and keeps its outcome, pinned exact and one short, with each parent cap (`b6c93f30`) asserted to give `ResourceLimit`: a record with a `string` field (`forwarded` 1,904 to 2,052, +148), a nested record (1,906 to 2,054, +148) and an enum (1,900 to 2,044, +144), and a call with an extra `bool` beside the list (1,917 to 1,991, +74: only the parameter's lookup), all in `rfc_cases::list_contents::list_destination_siblings_keep_their_outcomes_and_blockers`. A generic record, an owned or exclusive element and a shared destination are refused by the free screen and keep their parent caps. The evidence is [`ri-ema.md`](../../crates/compiler/tests/m5-acceptance/ri-ema.md); an earlier screen needs the M4 fact recorded in `ri-qlh.4`.

**Addition (2026-11-24, review ci-0mdr ST-1, ri-ema).** A fifth sibling of the same exception, listed late: `grouped` (`retain((dst), source)`) pays the parameter's lookup and stays `Incomplete` at the same blocker (`retain((dst), source)`), 1,905 to 1,979 work and 42 to 45 entries in its own module (+74, +3, inside bound (c) as amended 2026-11-24: under 150 work per sibling plus the entries of those lookups), asserted in `rfc_cases::list_contents::list_destination_siblings_keep_their_outcomes_and_blockers` with its parent cap (`b6c93f30`) giving `ResourceLimit`. In the module with the other family forms the same root reads 1,995 to 2,069 / 87 to 90. The evidence is [`ri-px1.11.5.md`](../../crates/compiler/tests/m5-acceptance/ri-px1.11.5.md).

**Addition (2026-11-24, review ci-0mdr, the "Unverified" item, ri-ema).** The measurement found more siblings of the standing exception than the five above, all behind a `&mut List<&R>` parameter where `R` has a `string` field (a nested record and an enum cost the same to within two work): any body that declares such a parameter pays its one lookup (+72 work, no entries; +70 for an enum), `Checked` or not; a call that passes the parameter on pays a second, direct or piped (`dst |> touchTwo(s)`), up to +148 work and +3 entries in one root. Each keeps its outcome and blocker and stays inside bound (c); the parent caps (`b6c93f30`) give `ResourceLimit`. Pinned in `rfc_cases::list_contents::list_parameter_siblings_pay_their_lookup_and_keep_their_outcomes`. The same measurement on `&mut &R` found two forms over bound (c) against the parent `bc9bfdd5` (+216 work, +210 for an enum: three lookups in one root) and is reported, not listed: see [`ri-px1.11.5.md`](../../crates/compiler/tests/m5-acceptance/ri-px1.11.5.md).

**Named exception (2026-11-24, chief of staff, ruling of 15:28 AEDT, ri-zee8; outside the standing ruling, whose bound (c) is unchanged).** The two forms found over bound (c) are granted at their measured costs: a root that declares a `&mut &T` parameter for an unsupported `T` and passes it to a function that also takes a `&T` pays three charged declaration lookups (+216 work for a `string` field, direct and piped, and for a nested record; +210 for an enum). Each keeps its outcome and blocker (`Incomplete`, `slot`), pinned exact and one short, with each parent cap (`bc9bfdd5`) asserted to give `ResourceLimit` through the shared helper. The roots, all in `rfc_cases::record_contents::forwarded_record_slot_roots_pay_three_lookups_and_keep_their_outcomes`: `f` in `FORWARD_NAMED` (`touchTwo(slot, s)`, 1,955 to 2,171), `f` in `FORWARD_NAMED_PIPE` (`slot |> touchTwo(s)`, 1,955 to 2,171), `f` in `FORWARD_NESTED` (1,957 to 2,173) and `f` in `FORWARD_ENUM` (1,951 to 2,161). The helper roots in those modules pay one and two lookups (+72, +144; +70, +140 for an enum), inside bound (c). The real fix is the M4 field-list fact in `ri-qlh.4`, which this bug (`ri-zee8`) is a further reason for.

## 2. Every new admission ships a parent-cap regression

Admitting a shape adds work to the shapes next to it. Pin the nearest unsupported siblings at the caps the parent revision measured, so the test fails when the new path spends before it rejects.

- **Check:** the diff adds a test whose targeted and graph caps cite the parent commit and are not re-measured on the candidate. It covers the nearest unsupported siblings: grouped, scoped and loop-wrapped forms of the admitted shape.
- **Example:** `c4bbf8a` pins `direct`, `computed`, `grouped`, `scoped` and `screened` at "exact results measured at a2d8e05, before arithmetic lowering", with the blocker span of each.

**Note (2026-11-24, chief of staff, ruling of 15:28 AEDT, ri-px1.11.5, ST-7).** When a ticket lands a typed admission and its lowering together, sibling caps are measured on the parent plus the typed admission only; the evidence file keeps the patch and the parent commit. Splitting such a ticket into a typed commit and a lowering commit is preferred from now on, so a real parent exists. The first use is O1's `EXCLUSIVE_WRITE_SIBLINGS` (ri-2s0.5): parent `7002855e` plus [`ri-px1.11.5-hybrid.patch`](../../crates/compiler/tests/m5-acceptance/ri-px1.11.5-hybrid.patch), recorded in [`ri-px1.11.5.md`](../../crates/compiler/tests/m5-acceptance/ri-px1.11.5.md).

## 3. Charge keyed lookups

A `BTreeMap` probe costs one comparison per halving and each comparison can read the whole key. A key that contains a `ModuleKey.path` is charged by path length times probes through `charge_key` (`ownership/graph.rs`).

- **Check:** each `.get(`, `.contains_key(`, `.entry(` and `.insert(` on a map whose key holds a `ModuleKey.path` is preceded by `charge_key`, or by a call that wraps it such as `module_index`. A fixed-width key needs `budget.step(1)`.
- **Example:** `6c9cb8c7` introduces `charge_key` and routes the graph and state-cache probes through it.
- **Known non-conforming sites (2026-11-23):** review `ci-uaa1` found more than 25 existing probes of `DeclarationId`-keyed maps charged as fixed width or not at all, in `ownership/build.rs`, `flow.rs` and `scalar.rs`, typed `result_use` and `scalars/generics/callable_copy.rs`. By owner decision of 2026-11-23 they are deferred to `ri-qlh.3` (charged lookup wrappers and the `ri-30q` lint), whose re-pins fall under the third case of rule 11. Those existing sites are known non-conforming; a new or touched probe must conform.

## 4. Reserve before you allocate

Storage is spent twice: as entries and as work. A vector, state clone or temporary reserves its size before it is built, so a limit stops the query before memory grows.

- **Check:** each new `Vec`, `clone()` of analysis state or collected temporary has a `budget.reserve(n)` on the path before it, with `n` covering the allocation.
- **Example:** `ac1db04b` adds `frame.budget.reserve(1)?` before the scoped child parameter is allocated.

## 5. Transparent groups are transparent

`((x))`, `(item)` and `((gate))?` mean what `x`, `item` and `gate?` mean. Grouped forms keep the receipt, add no uses, and charge written depth once per layer, checked before descent.

- **Check:** every new rule that matches an expression or pattern has a grouped variant in its tests. The test compares depth against the bare form (grouped is deeper) and fails one short of the grouped depth with `ResourceLimit`.
- **Example:** `78df2be` treats `NodeKind::Group` as depth without new uses ("Groups add depth, not new uses").

## 6. Dead or infeasible code grants nothing

An unreachable `?`, loop or borrow must not stamp a source, open storage, supply evidence or hide `R0103`. Only a feasible edge counts.

- **Check:** the tests hold a dead variant, a pruned variant and a feasible control of the same shape. The dead and pruned variants stay `Incomplete` or keep their diagnostic. The control succeeds.
- **Example:** `f4ff41b` requires a feasible postfix edge in a text cursor body, with `dead`, `pruned` and `feasible` sources.

## 7. A cached determination is keyed by its premises

A determination reused across revisits is valid only while every premise it used still matches: inputs, capture origins, assumptions, intermediate groups and output agreement. When one changes, drop the entry.

- **Check:** for each new or touched cache, list its premises in the diff. The key or the invalidation names every one. Each premise has a test that changes it between two uses.
- **Example:** `0b9d22b` adds the intermediate group premises that `46f16a7` left out of the scoped determination key.

## 8. `Incomplete`, `ResourceLimit` and `SourceRejected` are three outcomes

`Incomplete` means a supported analysis reached an unsupported shape. `ResourceLimit` means a cap ran out. `SourceRejected` means the source carries a finding (an `R`-code diagnostic). A test that asserts "not `Checked`" accepts all three and hides a swap between them.

- **Check:** every assertion names the expected outcome. An `Incomplete` assertion also names the blocker span (`blocked_at`). A `SourceRejected` assertion also names the finding. A `ResourceLimit` assertion also asserts that no MIR and no finding is published. Call `assert_graph_limit`, `assert_body_limit` or `assert_typed_limit` from `crates/compiler/tests/support/limit.rs` (a typed report: no receipt and no finding in any list it publishes); do not write the assertion by hand.
- **Example:** `62243cb3` asserts the blocker site of each cyclic shape, so a different `Incomplete` fails.

## 9. Tests assert the whole sequence

"A drop exists" or "at least one" establishes neither order nor exactly-once. Assert the full ordered list of actions, with each action and its place in one list.

- **Check:** a cleanup, delivery or ordering test compares one `Vec` of `(kind, place)` pairs for equality, not separate collections per kind and not `any`/`contains`.
- **Example:** `946be568` replaces separate `ended` and `dropped` vectors with one ordered `cleanup` list that interleaves `drop` with `end`.

## 10. A review finding closes with a committed test

A reviewer's finding is closed only by a test in the repository that fails without the fix. A probe in `/tmp` or a worker's summary does not close it. The test source lives in `crates/compiler/tests/`.

- **Check:** each fix commit for a finding adds or edits a test. Reverting only the source change fails that test, and the finding or its bead is named in the commit message or the ledger.
- **Example:** `343ff586` pins `dispose<Bag>((value))`, a grouped disposer operand, in `tests/m5/disposers.rs` together with the source fix that keeps it independently typed.

## 11. Cap changes

A published cap (in a test, or in the fixed-cap manifest once H1, `ri-px1.1`, adds it) moves only in the three cases below. Raising a cap so that a change fits is none of them. A diff that moves a cap in any other case fails review, and the ticket stops: its implementer closes the bead as `blocked` and reports the old and new numbers. A cap movement outside the three cases is decided by the rig's chief of staff (`rill/staff.chief-of-staff`) or the owner; the chief escalates to the owner when the movement changes what M5's definition of done means.

Every case requires, in one diff:

- the cap change as a single line in the fixed-cap manifest (`crates/compiler/tests/m5/legacy-caps.tsv`, once H1 has landed; before then, in the test that pins the cap);
- a dated ledger note that states the old numbers, the new numbers and the reason;
- an assertion that the **old** cap now reports `ResourceLimit`.

### Whole-graph expectations (owner decision D2 (b), 2026-11-22)

A new proved root may re-pin whole-graph expectations, and only those.

- **Check:** a new root is added to the graph and proved with its own test.
- **Check:** only `analyze_ownership` totals change. Every targeted cap in the diff is unchanged.
- **Check:** the diff attributes the difference per root (work and entries each root adds) and names the root that sets the new depth, since depth is a maximum and not a sum. The per-root figures add up to the change in the totals.
- **Check:** the historical totals stay in the ledger.
- **Example:** the ledger section "Canonical disposal of named scalar operands" (`d647a5a8`) adds 14 scalar `dispose` roots. The negative replacement graph moves to 203,358 / 8,126 / 3 and its two targeted budgets stay.

### An unsupported targeted route (owner decision D3 (b), narrowly, 2026-11-22)

A correctness fix may move the cap of an unsupported targeted route when the blocker site moves for a stated reason: the old number measured a path that skipped a check that the fix adds.

- **Check:** the commit message or ledger note names the skipped check and the old and new blocker sites.
- **Check:** the new route reports `Incomplete` at the new cap, with the new blocker site asserted (rule 8). The old cap gives `ResourceLimit`.
- **Check:** the cost is not avoidable. If rule 1 could place a screen earlier so the old cap still holds, the exception does not apply and the diff is refused.
- **Example:** `003c4147` checks both captures before the parent exit, as Standards required. The old live-`Err` cap stopped at the unlowered pair binding. `e022c80d` re-pins it under this case: `live_parent_error_reaches_the_join_at_its_repinned_cap` (`tests/m5/scoped_join/fresh_result.rs`) asserts that the old cap now gives `ResourceLimit`. Reproducing the old blocker would need an earlier proof that keeps the capture diagnostics.

### A ticket's mandated flip or charge (owner decision, 2026-11-23)

A ticket whose body mandates flipping a targeted outcome (for example `Incomplete` to `Checked`), or charging a lookup that a rule requires, may move the targeted caps that the flip or charge moves. Whole-graph moves in the same diff still follow D2 (b).

- **Check:** the ticket body or a recorded ruling names the flip or charge before the diff.
- **Check:** the ticket's evidence file states the old numbers, the new numbers and the reason.
- **Check:** the old cap asserts `ResourceLimit`.
- **Check:** the cost is not avoidable by a rule-1 screen.
- **Note (2026-11-23, chief of staff):** when a mandated flip lowers the cost, the old cap cannot give `ResourceLimit`: it still holds. The test then asserts the new exact triple and that one unit less than the new cap on each axis gives `ResourceLimit`, and the evidence file states old, new and the reason. Example: C12 (`ri-2s0.4`), rows `pre` and `computed` in `m4_8_match_boolean_snapshots::installed_boolean_matches` (426,921/48,910/12 to 426,895/48,892/12, and 428,253/49,198/12 to 427,988/49,104/12).
- **Example:** the owner ratified three such moves on 2026-11-23 (review `ci-uaa1`). A2 (`ri-785.1`) charges the O15 `counter` reads with `charge_key`: 2,504/329/6 to 2,562/329/6, graph 187,485 to 187,543 (`rfc_cases::o15_pre_charge_counter_caps_now_give_resource_limit`). B6 (`ri-785.9`) lowers i32 equality, so the O15 expectation flips from `Incomplete` at 1,893/32/1 to `Checked` at 4,577/425/6, and O3 and O5 move with it (`rfc_cases::equality_expectations_exceed_their_incomplete_caps`). C1 (`ri-vgz.3`) admits `callerExclusive`: `Incomplete` at 3,089/284/4 to `Checked` at 9,466/1,677/4 (`propagation::option_caller::exclusive_relay_caller_exceeds_its_parent_caps`).
