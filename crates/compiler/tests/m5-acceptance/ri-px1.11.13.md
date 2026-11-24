# Re-review fix C16: C3 gate fourth sibling (2A-F1) and the ri-j6fm-cause forms (2A-F2) (2026-11-24, city bead ci-011a)

Findings 2A-F1 and 2A-F2 of focused re-review `ci-9qga` (Subagent A), ticket `ri-px1.11.13`, spec `ri-2s0`. Base `2e13bc6e`, branch `m5-ownership-cleanup/r4f-ri-px1.11.13`. Parent for every number below: `81d317b3`, measured in its own `git worktree` and `CARGO_TARGET_DIR` (`probe-parent.log`). "Base" is `2e13bc6e` (`probe-base.log`), "fixed" is this branch with the first commit (`probe-fixed.log`). This is M4 Result use; no Rill program ran. **The ticket is closed `blocked`:** the fix the ticket prescribes moves one of C12's three pinned rows, and the ticket says to stop and report if that happens. Two commits are on the branch; see "Decision for the planner".

## Item 1 (2A-F1): mutable copy of the source, then lent

Source: `MUTABLE_SOURCE` with `let before = current;` replaced by `let mut before = current; let r = &mut before;`. Test row `pre: mutable copy of the source, then lent mutably (ri-px1.11.13, 2A-F1)` in `m4_8_match_boolean_snapshots::installed_boolean_matches`, pinned exact and one short, parent cap asserted to give the row's outcome.

| | Work / entries / depth | Outcome |
|---|---|---|
| Parent `81d317b3` | 324,747 / 48,205 / 12 | `SourceRejected`, `R0103` at `Err<bool,i32>(61i32)`, no pending |
| Base `2e13bc6e` | 324,758 / 48,205 / 12 (+11) | same |
| Fixed (either commit) | 324,747 / 48,205 / 12 | same |

Red first: with `values.rs` at the base, the row fails on its exact pin, `left: (324758, 48205, 12)`, `right: (324747, 48205, 12)` (`red-item1.log`, before the source change; `red-all-reverted.log`, with all rows and the source change reverted). Both load modes give the same numbers (probe lines in `probe-fixed.log`).

Change, first commit: in `parameter_copy_source` (`result_use/values.rs`) the test `last_written == id` moves before `written_binding(id)` and no longer asks whether the copy is mutable. Second commit: the same early return, only when no wildcard source is a parameter (below).

## The stop: C12's row moves under the first commit

The ticket requires C12's three rows, the admitted forms and the ri-j6fm pins X2 to X4 not to move. Under the first commit exactly one pinned row moved: `pre: mutable binder lent mutably before the match` (`pre("let mut before = flag; flip(&mut before);", "before")`, `Incomplete`, `Storage` at `*value = !*value;`, one `R0103` discard): 325,886 / 48,178 / 12 (its pin, equal to its parent) now 325,874 / 48,178 / 12 (-12). Its source is a parameter, so `written_binding` was paid there at the parent too.

What I checked: I changed only that row's pin to 325,874 and ran `m4_8_match_boolean_snapshots` once (`probe-after-12move.log`): the whole test passed. So in that binary, both load modes, no other pin moved: the three admitted forms (324,714 / 48,105 / 12, 324,568 / 48,115 / 12, 324,518 / 48,115 / 12), C12's other two rows, X2, X3, X4, the two `ReadOnly` twins, the new 2A-F1 row and both 2A-F2 rows. I did not run the `m5` filters, `m4_8_scoped_join_caps`, `m4_8_callable_once` or `m4_8_result_use` for this branch (blocked; see below), so nothing is claimed about them. The pin edit was reverted.

## Decision for the planner

* **A, first commit** (`last_written == id` before `written_binding`, as the ticket says): 2A-F1 at its parent cap; C12's lent-binder row moves -12 (below its parent). Needs the chief to accept re-pinning that row to 325,874 / 48,178 / 12 (its parent cap 325,886 still gives the same outcome).
* **B, second commit** (the same return only when no `wildcard_sources` entry is a parameter): 2A-F1 at 324,747 / 48,205 / 12 and C12's lent-binder row stays 325,886 / 48,178 / 12. With B the whole `m4_8_match_boolean_snapshots` binary passes with every pin as written in this file's rows and the existing ones (`variant-B-narrow.log`). B adds a source-kind condition that the ticket did not ask for, so it is the planner's choice. To take A, drop the second commit and re-pin the one row.

## Item 2 (2A-F2): pinned under the ri-j6fm exception, widened

Chief of staff ruling 2026-11-24 17:54 AEDT (`chief-rulings-21.md`, mailed by the planner): the exception covers the source or the copy paying its `never_written` scan when another local is written or lent before the match. Both forms are `SourceRejected`, `R0103` at `Err<bool,i32>(61i32)`, no pending, in both load modes, at the parent and now.

| Row | Parent `81d317b3` | Base `2e13bc6e` | Fixed (A and B) |
|---|---|---|---|
| (a) `let before = current; let r = &mut current; let mut z = flag; z = !flag;` | 326,063 / 48,685 / 12 | 326,153 (+90) | 326,153 / 48,685 / 12 |
| (b) `let mut before = current; let r = &mut before; let mut z = flag; z = !flag;` | 326,063 / 48,685 / 12 | 326,113 (+50) | 326,113 / 48,685 / 12 |

(b) did not drop by 11 after item 1: its copy is not `last_written` at the match (`z` is), so the early return is not reached. Each row is pinned exact and one short (`ResourceLimit` through `crate::limit::assert_typed_limit`) and its parent cap is asserted to give `ResourceLimit`. They are named in the rule-1 exception note of `docs/agents/compiler-metering.md`. There is no red step for these two rows: they pin cost that exists at the base and that the ruling accepts, so there is no fix to revert.

## Logs

`~/.t3/evidence/rill/m5-build-2026-11-22/ci-011a/`: `red-item1.log`, `red-all-reverted.log`, `probe-parent.log`, `probe-base.log`, `probe-fixed.log`, `probe-after-12move.log`, `variant-A-reorder.diff`, `variant-B-narrow.diff`, `variant-B-narrow.log`. The probe rows were a temporary early-return loop in the test, not committed.

## Not established

Only `m4_8_match_boolean_snapshots` was run. The gate checks (`m5` filters, `m4_8_scoped_join_caps`, `m4_8_callable_once`, `m4_8_result_use`, clippy), the merge of the newest integration tip and the push were not run, because the ticket is blocked at the decision above. I swept no other form for a cost over its parent.
