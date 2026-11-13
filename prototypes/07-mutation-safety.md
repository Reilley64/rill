# Mutation safety examples

Design cases for [#15](https://github.com/Reilley64/rill/issues/15) and [ADR 0007](../docs/adr/0007-explicit-mutation-with-conservative-borrow-dependencies.md). The expected outcomes below have been checked against the approved design by inspection, **not by a compiler**. They are inputs to #28's explicit checking rules and eventual compile-pass/compile-fail tests, not evidence of a soundness proof.

`07-mutation.rill` supplies complete intended-valid examples with independent top-level expectations. Snippets here are separate cases, not one program. Library/body-less signatures are schematic contracts; #23 still owns their complete standard-library definitions. An executable fixture suite must make those dependencies explicit.

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

The prototype's complete `remember(slot: &mut &Point, point: &Point)` and `retainedInput` examples intend to accept storing a caller-provided reference through a mutable slot whose sources remain valid. Forwarding caller-provided `dst` and `user` to the schematic list-based `remember` can also be valid: preserve that possible flow in the forwarding function's contract. A temporary reborrow ending does not remove dependencies written into the original destination. Do not conflate the outer container access loan with the sources referenced by its elements.

## Results, temporaries, and exclusive restrictions

- **Accept:** `carriedBorrow` in the prototype, where `identity<T>(value: T): T` preserves a reference dependency through its owned generic parameter.
- **Accept when checked against the inputs' dependencies:** returning an existing `&User` from an owned `List<&User>`. Moving/freeing the list storage must not erase the users' source dependencies.
- **Reject:** returning `&local` from a function owning `local`, or returning a reference into locally owned list storage that will be freed.
- **Reject:** `let view = identity(&shifted(Point { x: 1, y: 2 }));` followed by a later statement using `view`; the borrowed temporary cannot escape its statement. Borrowing the same temporary entirely within an expectation can be valid.
- **Reject:** reading or mutating a whole value while a result retaining an exclusive dependency on that value is still needed. For example, after a schematic `editAndView(items: &mut List<int>): &int`, the returned read-only element reference does not automatically downgrade access to `items`.

## Callable capabilities and copying

These contracts must not be strengthened through annotations, returns, records, or branch joins:

```ts
function wrongMode(work: mut () => int): () => int {
  return work; // Rejected: mutable invocation cannot become read-only invocation.
}

function wrongAccess(work: &(mut () => int)): int {
  return work(); // Rejected: only read-only access to a mutable callable.
}

function wrongOwnership(work: &mut (once () => string)): string {
  return work(); // Rejected: exclusive access is not ownership for consumption.
}
```

Exclusive-capture duplication must remain rejected even when wrapped in a record or given an arrow annotation:

```ts
let mut counter = 0;
let access = &mut counter;
let tick = () => {
  *access += 1;
};
let duplicate = copy(tick); // Rejected: would duplicate the exclusive capture.
```

A closure owning an independent copied integer counter is a different case: mutable invocation does not itself forbid a valid explicit environment copy. Copy evidence must survive the interface, or copying is rejected; a plain arrow type does not supply that evidence. No callable gains implicit copyability just through annotation.

## Generic and erased-callable evidence

The prototype's `requireNoBorrow<T>` deliberately requires `where noBorrow(T)`. Reject the following generic body before monomorphization, even if its current callers happen to use `int`:

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

Known concrete capture structure or explicit generic requirements can provide evidence. An annotation must never erase actual dependencies or invent a `noBorrow` guarantee. #23 must define callable requirements/signatures that preserve this evidence for `spawn` and waiting helpers, rather than validating them only after monomorphization.

## Control flow, lifetime release, and cleanup

The RFC's future accepted/rejected fixtures must also cover:

| Case | Required outcome |
| --- | --- |
| A branch may retain a short-lived source in a longer-lived destination. | Preserve the possible dependency at the join; reject use after the source expires. |
| A loop may retain a different source on each iteration. | Account for dependencies at backedges; do not reset them at each iteration. |
| A clear-like call returns `void`. | Do not erase existing dependencies merely because of its name or by inspecting its body at the caller. |
| The relevant old value is replaced/destroyed or reaches its final possible use. | Release dependencies only once old references cannot be used, including by `dispose` or through another holder. |
| A replacement would leave references into freed old storage. | Reject it. Evaluating the RHS first does not make dangling references valid. |
| A binding containing a `Shared<T>` handle is mutable. | Do not grant ordinary `&mut T` access to the shared payload. |
| Cancellation arrives while old-value disposal suspends during replacement. | Pending #16: specify ownership and initialization-state cleanup without leaks, double disposal, or use of an uninitialized destination. Do not assume cancellation masking. |

#28 must formalize the combined contracts, reborrowing and nested-reference obligations, generic/callable evidence, control-flow merges, and disposal-sensitive last use. #16 must resolve the explicit cancellation and synchronized-sharing handoffs. Hand-checking this list does not make those remaining checking rules or runtime behaviors implemented or proven.
