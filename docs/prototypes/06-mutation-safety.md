# Mutation safety examples

Read the source examples in [`prototypes/06-mutation.rill`](../../prototypes/06-mutation.rill).

These are design cases for [#15](https://github.com/Reilley64/rill/issues/15) and [ADR 0007](../adr/0007-explicit-mutation-with-conservative-borrow-dependencies.md). Reviewers checked the expected outcomes against the approved design by inspection, not with a compiler. The cases inform #28's explicit checking rules and eventual compile-pass/compile-fail tests. They are not evidence of a soundness proof.

`06-mutation.rill` supplies complete intended-valid examples with independent top-level expectations. Snippets here are separate cases, not one program. Library signatures without bodies are schematic contracts. #23 still owns their complete standard-library definitions. An executable fixture suite must make those dependencies explicit.

## Basic access and reborrowing

The mutation prototype intends to accept mutable accumulators, scalar dereferencing, owned mutable locals, repeated forwarding of `&mut`, and simultaneous borrows of distinct named fields. It intends to reject:

```ts
let fixed = Point { x: 1, y: 2 };
fixed.x = 3; // Rejected: the owned binding is immutable.
```

```ts
let mut point = Point { x: 1, y: 2 };
let whole = &point;
let x = &mut point.x; // Rejected: whole-record read access is still needed.
log(`${whole.x}`);
```

Separate fields do not prove separate pointees. The checker must preserve the actual source restrictions of reference-valued fields. Extracting one reference-containing field conservatively retains the aggregate's possible dependencies; Q5 does not promise field-sensitive provenance or release of all aggregate dependencies after one field replacement.

## Function exits and retained storage

Use the schematic contract `remember(dst: &mut List<&User>, user: &User): void`, which can retain `user` in `dst`. An implementation must check that retained references satisfy the caller's destination obligations; the call propagates the retained dependency even though it returns `void`.

With `type User = { name: string };`, both cases below must be rejected:

```ts
function poison(dst: &mut List<&User>): void {
  let local = User { name: "short-lived" };
  remember(dst, &local);
} // Rejected: the caller's destination would retain a local source.
```

```ts
function overwrite(slot: &mut &User): void {
  let local = User { name: "short-lived" };
  *slot = &local;
} // Rejected: nested mutable access cannot bypass the exit contract.
```

The prototype's complete `remember(slot: &mut &Point, point: &Point)` and `retainedInput` examples are intended to be valid. They store a caller-provided reference through a mutable slot whose sources remain valid. Forwarding caller-provided `dst` and `user` to the schematic list-based `remember` can also be valid. The forwarding function's contract must preserve that possible flow. Ending a temporary reborrow does not remove dependencies written into the original destination. The loan for access to the outer container is distinct from the sources referenced by its elements.

## Results, temporaries, and exclusive restrictions

- Accept `carriedBorrow` in the prototype. `identity<T>(value: T): T` preserves a reference dependency through its owned generic parameter.
- Accept returning an existing `&User` from an owned `List<&User>` when checked against the inputs' dependencies. Moving or freeing the list storage must not erase the users' source dependencies.
- Reject returning `&local` from a function owning `local`, or returning a reference into locally owned list storage that will be freed.
- Reject `let view = identity(&shifted(Point { x: 1, y: 2 }));` followed by a later statement using `view`. The borrowed temporary cannot escape its statement. Borrowing the same temporary entirely within an expectation can be valid.
- Reject reading or mutating a whole value while a result retaining an exclusive dependency on that value is still needed. For example, after a schematic `editAndView(items: &mut List<i32>): &i32`, the returned read-only element reference does not automatically downgrade access to `items`.

## Callable capabilities and copying

These contracts must not be strengthened through annotations, returns, records, or branch joins:

```ts
function wrongMode(work: mut () => i32): () => i32 {
  return work; // Rejected: mutable invocation cannot become read-only invocation.
}

function wrongAccess(work: &(mut () => i32)): i32 {
  return work(); // Rejected: only read-only access to a mutable callable.
}

function wrongOwnership(work: &mut (once () => string)): string {
  return work(); // Rejected: exclusive access is not ownership for consumption.
}
```

Exclusive-capture duplication must remain rejected even when wrapped in a record or given an arrow annotation:

```ts
let mut counter: i32 = 0;
let access = &mut counter;
let tick = () => {
  *access += 1;
};
let duplicate = copy(tick); // Rejected: would duplicate the exclusive capture.
```

A closure owning an independent copied integer counter is a different case. Mutable invocation does not itself forbid a valid explicit environment copy. Copy evidence must survive the interface, or copying is rejected; a plain arrow type does not supply that evidence. No callable gains implicit copyability just through annotation.

## Generic and erased-callable evidence

The prototype's `requireNoBorrow<T>` deliberately requires `where noBorrow(T)`. Reject the following generic body before monomorphization, even if its current callers happen to use `i32`:

```ts
function unchecked<T>(value: T): T {
  return requireNoBorrow(value); // Rejected: unconstrained T may carry borrows.
}
```

An arrow's invocation capability is not evidence that its capture environment is borrow-free:

```ts
function launch(work: () => void): Routine {
  return spawn(work); // Rejected: the interface provides no noBorrow evidence.
}
```

Known concrete capture structure or explicit generic requirements can provide evidence. An annotation must never erase actual dependencies or invent a `noBorrow` guarantee. #23 must define callable requirements and signatures that preserve this evidence for `spawn` and waiting helpers. The checker must not wait until after monomorphization to validate them.

## Control flow, lifetime release, and cleanup

The RFC's future accepted/rejected fixtures must also cover:

| Case | Required outcome |
| --- | --- |
| A branch may retain a short-lived source in a longer-lived destination. | Preserve the possible dependency at the join; reject use after the source expires. |
| A loop may retain a different source on each iteration. | Account for dependencies at backedges; do not reset them at each iteration. |
| A clear-like call returns `void`. | Do not erase existing dependencies merely because of its name or by inspecting its body at the caller. |
| The relevant old value is replaced, destroyed or reaches its final possible use. | Release dependencies only once old references cannot be used, including by `dispose` or through another holder. |
| A replacement would leave references into freed old storage. | Reject it. Evaluating the RHS first does not make dangling references valid. |
| A binding containing a `Shared<T>` handle is mutable. | Do not grant ordinary `&mut T` access to the shared payload. |
| Cancellation arrives while old-value disposal suspends during replacement. | ADR 0009 defers delivery through installation while keeping the replacement owned and tracking initialized/disposed state. Deliver pending cancellation afterward against a valid slot and clean it up once. Deferred waits continue normally; requests do not abandon them. |

#28 must formalize the combined contracts, reborrowing and nested-reference obligations, generic/callable evidence, control-flow merges, and disposal-sensitive last use. [ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md) records #16's cancellation and synchronized-sharing policies; `04-concurrency-safety.md` adds guard/wait/native-operation cases. Hand-checking these lists does not make the remaining checking rules or runtime behaviors implemented or proven.
