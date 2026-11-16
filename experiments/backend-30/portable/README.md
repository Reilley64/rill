# Portable explicit-record candidate. Throwaway mechanism probes

**Question:** can a portable Wasm lowering combine proper tails, suspension and independently checked retained-state/addressable regions without Asyncify or a parked engine fiber?

**Answer for this finite manually lowered machine:** yes. This is **not adoption of a Rill architecture**, automated compiler/checker output, complete owned-value ABI or production runtime support. See the [experiment/proposal report](../../../docs/research/0030-portable-suspension-experiments.md) and [pinned candidate research](../../../docs/research/0030-portable-suspension-candidates.md). Earlier #30 evidence is untouched; the support gate remains OPEN.

## Reproduce

Use the LLVM/LLD 23.1.5, Binaryen 133 and Wasmtime 50.0.1 ARM64 C API already downloaded by the [earlier runner](../README.md). Recorded host is macOS/AArch64 with Node 26.13.0 and Xcode SDK. No new dependency installation, server or Docker execution is required for these probes.

```sh
python3 experiments/backend-30/portable/run.py --out /tmp/rill-30-portable-reproduce
python3 experiments/backend-30/portable/retcon.py --out /tmp/rill-30-retcon-reproduce
```

The first runner accepts `--llvm`, `--linker`, and `--tools` overrides like the earlier harness. The small secondary script uses the same recorded default tool locations. Assertions must remain enabled. Both runners save every command, exit code, stdout/stderr, generated IR/object/Wasm/WAT and machine-readable observations under their output directories. Binaries/tools are not checked in. `evidence/` contains the recorded JSON snapshots; a zero runner exit means expected observations reproduced, not #30 closure.

## Candidate machine

`machine.c` is manually lowered C, compiled as freestanding wasm32-wasip1. C arena casts deliberately use LLVM/Clang's `-fno-strict-aliasing` mode; this is an implementation-specific probe, not a claim of portable strict-ISO-C object typing. LLVM verification and Wasm validation are separate checks. All payloads are primitive integers or initialized byte patterns; owned-value moves/destruction remain unimplemented.

A routine has a fixed addressable arena, saved arena, charged control header and bounded argument staging. An iterative dispatcher invokes one step at a time through a private fixed function table:

- Ordinary calls push a record and retain caller storage/return progress.
- Eligible self/mutual/indirect transfers replace the current record. Records vary from 64-256 bytes with 16-64 byte alignment; replacement preserves the earlier ordinary caller and charges padding/staging before writes.
- A suspended step stores its PC/token/state and returns through `poll` to the host. Wake resumes the actual current function/PC, not the original tail-entry path. Effects are checked once, including across repeated polls and yields.
- Arguments, return/result state, links, PC/function IDs, token generation, live scratch, observational counters and allocator bookkeeping all reside in charged records/header. The 256-byte staging packet is in the **416-byte header**, not an exempt allocation.
- The complete header is charged from routine initialization. Ordinary records are charged even before a yield. These conservative admission/layout policies are experimental architecture choices, not already approved implementation rules.
- Per-routine maximum array backing is fixed, not allocated per transfer. Each fixture routine reserves **2,097,728 bytes** for maximum arena backing/header/alignment; unused capacity is not live activation data. Logical Saved includes the header and actual record/padding extents. The fixture's fixed backing does not establish production allocation/reclamation or a total heap quota.

### Transient C-ABI stack is not free

The module reserves a shared **64 KiB** compiler/runtime linear stack, placed first by the linker. Every routine conservatively charges that full amount against its addressable capacity **before any routine step is admitted**. Thus a 1 MiB addressable setting does not mean 1 MiB of user slots plus free runtime storage; this fixture's minimum addressable setting is 64 KiB + 64 bytes. Bootstrap/configuration operations run on the already reserved bounded runtime stack, not an unadmitted source routine's frame.

`run.py` checks the complete linked Wasm call graph, including all private indirect-table targets; it rejects recursion, table mutation, unknown imports and tail/ref-call instructions. All reported Clang frame usages must be static. The sum of all defined-function frame sizes plus **128-byte leaf red-zone + 16-byte alignment allowance per function** is a conservative bound for this acyclic fixture, below the charged reserve in both modes. This is not a general proof for arbitrary compiler output: new helpers, recursion, dynamic allocations, ABI shapes, callbacks or reentry invalidate the argument unless analyzed/instrumented. Engine-native stacks remain separate.

`--mvp-features` validation and disabled codegen tail/multivalue/reference/bulk extensions check that the mechanism does not require a new suspension instruction. Wasmtime explicitly disables tail-call support and uses a **32 KiB engine stack limit**. The target remains wasm32-wasip1; the fixture has a single `env.fatal` import and is **not a complete WASI command/provider implementation or browser test**.

## Embedding and failure rules

- `check.mjs` runs tail/ordinary/multi-routine, exact-fit, malformed-protocol and capacity cases in Node/V8. Fatal cases use fresh instances; after fatal escape, only external memory reads inspect pre-recorded canaries. No source routine or diagnostic Wasm export is re-entered.
- `host.c` runs the same module synchronously in Wasmtime/Cranelift with tail calls disabled. No async call future or fiber exists at a logical yield. Between completed `poll` calls it legally forces ordinary-memory relocation while a pthread worker owns an independent input/output copy. Copyback uses fresh memory/extent and the retained destination, after actual worker join. Token/cancel-request/handle models cannot release the slot or wake it early. This is not zero-copy pinning or production Rill cancellation.
- `host.c` also injects terminal guest failure while copied native work remains pending, destroys the guest store without re-entering it, then lets the real worker access its independent copy. It joins before freeing that copy and never performs post-fatal copyback. Host-created traps may surface as a Wasmtime C-API error rather than a core trap; both are treated as terminal to the invocation, not a normal source result. This extends the original candidate run; the earlier report/snapshots describe their original scope.
- Borrowed ordinary slots and modeled guard/join/wrapping phases remain live through child return/yield. A malformed tail effect with an outstanding obligation traps before replacement. These are manually chosen source-like cases, **not checker eligibility evidence**. Cancellation/masking/cleanup/joins are modeled phases; no canonical Err or disposer implementation is claimed.

## Secondary LLVM retcon probe

`retcon.py` generates valid LLVM 23 returned-continuation IR with the exact wasm32 data layout, splits it, then runs O0/O2 ordinary-call modules without tail instructions. `retcon-check.mjs` checks one million resumes, final destruction and allocation balance:

- 16-byte/8-aligned inline buffer: no fallback allocation.
- Deliberately 4-byte/4-aligned advertised buffer: one **8-byte** fallback frame, freed once.
- A seven-byte fallback region terminates in the checked allocator before the generated eight-byte frame store; adjacent canary stays intact. Allocator never returns null.

This demonstrates spill/split/allocation machinery, not source tail-call elimination, a complete saved ledger, over-aligned frames, variable ABI shapes or suspendable Rill cleanup. Repeated `return await` can still retain parent objects unless a frontend explicitly replaces them. The pointer-result resume ABI and fixed buffers here are not approved public/compiler interfaces.
