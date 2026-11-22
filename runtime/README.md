# Target-runtime support

This directory owns target-runtime support, including compiler-required native support and target-specific runtime inputs. Rill standard-library algorithms belong in [`stdlib/`](../stdlib/README.md).

The `runtime` Cargo package is implementation-private. Its current ri-hw5 slices implement safe Rust channel try operations, coalescing wake state, one-shot channel readiness watches, an explicitly owned mio reactor and a one-owner nonblocking `TcpProgress` mechanism required by [ri-w0c](../docs/progress-io-contracts.md). Component tests exercise the mechanism and macOS loopback TCP through `mise run check`; they do **not** execute Rill source, define the eventual Rill ABI or establish the Linux/macOS support matrix.

No generated-code ABI, Rill scheduler/cancellation integration, blocking native-job executor or TLS/provider integration exists here yet. The cross-component `test:runtime` task remains unavailable and exits nonzero; a passing package unit test must not make it pass. ri-hw5 retains the complete shared progress/TCP acceptance inventory, and ri-ei7/#48 retain TLS/HTTP delivery.

Implementation-local tests live with their runtime component. Future cross-component runtime and target acceptance is owned by [`tests/`](../tests/README.md). The separate `test:llvm` task checks an LLVM C-API compile/link smoke only; it does not validate this runtime or a Rill backend.

[ADR0014](../docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md), [ADR0006](../docs/adr/0006-whole-program-monomorphization-on-llvm-23.md) and the [progress contract](../docs/progress-io-contracts.md) record relevant semantics and qualifications. The [readiness/TCP mechanism note](../docs/research/0047-native-readiness-reactor.md) records the locked mio version, primary-source constraints, current tests and untested cells. ri-48g retains production/formal ABI, lowering and runtime acceptance. ri-dej completed standalone mechanism experiments, not production integration.

Existing [experiments](../experiments/README.md) stay in place with their recorded results and limitations. They are not runtime source or passing product suites. Future installed target inputs live under `lib/rill/runtime/<target>/`; [development](../docs/development.md) covers toolchain selection and host prerequisites.
