# Panics end the program without unwinding

A panic is for bugs only: indexing past the end of a list, dividing by zero, integer overflow, or `panic(message)`. It can't be caught, and in any goroutine it ends the whole program at once with its message and trace. Nothing unwinds, so `dispose` doesn't run and the OS takes back memory and files. This keeps unwind tables out of the LLVM IR and Wasm output, and errors a program expects to handle stay `Result` values.

## Considered options

- **Unwinding that runs `dispose`, as Rust does by default.** Buffered files would flush on a panic, but every function would need unwind tables and landing pads, on native and Wasm. Rejected.
- **No panics at all.** Every index and division would return a `Result`. Rejected as too noisy; `get(&list, i)` gives the safe form where it's wanted.
