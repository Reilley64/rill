# M5 stock-take, 2026-11-22

Taken on 2026-11-22 for bead `ri-bf3` (resume M5 from the t3 session). Related: [rules and traps](m5-rules-and-traps.md), [resume log](m5-resume-log.md), [tickets](m5-tickets.md) (what followed).

## Sources (read these, not this summary, for detail)

All under `/Users/reilley/Repositories/city/notes/m5-ownership-cleanup/`, written 2026-11-22 by two read-only subagents of an earlier chief of staff:

- `session-record.md`: what the t3 coordinator did, per slice, with transcript line and statev2 message pointers. Section 0 explains its pointer notation.
- `git-inventory.md`: every head, dirty worktree and stash compared with D, I and M. Section 5 holds the carry-forward lists.
- `worktrees.tsv`, `branches-unmerged.txt`, `heads-outside-integration.txt`: raw inventories.
- `out-git.txt`: the inventory subagent's summary (two summaries; they differ on REVIEW-ONLY count, 0 vs 5. `git-inventory.md` section 2.4 is the fuller one).

Names used below, as of 2026-11-22: **D** = `origin/implementation/m5-ownership-cleanup` at `982319d`; **I** = local `integration/m5-unpublished-b41b99d` at `f24c5ad`; **M** = `main` at `90525c9` at inventory time.

## Why the t3 session stopped

On 2026-11-21 05:16Z the lead model hit a subscription usage limit; three workers died in the same minute. No handoff was written (`session-record.md` §0).

## Finished (on D)

- As of `982319d`, six of nine slices were closed: ri-4yg, ri-cfy, ri-33m, ri-rmc, ri-9k9, ri-9m8 (`session-record.md` §1).
- The session's last published head was `b5f8187`; `6d2fbe1` and `982319d` above it are docs-only bead-reference commits by Reilley on 2026-11-22.
- Open slices on 2026-11-22: ri-785 (callable captures, erased evidence), ri-vgz (typed propagation exits), ri-px1 (ordered cleanup, protected obligations, final verification; the t3 tracker had it blocked by ri-785). Per-slice state and stated gaps: `session-record.md` §1 and §4.
- Umbrella ri-2s0: no complete graph ownership receipt existed on 2026-11-21 (`session-record.md` §1, §4).

## Unpublished M5 work (not on D), found 2026-11-22

1. **I `f24c5ad`**: 49 commits, local only. 9 duplicate published work, 4 are change+revert pairs, 4 are merge-backs, 32 are unpublished; only the call-retention lane (8 commits) had a clean paired review. Lane table with review verdicts: `session-record.md` §2. The coordinator never treated I as publishable. I merges into D with no textual conflict (`git-inventory.md` §1).
2. **Coordinator worktree** `~/.t3/worktrees/rill/t3-4fc2b3c5-5520e1d991906aec` (on I): 3 uncommitted files, +81 −175, an uncompiled draft fix for Standards blocker `c61a0a14` (O15 `i32` arithmetic). t3 checkpoint `74a88d7` holds the same blobs.
3. **`t3-ade4ca3c-f677be38955c945a`** on `22f2979`: 7 uncommitted files, +370 −55, the forwarding cap-regression fix of task `dd1f1bdd`; focused replay matched, full M5 never run. No snapshot existed anywhere.
4. **`t3-55e7ba7b-7e82ed57997a9ad8`**: 1 uncommitted test file, an interrupted stub (task `a379dbb1`).
5. **`003c414`** `fix/scoped-result-cleanup-admission`: delivered, never reviewed or integrated; merges cleanly onto D+I.
6. **`e13e7e3`** `fix/rigid-call-budget-accounting`: parked with the rigid generic root; conflicts with I unless `d236b15` is restored.

Built on I but M4 Result-use rather than M5 (decision needed whether they belong in PR #73): `bd10617` `fix/m4-boolean-merge-equivalence` (the delivered fix for the blocked Boolean lane) and `e60aef3` `feat/m4-private-callable-once` (M4 half of declared `callableOnce` invocation).

Parked, net zero in I: `i32` equality root (`a6271cc`, reverted `4c114f6`); rigid generic root (`d236b15`, reverted `b923a00`). Pinned by `checkpoint/m5-*` branches.

Not M5, preserve and do not merge: `feat/ri-hw5`, two untracked research notes, three stashes, `backup/main-before-conventional-commits` (`git-inventory.md` §5.5).

## In flight at the stop

Table of unfinished or unintegrated delegated tasks, and the coordinator's last actions: `session-record.md` §3. Inferred next steps: finish and re-review the O15 fix; land the forwarding correction; fix the shared Option relay's Standards blocker; build the exclusive Option relay; review and integrate `e60aef3`, then its M5 half.

## Merge facts

- M into D conflicts only in three docs files: `docs/agents/issue-tracker.md` (modify/delete), `docs/implementation-plan.md`, `docs/research/0026-language-rfc-formats.md` (`git-inventory.md` §1).
- On 2026-11-22 21:23 AEDT local `main` had moved to `98c5aa08` after the inventory was taken at `90525c9`; re-check conflicts at merge time.

## Evidence at risk

On 2026-11-22 most t3 evidence lived in `/tmp` (list in `session-record.md` §0 and §3). See [environment](environment.md) for where it was copied.
