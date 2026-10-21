# Ownership is visible, and borrows never name a lifetime

Rill frees memory through ownership written in types, as Rust does, and not through reference counting the compiler hides, as Koka, Lean, and Roc do. A plain `T` parameter takes ownership and `&T` borrows, and callers write `&x` to borrow. Any value can hold references, and a value that holds one is itself a borrow. A returned reference borrows from every `&` parameter, so the compiler never needs to be told where it points, and Rill has no lifetime syntax. Copies are explicit (`copy(x)`), shared ownership is an opt-in counted `Shared<T>`, a function that owns a value can rebuild it in place, and a type's `dispose` overload runs when a value of that type is freed.

## Considered options

- **Hidden reference counting (Koka, Lean, Roc).** No ownership syntax, but every heap object carries a count, and speed depends on whether a value happens to be shared. Rejected for unpredictable cost.
- **Borrows only as parameters (Hylo).** No lifetimes, but every lookup such as `get` or `find` has to copy its result. Rejected because lookups are too common.
- **Rust lifetimes (`'a`).** Rejected as hard to read and write. Assuming a returned reference borrows from every `&` parameter is sometimes stricter than needed, and the fix is to pass that argument owned or copied.
