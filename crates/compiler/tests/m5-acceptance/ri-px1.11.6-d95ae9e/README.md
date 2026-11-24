# The `d95ae9e` re-derivation of A7's old numbers (ST-18, R-2)

Transcript only, not reproduced by this ticket. These are the files that produced the three A7 old numbers quoted in [`ri-px1.11.md`](../ri-px1.11.md) ("Re-derivations (R-2)") and in the ledger section "Live parent `Err` before a fresh `Result` join": `liveParentError` `Incomplete` at `pair`, 3,449 / 320 / 3; `retiredParentError` `Incomplete` at the `join(...)` call, 3,457 / 326 / 3; whole graph `Incomplete` 197,064 / 4,761 / 5, in both load modes, at `d95ae9e`. They were copied here from `~/.t3/evidence/rill/m5-build-2026-11-22/ci-paai/measure/` (city bead ci-paai) on 2026-11-24 by `ri-px1.11.6`; the original directory is outside the repository.

| File | What it is |
| --- | --- |
| [`live-parent-err.rill`](live-parent-err.rill) | The source measured (`lpe.rill` there; the fixture `legacy-caps/live-parent-err` holds the same functions). |
| [`measure_snippet.rs.txt`](measure_snippet.rs.txt) | The throwaway test `zz_measure`, appended to `tests/m5.rs` of a scratch worktree (`.txt` so that cargo does not see it). It prints each mode's typed statistics, the graph's statistics and, per named function, `lower_body`'s outcome, statistics, blocker and graph root. |
| [`measure.sh`](measure.sh) | The command: `git worktree add --detach` at the commit, append the snippet, run `cargo test --locked --no-default-features -p compiler --test m5 -- zz_measure --exact`. It names `/tmp/ci-paai/measure_snippet.rs` and a worktree path that no longer exist; use the snippet above. |
| [`lpe-d95ae9e.txt`](lpe-d95ae9e.txt) | The output at `d95ae9e`, with the numbers above. |

Nothing here was re-run for this record: the numbers are a transcript of the original run. Rule 10 asks that a re-derivation be checkable from the repository; this is the closest the repository holds, and it is not a test that fails without a fix (the old numbers are history, not a pin). The pins that hold today are `scoped_join::fresh_result::live_parent_error_reaches_the_join_at_its_repinned_cap` (the old cap gives `ResourceLimit`).
