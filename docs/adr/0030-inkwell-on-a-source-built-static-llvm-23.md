# inkwell on a source-built static LLVM 23.1.5, published as a release asset

**Status:** accepted, 2026-11-25 (owner decision). Drafted from Hello World grilling decisions Q30 and Q31, 2026-11-25; the owner decided Q10 that each ABI decision is recorded as an ADR the chief of staff drafts. M6 slice 1 needs a code generator before it can run a program, so this ADR records how Rill reaches LLVM and where `rill` gets it. [ADR 0006](0006-whole-program-monomorphization-on-llvm-23.md) chose inkwell pinned to LLVM 23 and did not say how a developer or CI obtains LLVM, or what "statically linked" covers.

## Decision

- **Binding.** The backend uses inkwell 0.11.0 on LLVM 23.1.5 (Q30).
- **Provisioning.** Each platform's LLVM is built from source with the native backend only (Q30). The result is published as a release asset on `Reilley64/rill`, under the tag `llvm-23.1.5-1`. That tag sits on `main` as an anchor, and the release notes name the build commit `54cdec3b661812d2131d09d3bd0ac535583bcbe6` (workflow run <https://github.com/Reilley64/rill/actions/runs/45535618750>). This is a chief of staff ruling of 2026-11-25. CI and `mise run setup:llvm` download it and check its checksum. A workflow that a person triggers by hand is the only thing that rebuilds it (Q30).
- **Linking.** `rill` links LLVM statically and loads the system C++ and C libraries dynamically (Q31).
- **Host linker.** `rill build` links the program through the host `cc` driver, a declared prerequisite (grilling round 1, Q6). The object comes from LLVM's target machine, so the user needs no installed LLVM.

## Evidence

The throwaway prototype `ci-ajns` (branch `prototype/llvm23-inkwell`, report `experiments/llvm23-inkwell/README.md`) built and ran one program on `ubuntu-24.04`, `ubuntu-24.04-arm`, `macos-15` and `macos-15-intel`. The chief of staff verified runs 45520504977 and 45527613357. It is a prototype, not the backend; its limits are one program, one object, no optimization, no tail calls and no Wasm.

- A released inkwell supports LLVM 23: 0.11.0 has `llvm23-1*` features on llvm-sys 231.0.2. No git dependency was needed.
- The prototype built LLVM from the pinned `llvm-project-23.1.5.src.tar.xz` for one backend per host (X86 or AArch64), installing headers, static libraries and `llvm-config` only. Installed size was 206 to 299 MiB, the compressed artifact 63 to 86 MB, and the LLVM build 28 to 52 minutes on a hosted runner.
- It linked LLVM through `force-static`, selected the native `target-*` feature for each architecture because the default `target-all` references backends a one-backend LLVM lacks, and emitted a position-independent object because Linux `cc` links PIE by default.
- Static LLVM leaves loader dependencies: libstdc++ and libgcc_s on Linux, libc++, libiconv and libSystem on macOS. No `libLLVM` appeared.
- inkwell 0.11.0 links `libffi` unconditionally outside Windows. The `no-libffi-linking` feature removed it, and the prototype passed with it on all four platforms.

These are findings, not decisions. The exact feature set, the cut of LLVM and the build recipe belong to the tickets that implement the workflow and the backend.

## Consequences

- ADR 0006's "statically linked into `rill`" means LLVM only, and a native-only LLVM cannot build Wasm. Both are recorded as dated notes on that ADR.
- Provisioning no longer follows the route in `docs/development.md` (checksum-pinned upstream archives for three hosts and a source build for macOS x86-64, with 48 GiB and 80 GiB budgets). The prototype contradicts those budgets. That document still describes the earlier route and needs its own update.
- Provisioning needs the network once, then a checksum. Nobody builds LLVM except through the manual workflow.
- The system C++ runtime becomes a runtime dependency of `rill` on every platform, and a missing one is a missing prerequisite, like `cc`.

## Considered options

- **Cut the upstream LLVM archives.** They are 1.5 to 2 GB, and upstream ships no macOS x86-64 archive for this release. Whether `llvm-config` survives deleting libraries from a full archive was untested, so this route is untested, not ruled out.
- **Shared LLVM (`libLLVM`) or a system LLVM such as Homebrew's.** The user would need LLVM installed, and `docs/development.md` already refuses an implicit Homebrew prefix and a shared fallback. The wording of Q31 (static LLVM) excludes it.
- **Raw `llvm-sys`, and textual `.ll` with external LLVM tools.** ADR 0006 rejected both.
- **Build LLVM in every CI run.** The wording of Q30 (a published asset, rebuilt only by hand) excludes it. The prototype's first full run needed 28 to 52 minutes per platform.

The ri-qlh record gives no reasons for rejecting the shared, system or per-run options. The reasons above come from ADR 0006, `docs/development.md` and the prototype.

## Open

The owner did not decide these. Nothing above decides them.

- Whether any platform's artifact also carries other backends, and how a Wasm or RISC-V build gets its LLVM. The native-only LLVM gives neither, and ADR 0006's reasons for LLVM 23 (RISC-V `musttail` with `byval`, Wasm reference types) rest on those targets.
- The artifact name and layout, and the checksum file format.
- Whether to use the `no-libffi-linking` feature, and whether to link libstdc++ statically on Linux. The prototype did not try the latter.
- The policy for moving to a later LLVM release, and how long a published asset stays available. The prototype's CI artifacts expire after 14 days.
- Which hosts the declared `cc` prerequisite covers beyond the four hosted platforms, and what `rill build` reports when `cc` or the system C++ runtime is missing.
