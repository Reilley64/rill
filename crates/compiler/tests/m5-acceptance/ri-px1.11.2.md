# Review ci-0mdr fixes, part C3: mutable binder and mutable source check (2026-11-24, city bead ci-e9ek)

Part of the single fix pass of review `ci-0mdr` (second review of `81d317b3`); C3 is `ri-px1.11.2` (Q4, SP-7). Base `da5bec50` (the integration tip that contains C1, `ead19c6a`), branch `m5-ownership-cleanup/r2f-ri-px1.11.2`. The records child (`ri-px1.11.6`) consolidates this file into the section "Review ci-0mdr fixes" of `ri-px1.11.md`.

## Findings

| Finding | Status | Where |
|---|---|---|
| Q4, SP-7 (`pre: mutable binder`) | fixed | `analysis/typed/result_use/values.rs`; test `m4_8_match_boolean_snapshots::installed_boolean_matches`, row `pre: mutable binder` asserts `Checked` with a receipt, both load modes. Red before the change: the row failed with `SourceRejected` where `Checked` is asserted (`red.log`) |
| Q4, SP-7 (`pre: mutable source`) | fixed | same test, row `pre: mutable source` |

This corrects the "Genuine rejections stay R0103" group of `installed_boolean_matches` and the rows `pre: mutable binder` and `pre: mutable source` of [`ri-2s0.4.md`](ri-2s0.4.md) and [`ri-2s0.10.md`](ri-2s0.10.md), which pinned both as `SourceRejected`. This is M4 Result use; no Rill program ran.

## What changed

`parameter_copy_source` (the on-demand lookup that finds the wildcard source a read local copies, added by `ri-2s0.4`) used to accept only `let name = parameter;` with no `mut`, a parameter source. It now also accepts:

- a `let mut` copy, and a `let mut` source, when nothing writes that local. `never_written` scans the nodes after the binding up to the first block that contains it (the local's whole scope, loop bodies included). An `Assignment` whose left side, or a `&mut` whose operand, is a place rooted at the local counts as a write, and so does one whose place has no facts. References resolve through the typed place root, not the name, so shadowing cannot fool it. An arrow that only reads the local needs no screen: a scalar capture copies (RFC 0007, "Mutating an implicitly copied scalar capture changes the captured copy"), and an assignment or `&mut` inside the arrow is a node in the same scan.
- a source that is a local `let` binding, not only a parameter. The lookup that finds the binding (`written_binding`) is the old code, factored out unchanged; the source's mutability decides whether `never_written` runs. An immutable local source therefore checks as well (`pre: immutable local source`, pinned below). The brief names only the mutable one; the same screen covers both, and rejecting the immutable one would need a rule of its own.

Order (metering rule 1): every free screen and every old paid screen runs first. The new scans run last, after the initializer is matched to a wildcard source, so `pre: copy of another parameter` (initializer is not a source) returns before any new charge. The gate widening (a local source also passes the free screen) is free.

## Tests

[`installed_boolean_matches`](../m4_8_match_boolean_snapshots.rs), both installed load modes, exact and independently one-short work, entries and depth give `ResourceLimit` through `crate::limit::assert_typed_limit`.

Flipped, now `Checked` with a receipt (rule 11, third case: the chief's mandated flip):

| Row | Old (`da5bec50`), `SourceRejected` `R0103` | New, `Checked` | Old cap now |
|---|---|---|---|
| `pre: mutable binder` | 324,709 / 48,123 / 12 | 324,714 / 48,105 / 12 | `ResourceLimit` (work +5) |
| `pre: mutable source` | 324,529 / 48,133 / 12 | 324,568 / 48,115 / 12 | `ResourceLimit` (work +39) |
| `pre: immutable local source` (new row, same source with `let current`) | 324,529 / 48,133 / 12 (measured at `a4e56651`, below) | 324,518 / 48,115 / 12 | holds: the parent cap still gives `Checked` (work and entries both lower), asserted as the row's fixed cap |

Parent measurement of the third row (chief of staff condition 1, `chief-rulings-14.md`, 2026-11-24 14:50 AEDT): the fixture of `pre: immutable local source` run at `a4e56651`, C3's base, in its own `git worktree` with its own `CARGO_TARGET_DIR`, both load modes. Outcome `SourceRejected`, one discard, `R0103` at `Err<bool,i32>(61i32)`, no pending, no receipt; triple 324,529 / 48,133 / 12, the same numbers as the parent's `pre: mutable source`. This is a fix of a false `R0103`, not a change of a correct outcome. Log and raw lines: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-sybf/parent-measure.log` and `parent-measure.txt`.

The first two are entries lower and work higher than the rejection they replace, so the old cap asserts `ResourceLimit` (installed with `installed_over_parent`). No cap went up for a published root.

Rejecting twins (new rows, same source with a write between the copy and the match; the genuine `R0103`, one discard, no pending):

| Row | Outcome | Exact |
|---|---|---|
| `pre: mutable binder reassigned before the match` (`let mut before = flag; before = !flag;`) | `SourceRejected` `R0103` | 325,493 / 48,441 / 12 |
| `pre: mutable source reassigned after the copy` (`current = !flag;` after `let before = current;`) | `SourceRejected` `R0103` | 325,489 / 48,455 / 12 |

Twins of `pre: immutable local source` (chief of staff condition 3). The immutable local `current` copies the parameter `flag`, and neither can be written: RFC 0007 makes an owned parameter and an ordinary `let` immutable. A write of the source between the copy and the match is therefore not a Result-use rejection. It is a source error, one `ReadOnly` body finding at the assignment statement, so the screen never sees a written source (`Incomplete`, no receipt, no `R0103`, no `Checked`). Both are pinned in both load modes by `installed_read_only` with the finding, the completion and the exact numbers, replayed at the exact cap:

| Row | Write | Outcome | Exact |
|---|---|---|---|
| `pre: immutable local source reassigned after the copy` | `current = !flag;` | `Incomplete`, `ReadOnly` at `current = !flag;` | 324,920 / 48,237 / 12 |
| `pre: immutable local source, its parameter written after the copy` | `flag = !flag;` | `Incomplete`, `ReadOnly` at `flag = !flag;` | 324,753 / 48,171 / 12 |

Each is also pinned one short on work, entries and depth (`ResourceLimit`, no finding published: `ri-68mu` is on the base). A source that can be written is the case `pre: mutable source reassigned after the copy` above: an immutable local `before` copying the `let mut` local `current`, which is written after the copy; it is a genuine `R0103` (325,489 / 48,455 / 12). Neither twin is accepted.

Mutable-borrow twins (`flip(&mut before)`, `flip(&mut current)`): `Incomplete`, one pending `Storage` at the borrow, no receipt; the borrow of a Boolean local is unsupported before this change, so these pin that the screen never turns it into `Checked`.

| Row | Exact |
|---|---|
| `pre: mutable binder lent mutably before the match` | 325,898 / 48,178 / 12 |
| `pre: mutable source lent mutably after the copy` | 325,746 / 48,188 / 12 |

Mutation check: with `never_written` forced to `true`, the reassigned-binder twin gives `Checked` where `SourceRejected` is asserted, so the scan is what rejects it (not run with the exact pins; run on the branch and reverted).

Siblings that keep their numbers (rule 2): `pre: copy of another parameter` 324,712 / 48,123 / 12 (parent cap 324,698 / 48,123 / 12 still gives `ResourceLimit`) and `computed: alias selects another place` 325,886 / 48,394 / 12 (parent cap 325,928 / 48,413 / 12 holds), unchanged. The `pre` group (bare control and the three grouped forms), `pre` and `computed` keep their exact numbers (324,672; 324,704; 324,698; 324,742; 426,895; 427,988).

## Caps

No published cap moved: `m5::legacy_caps` and `m5::graph::inventory` (4 tests), `m5 rfc_cases retained` (123 tests), `m4_8_scoped_join_caps`, `m4_8_callable_once`, `m4_8_result_use` and `m4_8_match_boolean_snapshots` all pass with no edited expectation. The M4 17-source baseline stays 372,246 / 54,059 / 12 (`legacy_caps`). Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-e9ek/`.

## Standing rule-1 exception

None added. One use removed: `pre: mutable binder` left the rule-1 exception note of `docs/agents/compiler-metering.md` (it no longer rejects), and so did `pre: mutable source`, which the 2026-11-23 ruling on R-C12 had listed with it. The list now reads eight siblings. `pre: copy of another parameter` stays under the exception.

## Not established

- A copy or source written by any means that is not an `Assignment` or `&mut` in its scope (none exists in RFC 0007's mutation rules as read here); a pattern-bound mutable local with a tuple or record pattern; a field of a record, or a whole non-`bool` local. All keep the old `R0103`.
- A write after the read: the scan covers the whole scope after the binding, so by construction `let mut before = flag; match (flag) { _ => before }; before = !flag;` keeps the old `R0103`, the conservative direction. No test runs it.
- Nothing here proves ownership cleanup or runs Rill.
