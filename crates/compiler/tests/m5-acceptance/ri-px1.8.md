# Scope-exit guard release in the ordinary graph (ri-px1.8, 2026-11-23)

This is ticket D3, with the field case of `ri-77t` that the planner added to it. The ordinary graph verified a `ProtectedRelease` edge only for a written `dispose(lock)`: a single release at its call (`c668ad4`). A scope-exit release now closes the edge too, when every release on that edge is cleanup of an ordinary function's parameter, either the whole parameter or one field of a flat, non-generic application record. The verifier reads the parameter's declared type, or the field's type in the record's header. That type must be `LockGuard<i32>`, its clear required guard-release scope must name this provider, and its native representation must be the guard payload and source. The checks before the first charge use only the MIR and syntax already paid for. A guard in a local binding, a nested projection, an active disposer epilogue and any root without a function header are rejected there, at no cost.

At the parent (`36f1cf15` with D1's branch), the implicit-only graph `function implicit(lock: LockGuard<i32>): void {}` was `Incomplete` at exactly 185,222 / 3,993 / 3, blocked at `{}` ([`implicit_guard_release_closes_the_ordinary_root`](../m5/graph/closed_release.rs)). In both installed modes its root now checks with the targeted MIR, one release of `lock: LockGuard<i32>`. The targeted query is unchanged at 1,890 / 31 / 1.

Whole-graph re-pins under rule 11 (D2 (b)). Entries and depth do not move.

| Source | Old | New | Per-root work |
| --- | --- | --- | --- |
| implicit-only (`closed_release.rs`) | 185,222 / 3,993 / 3, `implicit` `Incomplete` | 185,246 / 3,993 / 3 | `implicit` +24 |
| `mixed_wrapper.rs` | 197,974 / 5,550 / 5 | 198,204 / 5,550 / 5 | `mixedWrapper` +102, `mixedGrouped` +102, `guardBesideView` +26 |

The work is the verifier's own charges. For each root: the cleanup block's actions (1, 2 or 5); the parameter scan (1 or 2); the 8 ownership-protection scopes; the `core.rill` family key (10); and the 4 native representations. A field adds its record-header probe: the `main.rill` key times 7 probes, which is 70, plus the record's 3 fields. Depth stays at 3 and at `mixedGrouped`'s 5. Each new cap is exact; each independently one-short cap gives `ResourceLimit`, and so does each old cap.

No targeted cap moved. A guard moved into a local binding and released at scope exit stays outside. It keeps the caps measured at the parent ([`implicit_guard_binding_release_keeps_its_parent_caps`](../m5/graph/closed_release.rs)): targeted `Checked` at 1,988 / 76 / 3; the graph is `Incomplete` at 185,441 / 4,038 / 3, blocked at the body with its `ProtectedRelease` edge, and each one-short cap gives `ResourceLimit`.

Roots flipped on purpose, with no other expectation edited: `implicit` in `written_guard_release_closes_only_the_ordinary_caller_edge` (its written-release assertions are unchanged); `dropGuard` in `active_disposer_is_not_a_source_root_or_a_protected_provider_proof`, where the provider's source root stays `Incomplete`; `mixedWrapper`, `mixedGrouped` and `guardBesideView` in `mixed_wrapper.rs`, whose graph roots now carry the whole targeted action list. `protected_fields_keep_their_obligation_through_disposer_cleanup` now also asserts that the `ordinary(value: Plain)` and `mixedWrapper` graph roots are `Checked`, with the targeted cleanup. These are ordinary cleanup judgments, not cancellation delivery or Rill execution.

Commits: `b7e9d0f2` (verifier, tests and caps), on top of D1's branch `032b390b`, merged with the base tip. This record was moved here from `m5-acceptance.md` when the base adopted per-ticket records.

Not established: a guard released implicitly from a local binding, a nested projection or a custom-disposer epilogue is still not a verified edge, and those roots stay `Incomplete`. The provider source roots stay open. No Rill program ran.
