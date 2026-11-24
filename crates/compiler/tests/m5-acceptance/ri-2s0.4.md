# Boolean match `pre` and `computed` check with a receipt (ri-2s0.4, C12, 2026-11-23)

Review `ci-uaa1` SP-F9 found two valid programs of review `e519e598` rejected with a false `R0103` and pinned as "known limitations" in [`installed_boolean_matches`](../m4_8_match_boolean_snapshots.rs). Both are now `Checked` with a receipt, in both installed load modes. This corrects the rows `pre` and `computed` and the sentence "false diagnostics, pinned … as known limitations" in the ledger section "Boolean match review counterexamples, 2026-11-23" ([`m5-acceptance.md`](../m5-acceptance.md), ri-785.2). That section's other rows stand. This is M4 Result use, not an ownership proof, and no Rill program ran.

## What changed (`analysis/typed/result_use`)

- `computed`: `match (*(if (gate) &flag else &flag)) { true if flag => …, false if !flag => …, _ => {} }`. A Boolean scrutinee with no typed source place used to mint a fresh copy on its own temporary, so the guards `flag` and `!flag` never met it. `ValueUse::Scrutinee` now marks that scrutinee. When its read resolves through an alias to a place with no value, `read_place` mints the copy at the aliased place and installs it on the temporary. Each `if` alternative carries its own alias and gets its own copy.
- `pre`: `let before = flag; … match (flag) { _ => before }`. The copy of `flag` was taken before the match began tracking it. A read of an unknown Boolean *local* inside a wildcard match whose source is a *parameter* now looks, on demand, at the written binding of that local (`parameter_copy_source`). If the binding is `let name = parameter;`, with no `mut` (an owned parameter and an ordinary `let` are immutable, RFC 0007 "Ordinary let bindings and owned parameters are immutable"), a whole `bool` read, optionally grouped in the pattern or the initializer, then the two hold one value wherever the name is in scope. The read installs the source's value, or a fresh copy of it.
- An eager variant (track every `let x = boolParameter;` as it is bound) was built first and rejected: it moved the published caps of `fixed-list` (+97 work, +27 entries) and `retaining-rc` (+37, +13) in `legacy-caps.tsv`. The demand lookup moves no published cap.

The demand lookup finds the declaration with the same binary search as `declaration_at`, then scans forward to the first node that spans the name and is not part of its pattern (nodes follow their children, so that is the binding). Every step is charged. Nothing runs before the free screens: the read is a whole `bool` of a `Binding` local, and a wildcard source is a parameter.

## Tests

[`installed_boolean_matches`](../m4_8_match_boolean_snapshots.rs), both load modes, exact and independently one-short work, entries and depth give `ResourceLimit` with no receipt. Red on the parent (`7002855e`): the flipped rows fail with `SourceRejected` where `Checked` is asserted (log `red.log`).

| Row | Outcome | Old (parent) | New |
| --- | --- | --- | --- |
| `pre` | `SourceRejected` `R0103` → `Checked`, receipt | 426,921 / 48,910 / 12 | 426,895 / 48,892 / 12 |
| `computed` | `SourceRejected` `R0103` → `Checked`, receipt | 428,253 / 49,198 / 12 | 427,988 / 49,104 / 12 |

Rule 11, third case (ticket's mandated flip): the reason is the flip above. Both caps **move down**, because the rejected route also paid for the `R0103` report. So the old cap cannot assert `ResourceLimit`: it still holds and gives `Checked` with room. The test asserts exactly that (`installed` with the old triple as the fixed cap, outcome `Checked`). The ticket's wording "the old cap asserts `ResourceLimit`" assumes a higher cost; it does not apply to a cost that falls. No cap went up.

Siblings (rule 2), same template at the parent in a `git worktree add` at `7002855e`:

| Sibling | Outcome | Parent | Here |
| --- | --- | --- | --- |
| `pre`, bare control | `Checked` | (new) | 324,672 / 48,105 / 12 |
| `pre`, `let before = (flag);` | `Checked` | (new) | 324,704 / 48,117 / 12 |
| `pre`, `let (before) = flag;` | `Checked` | (new) | 324,698 / 48,117 / 12 |
| `pre`, arm body `(before)` | `Checked` | (new) | 324,742 / 48,122 / 12 |
| `computed`, scrutinee `((*(if …)))` | `Checked` | (new) | 325,778 / 48,356 / 12 |
| `pre`, `let mut before = flag;` | `SourceRejected` `R0103` | 324,698 / 48,123 / 12 | 324,709 / 48,123 / 12 |
| `pre`, `let before = other;` | `SourceRejected` `R0103` | 324,698 / 48,123 / 12 | 324,712 / 48,123 / 12 |
| `pre`, source is `let mut current = flag;` | `SourceRejected` `R0103` | 324,529 / 48,133 / 12 | 324,529 / 48,133 / 12 |
| `computed`, `if (gate) &flag else &other` | `SourceRejected` `R0103` | 325,928 / 48,413 / 12 | 325,886 / 48,394 / 12 |

Two siblings, the mutable binder and the other parameter, cost 11 and 14 more work than at the parent. **This departs from rule 2 and needs a ruling.** The lookup that rejects them is the cost of finding and reading the written binding. Reading `ExpressionFacts.place` does not tell a `mut` local from an immutable one (both are `Writable`; screening on it made `pre` itself fail), and the initializer that separates the other parameter is not paid for before the lookup. Their outcome does not change (still `R0103`, never `ResourceLimit` at default limits), and no published cap covers them. The test pins their exact numbers here and states the parent numbers. The other two siblings keep their parent caps, asserted by running at them. The stdlib sets depth 12 for every row, so grouping shows in work and entries, not depth; the depth axis one short still gives `ResourceLimit`.

**Second departure.** Rule 11 and the ticket expect the old cap to assert `ResourceLimit`. Both flipped rows got cheaper, so the old caps give `Checked`; the test asserts that instead.

**Superseded (2026-11-24, review `ci-0mdr` ST-19, SP-7).** "Needs a ruling" above was ruled: the chief of staff granted the siblings as a rule-1 exception on 2026-11-23 (`chief-rulings-3.md`, item 2; `compiler-metering.md` rule 1). The ruling of 2026-11-24 13:46 AEDT (Q4) then fixed the mutable binder and the mutable source: both are `Checked` with a receipt and left the exception list ([`ri-px1.11.2.md`](ri-px1.11.2.md)). Only `pre: copy of another parameter` still pays before it rejects. The row "`pre`, source is `let mut current = flag;`" above is `Checked` now, not `SourceRejected`.

Published caps left unchanged, all green: `m4_8_scoped_join_caps`; `m5::legacy_caps` (the M4 17-source baseline stays 372,246 / 54,059 / 12); `m5::rfc_cases`; `m5::graph::inventory`; `m4_8_callable_once`; `m4_8_result_use`; `m4_8_ownership` (the 372,246 baseline). `cargo fmt --all -- --check`, `mise run check:docs`, `git diff --check` and `cargo clippy --no-default-features -p compiler --all-targets -- -D warnings` are clean. Logs under `~/.t3/evidence/rill/m5-build-2026-11-22/ci-xt9h/`. Hosted CI was not run, by city instruction; the merger watches the integration branch.

Commits: the fix and tests (see the branch `m5-ownership-cleanup/r-c12-ri-2s0.4`), merged with the base tip.

Not established: a copy of a parameter that is not a whole `bool` parameter (a field, a mutable local source, a `let` source), a `let` source of an unknown Boolean, or a tuple or record pattern that binds the copy still gets the old `R0103`. The computed route covers a scrutinee that reads through an alias to a place; any other computed scrutinee is unchanged. Nothing here proves ownership cleanup or runs Rill.
