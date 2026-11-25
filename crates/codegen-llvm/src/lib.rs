//! Owns LLVM code generation from the shared compiler's results.
//!
//! Implementation is absent. This crate depends on `compiler` and on the
//! static LLVM 23.1 bindings (`inkwell`) for the host's native backend only;
//! it provides no lowering or code generation APIs yet. Building it needs the
//! prefix that `mise run setup:llvm` installs, named by `LLVM_SYS_231_PREFIX`.
