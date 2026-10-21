# Emit LLVM IR directly from a compiler written in Rust

Rill must target the same platforms as Rust, so the compiler emits LLVM IR itself and does not generate Rust source for `rustc`. Rill keeps full control over its semantics and error messages, and programs build without a Rust toolchain. The cost is that Rill's compiler must enforce memory safety on its own, because Rust's borrow checker never sees Rill code. The compiler is written in Rust first. Moving it to Rill (self-hosting) is a later, separate effort.

## Considered options

- **Generate Rust source and run `rustc` on it.** Rust's borrow checker would check Rill programs for free and Rust crates would be easy to call. It was rejected because `rustc` errors would show up for Rill code, and Rill's semantics would be limited to what Rust source can express.
