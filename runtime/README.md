# Target-runtime support

This directory owns target-runtime support, including future compiler-required native support and target-specific runtime inputs. Rill standard-library algorithms belong in [`stdlib/`](../stdlib/README.md). The [native runtime library](native/src/lib.rs) is the only input that connects to Rill programs, and only through the C symbols below; Rill lowering for them is separate work.

## Native runtime library

`native/` is the package `rill-runtime`: a static library per host target that also links [`collection/`](collection/src/lib.rs) unchanged. It uses Rust `std`; on macOS its `rustc --print native-static-libs` list is `-liconv -lSystem -lc -lm`. Programs link it through the host `cc`. The [harness](native/tests/harness.rs) builds a C startup file against the archive and checks each behaviour below through the process's streams and status.

| Symbol | Provided by | Behaviour |
|---|---|---|
| `int main(int argc, char **argv)` | runtime | Sets `SIGPIPE` to ignored, copies the arguments and environment as bytes, starts counting allocations when `rill_check_allocations` is 1, calls `rill_startup`, then `rill_startup_cleanup`, on the entry stack, switches back, unmaps the stack, reports the count and returns 0. |
| `void rill_startup(void)` | generated code | The program's startup. |
| `void rill_startup_cleanup(void)` | generated code | Runs after normal completion of startup; a panic skips it. |
| `const uint8_t rill_check_allocations` | generated code | One byte, always defined: 0 by default, 1 when the program was built with the hidden option `--check-allocations`. Nothing reads an environment variable at run time. The C harness defines it too. |
| `void rill_rt_entry_stack_region(uintptr_t *low, uintptr_t *high)` | runtime | Test support. The lowest usable address of the entry stack and one past the highest, `high - low` being 1 MiB; both are 0 before `main` switches to it and after it switches back. |
| `uint8_t *rill_rt_allocate(size_t size, size_t align)` | runtime | System allocator. Never returns null: failure panics with `out of memory`, an `align` that is not a power of two with `invalid allocation layout`. A zero `size` allocates nothing and returns a dangling pointer equal to `align`; callers should avoid it. |
| `uint8_t *rill_rt_grow(uint8_t *ptr, size_t old_size, size_t align, size_t new_size)` | runtime | Resizes a block, keeping the bytes up to the smaller size. Same failure rules. A zero `old_size` allocates; a zero `new_size` releases and returns the dangling pointer. |
| `void rill_rt_release(uint8_t *ptr, size_t size, size_t align)` | runtime | Returns a block. A zero `size` does nothing. |
| `void rill_rt_write_line(const uint8_t *ptr, size_t len)` | runtime | Writes the bytes and one `\n` to file descriptor 1 with no buffering, retrying interrupted and partial writes. Any other failure, including a closed descriptor (`EBADF`) and a pipe with no reader (`EPIPE`), is a panic with `standard output is not writable`. |
| `_Noreturn void rill_rt_panic(const uint8_t *ptr, size_t len)` | runtime | Writes exactly `panic: <message>\n` to file descriptor 2 and ends the process with status 101 through `_exit`: no cleanup runs and nothing is buffered. |
| `size_t rill_rt_arg_count(void)`, `const uint8_t *rill_rt_arg(size_t index, size_t *len)` | runtime | The captured arguments, argument zero first, unvalidated and not NUL-terminated. An index out of range panics with `index out of range`. |
| `size_t rill_rt_env_count(void)`, `const uint8_t *rill_rt_env(size_t index, size_t *len)` | runtime | The captured `NAME=value` entries, with the same rules. |

`SIGPIPE` is ignored so that a write to a closed pipe fails and panics instead of ending the process by signal.

The entry stack ([ADR 0033](../docs/adr/0033-the-runtime-owns-main-and-the-entry-stack.md)) is an anonymous `mmap` region of 1 MiB of usable bytes with one page below it made `PROT_NONE` by `mprotect`. The page size comes from `sysconf`. An inline-asm trampoline for `x86_64` and `aarch64` switches the stack pointer to the top of the usable bytes, runs startup and its cleanup, and switches back. Overrunning the guard page ends the process by a signal (`SIGSEGV` or `SIGBUS`) and nothing more is specified in slice 1. A panic on the entry stack still ends the process with status 101 at once, without switching back.

The checking allocator is selected at build time by the hidden option `--check-allocations` of `rill build` and `rill run`, written before the entry (`rill run --check-allocations <entry> ...`). It is listed in no help text and no user documentation. The runtime's `#[global_allocator]` forwards to the system allocator, and `rill_rt_allocate`, `rill_rt_grow` and `rill_rt_release` allocate through it, so one counter of live allocations covers them and the list backing in `runtime/collection`. A grow counts neither as an allocation nor as a release unless it allocates fresh. Counting starts after the runtime captures the arguments and environment, so those are not counted. At exit the runtime writes exactly `live allocations: <N>` and a newline to file descriptor 2, unbuffered, once: after startup cleanup on a normal exit, and after the `panic: <message>` line on a panic. Without the option nothing is counted or written.

The [ownership archive](ownership/lib.rs) supplies the counted-owner retain and release operations, lock guard release, and channel endpoint retain and release for the first `aarch64-apple-darwin` profile. The Rust guard borrows its mutex until release, and blocking channel calls borrow their endpoints until return. There is no detached native I/O in this archive. Goroutine scheduling, cancellation, native I/O release and backend lowering still need separate implementations.

The [compiler build](../crates/compiler/build.rs) produces `librill_ownership.rlib` in Cargo's `OUT_DIR` for that target. To stage the scoped provider, copy that exact archive from the same compiler build into `lib/rill/runtime/aarch64-apple-darwin/`. Stage the corresponding [standard-library sources](../stdlib/README.md) from the same revision. The installed loader verifies the bytes; it does not load or run Rust code from the installed archive. The linked [Rust tests](ownership/tests.rs) exercise the native operations without running Rill or validating a Rill ABI.

**Note, 2026-11-25 (ri-qlh.22.3, owner decision Q18).** The analysis profile is no longer `aarch64-apple-darwin` alone. The compiler build produces the archive for the target the compiler is built for, one of `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin` and `x86_64-apple-darwin`, and stages it at `lib/rill/runtime/<that target>/`. The compiler analyses for the host it runs on and does not cross-compile. A request that names any other target loads source and gets no first-party evidence.

Cross-component runtime and target acceptance is owned by [`tests/`](../tests/README.md) through `test:runtime`, which currently exits nonzero as unavailable. The separate `test:llvm` task checks an LLVM C-API compile/link smoke only; it does not validate a Rill runtime.

[ADR 0014](../docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) and [ADR 0006](../docs/adr/0006-whole-program-monomorphization-on-llvm-23.md) record the backend decisions and qualifications. ri-48g retains production/formal ABI, lowering and runtime acceptance. ri-dej completed standalone mechanism experiments, not production integration. Provider-specific gates remain separate.

Existing [experiments](../experiments/README.md) stay in place with their recorded results and limitations. They are not runtime source or passing product suites. Future installed target inputs live under `lib/rill/runtime/<target>/`; [development](../docs/development.md) covers toolchain selection and host prerequisites.
