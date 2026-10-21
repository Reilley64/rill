# Whole-program monomorphization through inkwell and LLVM 23

The compiler monomorphizes every generic function, because Rill values are unboxed with fixed layouts, there is no GC to free boxes, and every overload call inside a generic body resolves at compile time. As a v0 simplification, it also builds the whole program, including `std`, from source on every build. Nothing in the language rules out separate compilation. #10's orphan rule means each module's overloads can be checked on their own, and as in Rust, a module could store its declarations and generic bodies as metadata for the modules that use it to monomorphize. Whole-program builds only avoid designing that metadata format and a module cache for now. Ownership is checked on generic MIR before monomorphization, so an error in a generic body is reported once, and inside a generic body `T` moves unless a `where copy(T)` clause allows `copy`. Code generation uses inkwell pinned to LLVM 23 and statically linked into `rill`, because `musttail` through the C API needs LLVM 18, RISC-V `musttail` with `byval` needs 23, and Wasm reference types changed shape in 23.

## Considered options

- **Boxing every value (OCaml, Java).** Generic code is compiled once, but every value needs a heap allocation, which conflicts with fixed layouts and in-place reuse and needs something to free each box. Rejected.
- **Witness tables only (Swift's unspecialized generics).** Generic code is compiled once and receives a hidden table of size, copy, `dispose`, and the overloads its `where` clauses name. Smaller binaries, but every operation on `T` becomes an indirect call. Kept as a possible later fallback for code size, not for v0.
- **Raw `llvm-sys`.** Fewer layers, but every codegen call is `unsafe`. Rejected.
- **Textual `.ll` and external LLVM tools.** The simplest start, but users would need LLVM installed, and the text format has no compatibility promise. Rejected.
