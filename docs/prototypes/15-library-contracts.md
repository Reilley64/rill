# Standard-library contract design checks (#23)

Read the source examples in [`prototypes/15-library-contracts.rill`](../../prototypes/15-library-contracts.rill).

Sources: [ADR 0015](../adr/0015-language-required-library-contracts.md) and the [public interface catalogue](../standard-library-contracts.md). `15-library-contracts.rill` contains **35 intended-valid expectations**. They are hand-checked design cases, not compiled/executed tests, scheduler measurements, a checker proof or accepted RFCs. Native/concurrent cases do not establish that the backend gate #30 has passed.

## Intended-valid inventory

| Expectations | Hand-checked contract |
| --- | --- |
| 1 | Explicit copy borrows its source; ordinary string comparison observes both owners. |
| 2–3 | Consuming/borrowed map, contextual callback access, independent source retention. |
| 4 | Explicitly borrowed repeatable mutable callback retains its state across maps (1,3,6 then 10; sums 10+10). |
| 5 | Exclusive element access ends before the next iteration. |
| 6–8 | Map replacement returns 7, lookup sees 9, removal leaves zero entries. |
| 9–10 | Consuming Option/Result maps invoke success callbacks once and preserve types. |
| 11–12 | Explicit caused-by wrapping retains one node; mapErr changes payload without dropping that node. |
| 13–15 | Valid borrowed UTF-8, invalid prefix diagnostic, explicit U+FFFD lossy replacement. |
| 16–18 | Scalar rather than byte traversal; é is U+00E9; surrogate input is rejected. |
| 19–23 | Out-of-range conversion, truncate-before-check -0.5 to u8, strict parse, wrapping and saturation. |
| 24–25 | Counted owner copying is not payload copying; guard disposal enables subsequent locking. |
| 26–28 | Buffered drain/closure, failed-send ownership recovery, exactly one ready selection. |
| 29–32 | Plain/fallible joining, heterogeneous settled errors, complete any-failure aggregate. |
| 33 | Fixed-arity sized spawning with explicit once/noBorrow/sendable evidence; not a stack-capacity measurement. |
| 34 | A nominal user type extends canonical iterate/next without a trait. |
| 35 | Custom wrapper disposal updates caller state and still releases its remaining owned guard. |

The quiet sender in case 28 is explicitly disposed after selection so it remains alive during selection; otherwise early last-use disposal could close it and introduce a second ready case. No test relies on ready-case ordering, fairness, hard timeouts or implicit exit joins.

## Independent rejection/adversarial cases

These are separate cases, not additional active declarations in the valid file. Exact diagnostic wording belongs to #24.

### Callable evidence and access

1. Reject a generic spawn wrapper that takes only `work: () => void` and assumes noBorrow/sendable environment evidence. Accept generic F with callableOnce(F,[],void), noBorrow(F), sendable(F).
2. Reject map with an actual once-only callable, even if a particular input list happens to have one element. Static callable requirements, not runtime length, determine eligibility.
3. Reject invoking a mutable-only callback through &F; accept explicit &mut F with repeatable capability. Borrowing a once-only callable mutably does not permit consuming its owned captures.
4. Reject returning references into the locally owned callback environment or into an owned list element that the operation destroys. Accept valid external references already carried by owned elements and borrowed-source results when the existing dependency rules establish them; #28 must specify those contracts, not body-inspect callers or impose blanket noBorrow on all map outputs.
5. Reject annotation/record/enum/return joins that duplicate exclusive captures, strengthen invocation guarantees or fabricate missing property evidence. Read-only invocation remains unrelated to purity.
6. Validate `map(show)` with canonical overloaded show and known input access; unresolved context requires annotation, not a guessed callable or delayed instantiation failure.

### Collection ownership, keys and iteration

7. Reject ordinary owned `copy(list)`; use copy(&list). A value already typed &T is already the borrowed argument. There is no blanket textual rule that every expression named copy needs another ampersand. Shared copy increments resource ownership; guard copy remains unavailable.
8. Reject attempts to mutate a stored map key or change the collection structure through an active iterator/element loan. A logical eq/hash contract violation must not authorize memory corruption.
9. Reject advancing a mutable iterator while its previous exclusive result will still be used. Accept one-at-a-time use. No user-level arbitrary-index disjointness proof or mutable stored-key access follows from this API.
10. Reject returning a reference into a body-local list/map/view or iterator descriptor slot after its owning storage ends. Distinguish a source-derived view copied through a local descriptor from a reference to that descriptor slot.
11. Reject an unconstrained empty literal/factory requiring unknown element/key/value types or numeric widths; no later-use look-ahead. Validate tuple/list literal contextualization and generic iterate/next output inference explicitly before treating these source examples as compiler-ready.
12. Reject a standard-library implementation supplied only by differing return types/where conditions on the same canonical parameter patterns. Owned/borrowed map has distinct input patterns; all has no implicit plain-result overload.

### Protected cleanup and ownership state

13. In a module defining Payload, reject a specialization that displaces genuine dispose(LockGuard<Payload>) or count-preserving Shared/Sender operations. Normalize aliases/type renaming and check even without calls. Reject negative-capability bypasses through generics, stored function values or erased interfaces; ordinary less-specific defaults must not win over a protected action.
14. A custom wrapper disposer that ignores its guard/native/owner field still triggers remaining-field cleanup. Moving a field or explicitly disposing it must mark it consumed so cleanup does not repeat it. Do not recursively invoke the wrapper disposer on its own consumed parameter. Test partial initialization, nested wrappers, branches and replacement.
15. Reject guard copy/ownership transfer, disposing a guard with live protected views, forged standard identities or general user-defined Deref. Read-only sharing is distinct from transferring unlock responsibility.
16. Cancellation cannot free stack/source/registration state on request alone; wait for actual execution/cleanup and native completion/confirmed abort. An abort request is not confirmation. Include acquisition-to-guard handoff, masking, publication and completion races. Wasm owner retention alone does not prove host-pointer stability across memory.grow (#30).
17. Standard remaining-field/cause/container cleanup and iteration must bound call-stack growth per #22, preserving existing order/initialization/dependencies/masking. Worklist allocation failure is fatal, not recursive fallback or recoverable partially completed disposal. User callbacks/disposers are outside the standard recursion bound.

### Error representation and causes

18. Accept a primary borrowed error with display and valid caller dependencies. Reject retaining it as a heterogeneous cause when noBorrow fails. Even a static read-only view does not satisfy noBorrow. Plain propagation may remain valid; explicit conversion to an appropriate owned error can make causal wrapping valid.
19. Reject retaining an affine/non-sendable/non-shareable payload as an erased cause, independently of the outer error's properties. Check noBorrow/sendable/shareable/display before monomorphization; runtime metadata is not permission to forge generic evidence.
20. Reject an unavailable display implementation, implicit string-to-Error conversion, implicit typed-error conversion, reflected formatter fallback or copying an opaque unique cause. Accept Err(error("text")) for the default Error and Err("text") only with a compatible string error contract.
21. Postfix propagation, returns and map/andThen preserve the exact wrapper, origins and chain. mapErr preserves envelope metadata while transforming E. Infix wrapping evaluates the RHS only on failure, once, returns early and records one new construction location. Test skipped moves, suspension/cancellation and source temporary cleanup. No automatic propagation trace.
22. Direct Err construction through an alias has the same call-site attribution and retention constraints. Adapt it with an arrow for first-class use; do not add variable-arity ordinary functions. Constructing Result.Err requires a valid opaque wrapper, not forged fields. Borrowed Result inspection must not clone its cause to return an owned failure.
23. Standard formatting visits the entire explicit chain through each payload's display, with owned output and bounded standard traversal; a user formatter may suspend/effect. Do not truncate the chain, drop an active guard early, reflect private fields or reconstruct eliminated proper-tail frames. Human text is not a machine-readable error protocol.

### Concurrency and channel commits

24. Reject scoped jobs/results lacking required transfer evidence; scoped helpers can preserve valid caller-dependent borrows, but never return child-local references. Owned captures can be consumed once; reused read-only/mutable closures require the appropriate explicit borrowing/access evidence.
25. Reject plain jobs passed to all; use join. Reject mismatched common errors/success types where required; use explicit enum/default-error wrapping, not inferred unions. allSettled preserves its per-position error types. All losing work is canceled/joined as applicable before return or cancellation completion; fatal panic is not caught as Result.
26. Reject no-argument channel, optional/named sized-spawn arguments, unsupported tuple arities or dynamic Lists passed as if scoped tuple work. Source stack sizes are u64 bytes, not an inherited parent budget or saved-Wasm override. Invalid dynamic capacity settings panic; #22/#30 own actual target limits/backend evidence.
27. Failed send must return the exact undelivered value; a committed send is never rolled back. Test send/close/cancel races and disposing buffered/undelivered state exactly once. Payload display is not required merely to display SendError<T>.
28. Reject string-tagged select and a race-of-receives implementation that consumes losing messages. Only the winning handler executes. Closed/drained receivers yield None and can repeatedly win; no ordering/fairness guarantee. Handler results share explicit R but need no invented cross-goroutine evidence just for local mapping.
29. after delivers once then closes; zero delay is immediately eligible, not a mandatory yield. No wall-clock/hard-latency assumption. checked monotonic deadline construction cannot wrap into a different deadline.

### Bytes, numbers and I/O

30. Reject writable byte access derived from text, mutable substring construction, using a Bytes view as an owning header, implicit view-to-owner conversion or a fabricated &List<u8> header. descriptor-slot mutation is not payload permission. Decode/slice results retain sources until their actual last use, including native/callback/disposal use.
31. Invalid UTF-8/surrogates and reversed/out-of-bound/non-boundary text slices are typed Result/Option outcomes, not unchecked casts. Cover incomplete tails, overlongs, isolated continuations and lossy replacement spans. Owned copies must not retain input views. Numeric widths never come from JSON/text guesses or literal singleton refinements.
32. Reject unavailable infallible numeric pairs, ordinary return-only custom parsers/converters, as assertions, mixed-width operands and float wrapping helpers. #41 / [ADR 0017](../adr/0017-user-implementable-conversions.md) supersedes the closed-only conversion model with canonical user-implementable families (`tryInto`/`tryFrom`), not general return-type dispatch. Valid fallible conversions can fail at runtime even when constant; integer arithmetic traps only if evaluated. Preserve strict parse syntax, finite overflow rejection and truncate-before-check conversion.
33. Recoverable stdio/file errors return IoError; source references remain live through native completion/confirmed abort. Partial writes/cancellation are not rollback or safe retry. A successful writeFile is not atomic/durable replacement. maxBytes=0 is Some(empty), not EOF. No exported runtime global/capture-based I/O escape or assumed stable Wasm host pointer.
34. Source panic needs an import; compiler-generated fatal behavior is canonical despite local shadowing. Log output failure and allocation failure are fatal, not caught as Result. Stack overflow uses the separate bounded emergency path, not full normal error rendering.

## Integration boundaries

No Rill compiler/library is present here. The new cases deliberately exercise the confirmed API rather than claiming earlier prototypes were fully migrated. #24 owns diagnostics; #25 exercises a realistic program; #28 owns precise checking/state machines/interface reconciliation; #30 validates pinned backend mechanisms. JSON parsing/formatting is separately scoped by #31, with number representation, duplicate keys, limits and exact APIs still to decide there.
