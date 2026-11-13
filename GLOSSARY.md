# Rill

Rill is a functional-first programming language with TypeScript-style syntax. It compiles to native code and WebAssembly through LLVM, and it is memory safe without a tracing garbage collector.

## Language

**Borrow**:
Access to a value through a read-only reference (`&T`) or an exclusive mutable reference (`&mut T`) without owning it. Reference types do not name lifetimes.
_Avoid_: lifetime, pointer

**Borrow dependency**:
A requirement that a source remain valid and obey access restrictions while a reference, or a value carrying it, can still be used.
_Avoid_: ownership, lifetime annotation

**Borrow-free value**:
A value carrying no borrowed dependencies, including through nested fields or captures, as required by `noBorrow(T)`. This does not imply copyability or safe concurrent sharing.
_Avoid_: owned value, thread-safe value

**Callable**:
A function value with a read-only, mutable, or consuming invocation contract. That contract is separate from purity, borrowed captures, and copyability.
_Avoid_: Fn trait, async function

**Dispose**:
The `dispose` overload for a type: a function the compiler calls when a value of that type is freed. Calling it directly ends the value early.
_Avoid_: drop, destructor, deinit, finalizer

**Expectation**:
A top-level test declaration written `expect <expression>;`, whose boolean result determines whether it passes. Expectations run only under `rill test`, not as production assertions.
_Avoid_: runtime assertion, precondition

**Function**:
A named or anonymous piece of code that takes arguments and returns a value. Every Rill function can suspend, and callers wait for it without any extra syntax.
_Avoid_: async function, sync function, method

**Goroutine**:
A lightweight concurrent unit of execution, started with `spawn(() => f(x))`.
_Avoid_: thread, task, fiber, green thread

**Guard**:
A statement of the form `guard <condition> else { ... }` whose `else` block must leave its scope, with `return`, `break`, `continue`, or a call to a `never` function. It can also unwrap a value, as in `guard let user = findUser(id) else { ... }`.
_Avoid_: precondition, assert, early return

**Mutable borrow**:
Exclusive access through `&mut T` that permits mutation without ownership. Conflicting access to overlapping data is excluded while the borrow is needed.
_Avoid_: shared mutable reference, raw pointer

**Overload**:
One of several functions that share a name and fit that name's overload declaration, each with more specific parameter types. Overloads are how Rill gives one operation different behavior for different types.
_Avoid_: method, trait implementation, instance

**Overload declaration**:
A body-less `function` signature, ending in `;`, that declares a name as overloaded and fixes the shape every overload of it must fit.
_Avoid_: trait, interface, protocol, type class

**Reborrow**:
A temporary loan of existing borrowed access, restricting conflicting use of the original access until the loan is no longer needed.
_Avoid_: copy, new owner

**Routine**:
The handle `spawn(() => f(x))` returns for the goroutine it starts. `stop(&routine)` stops that goroutine at its next suspend point, and freeing the handle does not.
_Avoid_: task, future, promise, join handle

**Shared**:
A `Shared<T>` handle gives a value more than one owner by counting its owners. It is the only counted value in Rill, and shared ownership alone grants no mutable access to its payload.
_Avoid_: Arc, Rc
