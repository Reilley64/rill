# How should a Rust compiler drive LLVM?

Research for [issue #6](https://github.com/Reilley64/rill/issues/6). Facts were checked on 2026-10-21 against official docs, source code, and first-party repositories. Version numbers will go stale, so each one links to its source.

## LLVM release context

- The latest LLVM release is 23.1.4, published 2026-10-20. LLVM 23.1.0 shipped on 2026-08-25 and 22.1.0 on 2026-02-24. ([llvm-project releases](https://github.com/llvm/llvm-project/releases))
- LLVM calls its C API stability "best effort". Calls such as "create this type of instruction" are expected to be less stable than "take this IR file and JIT it". ([LLVM Developer Policy, C API changes](https://llvm.org/docs/DeveloperPolicy.html#c-api-changes))
- The textual IR format (`.ll`) is not backwards compatible. Bitcode (`.bc`) is: current LLVM loads any bitcode since version 3.0. ([LLVM Developer Policy, IR backwards compatibility](https://llvm.org/docs/DeveloperPolicy.html#ir-backwards-compatibility))

## llvm-sys

`llvm-sys` is a raw, unsafe Rust binding to the LLVM C API. ([README](https://gitlab.com/taricorp/llvm-sys.rs/-/blob/main/README.md))

**LLVM versions.** Each crate major version targets one LLVM release. The crate version is the LLVM version times ten, so `llvm-sys` 191 is for LLVM 19.1.x. The build script accepts any LLVM at least as new as the target unless the `strict-versioning` feature is on, and it blocks versions known to be binary-incompatible. ([README, LLVM compatibility](https://gitlab.com/taricorp/llvm-sys.rs/-/blob/main/README.md#llvm-compatibility)) The newest release is 231.0.1 (LLVM 23.1), published 2026-10-09. Releases 160 through 221 got patch releases the same day. ([crates.io API](https://crates.io/api/v1/crates/llvm-sys))

**Finding and linking LLVM.**
- The build needs `llvm-config` for the chosen LLVM version. It looks on `PATH`, or in `<prefix>/bin` when `LLVM_SYS_<version>_PREFIX` is set (for example `LLVM_SYS_221_PREFIX`). ([README, build requirements](https://gitlab.com/taricorp/llvm-sys.rs/-/blob/main/README.md#build-requirements))
- The README says most binary LLVM distributions, including the official release packages, do not ship `llvm-config`. It names apt.llvm.org packages, Fedora `llvm-devel`, and Arch `llvm` as ones that do. Otherwise it suggests building LLVM from source. ([README, build requirements](https://gitlab.com/taricorp/llvm-sys.rs/-/blob/main/README.md#build-requirements))
- Linking is chosen with one of four features: `prefer-dynamic`, `force-dynamic`, `prefer-static`, `force-static`. The default has been `prefer-static` since crate version 191; before that it was `force-static`. ([README, cargo features](https://gitlab.com/taricorp/llvm-sys.rs/-/blob/main/README.md#cargo-features))
- `no-llvm-linking` skips linking so another crate can link LLVM. `disable-alltargets-init` drops the C-compiled target init functions, so the build works without a C compiler. ([README, cargo features](https://gitlab.com/taricorp/llvm-sys.rs/-/blob/main/README.md#cargo-features))
- The crate declares that it links `llvm-<major>`. So one crate can list several `llvm-sys` versions as optional dependencies, as long as only one is enabled. ([README](https://gitlab.com/taricorp/llvm-sys.rs/-/blob/main/README.md))
- The README says cross-compilation "will theoretically work, but hasn't been tested". ([README, cross-compilation](https://gitlab.com/taricorp/llvm-sys.rs/-/blob/main/README.md#cross-compilation))

**Ergonomics.** Most interfaces are undocumented in the bindings. The README points users to the LLVM docs and Doxygen. ([README, documentation](https://gitlab.com/taricorp/llvm-sys.rs/-/blob/main/README.md#documentation)) Every call is `unsafe`.

**Maintenance.** The GitLab repository had commits and a release in October 2026. ([commit API](https://gitlab.com/api/v4/projects/taricorp%2Fllvm-sys.rs/repository/commits?per_page=5))

## inkwell

`inkwell` is a safe, typed wrapper over `llvm-sys`. It tries to catch some IR typing errors at Rust compile time. ([README](https://github.com/TheDan64/inkwell/blob/master/README.md))

**LLVM versions.** inkwell 0.11.0 was published 2026-10-20. ([crates.io API](https://crates.io/api/v1/crates/inkwell)) It supports LLVM 12 through 23 via one feature flag per version, from `llvm12-0` to `llvm23-1`. Each flag pulls in the matching `llvm-sys` (for example `llvm23-1` uses `llvm-sys` 231.0.1). LLVM 12 to 14 also turn on `typed-pointers`. ([Cargo.toml](https://github.com/TheDan64/inkwell/blob/master/Cargo.toml)) A project picks exactly one version flag. ([README, usage](https://github.com/TheDan64/inkwell/blob/master/README.md#usage)) It needs Rust 1.85 or newer. ([README](https://github.com/TheDan64/inkwell/blob/master/README.md#requirements))

**Linking.** inkwell repeats the `llvm-sys` link options per version, for example `llvm23-1-prefer-static` or `llvm23-1-force-dynamic`. A comment says static linking is forced if none is set. inkwell also links `libffi` on non-Windows platforms unless `no-libffi-linking` is enabled. The `target-*` features choose which LLVM targets get initializers, and the default `target-all` includes `target-webassembly`. ([Cargo.toml](https://github.com/TheDan64/inkwell/blob/master/Cargo.toml))

**Stability.** The project is pre-1.0 and says it may make breaking changes on master. ([README](https://github.com/TheDan64/inkwell/blob/master/README.md#usage)) Generated docs only show the latest LLVM version. ([README, documentation](https://github.com/TheDan64/inkwell/blob/master/README.md#documentation))

**Maintenance.** The repository has about 3,000 stars and had commits up to 2026-10-20. That includes an update to the new `llvm-sys` release. ([GitHub API](https://api.github.com/repos/TheDan64/inkwell), [commits](https://github.com/TheDan64/inkwell/commits/master))

## Other options

### llvm-sys plus a project-owned C++ shim (the rustc approach)

rustc does not use `llvm-sys` or inkwell. `rustc_llvm` compiles its own C++ files (`PassWrapper.cpp`, `RustWrapper.cpp`, and others) with the `cc` crate. ([rustc_llvm build.rs](https://github.com/rust-lang/rust/blob/main/compiler/rustc_llvm/build.rs)) The wrapper README says the LLVM C++ ABI breaks between versions and must never be exposed to Rust. Only C-style `LLVMXyz` or `LLVMRustXyz` types cross the boundary. ([llvm-wrapper README](https://github.com/rust-lang/rust/blob/main/compiler/rustc_llvm/llvm-wrapper/README)) With this approach a project can reach C++ APIs that the C API lacks, but it must compile and maintain C++ against each LLVM version.

### Emit textual IR and run LLVM tools

A compiler can write `.ll` text and pass it to `clang`, `llc`, or `opt`. The Rust side has no LLVM link dependency. But the text format has no backwards-compatibility promise, so output can break across LLVM versions. ([LLVM Developer Policy](https://llvm.org/docs/DeveloperPolicy.html#ir-backwards-compatibility)) The LLVM tools must then be present wherever the compiler runs.

### Emit bitcode directly (the Zig approach)

Zig writes LLVM bitcode itself and then hands it to LLVM. Roc's rewrite notes say this makes LLVM upgrades "trivial", because bitcode is backwards compatible and the API is not. The cost they cite is about 5% more LLVM codegen time for Zig. In early 2025 Roc wrote that Zig had "the only known implementation of that approach", so a Rust compiler would need to write its own bitcode writer. ([Roc rewrite notes](https://gist.github.com/rtfeldman/77fb430ee57b42f5f2ca973a3992532f)) LLVM is still needed to optimize and generate machine code from the bitcode.

### llvm-ir

`llvm-ir` turns existing LLVM IR into Rust data structures for analysis. Its README says it is meant for reading IR, not producing it. It supports LLVM 9 to 19. ([README](https://github.com/cdisselkoen/llvm-ir/blob/main/README.md)) The last release was 0.11.3 on 2025-02-21. ([crates.io API](https://crates.io/api/v1/crates/llvm-ir)) It does not fit an IR-emitting compiler.

### melior (MLIR)

`melior` wraps the MLIR C API and needs LLVM/MLIR 23. ([README](https://github.com/mlir-rs/melior/blob/main/README.md)) The latest release is 0.28.2 from 2026-09-28. ([crates.io API](https://crates.io/api/v1/crates/melior)) It only matters if Rill lowers through MLIR rather than emitting LLVM IR directly. ADR 0001 says "emit LLVM IR directly". ([ADR 0001](../adr/0001-emit-llvm-ir-directly.md))

## Installing and linking LLVM

### macOS

- Homebrew's `llvm` formula is at 23.1.3. It is keg-only (`provided_by_macos`) and depends on `zstd` and `z3`. ([formula JSON](https://formulae.brew.sh/api/formula/llvm.json))
- Because it is keg-only, its `llvm-config` is not on `PATH`. Roc's old build docs had users add `$(brew --prefix llvm@18)/bin` to `PATH` or set `LLVM_SYS_180_PREFIX=$(brew --prefix llvm@18)`. ([Roc crates/BUILDING_FROM_SOURCE.md](https://github.com/roc-lang/roc/blob/alpha4-rolling/crates/BUILDING_FROM_SOURCE.md))
- The formula builds with `LLVM_TARGETS_TO_BUILD=all` and `LLVM_LINK_LLVM_DYLIB=ON`, and its projects list does not include `lld`. ([llvm.rb](https://github.com/Homebrew/homebrew-core/blob/main/Formula/l/llvm.rb)) `lld` is a separate formula, also at 23.1.3. ([lld formula JSON](https://formulae.brew.sh/api/formula/lld.json))
- Versioned formulas such as `llvm@18` exist. Roc and Inko both told macOS users to install `llvm@18`. ([Roc docs](https://github.com/roc-lang/roc/blob/alpha4-rolling/crates/BUILDING_FROM_SOURCE.md), [Inko installation docs](https://github.com/inko-lang/inko/blob/main/docs/source/setup/installation.md))
- Roc hit `ld: library 'zstd' not found` on Apple Silicon. Its fix was adding `-L/opt/homebrew/lib` to `rustflags` in `.cargo/config.toml`. ([Roc docs](https://github.com/roc-lang/roc/blob/alpha4-rolling/crates/BUILDING_FROM_SOURCE.md))

### Linux

- apt.llvm.org provides Debian and Ubuntu packages and a `llvm.sh <version>` install script. Packages are per version, for example `llvm-22-dev`, `libpolly-22-dev`, and `lld-22`. The site supports the development branch and the last two releases on a best-effort basis. ([apt.llvm.org](https://apt.llvm.org/))
- Inko's Debian and Ubuntu instructions install `llvm-18-dev`, `libpolly-18-dev`, `libclang-common-18-dev`, `zlib1g-dev`, and `libzstd-dev`. Its Fedora instructions add `llvm-static` on top of `llvm-devel`, and Alpine uses `llvm18-static` plus `zlib-static` and `zstd-static`. Inko links statically through inkwell's `llvm18-1-prefer-static`. ([Inko installation docs](https://github.com/inko-lang/inko/blob/main/docs/source/setup/installation.md), [Inko compiler Cargo.toml](https://github.com/inko-lang/inko/blob/main/compiler/Cargo.toml))
- Roc hit missing `-lz` and `-lzstd` errors on Linux and fixed them with `libz-dev libzstd-dev`. ([Roc docs](https://github.com/roc-lang/roc/blob/alpha4-rolling/crates/BUILDING_FROM_SOURCE.md))

### Static versus dynamic and building from source

- With dynamic linking the LLVM shared library must be present at runtime. Static linking puts LLVM in the binary, which makes it larger but removes that runtime dependency. ([llvm-sys README](https://gitlab.com/taricorp/llvm-sys.rs/-/blob/main/README.md#cargo-features))
- Roc wanted a compiler that runs on any Linux distro, including Alpine. That needed bundled LLVM built from source with musl. They said this was easier in Zig, because Zig's own compiler already does it. ([Roc rewrite notes](https://gist.github.com/rtfeldman/77fb430ee57b42f5f2ca973a3992532f)) The new Roc compiler builds against a "roc-bootstrap" bundle that contains LLVM 22, LLD, and Binaryen. ([Roc BUILDING_FROM_SOURCE.md](https://github.com/roc-lang/roc/blob/main/BUILDING_FROM_SOURCE.md))
- rustc's bootstrap has a `llvm.download-ci-llvm` option that downloads a prebuilt LLVM. ([bootstrap.example.toml](https://github.com/rust-lang/rust/blob/main/bootstrap.example.toml)) When it uses an external LLVM instead, it requires version 22 or newer. ([bootstrap llvm.rs](https://github.com/rust-lang/rust/blob/main/src/bootstrap/src/core/build_steps/llvm.rs))

## Guaranteed tail calls (`musttail`)

**What LLVM guarantees.** `tail` is a hint that can be ignored. `musttail` means the call must be tail-call optimized for the program to be correct, even under `"disable-tail-calls"`. It guarantees no unbounded stack growth in recursive cycles. ([LangRef, call instruction](https://llvm.org/docs/LangRef.html#call-instruction))

**Rules for `musttail`.** ([LangRef](https://llvm.org/docs/LangRef.html#call-instruction))
- The call must come right before `ret`, and `ret` must return its value (or `undef`, or void).
- Caller and callee must use the same calling convention, and both or neither must be varargs.
- For conventions other than `tailcc` and `swifttailcc`, prototypes and ABI attributes must match.
- With `tailcc` or `swifttailcc`, prototypes need not match. Only `sret`, `byval`, `swiftself`, and `swiftasync` are allowed.
- The callee may not use anything from the caller's stack frame, such as allocas or pointers to them. A `byval` argument is the one exception.

**`tailcc`.** Under `tailcc`, calls in tail position are always optimized when the platform allows it. It does not support varargs. ([LangRef, calling conventions](https://llvm.org/docs/LangRef.html#calling-conventions)) A call marked `tail` is guaranteed to be optimized when both sides use `fastcc` or `tailcc`, plus `-tailcallopt` or `tailcc`. ([LangRef](https://llvm.org/docs/LangRef.html#call-instruction)) The C API enum has no named constant for `tailcc`. Its number is 18 in `CallingConv.h`. ([Core.h](https://github.com/llvm/llvm-project/blob/main/llvm/include/llvm-c/Core.h), [CallingConv.h](https://github.com/llvm/llvm-project/blob/main/llvm/include/llvm/IR/CallingConv.h))

**C API and bindings.**
- `LLVMSetTailCallKind` and `LLVMGetTailCallKind` were added to the C API in LLVM 18. They set `tail`, `musttail`, or `notail`. ([LLVM 18 release notes](https://releases.llvm.org/18.1.0/docs/ReleaseNotes.html#changes-to-the-c-api), [Core.h](https://github.com/llvm/llvm-project/blob/main/llvm/include/llvm-c/Core.h))
- Before LLVM 18 the C API had only `LLVMSetTailCall(bool)`, which sets `tail` and not `musttail`. ([Core.h](https://github.com/llvm/llvm-project/blob/main/llvm/include/llvm-c/Core.h))
- inkwell exposes `CallSiteValue::set_tail_call_kind` and `get_tail_call_kind` only with `#[llvm_versions(18..)]`. ([call_site_value.rs](https://github.com/TheDan64/inkwell/blob/master/src/values/call_site_value.rs))
- So `musttail` through the C API, `llvm-sys`, or inkwell needs LLVM 18 or newer.

**Backend support.**
- LLVM's code generator docs list tail-call optimization on x86/x86-64, PowerPC, AArch64, and WebAssembly. Each has its own limits, such as no varargs; on x86-64 PIC, only module-local calls. ([CodeGenerator, tail call optimization](https://llvm.org/docs/CodeGenerator.html#tail-call-optimization))
- rustc's explicit tail calls (`become`) lower to `LLVMSetTailCallKind(call, MustTail)`. ([rustc_codegen_llvm builder.rs](https://github.com/rust-lang/rust/blob/main/compiler/rustc_codegen_llvm/src/builder.rs), [become-musttail.rs test](https://github.com/rust-lang/rust/blob/main/tests/codegen-llvm/become-musttail.rs))
- rustc's build test for tail calls with stack-passed arguments passes on these targets: i686, x86-64 (Linux and Windows), ARM, Thumb, AArch64, s390x, SPARC, PowerPC64, BPF, m68k, NVPTX, RISC-V, and Wasm.
- Per the test's notes: RISC-V needs LLVM 23, because LLVM 22 lacked `byval` support there. LoongArch "disabled musttail in LLVM 22, didn't come back in LLVM 23". Wasm needs `+tail-call`. 32-bit PowerPC and AIX are listed as failing. ([bystack.rs test](https://github.com/rust-lang/rust/blob/main/tests/ui/explicit-tail-calls/support/bystack.rs))

## Emitting WebAssembly

**Target setup.**
- The LLVM build must include the WebAssembly target. Homebrew builds all targets. ([llvm.rb](https://github.com/Homebrew/homebrew-core/blob/main/Formula/l/llvm.rb))
- In inkwell, `Target::initialize_webassembly` sits behind the `target-webassembly` feature. Target features are passed as a string to `Target::create_target_machine`. ([targets.rs](https://github.com/TheDan64/inkwell/blob/master/src/targets.rs))
- LLVM 22 renamed the `wasm32-wasi` triple to `wasm32-wasip1`. The old name still works but warns. ([LLVM 22 release notes](https://releases.llvm.org/22.1.0/docs/ReleaseNotes.html#changes-to-the-webassembly-backend))
- LLVM 23 changed reference types to `target("wasm.externref")` and `target("wasm.funcref")`. They used to be pointers in address spaces 10 and 20. ([LLVM 23 release notes](https://releases.llvm.org/23.1.0/docs/ReleaseNotes.html#changes-to-the-webassembly-backend))

**Linking.**
- LLVM emits Wasm object files for `wasm32-unknown-unknown`, and `wasm-ld` (the Wasm port of lld) links them into a module. ([lld WebAssembly docs](https://lld.llvm.org/WebAssembly.html))
- lld can also be embedded by calling the C++ function `lld::lldMain`. ([lld docs](https://lld.llvm.org/)) This is C++, so a Rust compiler would need its own shim to embed it. That is an inference: no lld C API was found.
- `lld` is packaged apart from LLVM: Homebrew formula `lld`, apt.llvm.org package `lld-<N>`. ([lld formula JSON](https://formulae.brew.sh/api/formula/lld.json), [apt.llvm.org](https://apt.llvm.org/))
- The old Roc compiler linked Wasm by running `zig build-exe` with target `wasm32-wasi-musl`. ([Roc link.rs](https://github.com/roc-lang/roc/blob/alpha4-rolling/crates/compiler/build/src/link.rs))

**Tail calls on Wasm.**
- The default `generic` CPU does not enable the `tail-call` feature, on both `main` and `release/23.x`. Only `bleeding-edge` does. ([WebAssembly.td on main](https://github.com/llvm/llvm-project/blob/main/llvm/lib/Target/WebAssembly/WebAssembly.td), [on release/23.x](https://github.com/llvm/llvm-project/blob/release/23.x/llvm/lib/Target/WebAssembly/WebAssembly.td))
- Without `+tail-call`, a `musttail` call fails with "WebAssembly 'tail-call' feature not enabled". With the feature on, `musttail` still fails in these cases ([WebAssemblyISelLowering.cpp](https://github.com/llvm/llvm-project/blob/main/llvm/lib/Target/WebAssembly/WebAssemblyISelLowering.cpp)):
  - varargs calls
  - caller and callee return types that differ
  - arguments that trace back to an `alloca`
  - non-empty `byval` arguments
- Tail calls are part of the Wasm 3.0 standard. They work for direct calls and for calls through references or tables. ([Wasm 3.0 announcement](https://webassembly.org/news/2025-09-17-wasm-3.0/))
- Engine support: Chrome 112, Firefox 121, Safari 18.2, Node.js 20.0, and Wasmtime 22. Wasmtime enables them by default on Cranelift. ([webassembly.org features.json](https://webassembly.org/features.json))

## How Rust-hosted compilers drive LLVM

### Roc (legacy Rust compiler)

- The LLVM backend `roc_gen_llvm` used inkwell. ([gen_llvm Cargo.toml](https://github.com/roc-lang/roc/blob/alpha4-rolling/crates/compiler/gen_llvm/Cargo.toml)) It was pinned to a git revision of inkwell with feature `llvm18-0`. ([Cargo.toml](https://github.com/roc-lang/roc/blob/alpha4-rolling/Cargo.toml))
- It targeted LLVM 18. Install was through Nix (recommended), `brew install llvm@18`, `llvm.sh 18`, or Fedora `llvm18-devel`. ([crates/BUILDING_FROM_SOURCE.md](https://github.com/roc-lang/roc/blob/alpha4-rolling/crates/BUILDING_FROM_SOURCE.md))
- Roc functions used `fastcc` (calling convention 8). ([build.rs](https://github.com/roc-lang/roc/blob/alpha4-rolling/crates/compiler/gen_llvm/src/llvm/build.rs))
- Roc is moving its compiler to Zig. The notes call LLVM upgrades "very time-consuming in large part because of LLVM's breaking API changes". They also say they "actively would prefer" Zig's bitcode generation over inkwell. ([Roc rewrite notes](https://gist.github.com/rtfeldman/77fb430ee57b42f5f2ca973a3992532f))
- The Rust code is gone from `main`. A Roc doc says the new Zig LLVM path uses `std.zig.llvm.Builder`, which serializes to bitcode. ([llvm-codegen-comparison.md](https://github.com/roc-lang/roc/blob/main/docs/llvm-codegen-comparison.md))

### Inko

The Inko compiler is written in Rust. It depends on `inkwell = { version = "^0.8", features = ["llvm18-1", "llvm18-1-prefer-static"] }`. ([compiler/Cargo.toml](https://github.com/inko-lang/inko/blob/main/compiler/Cargo.toml)) Its docs require LLVM 18.1 or newer, and warn that newer versions may not work. ([installation docs](https://github.com/inko-lang/inko/blob/main/docs/source/setup/installation.md))

### rustc

rustc keeps its own bindings and C++ wrapper, as described above. It can build or download its own LLVM, or use an external LLVM 22 or newer. It emits `musttail` for `become`. (Sources in the sections above.)

## Implications for Rill

These are the open choices for the follow-up grilling ticket. Nothing here is decided.

1. **Binding layer.**
   - inkwell: safe, typed API covering LLVM 12 to 23, used by Roc and Inko. But it is pre-1.0 with breaking changes, and Roc found LLVM upgrades through it costly.
   - Raw `llvm-sys`: fewer layers, but every call is `unsafe` and mostly undocumented.
   - `llvm-sys` plus a Rill-owned C++ shim: reaches C++-only APIs, such as embedding lld. The cost is C++ code that must be kept in step with each LLVM version.
   - A Rill-owned bitcode writer: insulates codegen from LLVM API churn and relies on bitcode compatibility. There is no Rust implementation to reuse, and LLVM is still needed for optimization and codegen.
   - Textual `.ll` plus external tools: the simplest start, but there is no compatibility promise, and users need LLVM tools installed.
2. **LLVM version policy.**
   - Option A: pin one major and upgrade on purpose (Roc and Inko stayed on 18).
   - Option B: track the newest release.
   - Hard floors: `musttail` through the C API needs LLVM 18. RISC-V `musttail` with `byval` needs 23. Wasm reference types changed shape in 23.
3. **How developers and users get LLVM.**
   - System packages: Homebrew is keg-only and needs a `LLVM_SYS_*_PREFIX`. apt.llvm.org and Fedora static linking need extra `-dev` and `-static` packages.
   - A Nix shell, as in Roc's old compiler.
   - A prebuilt bundle Rill builds itself, as in the new Roc and rustc.
   - Separately, static or dynamic linking into the shipped `rill` binary.
4. **Tail call strategy.**
   - `musttail` on the default C convention requires matching prototypes. `tailcc` relaxes that but is Rill-internal and cannot be varargs.
   - Some targets lack support (32-bit PowerPC, AIX, LoongArch since LLVM 22).
   - On Wasm, `musttail` needs `+tail-call` and cannot pass pointers to caller allocas. This may interact with how Go-style suspendable functions lay out their frames.
5. **Wasm toolchain.**
   - Which Wasm triple(s) to support: `wasm32-unknown-unknown`, `wasm32-wasip1`.
   - Whether to require Wasm 3.0 tail calls in target engines.
   - How to link: run an external `wasm-ld`, embed lld through a C++ shim, or use another linker driver, as Roc did with Zig.
