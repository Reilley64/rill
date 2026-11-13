# Syntax prototypes

Sample programs for [issue #2](https://github.com/Reilley64/rill/issues/2). They are for choosing syntax, not a final grammar.

## Decisions so far

Later tickets changed some of these. The ticket behind each change is in parentheses.

- Statements end with a semicolon. Semicolons are required.
- `import std::log;` imports one name. `::` separates path segments.
- Ordinary top-level code runs only in the file you run. There is no `main` function. Ordinary top-level code in an imported file is ignored. Top-level `expect` declarations are separate tests, discovered in imported modules too by `rill test`. (#27)
- `expect <expression>;` is a top-level, test-only declaration requiring a `bool`: `true` passes and `false` fails. It is a keyword, not an imported function, and is rejected inside ordinary functions, closures, or other nested blocks. Ordinary builds neither execute expectations nor include their test-only code in the executable. Failures report the source location, expression, and useful operand values without re-evaluating expressions or changing evaluation order or short-circuiting. Production checks still use `guard`, `Result`, or `panic`. See `06-expect.rill` for the syntax and rejection examples, and prototypes 2, 3, and 5 for expectations alongside the functions they test. Those expectations construct their own inputs rather than relying on ordinary top-level startup code. (#27)
- `x |> f(y)` means `f(x, y)`. `x |> f` means `f(x)`. `x |> pad(10, _)` means `pad(10, x)`. The right side is a name, a call, or a parenthesized arrow function. `|>` binds looser than arithmetic, comparison, and logical operators, groups left to right, and unary `&` binds tighter. (#12)
- `?` after a pipe step applies to that step's result: `text |> parsePort? |> toAddress` means `toAddress(parsePort(text)?)`. (#12)
- There is no ternary. `if (c) a else b` is an expression that always needs `else`. On its own line, `if` is a TypeScript-style statement with blocks and `else if`. (#12)
- `guard <condition> else { ... }` and `guard let x = <Result> else (error) { ... }`. `(error)` binds the `Err` value. `guard` also takes a `Result` or `Option` directly, as in `guard send(&jobs, n) else { return; }`. The `else` block must leave its scope with `return`, `break`, `continue`, or a `never` call. (#11)
- Postfix `?` returns an `Err` or `none` early. An unused `Result` is a compile error; `let _ = x;` drops one on purpose. (#11)
- No semicolon after a block, as in TypeScript.
- `let x = 1;` is immutable; `let mut x = 1;` permits reassignment and changes to owned fields. No `const` or per-field mutability declarations. Owned parameters are immutable bindings; move one into a mutable local to modify it. Loops may update mutable accumulators. (#15)
- A plain `T` parameter takes ownership, `&T` borrows read-only, and `&mut T` borrows exclusively for mutation. Callers explicitly obtain access with `&x` or `&mut x`, except for implicitly copied values such as `int` and `Point`. Existing copy classification is unchanged by mutability. `copy(x)` copies; neither copying nor moving erases borrowed dependencies, and exclusive access cannot be duplicated. `Shared<T>` gives several owners, not ordinary mutable access to the shared payload. (#7, #15)
- Check borrow conflicts statically until last use, including dependencies carried in other values and uses during disposal. Recognize disjoint named-field storage locally, not arbitrary indices or disjoint pointees merely from different field names. Forwarding mutable access to a borrowing parameter reborrows it. (#15)
- `=` and compound assignments such as `+=` are statements, not expressions; no `++` or `--` in v0. `*reference` accesses the referent, while field projection through borrows remains convenient. Replacement computes the destination/RHS once, frees/disposes the old value, then installs the replacement. Cancellation during disposal is a pending #16 contract. (#15)
- Reference-containing results and retained storage carry conservative input dependencies, including borrows nested in owned inputs. Check function exits against both returned values and caller-owned mutable destinations. Borrowing never extends the source's owning scope, and temporaries may be borrowed only within their enclosing statement. See [ADR 0007](../docs/adr/0007-explicit-mutation-with-conservative-borrow-dependencies.md), `07-mutation.rill`, and `07-mutation-safety.md` for the approved design and validation cases. (#15)
- A name needs an overload declaration before it can have overloads: a body-less signature such as `function show<T>(value: &T): string;`. Each overload is that signature with more specific types. (#10)
- The most specific overload wins. `show(items: &List<Point>)` beats `show<T>(items: &List<T>)`, so a generic overload is the default for types without their own overload.
- A string, number, or `bool` literal can be a parameter type, as in `greeting(language: "fr")`. A literal beats its type, such as `string`.
- Every call resolves to one overload at compile time. If a literal overload could match a value the compiler doesn't know, the call is an error, so `fib` uses `match`. `match` is the only way to split a union at run time. (#10)
- A generic overload states what it needs with `where show(T)`. `where noBorrow(T)` is a compiler-recognized intrinsic requiring no borrowed dependencies through fields/captures, not an overload a user can implement. Check generic bodies before monomorphization; an unconstrained `T` may carry borrows. (#10, #15)
- If no overload is the most specific, the call is a compile error.
- `spawn(() => f(x));` runs a zero-argument closure returning `void` in the background. Import `spawn` and `stop` directly from `std`; there is no `go` keyword or `std::process` module. `spawn` returns a `Routine`, and `stop(&routine)` stops it at its next suspend point. Freeing the handle does not stop it. (#8)
- `spawn` takes ownership of its closure, which moves its captures. A capture that holds a borrow cannot cross into the new goroutine. If the caller still needs a captured value, make an explicit `copy` before creating the closure; copying inside it would already have moved the original capture. The `std::routine` waiting helpers can still borrow their captures. (#7, #8)
- `channel<int>(10)` returns a `Sender<int>` and a `Receiver<int>`. The channel closes when its last `Sender` is freed. `select([tag(...), ...])` takes exactly one value from several receivers. It was called `first` in #8 and was renamed so it doesn't share a name with `std::list`'s `first`, and `after(ms)` is a receiver for timeouts. (#8)
- `std::routine` runs functions at the same time and waits, like TypeScript's `Promise` helpers: `all`, `allSettled`, `race`, and `any`. When `race` or `any` has a winner, or `all` fails early, the other functions stop at their next suspend point. A `race` of receives can lose a message; use `select`.
- When the top-level code ends, the program exits, as in Go. Running goroutines stop. A batch job or queue keeps the program alive by waiting on a channel.
- `for (let x of &list)` loops over a list. `for (let x of &receiver)` receives until the channel is closed.
- Arrow expressions (`(x) => f(x)`), tuples (`[int, int]`), and destructuring (`let [a, b] = ...`) use TypeScript-style syntax. Callable types distinguish repeatable read-only `() => R`, repeatable exclusive `mut () => R`, and consuming `once () => R`. Infer the required mode; weakening is allowed, but annotations cannot strengthen invocation guarantees or erase borrow dependencies. Read-only invocation does not imply purity. Mutating owned captures requires mutable access to the closure. Invocation mode, borrow dependencies, and copyability are separate; erased arrows do not automatically prove `noBorrow` or `copy`. (#15)
- A union is a TypeScript tagged union: records with a literal `kind` field, joined with `|`. There is no `enum`.
- `match (value) { pattern => expression, ... }` is an expression. Patterns can pull out fields, check nested values, use `_`, and take an `if` condition. The compiler rejects a `match` that misses a case.
- Inside a `match` arm, the value has the type of the member it matched. `{` after `=>` starts a block, so a record arm needs parentheses: `=> ({ ...shape })`. A block arm in a `match` with a value must leave the function. (#11)
- `{ ...record, field: value }` makes a new record.
- `std` has `type Option<T> = { kind: "some"; value: T } | { kind: "none" }`. `receive` on a closed channel returns `none`.
- Errors have causes and traces by default, as in the Rust crate [ohno](https://crates.io/crates/ohno). `Err(message)` and `Err(message, cause)` make an `Error`. The `Error` records each place it was made or returned through, and printing it shows the trace and every cause.

## Open questions

- The combined mutation checker is a proposal, not an implemented or demonstrated-sound system. #28 must supply explicit checking rules and adversarial examples; `07-mutation-safety.md` is hand-checked source material, not an executed fixture suite. #16 owns synchronized sharing and cancellation/disposal, and #23 owns evidence-preserving standard-library callable signatures. (#15)
- Numeric and string types. The programs use `int`, `float`, and `string`. (#9)
