# Rill

Rill is a functional-first programming language with TypeScript-style syntax. It compiles to native code and WebAssembly through LLVM, and it is memory safe without a tracing garbage collector.

## Language

**Borrow**:
Access to a value through a reference (`&T`) without owning it. The compiler works out what each borrow points into, so a reference type never names a lifetime.
_Avoid_: lifetime, pointer

**Dispose**:
The `dispose` overload for a type: a function the compiler calls when a value of that type is freed. Calling it directly ends the value early.
_Avoid_: drop, destructor, deinit, finalizer

**Function**:
A named or anonymous piece of code that takes arguments and returns a value. Every Rill function can suspend, and callers wait for it without any extra syntax, so Rill has only one kind of function.
_Avoid_: async function, sync function, method

**Goroutine**:
A lightweight concurrent unit of execution, started with `go f(x)`.
_Avoid_: thread, task, fiber, green thread

**Guard**:
A statement of the form `guard <condition> else { ... }` whose `else` block must leave the enclosing function. It can also unwrap a value, as in `guard let user = findUser(id) else { ... }`.
_Avoid_: precondition, assert, early return

**Overload**:
One of several functions that share a name and differ in their parameter types. Overloads are how Rill gives one operation different behavior for different types.
_Avoid_: method, trait implementation, instance

**Shared**:
A `Shared<T>` handle gives a value more than one owner by counting its owners. It is the only counted value in Rill.
_Avoid_: Arc, Rc
