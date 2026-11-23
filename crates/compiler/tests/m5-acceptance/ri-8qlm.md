# ri-8qlm: O21 guard-release replacement is rejected at collection, 2026-11-23

Ticket ri-8qlm (gap-analysis K3, RFC 0007 case O21; review `ci-uaa1` SP-F1, ri-px1.10 #3), brought into M5 by the owner's decision of 2026-11-23 19:18 AEDT. RFC 0007 line 523: "Reject a non-trusted replacement during collection, R0500, even without a call."

## Change

Before: `implement release(value: LockGuard<Payload>): void { }` reported the R0500 overlap. The typed completion stayed `Incomplete`, because the scalar pass checked the body as a pending family role and left `obligation FamilySelection` at `{ }`.

Now:

- The scalar pass receives the ownership overlaps (`IntegratedInputs::ownership_overlaps`). A pending implementation that an overlap names is rejected, and its role is decided rather than pending. Its body is still checked, but it records no `FamilySelection` obligation and publishes no checked body (`scalars.rs`, `run` and `check_named_body`).
- `completion::finish` treats the root of an implementation that an overlap names as accounted for, as it does a root that encloses a finding (`overlap_rejected` in `typed/completion.rs`). The existing `protected_violation` flag already marks the report rejected.

Screen before charge (metering rule 1):

- The scalar pass spends `step(overlaps.len())` only for a pending implementation and only when an overlap exists.
- `overlap_rejected` returns without charging when there is no overlap, and runs only after a root has neither a checked body nor a finding.

A program without an ownership overlap does no new work.

## Test

`m5::rfc_cases::o21_guard_release_replacement_reports_r0500` runs the installed compiler in both load modes. It was red on the base (`Incomplete` vs `SourceRejected`, in the child run) and is green now. O21 has no typed receipt, so the test has no targeted query or ordinary graph root to assert. It pins the typed report, as for every other `Outcome::Typed` row:

| Source | Typed outcome | Signals | Exact cap |
| --- | --- | --- | --- |
| 0, O21 (`{ }`) | `SourceRejected` | `overlap R0500` at `implement release(value: LockGuard<Payload>): void { }`, related: `mutex.rill` `implement<T> dispose(value: LockGuard<T>): void { releaseGuard(value); }` (the required provider it would replace). No obligation | 324,267 / 47,658 / 12 |
| 1, sibling: the replacement's body leaves a second obligation | `Incomplete` | `obligation FamilySelection` and `pending TypedInput` at `release(view)`, plus the R0500 | 324,633 / 47,683 / 12 |
| 2, sibling: O21 plus an unrelated function with an obligation | `Incomplete` | `obligation FamilySelection` and `pending TypedInput` at `release(view)`, plus the R0500 | 325,896 / 47,704 / 12 |

For each source and mode, the test replays the exact cap with the same outcome and statistics. One short on work, entries and depth, each independently, gives `ResourceLimit` with no receipt.

The signal line for an ownership overlap now names its related item: the provider of the protected scope with the same owner, family and rule. Only O21 has an overlap signal in `rfc_cases`.

## Caps (metering rule 11, ticket's mandated flip)

This is the third case of rule 11, named by the ticket before the diff. The owner's decision authorises it.

| Subject | Old | New | Reason |
| --- | --- | --- | --- |
| O21 typed (`rfc_cases`, source 0) | 323,823 / 47,493 / 12, `Incomplete` | 324,267 / 47,658 / 12, `SourceRejected` | `finish` no longer stops at the body's obligation. It now walks the pending uses, declaration issues and conversion families, and accounts for every body root (one entry per checked body) before it returns `SourceRejected` |

The old cap is pinned as `O21_PRE_REJECTION`. In both modes it gives `ResourceLimit`, with no receipt and no body finding.

Siblings (rules 1 and 2): caps measured at the parent `7002855e` in a separate `git worktree add`, running this test file against the parent's source:

| Sibling | Parent (`7002855e`) | Candidate |
| --- | --- | --- |
| 1 | 325,035 / 47,684 / 12, `Incomplete` | 324,633 / 47,683 / 12, `Incomplete` |
| 2 | 326,297 / 47,705 / 12, `Incomplete` | 325,896 / 47,704 / 12, `Incomplete` |

The parent had the same signals plus the `{ }` `FamilySelection` obligation. The candidate is lower on every axis (one obligation fewer). `O21_SIBLINGS_PARENT` asserts that each sibling, at its parent cap, is still `Incomplete` with the candidate's statistics.

No other cap moved. `m5::legacy_caps`, `m5::graph::inventory` and every other `rfc_cases` row pass unchanged, as do the binaries listed below. The whole graph is not touched: O21 has no receipt.

## Local checks

Logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-mhpj/`:

| Check | Result | Log |
| --- | --- | --- |
| `m5 -- rfc_cases::o21` on the base, test first | red, as expected | `red-o21.log` |
| `m5 -- rfc_cases::o21` | 1 passed | `green2-o21.log` |
| `m5 -- rfc_cases` | 26 passed | `m5-rfc_cases.log` |
| `m5 -- legacy_caps` | 2 passed | `m5-legacy_caps.log` |
| `m5 -- graph::inventory` | 1 passed | `m5-graph__inventory.log` |
| `m5 -- graph::scalar` (has an R0500 row) | 10 passed | `m5-graph__scalar.log` |
| `m5 -- disposers` | 9 passed | `m5-disposers.log` |
| `--test m4_8_ownership`, `m4_8_primitives`, `m4_8_list` (overlap tests) | 1 passed each | `m4_8_*.log` |
| `--test m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`, `m4_8_callable_once` | 1 passed each | `m4_8_*.log` |
| `cargo clippy -p compiler --all-targets -D warnings`, `cargo fmt --all --check` | clean | `clippy.log`, `fmt.log` |

## Not established

- This covers ownership protection overlaps (`ownership_overlaps`). Collection and primitive protected overlaps also report R0500, and this change does not credit them. Neither does it credit an overlapping implementation whose contract cannot be formed: that path still records `FamilySelection` at the declaration. O21 needs neither path.
- The R0500 overlap is not a body finding. Its related item is derived from the protection scope in the report. No diagnostic renderer consumes `ownership_overlaps` yet.
- Correction to `ri-px1.10.md` (K3 row and the third "Limits" entry): O21 is no longer `Incomplete`. It is `SourceRejected` with R0500, closed by this ticket.
