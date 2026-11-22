# Development

Rill's first scaffold establishes a private Rust workspace and development tasks. It is not a working language toolchain. `rill --help` and `rill --version` are real CLI behavior. Product commands, including checking, compilation, execution, language tests and the language server, report that they are unavailable and exit nonzero. A successful scaffold build or infrastructure check does not mean a Rill program ran.

The repository and tooling decision is [ADR 0026](adr/0026-toolchain-repository-and-mise.md), approved in ri-x5n.

## Setup

Install mise at a version accepted by `min_version` in `mise.toml`. CI pins its own mise release; repository tasks do not upgrade a global installation. Review the configuration and tasks before trusting or executing them. From the repository root, use:

```sh
mise trust mise.toml
mise install --locked
mise run setup
mise run check
mise run build:scaffold
mise exec -- cargo run --locked -p cli --no-default-features -- --help
mise exec -- cargo run --locked -p cli --no-default-features -- --version
```

`setup` prepares the default developer tools and dependency environment. It does not install LLVM or the optional experiment toolchains. First setup may access the network for tools and Cargo or Python dependencies. Committed locks are not a vendored dependency tree or an air-gap guarantee.

mise owns tool installation, version selection and task entry points. Cargo owns the Rust workspace and dependencies; uv owns Python dependencies. Keep tool versions pinned and commit the applicable `mise.lock`, `Cargo.lock`, `uv.lock` and any tool-backend lock sidecars. Use locked installation and dependency resolution rather than refreshing locks as part of ordinary checks. The mise configuration is the Rust version authority; there is no separate `rust-toolchain.toml` or custom bootstrap wrapper.

Use `mise exec -- ...` for direct developer commands so they use the selected tools. Shell activation is optional. User-level mise overrides and pre-existing tool directories can affect execution, so record resolved tool paths and versions when diagnosing differences. A lockfile does not certify an already installed tree.

## Workspace and source ownership

The root Cargo workspace is private. Its crates are implementation boundaries, not separately published Rill packages.

| Path | Responsibility |
| --- | --- |
| `crates/compiler/` | Shared source, analysis and checking facts, independent of LLVM and LSP. |
| `crates/ide/` | Editor analysis state and features built on the compiler core, without a second checker. |
| `crates/lsp/` | Rust LSP protocol and transport adapter over IDE services. |
| `crates/codegen-llvm/` | LLVM backend integration, separate from front-end analysis. |
| `crates/cli/` | The `rill` executable and command routing. |
| [`stdlib/`](../stdlib/README.md) | Rill standard-library source, with a future explicit `std.rill` facade. |
| [`runtime/`](../runtime/README.md) | Implementation-private target-runtime package/support; currently tested ri-hw5 channel wake/watch, private readiness-reactor and macOS loopback TcpProgress mechanism slices, without a Rill ABI or target support matrix. |
| [`tests/`](../tests/README.md) | Cross-component acceptance ownership and scaffold infrastructure checks. |

The CLI's default `llvm` feature currently selects only the backend stub crate and links no LLVM library. Fast checks and `build:scaffold` disable that feature so future backend bindings stay outside the LLVM-free dependency graph.

The scaffold checks manifest dependency direction; the names alone do not prove semantic reuse. The compiler, IDE, LSP and backend still provide boundaries, not implemented product behavior. The runtime package now contains ri-hw5 channel wake/watch and private readiness/TcpProgress mechanism slices with Rust component tests, but no generated-code ABI, Rill scheduler/cancellation integration, Linux/macOS target acceptance or Rill execution. Production runtime acceptance remains unavailable. There are no placeholder Rill implementations in `stdlib/`.

## Task profiles and current status

These are task groups, not alternate Rust compiler profiles. Invoke each task as `mise run <name>`. GitHub Actions uses these same task entry points rather than a separate implementation of the checks. A configured host matrix is not evidence that every host passed.

| Task | Scope in the first scaffold |
| --- | --- |
| `setup` | Default developer setup, without LLVM or heavy experiment tools. |
| `check` | LLVM-free infrastructure/component checks, including workspace dependency direction, CLI scaffold behavior and runtime channel/readiness/TCP unit tests. Not language, stdlib, cross-component runtime, target support, TLS or client acceptance. |
| `build:scaffold` | Build the CLI scaffold, not a functioning Rill compiler. |
| `build` | Product build is unavailable; exit nonzero. |
| `test:language` | Language conformance is unavailable; exit nonzero. |
| `test:stdlib` | Standard-library execution tests are unavailable; exit nonzero. |
| `test:runtime` | Runtime acceptance is unavailable; exit nonzero. |
| `test:lsp-client` | Complete client acceptance is unavailable; exit nonzero. |
| `verify:release` | Require infrastructure checks, a product build and every product suite. Currently fails because product functionality is unavailable. |
| `plan:llvm` | Inspect the optional LLVM route and prerequisites. A plan is not installation evidence. |
| `setup:llvm` | Explicit opt-in LLVM provisioning, separate from default setup. |
| `test:llvm` | LLVM C-API compile/link smoke only, not a Rill backend or inkwell integration test. |
| `experiment:*` | Explicit entry points for existing reproductions, with their original scope and prerequisites. |

`Scaffold checks` configures the LLVM-free checks on four GitHub-hosted OS/architecture profiles. `Native acceptance gate` runs for native-impacting changes and manual dispatch. It currently fails at `test:runtime` because native acceptance is absent; this is intentional, not a passing native profile. A manual run also requires `verify:release` after native acceptance succeeds. Neither workflow installs LLVM or publishes releases. These workflows have been syntax-checked locally, not executed on GitHub.

Unavailable product tasks must not become successful empty suites. Infrastructure tests can prove routing, nonzero unavailable responses or manifest constraints; they cannot prove language semantics. Consult [test ownership](../tests/README.md) before adding acceptance tests.

Existing [Rill examples](prototypes/README.md) remain design evidence and fixtures. Their Python or schema checks, where present, do not execute Rill. This scaffold does not promise to run every fixture validator as part of `check`. [Experiments](../experiments/README.md) remain standalone reproductions, not product suites. Run an `experiment:*` task only after reading its runbook; preserve recorded inputs/results and direct new output to untracked paths.

## Recorded scaffold validation

On 2026-11-16, the scaffold working tree based on `8934802` passed these checks on macOS 27.0.1 AArch64 with mise 2026.11.4 and the selected base tools:

- `mise install --locked` and `mise run setup` installed/verified Rust 1.98.0, Python 3.13.18, uv 0.13.8 and the locked Python check environment.
- `mise run check` passed formatting, LLVM-free clippy and Cargo tests, eight CLI process tests, nine tool-infrastructure tests, 25 offline LLVM-provisioning tests, diagnostic fixture validation, 110 Python TLS-model comparisons and 32 SQLite reference tests. These counts are separate evidence categories, not Rill acceptance totals.
- `mise run build:scaffold`, CLI help/version and `mise run check:ci` passed. The default-feature workspace also passed clippy and tests, exercising the backend stub without LLVM.
- The local-document checker found no broken destinations. All 107 tracked files under `prototypes/` and `experiments/` matched the base commit byte-for-byte.
- Product build and all four product-suite tasks exited 2 as unavailable. `verify:release` exited 2 at the unavailable product build. `test:llvm` exited 1 for the absent project prefix; no smoke passed. The plan-only task created no LLVM output directory.

The multi-platform mise lock resolved tool metadata, not four installations. GitHub workflows, other-host setup, LLVM download/build/static-link smoke, inkwell/libffi integration, native experiment reproductions, Rill execution and complete OpenIDEA acceptance were not run. This snapshot does not certify later changes or a production installation.

## Host prerequisites

mise does not provide an operating-system SDK, target sysroot or all native dependencies merely by installing a compiler.

- On both macOS architectures, install and select Apple Command Line Tools or Xcode with a usable macOS SDK and platform linker. Resolve the SDK through `xcrun`; licenses and privileged OS setup are external prerequisites.
- On each Linux architecture, provide the host C/C++ toolchain as needed, a native linker, libc headers, startup objects and the runtime/development libraries required by the selected tools. No universal Linux distribution or musl compatibility has been validated.
- A source LLVM build needs a C++17-capable bootstrap compiler, CMake, Ninja, Python and the native SDK or sysroot. Its CPU, memory, disk and build-time costs are separate from the fast developer path.
- Tool downloads need network access and a working CA trust configuration. OS prerequisite installation is explicit; tasks must not silently acquire privileges or use an undeclared package manager.

Experiment-only prerequisites can include Rosetta, Docker, Wasmtime, Binaryen or Node. They belong to their explicit tasks and runbooks, not the default checks. The unchanged historical backend runner also requires `brew info --json=v2 llvm` to record Homebrew metadata, even when the task passes the pinned prefix for every LLVM executable. That reproduction requires an existing matching historical environment; its task does not install Homebrew or use Homebrew LLVM as a fallback. It is not a portable fresh-setup test. A Wasm backend and `wasm-ld` do not supply a WASI SDK, and no SDK or target acceptance follows from setup success.

## Optional LLVM 23.1.5 profile

LLVM is unnecessary for fast checks and the scaffold CLI. Inspect the plan before opting in:

```sh
mise run plan:llvm
mise run setup:llvm
```

The second command selects the binary route on the three hosts with published bundles. On native macOS x86-64, source compilation requires an explicit opt-in and bootstrap/SDK paths instead:

```sh
mise run setup:llvm -- --build-source \
  --sdk "$(xcrun --show-sdk-path)" \
  --cc "$(xcrun --find clang)" --cxx "$(xcrun --find clang++)"
```

Provisioning requires at least 48 GiB free disk for bundles or 80 GiB for source. The source route requires at least 8 GiB RAM, 4 GiB per build job, and defaults to two jobs with one linker job. The script rejects insufficient resources rather than substituting another route. These budgets exceed typical small hosted-runner capacity.

After provisioning, use `mise run test:llvm` on Linux. On macOS, pass the selected SDK explicitly:

```sh
mise run test:llvm -- --sdk "$(xcrun --show-sdk-path)"
```

The selected route preserves a full, explicit LLVM prefix with headers, libraries and tools. It uses checksum-pinned upstream LLVM 23.1.5 archives for Linux x86-64, Linux AArch64 and macOS AArch64. Upstream supplies no macOS x86-64 archive for this release, so that host uses a pinned source build. There is no implicit Homebrew prefix, driver-only substitute, shared-LLVM fallback or LLVM downgrade.

Provisioning validation has not passed on these hosts. Artifact publication and source recipe inspection do not prove usable static libraries, coherent prefixes or native library closure. `test:llvm` is a C-API smoke boundary only. It does not establish the eventual Rust binding integration, complete backend targets, required native or Wasm lowering, or any Rill execution.

A usable route still needs per-host evidence for exact versions and prefixes, requested tools and target backends, static LLVM linkage, remaining loader dependencies and SDK/sysroot compatibility. Static LLVM linkage does not imply a fully static executable. Existing backend experiments and the production acceptance gates retain their separate obligations. The [LLVM provisioning research](research/0046-llvm23-provisioning-options.md) records artifact and recipe evidence, not a successful installation.

## Future installed toolchain

The approved future layout is:

```text
<prefix>/
  bin/rill
  lib/rill/
    std/
    runtime/<target>/
    toolchain.json
```

The selected toolchain identifies compatible standard-library source, target-runtime inputs and toolchain metadata. Installed standard-library lookup must not depend on the process's working directory. Rill end users will not need Rust to use the distributed toolchain. The first scaffold does not implement this installer, metadata format or source lookup.

For language and library rules, use the [contract index](README.md#contracts) rather than this guide. Production/formal backend acceptance remains ri-48g; complete OpenIDEA client delivery remains ri-bv2. The scaffold neither closes those gates nor changes the existing Rill source examples or recorded evidence.
