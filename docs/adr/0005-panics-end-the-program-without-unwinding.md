# Panics end the program without unwinding

A panic is for bugs only: indexing past the end of a list, integer division/remainder by zero, integer overflow, or `panic(message)`. It can't be caught, and in any goroutine it ends the whole program at once with its message and trace. Nothing unwinds, so `dispose` doesn't run and the OS takes back memory and files. This keeps unwind tables out of the LLVM IR and Wasm output, and errors a program expects to handle stay `Result` values.

[ADR 0011](0011-explicit-numeric-widths-and-utf8-string-views.md) explicitly gives floats IEEE non-trapping division/remainder behavior. Invalid individual literals reject at compile time, but otherwise well-typed constant numeric expressions retain ordinary panic-if-executed semantics; folding must preserve acceptance/effect order. Failed try_from/try_into conversions return Result, not a catchable panic. Required cancellation cleanup remains distinct from this fatal-panic path under ADR 0009.

[ADR 0014](0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) clarifies that normal panic traces describe available frames, without requiring the history of eliminated proper-tail calls. Managed addressable/saved-stack overflow uses a bounded best-effort emergency report, not a required full trace or user formatter on an exhausted stack. Host engine exhaustion is fatal to the Rill execution, not necessarily its embedding host process; resource/host failure may prevent a report. No unwind or cleanup guarantee is added. Confirmed ri-vtw separately replaces historical recoverable propagation traces with construction origins and explicit causes; it is not a panic-trace rule.

[ADR 0016](0016-structured-diagnostics.md) distinguishes false expectations from fatal execution: a false expectation finishes normal expression cleanup, reports an ordinary failure and lets the sequential suite continue. It does not call panic or unwind. Actual panic/overflow/resource exhaustion still ends the current Rill execution without a guaranteed complete report; fatal cases are separately isolated, not caught by a new expectation helper. Compiler/server internal failures and driver statuses are tooling outcomes, not changed source-language panic semantics.

[ADR 0023](0023-tcp-addresses-resolution-and-controlled-networking.md) / ri-vno separates fatal Rill/runtime allocation, reservation, bookkeeping or error-construction failure from a native operation reporting resource exhaustion. The native operation can produce a typed NetError with native detail. This is not a recoverable Rill allocator or guaranteed out-of-memory report; constructing the error can still fail fatally. Explicit token cancellation is also an ordinary controlled-operation outcome, not this panic path or intrinsic routine cancellation.

[ADR 0013](0013-caused-by-errors-with-explicit-wrapping.md) changes recoverable Result errors to caused-by chains with construction locations and no propagation trace. It does not remove fatal panic traces or change this ADR's no-unwinding policy.

## Considered options

- **Unwinding that runs `dispose`, as Rust does by default.** Buffered files would flush on a panic, but every function would need unwind tables and landing pads, on native and Wasm. Rejected.
- **No panics at all.** Every index and integer division would return a `Result`. Rejected as too noisy; `get(&list, i)` gives the safe form where it's wanted.
