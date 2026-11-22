# Stack capacity and recursion cases

Read the source examples in [`prototypes/20-stacks-recursion.rill`](../../prototypes/20-stacks-recursion.rill).

These are approved design inputs for ri-636 and [ADR 0014](../adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md). The companion has 14 intended-valid expectations, checked by inspection only. Validation of this companion did not run a Rill compiler/runtime, measure stacks, run native probes or execute a Wasm pipeline. Small successful calculations cannot establish constant-space behavior or safe overflow containment.

The examples follow confirmed ri-kty enums and ri-vtw error metadata, rather than the earlier singleton/union/automatic-propagation-trace sketches. The new sized-spawn path and signature illustrate ri-8n7's interface. They must preserve ordinary spawn's requirements for owned captures, invocation mode, noBorrow and sendable.

## Intended-valid source cases

- `countDown` uses a scalar accumulator with a semantically eligible self-tail transfer. Numeric overflow remains checked; this is not a stack guarantee.
- `even`/`odd` use mutually recursive eligible transfers, not merely self-recursion optimization.
- `forward` invokes an indirect read-only callable whose descriptor and environment belong to an earlier caller. Its outer example's local borrow is deliberately distinct from forward's own eligibility.
- `viaStage` uses current qualified enum patterns and payloads with ordinary numeric contexts, not old kind-tag unions or literal parameter overloads.
- `iterativeTotal` traverses the input with for/of and an explicit-width mutable accumulator.
- `afterCall` uses valid ordinary recursion with real work after the recursive call. Its intended case uses only a small depth.
- `readLocal`/`copyLocalOwner` make valid calls that require retaining local storage or an owner. A final-looking call does not permit tearing that storage down. Optimizations may remove equivalent work but do not create a portable proper-tail promise.
- `startWithCapacity`/`createdAndJoined` use a fixed-arity helper with u64 bytes, owned noncapturing work and joined stop. They do not assert that the worker must run before cancellation or that a test physically observed stack reclamation. Handle metadata can outlive completed execution storage.

## Configuration contract, not executable CLI tests

Illustrative future invocations:

```sh
# Decimal byte counts: 1 MiB default, 8 MiB entry.
rill build --stack-size 1048576 --entry-stack-size 8388608 main.rill
rill test --stack-size 1048576 prototypes/20-stacks-recursion.rill
# For a validated Wasm backend, not a claim that this pipeline works today:
rill build --target wasm32-wasip1 --stack-size 1048576 --saved-stack-size 1048576 main.rill
```

CLI quantities above are decimal bytes; no additional unit parser or source literal suffix is settled here. Source uses u64 bytes, for example `8u64 * 1024 * 1024`. The larger example must fit documented target limits; no universal min/max is assumed. Validate upward alignment/guard arithmetic and the effective capacity before reserving it.

| Request or event | Expected contract |
| --- | --- |
| No overrides, native | Managed entry/spawn has a usable default of 1 MiB. It does not inherit the OS main-stack size. |
| Entry override | It affects entry only; ordinary children still use the program default. |
| One sized spawn | It sets only that routine's addressable capacity. Descendants do not inherit it. |
| Scoped helper creates children | Children use the program default unless a specified interface explicitly overrides it. |
| Valid unaligned capacity | Check upward alignment; do not silently clamp downward. |
| Zero/below-minimum/excessive/unrepresentable effective capacity in build settings | Reject before execution |
| Invalid dynamic u64 request to sized spawn | Panic when executed, not a new Result return shape |
| Reservation failure | Fatal resource exhaustion under ordinary infallible spawning |
| Future physical backing unavailable/host OOM | May terminate without a language report |
| Wasm saved budget | Program-wide default 1 MiB per routine, separate from addressable region |
| Saved-state option explicitly supplied to native | Reject rather than ignore |
| Host engine stack exhaustion | Fatal to Rill execution; no guest cleanup/resumption or forced host-process termination promise |

Exhausting either Wasm managed budget uses bounded best-effort fatal reporting. Increasing one guest budget does not enlarge the other or the host engine stack. These bounds do not guarantee a recursion depth or native residency and are not total-memory quotas. Workers, the scheduler and blocking jobs have separate stacks. Native frames executing on a managed region consume that region.

## Eligibility and rejected shortcuts

A proper-tail transfer cannot need eliminated caller storage or post-call disposal/joins. Retaining these obligations makes a final-looking call ordinary; it is not generally a source type error. Do not introduce a new tail keyword or require every ordinary recursive program to be rejected.

Schematic guard case (library spellings remain ri-8n7):

```ts
function guarded(counter: &Mutex<u64>, n: u64): u64 {
  let held = lock(counter);
  return countDown(n, 0);
} // held must be disposed after the call; this call is not promised proper-tail.
```

A callee that reacquires the same mutex can observe that the guard is still held. Do not dispose it before the call or at its last explicit mention to force TCO. Explicit disposal before transfer is a source-program choice and remains subject to outstanding loans/child joins.

Likewise, do not eliminate a caller-local slot that a callee, native job or closure needs. Incoming borrowed sources that belong to an earlier frame differ from newly borrowed local storage. ABI restrictions cannot become undocumented semantic ineligibility. If the compiler cannot lower a genuinely eligible call, it must diagnose the unsupported call or target rather than silently accumulate frames. Development and optimized builds have the same guarantee.

ri-vtw adds no automatic propagation history. Direct unchanged failure forwarding has no such trace obligation, but all actual control/storage/cleanup work still counts. Explicit infix caused-by wrapping or Err construction can require work after a call; early error return is not by itself proof of a proper-tail call. Complete explicit causes are error data. Normal panic traces may omit eliminated calls; no unbounded diagnostic shadow stack is required.

## Bounded cleanup is not constant total memory

Standard traversal/container internals and compiler-generated default cleanup must not recurse once per input depth/link. Use loops, worklists or eligible transfers. Allow only small explicitly bounded recursive helpers. Large user payload frames or an already-full caller can still overflow; arbitrary callbacks/user disposers can recurse.

A cleanup worklist can retain depth-dependent pending operations because those operations are not an eligible tail chain with no obligations. This is not tracing GC or permission to hide a genuinely eliminated activation chain on the heap. Saved activation accounting cannot be evaded by relabeling an activation allocation as worklist data; ordinary worklist/program/cause graphs are not total-memory-quota subjects.

Preserve observable disposal order, ownership/initialization state, borrowed-source and guard dependencies, suspension, cancellation masking and joins. Standard caused-by rendering/default release must traverse complete chains without deep call-stack recursion; user payload display implementations are callbacks. Worklist resource failure is fatal, never permission for recursive fallback, skipped cleanup with normal resumption, or recoverable partial disposal.

## Required isolated backend cases

These belong to ri-dej, not this expectation file. Fatal cases need separate processes/instances; do not add catchable overflow or claim outcomes from ordinary source inspection.

1. Long self-, mutual- and indirect tail chains in each target/build mode; measure all activation storage, not only native SP. Include by-value aggregates, return storage, callable environments and ABI transitions. Never execute undefined LLVM as evidence of language behavior.
2. Test ineligible transfers with local borrows, live guards, pending children/native jobs and explicit error wrapping. Confirm source lifetimes, no early unlock and actual ordinary-call/report behavior.
3. Tiny valid regions, large static/dynamic/aligned frames, stack-passed outgoing arguments and return/aggregate temporaries. Probe/check before first dangerous access, including caller argument setup. Adjacent-region canaries must remain intact; guard-page-only checks can be jumped over.
4. Wasm addressable and saved-budget exhaustion separately, plus engine stack traps. Overall linear-memory bounds are insufficient if an overrun can corrupt another live region. Real saved state across multiple allocations still counts against its budget.
5. Suspend and resume through long chains while references point into addressable storage. Copying engine locals does not automatically preserve address-taken slots. Native/host borrowers must not use stale host addresses after memory.grow. Validate copying, pinning, growth coordination or another safe mechanism.
6. Deep standard/default cleanup and explicit cause chains with observable order, suspending user callbacks and allocation-failure injection. No unbounded recursive fallback; no truncated causes.
7. Test stop, cancellation, termination and migration races against native completion or confirmed abort. Do not reuse stack storage merely because the handle disappeared, cancellation was requested or a native abort was requested. Include host borrowers outliving normal/fatal Rill termination in a surviving embedding host. Retaining their storage does not guarantee user cleanup or a global join. Migration must not duplicate concurrent execution, and these cases add no global exit join.
8. Invalid build versus dynamic settings, alignment overflow, target min/max boundaries and explicit inapplicable flags. Minimal fatal reporting must not depend on a full formatter, heap allocation, unwind or a healthy exhausted stack.

The inspected Binaryen Asyncify rejection of tail-call instructions is negative evidence, not proof of a viable pipeline. Pin revisions and commands, and mark each case as passed, failed or untested. ri-dej remains an explicit gate unless a map decision transfers an unresolved gate. These cases do not assume an alternative architecture or a target drop. ri-8n7 owns full source interfaces/evidence, ri-3t8 actionable diagnostics, and ri-48g exact checking/ABI/lowering/cleanup rules. Hand-checked source examples are not implementation readiness or a soundness proof.
