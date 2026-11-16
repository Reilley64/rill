# LLVM 23 provisioning options for interview #46

Inspected 2026-11-16 UTC. This is unapproved interview input, not an installation contract. [ADR 0006](../adr/0006-whole-program-monomorphization-on-llvm-23.md) retains LLVM 23 through inkwell, with LLVM statically linked into `rill`. Recorded experiments use **23.1.5**. There is no compiler workspace yet. This report selects neither a binding dependency nor a Rust version.

## Finding

Actual LLVM 23.1.5 binary artifacts exist. LLVM's release supplies Linux x86-64, Linux AArch64 and macOS AArch64 archives, with published SHA-256 digests and provenance bundles. It supplies **no macOS x86-64 archive**. Conda-forge publishes LLVM development, Clang and LLD packages at 23.1.5 for **all four hosts**. Those facts improve on the unresolved artifact question in [the mise investigation](mise-toolchain-management.md), but do not establish a working four-host setup. [1], [2], [7], [8], [9]

A source build is not proven necessary on every host. It remains a fallback, and is needed for macOS x86-64 if the requirement is one unmodified LLVM-upstream distribution route rather than Conda-forge or project-built packages. Conda is a credible four-host candidate, subject to prefix assembly, native-library closure and static-link tests. The built-in mise Conda backend does not combine separately declared tools into one environment. [14]

## Upstream release artifacts

The inspected release is GitHub release `432551565`, published 2026-11-03. Tag `llvmorg-23.1.5`, annotated tag object `604d7ddfacefc074968a9ba0d88c667ab31b42bd`, resolves to commit `392dfff97d29b59d77e5cac6d4b6ef2585fe7117`. [1], [2]

These are real assets from the release API, not inferred URLs. Sizes are compressed bytes. SHA-256 values describe the archive, not the installed tree. [1]

| Native host | Asset name | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| Linux x86-64 | `LLVM-23.1.5-Linux-X64.tar.xz` | 2014093084 | `f72edb0b36580ece7f52e23858f00c18fad4e87395ea7ad05d7e55c39f8fc78a` |
| Linux AArch64 | `LLVM-23.1.5-Linux-ARM64.tar.xz` | 1828141344 | `44f1e4b7a125b2763d56386a199928dd57ec32e151cc6042dce2f7b0da7bb651` |
| macOS AArch64 | `LLVM-23.1.5-macOS-ARM64.tar.xz` | 1569733272 | `aba592b4a15e8f90c634c5367b51ec8418b8bcd2829865db331a17dc9418dcff` |
| macOS x86-64 | No asset in the inspected release | | |

All existing archive URLs have the base `https://github.com/llvm/llvm-project/releases/download/llvmorg-23.1.5/`, followed by the exact asset name. The API also lists `.tar.zst` alternatives. LLVM states that those contain the same package contents. The macOS Intel link in the release body is an HTML comment, not evidence of an asset. Stable release manifests 23.1.0 through 23.1.4 also contain no macOS x86-64 archive. This does not rule out independently maintained community distributions. [1], [3]

Each listed `.xz` archive has a small `.jsonl` provenance bundle. Its decoded subject digest agrees with the API value; its resolved source commit is `392dfff97d29b59d77e5cac6d4b6ef2585fe7117`, and its invocation is GitHub Actions run `41619638772`, attempt 1. I inspected these statements, but did not cryptographically verify signatures or hash any archive bytes. Provenance is not a file-content inventory or a successful Rill build. [4]

### What the upstream recipes establish

The pinned release workflow uses `clang/cmake/caches/Release.cmake` and packages the final bootstrap stage. This is a development-distribution candidate, not merely a Clang driver download. The recipe enables Clang and LLD alongside other projects. LLVM defaults install headers, `llvm-config`, ordinary LLVM tools and static component libraries. `LLVM_INSTALL_TOOLCHAIN_ONLY` and `BUILD_SHARED_LIBS` default to `OFF`. The release packaging script explicitly processes `lib/*.a` to remove LTO sections. These sources establish build intent, not an inspected archive inventory. [5], [6], [22]

The release cache does not enable the monolithic LLVM dylib. Static component libraries are therefore expected; a usable `libLLVM.so` or `libLLVM.dylib` must not be assumed. The tool executables' linkage is a separate question. The cache leaves static linkage of the C++ runtime disabled. It selects static zstd, and the Linux workflow builds static libxml2 and selects it for the final stage. Metadata does not establish whether all required external archives, link names and search paths are present and relocatable for a downstream static LLVM consumer. [5], [6], [22]

| Native host | Development prefix, Clang and LLD | Host prerequisites and limits |
| --- | --- | --- |
| Linux x86-64 | Published archive; full-prefix intent above. LLD's source installs `wasm-ld` as well as `ld.lld`. | Recipe builds on Ubuntu 22.04. That is not an audited minimum glibc or GLIBCXX symbol requirement. Native C/C++ runtime, linker and libc development files remain prerequisites unless separately provisioned. |
| Linux AArch64 | Published archive; same full-prefix intent and LLD aliases. | Recipe uses Ubuntu 22.04 ARM. The same unresolved runtime and sysroot requirements apply independently on AArch64. |
| macOS AArch64 | Published archive; same full-prefix intent and LLD aliases. | Recipe uses macOS 14 runners and restricts Darwin runtime builds to arm64. This is not a verified minimum deployment version. A selected Apple SDK and platform tools remain external prerequisites. |
| macOS x86-64 | No upstream 23.1.5 archive to declare. | Conda-forge or a pinned native source build remains a candidate. An arm64 archive or Rosetta does not establish native x86-64 provisioning. |

Workflow and linker facts in this table come from the pinned sources. No row is an installation result. LLD builds ELF, Mach-O, COFF, MinGW and Wasm drivers, but a driver being present does not supply target libraries or certify its use as the native platform linker. [5], [10]

## Conda-forge package availability

The version-specific Anaconda APIs list the following native artifacts. Every entry is **23.1.5**, build number **0**, in the named subdirectory. Cells give the build string, so the LLVM development filename is `llvmdev-23.1.5-<build>.conda`; the versioned driver filename is `clang-23-23.1.5-<build>.conda`. [7], [8], [9]

| Subdirectory | `llvmdev` | `clang-23` | `lld` |
| --- | --- | --- | --- |
| `osx-64` | `h36529c3_0` | `default_hd4d7be2_0` | `he8cbeb5_0` |
| `osx-arm64` | `h79441dc_0` | `default_hf8b46ca_0` | `h98a3de6_0` |
| `linux-64` | `h7399f5f_0` | `default_h782a7a8_0` | `h5985d43_0` |
| `linux-aarch64` | `h2c72a67_0` | `default_h8d1ab40_0` | `h260d519_0` |

The API's SHA-256 values pin these particular `llvmdev` artifacts. They are not a complete environment lock. The Clang, LLD, `libllvm23` and LLVM tool APIs also publish individual artifact hashes and dependency records. [7], [8], [9], [11]

| Subdirectory | `llvmdev` SHA-256 |
| --- | --- |
| `osx-64` | `52eed74657899eba865e2112b1c2d0b084d46db96199a8379423f91dc0a11303` |
| `osx-arm64` | `e97ad3e0a696e2b189127f9ed7dee4f064e70c0c69e375458a5afd0334850148` |
| `linux-64` | `bc95a56744f5fc6fb64efc9b0e218afe811c713de91e1f09d88751073ebcf8c9` |
| `linux-aarch64` | `a719d1e27d365e85b46d2d6a6512fd46641a4cfcc9e005e6f028aee02945cd25` |

### Package contents and native dependencies

The pinned LLVM feedstock describes `llvmdev` as development headers/libraries and tests `include/llvm/Pass.h`, `lib/libLLVMCore.a` and `bin/llvm-config`. Its dependencies pin matching `libllvm23` and `llvm-tools` builds. The tool packages supply `llc`, `opt` and other executables through versioned files and unversioned symlinks; they deliberately exclude `llvm-config`. `llvm` is a shared-runtime metapackage, not the development prefix. `libllvm23` supplies the monolithic shared LLVM library. The recipe enables both that dylib and static component libraries, so the availability of a shared runtime does not by itself exclude static LLVM linkage. [11], [12]

Clang and LLD are separate packages. `clang-23` is the versioned driver package, not LLVM development headers/libraries. The unversioned `clang` packages also exist on all four hosts, with `cfg` and `nocfg` variants. Their native-toolchain dependency sets are larger. Linux packages request binutils, `libgcc-devel_linux-*` and `sysroot_linux-*`; macOS packages request cctools/ld64 and matching LLVM tools. Neither spelling of Clang replaces `llvmdev`. The Clang recipe has its own patches and links against packaged LLVM. [8], [13], [25]

The LLD feedstock builds and installs the upstream LLD project without suppressing the Wasm driver. Combined with upstream's alias rules, that is evidence that `wasm-ld` is intended to ship on all four hosts. The recipe only tests `ld.lld --version` on Linux, so actual `wasm-ld` files, links and execution still need inspection. [9], [10], [18]

The published dependency records give these host-library requirements. They are bounds and direct or one-level transitive dependencies, not a solved closure. [7], [8], [9], [11]

- Both macOS `llvmdev` packages require `__osx >=11.0`, `libcxx >=21`, libzlib and zstd, plus matching shared LLVM/tools. The Clang driver raises the libcxx bound to `>=23.1.5`. These Conda libcxx requirements must not silently resolve to an arbitrary system libc++.
- Both Linux development packages require `libgcc >=15`, `libstdcxx >=15`, libzlib and zstd, plus matching shared LLVM/tools. The x86-64 records explicitly require `__glibc >=2.17,<3.0.a0`. The AArch64 records inspected do not carry that explicit virtual-package bound; its pinned feedstock configuration uses a 2.17 build sysroot. Neither fact establishes compatibility with every Linux distribution or with musl.
- `libllvm23` adds `libxml2` and `libxml2-16 >=2.15.4`. The inspected zlib and zstd bounds are `libzlib >=1.3.2,<2.0a0` and `zstd >=1.5.7,<1.6.0a0`. Clang also brings compiler-rt and libclang-cpp. LLD depends on shared LLVM at runtime.

A runtime solve is not proof that all development linker names and external libraries needed by `llvm-config --link-static --system-libs` exist. Link-time development packages, C++ runtime selection and eventual `rill` runtime dependencies need a separate check. Conda-forge metadata proves publication, not source certification. The pinned feedstocks describe source URLs, checksums and patches, but this inspection did not establish an authenticated source-to-binary chain for the listed packages. Matching 23.1.5 labels do not establish that chain or ABI coherence. [12], [13], [17], [18]

## Backend targets and SDKs

LLVM's final release stage defaults to `LLVM_TARGETS_TO_BUILD=all`. The `Native` setting in the release cache applies to stage 1, and is not in Clang's default bootstrap passthrough list. The Conda LLVM recipe also leaves the standard target list at its default and adds WebAssembly explicitly. The exact standard list at the pinned LLVM commit is: [6], [12], [22], [23]

```text
AArch64 AMDGPU ARM AVR BPF Hexagon Lanai LoongArch Mips MSP430
NVPTX PowerPC RISCV Sparc SPIRV SystemZ VE WebAssembly X86 XCore
```

Thus X86, AArch64, RISCV and WebAssembly are intended in these builds, independently of the native host architecture. Experimental targets ARC, CSKY, DirectX, M68k and Xtensa are not part of `all`. Recipe defaults are not a substitute for the installed `llvm-config --targets-built`, `llc --version` and target-emission tests. No per-triple CPU-feature, ABI, `musttail` or Wasm acceptance follows from a backend name. [6]

Backend inclusion also does not supply a sysroot. Apple SDK headers, system-library stubs, deployment target and platform tools must be explicitly located on **both** macOS hosts. Apple's TN2339 identifies the separately installed Command Line Tools package as containing the macOS SDK, and describes Xcode tool selection. That document establishes ownership, not a current SDK-version requirement. Linux builds need the chosen libc headers, startup objects, native linker and runtime/development libraries on **each** architecture. Clang's cross-compilation documentation separates backend selection from target headers, libraries and binutils. A Wasm backend and `wasm-ld` do not supply a WASI SDK/sysroot. None of these artifacts certifies Rill's target matrix. [24], [26]

## Mise routes and tradeoffs

The baseline is mise 2026.11.8, commit `a4a1996f49779db556570678c81f658e5a28fe19`, as in the earlier report. These are declaration capabilities, not executed configurations. [14], [15], [21]

| Candidate | What can be declared | Remaining cost or blocker |
| --- | --- | --- |
| Upstream archives through HTTP | Exact per-host URLs and SHA-256 values above, extraction options and `bin_path`. Preserve the whole prefix, not just executables. | Three available hosts, no macOS x86-64 asset. Archive layout, external library closure and static-link suitability are untested. |
| Upstream archives through GitHub | `github:llvm/llvm-project`, exact tag `llvmorg-23.1.5`, explicit per-platform asset names and hashes. The backend supports artifact-attestation checks. | Same missing host. Do not let asset autodetection choose source/docs archives or invent the commented Intel URL. Extraction must preserve headers and libraries. |
| Built-in Conda | Explicit `conda:llvmdev`, `conda:clang` or `conda:clang-23`, and `conda:lld` at 23.1.5, with per-host dependency locks. | Each declaration gets a separate solved prefix. Only the requested package's commands enter PATH. `llvmdev`'s dependency tools are not automatically exposed, and three declarations do not create one coherent prefix. Exact prefix paths, activation and native dependencies need testing. |
| Project-owned source task/backend | A checksum-pinned LLVM source archive, mise-provided build tools, explicit native prerequisites and a private installation prefix. | New provisioning code, build time and maintenance on all four hosts. No source-build implementation or result exists here. |

For Conda, either use separate, explicitly verified prefixes for their distinct roles or design a reviewed task/backend that assembles one locked environment. The built-in backend does not consume `environment.yml`. This report does not approve a solver, aggregate package or new provisioning mechanism. Conda package archives should not be treated as ordinary HTTP tarballs; Conda installation includes dependency resolution and prefix handling. [14]

A source route can pin `llvm-project-23.1.5.src.tar.xz`, SHA-256 `fb50b1eac93ddeb3aa8dab4cfe324dff74f4a9c2bdee6160c9e36188f8d06a6e`, at the same release base URL. Candidate build requirements are CMake >=3.20, Python >=3.8, Ninja or another supported generator, a C++17-capable bootstrap toolchain and the native SDK/sysroot. Configure LLVM with `clang;lld`, an explicit complete backend list, `BUILD_SHARED_LIBS=OFF`, `LLVM_INSTALL_TOOLCHAIN_ONLY=OFF`, and a full install rather than a single executable target. Optional native libraries must be deliberately pinned or disabled, not discovered accidentally from Homebrew. Assertions, runtime linkage and resource limits need an explicit policy. These are inputs for a future recipe, not a tested command. [1], [6], [16]

## Binding and verification boundary

Inspected inkwell commit `04bfd21f4cbee15776555b05b00d68f403455399` exposes `llvm23-1` and `llvm23-1-force-static`, delegating to llvm-sys 231. Inspected llvm-sys commit `fb6485983194bea2a8609044f44addffc2322f4e` uses `LLVM_SYS_231_PREFIX`, compiles a C wrapper against LLVM headers, and asks `llvm-config` for library names and system libraries. A driver-only installation or Rust's `llvm-tools` component is not evidence of these requirements. [17], [19]

Use an explicit static requirement in any later dependency proposal. Despite stale manifest comments claiming a force-static default, the inspected llvm-sys implementation defaults to *prefer* static and permits a shared fallback. Its strict-version check matches major/minor, not patch; exact 23.1.5 reproduction needs a separate prefix/version check. On macOS, failed prefix discovery can still fall back to Homebrew, and system-library search also consults Homebrew. A mise-only workflow must detect/prevent that leakage rather than assume the prefix variable is a sandbox. [17]

Static LLVM does not mean a fully static executable. llvm-sys normally links reported system libraries dynamically unless `crt-static` applies. Inkwell's inspected build script also requests dynamic libffi on non-Windows unless its `no-libffi-linking` feature is used. Libffi availability or the validity of disabling it must be checked in the eventual dependency proposal; it is not supplied merely by selecting LLVM's default `LLVM_ENABLE_FFI=OFF`. No feature selection is approved here. [6], [17], [20]

Before accepting any route, later installation/smoke tests must establish:

1. The selected artifact bytes match the pinned digest and applicable provenance verification. Inspect actual headers, generated config headers, `llvm-config`, every requested static library, Clang and LLD aliases. Reject mixed prefixes or unexpected fallback paths.
2. Record `llvm-config` version/prefix, `--targets-built`, `--libnames --link-static` and `--system-libs --link-static`. Confirm external archives/link names and runtime loader dependencies. Check macOS deployment/SDK compatibility and Linux libc/GLIBCXX requirements on each host, without unrecorded Homebrew libraries.
3. Compile and link a minimal inkwell/LLVM consumer with static LLVM enforced. Inspect the resulting executable to rule out a shared LLVM dependency and record its remaining native-library requirements. Run it on each native host.
4. Run Clang/LLD and backend-specific smoke tests, including `wasm-ld`, followed by the pinned experimental probes. Target object emission is not native-host installation validation or Rill backend acceptance.

Only APIs, small provenance manifests, documentation and source recipes were fetched. No binary/source archive was downloaded. No tool was installed or built, no LLVM/Clang/LLD binary was executed, and no mise/Homebrew state or project manifest changed. This report does not choose a downgrade, drop a platform, certify a distribution or approve a provisioning recommendation.

## Primary sources and revision pins

All live metadata below was inspected 2026-11-16. Version-specific APIs can change after inspection; exact artifact names and hashes above identify the observed candidates. Source links pin commits rather than branch heads.

[1]: https://api.github.com/repos/llvm/llvm-project/releases/tags/llvmorg-23.1.5
[2]: https://api.github.com/repos/llvm/llvm-project/git/tags/604d7ddfacefc074968a9ba0d88c667ab31b42bd
[3]: https://api.github.com/repos/llvm/llvm-project/releases/tags/llvmorg-23.1.0
[4]: https://github.com/llvm/llvm-project/releases/download/llvmorg-23.1.5/LLVM-23.1.5-Linux-X64.tar.xz.jsonl
[5]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/.github/workflows/release-binaries.yml
[6]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/CMakeLists.txt
[7]: https://api.anaconda.org/release/conda-forge/llvmdev/23.1.5
[8]: https://api.anaconda.org/release/conda-forge/clang-23/23.1.5
[9]: https://api.anaconda.org/release/conda-forge/lld/23.1.5
[10]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/lld/tools/lld/CMakeLists.txt
[11]: https://api.anaconda.org/release/conda-forge/libllvm23/23.1.5
[12]: https://github.com/conda-forge/llvmdev-feedstock/tree/a5c37225c122af1a8b6443aeb0ed68557d0e0a1b
[13]: https://github.com/conda-forge/clangdev-feedstock/tree/30fd4e93e83162141ae372db80ef9db7baa31094
[14]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/dev-tools/backends/conda.md
[15]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/dev-tools/backends/http.md
[16]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/docs/GettingStarted.md
[17]: https://github.com/tari/llvm-sys.rs/blob/fb6485983194bea2a8609044f44addffc2322f4e/build.rs
[18]: https://github.com/conda-forge/lld-feedstock/tree/883a2c4b6e49af2cfbc33871cb6c75508e70124b/recipe
[19]: https://github.com/TheDan64/inkwell/blob/04bfd21f4cbee15776555b05b00d68f403455399/Cargo.toml
[20]: https://github.com/TheDan64/inkwell/blob/04bfd21f4cbee15776555b05b00d68f403455399/build.rs
[21]: https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/dev-tools/backends/github.md
[22]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/clang/cmake/caches/Release.cmake
[23]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/clang/CMakeLists.txt
[24]: https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/clang/docs/CrossCompilation.md
[25]: https://api.anaconda.org/release/conda-forge/clang/23.1.5
[26]: https://developer.apple.com/library/archive/technotes/tn2339/_index.html

Additional inspected files under those pins:

- [23.1.1](https://api.github.com/repos/llvm/llvm-project/releases/tags/llvmorg-23.1.1), [23.1.2](https://api.github.com/repos/llvm/llvm-project/releases/tags/llvmorg-23.1.2), [23.1.3](https://api.github.com/repos/llvm/llvm-project/releases/tags/llvmorg-23.1.3), [23.1.4](https://api.github.com/repos/llvm/llvm-project/releases/tags/llvmorg-23.1.4) asset manifests.
- [Linux AArch64 provenance](https://github.com/llvm/llvm-project/releases/download/llvmorg-23.1.5/LLVM-23.1.5-Linux-ARM64.tar.xz.jsonl), [macOS AArch64 provenance](https://github.com/llvm/llvm-project/releases/download/llvmorg-23.1.5/LLVM-23.1.5-macOS-ARM64.tar.xz.jsonl).
- LLVM [release cache](https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/clang/cmake/caches/Release.cmake), [bootstrap passthrough](https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/clang/CMakeLists.txt), [archive LTO stripping](https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/clang/cmake/caches/release_cpack_pre_build_strip_lto.cmake), [system-library collection](https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/tools/llvm-config/CMakeLists.txt), [cross-compilation prerequisites](https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/clang/docs/CrossCompilation.md).
- Conda-forge [LLVM recipe](https://github.com/conda-forge/llvmdev-feedstock/blob/a5c37225c122af1a8b6443aeb0ed68557d0e0a1b/recipe/meta.yaml), [build](https://github.com/conda-forge/llvmdev-feedstock/blob/a5c37225c122af1a8b6443aeb0ed68557d0e0a1b/recipe/build.sh), [install](https://github.com/conda-forge/llvmdev-feedstock/blob/a5c37225c122af1a8b6443aeb0ed68557d0e0a1b/recipe/install_llvm.sh), and per-host `.ci_support` configurations. [Clang recipe](https://github.com/conda-forge/clangdev-feedstock/blob/30fd4e93e83162141ae372db80ef9db7baa31094/recipe/recipe.yaml). [LLD recipe/build](https://github.com/conda-forge/lld-feedstock/tree/883a2c4b6e49af2cfbc33871cb6c75508e70124b/recipe).
- Package APIs for [unversioned Clang](https://api.anaconda.org/release/conda-forge/clang/23.1.5), [LLVM tools](https://api.anaconda.org/release/conda-forge/llvm-tools/23.1.5), [versioned LLVM tools](https://api.anaconda.org/release/conda-forge/llvm-tools-23/23.1.5).
- Mise [GitHub backend](https://github.com/jdx/mise/blob/a4a1996f49779db556570678c81f658e5a28fe19/docs/dev-tools/backends/github.md). Inkwell [Cargo features](https://github.com/TheDan64/inkwell/blob/04bfd21f4cbee15776555b05b00d68f403455399/Cargo.toml) and [libffi build script](https://github.com/TheDan64/inkwell/blob/04bfd21f4cbee15776555b05b00d68f403455399/build.rs). llvm-sys [manifest](https://github.com/tari/llvm-sys.rs/blob/fb6485983194bea2a8609044f44addffc2322f4e/Cargo.toml).
