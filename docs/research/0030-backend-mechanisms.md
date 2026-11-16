# #30 backend mechanisms: pinned source audit

Checked 2026-11-15. **Static research only:** no IR/Wasm executed, no mechanism harness implemented, no target/build-mode support or safety proved. This note is input to the parent's standalone experiments, not a replacement for their commands/results matrix. Negative or incomplete evidence leaves [#30](https://github.com/Reilley64/rill/issues/30) open. Follow-up execution is recorded separately in [the experiment report](0030-backend-experiments.md); its positive mechanisms do not change this audit's static-only scope.

## Scope and pins

Read #30 and [#22's approved resolution](https://github.com/Reilley64/rill/issues/22), [ADR 0006](../adr/0006-whole-program-monomorphization-on-llvm-23.md), [ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md), [ADR 0015](../adr/0015-language-required-library-contracts.md), and the [library catalogue](../standard-library-contracts.md), especially protected/remaining-field cleanup, causes, native retention, scoped joins and stack settings. Fixed/non-moving addressable stacks, proper tails in every build, masked suspending cleanup and actual borrower completion remain requirements. A source-approved design is not backend validation.

Official source pins used throughout:

- **LLVM `llvmorg-23.1.5` → `392dfff97d29b59d77e5cac6d4b6ef2585fe7117`.** GitHub's [annotated tag object](https://api.github.com/repos/llvm/llvm-project/git/tags/604d7ddfacefc074968a9ba0d88c667ab31b42bd) resolves to that commit. LLVM 23 documentation here is `llvm/docs/LangRef.md`, not the older `.rst` path. This pins upstream rules; it does not certify a Homebrew binary's patch set.
- **Binaryen `version_133` → `fba5005a132d85d578be5bd010e0487ed8e2ce3e`**, [official tag ref](https://api.github.com/repos/WebAssembly/binaryen/git/ref/tags/version_133).
- **Wasmtime `v50.0.1` → `c307ec81de02852d815e9849f7c89d955c03c30d`**, [official tag ref](https://api.github.com/repos/bytecodealliance/wasmtime/git/ref/tags/v50.0.1). Also inspected the downloaded release's C headers under `/tmp/rill-backend-30-tools/wasmtime-v50.0.1-aarch64-macos-c-api/include/`.
- **Wasm tool-conventions** → `25aceba5adba8f3f7cdbadd8e8705784aebcad6a`, specifically [BasicCABI.md](https://github.com/WebAssembly/tool-conventions/blob/25aceba5adba8f3f7cdbadd8e8705784aebcad6a/BasicCABI.md).

Parent-reported environment, **not revalidated by this audit**: Homebrew LLVM 23.1.5 tools at `/opt/homebrew/opt/llvm/bin`; Rust 1.101.0-nightly `41089d07a` with LLVM 23.1.4; Apple clang 21/linker environment; macOS 27 / Darwin arm64. Binaryen 133 and Wasmtime 50.0.1 CLI/C API archives were reported officially downloaded and digest-verified under `/tmp/rill-backend-30-tools`. Rust's embedded LLVM is not the same revision as the separately installed LLVM 23.1.5. Record executable paths, complete version/build outputs, digests, triples, codegen/link flags and build modes with the experiments; these reported versions alone do not establish reproducibility.

## 1. LLVM: legal IR first, then target/ABI lowering

### Exact mandatory-tail rules

The pinned [call specification][L-call] says `tail` is a hint, whereas `musttail` is a correctness requirement, including with `disable-tail-calls`, and bounds stack growth in recursive call-graph cycles. Both markers promise that the callee does **not** access the caller's allocas, varargs or byval storage, except through the explicit byval-copy exception. An ordinary final-looking source call is not permission to assert that promise.

For `musttail`:

- Call followed immediately by return of the call result, `undef`, or `void`; no post-call cleanup, observer, wrapping or join. LangRef mentions a pointer bitcast; the pinned [verifier][L-verifier] actually checks the next instruction is `ret`, and requires exact return-type equality. For these tools, use the stricter verified shape rather than relying on a historical bitcast allowance.
- Caller function convention and call convention match; the call's convention must also match the actual target definition, including indirect targets. Casting a function pointer does not repair an ABI mismatch.
- Normally caller/callee prototypes and ABI-impacting attributes match (`sret`, `byval`, `inreg`, `returned`, `inalloca`, etc.). `tailcc`/`swifttailcc` allow differing caller/callee parameter prototypes, but only `sret`, `byval`, `swiftself`, `swiftasync` among ABI-impacting attributes; the verifier still checks identical return types and rejects varargs for these conventions. This does **not** allow lying about an individual call's actual callee signature.
- A returned aggregate must **not** undergo automatic lowering to a hidden `sret` pointer. Verification alone cannot settle target return legalization. Small direct aggregates and a deliberately explicit `void(ptr sret(T), ...)` ABI are separate cases, not one blanket "aggregate supported" result. The [parameter specification][L-params] requires an `sret` function to return void and the return buffer to be valid/aligned.

The `tailcc` [convention description][L-cc] and [codegen documentation][L-codegen] are not a portable ABI guarantee: target restrictions still apply. `musttail` is preferable for an experiment that must fail rather than silently become an ordinary call. Run verification on the actual IR and inspect codegen in both development and optimized modes. A verifier pass does not detect every invalid lifetime promise.

### Candidate and rejected ABI cases

| Case | Source-audit conclusion / experiment boundary |
| --- | --- |
| Scalar self/mutual/indirect cycles | Legal candidates with matching conventions/signatures, valid operands and no caller-local dependency. Indirect pointer targets must really have the declared ABI. Test each topology separately; a self-recursion-to-loop optimization says nothing about indirect transfers. |
| Native x86-64 `tailcc` + byval forwarding/shuffling | Plausible target candidate, not a pass result. The [X86 lowering][L-x86] explicitly handles byval overlap using temporary copies and moving arguments into the incoming area. Include large/aligned arguments and account for those temporaries/outgoing areas. Do not generalize to AArch64. |
| AArch64 function **with incoming byval parameter** | **Known source-level backend rejection:** [AArch64 eligibility][L-arm] returns false for any caller argument with byval, before its guaranteed-TCO convention check. Its LowerCall diagnoses failed `musttail`. Thus “LLVM 23 accepts byval in LangRef” does not establish this ABI shape on macOS or Linux AArch64. Test and report a compile failure, not an ordinary-call fallback. |
| Explicit `sret` forwarding | Candidate when buffer belongs to an earlier live caller or other sufficiently retained storage, not the eliminated activation. Return void directly after the call; preserve ABI attributes/convention. A current-frame temporary passed as the callee's return buffer is not a valid mandatory-tail workaround. Test actual result bytes as well as bounded space. |
| Large direct aggregate return | **Not established**; reject as a positive test if target legalization needs automatic `sret`. Probe compilation only until return ABI is known. Scalarized values or an explicit return-buffer ABI can be investigated without claiming that they already implement Rill's complete ABI. |
| Borrowed caller local / pending guard / join / explicit wrapping | Keep the source call ordinary when obligations survive it. In particular, **do not execute** IR that uses `tail`/`musttail` to forward a local to an unannotated pointer parameter that the tail callee dereferences. Such IR violates the marker's semantics even if verifier/codegen accepts it. |

For wasm32, [LLVM's actual supported-convention list][L-wasm] **does not include `CallingConv::Tail` (`tailcc`)**. It includes C/Fast and Swift/SwiftTail, among others. Start scalar wasm experiments with a supported convention and matching `musttail` signatures, plus `+tail-call`; do not copy a native `tailcc` invocation unchanged. LowerCall requires the tail-call target feature, rejects varargs tails, compares legalized return types and rejects arguments traced to an IR alloca. Nonzero byval arguments are lowered via a newly created stack object and memcpy. These are additional reasons **not to certify wasm byval/aggregate tails from IR verification or scalar success**. The wasm C ABI itself generally passes nontrivial aggregates indirectly ([calling sequence][W-abi]); this is not evidence every such shape is eligible.

### Valid observational sampler, not a lifetime loophole

A candidate sampler may observe the **currently live** activation before its mandatory transfer: take a current-frame address, convert to an integer and update constant-size min/max/count statistics, or call an ordinary synchronous observer before the `musttail`. That observer must finish all access before it returns. The tail callee receives only genuinely valid arguments, never the sampled local's pointer for later use. No instruction may be inserted between `musttail` and `ret`. These constraints follow directly from [L-call].

Compare sufficiently long chains at increasing counts, preserve a non-tail recursion control, and inspect assembly/Wasm so optimization cannot erase the intended mechanism. Use constant-space observation rather than retaining one sample per transfer. A narrow address range only observes that stack location; it does **not** prove alloca/outgoing-argument peaks, hidden heap/saved history, stack switching, borrow safety or all targets. Do not compare/dereference a stale local pointer after frame elimination. Frame-address or stack intrinsics also have target-specific limitations; they are not a universal stack-capacity API.

## 2. Binaryen 133: the Asyncify incompatibility persists

The old [inspected commit][B-old] and pinned [version 133 source][B-analysis] have the same explicit fatal checks in `ModuleAnalyzer`: direct `Call::isReturn` and indirect `CallIndirect::isReturn` produce **"tail calls not yet supported in asyncify"** (lines 634-658). These checks occur during analysis, before "needs instrumentation" filtering; remove/only/ignore-indirect lists are not a demonstrated tail-call escape hatch. A later return-call special case inside catch-unwind checking is not implementation of tail suspension.

The pinned [pass registry][B-registry] registers **`tail-call`**, and [TailCall.cpp][B-tail] converts eligible ordinary calls **to** `return_call`/`return_call_indirect`/`return_call_ref`. It is not a `tail-call-lowering` pass that removes them. No pass named `tail-call-lowering` is registered at this pin. A feature enable/disable flag is not evidence of a semantics-preserving lowering.

**Replacing return calls with ordinary call+return does not preserve Rill proper-tail semantics.** Asyncify [documents saving call indices and locals][B-layout]; its [locals instrumentation][B-save] emits a call-index push plus relevant-local saving for each unwound instrumented activation. Converting a formerly proper-tail chain into ordinary calls therefore admits depth-dependent host frames before suspension and depth-dependent saved activation history on suspension. Later optimizing a few calls does not establish a bounded chain. Re-running `tail-call` after Asyncify cannot in general remove the post-call unwind checks and save/restore obligations that the transform has introduced. No bounded-history workaround is established by this audit.

There is also a **distinct saved-budget containment gap** in stock Asyncify: [makeCallIndexPush][B-save] contains the explicit TODO "add a check against the stack end here"; makeLocalSaving emits stores and advances the position without a pre-store end check. [Runtime control exports][B-controls] check position versus end, but that is not a check before each saved-state write. A too-small logical saved region inside a larger memory can therefore be crossed before a later trap. Whole-memory bounds, a post-unwind check or `asyncify-asserts` are not proof of #30's pre-corruption budget containment. A separate bounded secondary memory can change physical containment, but does not resolve proper-tail incompatibility, checked arithmetic or the specified logical-budget contract.

## 3. Wasmtime 50: a host-specific experiment, not portable adoption

### Tail calls and async execution are present mechanisms

Wasmtime's [tail-call configuration][T-config] defaults true. Its [proposal status][T-proposals] lists tail calls, with a Winch Tier-1-support caveat; use **Cranelift** for this candidate, not an unspecified strategy. [AArch64 lowering][T-arm-tail] implements direct and indirect return calls. Its own [bounded-recursion test][T-tail-test] contrasts ordinary exhaustion with a million tail calls. These are upstream evidence, not tests run here and not Rill support evidence.

The [C async API][T-async-h] executes Wasm on a separate stack and yields via fuel, epochs or async host functions. The [implementation][T-async-rs] calls Rust `Func::call_async`; [fiber execution][T-fiber] suspends/resumes a native fiber rather than serializing each Wasm caller into a Binaryen saved-local buffer. The default Unix [fiber allocation][T-unix] uses a fixed mmap region, page rounding and a guard page. Consequently, **leaving Wasm return-call instructions intact and testing an async host import is a legitimate candidate experiment** on AArch64; Asyncify rejection does not imply that Wasmtime's host-fiber mechanism rejects tail calls. The composition, including indirect tails, result ABIs, long repeated suspension and resource accounting, still needs the parent's measurements.

**Architecture boundary:** this is an embedding API offered by a particular host. Neither a WASIp1 module nor a browser deployment thereby acquires that host's fiber API, scheduling or budget enforcement. This does not select a new Rill architecture, authorize a target drop, or replace the approved separate managed addressable/saved budgets with one host stack limit. Mapping engine-retained activations/reservations to the saved-budget contract remains unresolved; "no Asyncify allocation" is not permission to stop accounting saved state.

### Current C API details worth using, and a stale-doc trap

Use the full release with `WASMTIME_FEATURE_ASYNC`, ordinary engine/store creation, then:

- `wasmtime_config_wasm_tail_call_set`, `wasmtime_config_max_wasm_stack_set`, `wasmtime_config_async_stack_size_set` before engine creation.
- `wasmtime_linker_define_async_func` with a `wasmtime_async_continuation_t` (callback, environment, finalizer); `wasmtime_linker_instantiate_async`, `wasmtime_func_call_async`, `wasmtime_call_future_poll`, `wasmtime_call_future_delete`.
- Optional scheduling observations: `wasmtime_config_consume_fuel_set`, `wasmtime_context_set_fuel`, `wasmtime_context_fuel_async_yield_interval`; or epoch instrumentation/deadline/yield APIs. A fuel yield interval does not refill an exhausted total fuel allowance.

There is **no `wasmtime_config_async_support_set` declaration/implementation at this pin**. Some async-header prose still refers to an "async config/store"; follow the pinned [official C example][T-example], which creates an ordinary config/store without that old setter, and the current source, rather than importing older-version setup instructions. The async C feature is distinct from component-model async feature options. [T-async-h], [T-async-rs], [T-example]

Important lifetime/exclusivity rules from [T-async-h]:

- Keep call/instantiation inputs, result/trap/error slots, context and store alive and unmodified until the future is **deleted**, not merely until one poll returns false/true. Poll false means yielded; poll true means complete, after which do not poll again. Delete completed instantiation future before starting the call future.
- Only one call future may be alive per store. Do not call unrelated store APIs from the outer driver while it is suspended. `caller` is temporary and usable only in the initial host callback; retaining it in a continuation is invalid. The continuation may retain its documented argument/result/trap storage until completion, but that does not pin a guest linear-memory allocation.
- A continuation finalizer is resource destruction, **not proof of foreign-operation completion**. The [Rust continuation Drop][T-async-rs] runs the finalizer even when dropped. Deleting an unfinished call future drops its Rust future; [FiberFuture::drop / StoreFiber::dispose][T-fiber] requests fiber termination via an error. That is neither Rill cancellation masking nor a guaranteed native-worker join. An external job must independently retain its resources until actual completion/confirmed abort.

### Three different storage domains

1. **Addressable linear stack:** guest offsets/regions used for address-taken values. The [C ABI][W-abi] explicitly separates this from the engine call stack; some functions use no linear frame.
2. **Saved activation state:** Asyncify's serialized call indices/locals, or other state retained during suspension. Wasmtime native async fibers retain their machine frames instead; a replacement implementation needs an explicit budget/accounting explanation, not a renamed allocation.
3. **Host/engine stack:** native execution and host-call frames. Wasmtime's [max_wasm_stack / async_stack_size docs][T-stack-config] set separate host-mechanism limits: default 512 KiB Wasm limit and 2 MiB async stack. The Wasm limit must be nonzero and cannot exceed async stack size; headroom for host functions is `async_stack_size - max_wasm_stack`. Host functions are not prevented from exhausting the underlying stack, which can abort the process. These defaults are **not** Rill's two separate 1 MiB managed defaults.

A host stack-overflow trap must be fatal to the Rill invocation, although an embedding host can survive the trap. That does not promise user cleanup after overflow. Custom async stack creators (`wasmtime_config_host_stack_creator_set`) and pooling limits can expose allocation/finalization observations, but are host-specific low-level mechanisms, not complete managed-stack checking. Completed fibers may be [cached by the store][T-cache]; lack of immediate RSS decrease is not by itself a leak or a still-live Rill activation.

## 4. Memory.grow, safe observations and retained native access

### Documented mechanisms, not owner-count folklore

Wasmtime [Memory safety docs][T-memory] explicitly warn that growth can relocate the base and reentry can mutate/alias memory. A `wasmtime_memory_t` is a store-bound handle, not an independent lifetime owner of the old base. The following are distinct candidate mechanisms:

- **Copy:** copy checked input bytes to independently owned host memory before asynchronous native access; keep output host-owned until completion and copy back only while the destination, store and exclusive access are valid. Rust's documented `Memory::read`/`write` are copy operations; C exposes raw data/size, so the embedder must do the checked synchronous copy. Copying changes the physical buffer accessed and needs a defined completion/copyback protocol; it does not justify retaining a guest pointer.
- **Exclusive no-growth lease:** prevent *all* guest reentry/growth and embedding growth while a foreign pointer is retained, and keep the store/storage alive. This is an embedder protocol derived from [T-memory]'s invalidation rules, not a stock `pin`/lease API discovered in the C headers. Blocking a single known growth call is insufficient. The C call-future exclusivity rule helps serialize one store but is not a general guest-memory pin.
- **Stable reservation / growth coordination:** [Config][T-memory-config] documents `memory_may_move(false)`: memory cannot grow beyond its original allocation. Tune `memory_reservation` / `memory_reservation_for_growth` for the required maximum; growth beyond capacity fails rather than silently moving. Pooling memories also never move. This preserves base stability only with appropriate lifetime/access discipline; it is host configuration, not universal WASI/browser behavior or unconditional successful growth.
- **Custom allocation:** C exposes `wasmtime_config_host_memory_creator_set`, `wasmtime_memory_creator_t`, `wasmtime_linear_memory_t` and grow/get/finalizer callbacks ([header][T-memory-creator]). The [Rust safety contract][T-memory-custom] requires page alignment, guard/reservation correctness and no relocation for growth within reported capacity. This permits investigating independently retained backing owners and growth refusal while leased. It supplies neither an implemented borrower state machine nor permission to keep using the old version after a relocating copy.
- **SharedMemory:** [SharedMemory::data][T-shared-rs] guarantees a stable base while that handle lives; growth does not move it. The [C API][T-shared-h] provides independent reference-counted `wasmtime_sharedmemory_clone`/`delete`, data/size and grow calls, not requiring store access. This is a candidate for retained storage beyond store drop, **not an automatic choice for Rill**: it requires shared-memory/threads support and a justified synchronization/access protocol. The pinned [proposal-status notes][T-proposals] also identify resource-limiter integration gaps and lack of shared-memory support in pooling. Base stability does not authorize races or ordinary concurrent mutable slices. Do not use this as evidence that ordinary unshared memory is stable.

### Safe relocation observation for the parent

A controlled unshared-memory experiment can set on-demand allocation (not pooling), `wasmtime_config_memory_may_move_set(true)`, `wasmtime_config_memory_reservation_set(0)` and `wasmtime_config_memory_reservation_for_growth_set(0)`. Use a small initial memory with a sufficiently larger maximum. [The pinned configuration docs][T-memory-config] describe growth beyond capacity causing a new allocation/copy; [the implementation][T-memory-grow] checks the relevant capacity/movement constraints. Default 64-bit reservation is 4 GiB and growth slack is 2 GiB, so leaving defaults may conceal relocation in small tests.

Use [C memory calls][T-memory-h]: `wasmtime_memorytype_new` (current signature includes `page_size_log2`), `wasmtime_memory_new` or an exported memory, `wasmtime_memory_data`, `wasmtime_memory_data_size`, `wasmtime_memory_size`, `wasmtime_memory_grow`. Before growth, check `offset <= size` and `length <= size - offset`, copy/check initialized bytes and record the base **as `uintptr_t` while valid**. End all raw-memory borrows/access. Grow; on success reacquire data/size and validate bytes through **only the new pointer**. Compare the saved integer address with the new integer address; never dereference, subtract or reuse the stale pointer. Handle returned growth error without reading unchanged `prev_size` as success. This tests relocation/data preservation, not safe stale-pointer use.

**Do not grow the same store from the outer driver while its async call future is pending.** The C API's exclusivity contract and [fiber KeepStore rule][T-fiber] prohibit that supposed "concurrent grow" probe. Separate legal observations:

- Relocating growth between completed/deleted futures, with an external borrower holding only an independent copy; or guest growth during a call after a host operation has handed off a copy and returned. This demonstrates growth while copied native work is outstanding, not same-store reentry during a pending core async host continuation.
- A pending async host callback retaining a copied buffer, with completion/finalizer ownership observed, separately from the relocating-growth control. Combining the two controls does not prove the missing exact interleaving.
- Shared-memory growth through the separately owned shared-memory handle while a core call is suspended can investigate stable-base retention without unrelated store calls, subject to synchronization. That is a different memory mechanism, not relocation evidence.

If the required same-unshared-memory suspended-growth scenario cannot be expressed under the selected embedding's legal access model, mark that cell **blocked/untested**, rather than violate its API or silently substitute a copy-only baseline. A different sanctioned embedding/scheduler mechanism requires explicit design review.

### Trap, store drop and borrower completion

[Store documentation][T-store] says instances remain allocated until store drop; [memory docs][T-memory] say ordinary Memory belongs to its store. Thus a caught host trap with the store retained is different from dropping the store. Keep execution futures deleted before store deletion, and do not keep ordinary-memory pointers live beyond store lifetime. Retaining an engine/module/handle does not replace retaining its memory allocation.

To exercise a surviving native borrower after fatal Rill termination, the embedding needs an independent host-copy/custom-backing owner or retained SharedMemory handle, or must defer store destruction until the borrower completes. The foreign job, not merely an async continuation's environment, must own the required retention. Observe release after actual borrower completion/confirmed abort; a trap/cancel request/handle drop is not that event. After store drop, only access independently retained host storage under its own contract. Never validate the experiment by reading freed guest memory. No global join or Rill cleanup after fatal termination is implied by successful host retention.

## 5. Bounds, cleanup and limits of the proposed evidence

- LLVM's [`probe-stack` / `stack-probe-size` attributes][L-probes] ensure probe/access spacing relative to a guard region; they are **not a complete managed-stack instrumentation scheme** or capacity API. Target lowering, supported probe implementation, alignment/padding, ABI copies/return temporaries, outgoing argument allocation and runtime/native entry boundaries still need audit and adversarial containment tests. A requested attribute or a guard page does not prove generated accesses cannot skip managed limits.
- Binaryen [`stack-check`][B-stack] instruments assignments to LLVM's `__stack_pointer` and exposes `__set_stack_limits`; it is not a check on every memory access, saved-state push or host frame. The [Wasm C ABI red zone][W-abi] allows leaf accesses below SP without an SP update. Checked pointer arithmetic and complete frame/temporary accounting are separate requirements; a post-wrap SP comparison cannot establish them. Overall linear-memory bounds permit corruption of adjacent *in-bounds* regions.
- Wasmtime engine-stack limits/guards are not checks of a Rill linear-stack arena or Asyncify saved region. Its AArch64 backend has [inline probe code][T-arm-probes], while a different out-of-line probe generator is explicitly unimplemented; "AArch64 has no probes" and "all managed accesses are covered" are both unjustified conclusions. Inspect the actual selected codegen, not just a matching function name.
- [ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md) requires masked disposal/required joins that may suspend indefinitely, and actual cessation of native access. [ADR 0015](../adr/0015-language-required-library-contracts.md) and [catalogue §§2,4,6,11](../standard-library-contracts.md) require exactly-once remaining-field cleanup, full cause-chain rendering/release with bounded standard traversal, protected actions, and bounded fatal reporting. LLVM tail-call legality and Wasmtime future Drop implement none of those Rill semantics.
- A candidate iterative cleanup dispatcher/worklist must preserve order/dependencies/initialization state across suspended callbacks and deferred cancellation, then join borrowers before release. Worklists may legitimately scale with pending cleanup/cause data; they must not store one eliminated caller activation per eligible tail transfer. Inject worklist allocation failure and take the fatal path, **not recursive fallback**. Arbitrary user callback/disposer recursion is outside the standard traversal bound. A standalone manual worklist demonstrates only that mechanism, not compiler-generated default cleanup or a sound mask/ownership checker.
- Fatal limit tests need isolated processes/instances and adjacent-region canaries, with failure detected **before** an out-of-budget write. Test native/addressable/saved/host exhaustion separately. Small successful tail chains, no crash, one unchanged base pointer, or bounded sample range are not universal-depth evidence or cross-target runtime validation. No execution occurred in this research portion.

## 6. Actionable disposition

**Implementation experiments without changing the contract:** verified scalar native/wasm mandatory-tail cycles; x86 byval and explicit return-buffer ABI variants; negative AArch64 incoming-byval compilation; Binaryen direct/indirect rejection; untouched Wasm return calls plus Wasmtime async import; safe relocating-growth/copy controls and stable-reservation/shared-memory contrasts; iterative ordered/masked cleanup and allocation-failure models. Record each as mechanism pass/fail/untested, not Rill acceptance.

**Human decision if the required scope remains missing:** a portable suspension + proper-tail lowering when Asyncify rejects tails; a complete AArch64/wasm aggregate ABI that cannot silently fall back; how any host-fiber alternative obeys saved-budget accounting and the WASIp1/browser deployment contract; and any required change of architecture, target scope or approved semantics. The existence of a Wasmtime-specific experiment does not decide those questions. Missing budget-before-corruption/native-lease/cleanup evidence remains an implementation blocker even if a tail-loop test passes. Keep #30 open while that evidence or a separately tracked, explicitly approved scope decision is absent.

## Pinned primary-source references

All LLVM/Binaryen/Wasmtime links below use the exact commits listed above; local Rill links are the design inputs read in this checkout, not a claimed implementation revision.

[L-call]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/docs/LangRef.md#L13520-L13644
[L-params]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/docs/LangRef.md#L1364-L1475
[L-cc]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/docs/LangRef.md#L498-L515
[L-verifier]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/lib/IR/Verifier.cpp#L4165-L4236
[L-codegen]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/docs/CodeGenerator.md#L1767-L1834
[L-arm]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/lib/Target/AArch64/AArch64ISelLowering.cpp#L9800-L9873
[L-x86]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/lib/Target/X86/X86ISelLoweringCall.cpp#L2047-L2265
[L-wasm]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/lib/Target/WebAssembly/WebAssemblyISelLowering.cpp#L1264-L1405
[L-probes]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/docs/LangRef.md#L2606-L2637
[W-abi]: https://github.com/WebAssembly/tool-conventions/blob/25aceba5adba8f3f7cdbadd8e8705784aebcad6a/BasicCABI.md#L123-L206
[B-old]: https://github.com/WebAssembly/binaryen/blob/74ff1eb9cc8393f16854014667e540a3ba883b97/src/passes/Asyncify.cpp#L634-L658
[B-analysis]: https://github.com/WebAssembly/binaryen/blob/fba5005a132d85d578be5bd010e0487ed8e2ce3e/src/passes/Asyncify.cpp#L530-L667
[B-registry]: https://github.com/WebAssembly/binaryen/blob/fba5005a132d85d578be5bd010e0487ed8e2ce3e/src/passes/pass.cpp
[B-tail]: https://github.com/WebAssembly/binaryen/blob/fba5005a132d85d578be5bd010e0487ed8e2ce3e/src/passes/TailCall.cpp#L205-L319
[B-layout]: https://github.com/WebAssembly/binaryen/blob/fba5005a132d85d578be5bd010e0487ed8e2ce3e/src/passes/Asyncify.cpp#L106-L187
[B-save]: https://github.com/WebAssembly/binaryen/blob/fba5005a132d85d578be5bd010e0487ed8e2ce3e/src/passes/Asyncify.cpp#L1510-L1724
[B-controls]: https://github.com/WebAssembly/binaryen/blob/fba5005a132d85d578be5bd010e0487ed8e2ce3e/src/passes/Asyncify.cpp#L1956-L2008
[B-stack]: https://github.com/WebAssembly/binaryen/blob/fba5005a132d85d578be5bd010e0487ed8e2ce3e/src/passes/StackCheck.cpp#L60-L182
[T-config]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/config.rs#L933-L946
[T-proposals]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/docs/stability-wasm-proposals.md#L18-L63
[T-arm-tail]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/cranelift/codegen/src/isa/aarch64/lower.isle#L2646-L2669
[T-tail-test]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/tests/misc_testsuite/tail-call/bounded-recursion.wast
[T-async-h]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/c-api/include/wasmtime/async.h
[T-async-rs]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/c-api/src/async.rs
[T-example]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/examples/async.cc
[T-fiber]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/runtime/fiber.rs
[T-unix]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/fiber/src/unix.rs#L163-L215
[T-stack-config]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/config.rs#L818-L880
[T-cache]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/runtime/store/async_.rs#L204-L224
[T-memory]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/runtime/memory.rs#L43-L178
[T-memory-config]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/config.rs#L1900-L2121
[T-memory-grow]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/runtime/vm/memory.rs#L661-L706
[T-memory-h]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/c-api/include/wasmtime/memory.h
[T-memory-creator]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/c-api/include/wasmtime/config.h#L561-L646
[T-memory-custom]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/runtime/memory.rs#L728-L815
[T-shared-rs]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/runtime/memory.rs#L923-L943
[T-shared-h]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/c-api/include/wasmtime/sharedmemory.h
[T-store]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/runtime/store.rs#L138-L195
[T-arm-probes]: https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/cranelift/codegen/src/isa/aarch64/abi.rs#L748-L774
