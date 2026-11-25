# A panic writes one line to standard error and ends the program with status 101

**Status:** draft for owner approval (charting decisions Q11 and Q12, 2026-11-24; Hello World grilling decisions Q10 and Q22, 2026-11-25; the owner decided Q10 that each ABI decision is recorded as an ADR). [ADR 0005](0005-panics-end-the-program-without-unwinding.md) says a panic ends the whole program with its message and trace and unwinds nothing. It names no output stream, no exit status and no format. Slice 1 runs a real program, so those become observable.

## Decision

- **Normal completion.** `log` writes to standard output. When startup completes normally the program exits 0, and `rill run` returns the program's status as its own (charting Q11; grilling Q19).
- **Cleanup on normal completion.** When startup completes normally, the startup scope's owned values are cleaned up before exit (charting Q12; statechart 1 rule R2c).
- **Panic.** A panic writes exactly `panic: <message>` and a newline to standard error, with no trace in slice 1, and ends the program with status 101 (grilling Q22 and Q10). A panic runs no cleanup (ADR 0005; charting Q12; rule R8).
- **Distinct from the driver.** The panic status differs from the driver's statuses (charting Q11). The [driver status table](../diagnostic-contracts.md#driver-status), which [ADR 0016](0016-structured-diagnostics.md) records, lists them as 0, 1, 2, 3 and 130, so 101 is free. A program the compiler cannot yet compile fails with driver status 1 and no executable (Q8).
- **Failing writes.** A failed `log` write, such as standard output closed, is a panic (Q10). A non-UTF-8 argument is a panic raised by the program's own `guard`, not by the runtime (grilling Q14), and ends with the same status.
- **Panics in cleanup.** A panic inside a `dispose` overload, or during exit cleanup, ends the program like any other panic (statechart decision S9).

## Consequences

- A test can tell a Rill panic (101, one `panic: ` line on standard error) from a driver failure (1, 2, 3) and from a normal exit (0) by status alone.
- ADR 0005's "trace" is not produced in slice 1. This ADR does not repeal it; it records that slice 1 omits it.
- The [diagnostic contract](../diagnostic-contracts.md#driver-status) says raw application exits are not normalized into driver statuses. Passing the program's status through `rill run` agrees: the driver reports what the program returned and does not map it.
- Acceptance runs the real `rill` binary on four platforms for: a name argument, no argument, a non-UTF-8 argument (panic, 101), and standard output closed (panic, 101). The checking allocator ([ADR 0032](0032-generated-code-allocates-through-three-runtime-functions.md)) shows zero live allocations after a normal exit and a non-zero count after a panic.

## Considered options

The ri-qlh record gives no reasons for 101 and names no rejected alternative. These are what the wording excludes.

- **Status 1, 2, 3 or 130.** Charting Q11 requires a status distinct from the driver's, and these are driver statuses.
- **Abnormal termination by a signal, as `abort` does.** The program would end with a platform-dependent status and might write a core dump, and Q10 names status 101.
- **Unwinding and running cleanup before the exit.** ADR 0005 rejected it.
- **A trace in slice 1.** Q22 says none.

## Open

The owner did not decide these. Nothing above decides them.

- The message text of a failed `log` write. No owner decision names it.
- Whether and when panics gain a trace, and what status and report a stack overflow, an allocation failure and other fatal conditions use. Slice 3 adds the overflow report.
- Whether standard output is buffered, and whether a broken pipe (`SIGPIPE`) ends the program by signal or becomes a failed write. Closing standard output is decided to be a panic; a closed pipe is not mentioned.
- What a panic message may contain: multiple lines, bytes that are not UTF-8, or a length limit.
- What `rill run` reports when the program is killed by a signal, since a signal has no exit status.
- Which statuses other than 0 and 101 a program can return once a later slice adds a way to set one.
