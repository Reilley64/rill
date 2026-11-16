# #30: native admission, owned tail ABI, migration and terminal retention

## Status and scope

**Disposition: #30's standalone mechanism-validation work is complete.** The independent read-only review found no actionable P1/P2 safety/claims defect and no additional minimal mechanism experiment required by the issue's checklist. Its P3 stale lifecycle evidence manifest was refreshed in full from the source-aligned replay, including every dependency manifest. #28 retains production/formal support acceptance; this is not an implementation-ready compiler/backend seal.

This follow-up executes the previously missing **standalone mechanism experiments**. It does not implement Rill, prove the borrow checker, adopt Boost as a runtime dependency, approve a production private ABI, or claim all compiler-generated/native/provider code is supported. Source semantics and intended targets are unchanged. The approved portable Wasm record/dispatcher direction is not reopened.

The initial [backend experiments](0030-backend-experiments.md) and [portable experiments](0030-portable-suspension-experiments.md) remain historical snapshots. Their negative evidence is not erased. The portable fixture now additionally composes terminal guest failure/store destruction with actual outstanding foreign work.

## Reproduce

Assertions are required; do not use `python -O` or define `NDEBUG`.

```sh
python3 prototypes/backend-30/native-capacity/run.py --out /tmp/rill-30-native-capacity
python3 prototypes/backend-30/native-lifecycle/run.py --out /tmp/rill-30-native-lifecycle
# Lifecycle stages pinned Linux C++/UAPI headers required by the mixed ABI runner.
python3 prototypes/backend-30/native-abi/run.py --out /tmp/rill-30-native-abi
python3 prototypes/backend-30/portable/run.py --out /tmp/rill-30-portable
```

Full instructions, exact scope, source citations and checked-in JSON:

- [Native capacity/packet control](../../prototypes/backend-30/native-capacity/README.md).
- [Mixed-layout owned tail ABI](../../prototypes/backend-30/native-abi/README.md).
- [Native lifecycle/recursive admission](../../prototypes/backend-30/native-lifecycle/README.md).
- [Portable records/foreign work](../../prototypes/backend-30/portable/README.md).

The new native code uses LLVM/Clang **23.1.5**, upstream `392dfff97d29b59d77e5cac6d4b6ef2585fe7117`, for **all mechanism bodies including Linux**. GCC only builds the capacity fixture's POSIX host or links LLVM-produced objects. Mach-O uses Apple ld **27037.1**; Linux uses GNU ld **2.44** / glibc **2.41** from immutable image digests. Inventoried Rust compilers do not build these mechanisms. Native context primitives are source/digest-pinned Boost.Context **1.92.0**, `cc1b9e02cd530d6607becc984e1e5bfa4f7b3600`; only the plain make/jump assembly ABI is used, not its C++ fiber/destructor wrapper. Original source/license notices stay intact in the external cache.

| Native binary target | Execution evidence | Modes |
| --- | --- | --- |
| AArch64/macOS | Physical Apple M3, macOS 27.0.1 (26A434) | O0/O2 |
| x86-64/macOS | Rosetta on that host, **not physical x86** | O0/O2 |
| AArch64/Linux | Existing ARM64 Docker VM | O0/O2 |
| x86-64/Linux | Translated/emulated Linux VM execution, **not physical x86** | O0/O2 |

The portable module still targets **wasm32-wasip1**, executes synchronously in Node/V8 and Wasmtime/Cranelift, and requires neither Asyncify nor engine tail instructions. Wasmtime **50.0.1**, Binaryen **133**, engine flags and exact host/tool hashes are recorded by its runner. No browser compatibility, Windows, or wasm32-unknown-unknown acceptance is added.

## Execution matrix

Counts are per-suite command/outcome inventories, **not counts of passing Rill tests**. Fatal/negative controls are successful reproductions of expected failure, not source success or recovery APIs.

| Suite | Commands | Recorded outcomes | Disposition |
| --- | ---: | ---: | --- |
| Native capacity and first packet ABI | 217 | 136 | Expected positive, isolated terminal, rejected-codegen and unchecked-corruption outcomes reproduced |
| Mixed-layout owned tail ABI | 123 | 24 chains | All checks pass; each chain performs one million transfers |
| Native lifecycle/recursive admission | 51 warm-cache | 80 | Ten cases × four binaries × two modes; expected terminal escapes included |
| Portable records, refreshed host | 29 | 66 | Original 64 observations plus two terminal-store/native-work compositions |

Fresh lifecycle header staging adds commands. Historical baseline **108 commands / 162 observations** still includes four `contract: FAIL` rows for repeated Asyncify/red-zone containment counterexamples. Optional retcon's **28/8** narrow evidence is unchanged; it is not required machinery or a complete source-tail lowering.

### Capacity before prologues, copies and results

The capacity fixture runs actual **192 KiB/256 KiB, 4096-aligned** native frames, a **32 KiB by-value aggregate**, outgoing stack copies, a **32 KiB return temporary** and twelve scalar arguments. The outside pointer-only admission thunk executes before the argument builder's large prologue as well as the callee's. Generated `.su` frames and assembly, explicit alignment/red-zone/link allowance and headroom determine the finite acyclic bound. The subtree permits no unaccounted native/library/helper call.

Default, near-threshold and exact conservative admission-fit cases succeed with adjacent logical canaries intact. Tiny, one-byte-over-footprint, one-alignment-unit-short and actual **2 MiB** oversized-frame cases terminate externally with builder entry count zero and intact canaries. Checked-u64/round-up invalid cases reject before mapping. The unchecked control really corrupts an adjacent logical canary **within valid physical backing**; physical guard pages would not establish the promised containment.

Exact conservative admission fit is not the exact last byte touched, a universal recursion depth or final public minimum/headroom policy.

### Proper tails with differing unboxed layouts and ownership

The stronger ABI matrix actually transports records of **176/224/320 bytes**, aligned **16/32/64**, through self/mutual/genuine indirect native chains. Every chain changes A → B → C layouts. It checks signed/unsigned integer fields through i128, bool/tagged variants, exact floating negative-zero/quiet-NaN bits, an immutable borrow into the earlier root, an owned 73-byte buffer and an owned callable-environment-like capture.

An earlier **ordinary** root owns current/staging slots and result storage. No current tail caller's alloca is passed onward. All native tail bodies use the same private `tailcc` signature and verified `musttail`, with helpers fully returning before transfer. Two packet slots total **768 bytes**, independent of transfer count; the result is **176 bytes**. Complete native frame usage is larger and separately recorded. No per-transfer allocator or retained activation chain exists.

Each of **24 chains** executes **1,000,000 transfers**, **1,000,001 SP samples** and **1,000,001 logical ownership moves**, including the result move. All sampled spans are **zero bytes**. Every moved-from owner is poisoned/checked before its record lifetime ends; destination values are checked before regeneration. A genuinely independent deep copy stays unchanged. The final callee invokes/releases its capture once; the earlier root consumes/releases the returned buffer and independent copy once each, using real allocation/free operations.

Generated register-indirect branches remain indirect at both modes; mutual edges are tail branches; O2 self chains become loops. C++ active union members use valid placement construction/lifetime rules, not stale-member reads or an aliasing extension. This is manual lowering evidence, not Rill move/drop elaboration. Public `dyn`, reflection, callable or capture interfaces are not introduced.

The rejected AArch64 incoming LLVM `byval` shape remains a negative control. Source by-value semantics **need not require that LLVM attribute**: the private earlier-owned uniform packet/result route preserves value and ownership while avoiding incompatible native signatures. No ordinary-recursive fallback replaces an eligible tail.

### Ordinary recursion, suspension, migration and cleanup

Native lifecycle uses a fixed non-moving **1 MiB logical usable region per context**. Writable adjacent canaries and physical guards are outside that region. ABI/context-save frames on the region consume it; separate scheduler/native-job stacks are separate resources, as already specified in ADR 0014.

- Three logical contexts execute on two OS workers, including an atomic barrier requiring overlapping execution. Each subsequently migrates.
- A real native job retains a pointer into a real automatic aligned buffer while its owning context migrates at least **10,000 times**. Native work itself has a checked **256 KiB automatic frame on a separate OS-thread stack**, not extra capacity secretly added to the managed stack. An exclusive-output case writes the buffer only while its owner is parked; the owner observes output after actual join.
- Required join/cleanup does not finish at handle drop or cancellation request. A real C++ disposer suspends/migrates while masked and finishes only after acknowledgement. Completed status handles outlive stack release without owning execution storage.
- A real scoped child context borrows its parent's automatic buffer. Parent exit waits while the child's own destructor is suspended, then permits parent disposal only after child body/cleanup completion and loan release.
- An iterative worklist handles **200,000 nodes**, **133,333 initialized owning payloads**, partial-init slots and checked child-before-parent order. Actual non-inlined formatter/destructor callbacks yield **180 times** with local state live. Complete cause-like identifier formatting matches independent checksum **56,866,720**. Sampled callback-frame address spans are **4–52 bytes**, not claimed peak stack usage. This is not the Rill `Err<E>` implementation.
- Native worklist allocation failure escapes terminally without recursive fallback; the earlier C control separately fails during worklist growth.

The recursive admission case adds real **8192-byte / 4096-aligned** bodies with live post-call work. Every child body goes through a small entry before its prologue. Eight successful edges complete nine bodies; a million-edge request then reaches the checked boundary and terminates without another body prologue or logical-canary corruption. Generated static body/entry frames plus the audited acyclic emergency helper graph, per-node allowance and context-save/headroom reserves determine the requirement (**41,656–67,216 bytes** in these fixtures). This is per-edge admission of a recursive graph, not an assertion that the recursive graph is acyclic.

The terminal path preserves all suspended scopes. The outstanding native job subsequently reads the original buffer; only its independent backing lease remains after execution ownership is dropped. The mapping releases once foreign access ends. Healthy fatal escape and normal top-level-exit/background retention have the same lifetime discipline, without global joining or user-disposer promises. No normal return/unwind ends an automatic buffer's lifetime before that read. Ordinary escape edges are verified as `notail call`/`invoke`; sibling-call removal of the owning scope is disabled, not used to invalidate its borrower.

These controls also retain the distinction between ordinary final-looking calls with caller borrows/guards/scoped or native joins/wrapping and genuine eligible tails. The existing C control's explicit wrapping/cleanup phases remain manual source-rule models; unchanged error propagation acquires no automatic trace work. An actual Rill eligibility/checker diagnostic is not claimed.

### Terminal Wasm teardown with foreign work outstanding

The refreshed portable host starts an actual gated native worker with an independent copy of guest input. With that worker still pending, it drops the guest handle/requests cancellation, injects terminal reservation failure, and destroys the Wasmtime store. Only afterward can the worker access its copy. It joins and frees that copy without guest reentry or post-fatal copyback. Both O0/O2 pass. A host-created Wasmtime trap can surface as a C-API error rather than a core trap; neither becomes a source `Result` or recovery continuation.

The existing normal-completion composition separately grows/relocates guest memory **between completed polls**, with foreign work still pending, and reacquires the fresh guest base/extent for copyback after real completion. It does not invoke forbidden same-store APIs while a Wasmtime async future exists, retain a stale guest pointer, or mistake cancellation for completion.

## Compatible lowering route and acceptance boundary

The evidence supports a concrete route, not stock Asyncify:

1. **Native:** existing LLVM23 `tailcc`/`musttail`, fixed mapped routine stacks, explicit ordinary-frame retention, private earlier-owned input/staging/result storage and small outside admission entries. For a finite closed monomorphized tail component, reserve the maximum reachable input layout/alignment and staging/result needs in the earlier ordinary owner; per-tail steps replace/move that state, never allocate an activation history. Pending current-caller storage/cleanup remains ordinary. Checked static metadata must include actual compiler frame, copies, alignment, helper/entry and emergency reserves **before** large builders/prologues. Unaccounted native calls require explicit separate-stack bridging or their own admitted bound.
2. **Wasm:** already-approved compiler-owned records plus iterative dispatcher. Retain ordinary callers; replace eligible current records; suspend back to the scheduler and resume the surviving function/PC. Charge retained metadata/staging/padding before writes against the separate per-routine addressable/Saved capacities; the program-wide override is a setting, not an aggregate quota. The two simultaneous 8000-deep portable chains still each fit their own Saved default while exceeding 1 MiB combined.
3. **Both:** actual borrower/required-cleanup completion governs reclamation; external native access uses a stable retained native region or independently owned copy with fresh-pointer copyback. Fatal/host exhaustion has no unwind/user-cleanup guarantee and never reenters the failed invocation. Ordinary semantic heaps/worklists are not eliminated activation history or total-memory quotas.

This is a **component/profile validation**, not a universal mathematical safety proof or production support seal. The following are not falsely promoted to executed results:

- Automatic Rill MIR liveness/eligibility, layout maximization, generated admission placement, integer/addressability limits, move/borrow/drop/evidence checking, source diagnostics and all-language adversarial tests remain formal checking/lowering work for #28/#24. No whole compiler was required to perform the experiments above.
- Full typed ABI specification, all aliasing/public/native interop, every layout or callback, and composition with every runtime/provider need implementation validation. Three layouts are not an exhaustive type universe. Generic/dynamic data is not permission for unaccounted dynamic stack frames or hidden boxing.
- Real libc allocation/free, printing and capture invocation in the ABI tests are outside the finite capacity subtree; the ABI suite runs on the host OS stack. Lifecycle recursion admits its specifically audited subtree; other C++ lifecycle control/library operations execute with ample fixture capacity and are not a general native-library frame proof. The separate job demonstrates a viable explicit boundary, not automatic rewriting of every call.
- The native fatal mechanism is **external scheduler escape/termination** with reserved bounded stack, not a signal/sigaltstack test. Extra sanitizer, CET/shadow-stack/arm64e profiles are not established. Workers use uniform floating environment and no live thread-affine/TLS-dependent user object across yield.
- Physical x86 hardware, a complete Rill compiler/runtime, production allocator/scheduler/provider acceptance and RFC acceptance are not established. No intended target is dropped because available execution was translated.

The earlier provisional Asyncify pipeline remains disproven. The new positive mechanisms remove the earlier *unexecuted experiment* gaps; they do not retroactively turn negative controls into support or silently clear #28's production/formal acceptance requirements.

### Explicit completion disposition

Close #30 as **completed standalone mechanism validation for the stated pinned/profile evidence**, not as production backend acceptance. No required missing mechanism is being hidden behind a demand for the whole Rill compiler or moved to an untracked blocker. The approved Wasm direction now has positive record/dispatcher/accounting/foreign-access evidence; native tailcc has a legal owned, differing-layout route plus pre-write admission and real fixed-stack lifetime/migration/cleanup controls.

Retain the already-owned #28 checking/lowering/integration gate: specify the complete private ABI and layout/admission rules, implement their automatic generation, validate the resulting Rill/runtime/provider pipeline, and only then declare those production constraints implementation-ready/supported. #13 consumes these candidate mechanisms without treating the probe carrier/library/constants as an adopted runtime. #24's reporting contract and individual provider gates remain separate. This is an explicit scope disposition, not a new architecture decision, target narrowing, fallback permission or RFC acceptance.
