# Development

Rill's first scaffold establishes a private Rust workspace and development tasks. It is not a working language toolchain. `rill --help` and `rill --version` are real CLI behavior. Product commands, including checking, compilation, execution, language tests and the language server, report that they are unavailable and exit nonzero. A successful scaffold build or infrastructure check does not mean a Rill program ran.

The repository and tooling decision is [ADR 0026](adr/0026-toolchain-repository-and-mise.md), approved in ri-x5n.

## Setup

Install mise at a version accepted by `min_version` in `mise.toml`. CI pins its own mise release; repository tasks do not upgrade a global installation. Review the configuration and tasks before trusting or executing them. From the repository root, use:

```sh
mise trust mise.toml
mise install --locked
mise run setup
mise run build:scaffold
mise exec -- cargo run --locked -p cli --no-default-features -- --help
mise exec -- cargo run --locked -p cli --no-default-features -- --version
```

`setup` prepares the default developer tools and dependency environment. It does not install LLVM or the optional experiment toolchains. First setup may access the network for tools and Cargo or Python dependencies. Committed locks are not a vendored dependency tree or an air-gap guarantee.

Full test suites and validation gates run in CI, not during setup. For local checks, follow step 3 of the [toolchain development guide](agents/development.md).

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
| [`runtime/`](../runtime/README.md) | Target-runtime support and future native/ABI integration. |
| [`tests/`](../tests/README.md) | Cross-component acceptance ownership and scaffold infrastructure checks. |

The CLI's default `llvm` feature selects `codegen-llvm`, which depends on inkwell and links the static LLVM prefix that `setup:llvm` installs. Fast checks and `build:scaffold` disable that feature, so the compiler and every LLVM-free check stay outside that dependency graph.

The first scaffold checks manifest dependency direction; the names alone do not prove semantic reuse. The compiler, IDE, LSP and backend currently provide boundaries, not implemented product behavior. Language algorithms, runtime mechanisms and production ABI work remain behind their existing gates. There are no placeholder Rill implementations in `stdlib/`.

## Task profiles and current status

These are task groups, not alternate Rust compiler profiles. Invoke each task as `mise run <name>`. GitHub Actions uses these same task entry points rather than a separate implementation of the checks. A configured host matrix is not evidence that every host passed.

| Task | Current scope |
| --- | --- |
| `setup` | Default developer setup, without LLVM or heavy experiment tools. |
| `test` | Rust unit, integration and documentation tests for the compiler, IDE, LSP, CLI, and the collection and error-storage runtime crates. No LLVM or Rill execution. |
| `test:compiler` | Focused compiler unit, integration and documentation tests, without LLVM. |
| `check` | Rust tests, formatting, lint, documentation, fixture and tool checks. Not complete language, stdlib, runtime or client acceptance. |
| `check:rust` | Rust formatting and LLVM-free clippy. Tests run through `test`. |
| `build:scaffold` | Build the CLI scaffold, not a functioning Rill compiler. |
| `build` | Product build is unavailable; exit nonzero. |
| `test:language` | Language conformance is unavailable; exit nonzero. |
| `test:stdlib` | Standard-library execution tests are unavailable; exit nonzero. |
| `build:prototypes` | Build `rill` with the `llvm` feature and stage an installation in `target/prototypes/prefix` (`bin/rill`, `lib/rill/std`, `lib/rill/runtime/<host>`). Needs `LLVM_SYS_231_PREFIX` and `LLVM_SYS_231_STRICT_VERSIONING=1`. |
| `test:prototypes` | Run `crates/cli/tests/prototypes.rs` through the `rill` that `build:prototypes` staged: `rill run` and `rill build` on `tests/prototypes/hello-world.rill`, `-o`, no leftover executable, R0605 for an unsupported shape. Needs the same environment. |
| `test:runtime` | Runtime acceptance is unavailable; exit nonzero. |
| `test:lsp-client` | Language server protocol tests are unavailable; exit nonzero. |
| `verify:release` | Require infrastructure checks, a product build and language, standard-library and runtime suites. Currently fails because product functionality is unavailable. |

A distribution that claims language server support must also pass `test:lsp-client` for its advertised capabilities. The general release check does not require a language server.
| `plan:llvm` | Inspect the optional LLVM route and prerequisites. A plan is not installation evidence. |
| `setup:llvm` | Explicit opt-in download of the trimmed LLVM archive for this host from the `Reilley64/rill` release, by pinned checksum. Separate from default setup. |
| `build:llvm` | Build this host's trimmed LLVM archive from pinned source. The `Release: Trimmed LLVM archives` workflow runs it; run it locally only for a reproduction. |
| `test:llvm` | LLVM C-API compile/link smoke only, not a Rill backend or inkwell integration test. The inkwell smoke is `cargo test -p codegen-llvm`, run by `Test: Native acceptance`. |
| `experiment:*` | Explicit entry points for existing reproductions, with their original scope and prerequisites. |

`Test: Scaffold checks` configures the LLVM-free checks on four GitHub-hosted OS/architecture profiles. `Test: Native acceptance` runs for native-impacting changes and manual dispatch. A manual run also asserts that `verify:release` passes the repository checks and then stops at the stub for `build`; replace that assertion when the task exists. `Test: Native acceptance` runs `setup:llvm` on each of the four platforms, then the `codegen-llvm` tests against that prefix, then `build:prototypes` and `test:prototypes`, which run Hello World through the real `rill` binary (`rill run` and `rill build`, linked with the host `cc`); then asserts that `test:runtime` still exits 2 with its scaffold message, because the prototype acceptance is not the production runtime acceptance that task reserves; `Test: Scaffold checks` installs no LLVM, and neither workflow publishes releases. `Release: Trimmed LLVM archives` runs only when dispatched by hand; it builds the four hosts' archives and uploads them as workflow artifacts, and a maintainer attaches them to the release.

Both workflows use explicit runner-local Cargo and Rustup homes before mise installs tools. Nested mise tasks then retain the same Rustup provider, instead of losing the runner's external provider during tool lookup. Workflow names and YAML layout follow the [GitHub automation guide](../.github/AGENTS.md).

The 2026-11-17 [initial hosted scaffold run](https://github.com/Reilley64/rill/actions/runs/44001670793) passed Linux x86-64/AArch64 and macOS x86-64. macOS AArch64 failed during nested mise tool lookup. Local reproduction confirmed the tool-lookup failure and the explicit-home correction.

The [follow-up hosted run](https://github.com/Reilley64/rill/actions/runs/44006663293) at `37eed9f` passed all four profiles, including macOS AArch64. [Native acceptance](https://github.com/Reilley64/rill/actions/runs/44006663069) still exited 2 as unavailable on every profile. These results cover scaffold and M1 tests, not complete Rill execution or backend acceptance.

Use `mise run test` for the implemented LLVM-free Rust suites and `mise run test:compiler` during compiler development. The full `check` task reuses `test`, so its Cargo test command runs once. Python tooling tests and fixture/model checks remain in `check`, outside the focused Rust test tasks.

Unavailable product tasks must not become successful empty suites. Infrastructure tests can prove routing, nonzero unavailable responses or manifest constraints; they cannot prove language semantics. Consult [test ownership](../tests/README.md) before adding acceptance tests.

Existing [Rill examples](prototypes/README.md) remain design evidence and fixtures. Their Python or schema checks, where present, do not execute Rill. This scaffold does not promise to run every fixture validator as part of `check`. [Experiments](../experiments/README.md) remain standalone reproductions, not product suites. Run an `experiment:*` task only after reading its runbook; preserve recorded inputs/results and direct new output to untracked paths.

## Recorded scaffold validation

On 2026-11-16, the scaffold working tree based on `8934802` passed these checks on macOS 27.0.1 AArch64 with mise 2026.11.4 and the selected base tools:

- `mise install --locked` and `mise run setup` installed/verified Rust 1.98.0, Python 3.13.18, uv 0.13.8 and the locked Python check environment.
- `mise run check` passed formatting, LLVM-free clippy and Cargo tests, eight CLI process tests, nine tool-infrastructure tests, 25 offline LLVM-provisioning tests, diagnostic fixture validation, 110 Python TLS-model comparisons and 32 SQLite reference tests. These counts are separate evidence categories, not Rill acceptance totals.
- `mise run build:scaffold`, CLI help/version and `mise run check:ci` passed. The default-feature workspace also passed clippy and tests, exercising the backend stub without LLVM.
- The local-document checker found no broken destinations. All 107 tracked files under `prototypes/` and `experiments/` matched the base commit byte-for-byte.
- Product build and all four product-suite tasks exited 2 as unavailable. `verify:release` exited 2 at the unavailable product build. `test:llvm` exited 1 for the absent project prefix; no smoke passed. The plan-only task created no LLVM output directory.

The multi-platform mise lock resolved tool metadata, not four installations. GitHub workflows, other-host setup, LLVM download/build/static-link smoke, inkwell/libffi integration, native experiment reproductions, Rill execution and language server acceptance were not run. This snapshot does not certify later changes or a production installation.

## Host prerequisites

mise does not provide an operating-system SDK, target sysroot or all native dependencies merely by installing a compiler.

- On both macOS architectures, install and select Apple Command Line Tools or Xcode with a usable macOS SDK and platform linker. Resolve the SDK through `xcrun`; licenses and privileged OS setup are external prerequisites.
- On each Linux architecture, provide the host C/C++ toolchain as needed, a native linker, libc headers, startup objects and the runtime/development libraries required by the selected tools. No universal Linux distribution or musl compatibility has been validated.
- `build:llvm` needs a C++17-capable compiler and the native SDK or sysroot, plus the pinned CMake, Ninja and Python that mise provides. Its CPU, disk and build-time costs (see the LLVM profile below) are separate from the fast developer path.
- Tool downloads need network access and a working CA trust configuration. OS prerequisite installation is explicit; tasks must not silently acquire privileges or use an undeclared package manager.

Experiment-only prerequisites can include Rosetta, Docker, Wasmtime, Binaryen or Node. They belong to their explicit tasks and runbooks, not the default checks. The unchanged historical backend runner also requires `brew info --json=v2 llvm` to record Homebrew metadata, even when the task passes the pinned prefix for every LLVM executable. That reproduction requires an existing matching historical environment; its task does not install Homebrew or use Homebrew LLVM as a fallback. It is not a portable fresh-setup test. A Wasm backend and `wasm-ld` do not supply a WASI SDK, and no SDK or target acceptance follows from setup success.

## Optional LLVM 23.1.5 profile

LLVM is unnecessary for fast checks and the scaffold CLI. Inspect the plan before opting in:

```sh
mise run plan:llvm
mise run setup:llvm
```

`setup:llvm` downloads one archive for the host from the `llvm-23.1.5-1` release on `Reilley64/rill`, checks its size limit and SHA-256 against the pins in `tools/llvm/common.py`, extracts it safely and inspects the result. The prefix is `target/toolchains/llvm-23.1.5/<host>`, where host is `linux-x64`, `linux-arm64`, `macos-arm64` or `macos-x64`. It needs about 2 GiB of free disk. The script rejects insufficient disk rather than substituting another route.

A pin with no digest makes `setup:llvm` stop before any download and name the pin. A download whose size or SHA-256 differs from the pin fails and leaves no prefix.

After provisioning, use `mise run test:llvm` on Linux. The smoke builds with the host `/usr/bin/cc` and `/usr/bin/c++`, because the prefix holds no compiler. On macOS, pass the selected SDK explicitly:

```sh
mise run test:llvm -- --sdk "$(xcrun --show-sdk-path)"
```

The archive is a trimmed LLVM: one backend for the host architecture (`X86` or `AArch64`), no LLVM projects such as clang or lld, static libraries, headers and `llvm-config`. Nothing else is built, and no optional native library is linked (zlib, zstd, libxml2, libedit and libffi are off). `llvm-config --link-static --system-libs` reports `-lrt -ldl -lm` on Linux and `-lm` on macOS. The `Release: Trimmed LLVM archives` workflow builds it on `ubuntu-24.04`, `ubuntu-24.04-arm`, `macos-15` and `macos-15-intel` from the checksum-pinned `llvm-project-23.1.5.src.tar.xz`, with the flags in `tools/llvm/release.py`. Nothing builds LLVM on push or pull request.

Measured on standard GitHub-hosted runners of this repository: a build takes 32 to 36 minutes on `ubuntu-24.04`, `ubuntu-24.04-arm` and `macos-15` and 71 minutes on `macos-15-intel`. The `.tar.xz` archives are 40 to 52 MiB. The installed prefix is 206 to 215 MiB on macOS (the prototype measured 292 to 299 MiB on Linux). The build directory, source and prefix together take 2.3 to 2.4 GiB, and free disk fell by 2.6 to 2.9 GiB over a run; no sample was taken during the build, so treat 3 GiB as the budget for a local `build:llvm`. There is no implicit Homebrew prefix, driver-only substitute, shared-LLVM fallback or LLVM downgrade.

Archive installation does not prove an inkwell build. `crates/codegen-llvm/tests/smoke.rs` does: it builds a module whose `main` returns 42, runs the LLVM verifier, writes an object for the host target, links it with the host `cc` and asserts the program's status. `Test: Native acceptance` runs it on the four platforms with `cargo test --locked -p codegen-llvm`. `crates/codegen-llvm` takes inkwell `=0.11.0` with `default-features = false`, `llvm23-1-force-static`, `no-libffi-linking` (inkwell otherwise links libffi unconditionally) and `target-x86` or `target-aarch64` by `cfg(target_arch)`. Using the prefix from Rust needs `LLVM_SYS_231_PREFIX` set to it, `LLVM_SYS_231_STRICT_VERSIONING=1`, inkwell `=0.11.0` with `llvm23-1-force-static` and the native `target-*` feature rather than `target-all`. `test:llvm` is a C-API smoke boundary only. Neither smoke establishes WebAssembly or RISC-V backends, or any Rill execution. Because the prefix has no `clang`, `lld` or `wasm-ld`, `experiment:backend` cannot use it.

A usable route still needs per-host evidence for exact versions and prefixes, requested tools and target backends, static LLVM linkage, remaining loader dependencies and SDK/sysroot compatibility. Static LLVM linkage does not imply a fully static executable. Existing backend experiments and the production acceptance gates retain their separate obligations. The [LLVM provisioning research](research/0046-llvm23-provisioning-options.md) records artifact and recipe evidence, not a successful installation.

## Future installed toolchain

The approved future layout is:

```text
<prefix>/
  bin/rill
  lib/rill/
    std/
    runtime/<target>/
      error.rs
      librill_ownership.rlib
      librill_runtime.a
    toolchain.json
```

The selected toolchain identifies compatible standard-library source and target-runtime inputs. The installed loader derives its source root from the executable path, not the working directory. Toolchain metadata does not grant authority. The scaffold does not implement a release installer or metadata format.

For the M4.8 static check on the host target, stage the current distribution inputs and run the separate Rust acceptance runner:

```sh
repo=$PWD
work=$(mktemp -d)
mise exec -- python tools/stage_typed_installation.py "$work/prefix"
(cd "$work" && ./prefix/bin/rill --version && \
  ./prefix/bin/m4_8_installed "$repo/tests/installed/typed/main.rill")
```

The staging script takes the executable, runner and ownership archive from one locked Cargo build. It copies the listed first-party sources and runtime inputs into a new prefix. The runner loads and analyzes the entry in both modes. It exits 0 only when both modes issue a typed receipt. The [M4.8 verification record](../crates/compiler/tests/m4-8-acceptance.md) records the installed outcomes separately from the scaffold checks. Neither check runs Rill or validates an ABI.

`rill check <entry>` runs the front end on the layout above and needs no LLVM, so the `--no-default-features` build works. It finds the layout from the canonical path of the running `rill`, so the binary must sit in a `bin/` directory. It reads `<prefix>/lib/rill/std/*.rill`, `<prefix>/lib/rill/runtime/<target>/error.rs` and `<prefix>/lib/rill/runtime/<target>/librill_ownership.rlib`, where `<target>` is the target the `rill` binary was built for. The archive must be the exact bytes the compiler pins (`compiler::loader::pinned_ownership_archive`); the loader rejects any other. The `crates/cli` tests stage this layout in a temporary directory beside a copy of the built `rill`. `check` exits 0 for a compilable program, 1 for a source finding or R0605, 2 for an invalid invocation such as an unreadable entry, and 3 when analysis stops at a limit and is incomplete.

`rill build <entry> [-o <path>]` and `rill run <entry> [<argument>...]` exist only in the build with the `llvm` feature (the default); without it they keep the scaffold's unavailable exit 2. They run the same front end as `check` and report a blocker the same way. A compilable program is lowered by `codegen-llvm` to an object and linked with the host `cc`: `cc -o <out> <object> <prefix>/lib/rill/runtime/<target>/librill_runtime.a <native libs>`. The native libraries are `NATIVE_LIBS` in `crates/cli/src/native_libs.rs`, and `crates/cli/tests/native_libs.rs` asserts the list equals what `cargo rustc -p rill-runtime --lib -- --print native-static-libs` prints for the host. `build` writes `./<entry file stem>`, or the `-o` path, by linking to a temporary file in that directory and renaming it, so a failure leaves no executable. `run` builds into a fresh temporary directory, runs the program with the entry as argument zero, the remaining arguments untouched and the three standard streams inherited, deletes the directory and exits with the program's status, or 128 plus the signal number when a signal ends it. A backend failure exits 3, and a missing or failing `cc` exits 1 naming `cc`. `build:prototypes` stages the layout, archive included, and `test:prototypes` tests through it. `tools/stage_typed_installation.py` stages the same layout; `--llvm` builds the product `rill` and omits the M4.8 runner.

For language and library rules, use the [contract index](README.md#contracts) rather than this guide. Production/formal backend acceptance remains ri-48g; standards-based language server support is tracked separately in ri-bv2. The scaffold does not establish either result or change existing evidence.
