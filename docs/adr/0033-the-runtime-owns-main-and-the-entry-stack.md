# The runtime owns `main` and runs startup on a managed entry stack

**Status:** accepted, 2026-11-25 (owner decision). Drafted from Hello World grilling decisions Q35 and Q9, 2026-11-25; the owner decided Q10 that each ABI decision is recorded as an ADR. [ADR 0014](0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) says entry execution uses a managed, fixed 1 MiB region rather than the OS main-thread stack. [ADR 0019](0019-owned-startup-environment-and-arguments.md) says arguments and the environment are captured at a controlled point before Rill code runs. Nothing named who owns the process entry point or the order of these steps. **ADR 0014 stands; this ADR adds no deviation.**

## Decision

The C `main` function lives in the runtime, a Rust static library per target (grilling round 1, Q5). In order, it:

1. captures the command-line arguments and the environment (Q35);
2. reserves the 1 MiB entry stack with a guard page (Q35);
3. switches to that stack (Q35);
4. calls the generated startup code, which runs the entry file's startup statements (Q35);
5. runs startup cleanup, so the startup scope's owned values are cleaned up, still on the entry stack (Q35; charting Q12 and chart rule R2c);
6. switches back and returns 0 (Q35).

In slice 1 the entry stack is 1 MiB with a guard page, the default size ADR 0014 sets (Q9). `--stack-size`, `--entry-stack-size` and the overflow report arrive in slice 3 (Q9).

A panic never reaches steps 5 and 6. It ends the program at once and runs no cleanup ([ADR 0035](0035-panic-ends-the-program-with-status-101.md)).

## Consequences

- Generated code supplies startup and its cleanup. It does not define `main`, so the runtime fixes the order of capture, stack switch and cleanup for every program.
- Every program runs its startup the way it finally will, on a guard-paged managed stack and not on the OS stack, so slice 3 changes a size and a report, not where startup runs.
- Capture happens before any generated code, so `std::env::inputs()` reads captured data and never the live environment, as ADR 0019 requires.
- A normal return from startup exits 0. In slice 1 no other status comes from a normal return.

## Considered options

The ri-qlh record names no rejected alternative for Q35 or Q9. These are what the wording excludes.

- **A `main` emitted by the generated code.** Capture, stack setup and the panic exit would then be generated for every program. Q5 and Q35 put them in the runtime.
- **Running startup on the OS main-thread stack for slice 1.** ADR 0014 says entry execution is not an inherited OS stack, and Q9 declines a deviation.
- **A new thread for startup.** Q35 describes a switch on the same thread and a switch back, and no thread.

## Built in slice 1

Closed by ri-qlh.22.9; planner rulings, not owner decisions ([`runtime/native`](../../runtime/native/src/entry_stack.rs), [`runtime/README.md`](../../runtime/README.md)).

- **Reserving the region.** `mmap` maps one anonymous private region of 1 MiB of usable bytes plus one page, and `mprotect` makes the lowest page `PROT_NONE`. The guard sits below the usable bytes and outside them, as ADR 0014 requires. The page size comes from `sysconf(_SC_PAGESIZE)`, not a constant, because macOS on arm64 has 16 KiB pages. The same code runs on Linux and macOS through the `libc` crate.
- **Switching.** A small `core::arch::asm!` trampoline for `x86_64` and `aarch64` saves the stack pointer in a callee-saved register (`r12`, `x20`), sets it to the top of the usable bytes (page aligned, so 16-byte aligned for both ABIs), calls a function that calls `rill_startup` then `rill_startup_cleanup`, and restores the stack pointer. No stack-switching crate is used. `main` then unmaps the region and returns 0.
- **Panic.** A panic on the entry stack ends the process at once with status 101 and does not switch back ([ADR 0035](0035-panic-ends-the-program-with-status-101.md)).
- **Test support.** `rill_rt_entry_stack_region` reports the usable bounds, which are 0 outside the region's lifetime. The harness compares a local's address and reads the byte just below the region, which ends the process by `SIGSEGV` or `SIGBUS`.

## Open

The owner did not decide these. Nothing above decides them.

- What happens in slice 1 when startup overruns the 1 MiB. The overflow report arrives in slice 3, so the behavior until then, such as the guard page's fault, is unspecified.
- The C names and signatures of the startup and cleanup functions. The runtime documents its symbols.
- Whether the arguments are captured as owned bytes or as borrowed `argv` storage, and how the environment is ordered and validated before `std::env` sees it (ADR 0019 sorts and validates in the library).
- Which statuses a program other than 0 and the panic status can produce, since slice 1 returns 0 only.
