# Syntax prototypes

Sample programs for [issue #2](https://github.com/Reilley64/rill/issues/2). They are for choosing syntax, not a final grammar.

## Decisions so far

- Statements end with a semicolon. Semicolons are required.
- `import std::log;` imports one name. `::` separates path segments.
- Top-level code runs only in the file you run. There is no `main` function. Top-level code in an imported file is ignored.
- `x |> f(y)` means `f(x, y)`. `x |> f` means `f(x)`.
- `guard <condition> else { ... }` and `guard let x = <Result> else (error) { ... }`. `(error)` binds the `Err` value.
- No semicolon after a block, as in TypeScript.
- `let x = 1;` binds a name. There is no mutation at all, so there is no `const` and no `mut`. Mutable variables are deferred.
- An overload is another `function` declaration with the same name and different parameter types.
- The most specific overload wins. `show(items: List<Point>)` beats `show<T>(items: List<T>)`, so a generic overload is the default for types without their own overload.
- A string, number, or `bool` literal can be a parameter type, as in `greeting(language: "fr")`, `fib(n: 0)`, or `show(value: true)`. A literal beats its type, such as `string`.
- The compiler picks an overload by argument types at compile time. If literal overloads exist and the value isn't known at compile time, it adds a run-time check of the value. If the value is known, as in `greeting("fr")`, there is no check.
- If no overload is the most specific, the call is a compile error.
- `go f(x);` runs `f` in the background on its own, for work such as batch jobs and queues. Its result is dropped. Goroutines talk over a `Channel<T>`.
- `std::routine` runs functions at the same time and waits, like TypeScript's `Promise` helpers: `all`, `allSettled`, `race`, and `any`. When `race` or `any` has a winner, or `all` fails early, the other functions stop at their next suspend point.
- When the top-level code ends, the program exits, as in Go. Running goroutines stop. A batch job or queue keeps the program alive by waiting on a channel.
- `for (let x of list)` loops over a list. `for (let x of channel)` receives until the channel is closed.
- Arrow functions (`(x) => f(x)`), tuples (`[int, int]`), and destructuring (`let [a, b] = ...`) work as in TypeScript.
- A union is a TypeScript tagged union: records with a literal `kind` field, joined with `|`. There is no `enum`.
- `match (value) { pattern => expression, ... }` is an expression. Patterns can pull out fields, check nested values, use `_`, and take an `if` condition. The compiler rejects a `match` that misses a case.
- Inside a `match` arm, the value has the type of the member it matched. An arm body is one expression, so `{` after `=>` starts a record.
- `{ ...record, field: value }` makes a new record.
- `std` has `type Option<T> = { kind: "some"; value: T } | { kind: "none" }`. `receive` on a closed channel returns `none`.
- Errors have causes and traces by default, as in the Rust crate [ohno](https://crates.io/crates/ohno). `Err(message)` and `Err(message, cause)` make an `Error`. The `Error` records each place it was made or returned through, and printing it shows the trace and every cause.

## Open questions

- `match` and literal overloads can both split a union by its `kind`. Should both be allowed? (#10)
- A generic overload like `show<T>` has no way to state that `show(T)` must exist. #5 found that inferring overloads with no annotations is NP-hard. (#10)
- Numeric and string types. The programs use `int`, `float`, and `string`.
