# Owned startup environment and argument inputs

[Issue #36](https://github.com/Reilley64/rill/issues/36) confirms `std::env`: explicit independently owned `Inputs` values copied from a fixed safe-startup capture, read-only byte access, strict owned-text conveniences and validated synthetic construction. The complete Q1-Q21 interview and consolidated summary were explicitly confirmed. [The public catalogue](../standard-library-contracts.md#12-environment-and-argument-inputs) is the API/behavior contract; [prototype 14](../../prototypes/14-environment-inputs.rill) and its [case inventory](../prototypes/14-environment-inputs.md) are hand-checked design evidence, not an implemented library, executed tests or accepted feature RFC.

## Why capture rather than live lookup

Native environment data is shared mutable process state. A Rill-only lock cannot synchronize arbitrary foreign readers/writers. Capture at a controlled safe point before Rill workers, or accept owned inputs from an embedding host. After capture, library access never consults the live environment. A multithreaded embedding cannot justify capture merely by calling it startup. This is trusted runtime state, like existing standard I/O services, not a new source-level runtime global, module initializer, named-function capture rule or hidden shared owner for returned snapshots.

`inputs()` returns independently owned data or a fresh error describing stored safe failure information. Ordinary acquisition/validation failure does not force pre-entry termination or masquerade as an empty snapshot. No user formatter runs during capture. Fatal allocation failures, host traps and inability to instantiate a Wasm program remain outside Result recovery. Copies returned to applications contain no borrowed startup storage; `copy(&inputs)` is explicit and independent.

## Bytes, validation and reproducibility

Arguments and environment data delivered on current Linux/macOS/WASI targets need not be UTF-8. Preserve bytes, including non-UTF-8 names, while rejecting NULs, invalid names, malformed entries and exact duplicate names. Native shell-identifier conventions are not the name grammar. Keep the full supplied argument sequence without interpreting quoting or treating argument zero as an executable path. Sort accepted environment entries by raw byte names, not host order, locale or Unicode normalization. These validation/ordering guarantees are Rill policies, not universal platform guarantees.

Byte queries borrow stored data; text conveniences validate only the data requested and return independently owned strings. Missing and present-empty remain distinct. Construction fails as a whole in the specified first-error order. `EnvError` exposes kind/position, not offending contents; standard rendering must not include values or full arguments, even through retained diagnostic state. Explicit application printing is still the application's responsibility; this is not secret detection or secure-erasure support.

`rill test` skips actual host capture and supplies empty inputs. Synthetic `fromParts` inputs use the same validation/ordering rules. Tests remain independent of developer/CI secrets. Linux/macOS and wasm32-wasip1 share the public contract, but WASI sees only instance-supplied inputs, never implicit host inheritance. A host may only support UTF-8 injection even though delivered guest data is byte-oriented. No Windows support or live-host testing opt-in is added.

## Alternatives and boundaries

Rejected live process lookup/mutation; implicit borrowed process-global storage; UTF-8 panic/lossy conversion or silent entry dropping; duplicate first/last-wins normalization; treating missing and empty as the same; trusting argv[0] as a path; and ambient host inputs in ordinary tests. A failed acquisition is a typed result, not an empty fallback. General flag parsing, dotenv/layered configuration, process spawning and an executable-location API remain separate scope.

#23's core ownership/errors/Bytes contract is extended, not reopened. #35 owns filesystem/path behavior; #24's confirmed diagnostics policy remains applicable. #28 must formalize transitive raw-view and lookup-key dependencies, source exits, safe runtime/host capture, checked sizing, immutable outcome publication and repeatable independently owned acquisitions, validation priority, sanitized diagnostics and bounded capture/sort/copy/decode/cleanup. Preserve #23's cause-retention boundary and #30's backend/Wasm gates. An inconsistent necessary contract returns to the map; hand-checked examples are not a checker or platform proof.

## Platform evidence

- [POSIX environment](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap08.html): names/values, case sensitivity, unspecified ordering and duplicate-name consequences.
- [POSIX exec](https://pubs.opengroup.org/onlinepubs/9799919799/functions/exec.html): argv/environ strings and the advisory nature of argv[0].
- [POSIX getenv](https://pubs.opengroup.org/onlinepubs/9799919799/functions/getenv.html) and [setenv](https://pubs.opengroup.org/onlinepubs/9799919799/functions/setenv.html): missing versus empty, name validity and concurrent modification boundaries.
- [Apple environ interface](https://github.com/apple-oss-distributions/Libc/blob/main/man/FreeBSD/environ.7): native string arrays and library/bundle access.
- [WASI preview1 WITX](https://github.com/WebAssembly/WASI/blob/wasi-0.1/preview1/witx/wasi_snapshot_preview1.witx): args/environ byte buffers, host-returned failures and no ambient process-mutation interface.
- [Rust vars_os](https://doc.rust-lang.org/std/env/fn.vars_os.html), [args_os](https://doc.rust-lang.org/std/env/fn.args_os.html), and [environment mutation safety](https://doc.rust-lang.org/std/env/fn.set_var.html): byte-oriented owned access, invocation snapshots versus live lookup and foreign-concurrency hazards. Rill does not copy Rust's malformed-entry filtering or invalid-name/missing conflation.

These sources explain the boundary. They do not establish Rill's implementation safety or every embedding host's behavior. Current provider-specific capture and host sizing/read consistency still need validation.
