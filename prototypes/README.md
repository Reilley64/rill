# Syntax prototypes

Sample programs for [issue #2](https://github.com/Reilley64/rill/issues/2). They are for choosing syntax, not a final grammar.

## Decisions so far

Later tickets changed some of these. The ticket behind each change is in parentheses.

- Statements end with a semicolon. Semicolons are required.
- `import std::log;` imports one name. `::` separates path segments.
- Top-level code runs only in the file you run. There is no `main` function. Top-level code in an imported file is ignored.
- `x |> f(y)` means `f(x, y)`. `x |> f` means `f(x)`. `x |> pad(10, _)` means `pad(10, x)`. The right side is a name, a call, or a parenthesized arrow function. `|>` binds looser than arithmetic, comparison, and logical operators, groups left to right, and unary `&` binds tighter. (#12)
- `?` after a pipe step applies to that step's result: `text |> parsePort? |> toAddress` means `toAddress(parsePort(text)?)`. (#12)
- There is no ternary. `if (c) a else b` is an expression that always needs `else`. On its own line, `if` is a TypeScript-style statement with blocks and `else if`. (#12)
- `guard <condition> else { ... }` and `guard let x = <Result> else (error) { ... }`. `(error)` binds the `Err` value. `guard` also takes a `Result` or `Option` directly, as in `guard send(&jobs, n) else { return; }`. The `else` block must leave its scope with `return`, `break`, `continue`, or a `never` call. (#11)
- Postfix `?` returns an `Err` or `none` early. An unused `Result` is a compile error; `let _ = x;` drops one on purpose. (#11)
- No semicolon after a block, as in TypeScript.
- `let x = 1;` binds a name. There is no mutation at all, so there is no `const` and no `mut`. Mutable variables are deferred.
- A plain `T` parameter takes ownership and `&T` borrows. Callers write `&x` to borrow, except for types that copy implicitly, such as `int` and `Point`. `copy(x)` copies, and `Shared<T>` gives a value several owners. (#7)
- A name needs an overload declaration before it can have overloads: a body-less signature such as `function show<T>(value: &T): string;`. Each overload is that signature with more specific types. (#10)
- The most specific overload wins. `show(items: &List<Point>)` beats `show<T>(items: &List<T>)`, so a generic overload is the default for types without their own overload.
- A string, number, or `bool` literal can be a parameter type, as in `greeting(language: "fr")`. A literal beats its type, such as `string`.
- Every call resolves to one overload at compile time. If a literal overload could match a value the compiler doesn't know, the call is an error, so `fib` uses `match`. `match` is the only way to split a union at run time. (#10)
- A generic overload states what it needs with `where show(T)`. (#10)
- If no overload is the most specific, the call is a compile error.
- `go f(x);` runs `f`, which must return `void`, in the background. It returns a `Routine`, and `stop(&routine)` stops it at its next suspend point. (#8)
- `channel<int>(10)` returns a `Sender<int>` and a `Receiver<int>`. The channel closes when its last `Sender` is freed. `first([tag(...), ...])` takes exactly one value from several receivers, and `after(ms)` is a receiver for timeouts. (#8)
- `std::routine` runs functions at the same time and waits, like TypeScript's `Promise` helpers: `all`, `allSettled`, `race`, and `any`. When `race` or `any` has a winner, or `all` fails early, the other functions stop at their next suspend point. A `race` of receives can lose a message; use `first`.
- When the top-level code ends, the program exits, as in Go. Running goroutines stop. A batch job or queue keeps the program alive by waiting on a channel.
- `for (let x of &list)` loops over a list. `for (let x of &receiver)` receives until the channel is closed.
- Arrow functions (`(x) => f(x)`), tuples (`[int, int]`), and destructuring (`let [a, b] = ...`) work as in TypeScript.
- A union is a TypeScript tagged union: records with a literal `kind` field, joined with `|`. There is no `enum`.
- `match (value) { pattern => expression, ... }` is an expression. Patterns can pull out fields, check nested values, use `_`, and take an `if` condition. The compiler rejects a `match` that misses a case.
- Inside a `match` arm, the value has the type of the member it matched. `{` after `=>` starts a block, so a record arm needs parentheses: `=> ({ ...shape })`. A block arm in a `match` with a value must leave the function. (#11)
- `{ ...record, field: value }` makes a new record.
- `std` has `type Option<T> = { kind: "some"; value: T } | { kind: "none" }`. `receive` on a closed channel returns `none`.
- Errors have causes and traces by default, as in the Rust crate [ohno](https://crates.io/crates/ohno). `Err(message)` and `Err(message, cause)` make an `Error`. The `Error` records each place it was made or returned through, and printing it shows the trace and every cause.

## Open questions

- Numeric and string types. The programs use `int`, `float`, and `string`. (#9)
