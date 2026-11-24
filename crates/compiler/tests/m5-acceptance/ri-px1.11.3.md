# Review ci-0mdr fixes, part C4: screen before charging (ST-2, ST-4, ST-9) (2026-11-24, city bead ci-e8rg)

Child `ri-px1.11.3` of the single fix pass `ri-px1.11` for review `ci-0mdr`. Base `origin/implementation/m5-ownership-cleanup` at `da5bec50` (it holds C1, `ead19c6a`, and its `ResourceLimit` helpers). Branch `m5-ownership-cleanup/r2f-ri-px1.11.3`. Every parent cap below was measured at `7002855e` in a separate `git worktree add` (removed after), with one throwaway probe test that prints each root's completion, statistics and blocker; the probe sources and outputs are in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-e8rg/probes/`. The new tests were written first and were red on the base for the two siblings that moved (`red-base.log`, below). Each `.out` in `probes/` ran on one revision: `parent.out` and `parent3.out` at `7002855e`, `parent2.out` at `f24d825b`, `cand*.out` on the base `da5bec50` before the fix (`probes/README.txt`).

## Findings

| Finding | Status | Where |
|---|---|---|
| ST-2 | fixed | `family_view_shape` (`ownership/build.rs`) now also refuses, from facts already paid for, a call whose written argument is not `&mut List<i32>` and a selection whose implementation is not an `implement` of this module with an empty substitution. Test `rfc_cases::o11_family_view_over_a_non_list_stays_at_its_parent_cap` |
| ST-9, `i32_arithmetic.rs:326`, decided **left** operand | recorded: not reproducible, already at its parent cap | A decided left operand makes the whole `&&` not normal, and in each context tried (a `return` operand, a `let` initializer, an `if` condition, a call argument) the `&&` node's own flow check refuses it before `i32_equality_conjunction`, so `normal_value(left)` never charges for it. Pinned at the parent's numbers by `rfc_cases::decided_conjunction_left_keeps_its_parent_cap`, which passes on the base too (rule 10 cannot be met for this shape) |
| ST-9, `i32_arithmetic.rs:326`, decided **right** operand (found while probing the left) | fixed | The left operand's checks ran, then the right operand was lowered and rejected: the blocker moved from the whole `&&` to the right operand and the root paid 6 work, 3 entries, 1 depth. `i32_equality_conjunction` now stops with `Pending(node)` before its first charge when either operand's flow is not normal (`Build::flows_normally`, the flow test of `normal_value` without its `step`). Test `rfc_cases::decided_conjunction_right_keeps_its_parent_cap`, red on the base |
| ST-9, `i32_arithmetic.rs:202`, `self.place(target)` | **stopped** (no free screen; +1 work, numbers below) | The checks after it (`place.ty`, `literal.ty`, `place.place`, `arithmetic_literal`) cannot fire: an out-of-range or negative literal never reaches it (typed `Incomplete` or a prefix node refused earlier), so moving them ahead of the charge changes nothing. The one reject that is real, `!self.slots[slot].exclusive_field_reborrow`, reads the slot the charge resolves; no paid fact says how a binding was written (there are no parent links in the syntax tree). See "Stopped" |
| ST-4 | **stopped** (no free screen) | See "Stopped". No test added, no cap pinned |

## ST-2: what was measured

At `7002855e`, `viewOf(&mut owned)` stops `Incomplete` at the call; on the base it paid the family probe, the implementation scan and `normal_value` before the formal-type reject. Targeted, then graph (the graph holds the root and the implementation root; typed is unchanged):

| Source | Parent (`7002855e`) root / graph | Base before the fix | Now |
|---|---|---|---|
| `implement viewOf(item: &mut Box): &i32` (`nonList`) | 2,625 / 165 / 3; 188,604 / 4,266 / 5 | 3,397 / 165 / 3 (+772 work); 189,376 / 4,266 / 5 | the parent's |
| `implement viewOf(item: &mut i32): &i32` (`scalar`) | 1,911 / 48 / 3; 187,826 / 4,149 / 5 | 2,683 / 48 / 3 (+772); 188,598 / 4,149 / 5 | the parent's |

Typed statistics are the parent's in both (326,415 / 47,961 / 12 and 325,579 / 47,912 / 12). The test pins each root and each graph exact and one short (`o12::check_cases`, which calls C1's `assert_body_limit` and `assert_graph_limit`), both load modes, targeted query and graph root, blocker `viewOf(&mut owned)`. The admitted O11 caps (3,028 / 202 / 6, 3,316 / 336 / 6, 191,877 / 4,600 / 6) and the four older siblings did not change: no existing `step` or `charge_key` moved.

The review's other two shapes have no sibling to pin: an implementation with binders (`implement editAndView<T>(list: &mut List<i32>)`, also over `List<T>`) and an implementation in another module (`import other::{viewOf}`) are typed `Incomplete` (no receipt), so the ownership builder never sees them. The screen checks the module and the substitution anyway; the binders reject inside `selected_signature` stays behind its charge and cannot be reached. A `&mut List<bool>` formal stops earlier, at the `List<bool>` parameter, at the same caps before and after.

## ST-9: what was measured

Both sources hold `function stop(): never { while (true) {} }` and `function id(x: i32): i32 { return x; }`. Parent, then base before the fix, root targeted (work / entries / depth), graph:

| Source | Parent | Base before | Now |
|---|---|---|---|
| `id(if (true) stop() else 1i32) == b && a == b` (decided left) | 1,897 / 42 / 3, blocker the whole `&&`; 187,466 / 4,246 / 5 | equal | equal |
| `a == b && id(if (true) stop() else 1i32) == b` (decided right) | 1,897 / 42 / 3, blocker the whole `&&`; 187,466 / 4,246 / 5 | 1,903 / 45 / 4, blocker `id(if (true) stop() else 1i32) == b`; 187,472 / 4,249 / 5 | the parent's |

The same decided left operand as a `let` initializer, an `if` condition, a call argument and a `return` operand gave the parent's numbers in each context before the fix. The admitted conjunctions (`o16_join_order`, `rfc_cases` O16, the `id(a) == b && a == b` control) are unchanged: the new test passes through the same `normal_value(left)` step.

Red on the base: a throwaway `git worktree add` of this branch's merged tip with `crates/compiler/src` checked out from `origin/implementation/m5-ownership-cleanup` (removed after), running the three new tests (`red-base.log`; the failing children print the assertion at `rfc_cases.rs:287`, so they ran). `decided_conjunction_right_keeps_its_parent_cap` failed with graph statistics `187472 / 4249 / 5` against `187466 / 4246 / 5`; `o11_family_view_over_a_non_list_stays_at_its_parent_cap` failed with `189376 / 4266 / 5` against `188604 / 4266 / 5`; `decided_conjunction_left_keeps_its_parent_cap` passed there, as recorded.

## Stopped, for the chief: ST-4 and ST-9 at `i32_arithmetic.rs:202`

ST-4. `borrowed_temporary_field` and `binding_temporary_borrow` reject a call result whose record has a custom `Dispose` only after `self.slot(call_node, ..)`, because the answer comes from `DisposerLookup::select` (`outer_disposer`), which projects and ranks the `dispose` family and is charged. No paid fact names a custom disposer: `TypedReport::ownership_protection()` is a per-owner list that a scan would have to charge, and `primitive_protection()` covers primitives only. The standing rule-1 exception covers only a record or enum field or variant list, not a disposer selection, and the cost is far over its bound (c) of 150 work. Measured with `type Tracked = { x: i32 }; implement dispose(value: Tracked): void {}; function make(): Tracked { return Tracked { x: 1i32 }; }`:

| Root | Parent (`7002855e`) | Base |
|---|---|---|
| `within`, `return (&make()).x;` | 1,875 / 27 / 3, blocker `(&make()).x` | 2,252 / 90 / 6 (+377 work, +63 entries, +3 depth), same blocker |
| `bound`, `let v = &make();` | 1,891 / 34 / 4, blocker `make()` | 2,263 / 97 / 4 (+372 work, +63 entries), same blocker |

Reordering cannot restore the parent numbers (the record screen, the signature check and the slot all precede the only test that rejects), and pinning the base's numbers is forbidden by the brief, so no test or code was added. For the chief: either a new M4 fact carrying a type's custom disposer (the `ri-qlh.4` class of fact), or a ruling that grants these two siblings an exception with the numbers above. The pinned test the review names, `o12::disposer_record_results_keep_their_parent_caps`, is not written. The MUST KILL comment at `build.rs` 3028-3029 (the disposer note in `binding_temporary_borrow`) is not restructured.

`i32_arithmetic.rs:202`. For a body that holds a binding written `&mut record.field` and another `&mut i32` binding that is not (`let d = p;` or `let d = &mut *p;`, then `*d += 1i32;`), `self.place(target)` resolves the path before the slot flag rejects the local descriptor. Measured against the parent of ri-fmsk (`f24d825b`, where the route did not exist): `viaOther` 2,245 / 103 / 2 at the parent and 2,246 / 103 / 2 on the base (+1 work; graph 186,741 to 186,742), `viaReborrow` 2,249 / 105 / 2 and 2,250 / 105 / 2 (+1; graph 186,865 to 186,866); the blocker stays `*d += 1i32;`. No free screen exists (the binding's initializer is not reachable from the target), so this +1 is reported, not pinned: it is the rule-1 question for the chief if a +1 matters. At `7002855e` itself both bodies are typed `Incomplete`, so that parent has no ownership cap for them.

## MUST KILL sites in this area, for the C7 bead (chief item (a))

None is restructured here: `build.rs` 7951-7975 and 8271-8273 of `81d317b3` (the screen and charge ordering in `family_retaining_shape` and `call`; ST-2 changed `family_view_shape`, a different function), `build.rs` 3028-3029 (ST-4, stopped) and `i32_arithmetic.rs` 191-197 (the local-descriptor note). Line numbers drift after merges.

## Caps

No published cap moved. Every pinned cap in the filters below passes with no edited expectation; the three new tests pin the parent's numbers.

## Checks (local, logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-e8rg/`)

After merging the base tip `a4e56651` (C2, `ri-px1.11.1`), at `e2eff3b6`:

- `m5-filters-merged.log`: `--test m5 -- rfc_cases retained legacy_caps graph::inventory o16_join_order o15 o05`: 156 passed.
- `m4_8-merged.log`: `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`, `m4_8_callable_once`: pass.
- `fmt2.log`, `clippy2.log`, `docs2.log`: `cargo fmt --all -- --check`, `cargo clippy -p compiler --tests -- -D warnings` and `mise run check:docs` clean; `git diff --check` and `git status --short | grep target/` empty.
- `red-base.log`: the red run above. Before the merge the same filters passed on the branch alone (`m5-filters.log`, 151 passed; `m4_8.log`, `fmt.log`, `clippy.log`, `docs.log`).

## Not established

- The decided-left and the `&mut List<bool>` pins pass on the base too; they pin the parent's numbers, they do not reproduce a regression.
- The foreign-module and binders shapes of ST-2 are not exercised by any test (typed stops them).
- This is compiler MIR evidence; no Rill program ran.
