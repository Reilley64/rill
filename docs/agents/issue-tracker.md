# Beads issue tracker

Issues and specifications live as beads in this rig's store (prefix `ri-`). Use `gc bd` for all operations; pass `--rig rill` only to create and list, because bead IDs route themselves. Read a ticket's current body and relevant decision comments before editing it. Preserve concurrent updates.

Beads migrated from GitHub keep their old number as `external_ref` `gh-<n>`, and each migrated comment starts with its GitHub comment ID. Recorded evidence and fixtures still cite those numbers; resolve one with `gc bd list --rig rill --status all --json | jq '.[] | select(.external_ref == "gh-<n>") | .id'`.

**PRs as a request surface: no.**

## Operations

- Create: `gc bd create "<title>" --rig rill --body-file <file> -t <task|bug|feature>`, with `-l <labels>` and `--parent <id>` as needed.
- Read: `gc bd show <id>`, and `gc bd comments <id>` for the discussion.
- List: `gc bd list --rig rill --status open --json`, filtered with `--label` or `--parent`.
- Block: `--deps blocked-by:<id>` on create, or `gc bd dep add <blocked> <blocker>`. A ticket is unblocked when every blocker is closed.
- Comment: `gc bd comment <id> "<text>"`. Label: `gc bd label add <id> <label>` / `gc bd label remove <id> <label>`.
- Close: post the explanation with `gc bd comment`, then `gc bd close <id> --reason "<short reason>"`.
- Edit a body: write it to a file, then `gc bd update <id> --body-file <file>`.

## Wayfinding

The map (ri-kl0) owns destination and completion criteria. Its child beads are the work items. [RFC conventions](../rfcs/README.md) define proposal status and evidence; ri-48g integrates the complete RFC set. Drafting can proceed on settled requirements while incomplete designs remain linked blockers.

- Label the map `wayfinder:map` and children `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling` or `wayfinder:task`.
- Create children with `--parent ri-kl0`, and use native dependencies as the completion gate.
- Select from `gc bd ready --rig rill --parent ri-kl0 --unassigned --sort oldest`. Follow that order unless the user gives another priority.
- Claim it with `gc bd update <id> --claim` as the session's first bead write.
- On resolution, publish the decision/evidence as a comment, close the design task and add a short summary with the bead ID to the map's body. Preserve separate implementation and validation gates.
