# Clock and cancellation design evidence. #33

Read the source examples in [`prototypes/24-clocks-cancellation.rill`](../../prototypes/24-clocks-cancellation.rill).

[Contract](../clock-cancellation-contracts.md), [temporal values](../time-contracts.md), [ADR 0022](../adr/0022-jiff-time-and-explicit-cancellation.md). `24-clocks-cancellation.rill` has **26 intended-valid, hand-checked expectations**, not executed tests. It uses manual clocks and owned token state, not measured sleeps, browser probes, native clock support or a compiler/runtime implementation. #28/#30 remain checking/backend gates.

## Active inventory

| Cases | Intended facts |
| --- | --- |
| 1–4 | Token outlives disposed source; explicit copies share state/authority; independent versus never-token identities. |
| 5–9 | Child directionality, ancestor propagation, multiple-parent OR, late child of requested parent, empty links. |
| 10–13 | Armed/disarmed unique guard, independent persistent subscriptions/closed one-shot channel, repeated request. |
| 14–18 | Controlled logical advancement, rejected negative advance without mutation, domain mismatch, explicit clock copy identity and reported manual resolution. |
| 19–22 | Registration before advancement, past deadline, canceled-zero priority, invalid interval before cancellation and retained clock cause. |
| 23–25 | Expired helper skips owned callback invocation, completion with an unadvanced future timer, original operation error remains nested. |
| 26 | A 1,025-edge descendant chain requests without a source-level recursive algorithm or invalidating intermediate nodes. This is not measured stack-safety evidence. |

The expiry callback has an exclusive caller borrow. Its result is scalar; the helper must release that borrow through joined cleanup before the caller reads it. No noBorrow requirement is silently added to scoped F/R. Deep child replacement releases old authority while the next child retains needed parent state. No token callback/user code is invoked by cancel; no withCancellation helper is supplied.

## Deterministic event traces. Not an executable scheduler

### T1: selected operation, late helper return

1. Clock C/deadline D validated, now<D; admit F and timer.
2. F completes and its R is selected.
3. Request cancellation of losing timer work; withdrawal/cleanup must finish.
4. Clock reaches D before all cleanup finishes.
5. Return Ok(R), **not reclassified Expired**. Actual return time is not the selection event.

### T2: expiry, unabortable operation

1. F has committed external effects and native work still uses caller storage.
2. Timer is selected; helper records Expired and requests child cancellation.
3. Abort is requested but not confirmed. Keep every source/selected-result obligation alive.
4. Parent joins under deferred further delivery. No return deadline exists; this can wait forever.
5. Only completion/confirmed abort and cleanup permit Err(Expired). Nothing rolls back committed effects.

### T3: simultaneous readiness

Operation completion and expiry both ready before selection: either outcome allowed. Same for a token-aware wait where token request and due clock are ready. Exactly one commit; losing messages/results/registrations dispose correctly. Do not copy Tokio run-until completion bias or promise strict completion timestamps.

### T4: token state, explicit outcome, intrinsic cancellation

1. Source requests token. cancel returns after signal propagation; operations may still run.
2. Plain code that does not observe that token continues (ordinary budget-point goroutine cancellation remains independent).
3. Token-aware pending wait selects cancellation, confirms release, returns Err(WaitError.Cancelled) as normal control flow.
4. A caller can handle that error and continue. No catchable panic, hidden goroutine attachment or generic token scope exists.
5. stop on the caller instead performs existing intrinsic request-and-join. It is not returned as WaitError.

### T5: mask boundary

A disposal/required join masks intrinsic delivery, not explicit state observation. A token query stays true; a signal wait can return normally. An explicitly token-aware operation can return its ordinary cancellation outcome only after its own safe release. Required native cleanup cannot use the request as an excuse to abandon a buffer/handle/join. Use the never token for cleanup that must ignore external token requests. No public unmask operation.

### T6: linked lifetime and request races

Create a child while parent request propagates: it must observe requested or register before the relevant propagation boundary, never miss the signal. Parent true/child false may be visible during propagation; cancel return guarantees affected requests, not a tree snapshot or task completion. A second cancel cannot merely see the parent's flag and return while required descendant propagation is unfinished. Child/OR graph storage and release cannot recurse unboundedly, leak permanent internal cycles or retain dangling source descriptors.

### T7: defaults, manual authority and failure

System factory handles share one runtime domain with suspend-inclusive, rate-correctable nondecreasing readings. Missing profile is Err, not wall/exclusive fallback. Manual controller is separate authority over its shared Clock; advance marks eligible without running user jobs. Dropping all controllers cannot fake time or completion. Native timeline regression poisons the domain; invalid manual movement leaves it unchanged. Domain/range checks do not saturate.

## Independent boundary cases

- Full i128 coordinate endpoints: checked shift overflow, same-domain negative differences, differences outside SignedDuration range, cross-runtime/domain comparisons, ID exhaustion without reuse. There is no instant parser/numeric cast/persistence protocol.
- Positive interval below host resolution rounds waiting upward; host early/spurious callback rechecks selected now. Native-counter fractional conversion floors, unrelated to upward waiting conversion. Repeated readings are legal. Native reported resolution may differ from accuracy; unknown is None, not 1ns inferred from representation.
- OS suspend counts but does not grant wake permission. Context inactivity, scheduler delay, wall steps/rate corrections and cleanup delay are distinct. Browser/WASI capability names do not establish this contract; keep targets unsupported until verified. A readable clock alone does not validate its timer path.
- Relative timeout anchors once. No wall alarm or implicit Timestamp→Instant conversion. Sampling two clocks is not atomic. Nested operations forward the same deadline rather than resetting the budget.
- Typed wait preflight: invalid negative/domain first, then already-requested token (including expired/zero), then clock setup. Later readiness/failure ties have no priority guarantee. Notification factory error is one immediately eligible result, not nested factory Result or silent EOF.
- Last receiving owner disposes uncommitted timer/subscription registration safely. A committed message is not undone; multiple independent subscriptions each notify, but copies of one Receiver consume one message normally. Re-subscribing after cancellation observes the persistent state.
- Ordinary source/token/clock/Routine disposal does not cancel/join peers. CancelOnDispose owns authority and is unique/disarmable, not a mutex guard or general join guard. An empty user wrapper disposer cannot suppress its required remaining-field action.
- Root authority gone does not dangle surviving tokens. Linked tokens retain needed state after source descriptors die; cancelling a child cannot request parents/sibling branches. Never token stays never, not a default fresh cancelable source. One-shot state never resets.
- No arbitrary reasons, public callbacks/ExecutionContext, mutable link attachment, source CancelAfter, token inheritance or public mask control. Generic withCancellation and renamed equivalent/combined helpers remain excluded.
- withDeadline preserves R, including Result<T,E>, typed error origin and valid caller dependencies. Reject callable/result transfers missing sendability, erased capture assumptions, owned-closure-local views, and freeing a caller source before losing native work/cleanup finishes.
- Legacy error-free sleep/after cannot report capability failures: panic rather than wrap, false success, unexplained close or a deliberately never-ready unsupported request. Fatal panic/allocation/host traps are not caught as typed cancellation; no normal cleanup guarantee on panic is introduced.

## Validation status

Static local links/fences/whitespace/source widths/delimiters and the 26-case inventory passed, alongside the companion's independent arithmetic/calendar checks and preserved JSON/Serena hashes. Read-only consistency review found no blocking issue; the temporal option-home omission was fixed. No scheduler, token, timer, clock or Rill expectation ran. Event traces remain inspection cases, not measured outcomes.

## Rejection examples and implementation gates

Expected source rejections: cancel(&token) (observation is not authority); implicit copy of source/token/clock/guard; public raw state/identity construction; returning views into local handle/config slots; background borrowed captures lacking noBorrow; arbitrary F erasure followed by asserted properties; suppressing protected guard/registration release; string-tag select or an unavailable withCancellation call. These are hand-checked expectations, not compiler diagnostics or compile-fail execution.

#28 must supply callable/callee-exit/transitive-source checks, once-only ownership/commit states, intrinsic-versus-typed outcomes, concurrent DAG publication/helping, factory/subscription races, ID exhaustion, backend-capacity translation and bounded cleanup. #30 separately validates actual native/Wasm/import/tail/pointer/clock mechanisms. Existing topology research does not prove those rules; return an unworkable necessary rule to the map. No benchmark, hard timing bound or runtime safety proof follows from these fixtures.
