# Whole-program monomorphization through inkwell and LLVM 23

The compiler builds the whole program, including `std`, from source on every build, and monomorphizes every generic function. Overloads are resolved at compile time, #10's orphan rule needs every module in view, and monomorphization needs every caller, so v0 has no separate compilation. Ownership is checked on generic MIR before monomorphization, so an error in a generic body is reported once, and inside a generic body `T` moves unless a `where copy(T)` clause allows `copy`. Code generation uses inkwell pinned to LLVM 23 and statically linked into `rill`, because `musttail` through the C API needs LLVM 18, RISC-V `musttail` with `byval` needs 23, and Wasm reference types changed shape in 23.

## Considered options

- **Raw `llvm-sys`.** Fewer layers, but every codegen call is `unsafe`. Rejected.
- **Textual `.ll` and external LLVM tools.** The simplest start, but users would need LLVM installed, and the text format has no compatibility promise. Rejected.
