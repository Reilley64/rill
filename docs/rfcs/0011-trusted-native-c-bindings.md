# RFC 0011: Trusted native C bindings

| Field | Value |
| --- | --- |
| ID | 0011 |
| Status | Draft |
| Owner | Rill project owner |
| Source inputs | [M6 gate #77](https://github.com/Reilley64/rill/issues/77), [C interop survey](../research/c-interop-language-survey.md) (SHA-256 `ac0739b8233fda8fb36be4e640bb7a0841e8d74fd22575f08ae8933d99984f63`), [ADR 0024](../adr/0024-external-sqlite-native-provider.md), [ADR 0025](../adr/0025-explicit-tls-trust-and-private-provider-boundary.md), [ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md), [zlib manual](https://zlib.net/manual.html), [SDL2 thread rules](https://wiki.libsdl.org/SDL2/SDL_RenderPresent) |
| RFC dependencies | [RFC 0002: canonical identity and metadata](0002-modules-and-program-loading.md#family-collection-and-metadata-boundaries), [RFC 0007: dependencies](0007-ownership-borrowing-mutation-and-cleanup.md#layered-dependency-model) and [native completion](0007-ownership-borrowing-mutation-and-cleanup.md#protected-actions-and-asynchronous-completion), [RFC 0008: error wrappers](0008-errors-propagation-and-caused-by-wrapping.md#wrappers-payloads-and-access). Review the shared rules against final M6 revisions before accepting this RFC. |
| Replaces / superseded by | None. The SQLite and TLS provider contracts retain their separate authority. |
| Decision | Not yet decided |
| Reviewed revision | Not yet reviewed |

## Summary and scope

A Rill executable can call C libraries through a trusted binding package. The package exposes ordinary Rill operations and links a small C adapter to selected native libraries. One compiler and runtime mechanism can admit more than one package. Each build must explicitly approve the packages and native artifacts it uses.

The first two cases are zlib decompression and SDL2 window input and presentation. They exercise bounded byte buffers, native resource ownership and main-thread operations. Rill owns the application's state and logic. A C adapter can handle platform setup and call into the private runtime.

This draft does not add public pointers, C declarations in Rill source, runtime library loading or public C exports. It does not add C++ ABI support, arbitrary callbacks, general C header import or OS-thread pinning. It does not standardize SDL2 or zlib APIs.

## Motivation

A Rill application should be able to reuse a C library for decompression or window input. It should not need to move its logic to C or reimplement those services. A native import path lets it call libraries through a small adapter.

Rill already has narrow SQLite and TLS provider designs. They do not authorize third-party native bindings. Adding a new compiler exception for every library would make reuse difficult. A common admission mechanism can keep package-specific trust decisions separate from the compiler.

## Guide-level explanation

A binding author provides Rill declarations, a C adapter and a description of the native contract. The application build selects the package and explicitly admits its artifacts. Application source imports the package like any other Rill module. The package name below is illustrative, not a standard-library namespace.

Example excerpt, not checked. Assume the package supplies the binding in the [zlib case](#first-slice-zlib).

```rill
import compression::{InflateError, inflateZlib};
import std::{Result};
import std::bytes::Bytes;

function decodeBytes(input: Bytes): Result<Bytes, InflateError> {
  let output = inflateZlib(&input, 1048576u64)?;
  return Result.Ok(output);
}
```

The library returns owned bytes or a typed error. No C address is visible to the caller. An SDL2 binding can return an opaque window owner instead. Rill decides what to draw; the adapter executes the video operations on the required platform thread.

## Syntax and reference semantics

### Build admission and identity

A binding package is a Rill module set and its native contract. The contract names a package identity, target profile, bridge ABI revision and exact native inputs. It lists the canonical Rill declarations, the fixed C entry points and their signatures. It specifies parameter access, result ownership, completion and required release actions. It specifies the executor for each entry point. The serialization and build command remain open.

Authority comes from explicit build approval of the contract and its declared native artifacts. Importing a module does not approve a package. Dependency declarations do not grant approval to other packages or additional native artifacts. Names, paths, binding data and matching signatures do not confer trust by themselves.

Bind each entry to its canonical declaration in the captured program. An alias preserves that identity. A same-spelled declaration cannot acquire it. Reject a missing approval, duplicate binding, unsupported profile, changed input or signature mismatch before producing an executable that can use the binding. Do not search arbitrary libraries for a replacement symbol. Check the C adapter against the selected library headers. The build must resolve the approved entry points to the selected artifacts. A successful link alone does not prove the ABI or the ownership contract.

This draft proposes fixed, build-linked adapters on one native profile first. It does not specify a general package manager. It does not grant new access to existing SQLite, TLS or protected toolchain operations.

### Boundary values and ownership

The first bridge uses fixed-width integer values, call-scoped byte buffers and opaque resource identities. The C adapter handles platform-dependent widths, layouts, macros and library-specific structs on its side. A Rill aggregate has no C representation merely because it has the same fields. Rill strings, errors, references and callable values do not cross by their native Rill layout.

A read buffer has a checked address and length inside the bridge only. C may read it for the call but must not retain it after completion. The bridge can use staged bytes instead of pointing into Rill storage. Keep any required borrow until the operation has stopped accessing the buffer. No C address returns to the application.

For output, the bridge provides bounded storage and tracks the initialized length. Check size conversions and arithmetic before access. Do not expose more bytes than the validated initialized length. On success, return owned Rill data. On failure, release pending storage without exposing partial bytes. Keep allocation and release in the same allocator domain.

An opaque Rill handle owns one release obligation. A handle is not a public integer or pointer. A successful native creation gives the bridge responsibility until adoption or release. If adoption fails or cancellation intervenes, the bridge releases the resource. An admitted release action cannot be replaced by an ordinary overload. This proposal needs a new approval for that admission mechanism. The existing rules for protected toolchain operations do not grant it.

### Completion, cancellation and thread access

Use the [existing ownership rules](0007-ownership-borrowing-mutation-and-cleanup.md#protected-actions-and-asynchronous-completion) for work that can outlast a caller's wait. A request can be withdrawn before dispatch. After dispatch, keep its required data and resources until the C operation completes or the adapter confirms abortion. A request to cancel is not confirmation. Do not undo committed native effects by assumption.

An operation that requires the platform main thread must run there, including release. A Rill goroutine may submit an operation and wait without being pinned to that thread. The bridge owns the requests until completion. Serialize operations on a window. Do not destroy a window while a dispatched operation can still access it. Finish required releases before normal platform shutdown. This rule adds no global join for unrelated background work.

Report expected library failures as typed `Result` errors. A missing build input is not an application `Result` error. Stop execution on a detected violation of the trusted boundary, such as a reported length outside the supplied capacity. This check cannot detect all native memory violations.

### First slice: zlib

Proposed package signature sketch. It does not register a binding or define a new grammar form.

```rill
function inflateZlib(input: &Bytes, outputLimit: u64): Result<Bytes, InflateError>;
```

Decode one complete zlib stream. The output must not exceed the requested limit. Reject malformed or incomplete streams and extra bytes after the stream. Reject a size that the selected adapter cannot represent. Distinguish invalid input and output limit exhaustion when the adapter can establish the distinction. Do not infer an exact cause from an ambiguous native status alone. A typed failure exposes no partial output. Use the [zlib manual](https://zlib.net/manual.html) when selecting the native entry points and translating statuses.

### Second slice: SDL2

The first video package opens a window, copies selected input events into Rill values, and presents a frame supplied by Rill. Frame size and pixel layout must be explicit in the package contract. Check dimension conversions, stride and byte length before submitting a frame. Start with copied frame data and copied events. No SDL object address or pointer-bearing event crosses into Rill.

Initialize video, process events, present frames and release the window on the platform main thread. SDL2 restricts [event pumping](https://wiki.libsdl.org/SDL2/SDL_PumpEvents) and [presentation](https://wiki.libsdl.org/SDL2/SDL_RenderPresent). The first window owner is non-copyable. The draft does not grant sendability or shareability to it. Rill owns the frame loop logic. The native service executes its selected video operations and services the event loop. Audio, native callbacks and zero-copy rendering need separate contracts.

## Valid/invalid examples and diagnostics

These are proposed acceptance cases, not recorded compiler or runtime results. The positive zlib and SDL2 cases use actual linked libraries, not replacement implementations.

| Case | Context | Expected outcome | Owning rule | Evidence |
| --- | --- | --- | --- | --- |
| C1 | Rill calls `inflateZlib` on a known stream. The limit equals the decoded length. | It returns the exact expected bytes as owned data. | [zlib slice](#first-slice-zlib) | NotChecked |
| C2 | Supply corrupt or truncated input. Separately, decode the valid stream at one byte below its required capacity. | Return typed failures. No partial output escapes. | [zlib slice](#first-slice-zlib) | NotChecked |
| C3 | Import an unapproved binding. Separately, change an approved artifact or its declared signature. | The build rejects the binding and identifies the approval, input or signature failure. A same-spelled source declaration does not repair it. | [Build admission](#build-admission-and-identity) | NotChecked |
| C4 | An adapter reports a result longer than its supplied buffer. | Stop without publishing a `Bytes` value for that result. | [Boundary values](#boundary-values-and-ownership) | NotChecked |
| C5 | Run a Rill entry that opens a window, consumes a copied event and supplies a fixed pixel frame. | Actual SDL2 calls run on the main thread. The Rill program observes the event. The provider presents the supplied frame. | [SDL2 slice](#second-slice-sdl2) | NotChecked |
| C6 | Cancel after a native operation commits but before its result is delivered. Use synchronization to hold the operation open. | The bridge retains required inputs until completion. An unadopted window releases exactly once on the main thread. | [Completion](#completion-cancellation-and-thread-access) | NotChecked |
| C7 | Try to copy a window owner or replace its admitted release operation. | The compiler rejects the attempt without granting a second owner or replacing the release. | [Boundary values](#boundary-values-and-ownership) | NotChecked |

## Feature interactions

[RFC 0002](0002-modules-and-program-loading.md#family-collection-and-metadata-boundaries) owns source identity and ordinary name collection. Binding admission adds no overload ranking rule. [RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md) owns borrows, resource dependencies and cleanup. This draft does not authorize a native registry to manufacture those properties from a type name.

The bridge must preserve the [cancellation rules in ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md#cancellation-replacement-and-operation-ownership). It cannot use native work to bypass required cleanup. Ordinary Rill propagation forwards typed library failures under [RFC 0008](0008-errors-propagation-and-caused-by-wrapping.md#postfix-propagation-and-normal-return). Fatal panic remains separate from recoverable library failure. No unwinding crosses this C boundary.

## Compatibility and migration

The proposal adds no source syntax. Source-only programs do not need native package approval. Existing SQLite and TLS bindings do not migrate to this mechanism by default. They keep their own approval, provider and verification gates.

The first implementation needs an explicit native target and library build. An unsupported target must reject the binding instead of substituting a stub. Other native profiles and Wasm need separate evidence. None of this work changes the M5 or M6 acceptance baseline.

## Security/resource implications

A native binding runs trusted code in the process. Header translation, signature checks and an admission record cannot prove what C does. Review the adapter and the selected libraries for the contracts the application relies on. This mechanism does not sandbox native code.

Bound buffer sizes before allocation. Account for input staging, output storage and pending requests. A compressed input cannot override the output limit. Do not use a cancellation request to reclaim in-use storage. A native call can delay cancellation indefinitely if it cannot complete or be aborted. No latency, throughput or portability guarantee follows from a successful smoke test.

## Rationale, alternatives and prior art

The [interop survey](../research/c-interop-language-survey.md) compares header translation, checked exports and wrapper ownership. Rust provides a useful model for a small trusted adapter with an ordinary safe API outside it. Zig shows how the build can handle C sources and headers. Neither can infer pointer retention or thread restrictions from a header.

A C++ host calling exported Rill code is useful for an incremental port. It is not a prerequisite for a Rill executable that calls SDL2 and zlib. Public `extern` declarations and raw pointers could reach more libraries, but they would move foreign ownership assumptions into application code. Separate compiler integrations for every library would limit reuse. This draft takes a middle path: one binding mechanism with explicit admission for each package.

## Implementation implications

No implementation exists for this proposal. Wait for [native execution in M6](https://github.com/Reilley64/rill/issues/77). First check build admission and lower the fixed zlib import. Link the real library and test the byte boundary. Then add the owner and main-thread service needed by SDL2. Do not add raw pointer syntax as a shortcut.

The adapter may use an owned request and a native worker where the library allows it. An SDL2 video operation must use the main thread. The exact execution mechanism can vary if it preserves the access, completion and cleanup rules.

## Evidence and checking plan

| Artifact / revision | Evidence label | Rule/question exercised | Checker/tool and command/environment | Observed outcome | Limits |
| --- | --- | --- | --- | --- | --- |
| [C interop survey](../research/c-interop-language-survey.md) at the source-input digest | HandChecked | Compare header import, ABI admission and foreign ownership contracts. | Survey author read upstream docs and source on 2026-11-19. | The comparison identified ways to declare, link and wrap C calls. | No Rill import or SDL2 call is established. The survey's proposed checks are not executed evidence. |
| C1 through C7, this draft | NotChecked | Native import, copy boundary, owner release and thread affinity. | No implementation or test run. | No observed runtime result. | Expected results appear above. |

### Planned checks. Not executed evidence

- Check C3 before linking and check C7 before native execution. Retain diagnostics with the declaration, artifact or profile involved.
- Pin the target, compiler, adapter and library versions. Build and run C1, C2 and C4 in native CI. Record exact commands and results.
- Check C5 with an SDL2 backend that supports the window and render path. A dummy event test alone cannot verify that path.
- Use synchronized completion and release checks for C6. Check ordinary scope exit and explicit disposal as separate paths. Do not infer cleanup from timing.

## Open/deferred questions

**In-scope semantic blockers.** Specify the build approval input and how it identifies selected dependencies. Specify the approved ABI descriptor and how opaque handles are represented and validated at the bridge. Choose the first native profile and the exact SDL2 startup and shutdown contract for that profile. Define how an adapter distinguishes decompression limit failure from invalid input. These choices require review before this draft can become ReviewReady.

**Implementation and validation gates.** M6 native execution, actual provider linking, checked buffer and handle marshalling, and the main-thread service remain unverified. Keep SQLite and TLS acceptance separate.

**Outside this proposal.** Public C exports, arbitrary callbacks, C++ object interop, runtime symbol loading, unrestricted pointers, zero-copy frame transfer, audio and additional target profiles.

## Decision and change history

### Readiness record

No [ReviewReady review](README.md#canonical-reviewready-checklist) has been performed. The remaining contract and profile choices keep this RFC Draft. No acceptance or implementation is claimed.

### Human decisions and amendments

| Date / actor | Kind and scope of change | Reviewed revision or snapshot | Decision link / rationale |
| --- | --- | --- | --- |
| 2026-11-19, drafting agent | Drafted a Rill-led native import proposal with zlib and SDL2 as first cases. | Not yet reviewed. | The owner requested this RFC. This request did not accept its rules. |
