# O16 List delivery evidence and remaining join checks

The [owner approval](https://github.com/Reilley64/rill/issues/66#issuecomment-7040596504) allows a bounded static check for the canonical two-job operation. This increment does not admit that operation. It checks delivery cleanup for one installed List-backed leaf frame.

## Checked source

[`installed_list_backing_on_budget_delivery`](m5/cancellation.rs) moves the incoming `List<i32>` to `numbers`, takes two shared views, and uses both after the loop. On the base `817c329`, it had an ordinary ownership result but no enabled delivery proof. At `fdb6904`, both installed modes produce these results in the targeted query and the graph's enabled-frame judgment:

| Checkpoint | Checked terminal cleanup |
| --- | --- |
| Entry, before the move | End `again`, dispose `input` with the selected installed List implementation and bridge. |
| Reached loop backedge | End `second`, then `first`, dispose `numbers` once, then end `again`. No result or normal suffix. |

The ordinary loop exit still reaches the source `dispose(numbers)` once. A second witness disposes `input` before the loop. Its backedge delivery does not dispose it again. A frame with an ordinary call and a loop keeps its delivery judgment unresolved. Entry-only List readers keep their ordinary-only contracts. The check reuses initialized ownership states and the compiler's lexical cleanup walk. It does not acquire another List owner, clone a child proof, or add a checkpoint to an index read or the List disposal bridge.

## Installed blockers

The staged `shared/main.rill` and `erased-slot/main.rill` from `/tmp/rill-o16-d647.4S6RMi/cases` still exit at M3 in both modes with `R0400: import has no public target (import std::routine::join;)`. There is no typed receipt for either. The alias, competing capture, child-local escape, heterogeneous result and caller-backed result fixtures also stop at the import. These are import failures, not `R0201`, `R0202` or a canonical erasure judgment.

Two independent boundaries remain after removing the import from the experiment:

- The selected application control in [`installed_pair_invocation`](m4_8_ownership.rs) has a `CallBoundary` blocker and no typed receipt for `join([() => (&numbers)[0u64], () => (&mut numbers)[1u64]])`. The exact child Result-use proof in `typed/result_use/pairs.rs::pair_child` admits shared capture storage only. For the scoped operation, check this specific exclusive capture's acquisition and Result-free scalar body, then pass the actual exclusive loan to ownership for the overlap judgment. Do not reject all exclusive jobs or turn the ownership conflict into a typing failure.
- The same installed control has a `CallBoundary` blocker for `join([() => { let child = 1i32; return &child; }, () => (&input)[0u64]])`. `result_agrees` admits primitive leaves and their closed tuples, not reference results, and `pair_child` admits only expression or single-return bodies. The scoped operation needs per-slot residence and backing evidence for the exact reference result and local initialization. Ownership must distinguish an escaping child-local dependency from a valid caller dependency. It must not discharge either by assuming `noBorrow`.

The isolated controls are ordinary application implementations, not substitutes for the canonical provider. `35b1dc7` keeps the supported ordinary List pair at its own `CallFacts.list_arrows` and checks both written routes before lowering. The child transfers the scalar, ends its captured view, then returns. The resulting `SynchronousListArrows` dependency still establishes only two synchronous invocations.

The scoped proof still needs a distinct transition on the checked ownership states. It must move both initialized environments at one commit, record admitted child delivery and cleanup edges, preserve both backing dependencies across a deferred wait, and distinguish each child's execution end from cleanup completion. Neither the first staged result nor a stop request permits publication or parent release. The delivered edge must retire only initialized unpublished results and then run checked parent cleanup; reverse completion must publish the two results in source slot order instead. `MaskingAndJoin` stays open. No `routine.rill`, installed role, protection scope, or selected provider was registered.

## Replay at the code pin

At `fdb6904` on top of the reviewed correction `35b1dc7`, eight exact M5 tests and `installed_ownership_requires_the_built_archive` passed serially. They cover both installed modes, target and graph delivery, old fixed and exact/one-short caps, foreign capture rejection, ordinary overload and erased-slot controls. Scoped Clippy, changed-file formatting and `git diff --check` passed. The build used only `/tmp/e76.1PxxJn/target`, with debug info and incremental builds disabled and a short `TMPDIR`.

The same-code executable-relative stage under `/tmp/e76.1PxxJn/join-final` contains the original 17 source files byte-for-byte. In 18 static fixture checks (normal and test in each), the baseline and both application `join` controls kept their typed receipts, with `[Ownership, Execution]` unperformed. The old Result discards, dormant implementation mismatch, non-Boolean expectation, mixed incomplete source, missing archive and corrupt pin kept their prior outcomes. The canonical fixtures had no receipt. The installed ownership regression separately checks the captured 17-key set. Local command logs are `/tmp/e76.1PxxJn/join-parent-red.log` and `/tmp/e76.1PxxJn/join-final-*.log`.

No broad suite, Rill execution, scheduler run, ABI check, or native operation was performed.
