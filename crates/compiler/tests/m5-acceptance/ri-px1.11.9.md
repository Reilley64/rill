# Re-review fix C12: C3's gate must not charge unsupported siblings before rejecting (2026-11-24, city bead ci-x01v)

Finding A-F1 of focused re-review `ci-b4b2` (Standards, subagent A), ticket `ri-px1.11.9`, spec `ri-2s0`. Base `d87e5dd2`, branch `m5-ownership-cleanup/r3f-ri-px1.11.9`. Parent for every number below: `81d317b3`, the revision before C3 (`ri-px1.11.2`) widened the gate of `parameter_copy_source`. This is M4 Result use; no Rill program ran.

## Finding

| Finding | Status | Where |
|---|---|---|
| A-F1 (rules 1 and 2): C3's gate let a local `Binding` source through, then paid `written_binding(source)` and the `never_written` scan before returning `None` for three siblings | fixed | `analysis/typed/result_use.rs`, `analysis/typed/result_use/values.rs`; test `m4_8_match_boolean_snapshots::installed_boolean_matches`, rows below, both load modes |

Red before the change: with the source change reverted, each row fails on its exact pin (`red-*.log` in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-x01v/`, one log per row because the test stops at the first failed assertion): `pre: mutable source reassigned after the copy` gives (325,489, 48,455, 12) where (325,403, 48,455, 12) is asserted; `pre: mutable binder lent mutably before the match` gives (325,898, 48,178, 12) where (325,886, 48,178, 12) is asserted; `pre: mutable source lent mutably after the copy` gives (325,746, 48,188, 12) where (325,702, 48,188, 12) is asserted.

## The screen

`Check` keeps `last_written`, the root of the latest checked assignment or `&mut` it evaluated. Both handlers already read the place facts for that operand (`expression_facts`), so recording the root costs no `step`, `reserve`, `charge_key` or allocation. `parameter_copy_source` reads it twice, each time before a charge it used to make:

1. In the gate that the widening opened, a wildcard source that is a local `Binding` counts only when it is not `last_written`. A source the body already wrote or lent no longer passes, so the row returns at the same free point as at the parent (rows 1 and 2).
2. Right after `written_binding(id)` reports a mutable copy, a copy that is `last_written` returns `None`. The parent returned at the same point for every mutable copy (row 3).

`last_written` is set for an assignment only when the assignment has facts (`checked`). A write the source language forbids (an assignment to an immutable local) has none, so the immutable-source twins keep the numbers C3 pinned: `pre: immutable local source reassigned after the copy` stays 324,920 / 48,237 / 12 and `... its parameter written after the copy` stays 324,753 / 48,171 / 12. Setting the slot for them cost +76 work and +55 entries on the first twin (measured while building this change), because they stopped being copies.

The screen returns exactly what `never_written` would have returned: a write rooted at the local that was evaluated is an `Assignment` or `&mut` inside the local's scope, which the scan counts. It adds no rejection the scan would not have made.

## Per row: parent, now, outcome, blocker

Both load modes give the same numbers. Parent measured at `81d317b3` in its own `git worktree` and `CARGO_TARGET_DIR` (`probe-parent.txt`); "before" is the base `d87e5dd2` (`probe-now.txt`); "after" is this branch (`probe-fixed.txt`). The probe source is `px119_probe.rs.txt` (not committed).

| Row | Parent | Before | After | Outcome (parent, before, after) | Blocker (parent, before, after) |
|---|---|---|---|---|---|
| `pre: mutable source reassigned after the copy` | 325,403 / 48,455 / 12 | 325,489 (+86) | 325,403 / 48,455 / 12 | `SourceRejected`, `R0103` at `Err<bool,i32>(61i32)`, no pending, all three | none |
| `pre: mutable source lent mutably after the copy` | 325,702 / 48,188 / 12 | 325,746 (+44) | 325,702 / 48,188 / 12 | `Incomplete`, one `R0103` discard published, no receipt, all three | `Storage` at `*value = !*value;`, all three |
| `pre: mutable binder lent mutably before the match` | 325,886 / 48,178 / 12 | 325,898 (+12) | 325,886 / 48,178 / 12 | `Incomplete`, one `R0103` discard published, no receipt, all three | `Storage` at `*value = !*value;`, all three |

The blocker of each lent row is the same at the parent and now (the re-review did not check it): the pending site is the write inside `flip`, not the call that lends. The test asserts the site, the one pending and the one discard, so a different `Incomplete` fails.

Each row is pinned exact and independently one short on work, entries and depth (`ResourceLimit` through `crate::limit::assert_typed_limit`), and each parent triple is run as a fixed cap that still gives the row's outcome. The row `pre: mutable binder reassigned before the match` (325,493 / 48,441 / 12) equals its parent and now carries it as a parent cap too.

## Admitted forms: pins unchanged

| Row | Exact |
|---|---|
| `pre: mutable binder` | 324,714 / 48,105 / 12 |
| `pre: mutable source` | 324,568 / 48,115 / 12 |
| `pre: immutable local source` | 324,518 / 48,115 / 12 |

Also unchanged: `pre: copy of another parameter` 324,712 / 48,123 / 12 (under the rule-1 exception, parent cap `ResourceLimit`), `computed: alias selects another place` 325,886 / 48,394 / 12, the two `ReadOnly` twins above, and a never-written mutable copy behind an unrelated pending (`X1` in the probe: 325,983 / 48,189 / 12, `Incomplete`, no `R0103`, unchanged).

## Caps

No published cap moved. The three rows now equal their parents, so no exception to rule 1 is needed and none is added to `docs/agents/compiler-metering.md`. The two earlier pins of the lent rows (325,898 and 325,746) and the first row's (325,489) were new rows of C3 with no parent; they are replaced by the parent numbers, which are lower.

## Not established

- Only the latest write or lend is remembered. A body that writes the source and then writes or lends a different local before the match still pays the scan for the source and rejects at `R0103`: the probe's `X2` (`let before = current; current = !flag; let mut z = flag; z = !flag;`) costs 326,183 / 48,707 / 12 against 326,097 at the parent (+86), the same as before this change. Closing it needs a per-local "written" fact (an M4 fact, the pattern of `ri-qlh.4`) or a reservation per write, which would move the caps of every program with an assignment. Filed as `ri-j6fm` (needs-triage), not pinned or fixed here.
- A `&mut` of an immutable local is a source error with no signal at that handler; it is recorded as a write like any `&mut`. No test covers it.
