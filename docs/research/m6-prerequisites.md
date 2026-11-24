# M6 prerequisites: what the documents already decide

City bead ci-adsj, 2026-11-24. Serves ri-qlh (M6: Execution and tools). This report lists facts and their sources. It makes no recommendation and amends no contract.

## Sources and method

- Documents: `origin/main` at `7332ca88`. Citations are `path:line` at that commit.
- Code, `stdlib/`, `runtime/`, `experiments/`: `origin/implementation/m5-ownership-cleanup` at `dd9ed1d8`, read only. Citations to those paths say "M5".
- `git diff --stat origin/main dd9ed1d8 -- runtime experiments stdlib prototypes docs` shows six files: `docs/agents/compiler-metering.md`, `docs/agents/development.md`, `docs/implementation-plan.md` (the M5 row gains a link), `docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md` (rule R1 for nested routing, a checking rule), `runtime/collection/tests/storage.rs` and `stdlib/routine.rill`. `experiments/` and `prototypes/` are identical on both.
- Beads: read with `gc bd show`. The owner search in section 1 is a title scan of `gc bd list --all` in the rill store (381 lines), not a full-text search.
- Nothing was compiled or run. Section 3 uses two scripts over the prototype sources: an import extractor and a word-match of each imported name against the owning contract. Their limits are stated there.
- "Not found" means the search named in that row found nothing. It does not prove absence from every document.

Three words carry more than one meaning in the documents. This report keeps them apart.

| Word | Meanings found |
| --- | --- |
| provider | (a) The M4.8 sense: a pinned Rill body or implemented compiler/runtime operation that backs a core declaration, such as List storage (`docs/rfcs/0002-modules-and-program-loading.md:428`, `:437`). (b) A native OS source behind a standard operation, such as the entropy provider (`docs/standard-library-contracts.md:766`–`776`). (c) A separately gated native library bridge: OpenSSL for TLS, external SQLite (`docs/adr/0025-explicit-tls-trust-and-private-provider-boundary.md:7`, `docs/adr/0024-external-sqlite-native-provider.md:3`). |
| gate | An acceptance condition named in a document. Most name a bead as its holder. |
| ri-48g | The bead the documents name as holder of production ABI, lowering and runtime acceptance. It is closed (see 1.1). |

## 1. Semantic and ABI prerequisites

### 1.1 What the M6 row says, and who holds the prerequisites

The M6 row reads: "Implement only after each semantic/ABI prerequisite is complete. Native, Wasm and provider support retain separate gates" (`docs/implementation-plan.md:35`). The plan does not list the prerequisites. The table "does not establish implementation readiness or replace the individual tickets" (`docs/implementation-plan.md:37`).

**An enumerated list of M6 prerequisites: not found** in `docs/implementation-plan.md`, any ADR, any RFC or bead ri-qlh.

The documents assign the unlisted work to ri-48g:

- "ri-48g still owns the complete ABI/layout/admission specification, acceptance of generated lowering/checker/runtime/provider implementations, and RFC work" (`docs/adr/0006-whole-program-monomorphization-on-llvm-23.md:34`).
- "ri-48g retains production/formal ABI, lowering and runtime acceptance" (`runtime/README.md:11`).
- "Production/formal backend acceptance remains ri-48g" (`docs/development.md:167`).
- "Language algorithms, runtime mechanisms and production ABI work remain behind their existing gates" (`docs/development.md:45`).

Bead ri-48g ("Write the Rill v0 language-feature RFCs") is CLOSED, "Closed on GitHub (completed) at 2026-11-17". Its deliverable was review-ready RFC drafts. `docs/implementation-plan.md:9` says "Its writing deliverable does not become compiler implementation. Create and claim a separate implementation ticket when coding is authorized."

**An open bead that owns ABI, layout, calling convention, entry point, panic reporting, allocation, LLVM binding or code generation: not found.** The title scan matched only ri-qlh, ri-qlh.1, ri-0w2p, ri-3id, ri-hw5, ri-bgy and closed beads (ri-636, ri-dej, ri-opc, ri-irw, ri-w0c, ri-9y3). ri-0w2p is "Cut acceptance records from M6 on: tests are the evidence, generated row-to-test map" (open; title only, not read). ri-qlh says it "does not authorize implementation or expand M6's scope" and asks to "record the M6 scope and acceptance matrix" (bead ri-qlh description).

RFC status matters for "decided": RFC 0001 is Accepted; RFCs 0002 to 0011 are Draft (`docs/rfcs/README.md:199`–`209`). The Draft RFCs carry policies the owner confirmed individually. "Decided" below means an ADR or a confirmed policy states it. "Open" means a document says the choice is not made. "Unowned" means no open bead holds it.

### 1.2 Prerequisite table

| Prerequisite | Where it is named | Decided | Open | Holder |
| --- | --- | --- | --- | --- |
| String layout | `docs/adr/0011-explicit-numeric-widths-and-utf8-string-views.md:51`, `docs/rfcs/0006-numbers-strings-and-primitive-operations.md:471`–`479`, `:749` | Semantics only. `string` owns valid UTF-8. `&string` is a view "with its own extent", "not uniformly a pointer to an owned buffer header" (ADR 0011:51). Copying a view copies its descriptor, not the backing owner (RFC 0006:477). | "This draft does not select string header layout, allocator, native codec or LLVM lowering library" (RFC 0006:749). | ri-48g by the texts above; closed. Unowned. |
| Record layout | `docs/rfcs/0005-types-records-enums-and-inference.md:199`, `:329`, `:355` | Finite inline layout is required and checked (R0406); references and trusted containers give indirection (RFC 0005:306–329). Field expressions evaluate in written order (RFC 0005:199). Values are "unboxed with fixed layouts" (`docs/adr/0006-whole-program-monomorphization-on-llvm-23.md:3`). | "This RFC does not fix a target field-layout order or ABI" (RFC 0005:199). "Field offsets, tags, niches and calling conventions remain runtime/backend work" (RFC 0005:329). "fixes no target size, alignment, tag or ABI" (RFC 0005:355). | Unowned. |
| Enum layout | `docs/rfcs/0005-types-records-enums-and-inference.md:329`, `:831`, `:861`, `:240` | Same finite-layout rule. | "No fixed pointer size, tag encoding or object ABI follows from this proposal" (RFC 0005:831). "does not promise … stable native enum tags" (RFC 0005:861). The Result representation is not defined by the enum sketch (RFC 0005:240). | Unowned. |
| Error representation (`Err`, `Cause`, `SourceLocation`) | `docs/rfcs/0008-errors-propagation-and-caused-by-wrapping.md:284`, `:525`; `docs/rfcs/0002-modules-and-program-loading.md:433` | A candidate: "compiler-generated typed nodes owning payload, location and next link" (RFC 0008:284). | "This is an implementation sketch, not an accepted ABI" (RFC 0008:284). "`Err`, `Cause` and `SourceLocation` still need owned representation evidence" (RFC 0002:433). | Unowned. M5 has a Rust `runtime/error` crate whose header says "This Rust interface is not a Rill ABI" (M5 `runtime/error/src/lib.rs:1`). |
| Closure and callable representation | `docs/rfcs/0004-functions-overloads-and-generic-requirements.md:901`, `:939`; `docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md:587` | Lowering "must preserve callable modes, move/copy decisions and capture cleanup" (RFC 0004:939). | "does not adopt a native function-pointer ABI, dictionary ABI or runtime registry" (RFC 0004:901). "does not choose a concrete closure layout, allocator protocol, guard layout or pointer-pinning mechanism" (RFC 0007:587). | Unowned. |
| Calling convention | `docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:59`; `docs/adr/0006-whole-program-monomorphization-on-llvm-23.md:3`, `:26`–`:30` | Direction only. Native: "LLVM tailcc/musttail remains an intended native mechanism for eligible Rill transfers, not proof of a working target pipeline" (ADR 0014:59). Wasm: "compiler-owned activation/continuation records plus a nonrecursive dispatcher" for `wasm32-wasip1` (ADR 0006:26). Eligible tail calls must be honored in development and optimized builds or reported as unsupported, R0600 (ADR 0014:37, `docs/diagnostic-codes.md:98`). | The Wasm direction is "not a finished private typed/owned-value ABI" (ADR 0006:34). A calling convention for ordinary (non-tail) calls, argument and result passing: **not found**. | Unowned. |
| Entry point | `docs/rfcs/0002-modules-and-program-loading.md:324`; `docs/adr/0010-file-modules-explicit-exports-and-privacy.md:41`; `docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:9`–`:13`; `docs/standard-library-contracts.md:725` | Semantics. Ordinary execution runs only the entry file's startup statements; imported modules have no initializer (RFC 0002:324). Entry execution runs on a managed fixed 1 MiB region, "not an inherited OS main-thread stack limit" (ADR 0014:9–11). Environment capture happens "once at a controlled runtime point before Rill workers" (stdlib contracts:725). | The native `main` symbol, startup shim, runtime initialization order and how startup hands off to the scheduler: **not found**. | Unowned. |
| Normal exit and exit status | `docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md:84`; bead ri-qlh | Normal top-level completion has no implicit global join and no guarantee of disposal in background routines (ADR 0009:84). ri-qlh asks Hello World to show "exact standard output and zero exit status". | A documented rule that a program whose startup completes exits with status 0: **not found** outside bead ri-qlh. Whether startup-scope cleanup runs before exit: implied by scope-exit cleanup (RFC 0007:297) but **not stated for the startup frame**. M5 MIR has a `FinishStartup` terminator, "End the module's lexical startup frame; there is no callable result" (M5 `crates/compiler/src/analysis/ownership.rs:1166`–`:1167`). | Unowned. |
| Panic behaviour and exit status | `docs/adr/0005-panics-end-the-program-without-unwinding.md:3`, `:7`; `docs/standard-library-contracts.md:420` | A panic cannot be caught, "ends the whole program at once with its message and trace", and nothing unwinds (ADR 0005:3). Causes: out-of-range index, integer division or remainder by zero, integer overflow, `panic(message)` (ADR 0005:3). Compiler-generated failures "use canonical runtime metadata without a lexical panic binding" (stdlib contracts:420). Traces need not reconstruct eliminated tail calls (ADR 0005:7). | The panic exit status, the output stream, and the message and trace format: **not found**. Driver statuses explicitly exclude it: "Raw application exits/signals, OOM or abrupt host termination are not normalized into guaranteed driver statuses" (`docs/diagnostic-contracts.md:65`). | Unowned. |
| Allocation | `docs/standard-library-contracts.md:420`; `docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:47`; `docs/rfcs/0008-errors-propagation-and-caused-by-wrapping.md:355` | Allocation and reservation failure is fatal, not a recoverable Result (stdlib contracts:420). Cleanup worklist storage failure is fatal, with no recursive fallback (ADR 0014:47). No tracing GC and no hidden reference counting (`docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md:597`). | The allocator, its interface to generated code and the "allocator protocol" (RFC 0007:587; RFC 0006:749): not selected. | Unowned. |
| Stack capacity (ADR 0014) | `docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:9`–`:17`, `:23`–`:29`, `:51`–`:55`; `docs/standard-library-contracts.md:254` | Fixed non-moving region per routine, including entry; no copying or segmented stacks (ADR 0014:9). Default usable capacity 1 MiB for entry and spawned routines; guards and metadata are outside it (ADR 0014:11). `--stack-size`, `--entry-stack-size`, and on Wasm `--saved-stack-size`; native rejects the last (ADR 0014:13, :24, :27). `spawnWithStack(work, stackBytes)` (ADR 0014:13). Invalid build settings fail before execution; invalid dynamic requests panic (ADR 0014:17). Overflow must be contained before it corrupts adjacent storage; a callee-only prologue check is insufficient (ADR 0014:51). Overflow reports are bounded and best-effort (ADR 0014:53). | "Defaults are unmeasured design baselines" (ADR 0014:11). Target minimum and maximum, alignment and the emergency reporting mechanism per target are to be documented, not chosen (ADR 0014:15, :53). The policy is "approved, not an implemented runtime or proof" (ADR 0014:3). | ADR 0014:63 names ri-8n7 (interfaces), ri-3t8 (diagnostics) and ri-48g (checking, cleanup, ABI, lowering). Mechanism evidence: ri-dej, closed, see section 6. Production: unowned. |
| Monomorphization (ADR 0006) | `docs/adr/0006-whole-program-monomorphization-on-llvm-23.md:3`, `:20` | Every generic function is monomorphized. v0 builds the whole program, including `std`, from source on every build. Ownership is checked on generic MIR before monomorphization (ADR 0006:3). Witness tables are "a possible later fallback … not for v0" (ADR 0006:20). | A metadata format and module cache are postponed (ADR 0006:3). The instantiation procedure, name mangling and the join from M5 results to monomorphic bodies: **not found**. | Unowned. Related open bead: ri-55r "rigid generic root" (named in `~/.t3/evidence/rill/ci-cqjd/m6-readiness.md`, step 4; not read here). |
| LLVM version | `docs/adr/0006-whole-program-monomorphization-on-llvm-23.md:3`; `docs/adr/0026-toolchain-repository-and-mise.md:9`; `docs/development.md:109`–`:138` | LLVM 23. Reasons: "`musttail` through the C API needs LLVM 18, RISC-V `musttail` with `byval` needs 23, and Wasm reference types changed shape in 23" (ADR 0006:3). The provisioning route pins 23.1.5: upstream archives for Linux x86-64, Linux AArch64 and macOS AArch64, a pinned source build for macOS x86-64; "no implicit Homebrew prefix, driver-only substitute, shared-LLVM fallback or LLVM downgrade" (`docs/development.md:134`). | "Provisioning validation has not passed on these hosts" (`docs/development.md:136`). | ri-x5n holds repository and toolchain setup; it is open, READY-FOR-HUMAN. |
| How codegen binds to LLVM | `docs/adr/0001-emit-llvm-ir-directly.md:3`; `docs/adr/0006-whole-program-monomorphization-on-llvm-23.md:3`, `:21`–`:22`; `docs/development.md:34`, `:37`, `:43`; `docs/agents/development.md:4` | The compiler emits LLVM IR itself and does not generate Rust (ADR 0001:3). "Code generation uses inkwell pinned to LLVM 23 and statically linked into `rill`" (ADR 0006:3). Raw `llvm-sys` rejected (ADR 0006:21). Textual `.ll` plus external LLVM tools rejected: "users would need LLVM installed" (ADR 0006:22). `crates/compiler/` is "independent of LLVM and LSP" (`docs/development.md:34`); backend bindings live in `crates/codegen-llvm/` behind the CLI's `llvm` feature (`docs/development.md:37`, `:43`). "Keep the compiler and IDE analysis independent of native LLVM installation" (`docs/agents/development.md:4`). End users need no Rust installation (ADR 0026:5). | No inkwell dependency exists: M5 `crates/codegen-llvm/Cargo.toml` depends on `compiler` only and `src/lib.rs` is five doc-comment lines. "inkwell/libffi integration … were not run" (`docs/development.md:96`). `test:llvm` is "LLVM C-API compile/link smoke only, not a Rill backend or inkwell integration test" (`docs/development.md:69`). Research records two unverified points: inspected `llvm-sys` defaults to prefer-static and permits a shared fallback, so a static requirement must be explicit; inkwell requests dynamic libffi unless `no-libffi-linking` is set (`docs/research/0046-llvm23-provisioning-options.md:111`, `:113`). The exact inkwell version and feature set: **not found** as a decision (research inspected commit `04bfd21f` exposing `llvm23-1` and `llvm23-1-force-static`, `docs/research/0046-llvm23-provisioning-options.md:109`). | Unowned for the binding itself. |
| Target set | `docs/adr/0001-emit-llvm-ir-directly.md:3`; `docs/rfcs/0002-modules-and-program-loading.md:416`; `docs/tls-native-boundary.md:9` | "Rill must target the same platforms as Rust" (ADR 0001:3). Native Linux and macOS on x86-64 and AArch64 plus `wasm32-wasip1` are the targets named by the library contracts (`docs/tls-native-boundary.md:9`). The first analysis profile is `aarch64-apple-darwin` with an empty option map (RFC 0002:416). | A first execution profile for M6: **not found** in `docs/`. `tests/README.md:25` says "Start with Hello World on an available native profile". | ri-qlh. |
| Installed layout | `docs/adr/0026-toolchain-repository-and-mise.md:5`; `docs/development.md:142`–`:153` | `<prefix>/bin/rill` and `<prefix>/lib/rill/{std,runtime/<target>,toolchain.json}`; the loader derives its root from the executable path (`docs/development.md:153`). | "The scaffold does not implement a release installer or metadata format" (`docs/development.md:153`). What `runtime/<target>/` contains for linking: **not found**. | ri-x5n (open). |

### 1.3 Semantic prerequisites the documents name as missing

- A concurrency and runtime RFC: "Scheduling, transfer/sharing derivation, child/native state machines, protected registry and private ABI. No complete RFC assigned" (`docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md:416`).
- RFC 0001's companions: RFCs 0002 to 0010 are Draft, and "Other companions remain unassigned" (`docs/rfcs/0001-top-level-expectations.md:160`). Its bounded capture procedure "remains authoring work" (`docs/rfcs/0001-top-level-expectations.md:142`, `:342`).
- Driver behaviour: "Complete per-command failure precedence and execution-stop transitions remain driver/runtime authoring work" (`docs/rfcs/0010-diagnostics-and-reporting.md:278`).
- Cleanup ordering: "Complete ordering across nested scopes, temporaries, partial construction and custom-body exits still needs formal specification" (`docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md:311`).
- Delivered core implementations (M4.8 A2): "Native List/iterator storage, owned error state, and protected guard, counted-owner, endpoint and retention actions need matching implementations … Runtime execution, ABI acceptance and release remain separate" (`docs/implementation-plan.md:1033`).

## 2. Separate gates for native, Wasm and provider support

The documents define no single list of gates. The table collects each gate where it is stated.

| Gate | Stated in | What it requires | Holder and state |
| --- | --- | --- | --- |
| Native acceptance (`test:runtime`) | `tests/README.md:11`; `docs/development.md:62`, `:72` | "Runtime, ABI and generated target behavior". The `Test: Native acceptance` workflow "currently fails at `test:runtime` because native acceptance is absent; this is intentional". A manual run also requires `verify:release`. | Task exits 2 as unavailable (`docs/development.md:78`). Unowned beyond ri-qlh. |
| Release (`verify:release`) | `docs/development.md:64`, `:66` | Infrastructure checks, a product build, and the language, standard-library and runtime suites. A distribution claiming a language server must also pass `test:lsp-client`. | Fails at the unavailable product build (`docs/development.md:94`). |
| Prototype targets | `tests/README.md:18`–`:25`; bead ri-qlh | `build:prototypes` compiles intended-valid entry files with the real compiler. `test:prototypes` runs them and checks "expected output, exit status and applicable expectations". Start with Hello World on a native profile. Keep rejections in diagnostic checks. Record unsupported entries and do not count them as passing. Keep the targets separate from the LLVM-free scaffold checks. | ri-qlh, open. Tasks do not exist in M5 `mise.toml`. |
| Prototype performance baselines | `tests/README.md:27`–`:35` | Three timings in native CI (build with fresh outputs, full test command, run of a compiled program), repeated, with samples and median; reporting only until a baseline exists. | ri-qlh, open. |
| Backend mechanism validation (ri-dej) | `docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:61` | "pinned revisions, commands, targets/build modes and reproducible pass/fail/untested evidence before support/implementation-readiness claims". Tests: ABI tails, overflow canaries, independent Wasm budgets and host traps, memory growth and host leases, iterative cleanup with failure injection, cancellation, reclamation and migration. | Standalone mechanism stage complete; bead ri-dej closed (`docs/research/0030-runtime-validation.md:5`). "Partial evidence is not a checker proof; unsupported targets remain gated" (ADR 0014:61). |
| Production backend acceptance | `docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:61`; `docs/adr/0006-whole-program-monomorphization-on-llvm-23.md:34` | "Production ABI/generated-admission/checker/runtime/provider support acceptance". Covers the complete ABI, layout and admission specification and acceptance of generated lowering. | Named holder ri-48g, closed. Unowned. |
| Wasm (`wasm32-wasip1`) | `docs/adr/0006-whole-program-monomorphization-on-llvm-23.md:9`, `:26`–`:34`; `docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:19`–`:31` | The record and dispatcher direction. Support is gated "on pinned reproducible target/build evidence, including ABI cases, probes/outgoing arguments, distinct Wasm addressable/saved/engine resources, memory growth and host borrowers, iterative cleanup and reclamation" (ADR 0006:9). Three separate resources: addressable stack 1 MiB per routine, saved state 1 MiB per routine, host engine stack (ADR 0014:23–25). A retained host pointer must survive `memory.grow` by a demonstrated mechanism (ADR 0014:31). Wasm stays single-threaded (`docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md:3`). "A Wasm backend and `wasm-ld` do not supply a WASI SDK" (`docs/development.md:107`). | Direction approved; "not a completed ABI/runtime or supported backend" (ADR 0014:59). If lowering cannot keep the contract, "return unresolved backend/scope to the map" (ADR 0006:9). Unowned. |
| Wasm library behaviour | `docs/standard-library-contracts.md:436`, `:776`; `docs/progress-io-contracts.md:265`, `:341`; `docs/tls-native-boundary.md:9` | Names and types stay typecheck-visible. Live networking, TCP progress and TLS provider operations return typed `Unsupported`. Random uses `random_get` only under a trusted host configuration. | Design only. |
| Core provider artifacts (M4.8 sense) | `docs/rfcs/0002-modules-and-program-loading.md:428`–`:441`; `docs/implementation-plan.md:1033` | Each provider entry names a pinned source body or "an implemented operation and exact artifact identity". "A placeholder body, bodyless declaration, name, or planned intrinsic cannot supply a provider." "Do not infer native execution or ABI compatibility from static admission." | First profile `aarch64-apple-darwin`. M5 ships the ownership archive for static analysis only (`runtime/README.md:5`–`:7`). |
| Secure random provider | `docs/standard-library-contracts.md:766`–`:786`; `docs/adr/0020-secure-random-byte-generation.md:15` | Linux: `getrandom(..., 0)`, handle short counts, retry EINTR, no insecure fallback. macOS: `SecRandomCopyBytes`. Wasm: `random_get` under a trusted host. Test-only provider fixtures stay separate from production configuration (`docs/prototypes/15-secure-random.md:18`). | Design; no bead found by the title scan. |
| Networking | `docs/networking-contracts.md:3`, `:364`; `docs/adr/0023-tcp-addresses-resolution-and-controlled-networking.md:33` | Live TCP, hostname and interface lookup on native Linux and macOS, "subject to pinned implementation evidence". "No poller/resolver/provider architecture, OS minimum, C ABI … is selected." All real-native rows are NOT RUN. | ri-hw5 "Deliver and validate channel wake, progress runtime and TCP drivers", open. |
| TLS provider | `docs/adr/0025-explicit-tls-trust-and-private-provider-boundary.md:7`; `docs/tls-native-boundary.md:3`, `:9`, `:11` | A private OpenSSL 3.5 bridge. "ri-ei7 must implement and validate bindings, layouts, C shim and native integration." ABI negotiation, byte layouts and marshalling "must be pinned to the delivered compiler/runtime". All native acceptance NOT RUN. | ri-ei7, open. "ri-ei7 gates TLS implementation/support/v0 readiness" (ADR 0025:7). |
| SQLite provider | `docs/adr/0024-external-sqlite-native-provider.md:3`–`:5`, `:16`; `docs/sqlite-provider-contracts.md:11`, `:44`; `docs/sqlite-provider-binding-v1.md:3`, `:23` | External, not `std::sqlite`. "requires a new narrowly approved build/runtime mechanism" that "is absent today". Versioned data-only metadata, generated marshalling, a fixed C shim. The C ABI uses fixed-width scalars and byte spans, "no … Rill record/enum/Err/string/list layouts". Targets: Linux glibc and macOS, x86-64 and AArch64. | ri-3id, open. "No compiler artifact consumer, generated product bindings, C shim, runtime hooks or Rill driver implement this specification." |
| Trusted C bindings | `docs/rfcs/0011-trusted-native-c-bindings.md:20`, `:51`–`:55`, `:153` | A binding package with a native contract, build approval and an ABI descriptor. First cases zlib and SDL2. | Draft RFC. In-scope blockers listed at `:153`. No bead found by the title scan. |
| Language server | `docs/implementation-plan.md:35`; `docs/development.md:66` | ri-bv2 "proceeds where the analysis can support it. It does not gate execution." | ri-bv2, open. |

## 3. The 25 prototype groups

### 3.1 Method and limits

- Imports come from every `.rill` file in a group, including `21-diagnostics/rejections/`. The table cites one import line per group.
- "Specified" means the imported name appears as a whole word in the owning contract: `docs/standard-library-contracts.md`, or the linked contract for `std::time`, `std::cancel`, `std::net`, `std::tls`, `std::http`, `std::sql`, `std::encoding::json` and `std::io::progress`. This is a word match, not a signature check. Counts are imported names, not uses.
- A word match also hits names the contract mentions only to remove them. Section 3.3 lists those.
- "Native need" is taken from the case notes in `docs/prototypes/` where they say what the expectations do and do not exercise. No prototype was loaded or run. The readiness probe found that 24 of 25 groups do not load at `adc3313f` (`~/.t3/evidence/rill/ci-cqjd/m6-readiness.md`, step 2).
- The standard library on M5 has 18 `.rill` files: `bytes`, `channel`, `char`, `collection`, `compare`, `convert`, `core`, `error`, `format`, `iter`, `mutex`, `number`, `option`, `result`, `routine`, `shared`, `std`, `string` (`stdlib/` at `dd9ed1d8`). No `log`, `panic`, `spawn`, `io`, `fs`, `path`, `env`, `random`, `time`, `cancel`, `net`, `tls`, `http`, `sql` or `encoding::json` module exists there.

### 3.2 Groups

Every group also needs what any executing program needs: an entry shim, and for `rill test` the expectation runner. The table does not repeat that.

**A. Pure language: no standard import.**

| Group | Imports | Note |
| --- | --- | --- |
| 21-diagnostics: `valid.rill`, `empty.rill`, `locations.rill`, `warning.rill` | None. | The other files of group 21 are in B. `locations.rill` has one deliberately false expectation and is "not part of the passing fixture" (`docs/prototypes/21-diagnostics.md:15`). |

**B. Standard library, no separately gated native bridge.** "Runtime service" names what the imported operations need from the runtime besides compiled Rill.

| Group | Standard modules imported | Specified | Runtime service the imports need |
| --- | --- | --- | --- |
| 01-basics | `std` (log) (`prototypes/01-basics.rill:3`) | Yes: `std` root exports log (`docs/standard-library-contracts.md:38`); behaviour at `:418`. | Standard output for `log`. |
| 02-caused-by-errors | `std` (log), `error`, `number`, `option`, `result` (`prototypes/02-caused-by-errors.rill:3`–`:7`) | Yes, except `parseNumber` is removed (3.3). | Standard output. Owned error representation. |
| 03-overloads | `collection` (List, join, map) (`prototypes/03-overloads.rill:3`) | Yes. | List storage. |
| 05-records-match | `collection` (List, map, sum) (`prototypes/05-records-match.rill:3`) | Yes. | List storage. |
| 06-mutation | `std` (log), `collection` (List) (`prototypes/06-mutation.rill:1`–`:2`) | Yes. | Standard output. List storage. |
| 07-modules | `std` (log) (`prototypes/07-modules/main.rill:4`) | Yes. | Standard output. Normal startup "prints only `(0, 0)` and `hidden 9`" (`docs/prototypes/07-modules.md:34`). |
| 08-numbers-strings | `bytes`, `char`, `convert`, `error`, `format`, `number`, `result`, `string` (`prototypes/08-numbers-strings.rill:5`–`:12`) | Partly. `codePoint`, `fromCodePoint` are not in the catalogue; `parseNumber`, `decodeUtf8`, `bytes` are removed (3.3). | String and Bytes representation. "Byte APIs require real source representations" (`docs/prototypes/08-numbers-strings.md:127`). |
| 11-conversions | `convert`, `error`, `result` (`prototypes/11-conversions.rill:2`–`:4`) | Yes. | Owned error representation. |
| 12-streaming-io | `std` (panic), `bytes`, `error`, `io`, `option`, `result` (`prototypes/12-streaming-io.rill:3`–`:8`) | Yes (`docs/standard-library-contracts.md:57`, `:438`). | None from the OS: the examples use in-memory fault sources and sinks (`docs/prototypes/12-streaming-io.md:13`, `:36`). Panic reporting. |
| 17-json | `std` (panic), `compare`, `encoding::json`, `error`, `option`, `result`, `string` (`prototypes/17-json.rill:3`–`:9`) | Yes, against `docs/json-contracts.md` (26 names). | None from the OS. Panic reporting. |
| 21-diagnostics: `expect-failures.rill` | `std` (log) (`prototypes/21-diagnostics/expect-failures.rill:3`) | Yes. | Standard output. Three deliberately false expectations (`docs/rfcs/0001-top-level-expectations.md:234`). |
| 21-diagnostics: `rejections/` | `std` (log, spawn, stop), `mutex` (LockGuard) (`prototypes/21-diagnostics/rejections/borrowed-spawn.rill:2`, `protected-overlap.rill:2`) | Yes. | None: intended rejections, not entry files for the build (`docs/prototypes/README.md:5`). |
| 24-progress-io | `bytes`, `channel`, `io::progress`, `option`, `result` (`prototypes/24-progress-io.rill:3`–`:12`) | Yes, against `docs/progress-io-contracts.md` (18 names) and the `std::channel` row (`docs/standard-library-contracts.md:53`). | Channels and wake notifiers. The driver is an application-owned model; no TCP or TLS (`docs/prototypes/24-progress-io.md:7`, `:27`). |
| 25-http | `std` (panic), `bytes`, `collection`, `compare`, `error`, `http`, `option`, `result` (`prototypes/25-http.rill:3`–`:10`) | Yes, against `docs/http-contracts.md` (48 names). | None from the OS: "only provider-free value/plan examples" (`docs/prototypes/25-http.md:5`). Panic reporting. |

**C. Standard library that needs a scheduler** (routines, channels, mutexes, timers). The documents do not call the scheduler a provider. M5 `runtime/README.md:5` says "Goroutine scheduling, cancellation, native I/O release and backend lowering still need separate implementations."

| Group | Standard modules imported | Specified | Runtime service |
| --- | --- | --- | --- |
| 04-concurrency-safety | `std` (spawn, stop), `channel`, `collection`, `convert`, `error`, `mutex`, `option`, `result`, `routine`, `shared`, `time` (sleep) (`prototypes/04-concurrency-safety.rill:5`–`:15`) | Yes (`docs/standard-library-contracts.md:38`, `:51`–`:55`). Uses `all` and `allSettled`, which M6 keeps and ri-bgy later renames (`docs/standard-library-contracts.md:274`). | Scheduler, channels, mutex parking, counted owners, timers. |
| 09-library-contracts | `std` (spawnWithStack, stop), `bytes`, `channel`, `char`, `collection` (with Map), `convert`, `error`, `iter`, `mutex`, `number`, `option`, `result`, `routine`, `shared`, `string` (`prototypes/09-library-contracts.rill:2`–`:16`) | Partly. `codePoint`, `fromCodePoint`, `viewUtf8` are not in the catalogue; `parseNumber` is removed (3.3). The catalogue itself says earlier fixtures with "no-arg channels, copy(value), variadic work calls, plain all, string tags, implicit Error construction or propagation reports" are historical (`docs/standard-library-contracts.md:654`). | Scheduler, sized stacks, channels, mutex, Map. |
| 20-stacks-recursion | `std` (spawnWithStack, stop), `collection` (List), `routine` (Routine) (`prototypes/20-stacks-recursion.rill:6`–`:8`) | Yes. | Scheduler, sized stacks, proper tail calls, build flags `--stack-size` and `--entry-stack-size` (`docs/prototypes/20-stacks-recursion.md:26`). |

**D. Needs a native OS source or bundled data (provider sense b).**

| Group | Standard modules imported | Specified | Native need | What the expectations exercise |
| --- | --- | --- | --- | --- |
| 10-batch-report | `std` (log, panic), `channel`, `collection`, `convert`, `error`, `fs` (readFile, writeFile), `mutex`, `number`, `option`, `result`, `routine`, `string` (`prototypes/10-batch-report/main.rill:2`–`:11`, `records.rill:2`–`:9`) | Partly. `viewUtf8` is not in the catalogue; `parseNumber`, `bytes` are removed (3.3). | Filesystem (whole-file read and write), standard output, scheduler, timer. | The entry reads `north.tsv` and `south.tsv` and writes a report; `expected-report.txt` is the fixture. "native file/UTF-8 work, scoped concurrency, notifications, mutex metrics" (`docs/prototypes/10-batch-report.md:9`). "No command-line argument or process-exit API is assumed" (`:20`). |
| 13-filesystem | `std` (panic), `bytes`, `compare`, `error`, `fs`, `io`, `iter`, `option`, `path`, `result` (`prototypes/13-filesystem.rill:4`–`:16`) | Yes (`docs/standard-library-contracts.md:59`, `:61`, `:517`). | Filesystem for the workflow functions; none for the expectations. | 16 "lexical expectations, plus effectful application workflow functions"; the index calls the workflows "uncalled" (`docs/prototypes/13-filesystem.md:5`, `docs/prototypes/README.md:42`). The probe found the source does not parse (`m6-readiness.md`, step 2). |
| 14-environment-inputs | `std` (panic), `bytes`, `collection`, `compare`, `env`, `error`, `option`, `result`, `routine` (join), `string` (`prototypes/14-environment-inputs.rill:4`–`:14`) | Yes for `std::env` (`docs/standard-library-contracts.md:67`, `:660`–`:735`). `bytes`, `decodeUtf8` are removed (3.3). | Startup argument and environment capture for `rill run`. Scheduler for `join`. | Two expectations call `inputs()` "only in rill test" and require the empty default; the rest use `fromParts` (`docs/prototypes/14-environment-inputs.md:13`). |
| 15-secure-random | `bytes`, `error`, `option`, `random`, `result` (`prototypes/15-secure-random.rill:5`–`:9`) | Yes (`docs/standard-library-contracts.md:68`, `:737`–`:796`). | OS entropy source (section 2). | The expectations "use empty standard calls" and application fakes; empty requests succeed "without consulting/initializing a provider" (`docs/prototypes/15-secure-random.md:5`, `docs/standard-library-contracts.md:762`). Live call shapes are present but not run by expectations (`docs/prototypes/15-secure-random.md:12`). |
| 18-time-values | `std` (panic), `compare`, `option`, `result`, `time`, `time::civil`, `time::fmt`, `time::tz` (`prototypes/18-time-values.rill:2`–`:26`) | Yes, against `docs/time-contracts.md`. Twelve `time::fmt` names and `durationSecondsTrunc` appear there only in slash shorthand such as "parseDate/Time/DateTime/Timestamp/Span/SignedDuration/Offset" (`docs/time-contracts.md:182`, `:84`), so the word match misses them. | A bundled time-zone dataset (`bundledDatabase`). No clock read (`docs/prototypes/18-time-values.md:19`). | 39 expectations. |
| 19-clocks-cancellation | `std` (panic), `cancel`, `channel` (receive), `collection`, `compare`, `error`, `option`, `result`, `time`, `time::fmt` (`prototypes/19-clocks-cancellation.rill:2`–`:16`) | Yes, against `docs/clock-cancellation-contracts.md`; `parseTime` by shorthand as above. | Scheduler, timer registrations, cancellation sources. The source uses manual clocks (`docs/prototypes/19-clocks-cancellation.md:5`); the system clock factory is a host source (`:61`). | 26 expectations. |

**E. Touches a separately gated native bridge (provider sense c).** In each group the expectations stay on the provider-free side of the module.

| Group | Standard modules imported | Specified | Gated bridge | What the expectations exercise |
| --- | --- | --- | --- | --- |
| 16-sql-codecs | `std` (panic), `bytes`, `cancel`, `collection`, `compare`, `convert`, `encoding::json`, `error`, `option`, `result`, `sql`, `sql::driver` (bind, execute), `string`, `time`, `time::civil` (`prototypes/16-sql-codecs/main.rill:11`–`:19`, `control.rill:4`–`:17`, `domain.rill:6`–`:9`) | Yes, against `docs/sql-contracts.md` (24 names). `unixMicrosecondsExact` by shorthand (`docs/time-contracts.md:84`). `bytes`, `decodeUtf8` are removed (3.3). | A concrete SQL driver. The standard interface has none; SQLite is external (ri-3id). | "The domain module imports no concrete driver"; cells and unique errors are synthetic (`docs/prototypes/16-sql-codecs.md:14`, `:17`). Two entries: `main.rill` (20 expectations) and `control.rill` (12) (`:7`). Manual clock only (`:26`). |
| 22-networking | `std` (panic), `cancel`, `collection`, `compare`, `error`, `format`, `hash`, `net`, `option`, `result`, `time` (`prototypes/22-networking.rill:4`–`:24`) | Yes, against `docs/networking-contracts.md` (33 names). | TCP and DNS (ri-hw5). | Address values, parsing and option records. "The manual cases do not exercise controlled native preflight, actual options application, `ControlledTcp`, timer registration, late I/O or dial arbitration" (`docs/prototypes/22-networking.md:28`). Needs `hash`. |
| 23-tls | `std` (panic), `bytes`, `collection`, `compare`, `error`, `hash`, `net`, `option`, `result`, `string`, `time`, `tls` (`prototypes/23-tls/values.rill:4`–`:15`) | Yes, against `docs/tls-contracts.md` (28 names). | OpenSSL bridge (ri-ei7). | 32 value expectations. "The two secure composition helpers are uncalled" (`docs/prototypes/23-tls.md:7`). Code that will not execute must still pass checking (`docs/rfcs/0001-top-level-expectations.md:65`). |

### 3.3 Imported names that the current contract removes or does not list

The catalogue says "Public parseNumber/decodeUtf8 and the replaced core helpers are not retained as compatibility aliases" (`docs/standard-library-contracts.md:393`). RFC 0006 lists the replacements (`docs/rfcs/0006-numbers-strings-and-primitive-operations.md:291`, `:734`–`:739`). The prototype index says "Migrate them to the canonical interface before executable acceptance" (`docs/prototypes/README.md:13`).

| Old import | Replacement | Groups importing it |
| --- | --- | --- |
| `std::number::parseNumber` | `tryInto<N>(&text)` (RFC 0006:734) | 02, 08, 09, 10 |
| `std::string::decodeUtf8` | `tryInto<string>(&bytes)` (RFC 0006:735) | 08, 14, 16 |
| `std::string::viewUtf8` | `tryInto<&string>(&bytes)` (RFC 0006:736) | 09, 10 |
| `std::string::bytes` | Replaced by the matrix (RFC 0006:291) | 08, 10, 14, 16 |
| `std::char::codePoint`, `fromCodePoint` | `into<u32>(value)`, `tryInto<char>(value)` (RFC 0006:738–739) | 08, 09 |

Other contract gaps found by the scripts: none. `std::time::sleep` (group 04) is listed (`docs/standard-library-contracts.md:55`).

## 4. Expectations, `rill test`, and the command contracts

### 4.1 What `expect` and `rill test` must do at run time

RFC 0001 is Accepted (`docs/rfcs/0001-top-level-expectations.md:6`, `:11`). It records that its readiness review is incomplete (`:12`, `:362`).

| Requirement | Source |
| --- | --- |
| `rill test` evaluates expectations instead of ordinary startup statements. Ordinary executables exclude expectations and their test-only code. Both modes check all loaded startup and expectation source. | `docs/rfcs/0001-top-level-expectations.md:16`, `:65`, `:89`–`:91` |
| Discovery covers the entry's loaded graph only, each canonical module once, including loaded standard-library modules. | `:79`–`:81` |
| Order: canonical module identity, then source position. Sequential. | `:83` |
| Test-only imports still count for overload family collection; removing test-only code must not change overload selection. | `:85` |
| No implicit setup, retry, filesystem rollback or per-expectation process isolation. No global background join. | `:91` |
| Per expectation: evaluate once with ordinary order and short-circuiting; complete normal cleanup; record pass for `true` or one failure for `false`; continue. | `:99`–`:102` |
| A false result is not a panic and causes no unwinding, rollback or second evaluation. | `:104` |
| A panic, managed stack overflow or fatal exhaustion ends execution. The runner does not catch it as `false` and promises no final report. Fatal cases run in separate programs or processes. | `:47`, `:106` |
| Front-end rejection means no expectation ran. An empty checked suite reports zero discovered and evaluated tests. | `:108` |
| Success requires complete checking and execution with no failed expectation. | `:110` |
| A result is identified by analysis context, source snapshot, canonical URI and declaration span, not by expression text. | `:114` |
| A failure report gives location, bounded expression text and operand observations labelled by source expression or left/right position. Logical operators distinguish skipped from unavailable operands. A helper exposes only its Boolean result. | `:118` |
| Observation happens at the original evaluation points and must add no evaluation, user `show`/`display`/`copy`/iterator/disposer call, generic requirement, move, retained value, loan, lifetime change or lock. | `:120`–`:130` |
| Preview limits: four levels with the root as level one; eight children per expansion; 4096 rendered UTF-8 bytes shared across an expectation's operands in evaluation order. Capture must bound work before traversal. | `:134`–`:140` |
| Observation states: `captured`, `notEvaluated`, `unavailable`, `truncated`. | `:148` |
| In `rill test`, `std::env` skips host capture and supplies empty inputs. Secure randomness keeps real provider behaviour. | `:255`; `docs/standard-library-contracts.md:729` |
| A `return` or postfix `?` cannot exit an expectation; lowering must preserve this. | `:71`–`:75` |
| Optimization cannot remove an evaluated trap or evaluate a skipped expression. | `:309` |

Open in RFC 0001: the complete bounded capture procedure (`:142`, `:342`) and the status numbers and protocol fields, which "the driver/report RFC must own" (`:110`). That RFC is 0010, Draft (`docs/rfcs/0010-diagnostics-and-reporting.md:6`).

M5 deferred one execution observation to M6: ri-qlh.1, "Done when an M6 execution test runs O15 and observes counter() evaluating to 12 with the expectation passing" (bead ri-qlh.1).

### 4.2 Command contracts

The driver status table is in the diagnostic contract (ri-3t8 design input) and in Draft RFC 0010. The owner confirmed RFC 0010's occurrence grouping, summary accounting and safe report-file opening; "Source/driver/sink procedures still need work" (`docs/rfcs/README.md:27`).

| Status | Meaning | Source |
| --- | --- | --- |
| 0 | The requested check, build or test operation completed successfully within its stated scope. | `docs/diagnostic-contracts.md:59`; `docs/rfcs/0010-diagnostics-and-reporting.md:350` |
| 1 | A valid request failed: source, build, false expectation or environmental operation failure. Report I/O failure is R0604, status 1. | `docs/diagnostic-contracts.md:60`; `docs/rfcs/0010-diagnostics-and-reporting.md:338` |
| 2 | Invalid invocation or configuration, including report destination mode (R0603). | `docs/diagnostic-contracts.md:61`; `docs/rfcs/0010-diagnostics-and-reporting.md:338` |
| 3 | Controlled internal tool failure. | `docs/diagnostic-contracts.md:62` |
| 130 | Interrupted under the host convention. | `docs/diagnostic-contracts.md:63` |

| Command | Does | Output | Exit | Source |
| --- | --- | --- | --- | --- |
| `rill check <entry>` | Shared front-end checks only. "produces no executable and performs no LLVM generation, startup or expectation execution". | Human reports on stderr. JSONL defaults to stdout. | 0 means the analysis phase passed, "not linking, backend support or soundness". A warning-only run can exit 0 (case D10). | `docs/diagnostic-contracts.md:33`, `:43`, `:48`; `docs/rfcs/0010-diagnostics-and-reporting.md:272`, `:401` |
| `rill build` | "performs its later stages; report scope must state which checks ran". Accepts `--stack-size`, `--entry-stack-size`; native rejects `--saved-stack-size`. Invalid build settings fail before execution (R0601). | Human on stderr. JSONL defaults to stdout. Output file name and location: **not found**. | Driver table. | `docs/diagnostic-contracts.md:43`, `:48`; `docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:13`, `:17`, `:27`; `docs/diagnostic-codes.md:101` |
| `rill run` | Builds, then runs the entry's startup. "Run/test expose equivalent compilation options". | Application stdout and stderr stay raw. JSONL needs an explicit non-stdout sink that does not alias application stdout or stderr. The structured scope "ends at its build/driver phase"; "A successful build summary is not application success." | The application's exit is "not overwritten by the earlier summary or converted into a compiler diagnostic" (case D22). Raw application exits are not normalized into driver statuses. The status `rill run` returns when the program exits normally, exits nonzero or panics: **not found** as a stated rule. | `docs/diagnostic-contracts.md:48`, `:53`, `:65`; `docs/rfcs/0010-diagnostics-and-reporting.md:274`, `:334`, `:425`; `docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:13` |
| `rill test` | Checks the canonical loaded graph first; front-end rejection means no execution; then runs expectations as in 4.1. | Human test reports on stderr; default limit 20 primary reports. JSONL needs an explicit non-stdout sink. Records: `diagnostic`, `expectationResult`, `toolFailure`, `summary`. | 0 for a completed run with no failure; 1 for a false expectation. A fatal termination has no guaranteed report or status mapping. | `docs/diagnostic-contracts.md:33`, `:35`, `:48`, `:53`, `:60`, `:69`, `:75`–`:79`; `docs/rfcs/0010-diagnostics-and-reporting.md:276`, `:356`, `:364` |
| `rill explain <code>` | Offline documented meaning. Unknown codes produce a driver error. | Not specified further. | Driver table. | `docs/diagnostic-contracts.md:39` |
| `rill lsp` | Planned; stdio JSON-RPC on stdout, logs on stderr. | — | — | `docs/lsp-contracts.md:13` |

Reporting options for all four product commands: `--report-format human|jsonl`, `--report-file <path|->`, `--color auto|always|never`, `--max-diagnostics <positive-count>` (`docs/diagnostic-contracts.md:47`–`:49`).

Program output facts that bear on Hello World: `log` "writes text plus newline, returns void, synchronizes whole messages … Failure is fatal" (`docs/standard-library-contracts.md:418`). Which stream `log` writes to: the text says "same stream synchronization as output operations" and does not name stdout or stderr. **The stream for `log`: not found.** Bead ri-qlh expects Hello World's "exact standard output".

Current state at M5: `build`, `check`, `run` and `test` print "unavailable" and exit 2 (M5 `crates/cli/src/main.rs:61`–`:80`; `docs/development.md:3`). RFC 0001 says "A successful empty suite cannot replace that response" (`docs/rfcs/0001-top-level-expectations.md:277`).

## 5. Cleanup and cancellation at run time

RFC 0007 is Draft; its cleanup-order, partial-move and explicit-dispose policies are confirmed (`docs/rfcs/README.md:19`). ADR 0009 is "approved design direction, not an implemented checker/runtime" (`docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md:5`). ADR 0022 is confirmed design input (`docs/adr/0022-jiff-time-and-explicit-cancellation.md:3`).

### 5.1 What M5 checks, for contrast

M5 produces per-root ownership MIR with cleanup operations (`Cleanup`, `Dispose`, `Drop`, `Install`, `ReleaseGuard`, `InvokeDisposer`, `DeferDisposerFields`, `ScopedJoin`) and static cancellation proofs (`Cancellation`, `ReplacementCancellation`, `DeferredDisposerCheckpoint`) (M5 `crates/compiler/src/analysis/ownership.rs:461`–`:1126`, `:130`–`:298`). The graph module opens "Ordinary and bounded delivered-exit judgments, not an execution plan", and three graph obligations stay open: `IntrinsicCancellation`, `MaskingAndJoin`, `EmbeddedAndUnreachedSource` (M5 `crates/compiler/src/analysis/ownership/graph.rs:1`, `:19`–`:25`). Nothing in M5 executes. M5 deferred run-time enforcement of native and child completion to M6 as ri-qlh.2.

### 5.2 What the runtime must do

**Cleanup (RFC 0007).**

1. Run cleanup at the owning scope boundary; explicit `dispose` and ownership transfer end a value earlier. Do not advance user disposers or protected actions to last use; do not unlock a guard early (`docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md:297`–`:299`).
2. Order within one boundary: dependents before their sources; reverse declaration order breaks ties; remaining record fields in reverse declaration order; tuple and List elements in reverse position; statement temporaries in reverse successful creation order (`:301`–`:303`).
3. A custom outer disposer runs before remaining-field cleanup; fields it moved or disposed are not disposed again; cleanup does not re-enter the outer disposer (`:305`, `:289`, `:621`).
4. Partial construction cleans only initialized fields and never calls the disposer of an incomplete value (`:283`, `:303`).
5. Track initialized, moved and disposed state per place so each value is disposed exactly once (`:253`, `:257`, `:615`).
6. Standard and default cleanup bound their call-stack use with iterative worklists; a worklist allocation failure is fatal; no recursive fallback and no partially disposed recoverable state (`:325`; `docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:45`–`:47`).
7. A pending disposal, guard release, child join or native join makes a final-looking call an ordinary call; no early release to obtain a tail call (`docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:35`).

**Replacement (RFC 0007 `:242`–`:249`; ADR 0009 `:71`–`:78`).** Six phases: evaluate destination and replacement, cancellable; keep the completed replacement owned; defer cancellation delivery when old-value disposal begins; dispose the old value if the destination still owns it; install and record the new state; deliver pending cancellation afterwards against the installed value, which normal cleanup then disposes exactly once.

**Cancellation (ADR 0009).**

1. A request is not delivery. Delivery is deferred during disposal and required cleanup joins; suspension, scheduling and normal completion continue; the request stays pending; a masked cleanup wait stays registered (`docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md:69`).
2. Cancellation points are budget-check yields and suspending or waiting operations (`:33`).
3. Cancelling an ordinary wait uses synchronized bookkeeping only while delivery is enabled. Withdraw an uncommitted mutex, channel, timer or poller registration. If lock acquisition wins the race, its release stays owned through guard initialization or cleanup. A committed channel transfer is not rolled back; a committed receive result has exactly one owner (`:80`).
4. `select` has one winning commit and withdraws the rest (`:80`).
5. `stop(&routine)` requests cancellation and joins termination plus cleanup; the join is cancellation-deferred; repeated and concurrent calls share termination state and cleanup runs once; self-stop panics; dropping the handle does not stop the routine (`:65`).
6. Scoped helpers (`all`, `allSettled`, `race`, `any`) select a result, cancel the remaining children, join their cleanup, then return. Cancelling the helper cancels and joins children under deferred delivery, keeping borrowed sources and the selected result valid (`:63`).
7. Native blocking work must stop touching borrowed buffers, state and handles before cleanup invalidates them: wait for completion or confirmed abortion. An abort request or a move to a blocking pool is not completion (`:82`; `docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md:321`).
8. Stacks and saved state are retained until execution, required cleanup and retained child and native borrowers complete or confirm release (`docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md:88`; `docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:55`).
9. Guard disposal unlocks exactly once and does not dispose the payload; a guard may be held across suspension and worker migration; reacquisition by the holder panics (`docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md:53`–`:55`).
10. Normal top-level completion performs no global join. Panic ends the program without cleanup. Cancellation cleanup "is a separate mechanism, not catchable panic or panic unwinding" (`:84`).
11. Cleanup can wait forever; there is no force kill and no bounded stop latency (`:69`, `:65`).

**Explicit tokens (ADR 0022; clock and cancellation contract).**

1. A token request is separate from intrinsic routine cancellation. Tokens do not attach to routines; there is no ambient token (`docs/clock-cancellation-contracts.md:164`; `docs/adr/0022-jiff-time-and-explicit-cancellation.md:12`).
2. Masking defers intrinsic delivery, not token observation: polling and signal waits stay truthful during cleanup (`docs/clock-cancellation-contracts.md:168`).
3. A token-aware operation returns its typed cancellation outcome "only after unregister/confirmed-abort/native-source release" (`docs/clock-cancellation-contracts.md:168`, `:97`).
4. Intrinsic cancellation "is never converted to WaitError" and is not a catchable panic (`docs/clock-cancellation-contracts.md:168`).
5. `cancel` publishes the request and completes descendant propagation before it returns; delivery cannot abandon a half-published propagation (`docs/clock-cancellation-contracts.md:158`).
6. Ordinary disposal of a source or token releases the handle only; it does not cancel or join. The explicit cancel-on-disposal guard can request, never join (`docs/clock-cancellation-contracts.md:146`; `docs/adr/0022-jiff-time-and-explicit-cancellation.md:12`).
7. `withDeadline` selects, requests cancellation of the losing scoped work, joins all execution, cleanup and native release, then returns; it promises no hard return bound (`docs/clock-cancellation-contracts.md:120`; `docs/adr/0022-jiff-time-and-explicit-cancellation.md:14`).

The documents list these as "obligations for later executable tests" with seventeen cases (`docs/prototypes/04-concurrency-safety.md:77`–`:99`). RFC 0007 says "Cleanup lowering needs explicit pending/disposal/installed states, dependency ordering and cancellation deferral" (`docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md:621`). A complete state machine: "Detailed rebuilding/partial-initialization cancellation transitions remain necessary formal work" (`:259`); "No CFG or runtime state machine" (`:535`). **A written routine state machine: not found.**

### 5.3 States and events for a statechart of one routine

Derived from the rules above. Each row cites the rule that needs it. The documents do not name these states; the names here are labels.

Routine lifecycle states (one region):

| State | Needed by |
| --- | --- |
| Created (callable owned, stack reserved, not started) | `spawnWithStack` chooses capacity "before it starts" (`docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md:13`); reservation failure is fatal (`:17`) |
| Running | ADR 0009:33 |
| Suspended at a wait (registered, uncommitted) | ADR 0009:80 |
| Suspended at a wait (committed, result owned) | ADR 0009:80 |
| Waiting in a required join (child, `stop` target or native completion) | ADR 0009:63, :65, :82 |
| Cleaning up after delivered cancellation | ADR 0009:69, :84 |
| Finished, stack retained for borrowers | ADR 0014:55 |
| Finished, stack released, handle retains status | ADR 0014:55 |
| Terminated by fatal panic (whole program) | ADR 0005:3 |

Cancellation request (orthogonal region): `NotRequested`, `Requested` (pending), `Delivered`. A request never clears (`docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md:69`).

Delivery mask (orthogonal region): `Enabled`, `Deferred`. The mask is entered at the start of disposal, at old-value disposal in a replacement and at a required cleanup join, and left after installation or the end of the cleanup (`docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md:69`, `:74`). Masks nest: a join inside a disposer is still deferred. A depth or equivalent is implied; **the nesting rule is not written**.

Per owned place (one instance per place; the routine's cleanup is the product of these): `Uninitialized`, `Initialized`, `PartiallyMoved` (ordinary records only), `Moved`, `Disposing` (outer disposer running), `RemainingFieldCleanup`, `Disposed`. Replacement adds `OldDisposing` and `Installed` (`docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md:244`–`:249`, `:283`–`:289`, `:305`).

Per guard: `Acquiring` (registered), `Granted` (release owned, guard not yet initialized), `Held`, `Released` (`docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md:80`, `:53`).

Per native operation: `Registered`, `AbortRequested`, `Completed`, `AbortConfirmed`. Sources may be released only from the last two (`docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md:82`).

Events:

| Event | Source |
| --- | --- |
| start | ADR 0014:13 |
| yield at a budget check | ADR 0009:33 |
| suspend at a wait; register | ADR 0009:80 |
| wait commits (lock granted, message transferred, timer fired, select winner chosen) | ADR 0009:80 |
| registration withdrawn | ADR 0009:80 |
| cancellation requested (by `stop`, by a scoped helper, by `withDeadline`) | ADR 0009:63, :65; `docs/clock-cancellation-contracts.md:120` |
| cancellation delivered (only when the mask is `Enabled`, at a cancellation point) | ADR 0009:69, :33 |
| mask entered; mask left with a pending request (delivers after installation or cleanup) | ADR 0009:74–76; RFC 0007:246–249 |
| scope exit; explicit `dispose`; move out | RFC 0007:297 |
| disposer starts; disposer returns; remaining-field cleanup done | RFC 0007:305 |
| replacement: evaluation done; old disposal starts; installed | RFC 0007:244–249 |
| child completes; child cleanup joined | ADR 0009:63 |
| native operation completes; abort confirmed | ADR 0009:82 |
| last borrower releases the stack | ADR 0014:55 |
| token requested; token observed (does not change the routine's cancellation region) | `docs/clock-cancellation-contracts.md:168` |
| normal return of the routine's callable | ADR 0009:84 |
| top-level startup completes (program exits; this routine is not joined) | ADR 0009:84 |
| panic, managed stack overflow or fatal allocation failure (program ends; no transition to cleanup) | ADR 0005:3; ADR 0014:29, :47 |

Outcomes the documents leave unspecified: the winner among simultaneously ready outcomes (`docs/standard-library-contracts.md:268`; `docs/clock-cancellation-contracts.md:97`), concurrent `log` message order (`docs/standard-library-contracts.md:418`) and any latency bound.

## 6. Reusable material in `experiments/` and `runtime/`

Recorded status is quoted from each artifact. None of it is Rill execution: "Standalone/native/model execution is not Rill compiler execution" (`docs/research/README.md:36`).

### 6.1 `experiments/` (identical on main and M5)

| Artifact | What it is | Recorded status |
| --- | --- | --- |
| `experiments/backend-30/run.py`, `native.c`, `wasmtime.c`, `asyncify.mjs`, `cleanup.c`, `evidence/` | The original backend baseline: LLVM IR generation, verification, O0 and O2 codegen, link and execute; a 1 MiB guarded pthread stack sampler; a Wasmtime async-host candidate; Asyncify controls; an ordered cleanup worklist model. | "not Rill programs, a compiler, a runtime implementation, or accepted target support" (`experiments/backend-30/README.md:3`). Exit 0 "does not mean all contracts pass"; it includes expected contract failures for Asyncify saved-buffer and leaf red-zone containment (`:38`, `:42`). Recorded host macOS AArch64 (`:11`). Needs Homebrew LLVM 23.1.5 metadata (`docs/development.md:107`). |
| `experiments/backend-30/native-capacity/` (`abi.c`, `capacity.c`, `host.c`, `probe.h`, `switch.S`) | Native pre-write stack admission and a packet tail-ABI candidate. | "executed standalone C/LLVM23 mechanisms … not a Rill compiler/runtime or target support approval"; "Native `tailcc`/`musttail` remains the direction. No public ABI/architecture adoption is implied" (`native-capacity/README.md:3`–`:7`). 217 checks, 136 outcome inventory (`docs/research/0030-runtime-validation.md:47`). |
| `experiments/backend-30/native-abi/` (`mechanism.cpp`) | Mixed-layout owned tail transfers, `musttail` chains of one million transfers. | "not a production ABI, checker, compiler, architecture adoption or production support closure" (`native-abi/README.md:3`). 123 checks, 24 chains (`docs/research/0030-runtime-validation.md:48`). |
| `experiments/backend-30/native-lifecycle/` (`lifecycle.cpp`, `source-pins.json`) | Fixed-stack M:N migration, recursive admission, retained native borrowers and scoped-child cleanup. Context switch uses pinned Boost.Context 1.92.0 make/jump assembly. | "not a complete Rill scheduler/checker or selection of a production Boost dependency". LLVM/Clang 23.1.5 at O0 and O2 for four targets; macOS x86-64 through Rosetta, Linux x86-64 emulated; "Physical x86-64 hardware is not tested" (`native-lifecycle/README.md:3`). Boost is not adopted (`docs/research/0030-runtime-validation.md:7`, `:30`). |
| `experiments/backend-30/portable/` (`machine.c`, `host.c`, `retcon.py`, `check.mjs`) | A hand-lowered Wasm machine with explicit activation records, a dispatcher, suspension, separate saved and addressable budgets, and guest teardown with outstanding foreign work. | "Throwaway mechanism probes"; "not adoption of a Rill architecture, automated compiler/checker output, a complete owned-value ABI" (`portable/README.md:1`, `:7`). Header and packet sizes and the 64 KiB reservation "are experimental constants, not approved final policy" (`docs/adr/0006-whole-program-monomorphization-on-llvm-23.md:34`). |
| `experiments/sqlite-provider/` (`contract.h`, `protocol_model.py`, `test_protocol.py`, `reference/`) | A Python model of the envelope, token and value encodings; a declaration-only C header; a pinned SQLite 3.54.1 C reference fixture. | "Design confirmed; #43 OPEN for implementation/integration validation" (`experiments/sqlite-provider/README.md:3`). 98 native checks and 32 unittest methods passed on macOS arm64; other native cells and WASI NOT RUN (`:10`–`:14`). "this directory adds zero Rill-executed expectations" (`:16`). The reference runner is Darwin-only (`:26`). |
| `docs/research/0045-progress-reference/` (`reference.c`, `run_reference.py`, `results.md`) | An in-memory OpenSSL 3.5.9 BIO-pair reference for TLS progress. Lives under `docs/research/`, not `experiments/`. | "a throwaway native reference experiment, not production Rill, a provider-delivery proof, an API freeze" (`docs/research/0045-progress-reference/README.md:3`). 13 of 13 cases on one macOS arm64 host (`docs/tls-native-boundary.md:92`). |

The combined disposition: "ri-dej's standalone mechanism-validation work is complete … this report does not establish that the compiler/backend is implementation-ready"; it "does not implement Rill, prove the borrow checker, adopt Boost as a runtime dependency, approve a production private ABI" (`docs/research/0030-runtime-validation.md:5`, `:7`).

Mise entry points exist for two reproductions: `experiment:backend` and `experiment:sqlite` (M5 `mise.toml:112`–`:119`).

### 6.2 LLVM tooling

| Artifact | What it is | Recorded status |
| --- | --- | --- |
| `tools/llvm/setup.py`, `check.py`, `common.py`, `test_llvm.py`; tasks `plan:llvm`, `setup:llvm`, `test:llvm` | Opt-in provisioning of a pinned LLVM 23.1.5 prefix and a C-API static-link smoke. | "Provisioning validation has not passed on these hosts"; the smoke "does not establish the eventual Rust binding integration, complete backend targets, required native or Wasm lowering, or any Rill execution" (`docs/development.md:136`). On the recorded scaffold run `test:llvm` exited 1 for an absent prefix; "no smoke passed" (`docs/development.md:94`). Needs 48 GiB free disk for bundles or 80 GiB for a source build (`docs/development.md:126`). |
| `docs/research/rust-llvm-bindings.md`, `docs/research/0046-llvm23-provisioning-options.md` | Primary-source notes on `llvm-sys`, inkwell, static linking and provisioning routes. | "unapproved interview input, not an installation contract" (`docs/research/0046-llvm23-provisioning-options.md:3`). Lists a per-host verification sequence that was not run (`:117`–`:119`). |
| `crates/codegen-llvm/` | The backend crate. | "Implementation is absent … no lowering or code generation APIs and has no LLVM bindings or native dependencies" (M5 `crates/codegen-llvm/src/lib.rs:1`–`:5`). |

### 6.3 `runtime/` at M5

| Artifact | What it is | Recorded status |
| --- | --- | --- |
| `runtime/ownership/` (`shared.rs`, `mutex.rs`, `channel.rs`, `bindings.rs`, `tests.rs`) | Counted-owner retain and release, lock guard release, channel endpoint retain and release, as Rust. Built by `crates/compiler/build.rs` into `librill_ownership.rlib` for `aarch64-apple-darwin`. | "Native ownership primitives. Rust interfaces only; no Rill ABI" (M5 `runtime/ownership/lib.rs:1`). "These are Rust items, not Rill ABI symbols" (M5 `runtime/ownership/bindings.rs:2`). The installed loader "verifies the bytes; it does not load or run Rust code from the installed archive" (`runtime/README.md:7`). No scheduler, cancellation or native I/O (`runtime/README.md:5`). |
| `runtime/collection/` (`src/lib.rs`, `src/link.rs`, `tests/storage.rs`) | `List<T>` over `Vec<T>` with owned, shared and exclusive iterators; typed provider and representation anchors. | "Rust backing for first-party List operations; no Rill ABI" (M5 `runtime/collection/src/lib.rs:1`). M5 adds 34 lines to `tests/storage.rs` over main. |
| `runtime/error/` (`src/lib.rs`, `tests/storage.rs`) | `Error`, `SourceLocation`, `OwnedErr<E>`, `Cause`, `wrap_result`. | "Owned error storage for the first-party runtime. This Rust interface is not a Rill ABI" (M5 `runtime/error/src/lib.rs:1`). |
| `tools/stage_typed_installation.py` | Stages `bin/rill`, the runner, standard sources and the ownership archive into a prefix. | "Neither check runs Rill or validates an ABI" (`docs/development.md:165`). macOS AArch64 only. |

`runtime/README.md:3`: "No Rill lowering or production ABI connects these inputs to Rill programs yet." An entry shim, an output function, a string or allocation interface and a scheduler: **not found** in `runtime/` at M5 (file list: fourteen files under `runtime/`: those named above, `README.md` and two `Cargo.toml` files).

## 7. Not-found list

Each item is a search that returned nothing in the places named. Section references give the context.

1. An enumerated list of M6 semantic and ABI prerequisites (1.1).
2. An open bead that owns ABI, layout, calling convention, entry point, panic reporting, allocation, LLVM binding or code generation (1.1).
3. String header, record field, enum tag, error node and closure layouts (1.2).
4. A calling convention for ordinary calls (1.2).
5. The native entry symbol, startup shim and runtime initialization order (1.2).
6. A documented rule that normal completion exits with status 0; a statement that startup-scope cleanup runs before exit (1.2).
7. Panic exit status, output stream and message and trace format (1.2).
8. The allocator and its interface (1.2).
9. The monomorphization procedure and symbol naming (1.2).
10. A decided inkwell version and feature set; any inkwell dependency in the workspace (1.2).
11. The first execution profile for M6 in `docs/` (1.2).
12. The contents of `lib/rill/runtime/<target>/` for linking (1.2).
13. `build:prototypes` and `test:prototypes` tasks in M5 `mise.toml` (2).
14. A bead for the secure random provider or for RFC 0011 (2).
15. The `rill build` output file name and location (4.2).
16. The status `rill run` returns for a normal, nonzero or panicking program (4.2).
17. The stream `log` writes to (4.2).
18. A written routine state machine; the nesting rule for the delivery mask (5.2, 5.3).
19. An entry shim, output function, string or allocation interface or scheduler in `runtime/` (6.3).
