# M5 ownership and cleanup evidence

[M5 #66](https://github.com/Reilley64/rill/issues/66) is open. This is a scoped compiler increment. It does not satisfy the whole [M5 exit gate](../../../docs/implementation-plan.md#ordered-milestones).

## Verified input and authority

The input is the M4 merge `b6af60a0a6cc175e386593769fbfd97087d8f04e`. Its tree `3211ffdb3a2ae42ddfb2129f9beb978b3fe249a4` matches [M4 PR #58](https://github.com/Reilley64/rill/pull/58) head `0a924afe92b7077ba2b0e89c9598a667b77d1bc5`. The [installed command](../../../docs/development.md#future-installed-toolchain) returned `Checked` with no core blockers and issued receipts in both modes on macOS AArch64 at the input commit. The [M4 record](m4-8-acceptance.md#verified-closeout-revision) names the original checked revisions and negative cases. This evidence does not extend to other installed profiles, execution, or native ABI acceptance.

The design inputs below use the M4 input commit unless a decision identifies a different snapshot. Only [RFC 0001](../../../docs/rfcs/0001-top-level-expectations.md) is Accepted. No RFC status changes here.

| Input | Use in this increment |
| --- | --- |
| [RFC 0007](../../../docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md) and [five-policy approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6915849379), snapshot SHA-256 `59d1bca35706045c2f0fda08a1d12847c79f99ceaad8316a17daafe0f7cf4c70` | Track initialized storage, access restrictions and carried sources separately. Preserve stable free-place borrows. Retire dependents before sources, with reverse declaration order for ties. Canonical explicit `dispose` consumes its named operand. |
| [#7](https://github.com/Reilley64/rill/issues/7#issuecomment-6308080484), [#15](https://github.com/Reilley64/rill/issues/15#issuecomment-6853088155), [ADR 0002](../../../docs/adr/0002-visible-ownership-without-lifetimes.md), [ADR 0007](../../../docs/adr/0007-explicit-mutation-with-conservative-borrow-dependencies.md), [ADR 0011](../../../docs/adr/0011-explicit-numeric-widths-and-utf8-string-views.md) | The later #15 decision replaces the immutable-only and return-only dependency model. Keep the distinction between a descriptor slot, its backing source and the suspended parent access of a reborrow. A named `string` moves, even if initialized from a literal. |
| [RFC 0003](../../../docs/rfcs/0003-expressions-bindings-and-control-flow.md), [RFC 0004](../../../docs/rfcs/0004-functions-overloads-and-generic-requirements.md), [RFC 0005](../../../docs/rfcs/0005-types-records-enums-and-inference.md), [#23](https://github.com/Reilley64/rill/issues/23#issuecomment-6872129850), [ADR 0015](../../../docs/adr/0015-language-required-library-contracts.md) | Use typed identities and evaluate arguments left to right. Keep invocation mode separate from capture acquisition. Do not treat a partial aggregate as eligible for its outer disposer. Canonical operations require canonical identity and evidence. |
| [RFC 0008](../../../docs/rfcs/0008-errors-propagation-and-caused-by-wrapping.md), [RFC 0009](../../../docs/rfcs/0009-user-implementable-conversions.md), [#29](https://github.com/Reilley64/rill/issues/29), [#23 cause refinement](https://github.com/Reilley64/rill/issues/23#issuecomment-6872129850) | Later decisions replace recoverable propagation traces and narrow retained-cause evidence. A conversion or error shape cannot substitute for an ownership transfer contract. Those edges remain pending here. |
| [#16](https://github.com/Reilley64/rill/issues/16#issuecomment-6863910595), [ADR 0009](../../../docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md), [ADR 0005](../../../docs/adr/0005-panics-end-the-program-without-unwinding.md), [ADR 0014](../../../docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) | The replacement tests establish normal-edge order only. Cancellation delivery must be deferred through old-value disposal and installation. Fatal exits do not unwind. No tail-eligibility or protected-release claim follows. |
| [RFC 0010](../../../docs/rfcs/0010-diagnostics-and-reporting.md) and [diagnostic codes](../../../docs/diagnostic-codes.md) | Attach source sites to findings. `R0200` names a transfer and a later use. Access, escape and release findings use `R0201` through `R0203`. No CLI, JSONL or LSP acceptance follows. |

## Compiler interface and procedure

`analysis::ownership::lower_body` takes a `TypedReceipt` and a named source function from the same capture. It returns a finding, an incomplete obligation, a budget limit or ownership MIR for that body. It checks every source callee reached by the supported direct call graph before it publishes MIR. Recursive calls use declared contracts provisionally until the checker verifies every reached body. This does not issue a whole-program ownership receipt.

The builder uses M4 place paths, declaration identities and Boolean facts to form a control graph. It puts scopes on normal, return, break and continue edges. The backward pass computes possible future reads. The forward pass remembers distinct states at each instruction. A read must succeed on every reached state. A repeated state reuses its MIR block, so a loop does not have to be unrolled to a fixed number of iterations. Budget exhaustion never exposes a partial MIR.

A copied descriptor keeps its backing loans. Borrowing a descriptor slot also retains access to its stored references. A reborrow can restrict access through its parent without using the parent slot as backing. Calls in this slice support standalone application bodies and primitive or primitive-reference arguments and results. The caller retains possible input sources for a reference result. A primitive result retains none. Nested mutable destinations are not classified by this rule.

Normal cleanup omits moved storage. A scope batch orders initialized holders before backing sources, then breaks ties in reverse declaration order. A replacement computes the incoming value before displacement. It drops the displaced value only if the old slot is still initialized, then installs the incoming value. Retiring a storage version removes its obsolete loans before the slot can receive a new value. The MIR names obligations; it does not encode target layout or execute code.

## Public tests

On macOS AArch64, [`m5.rs`](m5.rs) stages first-party sources and linked artifacts. It loads the graph in normal and test modes and requires a typed receipt before querying the compiler interface. The installed test does not run on other profiles. Each test family first failed against the prior increment.

| Issue | Witnesses |
| --- | --- |
| [#67](https://github.com/Reilley64/rill/issues/67) | `split` cleans `taken` on the move edge and `text` on the other edge. `bad` rejects the post-join use. `moved` returns without dropping the transferred owner. `discardOwned` drops the moved value in the wildcard's temporary. |
| [#68](https://github.com/Reilley64/rill/issues/68) | `lastUse` accepts a move after the last descriptor copy. `liveAlias` and `exclusive` reject conflict with a live holder. `forward` returns incoming backing; `ownSlot` cannot return body-local backing. `leavesScope` rejects a still-live borrow on inner scope exit. `nestedView` preserves a descriptor's stored backing. `descriptorSlot` rejects return of a reference to the descriptor slot itself. |
| [#69](https://github.com/Reilley64/rill/issues/69) | `callRetainsBoth` rejects a move of the second possible input source without inspecting the callee body. `callNoBorrow` permits a later replacement. `resumeParent` and `suspendParent` distinguish ended and retained reborrows. `releaseScalar` proves explicit canonical consumption. `groupedBorrow` accepts a grouped fresh loan. `transitivelyBad` rejects the callee's local escape without publishing caller MIR. `recursive` checks a recursive edge. |
| [#70](https://github.com/Reilley64/rill/issues/70) | `repeatMove` fails on a backedge. `restore` reinstates initialization and keeps normal drop before install. `leave` cleans only the initialized iteration locals, in reverse declaration order. |
| [#71](https://github.com/Reilley64/rill/issues/71) | `ownedCapture` leaves one captured string obligation. `consumedCapture` rejects use of the moved source. `copiedCapture` keeps a scalar source initialized. `escapingCapture` rejects a stable borrow of a body-local owner. |
| [#72](https://github.com/Reilley64/rill/issues/72) | `ordered` retires an earlier-declared dependent before its later-declared source. `reinitialized` does not let the old view order the replacement owner's cleanup. A typed `List<bool>` body reports `Incomplete`. Cross-capture queries are invalid. Exact work and entry limits succeed; one-short limits issue no MIR. |

The reborrow test exposed an M4 handoff issue. Mutable primitive calls removed saved Boolean facts and treated their loss as unresolved Result storage. The change discards those Boolean facts without a blocker and keeps the blocker for every other affected fact. The installed tests still require the M4 receipt before invoking M5.

## Open acceptance obligations

- Place projections, nested mutable storage effects and callee-exit checks for retained destinations. These must distinguish transferring a contained reference from borrowing its slot.
- Aggregate initialization, pattern and guard commitment, partial moves, iteration, rebuilding and custom-disposer bodies. Confirm remaining-field cleanup without repeating moved fields or invoking the outer disposer again.
- Callable invocation, borrowed passing and scoped jobs, multiple or computed captures, and erased/generic evidence. A known single captured value does not prove these modes.
- Typed propagation/wrapping exits, statement-temporary dependencies, protected actions, cancellation edge obligations, and full graph coverage of bodies, startup and expectations. Pending call and provider contracts cannot produce a program receipt.

No Rill program ran. Runtime, native/Wasm support, backend, CLI and client acceptance remain separate. Do not merge this increment or begin M6.
