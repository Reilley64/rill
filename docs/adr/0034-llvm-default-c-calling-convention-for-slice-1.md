# Slice 1 uses LLVM's default C calling convention internally

**Status:** draft for owner approval (Hello World grilling decision Q34, 2026-11-25; the owner decided Q10 that each ABI decision is recorded as an ADR). The docs name `tailcc` and `musttail` as the intended native mechanism for proper tail calls ([ADR 0014](0014-bounded-stacks-proper-tail-calls-and-backend-gates.md), [ADR 0006](0006-whole-program-monomorphization-on-llvm-23.md)) but chose no convention for ordinary calls. The first program calls no recursion and no tail call.

## Decision

- In slice 1, generated Rill functions call each other with LLVM's default C calling convention (Q34).
- `tailcc` arrives in slice 3, with recursion (Q34).
- ADR 0014 and ADR 0006 stand. Eligible tail calls must be honored in development and optimized builds, or reported as an unsupported lowering. The Wasm direction (compiler-owned records and a dispatcher) is unaffected.

## Consequences

- Calls between generated functions and calls into the runtime use one convention, so slice 1 needs no thunks.
- Slice 1 adds no convention-specific lowering. Slice 3 changes the convention of functions that take part in tail transfers, and the program-wide effect of that change is slice 3's to specify.
- A program in slice 1 gets no proper-tail guarantee from its calling convention. Slice 1's rule for accepting a program must keep ADR 0014's obligation: a program with an eligible tail call is accepted only if the call is honored, or it is reported as unsupported.

## Considered options

The ri-qlh record names no rejected alternative for Q34. These are what the wording excludes.

- **`tailcc` for every function from the start.** Q34 delays it to slice 3, with recursion, the first feature that needs it.
- **A Rill-specific convention that passes owned values in registers or by a private layout.** ADR 0006 and ADR 0014 call this a later ABI, "not a finished private typed/owned-value ABI".
- **`fastcc`.** Q34 names the default C convention.

## Open

The owner did not decide these. Nothing above decides them.

- How an aggregate crosses a call: an owned string is three words and a view is two ([ADR 0031](0031-owned-strings-are-pointer-length-capacity.md)). "LLVM default C convention" says how LLVM passes what the generated IR contains, not whether the IR passes a string as a first-class aggregate, as separate words or by pointer.
- How slice 1 meets ADR 0014 when a program has a call that is eligible for a tail transfer, since the convention gives no guarantee. Options include refusing the program or relying on LLVM `musttail`. The wording says neither.
- Whether the runtime's functions are called with the same convention. The runtime is a C-ABI static library, so they would be, but Q34 speaks only of calls inside generated code.
