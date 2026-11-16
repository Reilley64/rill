# mise toolchain management

Inspected 2026-11-16. Discussion input for the repository-layout interview, not an approved setup. The user requires mise as the developer-tool installation, version-selection and task entry point. LLVM 23 remains the approved compiler-backend choice.

## Finding

mise can own the project's tool declarations and commands without replacing Cargo or uv as dependency/build owners. Rust, Python and Node have documented installers. LLVM needs a separate, explicit installation decision. The inspected sources do **not** establish a usable full LLVM 23 installer on all four macOS/Linux x86-64/AArch64 hosts. A registry entry named `clang`, or a version catalogue containing `23.1.5`, is insufficient evidence.

The behavioral baseline below is released mise **2026.11.8**, commit `a4a1996f49779db556570678c81f658e5a28fe19`. Local mise is **2026.11.4**. No upgrade, installation, generated configuration or execution of project checks occurred. Primary sources are mise's documentation/source and the mise-maintained plugin/catalogue sources linked below. Third-party tutorials and package-manager instructions were not used.

## Current repository and host evidence

- [Root README](../../README.md) describes design documents, examples and standalone experiments, not a complete compiler or standard library. The inspection found no tracked Cargo manifest, `rust-toolchain.toml`, `pyproject.toml`, `uv.lock`, `package.json` or mise configuration. This does not select a future workspace layout.
- [Backend runbook](../../experiments/backend-30/README.md) records LLVM **23.1.5**, separate LLD **23.1.5**, Node **26.13.0**, Python, and the macOS SDK. Its runner fetches checksum-pinned Binaryen **133** and Wasmtime **50.0.1** archives. Extended reproduction requires existing Rosetta and a running Docker daemon. These are standalone mechanism observations, not accepted Rill target support.
- [Diagnostics fixtures](../prototypes/21-diagnostics.md) use `uv run --no-project --with jsonschema==4.25.1 python prototypes/21-diagnostics/check-fixtures.py`. The script imports `jsonschema`; it is not just a CLI dependency. Other Python checks mostly use the standard library. The [SQLite runbook](../../experiments/sqlite-provider/README.md) distinguishes its limited schema-subset model from a general JSON Schema validator. Its native reference runner requires Darwin and defaults to writing a tracked results report unless given `--report`.
- Read-only host observations: Darwin arm64; mise 2026.11.4; uv 0.12.2; `/usr/bin/python3` 3.9.6; Apple Clang 21.0.0; an SDK selected through `xcrun --show-sdk-path`. Existing Homebrew paths contain LLVM/`llvm-config` **23.1.5**, LLD **23.1.5**, CMake **4.4.5** and Ninja **1.13.2**. A Node **26.13.0** mise install directory exists. Rust/Cargo/Node commands resolve to mise shims; their presence alone does not prove a selected or runnable version. No `llvm-config` was on the inspected PATH.

Existing Homebrew installations are historical/local evidence, not the proposed installation contract. A future mise task must not silently depend on `/opt/homebrew` defaults or replace LLVM 23 with Apple Clang.

## What mise would own

### Configuration and locks

`mise.toml` declares `[tools]`, `[env]` and `[tasks]`. Config is merged with system, user and parent-directory configuration. Deeper project files normally override parents, but user global override files can override the project. A project config alone is therefore not hermetic. A localized, generated mise wrapper can ignore user-global config and keep mise state under the project; that remains a setup option, not a chosen directory layout. [1], [9]

`mise.lock` records exact resolved tool versions, backends, and artifact URLs/checksums where supported. `mise lock` resolves without installing tools and includes task-specific tools. Normal refresh preserves matching locked versions; `--bump` re-resolves requests. `mise install --locked` fails on missing matching entries and, for URL-recording backends, missing current-platform URLs/checksums. Prepare explicit `macos-x64`, `macos-arm64`, `linux-x64` and `linux-arm64` entries rather than assuming one host's lock covers the others. Commit dependency sidecars under `.mise/locks/` too if the selected backends create them. [2]

Lock strength depends on the backend. Core Rust records the top-level version, not a complete rustup artifact/dependency graph. Conda records package dependencies and artifacts; HTTP/GitHub record platform artifacts. Plugin installers can do additional work outside those recorded downloads. Already installed trees are trusted rather than rehashed against the lockfile. Locked installation still may need network access. `mise.lock` does not replace `Cargo.lock`, `uv.lock` or npm dependency locks. [2]

A minimum mise version or pinned wrapper/action should accompany any eventual configuration. These findings describe the cited release, not an assertion that every behavior works identically in the older local binary.

### Rust and Cargo

The built-in Rust installer uses rustup, installs rustup if needed, and selects the toolchain with `RUSTUP_TOOLCHAIN`. mise supports a rustup profile, components and cross-compilation targets. A pinned Rust version with an explicit profile avoids relying on the user's rustup default. `rustfmt`, `clippy`, `rust-src` and installed target standard libraries are toolchain concerns, subject to availability for that version/host. [3]

Cargo should still own crates, dependency resolution, workspace membership, features, build scripts, builds and tests. mise tasks would call Cargo with the selected toolchain. `cargo:` is mise's backend for installing Rust CLI tools; it is not a replacement for compiler/stdlib/LSP crates in Cargo manifests. Ordinary `cargo install` uses a shared Cargo bin directory rather than project-versioned CLI installation. [2], [3]

Do not create competing Rust version authorities. Options for the interview are:

1. Put Rust's version/profile/components/targets in `mise.toml`, with no independently maintained `rust-toolchain.toml`.
2. Keep `rust-toolchain.toml` authoritative and explicitly enable mise's Rust idiomatic-version-file reader in project configuration. mise reads its channel/profile/components/targets.

This choice matters because mise's `RUSTUP_TOOLCHAIN` outranks rustup's `rust-toolchain.toml` selection. A global mise Rust entry can otherwise override a project's file. Verify with `mise exec -- rustup show active-toolchain`, not just by reading manifests. [3]

Rust's `llvm-tools` component is not evidence of the full, separately pinned LLVM 23 distribution Rill needs. mise's Rust installer does not promise a matching Clang, LLD, headers or LLVM link libraries. A future Cargo LLVM binding/build must receive an explicit verified LLVM prefix and retain Cargo ownership of its dependency/build logic. No Rust release or binding crate is selected here.

### Python, uv and Node

mise can install and pin both Python and uv. Python normally comes from prebuilt python-build-standalone archives; unavailable builds may use a source-build route with host prerequisites. mise's docs warn that uv may choose its own downloaded Python instead. Set `UV_PYTHON` to the mise-selected interpreter path if mise must own interpreter installation, and verify the executable used by fixture tasks. Do not rely on a version number alone or the host's `/usr/bin/python3`. [4], [5]

uv should own Python dependency environments. Preserve the fixture's `jsonschema==4.25.1` pin when exposing its existing command through a mise task. A version pin on that direct dependency does not freeze its transitive dependencies. If fully reproducible fixture environments are wanted, discuss a uv-managed dependency lock or separately locked script environment. Do not add a Python project solely because mise is required. A `pypi:jsonschema` CLI environment would not automatically supply imports to an unrelated fixture interpreter. [2], [4]

mise can install Node directly, so existing `.mjs` backend probes need no new npm project. Pin/select Node through mise and invoke the existing Python runners or Node scripts from tasks. The recorded Node 26.13.0 is evidence for reproduction, not automatic approval of the future default. If npm dependencies appear later, npm still owns their manifest and lock. [2], [6]

## LLVM 23 and C/C++ availability

The released registry has `clang` backed first by `conda:clang`, then `vfox:jdx/vfox-clang`. `clang-format` has a Conda backend and the `mise-plugins/mise-llvm` source-build plugin. There is no `llvm.toml` or `lld.toml` entry in the inspected registry tree. This is a registry observation, not proof that no explicit backend can install those tools. The official mise-versions snapshot lists Clang **23.1.0 through 23.1.5**, but that catalogue contains neither a four-platform artifact matrix nor proof of a complete LLVM distribution. [10], [11]

| Host | Built-in Conda backend recognizes | Full LLVM 23 through mise verified here? |
| --- | --- | --- |
| macOS x86-64 | `osx-64` | No. Exact packages, dependency solve and usable bundle remain unverified. |
| macOS AArch64 | `osx-arm64` | No. Existing Homebrew 23.1.5 is not a mise installation test. |
| Linux x86-64 | `linux-64` | No. Exact packages, libc requirements and usable bundle remain unverified. |
| Linux AArch64 | `linux-aarch64` | No. Exact packages, libc requirements and usable bundle remain unverified. |

Specific limits from installer source:

- The built-in Conda backend supports those four platform subdirectories, but the requested package and every dependency must exist for the host. It solves each requested tool into a separate prefix and exposes the requested package's commands, not all dependency commands. Thus `conda:clang@23.1.5` cannot be assumed to expose `opt`, `llc`, `llvm-config`, LLVM libraries or `wasm-ld`, or form one coherent LLVM prefix with separately installed packages. Platform support in the backend is not package availability. [7]
- `jdx/vfox-clang` obtains a version manifest, downloads **latest Pixi**, then runs `pixi global install ... clang=<version>` in its install prefix. Its code names macOS/Linux x86-64/AArch64 Pixi archives. That verifies its platform dispatch, not availability of LLVM 23 packages or a complete bundle. Pinning this plugin's initial artifact/version would not freeze that later Pixi solve. [12]
- `mise-plugins/mise-llvm` downloads the LLVM source tag and builds **one executable named after the installed tool**, with `clang;clang-tools-extra` enabled. It requires CMake, Ninja and an existing host compiler. It does not configure LLD or install a full LLVM development prefix. Calling this a turnkey full LLVM installer would misdescribe the code. [13]

No package solve, archive inspection, source build or four-host execution was performed. Within the requested mise-only source scope, exact Conda 23.1.5 package availability and upstream full-distribution assets remain unresolved. Keep LLVM **23**, and keep the recorded **23.1.5** reproduction requirement separate from a future patch-version policy.

Before calling any route supported, verify the exact package/archive and checksum per host, required executables, headers/libraries, `llvm-config` version/prefix/build options, target backends and runtime library resolution. Then compile/link a native smoke test and run the existing IR/LLD probes. Cross-generating target objects does not validate the host installer or physical target hardware.

## Explicit routes for tools without a suitable installer

These options satisfy a mise entry-point requirement without quietly sending contributors to `brew install`:

1. Use an explicit supported backend rather than relying on registry preference. Conda is a candidate only after package/build/dependency checks pass. Pin the backend and review its generated lock entries.
2. Declare an exact full-distribution download with mise's HTTP or GitHub backend. HTTP supports per-platform URLs, checksums, extraction and binary-path options, and fails for a platform without a URL. This is an actual mise capability, but a four-platform LLVM 23 artifact manifest has **not** been established here. Do not invent download URLs. [8]
3. If suitable upstream artifacts are missing, discuss a project-maintained mise task or backend that builds pinned LLVM source using mise-provided CMake/Ninja and an explicitly checked bootstrap compiler. It must record source checksums, build flags, enabled projects/targets and installation prefix, and fail on unsupported hosts. This is new implementation and maintenance, not existing verified support.
4. A named, reviewed provisioning task could invoke a platform package manager when source builds or OS prerequisites require it. It must state that dependency, privilege requirements and version limitations, and verify results. It is an explicit exception within the mise workflow, not an undocumented fallback. No such exception is approved.

Apply the same policy to Binaryen, Wasmtime and other future tools. Their existing fetch code can initially remain behind an explicit reproduction task; calling it through mise alone does not move its downloads into `mise.lock`. A later migration must preserve existing digests and evidence semantics.

## Tasks, platform prerequisites and CI

mise supports TOML tasks and executable file tasks. Tasks run from their config root by default; `dir` can set another working directory. For the current root-relative runbooks, root tasks are sufficient as a discussion option. Set directories explicitly where needed and keep algorithms in existing scripts. No task-directory or compiler-workspace layout is selected. [1], [14]

`depends` schedules prerequisites and permits parallel execution; its list is not a sequence. `wait_for` only waits for work already scheduled. `depends_post` runs after a started task succeeds or fails, but not if its prerequisite failed before it started. Task-local `tools` do not apply to dependency tasks. Therefore a future tool-verification prerequisite needs its own tools or shared declarations. Avoid task freshness/caching rules that skip required fixture checks. [14]

A proposed noninteractive flow is a pinned mise bootstrap, reviewed configuration, strict tool installation including task tools, then named checks through `mise run`. Use `mise exec -- ...` when direct Cargo/Python/Node commands are needed. Neither requires interactive shell activation. Tasks may auto-install missing tools, so strict lock policy must apply when tasks run too, not only in a preceding install step. [2], [3], [14]

The official generated localized wrapper is an option for bootstrapping mise itself. It needs host `bash`, `curl`, CA certificates, `tar` and `sha256sum` or `shasum`; it is not an OS sandbox. Pin its mise version explicitly. GitHub's mise-action is another option; pin the action revision and mise release separately. Prefer verified downloads over trusting restored installed-tool trees. [9]

Trust is not an interactive-prompt problem alone. In the cited release, code-running commands automatically trust active config outside paranoid mode, and CI normally trusts config without prompting. A localized wrapper also trusts project config. Review tasks/hooks/templates/plugins before running them, and do not expose secrets to untrusted PR code. `MISE_SAFE=1` is documented for restricted metadata/lock operations but refuses tasks, so it cannot sandbox an untrusted test suite. Blanket `--yes` or `mise trust --all` is not a security policy. [9], [15]

mise does not supply the selected Apple SDK simply by installing Clang. The current native experiments require Xcode/SDK tools; extended backend probes also require Rosetta and Docker. A future mise prerequisite task should check SDK selection, compiler/linker paths and required services, then stop with explicit platform requirements. Apple software installation, licenses and privileged platform setup remain external prerequisites unless a specific provisioning path is approved. For Linux, specify libc, linker, headers and native-library requirements per supported distribution rather than claiming that a downloaded compiler supplies them all. The Conda docs explicitly retain native host requirements. [7]

For reproduction tasks, pass explicit LLVM/linker locations rather than the current Homebrew defaults. Keep assertions enabled. Direct new outputs to temporary/untracked paths, including SQLite's `--report`. Preserve the distinction between expected experimental failures, green runner exits and actual Rill support.

## Decisions still needed

- Choose a tested LLVM 23 installation route and artifact/build matrix. No four-host full LLVM installation is established by this research.
- Choose the Rust version authority and the mise release/bootstrap policy. Compiler, standard-library and LSP workspace layout remains a separate interview question.
- Decide whether current fixture commands need locked Python transitive dependencies and whether special reproduction tools remain script-fetched or move into mise backends.
- Define supported host distributions/SDK prerequisites and explicit unsupported-host failures. Requiring mise does not approve hidden Homebrew dependencies, a platform drop or a different LLVM version.

## Primary sources

All inspected 2026-11-16. Mise links below pin release 2026.11.8; plugin and catalogue links pin their inspected commits.

[1]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/configuration.md
[2]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/dev-tools/mise-lock.md
[3]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/lang/rust.md
[4]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/lang/python.md
[5]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/registry/uv.toml
[6]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/lang/node.md
[7]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/dev-tools/backends/conda.md
[8]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/dev-tools/backends/http.md
[9]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/continuous-integration.md
[10]: https://github.com/jdx/mise/tree/a4a1996f49779db556570678c81f658e5a28fe19/registry
[11]: https://github.com/jdx/mise-versions/blob/ed596e0bb7991f5754e2f3005fcf6b66a34e62a9/docs/clang.toml
[12]: https://github.com/jdx/vfox-clang/blob/b625c2aa666ee956c25f725543eb3da82fa76bc9/lib/util.lua
[13]: https://github.com/mise-plugins/mise-llvm/blob/b7b8dd389c790237145e7436f1cd85b49611e37e/lib/utils.bash
[14]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/tasks/task-configuration.md
[15]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/cli/trust.md
