# Whole-program monomorphization through inkwell and LLVM 23

The compiler monomorphizes every generic function, because Rill values are unboxed with fixed layouts, there is no GC to free boxes, and every overload call inside a generic body resolves at compile time. As a v0 simplification, it also builds the whole program, including `std`, from source on every build. Nothing in the language rules out separate compilation. #10's orphan rule means each module's overloads can be checked on their own, and as in Rust, a module could store its declarations and generic bodies as metadata for the modules that use it to monomorphize. Whole-program builds only avoid designing that metadata format and a module cache for now. Ownership is checked on generic MIR before monomorphization, so an error in a generic body is reported once, and inside a generic body `T` moves unless a `where copy(T)` clause allows `copy`. Code generation uses inkwell pinned to LLVM 23 and statically linked into `rill`, because `musttail` through the C API needs LLVM 18, RISC-V `musttail` with `byval` needs 23, and Wasm reference types changed shape in 23.

## Stack and target support qualification

[ADR 0014](0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) qualifies #8/#13's earlier tail-position claims: semantic proper-tail transfer needs no retained caller storage or post-call cleanup/joins. Pending obligations make a final-looking call ordinary; ABI inconvenience must not silently reclassify an eligible call. Honor eligible transfers in development and optimized builds or report actionable unsupported lowering. No early guard disposal or accumulating hidden heap activation chain.

The proposed LLVM tail-call plus Binaryen Asyncify pipeline is provisional, not validated: the inspected Asyncify revision rejects tail-call instructions. [#30](https://github.com/Reilley64/rill/issues/30) gates support on pinned reproducible target/build evidence, including ABI cases, probes/outgoing arguments, distinct Wasm addressable/saved/engine resources, memory growth and host borrowers, iterative cleanup and reclamation. A callee-only check or native-style guard-page assumption is insufficient. If no mechanism preserves the contract, return backend/scope to the map rather than weaken it or claim a replacement architecture already chosen. Exact lowering/checking remains #28 work; this ADR does not establish a working compiler/runtime.

## Shared diagnostic front end and editor delivery

[ADR 0016](0016-structured-diagnostics-and-complete-openidea-lsp.md) / #24 define shared source/context/checking facts for terminal, version-1 JSONL and LSP adapters. `rill check` and the native server analyze without LLVM executable generation or running startup/expectations/user formatters; success is front-end scope, not backend certification. Workspace indexing cannot add bodies to an entry's loaded implementation graph. Static ambiguity, missing inference, selected-body requirements and collection duplicates have different explanations; no new dispatch rule is introduced.

Complete applicable OpenIDEA v0.1.44 LSP support is prioritized in [#42](https://github.com/Reilley64/rill/issues/42), with an unpassed acceptance matrix and explicit host/client constraints. It gates tooling readiness, not drafting #28 RFCs. Exact inference/MIR/cleanup/trusted observation and backend evidence remain #28/#30. The Rust server's resources are not the analyzed program's stack budget.

## Considered options

- **Boxing every value (OCaml, Java).** Generic code is compiled once, but every value needs a heap allocation, which conflicts with fixed layouts and in-place reuse and needs something to free each box. Rejected.
- **Witness tables only (Swift's unspecialized generics).** Generic code is compiled once and receives a hidden table of size, copy, `dispose`, and the overloads its `where` clauses name. Smaller binaries, but every operation on `T` becomes an indirect call. Kept as a possible later fallback for code size, not for v0.
- **Raw `llvm-sys`.** Fewer layers, but every codegen call is `unsafe`. Rejected.
- **Textual `.ll` and external LLVM tools.** The simplest start, but users would need LLVM installed, and the text format has no compatibility promise. Rejected.
