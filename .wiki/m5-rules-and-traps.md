# M5 rules, gates and traps inherited from the t3 session

Recorded 2026-11-22 from `/Users/reilley/Repositories/city/notes/m5-ownership-cleanup/session-record.md` §2, §4, §6 (pointers into the t3 transcript are there). These were the t3 coordinator's rules as of 2026-11-21; a later dated entry here or in the [resume log](m5-resume-log.md) supersedes them. Related: [stock-take](m5-stocktake-2026-11-22.md).

## Owner instructions (bead ri-bf3, 2026-11-22)

- Preserve found work: commit or carry forward, never discard.
- Merges only on the draft branch: no rebases, no force pushes. PR #73 stays a draft.
- Finish M5 with `implement-spec` against ri-785, ri-px1, ri-vgz on `implementation/m5-ownership-cleanup`.
- From ri-2s0: do not begin M6; distinguish compiler/MIR evidence from execution; keep ri-2s0 open if any M5 obligation is incomplete.

## Rules the t3 coordinator worked under (as of 2026-11-21)

- Fixed work/entry/depth caps are never raised to make a change fit. Two real proofs (i32 equality root, rigid generic root) were parked for this reason. How legacy whole-graph caps should be re-pinned was never decided: an open policy question for the owner.
- Evidence standard: both installed load modes; targeted and graph-root results; exact and one-short limits; `Incomplete`, `ResourceLimit` and `SourceRejected` are distinct; MIR tests are not Rill execution; native CI jobs stop at the runtime stub and are neither pass nor failure.
- Publish only coherent reviewed batches; keep `## Merge Danger` in the PR body; PR body guard at 64,000 characters.
- A worker's summary is not evidence: verify source and logs.

## Publication gates the t3 coordinator ran per push

1. Paired independent Standards and Spec clearance of the exact commit range.
2. Release isolation: reviewed commits only, on the published head; full M5 passes; both installed modes replayed; historical captures and fixed caps unchanged.
3. Ledger entry in `crates/compiler/tests/m5-acceptance.md`, then `mise exec -- python tools/check_docs.py`.
4. `mise run check`, `mise run build:scaffold`, `mise run check:ci`, `git diff --check <base>..HEAD` on the exact SHA.
5. Guarded push, then PR body section, issue comment, hosted checks.

Commands and timings: `session-record.md` §6. Full M5 binary took 5.5 to 9 min; `mise run check` 5 to 9 min, on 2026-11-21.

## Traps

Reviewers flagged these repeatedly (commits listed in `session-record.md` §4 "Known traps"):

- A new proof path that does work before rejecting an unsupported shape turns `Incomplete` into `ResourceLimit` at old caps.
- `BTreeMap` probes keyed by `ModuleKey.path` must be charged (`charge_key` in `ownership/graph.rs`).
- Transparent groups `((x))` lose receipts or skip depth charges.
- Dead or infeasible code must not grant authority or suppress `R0103`.
- Cached call determinations reused after their premise changed.

Process:

- Focused tests miss cap regressions; only a full M5 run on the published base finds them.
- Usage limits kill the lead and all workers at once; workers die with uncommitted files.
- Each worker needs its own `CARGO_TARGET_DIR`; disk filled on 2026-11-20 (ENOSPC).
- Bespoke replay scripts lived in `/tmp`, not in the repository, as of 2026-11-21.
- Merging I into D re-raises the conflicts of merge-back `fbaad71` semantically (`91ce9cd` on D is a squash of I's `b29693f`, `08c8cf9`), even though `git merge-tree` reported I into D textually clean on 2026-11-22.
