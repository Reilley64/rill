# Slice 1 uses LLVM's default C calling convention internally

**Status:** accepted, 2026-11-25 (owner decision). Drafted from Hello World grilling decision Q34, 2026-11-25; the owner decided Q10 that each ABI decision is recorded as an ADR. The docs name `tailcc` and `musttail` as the intended native mechanism for proper tail calls ([ADR 0014](0014-bounded-stacks-proper-tail-calls-and-backend-gates.md), [ADR 0006](0006-whole-program-monomorphization-on-llvm-23.md)) but chose no convention for ordinary calls. The first program calls no recursion and no tail call.

## Decision

- In slice 1, generated Rill functions call each other with LLVM's default C calling convention (Q34).
- `tailcc` arrives in slice 3, with recursion (Q34).
- **No tail-call guarantee in slice 1.** This is an owner decision of 2026-11-25. A tail-eligible call is an ordinary call under the C convention, and a deeply recursive program dies at the entry stack's guard page. `tailcc` closes the gap in slice 3. [ADR 0014](0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) carries a dated note saying the same.
- ADR 0014's guarantee that eligible tail calls are honored, or reported as an unsupported lowering, is not met by slice 1 and is not weakened beyond it. ADR 0006 stands. The Wasm direction (compiler-owned records and a dispatcher) is unaffected.

## Consequences

- Calls between generated functions and calls into the runtime use one convention, so slice 1 needs no thunks.
- Slice 1 adds no convention-specific lowering. Slice 3 changes the convention of functions that take part in tail transfers, and the program-wide effect of that change is slice 3's to specify.
- A program in slice 1 gets no proper-tail guarantee. The compiler accepts a program with a tail-eligible call and compiles the call as an ordinary call. Such a program that recurses deeply enough ends at the entry stack's guard page ([ADR 0014](0014-bounded-stacks-proper-tail-calls-and-backend-gates.md), [ADR 0033](0033-the-runtime-owns-main-and-the-entry-stack.md)), and slice 1 does not report the call as unsupported.

## Considered options

The ri-qlh record names no rejected alternative for Q34. These are what the wording excludes.

- **`tailcc` for every function from the start.** Q34 delays it to slice 3, with recursion, the first feature that needs it.
- **A Rill-specific convention that passes owned values in registers or by a private layout.** ADR 0006 and ADR 0014 call this a later ABI, "not a finished private typed/owned-value ABI".
- **`fastcc`.** Q34 names the default C convention.

## Settled by the backend

The first backend (ri-qlh.22.8, part A) fixed how a string crosses a call: as separate words, the three of an owned string or the two of a view ([ADR 0031](0031-owned-strings-are-pointer-length-capacity.md)), and never as one aggregate. Slice 1 has no result other than `void` and `never`, so a result type has no convention yet. This is an implementation detail, not an owner decision.

## Open

The owner did not decide these. Nothing above decides them.

- Whether the runtime's functions are called with the same convention. The runtime is a C-ABI static library, so they would be, but Q34 speaks only of calls inside generated code.
