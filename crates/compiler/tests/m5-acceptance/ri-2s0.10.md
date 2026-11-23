# Rule-1 exceptions for nine unsupported siblings (ri-2s0.10, R-O7X, 2026-11-23)

Review `ci-uaa1` found four sites where an unsupported sibling is charged before it is rejected, and pinned at a cap measured on the candidate: ri-785.7 #1 (B3), ri-vgz.7 #1 (C5), ri-785.5 #1 (B2) and, from R-C12, the two siblings of ri-2s0.4 (C12). The chief of staff granted an exception under rule 11's authority line: `chief-rulings-2.md` item 4 (2026-11-23 22:18 AEDT, B3, C5 and B2) and `chief-rulings-3.md` item 2 (2026-11-23 22:30 AEDT, C12), both in `notes/m5-ownership-cleanup/review/` of the city repository. Every route stays `Incomplete` (C12: `SourceRejected` with `R0103`) at default limits; the costs are small; an earlier screen would need new M4 facts. The restructuring is filed as `ri-qlh.4`.

This ticket changes tests, records and docs only. No compiler source, no published cap and no whole-graph row moved except that the new `ignoredOther` fixture has its own graph pins. All results are LLVM-free host analysis in both installed load modes. No Rill program ran.

## What changed

- [`declared_shared.rs`](../m5/captures/declared_shared.rs): a new row, the lent `return other();` wrapper `ignoredOther`, is pinned. `ignored` gains its parent graph cap. A helper `assert_parent_graph_cap` asserts `ResourceLimit`, no finding and no MIR in any root.
- [`iteration_record.rs`](../m5/iteration_record.rs): `mixed` and `custom` assert their parent caps.
- [`m4_8_callable_once.rs`](../m4_8_callable_once.rs): three rows assert their parent caps.
- [`m4_8_match_boolean_snapshots.rs`](../m4_8_match_boolean_snapshots.rs): `installed_over_parent` (a wrapper around `installed`) asserts the parent cap of the two C12 siblings.
- [`compiler-metering.md`](../../../../docs/agents/compiler-metering.md): a dated note under rule 1 lists the exceptions by test name, and a dated note under rule 11's third case records that a mandated flip that lowers the cost asserts the new triple and one short instead of the old cap (example C12, `pre` and `computed`).

## Pins

Exact and one short on each axis are asserted in both modes by each test's existing loop (`assert_incomplete_at_caps`, `assert_graph_incomplete_at_caps`, `limits`, the `check_sources` loop and `installed`). "Parent" is asserted to give `ResourceLimit` with no MIR, no receipt and no finding. Parent values were measured in a scratch `git worktree add --detach` with its own `CARGO_TARGET_DIR`, at the commit the review names, with the same source text and both load modes (see [Measuring](#measuring)).

| Item | Test row | Parent (commit) | Current | Pinned before this ticket |
| --- | --- | --- | --- | --- |
| B3 | `ignored`, targeted | 1,917/55/3 (`a95e04e8`) | 1,922/55/3 | current and parent |
| B3 | `plain`, `ignored` source, graph | 187,611/4,040/3 (`a95e04e8`) | 187,616/4,040/3 before the equality root; 187,932/4,174/5 | current, pre-equality; **parent graph new** |
| B3 | `ignoredOther` (`skipOther(&f)`), targeted | 1,899/46/3 (`a95e04e8`) | 2,395/46/3 (+496) | **new row** |
| B3 | `ignoredOther` source, graph | 186,959/4,049/3 (`a95e04e8`) | 187,771/4,183/5 | **new row** |
| C5 | `mixed` | 2,512/350/2 (`057c5005`) | 2,515/350/2 | current; **parent new** |
| C5 | `custom` | 2,522/358/2 (`057c5005`) | 2,894/419/2 | current; **parent new** |
| B2 | `shared-formal-actual` | 323,599/47,577/12 (`9b93a660`) | 323,600/47,577/12 | current; **parent new** |
| B2 | `shared-result-capture` | 325,321/47,868/12 (`9b93a660`) | 325,337/47,871/12 | current; **parent new** |
| B2 | `shared-unclosed` | 325,701/47,779/12 (`9b93a660`) | 326,165/47,793/12 | current; **parent new** |
| C12 | `pre: mutable binder` | 324,698/48,123/12 (`7002855e`) | 324,709/48,123/12 | current; **parent assertion new** |
| C12 | `pre: copy of another parameter` | 324,698/48,123/12 (`7002855e`) | 324,712/48,123/12 | current; **parent assertion new** |

`ignoredOther` source: `function other(): bool { return true; }`, `function skipOther<F>(job: &F): bool where callable(F, [], bool) { return other(); }`, `function ignoredOther(flag: bool): bool { let f: () => bool = () => flag; return skipOther(&f); }`. The blocker is `skipOther(&f)`, the typed graph is `Checked`, and its graph root is `Incomplete` at that site. Its parent targeted cap gives `ResourceLimit` with no MIR and no finding. The ledger of `ri-px1.11.md` said "about +496"; the measurement is +496 exactly (1,899 to 2,395, entries and depth unchanged). For the owned form (`otherOwned`, `return other();` in a `callableOnce` wrapper) the ledger's "no change needed" stands: it keeps 324,801/47,618/12 at `a95e04e8` and today.

The C5 and B2 parents agree with the numbers in review `ci-uaa1`. The B2 entries at the parent (47,577; 47,868; 47,779) were not in the review; the formal actual keeps 47,577 entries, the Result capture adds 3 (47,868 to 47,871) and `shared-unclosed` adds 14 (47,779 to 47,793).

## Why no paid fact rejects earlier

From the review-fix ledger ([`ri-px1.11.md`](ri-px1.11.md), "Rule-1 sibling costs (O7)"), re-read against the code at `83407ca3`. Function names are stable; line numbers are of this commit.

**B3, ri-785.7 #1: [`call_declared_once`](../../src/analysis/ownership/build.rs) (about 7321-7495).** The free screen (7336-7385) rejects a call that is not the written `&name` of a read-only `() => bool` descriptor, not a plain function call, or is piped, defaulted, selected or joined. A wrapper whose body is `return name();` survives that screen whatever `name` is. The lent branch then finds the wrapper by identity with a charged binary search over the module's ordered declarations (`budget.step`, about 7385; +5 work) and screens the body shape `return <call of a value reference>;`, which `return other();` also matches. Telling `other()` from the formal `job()` needs the typed header: `budget.step(1 + path.len())` (7405), `graph::charge_key` (7410) and the header lookups (about 7410-7495). That is the +496. The `skip` wrapper is told apart only by its body, hence the +5. No fact published before these charges holds the wrapper's body or its callee, so the screen cannot move earlier. The owned once route (`otherOwned`) gets no M4 receipt and stops before this function.

**C5, ri-vgz.7 #1: [`record_text_disposal`](../../src/analysis/ownership/build/iteration.rs) (about 503-520).** `mixed` (a record with an `i32` field) and `custom` (a record with a user `release` disposer) are told apart from `Entry` by the record's field types and its disposer. The field types are in the typed declaration facts (a keyed lookup, rule 3), read by `record_fields`; the disposer is read by `outer_disposer` after `budget.step(fields.len())`. The loop's own facts (its List literal and yielded element type) say only that the element is a record. So the three charges precede the rejecting returns: +3 for `mixed`, +372 work and +61 entries for `custom`.

**B2, ri-785.5 #1: [`lent_call`](../../src/analysis/typed/result_use/once.rs) (about 145-215).** The written-shape screen (about 160-176) is free. The formal actual (`run(&f)` where `f` is a parameter) is told apart only after `signature`, the wrapper-body filter and one charged `expression_facts` probe of the operand (+1). The Result capture is rejected after `lent_descriptor` has charged the residences that find the captured `Result` (+16 work, +3 entries). `shared-unclosed` is rejected after the other caller `open` closes, when the unclosed `run(job)` is found (+464 work, +14 entries): the call is only found by scanning every caller. No M4 fact published before these charges decides any of the three.

**C12, ri-2s0.4: `parameter_copy_source` in [`typed/result_use.rs`](../../src/analysis/typed/result_use.rs).** The demand lookup reads the written binding of the copy before it can tell `let mut before = flag;` or `let before = other;` from the admitted `let before = flag;`. `ExpressionFacts.place` is `Writable` for both a `mut` and an immutable local, so it cannot screen the first (screening on it made `pre` fail), and the initializer that separates the other parameter is not paid for before the lookup. Source: [`ri-2s0.4.md`](ri-2s0.4.md), "Siblings".

An earlier screen needs new M4 facts. `ri-qlh.4` holds that work.

## Red before green

Each parent-cap assertion was written first and passes at the parent numbers measured above; run on a candidate with a lower cost they would fail, which is not the case here (the siblings cost more than their parents). The only first-run failure was my own placeholder for `ignoredOther` (9/9/9), which printed 2,395/46/3 and then 187,771/4,183/5; and the B2 assertion that no discards exist at a `ResourceLimit` report, which failed (a limit report can still carry discards) and was dropped.

## Tests

Local, filtered. Logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-cfcc/`. Each `m5` test reruns itself in an installed child (`installed`); the harness fails when the child ran no test, and each log shows the full-path filter ran one test.

- `m5 -- captures::declared_shared::declared_shared_siblings_keep_their_parent_caps`: passes (`m5-declared-shared-3.log`).
- `m5 -- iteration::record`: three tests pass (`m5-iteration-record.log`), including `record_yield_cleans_the_partial_item_then_the_suffix`.
- `--test m4_8_callable_once` (`private_once_checks_the_actual_shared_pair`): passes (`m4_8_callable_once.log`).
- `--test m4_8_match_boolean_snapshots` (`installed_boolean_matches`): passes (`m4_8_match_boolean_snapshots.log`).

## Measuring

The measuring files are not in the repository. They are at `~/.t3/evidence/rill/m5-build-2026-11-22/ci-cfcc/measure/`: `measure.sh` (a scratch worktree at the parent, `git worktree add --detach`, its own `CARGO_TARGET_DIR`, removed at the end), `measure_snippet.rs` (the `zz_measure` test appended to `m5.rs` at the parent; it prints typed and graph statistics in both modes), the source files (`b3.rill`, `b3other.rill`, `iter.rill`, `b2-*.rill`, `c12-*.rill`) and the outputs (`*-a95e04e8.txt`, `*-057c5005.txt`, `*-9b93a660.txt`, `*-7002855e.txt`). Both load modes agree on every row.

## Not established

- Nothing restructures the screens: that is `ri-qlh.4`. No cap other than the rows above was touched, and no fixed cap moved.
- `ignoredOther` pins the lent `&F` form only. The owned form is unchanged and already pinned (`otherOwned`).
- The full M5 binary, `mise run check` and the native workflow run in hosted CI only.
