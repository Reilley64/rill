# Stack capacity and recursion cases

Read the source examples in [`prototypes/14-stacks-recursion.rill`](../../prototypes/14-stacks-recursion.rill).

Approved design inputs for [#22](https://github.com/Reilley64/rill/issues/22) and [ADR 0014](../adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md). The companion has **14 intended-valid expectations**, checked by inspection only. No Rill compiler/runtime, stack measurement, native probe, or Wasm pipeline was executed. Small successful calculations cannot establish constant-space behavior or safe overflow containment.

The examples follow confirmed [#19 enums](https://github.com/Reilley64/rill/issues/19) and [#29 error metadata](https://github.com/Reilley64/rill/issues/29), rather than the earlier singleton/union/automatic-propagation-trace sketches. New sized-spawn path/signature is illustrative for #23 and must preserve ordinary spawn's owned capture/mode/noBorrow/sendable requirements.

## Intended-valid source cases

- `countDown`: scalar accumulator with a semantically eligible self-tail transfer; numeric overflow remains checked, not a stack guarantee.
- `even`/`odd`: mutually recursive eligible transfers, not merely self-recursion optimization.
- `forward`: indirect read-only callable invocation whose descriptor/environment belongs to an earlier caller. Its outer example's local borrow is deliberately distinct from forward's own eligibility.
- `viaStage`: current qualified enum patterns/payloads and ordinary numeric contexts, not old kind-tag unions or literal parameter overloads.
- `iterativeTotal`: input-length traversal via for/of and an explicit-width mutable accumulator.
- `afterCall`: valid ordinary recursion with real work after the recursive call, tested only at small depth.
- `readLocal`/`copyLocalOwner`: valid calls requiring local storage/owner retention; no permission to tear it down for a final-looking call. Optimizations may remove equivalent work but do not create a portable proper-tail promise.
- `startWithCapacity`/`createdAndJoined`: fixed-arity helper using u64 bytes, owned noncapturing work and joined stop. No assertion that the worker must run before cancellation, nor that the test physically observed stack reclamation. Handle metadata can outlive completed execution storage.

## Configuration contract, not executable CLI tests

Illustrative future invocations:

```sh
# Decimal byte counts: 1 MiB default, 8 MiB entry.
rill build --stack-size 1048576 --entry-stack-size 8388608 main.rill
rill test --stack-size 1048576 prototypes/14-stacks-recursion.rill
# For a validated Wasm backend, not a claim that this pipeline works today:
rill build --target wasm32-wasip1 --stack-size 1048576 --saved-stack-size 1048576 main.rill
```

CLI quantities above are decimal bytes; no additional unit parser or source literal suffix is settled here. Source uses u64 bytes, for example `8u64 * 1024 * 1024`. The larger example must fit documented target limits; no universal min/max is assumed. Validate upward alignment/guard arithmetic and the effective capacity before reserving it.

| Request or event | Expected contract |
| --- | --- |
| No overrides, native | Managed entry/spawn usable default 1 MiB; not inherited OS main-stack size |
| Entry override | Entry only; ordinary children still use program default |
| One sized spawn | That routine's addressable capacity only; descendants do not inherit it |
| Scoped helper creates children | Program default unless explicitly overridden by a specified interface |
| Valid unaligned capacity | Checked upward alignment, no silent downward clamp |
| Zero/below-minimum/excessive/unrepresentable effective capacity in build settings | Reject before execution |
| Invalid dynamic u64 request to sized spawn | Panic when executed, not a new Result return shape |
| Reservation failure | Fatal resource exhaustion under ordinary infallible spawning |
| Future physical backing unavailable/host OOM | May terminate without a language report |
| Wasm saved budget | Program-wide default 1 MiB per routine, separate from addressable region |
| Saved-state option explicitly supplied to native | Reject rather than ignore |
| Host engine stack exhaustion | Fatal to Rill execution; no guest cleanup/resumption or forced host-process termination promise |

Both Wasm managed-budget exhaustions use bounded best-effort fatal reporting. Increasing one guest budget does not enlarge the other or the host engine stack. Bounds are not recursion-depth guarantees, native residency, or total-memory quotas. Worker/scheduler/blocking-job stacks are separate resources; native frames executing on a managed region consume that region.

## Eligibility and rejected shortcuts

A proper-tail transfer cannot need eliminated caller storage or post-call disposal/joins. Retaining these obligations makes a final-looking call ordinary; it is not generally a source type error. Do not introduce a new tail keyword or require every ordinary recursive program to be rejected.

Schematic guard case (library spellings remain #23):

```ts
function guarded(counter: &Mutex<u64>, n: u64): u64 {
  let held = lock(counter);
  return countDown(n, 0);
} // held must be disposed after the call; this call is not promised proper-tail.
```

A callee that reacquires the same mutex can observe that the guard is still held. Do not dispose it before the call or at its last explicit mention to force TCO. Explicit disposal before transfer is a source-program choice and remains subject to outstanding loans/child joins.

Likewise, do not eliminate a caller-local slot that a callee/native job/closure needs. Incoming borrowed sources that belong to an earlier frame differ from newly borrowed local storage. ABI restrictions cannot become undocumented semantic ineligibility: if a genuinely eligible call cannot be lowered, diagnose the unsupported call/target rather than silently accumulating frames. Development and optimized builds have the same guarantee.

#29 adds no automatic propagation history. Direct unchanged failure forwarding has no such trace obligation, but all actual control/storage/cleanup work still counts. Explicit infix caused-by wrapping or Err construction can require work after a call; early error return is not by itself proof of a proper-tail call. Complete explicit causes are error data. Normal panic traces may omit eliminated calls; no unbounded diagnostic shadow stack is required.

## Bounded cleanup is not constant total memory

Standard traversal/container internals and compiler-generated default cleanup must not recurse once per input depth/link. Use loops/worklists/eligible transfers, allowing only small explicitly bounded recursive helpers. Large user payload frames or an already-full caller can still overflow; arbitrary callbacks/user disposers can recurse.

A cleanup worklist can retain depth-dependent pending operations because those operations are not an eligible tail chain with no obligations. This is not tracing GC or permission to hide a genuinely eliminated activation chain on the heap. Saved activation accounting cannot be evaded by relabeling an activation allocation as worklist data; ordinary worklist/program/cause graphs are not total-memory-quota subjects.

Preserve observable disposal order, ownership/initialization state, borrowed-source and guard dependencies, suspension, cancellation masking and joins. Standard caused-by rendering/default release must traverse complete chains without deep call-stack recursion; user payload display implementations are callbacks. Worklist resource failure is fatal, never permission for recursive fallback, skipped cleanup with normal resumption, or recoverable partial disposal.

## Required isolated backend cases

These belong to [#30](https://github.com/Reilley64/rill/issues/30), not this expectation file. Fatal cases need separate processes/instances; do not add catchable overflow or claim outcomes from ordinary source inspection.

1. Long self-, mutual- and indirect tail chains in each target/build mode; measure all activation storage, not only native SP. Include by-value aggregates, return storage, callable environments and ABI transitions. Never execute undefined LLVM as evidence of language behavior.
2. Negative eligibility with local borrows, live guards, pending children/native jobs and explicit error wrapping. Confirm source lifetimes, no early unlock and actual ordinary-call/report behavior.
3. Tiny valid regions, large static/dynamic/aligned frames, stack-passed outgoing arguments and return/aggregate temporaries. Probe/check before first dangerous access, including caller argument setup. Adjacent-region canaries must remain intact; guard-page-only checks can be jumped over.
4. Wasm addressable and saved-budget exhaustion separately, plus engine stack traps. Overall linear-memory bounds are insufficient if an overrun can corrupt another live region. Real saved state across multiple allocations still counts against its budget.
5. Suspend/resume through long chains while references point into addressable storage. Copying engine locals does not automatically preserve address-taken slots. Native/host borrowers must not use stale host addresses after memory.grow: validate copying/pinning/growth coordination or another safe mechanism.
6. Deep standard/default cleanup and explicit cause chains with observable order, suspending user callbacks and allocation-failure injection. No unbounded recursive fallback; no truncated causes.
7. Stop/cancel/termination/migration races and native completion/confirmed abort. Do not reuse stack storage merely because the handle disappeared, cancellation was requested or a native abort was requested. Include host borrowers outliving normal/fatal Rill termination in a surviving embedding host: retention is not guaranteed user cleanup or a global join. No concurrent execution duplication on migration and no new global exit join.
8. Invalid build versus dynamic settings, alignment overflow, target min/max boundaries and explicit inapplicable flags. Minimal fatal reporting must not depend on a full formatter, heap allocation, unwind or a healthy exhausted stack.

The inspected Binaryen Asyncify rejection of tail-call instructions is negative evidence, not proof of a viable pipeline. Pin revisions/commands and mark pass/fail/untested. #30 remains an explicit gate (or transfers an unresolved gate with a map decision); no alternative architecture or target drop is assumed. #23 owns full source interfaces/evidence, #24 actionable diagnostics, and #28 exact checking/ABI/lowering/cleanup rules. Hand-checked source examples are not implementation readiness or a soundness proof.
