# Generated code allocates only through three runtime functions

**Status:** accepted, 2026-11-25 (owner decision). Drafted from Hello World grilling decisions Q33 and Q20, 2026-11-25; the owner decided Q10 that each ABI decision is recorded as an ADR. The standard-library contracts make allocation failure fatal and the RFCs mention an allocator protocol, but nothing named the allocator or how generated code reaches it. The first program that runs builds an owned string with `append`, so slice 1 needs one interface.

## Decision

- Generated code allocates only through three runtime functions: allocate, grow and release. Each takes a size and an alignment (Q33).
- In M6 the runtime backs them with the system allocator (Q33).
- An allocation failure is a panic (Q33). It ends the program as [ADR 0035](0035-panic-ends-the-program-with-status-101.md) says; the standard-library contract that allocation failure is fatal and not a `Result` stands ([ADR 0014](0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) says the same of reservation failure).
- The runtime has a checking allocator behind a hidden, test-only build flag (Q20). At exit it reports live allocations on standard error. Tests assert zero live allocations after a normal exit and a non-zero count after a panic. These are statechart 1's rules R1, R2c and R8 (`experiments/statechart-1-owned-value/`): a value is disposed at most once, each startup value is disposed once when startup completes, and a panic disposes nothing.

## Consequences

- Generated code never calls `malloc`, `free` or any other allocator by name. Swapping the system allocator for the checking one changes the runtime, not the generated code.
- Cleanup becomes observable: a leaked string or a double release is a count the test can read.
- The checking allocator ships in no product build, because the flag is hidden and test-only.
- The runtime is the one place that decides what an allocation failure does.

## Considered options

The ri-qlh record names no rejected alternative for Q33 or Q20. These are what the wording excludes; no source gives the owner's reasons.

- **Generated code calls `malloc`, `realloc` and `free` directly.** A checking allocator would then need interposition, which differs by platform.
- **One allocation entry point with no grow.** Growing an owned string would copy every time.
- **A user-replaceable allocator in M6.** Q33 limits M6 to the system allocator.
- **Live-allocation counts reported only by an external tool such as a platform leak checker.** Q20 puts the check in the runtime, so the same test runs on all four platforms.

## Open

The owner did not decide these. Nothing above decides them.

- The exact parameters of each function, such as whether grow takes the old size, and the C names of all three. The runtime documents its symbols.
- What a zero size does, which alignments are valid, and what size limit counts as failure.
- Whether grow may extend in place, and what release may assume about the pointer's origin.
- The flag's name, the report's format and where the flag is set (a Cargo feature, an environment variable or a build profile).
- Whether the list backing in `runtime/collection` passes through the three functions. Q24 links that crate unchanged, and it allocates through Rust's own allocator. If it does not pass through them, the checking allocator cannot see list allocations, and "zero live allocations" would hold for strings only.
- Whether a later milestone lets a program choose its allocator.
