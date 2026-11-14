# Panics end the program without unwinding

A panic is for bugs only: indexing past the end of a list, integer division/remainder by zero, integer overflow, or `panic(message)`. It can't be caught, and in any goroutine it ends the whole program at once with its message and trace. Nothing unwinds, so `dispose` doesn't run and the OS takes back memory and files. This keeps unwind tables out of the LLVM IR and Wasm output, and errors a program expects to handle stay `Result` values.

[ADR 0011](0011-explicit-numeric-widths-and-utf8-string-views.md) explicitly gives floats IEEE non-trapping division/remainder behavior. Invalid individual literals reject at compile time, but otherwise well-typed constant numeric expressions retain ordinary panic-if-executed semantics; folding must preserve acceptance/effect order. Failed try_from/try_into conversions return Result, not a catchable panic. Required cancellation cleanup remains distinct from this fatal-panic path under ADR 0009.

[ADR 0013](0013-caused-by-errors-with-explicit-wrapping.md) changes recoverable Result errors to caused-by chains with construction locations and no propagation trace. It does not remove fatal panic traces or change this ADR's no-unwinding policy.

## Considered options

- **Unwinding that runs `dispose`, as Rust does by default.** Buffered files would flush on a panic, but every function would need unwind tables and landing pads, on native and Wasm. Rejected.
- **No panics at all.** Every index and integer division would return a `Result`. Rejected as too noisy; `get(&list, i)` gives the safe form where it's wanted.
