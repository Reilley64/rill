# O16 List delivery evidence and remaining join checks

The [owner approval](https://github.com/Reilley64/rill/issues/66#issuecomment-7040596504) allows a bounded static check for the canonical two-job operation. This increment does not admit that operation. It checks delivery cleanup for one installed List-backed leaf frame.

## Checked source

[`installed_list_backing_on_budget_delivery`](m5/cancellation.rs) moves the incoming `List<i32>` to `numbers`, takes two shared views, and uses both after the loop. At `afd1afd`, this body had ordinary MIR but no enabled delivery proof. At `810f679`, both installed modes produce these results in the targeted query and the graph's enabled-frame judgment:

| Checkpoint | Checked terminal cleanup |
| --- | --- |
| Entry, before the move | End `again`, dispose `input` with the selected installed List implementation and bridge. |
| Reached loop backedge | End `second`, then `first`, dispose `numbers` once, then end `again`. No result or normal suffix. |

The ordinary loop exit still reaches the source `dispose(numbers)` once. A second witness disposes `input` before the loop. Its backedge delivery does not dispose it again. A frame with an ordinary call and a loop keeps its delivery judgment unresolved. Entry-only List readers keep their ordinary-only contracts. The check reuses initialized ownership states and the compiler's lexical cleanup walk. It does not acquire another List owner, clone a child proof, or add a checkpoint to an index read or the List disposal bridge.

The new proof is limited to a top-level `while` whose test reads an immutable Bool parameter and whose body is empty or just `continue`. No other reached conditional, backedge, or ordinary call can contribute a checkpoint in this List proof. The enabled walk must actually reach a backedge: an ordinary CFG backedge alone is insufficient. For example, `while (again) { if (again) { break; } }` cannot reach its backedge when both tests read the same immutable parameter. It keeps ordinary MIR but does not gain an entry-only delivery proof.

## Installed blockers

The staged `shared` and `erased-slot` applications that import `std::routine::join` still exit at M3 in both modes with `R0400: import has no public target (import std::routine::join;)`. There is no typed receipt for either. The alias, competing capture, child-local escape, heterogeneous result and caller-backed result fixtures also stop at the import. These are import failures, not `R0201`, `R0202` or a canonical erasure judgment.

Two independent boundaries remain after removing the import from the experiment:

- The selected application control in [`installed_pair_invocation`](m4_8_ownership.rs) has a `CallBoundary` blocker and no typed receipt for `join([() => (&numbers)[0u64], () => (&mut numbers)[1u64]])`. The exact child Result-use proof in `typed/result_use/pairs.rs::pair_child` admits shared capture storage only. For the scoped operation, check this specific exclusive capture's acquisition and Result-free scalar body, then pass the actual exclusive loan to ownership for the overlap judgment. Do not reject all exclusive jobs or turn the ownership conflict into a typing failure.
- The same installed control has a `CallBoundary` blocker for `join([() => { let child = 1i32; return &child; }, () => (&input)[0u64]])`. `result_agrees` admits primitive leaves and their closed tuples, not reference results, and `pair_child` admits only expression or single-return bodies. The scoped operation needs per-slot residence and backing evidence for the exact reference result and local initialization. Ownership must distinguish an escaping child-local dependency from a valid caller dependency. It must not discharge either by assuming `noBorrow`.

The isolated controls are ordinary application implementations, not substitutes for the canonical provider. `35b1dc7` keeps the supported ordinary List pair at its own `CallFacts.list_arrows` and checks both written routes before lowering. The child transfers the scalar, ends its captured view, then returns. The resulting `SynchronousListArrows` dependency still establishes only two synchronous invocations.

The scoped proof still needs a distinct transition on the checked ownership states. It must move both initialized environments at one commit, record admitted child delivery and cleanup edges, preserve both backing dependencies across a deferred wait, and distinguish each child's execution end from cleanup completion. Neither the first staged result nor a stop request permits publication or parent release. The delivered edge must retire only initialized unpublished results and then run checked parent cleanup; reverse completion must publish the two results in source slot order instead. `MaskingAndJoin` stays open. No `routine.rill`, installed role, protection scope, or selected provider was registered.

## Replay at the code pin

At `810f679`, 21 focused M5 tests and `installed_ownership_requires_the_built_archive` passed, as did scoped Clippy, formatting and `git diff --check`. In the staged fixture the `read` body checks at 3,398 work / 703 entries / depth 5. The installed tests verify exact and independently one-short target and graph limits. A separate capture covers an entry-only reader, an ordinary call before the loop, a local condition, the correlated break above and a loop in a nested scope. All five retain their exact pre-change body budgets; the capture still fits its pre-change graph cap of 191,781 / 4,867 / 6. Each root retains ordinary MIR with unresolved delivery.

The installed ownership regression checks the original 17-key standard-source set. The separately staged baseline produced M4 receipts in both modes, with `[Ownership, Execution]` unperformed; the synchronous `actual-shared` body kept its previous M5 result and limits. Both canonical import controls still had no receipt. Earlier staged comparisons on the List-leaf implementation also preserved Result discards, the dormant implementation mismatch, the non-Boolean expectation, the mixed incomplete source, missing archive and corrupt pin.

No broad suite, Rill execution, scheduler run, ABI check, or native operation was performed.
