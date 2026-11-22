# Concurrency and cancellation safety cases

Read [the source examples](../../prototypes/04-concurrency-safety.rill). They combine the earlier channel/worker demonstration with the guard and cancellation cases from ri-40y and [ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md).

The source contains 11 intended-valid expectations. They have been inspected, not executed by a Rill compiler or runtime. Library names and call shapes follow the [published catalogue](../standard-library-contracts.md). These cases do not prove scheduling, synchronization or cancellation safety.

## Source coverage

The first seven expectations cover scoped mutation, early unlocking, last-use release, read-only guard sharing, disjoint payload loans, suspension while holding a lock and explicit background completion. The remaining four cover a producer and two workers, channel/timer selection, heterogeneous plain results and the fallible scoped helpers.

`queuedSquares` transfers owned messages through unbuffered queues and shares a receiver explicitly. Sender disposal closes each queue. The parent drains results and joins all three routines. `selectedPulse` accepts either a heartbeat or a timer; it never asserts a scheduling winner or a completion deadline. `helperOutcomes` distinguishes `join`, `all`, `allSettled`, `race` and `any` without guessing execution order.

## Guard-dependent access

A scope exit or explicit disposal unlocks the mutex. A dependent loan must reach its last use first. Reject this independent example:

```rill
let mut held = lock(&counter);
let value = &mut *held;
dispose(held); // Rejected: value retains the guard's protected-access obligation.
*value += 1;
```

A guard borrows its mutex source; it does not clone a shared owner. Reject returning a guard whose source handle is local:

```rill
function invalidGuard(): LockGuard<i32> {
  let counter = shared(mutex<i32>(0));
  return lock(&counter);
}
```

A payload reference also depends on the lock lease. Keeping the allocation alive is not sufficient:

```rill
function invalidView(counter: &Shared<Mutex<i32>>): &i32 {
  let held = lock(counter);
  return &*held; // Rejected: held would unlock on return.
}
```

A returned guard may borrow a caller-owned mutex. A returned payload view may borrow a caller-owned guard. In both cases the caller must retain every source and protected-access obligation through the result's last use, including transitive holders and disposer uses.

Reject `copy(&held)`, moving a non-copying payload through `*held`, constructing a guard through private fields or granting built-in dereference to an unrelated record named LockGuard. [ADR 0010](../adr/0010-file-modules-explicit-exports-and-privacy.md) protects required guard release from overload interception. Owning a nested Payload does not authorize replacing a genuine guard's disposal. Ordinary generic defaults remain specializable; there is no blanket outermost-type placement rule. Payload replacement still follows borrow and exactly-once disposal rules. Use `(*held).field` for payload projection; these examples add no user-defined dereference mechanism.

## Guard ownership versus scoped loans

- `sharedGuardReads` permits read-only scoped sharing because i32 is shareable. Children cannot mutate or unlock the guard, or take ownership of it.
- `scopedPayloadWrites` transfers disjoint exclusive payload loans to children. The guard stays in the parent, which joins all child work and cleanup before releasing it.
- Reject moving an owned guard to another goroutine, including through a scoped helper or wrapper. It is not sendable. Background capture also requires noBorrow.
- A read-only view of `&mut T` cannot copy that reference or obtain competing mutable access. Sharing a handle does not duplicate exclusivity.
- The owning goroutine may suspend or migrate between OS workers while holding its guard. This does not transfer the guard to another goroutine or require thread pinning.

A parent that retains a lock while joining a child waiting for that lock can deadlock. The runtime cannot fix that by unlocking early or abandoning the child. Reacquisition by the same goroutine instead follows the self-lock panic policy.

## Payloads, messages and generic evidence

Reject `mutex<&i32>(&source)` and `channel<&i32>(0u64)`. Payloads and messages require noBorrow and sendable. Scoped inputs/results and ordinary reference-containing records remain available. A sendable but non-shareable owned value may be protected by a mutex or transferred as a message. Synchronized endpoints do not grant shared access to that message.

The RFC needs accepted and rejected generic cases for structural or explicit `sendable(T)` and `shareable(T)` evidence. Eventual instantiations are not evidence for an unchecked generic body. Callable erasure cannot invent capture properties; noBorrow, copy and invocation mode do not substitute for transfer or sharing requirements. Cover goroutine-affine guards and recursive derivation through records, tuples, lists, enums and captures.

Scoped results may retain valid caller-owned borrows, but cannot borrow destroyed child-local storage. Preserve exclusive restrictions through the result's last use. Selecting a winner does not permit guard release while any child or cleanup still uses it.

## No hidden named-function captures

Reject access to an outer or top-level runtime binding from a named function:

```rill
let mut counter: i32 = 0;
function hiddenMutation(): void {
  counter += 1;
}
```

The restriction also covers immutable or constant-foldable runtime lets, indirect calls and dispose implementations. Types and function declarations remain visible under module rules. Use explicit parameters or checked arrow captures for state. Logging and other trusted runtime services need synchronization contracts, not an unchecked user-global escape.

## Runtime and cancellation state-machine cases

These are obligations for later executable tests, not results from this source. Timing alone cannot establish ownership or termination.

| Case | Required outcome |
| --- | --- |
| A lock acquisition contends. | Park the goroutine, not its OS worker. |
| A lock or message handoff publishes prior writes. | Establish memory visibility as well as exclusion. Owner counts alone grant no mutable payload access. |
| Cancellation races with lock acquisition or guard initialization. | Withdraw before grant, or release a granted acquisition through registered cleanup. Keep one release owner; never leak or double-unlock. |
| A guard leaves scope or is explicitly disposed. | Unlock exactly once without disposing the payload. Its last explicit mention is not an early-unlock instruction. |
| A goroutine reacquires its mutex or stops itself. | Panic under ADR 0005 rather than wait for itself. |
| Stop calls repeat or run concurrently. | Share termination state and perform cleanup once. Every completed join observes termination. Dropping the handle does not stop the target. |
| A helper selects a winner or all fails early. | Cancel and join remaining children and cleanup before returning. Selection time is not a hard return deadline. |
| Cancellation arrives during a scoped helper or stop's required join. | Retain sources and join the children or target. Defer delivery during required cleanup instead of abandoning the wait. |
| A request arrives during dispose or another deferred wait. | Keep the request pending and the wait registered. Scheduling and completion continue while delivery is disabled. |
| Cancellation interrupts destination or replacement-expression evaluation. | Clean up values owned at that point. The old storage remains valid. |
| Cancellation arrives during old-value disposal or installation. | Defer delivery through installation, track initialized/disposed state and clean up the valid replacement once if delivery follows. |
| A disposer or child still needs a protected loan. | Retain the guard and its mutex source through those uses and joins. |
| A native job still uses a buffer, resource, callback or handle. | Retain its sources until completion or confirmed abort. An abort request is not a release acknowledgement. |
| Send or receive races with cancellation. | Maintain one owner for pending messages and committed results. Do not roll back a committed transfer; a canceled committed receive may dispose its result. |
| Select has multiple registrations. | Commit one receive and withdraw the rest. Racing independent receives can consume losing messages and is not equivalent. |
| Cleanup or native work never completes, or a wait cycle exists. | No bounded stop/helper latency or force-kill guarantee applies. |
| Top-level execution ends normally or panics. | Do not join background work implicitly. Panic has no cleanup guarantee and is distinct from cooperative stop. |

Cancellation cleanup needs compiler/runtime paths distinct from fatal panic, including initialization tracking, dependency-sensitive disposal and protected guard release. The no-panic-unwinding rule does not remove cooperative cleanup. The catalogue defines public signatures and protected operations; ri-48g still owes formal checking and implementation validation. Return a blocking inconsistency to the language map rather than treating these examples as proof.

The guard API replaces the old callback-only withLock proposal. No callback-result noBorrow rule or callback-scoped mutex compiler contract remains.
