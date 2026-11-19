# Names for scoped joins

Rename `std::routine::all` to `tryJoin`. Keep `join` unchanged and remove `allSettled` from v0. This recommendation changes names, not cancellation or error policy.

## Rill contract

The [routine contract](../standard-library-contracts.md#6-background-and-scoped-work) accepts tuples of two to four jobs. The table shows the two-job forms.

| Current function | Job results | Return type |
| --- | --- | --- |
| `join` | `R1`, `R2` | `[R1, R2]` |
| `all` | `Result<T1,E>`, `Result<T2,E>` | `Result<[T1,T2],E>` |
| `allSettled` | `Result<T1,E1>`, `Result<T2,E2>` | `[Result<T1,E1>, Result<T2,E2>]` |

`join` returns each result unchanged in input order. `R1` and `R2` can be different `Result` types, or only one can be a `Result`. An `Err` does not cancel siblings. `allSettled` gives the same result as `join` on the same jobs. Its only extra check is that each job returns `Result`.

`all` requires a common `E`. It selects the first observed `Err` unchanged and requests cancellation of unfinished siblings. If all jobs succeed, it returns their values in input order. [ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md#scoped-work-and-stopping) and the routine contract require all scoped helpers to join child execution and cleanup before return. If cancellation reaches the helper, it cancels the children and waits for their cleanup. Early error selection does not put a time limit on return.

## Prior art

- Tokio's [`join!`](https://docs.rs/tokio/1.54.1/tokio/macro.join.html) keeps each output, even an `Err`. [`try_join!`](https://docs.rs/tokio/1.54.1/tokio/macro.try_join.html) returns one `Result` with a tuple of successes or the first error. The [macro stops polling pending branches](https://docs.rs/tokio/1.54.1/src/tokio/macros/try_join.rs.html#228-272) and drops their futures. Both macros run futures in one task. Tokio [detaches spawned tasks when their handles drop](https://docs.rs/tokio/1.54.1/tokio/task/struct.JoinHandle.html), so this name does not imply Rill's cleanup guarantee.
- JavaScript's [`Promise.all`](https://tc39.es/ecma262/multipage/control-abstraction-objects.html#sec-promise.all) fulfills with an input-order array or rejects on the first input rejection. It neither cancels nor waits for the other inputs on rejection. [`Promise.allSettled`](https://tc39.es/ecma262/multipage/control-abstraction-objects.html#sec-promise.allsettled) waits for all input promises and stores their `status` and `value` or `reason` in input order. JavaScript needs this conversion because rejection is separate from the fulfillment value. Rill jobs already return `Result` values.
- [Swift task groups](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0304-structured-concurrency.md#task-groups-and-child-tasks) provide a closer comparison for scope exit. If an error leaves the group body, Swift cancels remaining children and waits for them before it propagates the error.

## Use and migration

Use `tryJoin` for two independent lookups that must both succeed to serve a request. Use `join` when a handler can return a partial response and needs each lookup's distinct outcome. The handler still waits for both jobs. `tryJoin` names the fallible form of `join` and follows `tryInto` and `tryFrom`. Keep the names separate to avoid dispatch on job return types, as [ADR 0015](../adr/0015-language-required-library-contracts.md#decisions-and-consequences) requires.

After approval, replace `all` imports and calls with `tryJoin`. Replace `allSettled` with `join` without changing the jobs or result handling. Update the contract and the [concurrency](../../prototypes/04-concurrency-safety.rill), [library](../../prototypes/09-library-contracts.rill) and [batch report](../../prototypes/10-batch-report/main.rill) examples. The batch report already uses `join` with one `Result` job. Skip aliases in v0. `allSettled` offers a familiar spelling and a narrower type check, but saves no code.

## Checks to add

The prototypes are design cases, not runtime evidence. Keep the common-error success case and the distinct-error case. Check input order with reverse completion order. Use synchronization to check that `join` lets a sibling finish after an `Err`. Check that `tryJoin` returns the selected `Err` unchanged after it requests sibling cancellation and joins cleanup. Cancel the caller during cleanup. Check that the helper still joins. Do not assert a winner when failures are ready together or use elapsed time to infer cleanup.
