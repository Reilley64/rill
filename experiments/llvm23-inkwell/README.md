# Prototype: inkwell on a trimmed static LLVM 23.1.5 (ci-ajns)

Throwaway prototype, not the backend. It answers one question: can a Rust program using inkwell, statically linked against LLVM 23.1.5, build and run on the four GitHub-hosted platforms and emit an object file that the host `cc` links into a working program? What is the smallest LLVM that allows it?

**Answer: yes on all four, with a trimmed LLVM built from the pinned source archive.** The emitted program takes a name (owner request 2026-11-25): `main(argc, argv)` writes exactly `Hello Ada\n` for the argument `Ada`, and `Hello\n` (placeholder) with none; both exit 0. CI asserts those bytes and statuses.

## Runbook

CI is the reproduction: `.github/workflows/llvm23-inkwell.yml` runs on pushes to `prototype/llvm23-inkwell` that touch this directory or the workflow. Locally, with a prefix produced by `build-llvm.sh`:

```sh
./build-llvm.sh /tmp/llvm-work /tmp/llvm-prefix   # about 30-50 minutes, native backend only
./run.sh /tmp/llvm-prefix                         # builds, emits hello.o, links with cc, asserts output
FEATURES=no-libffi RUN_OUT=target/run-variant CARGO_TARGET_DIR=target-variant ./run.sh /tmp/llvm-prefix
```

Files: `build-llvm.sh` (trimmed LLVM), `src/main.rs` (the prototype, builds the module and writes the object), `run.sh` (build, emit, link, run, assert, print loader dependencies). The experiment is its own Cargo workspace; the root `Cargo.toml` lists its members explicitly, so no root change was needed. Nothing under `crates/`, `runtime/` or `stdlib/` changed.

Runs: [45520504977](https://github.com/Reilley64/rill/actions/runs/45520504977) (first full run: every LLVM built from scratch, all eight jobs green) and [45527613357](https://github.com/Reilley64/rill/actions/runs/45527613357) (LLVM from cache, adds the `no-libffi` variant, all green). The first push, with a `hello world` program, was cancelled when the brief changed and produced no result. Logs: `~/.t3/evidence/rill/ci-ajns/`.

## Does a released inkwell support LLVM 23?

Yes. inkwell **0.11.0** has `llvm23-1*` features on **llvm-sys 231.0.2** (published 2026-10-30; LLVM 23.1). No git branch or direct llvm-sys route was needed. Used:

```toml
inkwell = { version = "=0.11.0", default-features = false, features = ["llvm23-1-force-static", "target-x86" /* or target-aarch64 */] }
```

- `force-static` fails the build instead of falling back to `libLLVM` shared libraries. `LLVM_SYS_231_PREFIX` points llvm-sys at the prefix; `LLVM_SYS_231_STRICT_VERSIONING=1` rejects any other 23.x mismatch. llvm-sys finds the library list through `llvm-config --libnames --link-static`, so the prefix needs `llvm-config`.
- Default inkwell features include `target-all`, which references every `LLVMInitialize*` symbol and cannot link against a single-backend LLVM. Select the native `target-*` feature per architecture (`[target.'cfg(target_arch = ...)'.dependencies]`). I did not run the default features to see the failure; this follows from the feature definition and the successful single-backend links.
- The emitted object needs `RelocMode::PIC`; Linux `cc` links PIE by default. Untested alternative: default reloc mode.

## Trimmed LLVM

Built in CI from `llvm-project-23.1.5.src.tar.xz` (sha256 checked against the pin in `tools/llvm/common.py`) on every platform with one recipe. I did not cut the upstream archives: they are 1.5–2 GB, none exists for macOS x86-64, and I did not test whether `llvm-config` survives deleting libraries from a full archive (its component list lists every library, and llvm-sys links all of them). That route is untested, not ruled out.

Configure flags (backend `X86` on x86-64 hosts, `AArch64` on arm64 hosts):

```text
-G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=<prefix>
-DLLVM_TARGETS_TO_BUILD=<X86|AArch64>
-DLLVM_ENABLE_PROJECTS= -DLLVM_ENABLE_RUNTIMES=
-DLLVM_BUILD_TOOLS=ON -DLLVM_INCLUDE_TOOLS=ON
-DLLVM_INCLUDE_TESTS=OFF -DLLVM_INCLUDE_EXAMPLES=OFF -DLLVM_INCLUDE_BENCHMARKS=OFF
-DLLVM_INCLUDE_DOCS=OFF -DLLVM_INCLUDE_UTILS=OFF -DLLVM_ENABLE_BINDINGS=OFF
-DLLVM_ENABLE_ASSERTIONS=OFF
-DLLVM_ENABLE_ZLIB=OFF -DLLVM_ENABLE_ZSTD=OFF -DLLVM_ENABLE_LIBXML2=OFF
-DLLVM_ENABLE_LIBEDIT=OFF -DLLVM_ENABLE_FFI=OFF
-DLLVM_BUILD_LLVM_DYLIB=OFF -DLLVM_LINK_LLVM_DYLIB=OFF
```

Build and install only `ninja install-llvm-headers install-llvm-libraries install-llvm-config`. `LLVM_BUILD_TOOLS=ON` is there because `install-llvm-config` does not exist without it; no other tool is built. The prefix holds `include/`, `lib/` (115–116 static `.a` files), `bin/llvm-config` and `BUILDINFO.txt`. No clang, no other tools. `llvm-config --link-static --system-libs` reports `-lrt -ldl -lm` on Linux and `-lm` on macOS.

Source extraction needs `llvm/`, `cmake/`, `third-party/` **and `libc/`**: CMake fails with "LLVM libc is not found" without the last. `LLVM_ENABLE_TERMINFO` and `LLVM_ENABLE_CURSES` do not exist in 23.1.5 (CMake reports them unused).

## Results per platform (first run, 45520504977)

All four: **PASS**. `Hello Ada\n` and `Hello\n`, exit status 0 each. Hosts are standard runners of this public repository (4 vCPU except `macos-15`, 3 vCPU).

| | ubuntu-24.04 | ubuntu-24.04-arm | macos-15 | macos-15-intel |
| --- | --- | --- | --- | --- |
| Backend | X86 | AArch64 | AArch64 | X86 |
| Installed size | 299 MiB (306,260 KiB) | 292 MiB (299,264 KiB) | 206 MiB (210,736 KiB) | 215 MiB (219,796 KiB) |
| Artifact (`.tar.gz`) | 86.2 MB | 84.6 MB | 63.3 MB | 66.0 MB |
| LLVM build and install (ninja) | 30 min 44 s | 27 min 57 s | 29 min 28 s | 50 min 31 s |
| LLVM total (fetch, configure, build) | 31 min 10 s | 28 min 15 s | 30 min 33 s | 52 min 16 s |
| Prototype `cargo build --release`, cold | 5 s | 8 s | 13 s | 31 s |
| Prototype loader dependencies | libm, libstdc++, libgcc_s, libc | same | libffi, libSystem, libc++, libiconv | same |
| Emitted program links | libc.so.6 (and ld.so) | same | libSystem.B | same |

Ninja steps: 2,381 (Linux) and 2,383 (macOS). The prototype build time covers inkwell, llvm-sys and the other dependencies from a cold target directory; it excludes mise tool installation. The Linux `ldd` lines show no libffi, although the same link directive is present; I did not check why (likely `--as-needed`). No `libLLVM` appears in any dependency list (`run.sh` asserts this).

### inkwell's libffi link

inkwell 0.11.0's `build.rs` links `libffi` unconditionally outside Windows, even with `LLVM_ENABLE_FFI=OFF`. On macOS that shows as `/usr/lib/libffi.dylib` in `otool -L`. The `no-libffi` feature (inkwell `no-libffi-linking`) removes it; run 45527613357 builds and passes with it on all four platforms (prototype build 6 s ubuntu-24.04, 8 s ubuntu-24.04-arm, 7 s macos-15, 59 s macos-15-intel). The loader lists then match the table without libffi.

### What stays dynamic

Static LLVM does not make the prototype self-contained. Linux binaries need libstdc++ and libgcc_s from the host; macOS binaries need libc++, libiconv and libSystem. I did not try static libstdc++. The emitted `hello` needs only libc (Linux) or libSystem (macOS), because it calls `strlen`, `memcpy` and `write` and the host `cc` supplies the startup objects.

## What I would change

`tools/llvm/` and `docs/development.md`:

1. Add a trimmed source profile next to the existing routes: the recipe above, one route for all four hosts, including macOS x86-64. It builds in 28–52 minutes on a hosted runner.
2. Rewrite the resource claims in "Optional LLVM 23.1.5 profile". The 48 GiB (bundle) and 80 GiB (source) budgets and "exceed typical small hosted-runner capacity" do not describe this profile. I did not measure peak disk or memory; the builds ran on standard runners without failure.
3. Narrow `TARGETS` in `tools/llvm/common.py` per profile. It lists all 19 backends; the trimmed profile builds one.
4. Extend `test:llvm` from the C-API smoke to this inkwell smoke. The doc text "integration with inkwell is unverified" is now answered for emission of one object on four hosts, and nothing more.
5. Record the integration facts a developer needs: `LLVM_SYS_231_PREFIX`, strict versioning, `force-static`, the per-architecture `target-*` features (not `target-all`), `no-libffi-linking`, and the extra `libc/` source directory.
6. Cache the built prefix (this workflow keys `actions/cache` on the build script's hash) so CI pays the 30–50 minutes once. The uploaded artifacts expire after 14 days.

Contradictions and tensions with ADR 0006:

- It does not contradict the choice of inkwell pinned to LLVM 23: a released inkwell supports it.
- Its reasons for LLVM 23 are RISC-V `musttail` with `byval` and Wasm reference types, and the intended targets include `wasm32-wasip1`. A native-only trimmed LLVM has no WebAssembly or RISC-V backend, so the target set is a profile decision; this prototype does not test the Wasm path.
- "Statically linked into `rill`" holds for LLVM only; see "What stays dynamic".
- The ADR rejects external LLVM tools because users would need LLVM installed. The prototype needs none: the object comes from `TargetMachine`. Linking still needs the host `cc`; `docs/development.md` leaves the linker undecided, and this prototype did not decide it.

## Limits

- Evidence is one program, one object, no optimization (`OptimizationLevel::None`), no tail calls, no Wasm, no JIT.
- Standard runners of a public repository. A private repository gets different runner sizes and limits.
- Timings come from one run per platform, with no repeat for variance; the macOS x86-64 build took about 1.6 times the Linux build.
- Tools run were `mise exec` with the repository's pinned Rust 1.98.0; the hosted images' default C/C++ compilers built LLVM, and I did not record their versions.
