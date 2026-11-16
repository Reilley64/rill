# Bounded stacks, proper-tail calls, and explicit backend gates

[Issue #22](https://github.com/Reilley64/rill/issues/22) retains non-moving native stacks, configurable finite capacities, and a semantic proper-tail guarantee without sacrificing cleanup or borrowed storage. Standard traversal/default cleanup uses bounded call stacks; backend limitations are explicit support gates, not permission to silently accumulate frames. The consolidated design is approved, not an implemented runtime or proof.

This follows the confirmed [enum decision #19](https://github.com/Reilley64/rill/issues/19) and [caused-by decision #29](https://github.com/Reilley64/rill/issues/29), not superseded singleton/union/automatic-propagation-trace wording.

## Native capacity and configuration

Every native routine, including ordinary Rill entry execution, uses a fixed non-moving virtual-address region for its lifetime. No copying or segmented stacks in v0. Physical backing may be demand-driven: stable reservation is not residency, swap commitment, or a guarantee of future backing. Non-relocating growth is not intrinsically borrow-unsafe; segmentation is deliberately excluded for complexity rather than claimed impossible.

Default usable capacity is **1 MiB** for both entry and spawned routines, not an inherited OS main-thread stack limit. Guards/allocation metadata are outside usable bytes. ABI/runtime frames actually executing on the region consume it; separate scheduler/worker/blocking-job stacks are separate resources. Capacity is neither total memory nor a portable recursion-depth count. Defaults are unmeasured design baselines, not benchmark results.

Build-time `--stack-size` sets the program default; `--entry-stack-size` overrides only entry. Run/test expose equivalent compilation options; no new test scheduling/isolation rule is implied. Embed defaults in the program, not a manifest or ambient environment variable. A separate fixed-arity helper such as `spawnWithStack(work, stackBytes)` chooses a new routine's addressable capacity before it starts, without introducing optional/named arguments. Children and ordinary scoped-helper children use the program default, not their parent's override, absent an explicit override.

Source capacities are `u64` byte counts. Reject zero, below-minimum, excessive or unrepresentable requests. Validate required upward alignment with checked arithmetic and reject an effective size exceeding supported limits; never silently round down. Document target/runtime limits and report effective capacity where relevant rather than invent an unvalidated universal min/max. No public usize/isize is added.

Invalid build settings fail before execution. Invalid dynamic requests panic. Ordinary/sized spawn still returns `Routine`, not Result; reservation failure is fatal resource exhaustion. A fallible spawning API requires a separate decision. Future backing/host OOM may terminate without a language diagnostic. #23 must preserve spawn's owned callable/mode/noBorrow/sendable evidence, not erase it into an unqualified arrow parameter.

## Wasm resources are separate

A Wasm backend distinguishes:

- Addressable Rill stack in linear memory: initially **1 MiB per routine**, controlled by stack/entry settings and the sized-spawn helper.
- Saved suspension/activation storage: initially **1 MiB per routine**, controlled by program-wide `--saved-stack-size`; no per-routine/entry saved-state override in v0.
- Host engine execution stack: host-controlled, not enlarged by these guest settings.

Native builds reject an explicitly supplied saved-state option rather than ignore it. The defaults are budgets, not native-style cheap virtual reservation, residency, or recursion-depth promises. Saved-state capacity is not a total-memory quota for program data, cleanup worklists or explicit cause graphs; genuine saved activations cannot evade accounting by moving to a differently named allocation.

Both managed-capacity exhaustions use fatal-overflow behavior. Host execution-stack exhaustion is separately fatal to the Rill execution, with no Result/catchable overflow/cleanup promise. An external host may handle a trap, but the Rill execution must not resume as if a routine returned normally; the embedding host process itself need not terminate.

Addressable borrowed storage must stay valid through suspension. Ownership of a linear-memory region alone does not stabilize a retained **host pointer** across `memory.grow`: copying, pinning/growth coordination or another demonstrated mechanism is required. Saving engine locals does not automatically preserve address-taken linear-stack objects. Exact implementation/accounting needs validation, not assumptions about Asyncify or native guard pages.

## Proper tail calls, not merely final-looking calls

A proper-tail transfer has no caller-frame storage required by the callee and no caller work owed after return. Pending disposal, guard release, child/native joins or caller-local borrows can make `return f(...)` an ordinary call. Preserve observable lifetimes: never release guards early, skip cleanup or invalidate storage to obtain TCO. A compiler may remove equivalent computation, but source requiring retained obligations gains no portable tail guarantee merely from an optimization.

Eligible calls guarantee bounded call-stack/activation-state growth in development and optimized builds, including appropriate direct/mutual/indirect chains. Explicit program data may grow; retaining an eliminated eligible activation chain as heap continuations does not satisfy the guarantee. Semantic eligibility is not backend convenience: an ABI/lowering limitation must identify unsupported behavior/call/target, not silently classify the call as ordinary and accumulate frames. Provide inspectable eligibility reasons without a new tail keyword in v0.

Under #29, ordinary failure returns/postfix propagation add no automatic trace history. This removes that historical metadata obligation, not all possible control/cleanup work. Explicit caused-by wrapping constructs context after failure and cannot be assumed proper-tail merely because it returns early. Explicit cause graphs are error data, not automatically retained call frames. Ordinary panic traces describe available frames and need not reconstruct every eliminated tail call or maintain an unbounded shadow history; the explicit caused-by chain is not truncated by this rule.

## Bounded standard traversal and default cleanup

Input-dependent standard traversal, container internals and compiler-generated default cleanup use loops, explicit worklists or genuinely guaranteed proper-tail calls, not optional optimization. Small, explicitly bounded recursive helpers can remain; merely noting that machine input ranges are finite is not a useful small-depth bound. User callbacks/disposers may recurse and remain ordinary code outside this guarantee. No claim that every library invocation fits an almost-exhausted caller stack or uses constant total memory/time.

Iterative ownership cleanup preserves disposal order, initialization/dependency state, suspension, cancellation masking and required joins. It is not tracing GC or a new implicit reference-counting model. It may retain depth-dependent cleanup worklist data because cleanup with pending obligations is not an eligible proper-tail chain. Standard caused-by-chain traversal/rendering and generated disposal must likewise bound their own call stacks, while payload formatters are callbacks. Do not truncate explicit causes to obtain that bound.

Implementations may avoid worklist allocations, but inability to obtain required storage is fatal resource exhaustion. Do not fall back to depth-proportional recursion, skip obligations and resume normally, or expose a recoverable partially disposed state. Fatal termination still does not unwind or guarantee cleanup completion.

## Protection, reporting, and reclamation

Prevent exhaustion from corrupting adjacent storage before crossing a managed boundary: large frames, alignment, outgoing stack arguments, aggregate/return temporaries and runtime/native transitions all count. A callee-only prologue check is insufficient if caller argument setup can already corrupt storage. Native guard pages need adequate probes/checks; Wasm overall linear-memory bounds do not protect individual managed regions. Capacity overrides never disable protection.

Detected native/addressable/saved-state overflow attempts a bounded best-effort reason and routine/capacity information where safe, then terminates. No required full trace, user formatter, unbounded allocation, unwinding, or assumption normal panic code can run on an exhausted stack. Each target needs an emergency reporting mechanism; host OOM/termination may prevent any report.

Retain addressable stack/saved state until actual execution, required cleanup and all retained child/native borrowers complete or confirm release. Dropping a Routine handle or requesting cancellation is insufficient; completed handles may retain status after stack release. Where an embedding host survives Rill termination, external borrowers still constrain storage release; this is retention, not a user-cleanup or global-join promise. Migration preserves addresses and does not duplicate concurrent execution. Normal exit retains the no-global-background-join policy. ADR 0009's joined/deferred cleanup remains distinct from fatal panic.

## Supersession and evidence gate

This qualifies #8/#13's old unconditional tail-position, guard-page-only and uniform virtual-stack/Asyncify claims. LLVM tailcc/musttail remains an intended native mechanism for eligible Rill transfers, not proof of a working target pipeline. The inspected Binaryen Asyncify revision rejects tail-call instructions; [ADR 0006's human-approved #30 follow-through](0006-whole-program-monomorphization-on-llvm-23.md#confirmed-portable-wasm-lowering-direction-30) supersedes that provisional Wasm composition with compiler-owned records and iterative dispatch as a direction, not a completed ABI/runtime or supported backend. A supported backend still requires compatible mechanism evidence; remaining failures return unresolved target/lowering scope to the map rather than silently weaken guarantees or infer completed validation from directional approval.

[Backend validation #30](https://github.com/Reilley64/rill/issues/30) requires pinned revisions, commands, targets/build modes and reproducible pass/fail/untested evidence before support/implementation-readiness claims. Mechanism experiments can use minimal IR/Wasm harnesses without pretending a Rill compiler exists. Test ABI tails, overflow canaries, independent Wasm budgets/host traps, memory growth and host leases, iterative cleanup/failure injection, and cancellation/reclamation/migration. Partial evidence is not a checker proof; unsupported targets remain gated. The later [runtime-validation disposition](../research/0030-runtime-validation.md) completes #30's standalone mechanism stage for its pinned/profile evidence. Production ABI/generated-admission/checker/runtime/provider support acceptance remains with #28 and the applicable provider work; this does not approve a production carrier/library or experimental constants.

`prototypes/14-stacks-recursion.rill` and `.md` are hand-checked examples/cases, not executed stack tests. #23 owns complete public configuration/callable/capture interfaces and library bounds, #24 diagnostics/eligibility reports, #28 checking/cleanup/ABI/lowering rules and cross-feature RFC reconciliation. Closing #22 approves this policy, not a validated backend, accepted feature RFC, or completed runtime.
