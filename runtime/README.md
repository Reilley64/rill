# Target-runtime support

This directory owns target-runtime support, including future compiler-required native support and target-specific runtime inputs. Rill standard-library algorithms belong in [`stdlib/`](../stdlib/README.md). The first scaffold contains no runtime implementation, production ABI or native provider integration.

Implementation-local tests will live with their runtime component. Cross-component runtime and target acceptance is owned by [`tests/`](../tests/README.md) through `test:runtime`, which currently exits nonzero as unavailable. The separate `test:llvm` task checks an LLVM C-API compile/link smoke only; it does not validate a Rill runtime.

[ADR 0014](../docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) and [ADR 0006](../docs/adr/0006-whole-program-monomorphization-on-llvm-23.md) record the backend decisions and qualifications. [#28](https://github.com/Reilley64/rill/issues/28) retains production/formal ABI, lowering and runtime acceptance. [#30](https://github.com/Reilley64/rill/issues/30) completed standalone mechanism experiments, not production integration. Provider-specific gates remain separate.

Existing [experiments](../experiments/README.md) stay in place with their recorded results and limitations. They are not runtime source or passing product suites. Future installed target inputs live under `lib/rill/runtime/<target>/`; [development](../docs/development.md) covers toolchain selection and host prerequisites.
