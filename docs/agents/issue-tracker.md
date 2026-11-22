# GitHub issue tracker

Use `gh` for issues and specifications in `Reilley64/rill`. Read a ticket's current body and relevant decision comments before editing it. Preserve concurrent updates.

**PRs as a request surface: no.**

## Wayfinding

The map (ri-kl0) owns destination and completion criteria. Its GitHub sub-issues are the work items. [RFC conventions](../rfcs/README.md) define proposal status and evidence; ri-48g integrates the complete RFC set. Drafting can proceed on settled requirements while incomplete designs remain linked blockers.

- Label the map `wayfinder:map` and children `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling` or `wayfinder:task`.
- Link children with native sub-issues. If unavailable, use a map task list and `Part of #<map>` in the child.
- Use native dependencies as the completion gate. `issue_dependencies_summary.blocked_by` counts open blockers.
- Select an open, unassigned child with no open blocker. Follow map order unless the user gives another priority.
- Claim it with `gh issue edit <n> --add-assignee @me` as the session's first issue write.
- On resolution, publish the decision/evidence, close the design task and add a short linked summary to the map. Preserve separate implementation and validation gates.

## Native API identifiers

Dependency and sub-issue APIs need the numeric database id, not the issue number or node id:

```sh
gh api repos/Reilley64/rill/issues/<n> --jq .id
gh api --method POST repos/Reilley64/rill/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>
gh api --method POST repos/Reilley64/rill/issues/<map>/sub_issues -F sub_issue_id=<child-db-id>
```

If dependencies are unavailable, use `Blocked by: #<n>, #<n>` and check each referenced issue's state.

GitHub shares issue and PR numbers. Resolve an ambiguous number with `gh pr view`, then `gh issue view`. For routine CRUD and label operations, use the CLI's current help rather than a copied command catalogue.
