# ri-2s0.9 (ci-o11j): O22 `dispose(view)` of a `&string` ends only that descriptor, 2026-11-24

Status: built; the chief of staff accepted the O21 sibling flip (ruling below); the replacement witness is added (ci-bwnu).

## Reading

RFC 0007 line 525: `dispose(view)` ends that named descriptor value. Another copied descriptor can still retain the backing owner. Disposing the owner while that copy remains needed is R0203. A descriptor end is not a resource disposal, so no `dispose` family winner exists or is selected.

## Plan (written first; recorded here)

- M4 (typed): a new `DataExpression::DescriptorEnd`, formed in `canonical_call_step` (`scalars/formatting.rs`) for the prelude `dispose` called with exactly one shared `&string` argument. It selects no goal, so the `FamilySelection` obligation is not raised. `canonical_scalar_call` admits only that shape (`&string`, not `&mut`), so every other call takes the path it took before. No new lookup runs before the gate.
- M5: `WrittenDisposal::Descriptor` in `ownership/build.rs` (a named, unprojected local only; a projection stays `Pending`) lowers to `Action::Dispose`, then `Operation::EndDescriptor` in `flow.rs`. It takes the existing `Dispose` path: the operand is moved, so a later read is R0200, and `retire_loans` ends only that descriptor's loans. No `BuiltinDispose` edge and no `closed_release.rs` change: nothing is released. The copied descriptor keeps its loan, so `dispose(owner)` still reaches the existing R0203.
- `&mut string` is not admitted; it stays on its previous path.

Rejected alternative: `implement dispose(value: &string): void {}` in `stdlib/core.rill`. It enlarges the pinned prelude for every graph: O22 source 0 typed became 327,772 / 48,005 / 12 and `m5::legacy_caps` failed three tests. Reverted before any commit.

## Tests

`rfc_cases::o22_descriptor_disposal_ends_only_that_descriptor_and_owner_disposal_rejects` (renamed; both installed load modes; the `installed()` child checks it ran one test). Red on the base: the new source-0 expectation (typed `Checked`) failed with `Incomplete`. Then green.

| Source | Roots | Result |
|---|---|---|
| 0 | `aliased` (`dispose(view); dispose(owner); let last = other`) | `R0203` at `dispose(owner)`, related `&owner`; 2,303 / 237 / 4 |
| 0 | `ended` (`dispose(view); let last = other`) | Checked with MIR; 2,383 / 264 / 4 |
| 1 (unchanged) | `aliased` | `R0203`, 2,105 / 144 / 4; typed 322,883 / 47,652 / 12; graph 186,359 / 4,240 / 5 |
| 2 (new) | `released` (`dispose(view); dispose(owner)`) | Checked; 2,244 / 217 / 4 |
| 2 (new) | `reused` (`dispose(view); let again = view`) | `R0200` at `view`, related `dispose(view)`; 2,116 / 155 / 4 |

Whole results (each root and the whole graph also replayed at the exact cap and one short on every axis):

- Source 0: typed `Checked` 325,422 / 47,912 / 12; graph `SourceRejected` 188,285 / 4,569 / 5.
- Source 2: typed `Checked` 325,124 / 47,814 / 12; graph `SourceRejected` 188,456 / 4,447 / 5.

`pre_equality_graph` for sources 0 and 2 (187,969 / 4,435 / 5 and 188,140 / 4,313 / 5) is derived as graph minus 316 / 134 (the ri-785.9 rule), not measured. The test only shows that those caps give `ResourceLimit`.

## Cap moves (rule 11, third case: mandated flip)

- O22 source 0 typed: old `Incomplete` 325,919 / 47,848 / 12 became `Checked` 325,422 / 47,912 / 12. The old cap gives `ResourceLimit` (`O22_PRE_DESCRIPTOR_END`, asserted in both modes, no receipt, no findings). It trips on entries (47,912 > 47,848); work fell.
- No change: `m5::legacy_caps`, `m5::graph::inventory`, `m4_8_scoped_join_caps`, `m4_8_match_boolean_snapshots`.

## Rule 4: O21 siblings flip, accepted (chief ruling 2026-11-24 06:50 AEDT, ci-bwnu)

Ruling: `notes/m5-ownership-cleanup/review/chief-rulings-4.md`, item 1 (city repository), accepted under rule 11's authority line. The O21 siblings write `dispose as release` and call `release(view)` on a `&string`. O22 admits `dispose` of a `&string` as a descriptor end, so the second obligation (`FamilySelection` at `release(view)`) resolves and O21's R0500 alone remains, which is the outcome RFC 0007 demands for O21. Both siblings become `SourceRejected` (R0500 only); the flip comes directly from O22's mandated admission.

| Sibling | Old (`Incomplete`) | New (`SourceRejected`) | Reason |
|---|---|---|---|
| 1 (`release(view)` in the replacement body) | 324,633 / 47,683 / 12 (parent 325,035 / 47,684 / 12) | 324,760 / 47,779 / 12 | O22 admits `dispose` of a `&string`; the second obligation resolves, R0500 alone remains |
| 2 (`release(view)` in `ended`) | 325,896 / 47,704 / 12 (parent 326,297 / 47,705 / 12) | 326,030 / 47,803 / 12 | same |

The parents (`O21_SIBLINGS_PARENT`) now give `ResourceLimit`; the test asserts it.

## Replacement witness for R-O21's rule-2 point (chief's condition)

O21 source 3 keeps a witness that a second obligation still leaves the module `Incomplete`. The replacement's body disposes a `&mut string`: `let mut text = "a"; let view = &mut text; release(view);`. O22 admits only a shared `&string`, so `release(view)` here still records `obligation FamilySelection` and `pending TypedInput` at `release(view)`, plus the R0500.

| Source | Result | Typed cap (exact; one short on each axis gives `ResourceLimit`) |
|---|---|---|
| 3 | `Incomplete` | 324,633 / 47,683 / 12 |

The test (`rfc_cases::o21_guard_release_replacement_reports_r0500`) pins it in both installed load modes at the exact cap and one short on work, entries and depth, as it does for every `Outcome::Typed` row. Source 3 has no parent: it is new, not a moved sibling.

## Not established

`ri-082` (`dispose` of `Option<string>`), `&mut string` disposal, a projected descriptor (`dispose(record.view)`), and Rill execution. MIR only.
