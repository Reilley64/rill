# Target-runtime support

This directory owns target-runtime support, including future compiler-required native support and target-specific runtime inputs. Rill standard-library algorithms belong in [`stdlib/`](../stdlib/README.md). No Rill lowering or production ABI connects these inputs to Rill programs yet.

The [ownership archive](ownership/lib.rs) supplies the counted-owner retain and release operations, lock guard release, and channel endpoint retain and release for the first `aarch64-apple-darwin` profile. The Rust guard borrows its mutex until release, and blocking channel calls borrow their endpoints until return. There is no detached native I/O in this archive. Goroutine scheduling, cancellation, native I/O release and backend lowering still need separate implementations.

The [compiler build](../crates/compiler/build.rs) produces `librill_ownership.rlib` in Cargo's `OUT_DIR` for that target. To stage the scoped provider, copy that exact archive from the same compiler build into `lib/rill/runtime/aarch64-apple-darwin/`. Stage the corresponding [standard-library sources](../stdlib/README.md) from the same revision. The installed loader verifies the bytes; it does not load or run Rust code from the installed archive. The linked [Rust tests](ownership/tests.rs) exercise the native operations without running Rill or validating a Rill ABI.

Cross-component runtime and target acceptance is owned by [`tests/`](../tests/README.md) through `test:runtime`, which currently exits nonzero as unavailable. The separate `test:llvm` task checks an LLVM C-API compile/link smoke only; it does not validate a Rill runtime.

[ADR 0014](../docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) and [ADR 0006](../docs/adr/0006-whole-program-monomorphization-on-llvm-23.md) record the backend decisions and qualifications. [#28](https://github.com/Reilley64/rill/issues/28) retains production/formal ABI, lowering and runtime acceptance. [#30](https://github.com/Reilley64/rill/issues/30) completed standalone mechanism experiments, not production integration. Provider-specific gates remain separate.

Existing [experiments](../experiments/README.md) stay in place with their recorded results and limitations. They are not runtime source or passing product suites. Future installed target inputs live under `lib/rill/runtime/<target>/`; [development](../docs/development.md) covers toolchain selection and host prerequisites.
