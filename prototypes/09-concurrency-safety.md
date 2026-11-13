# Concurrency and cancellation safety cases

Inputs for [#16](https://github.com/Reilley64/rill/issues/16) and [ADR 0009](../docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md). These are expected outcomes checked by inspection, not an executed compiler/runtime suite or a soundness proof. `09-concurrency-safety.rill` supplies seven intended-valid top-level expectations; its new library module/factory spellings are provisional for #23.

Snippets below are independent cases using the corresponding prototype's types/helpers. Named functions receive state explicitly; record values use the named construction rule from #21.

## Guard-dependent access

Accept normal scope-exit unlocking and explicit early disposal after the last dependent use. Reject early disposal while a protected loan is still needed:

```ts
let mut held = lock(&counter);
let value = &mut *held;
dispose(held); // Rejected: value retains the guard's protected-access obligation.
*value += 1;
```

The guard borrows the mutex source; it does not secretly clone a shared owner:

```ts
function invalidGuard(): LockGuard<int> {
  let counter = shared(mutex<int>(0));
  return lock(&counter); // Rejected: the source handle is local.
}
```

A payload reference depends on the lock lease, not just on the allocation holding the payload:

```ts
function invalidView(counter: &Shared<Mutex<int>>): &int {
  let held = lock(counter);
  return &*held; // Rejected: held would unlock on return.
}
```

Accept returning a guard borrowing a caller-provided mutex when its ordinary input dependencies remain valid. Accept returning a payload borrow from a function borrowing a caller-owned guard when the guard remains needed for that borrow. Guard disposal/movement and source cleanup must remain restricted through all transitive holders and disposer uses.

Reject `copy(held)`, moving a non-copying payload out with `*held`, forging a guard through private fields, or granting built-in dereference to an unrelated record merely named `LockGuard`. [ADR 0010](../docs/adr/0010-file-modules-explicit-exports-and-privacy.md) additionally protects required built-in release/ownership mechanics from overload interception: owning a nested Payload cannot authorize replacement of a genuine guard's required disposal. Ordinary generic defaults remain specializable; this is not a blanket outermost-type placement rule. Payload replacement is allowed only under the existing borrow and exactly-once disposal rules. Use explicit `(*held).field` in these examples; no additional user-defined dereference/projection mechanism is being specified.

## Guard ownership versus scoped loans

- **Accept:** `sharedGuardReads`, since `LockGuard<int>` is shareable through read-only scoped access. This does not permit child mutation, unlocking, or ownership transfer.
- **Accept:** `scopedPayloadWrites`, where disjoint exclusive payload loans transfer to children and all execution/cleanup finishes before guard release. The guard itself stays in the parent goroutine.
- **Reject:** transferring an owned guard to another goroutine, even via a scoped helper or wrapper; it is not sendable. Background spawn also fails its borrow-free capture requirement.
- **Reject:** a read-only shared view of `&mut T` used to move out/copy that reference or obtain competing mutable access. Q11's sharing rule is not a way to duplicate exclusivity.
- **Accept:** suspension/worker migration of the goroutine owning a guard. This does not transfer ownership to another goroutine or require OS-thread pinning.

A child waiting for a lock retained by a parent that is joining that child can deadlock. That is not permission to unlock early or abandon the child. A same-goroutine reacquisition instead follows the specified self-lock panic policy.

## Payloads, messages, and generic evidence

Reject `mutex<&int>(&source)` and `channel<&int>()`: v0 payload/message types require noBorrow and sendable. Ordinary reference-containing records and scoped inputs/results remain available. A sendable but non-shareable owned value can be protected by a mutex or transferred as a channel message; synchronized endpoint use does not share access to an individual message.

The RFC must include accepted/rejected generic cases establishing `sendable(T)`/`shareable(T)` from structure or explicit requirements, not eventual instantiations. Erased callable annotations cannot invent capture evidence, and noBorrow/copy/invocation mode cannot substitute for transfer or sharing requirements. Test trusted exceptions such as goroutine-affine guards as well as ordinary recursive derivation through records, tuples, lists, unions, and captures.

Scoped helper results may retain valid caller-owned borrows. Reject results borrowing destroyed child-owned storage, and preserve any exclusive restriction until the returned dependency's last use. A guard cannot be released merely because a helper has selected a winner; all child work and cleanup using it must finish first.

## No hidden named-function captures

Reject access to outer/top-level runtime bindings from a named function:

```ts
let mut counter: int = 0;
function hiddenMutation(): void {
  counter += 1; // Rejected: named functions do not implicitly capture runtime lets.
}
```

The same restriction applies to immutable runtime lets even if constant-foldable, indirect calls, and dispose overloads. Types and function declarations remain visible under module rules. Explicit parameters and checked arrow captures are allowed. Trusted runtime services such as logging need synchronization contracts, not an unchecked user-level global-state escape hatch.

## Runtime and cancellation state-machine cases

#23/#28 must make these cases explicit and eventually executable. No timing-only tests may stand in for ownership/termination assertions.

| Case | Required outcome |
| --- | --- |
| Contended lock acquisition. | Park the goroutine, not its OS worker. |
| Lock release/acquisition or message handoff publishes prior writes. | Justify synchronization visibility/publication as well as exclusion; owner counts alone do not grant mutable payload access. |
| Cancellation races with lock acquisition/guard initialization. | Own release responsibility throughout the handoff: withdraw before grant, or release a granted acquisition through initialized/registered cleanup. Never leak or double-unlock. |
| A guard leaves scope or is explicitly disposed. | Unlock exactly once, without disposing the payload; do not unlock merely at its last explicit mention. |
| Same-goroutine mutex reacquisition or self-stop. | Panic under ADR 0005; no self-wait. |
| Concurrent/repeated stop. | Shared termination state, cleanup once, all completed joins observe termination. Freeing the handle still does not stop the target. |
| A helper selects a winner or all fails early. | Cancel/join remaining children and cleanup before return; selection time is not a hard return deadline. |
| Cancellation arrives during a scoped helper or stop's required join. | Retain sources and join children/target; defer further delivery during required cleanup rather than abandon the wait. |
| A cancellation request arrives during dispose or another deferred wait. | Keep it pending. Do not withdraw that operation's wait while delivery is disabled; scheduling/completion continue. |
| Cancellation during destination/RHS evaluation. | Clean up values actually owned at that point; old storage is still valid. |
| Cancellation during old-value disposal or installation. | Defer delivery through installation; keep the replacement owned, track initialized/disposed state, then clean up the valid installed value once if cancellation is delivered. |
| A disposer or child still needs a protected loan. | Preserve the guard and its mutex source through those uses and joins, then release it. |
| A native blocking job still uses a buffer, resource state, callback data, or handle. | Retain the required source until completion or confirmed abort; an abort request alone cannot permit cleanup. |
| A send/receive races with cancellation. | One owner for pending messages and committed results. Do not roll back a committed transfer; a canceled committed receive may dispose its result. |
| select has multiple registrations. | Exactly one winning commit; withdraw the rest. A separate race of receives still may consume losing messages. |
| Cleanup/native work never completes or a wait cycle exists. | No bounded stop/helper latency or force-kill guarantee. |
| Normal top-level completion or panic. | No implicit global background join; panic has no cleanup guarantee. Do not confuse either with cooperative stop. |

Cancellation cleanup must have compiler/runtime paths distinct from fatal panic, with initialization state and dependency-sensitive disposal/guard release. The existing no-panic-unwinding policy is not an excuse to skip cooperative cleanup. ADR 0010 records #17's module/scoping/privacy policy; #23 still owns complete signatures, protected-operation contracts, and evidence-preserving interface rules; #28 must resolve any v0-blocking checking/commit inconsistency rather than declare this list a proof.

The guard interface supersedes the earlier callback-only withLock proposal: there is no special callback-result noBorrow rule or callback-scoped mutex compiler contract left to implement.
