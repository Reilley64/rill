# Rill

Rill is a functional-first programming language with TypeScript-style syntax. It compiles to native code and WebAssembly through LLVM, and it is memory safe without a tracing garbage collector.

## Language

**Function**:
A named or anonymous piece of code that takes arguments and returns a value. Every Rill function can suspend, and callers wait for it without any extra syntax, so Rill has only one kind of function.
_Avoid_: async function, sync function, method

**Goroutine**:
A lightweight concurrent unit of execution, started with `go f(x)`.
_Avoid_: thread, task, fiber, green thread

**Guard**:
A statement of the form `guard <condition> else { ... }` whose `else` block must leave the enclosing function. It can also unwrap a value, as in `guard const user = findUser(id) else { ... }`.
_Avoid_: precondition, assert, early return

**Overload**:
One of several functions that share a name and differ in their parameter types. Overloads are how Rill gives one operation different behavior for different types.
_Avoid_: method, trait implementation, instance
