# Triage states

Triage roles are bead statuses in this rig's store, not labels. Set one with `gc bd update <id> --status <status>`, or create a ticket in it with `--status`. Category roles (`bug`, `enhancement`) stay labels.

| Status | Category | Meaning |
| --- | --- | --- |
| `needs-triage` | frozen | A maintainer needs to evaluate the issue. |
| `needs-info` | frozen | The reporter needs to provide information. |
| `ready-for-agent` | active | The work is specified for an autonomous agent. |
| `ready-for-human` | active | A human needs to implement the work. |
| `wontfix` | done | The project does not plan to act on it. |

Frozen statuses are hidden from `gc bd ready` and the default `gc bd list`; query them with `--status`. Wayfinding tickets take their state from their type: `ready-for-human` for the map and HITL tickets, `ready-for-agent` for AFK tickets. Claiming moves a ticket to `in_progress`; closing moves it to `closed`.
