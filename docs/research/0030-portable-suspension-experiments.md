# ri-dej portable suspension: executed candidate and proposed direction

2026-11-15. A concrete candidate now exists; architecture adoption and the support gate remain OPEN. This follow-up preserves the earlier [negative/positive evidence](0030-backend-experiments.md). It does not change approved targets/semantics or claim an automated Rill compiler/checker/runtime.

Primary-source alternatives and their immutable revisions are in [candidate research](0030-portable-suspension-candidates.md). The [portable probe README](../../experiments/backend-30/portable/README.md) describes executable sources, reproduction and protocol limits. Recorded evidence includes [main results](../../experiments/backend-30/portable/evidence/results.json), [main commands](../../experiments/backend-30/portable/evidence/commands.json), [retcon results](../../experiments/backend-30/portable/evidence/retcon-results.json) and [retcon commands](../../experiments/backend-30/portable/evidence/retcon-commands.json). Raw IR/WAT, call graphs, stack-usage files and stdout/stderr are regenerated and included in the issue's text-evidence publication; binaries/tools are excluded.

## Recommendation to bring back for approval

Propose compiler-owned explicit activation/continuation records and a nonrecursive dispatcher as the portable wasm32-wasip1 lowering direction. Keep the approved LLVM 23 pipeline and native fixed-stack/proper-tail work; do not silently replace native lowering or change public syntax, callable evidence, library APIs, tail eligibility or budgets.

In this direction, generated Wasm step functions return actions to a scheduler. Ordinary calls preserve charged caller continuations and fixed addressable storage. Eligible tails replace the current activation instead of pushing another record. Yield returns fully to the scheduling boundary; resume enters the actual surviving function/PC. All retained activation state is inspectable and chargeable. Source addressable storage never lives in an expired temporary C step frame.

This is a proposed architecture, not an adopted one. Testing it was authorized; accepting the replacement Wasm ABI/lowering remains a separate decision. No ADR or confirmed language/library contract is edited by this report.

## Executed scope

- The experiment uses the same LLVM/LLD 23.1.5, Binaryen 133 and Wasmtime 50.0.1 pins as the first experiment. It records version outputs and source/tool hashes. The host runs macOS 27.0.1, Darwin 27, on AArch64/Apple M3. The evidence also records Node 26.13.0 and its detailed V8/runtime version inventory.
- The experiment compiles manually lowered freestanding C through verified LLVM IR to wasm32-wasip1 at O0 and O2. It disables tail-call codegen and uses no Asyncify, coroutine/JSPI instruction in the main machine, or parked engine fiber. Binaryen validates the core-Wasm MVP feature profile. This reduces a mechanism dependency; it does not add an older-host or browser support commitment to the approved Wasm 3.0 target.
- Node/V8 executes the full main test matrix. Wasmtime/Cranelift executes the same main module synchronously with tail-call support disabled, 32 KiB engine stack and forced movable-memory configuration. Cranelift modes are none/speed, paired with O0/O2.
- Two logical routines share the same program-wide saved-size setting of 1 MiB per routine; there is no invented aggregate 1 MiB cap. Saved is not a total-memory quota. Host stack, managed addressable and managed Saved storage stay distinct.
- The retcon comparison uses LLVM 23's explicit early/split/cleanup passes, then O0/O2 optimization/codegen, and executes on Node only. It is a narrow compiler-machinery probe, not a second full candidate validation.

## Finite result matrix

PASS means the specified manually lowered probe passed, not a universal-depth proof or supported Rill backend. FATAL_EXPECTED means the external embedding observed terminal failure of that invocation; it is never a catchable Rill overflow API.

| Probe | Observed result | Boundary |
| --- | --- | --- |
| Self, mutual and indirect source-like tail transfer | **PASS**, 1,024 and 1,000,000 transfers in both modes; exactly N+1 effects, correct aggregate result, 977 waits at one million | The header, packets and byte patterns are hand-written, not produced by a MIR transform. |
| Tail activation/storage growth | **PASS**, one current activation plus one genuinely live ordinary root; identical live/allocated peak at both counts | Saved peaks 560 / 736 / 768 bytes for self/mutual/indirect; includes 416-byte header with staging. Variable 64–256-byte records, 16–64-byte alignment and padding are counted. No per-transfer allocation/log. |
| Rewind/replay counterexample avoided | **PASS**, repeated polls while waiting do not change PC/effects; resume continues the current function once | No re-entry through the eliminated tail-entry chain and no replay-based Asyncify extension is tested/claimed. |
| Live ordinary borrow plus modeled guard, join and wrapping phases | **PASS**, depth 2,000; 2,002 maximum records, 192,592-byte Saved peak; 501 waits; post-call observations/counters checked | The fixture classifies calls manually. Cleanup/masking/cancellation/Err semantics are models, not Rill implementations. Borrow slots remain in the fixed addressable arena. |
| Two simultaneously deep ordinary chains | **PASS** in both engines/modes, depth 8,000 each; 768,592 bytes Saved each, 1,537,184 combined | This confirms the per-routine setting, not an aggregate quota. Different routine tokens reject cross-wake. It does not prove native M:N migration. |
| Source-like managed budget failure | **FATAL_EXPECTED**, exact-root/no-callee room; larger aligned tail replacement; ordinary Saved depth; ordinary addressable slots; oversized addressable request | Failure precedes out-of-region writes; separate adjacent canaries stay intact. A whole-memory OOB trap is not the protection mechanism. |
| Configuration/arithmetic/protocol failures | **FATAL_EXPECTED**, invalid capacities, unrepresentable u64 setting, checked upward alignment overflow, u32 range overflow, invalid/unsupported alignment and malformed tail with pending obligations | The cases use separate instances and external memory-only inspection after fatal escape. Generic reserve arithmetic is not a full compiler-generated frame suite. |
| Exact-fit | **PASS**, 560-byte Saved / 65,600-byte addressable setting for the tiny self case; near-u32 arithmetic checked separately | This includes the header/runtime reservation. The prototype's minima/layout policy does not define adopted target limits. |
| Transient compiler/runtime linear stack | **PASS for this linked fixture**, acyclic call graph including six indirect step targets; static frame sum 784 / 0 bytes O0/O2, conservative red-zone/alignment bounds 4,960 / 4,176 bytes, under charged 64 KiB reserve | The fixture has only a known nonreentrant fatal import, with no table mutation or unknown recursion. This number does not bound arbitrary Rill/helper/ABI output. |
| Logical yield without engine continuation | **PASS**, `poll` returns normally; same ordinary core module runs in Node and synchronous Wasmtime | The guest C stack has returned before external growth or the next poll. No opaque engine continuation is omitted from Saved accounting. |
| Native completion while guest memory relocates | **PASS**, Wasmtime relocation forced legally between completed polls with native worker still waiting on an independent copy; destination stays live, early wake refused, copyback after pthread join through fresh memory, then resume | This tests a stronger interleaving than the old pending-C-future control. Request/drop are models; copying is real. It does not prove a zero-copy lease, native stack migration or arbitrary provider behavior. |
| LLVM retcon spill/resume/destruction | **PASS**, one million resumes per inline/fallback variant in O0/O2; zero inline fallback allocations, one 8-byte fallback allocation/free | The variant reuses the same single coroutine. It does not prove LLVM eliminates parent/awaiter objects in a source tail chain. |
| LLVM retcon undersized fallback | **FATAL_EXPECTED** for seven-byte region, canary intact before generated eight-byte store; allocator never returns null | This tests only this aligned frame shape. A complete frame/metadata ledger and over-aligned cases remain open. Inline variant legitimately needs no fallback region. |

The machine reserves fixed maximum backing of 2,097,728 bytes per routine plus one shared 64 KiB runtime linear-stack reservation. This intentionally simple backing policy is not production allocation/reclamation evidence. Unused backing/reservation is not live activation history; all actually retained control fields, live record extents, padding and staging are charged. The header includes observational counters rather than treating them as free activation metadata.

The shared compiler/runtime stack is conservatively charged in full against each routine's addressable capacity before its first step. Bootstrap/configuration uses the statically bounded pre-reserved runtime stack. The fixture therefore admits no routine below 64 KiB + 64 bytes addressable capacity. The acyclic linked-call-graph analysis and static frame reports cover this fixture's prologues, helpers and leaf red zones; adding helpers/dynamic alloca/ABI temporaries/reentry invalidates the bound unless rechecked or instrumented. These admission policies and the accounting split require architecture review, not inference from successful execution.

## Why this is the leading candidate

1. It avoids the incompatible mechanisms. No Asyncify saved push crosses an unchecked end, no unsupported return-call transform is needed, and suspension does not depend on a particular host's parked fiber. Tail semantics come from replacement in all builds, not an optimizer hint.
2. The compiler/runtime owns the state and checks. PCs, function IDs, return links, arguments/results, tokens, initialized/cleanup phase state and scratch can be laid out and charged before writes. Growth does not invalidate Wasm offsets, and returning to the scheduler creates a legal host copy/completion boundary.
3. It retains ordinary-call obligations. A live caller record/slot survives child calls; guard/join/wrapping work is not discarded to force TCO. The source checker still owes the eligibility/ownership evidence.
4. LLVM offers a compiler-assisted option to investigate. LLVM retcon can split and spill a small coroutine using frontend-supplied storage, with observable checked fallback allocation. It may reduce custom frame-layout/spill work, but does not supply record replacement, accounting, safe borrows or masked cleanup automatically. Do not make it a required component yet.

## Changes, costs and unresolved policy

This is substantial compiler/runtime work, not a Binaryen flag:

- Define a private typed call/resume/result ABI and MIR suspension/liveness transformation. Retained caller state, initialization/ownership bits, cleanup/join progress and return destinations must be explicit; full Rill callable/aggregate layouts still need implementation and adversarial validation.
- Emit step functions and a scheduler dispatcher, variable/aligned record replacement, bounded staging, exact-once argument evaluation/moves and checked pre-write allocation. Preserve old ordinary continuations/borrows without replay or early disposal.
- Place address-taken source locals in stable fixed arenas. Do not retain addresses of temporary step locals. Integrate host/native I/O copy or separately validated no-growth/stable-backing leases, operation identity and actual completion/confirmed-abort retention.
- Account for all generated helper/ABI storage and entry headroom, not only source locals. The probe's conservative 64 KiB runtime charge and allocation of maximum backing arrays are experiment choices, not proposed final constants. Actual target minima/maxima and capacity effects need measurement/documentation.
- Each source transition can add dispatch, record loads/stores and argument/result marshalling. Reserving records before suspension consumes Saved even for calls that never yield. Frame sizes, staging, backing fragmentation/caches and default-depth effects need engineering. No throughput/code-size speedup or slowdown ratio is measured here.
- Statically bounded non-suspending helpers could later remain direct calls, but non-suspending alone does not bound recursion. Any optimization must preserve the same tail, borrow, cleanup and accounting rules in development builds too.

Retain native LLVM fixed-stack/proper-tail work independently; AArch64 incoming-byval limitations, complete ABI shapes, native pre-write probes and migration/borrower reclamation are still open. Fatal guest teardown while a foreign job remains outstanding is not exercised by this new machine's integration; the earlier independent-copy/store-drop control is not a proof of that full composition. A portable Wasm mechanism does not validate clock, filesystem, SQL, TLS or other provider contracts. Browser execution is not tested or added to the original target gate.

## Actual next decision and gate

Ask the user whether to approve explicit records and an iterative dispatcher as the direction for portable Wasm lowering. That approval would leave the complete typed ABI, layout/admission policies and implementation acceptance subject to further design/validation. Keep LLVM retcon optional machinery for a later focused comparison; do not adopt JSPI/host-only fibers or a rejection-only Asyncify patch as the baseline.

Until that decision, this remains a proposal. Even after directional approval, ri-dej remains OPEN for missing full ABI/ownership/cleanup/native/bounds evidence, and ri-48g cannot claim affected lowering implementation-ready merely from these probes. No replacement ADR, target change, public API change, source-rule exception or support-gate closure is made here.
