# Owner policies

Policies the owner set for work in this rig. Follow them in every brief and report.

## Branches and merges

1. Merge only on integration branches. Never rebase and never force push. Preserve found work: commit it or carry it forward.
2. Keep PR #73 a draft until the chief of staff (`rill/staff.chief-of-staff`) marks it ready.
3. Commit nothing to `main`, except docs-only changes the owner has allowed the chief of staff to land. Put any other change on a pushed branch.

## Checks

4. Full test suites and full validation gates run only in CI. Run targeted checks locally: [development.md](development.md) item 3.

## Decisions

5. The chief of staff decides what comes up while a spec is built: planner and subagent questions, review rulings, scope within the spec, deferrals to named beads and changes to rules set for the work. Planners send questions they cannot decide to the chief, not to the owner.
6. The chief escalates to the owner a change to a goal or to what done means, final acceptance of a spec or milestone, irreversible or public actions (landing on `main`, a release, marking a PR ready, deleting branches or data), and spending or city configuration.
7. Post each decision with its reason on the bead it concerns.

## Agents and messages

8. `implementer` does every build. Re-brief a blocked ticket to a fresh `implementer` with the blocker answered, or split it. There is no `senior-implementer`.
9. Wait with `gc session wait --on-beads <ids> --note '<next action>'` (add `--any` to act on the first) and end the turn. Never wait with `sleep` or a polling loop.
10. Mail does not wake a session. Send a message that needs the recipient to act (a question, a ruling, a red CI result) with `gc mail send --notify`. Completion reports need no `--notify`.
