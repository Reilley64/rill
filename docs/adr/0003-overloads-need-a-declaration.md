# Overloads need a declaration and resolve at compile time

Rill has no traits, so overloading carries per-type behavior. To keep that checkable, a name can only have overloads after a body-less overload declaration, such as `function show<T>(value: &T): string;`, fixes their shape. Every overload is the declaration with some types made more specific, and every call resolves to exactly one overload at compile time. A generic overload names what it needs with `where show(T)`, an overloaded name passed as a value takes the declaration's type, and overloads may only be written in the declaration's module or in a module that defines a type in their parameters, so two libraries can't define conflicting overloads.

## Considered options

- **Undeclared overloads, checked per instantiation (C++ templates, Zig).** No new syntax, but a generic signature doesn't say what it needs, and errors appear inside generic bodies. #5 found that inferring the requirements is NP-hard.
- **Run-time checks for literal overloads.** #2 first allowed `fib(n: 0)` to be chosen at run time when the value isn't known. Rejected: a call whose overload can't be fixed at compile time is now an error that points to `match`.
