# Explicit mutation with conservative borrow dependencies

Issue ri-lql adds explicit mutation while retaining visible ownership, references inside values, and no programmer-written lifetime annotations. Use `let mut`, exclusive mutable borrows, and conservative dependency contracts rather than hidden shared mutable cells or runtime borrow checks. This supersedes ADR 0002's borrow-dependency rules and its immutable-only assumptions; ownership, explicit copies, `dispose`, and owned-value rebuilding remain.

**Status:** the design direction is approved. The combined checker is a proposal, not an implemented or demonstrated-sound system. The ownership/mutation RFC in ri-48g must supply explicit checking rules and adversarial accepted/rejected examples before it is implementation-ready. [ADR 0009](0009-concurrency-safety-and-cooperative-cancellation.md) now resolves ri-40y's cancellation/replacement policy and specifies transfer, sharing, and lock-guard contracts.

## Mutation and access

- Ordinary `let` and owned parameter bindings are immutable. `let mut` permits reassignment and changes to owned fields, without mutable-field declarations or a `const` keyword. An owned parameter can be moved into a mutable local. A mutable `Shared<T>` handle binding does not grant mutable access to its shared payload.
- `&T` is read-only access and `&mut T` is exclusive mutable access, written explicitly when obtaining that access. For overlapping data, permit multiple read-only borrows or one exclusive mutable borrow. Check conflicts statically; restrictions end after last use, including uses carried through other values or `dispose`.
- Recognize statically disjoint named-field storage locally; be conservative for collection indices. Passing a whole record borrows the whole record, without inspecting the callee's body for narrower effects. Different reference-valued fields do not establish disjoint pointees. Reference-containing field extraction conservatively retains the aggregate's possible source dependencies; replacing one field does not automatically clear all aggregate dependencies.
- Forwarding an existing mutable borrow to a borrowing parameter reborrows it. Suspend conflicting use of the original access until the nested borrow's last use, then restore it. Moving a mutable reference into an owned binding remains a move; neither implicit nor explicit copying may duplicate exclusive access.

## Assignments, copies, and replacement

`=` and compound assignments such as `+=` are statements, not value-producing expressions. There is no `++` or `--` in v0. `*reference` accesses the referent, not a raw pointer; record-field access through borrows remains convenient. Reading through a borrow does not permit moving out non-copying data.

Mutability does not change copy classification. Existing implicitly copied scalar, record, and tuple values still copy; other owned values move unless explicitly copied. Values carrying exclusive borrows cannot implicitly copy. Existing owned-record rebuilding guarantees remain. Loops can update mutable accumulators instead of requiring recursive accumulation.

Replacement evaluates the destination and right-hand side once, keeping the old value alive while the replacement is computed; it then frees/disposes the old value and installs the replacement. Reject a replacement leaving references into freed old storage. The replacement remains owned while disposal runs, and cleanup must track initialization/disposal state. ADR 0009 now defers cancellation delivery from old-value disposal through replacement installation, while permitting suspension/scheduling and retaining the replacement's ownership. Deliver pending cancellation only after a valid destination is installed, then clean it up exactly once; destination/RHS evaluation remains normally cancellable. Track initialized/disposed states and retain all dependencies needed through disposal. ADR 0005's non-unwinding panic policy is unchanged.

## Dependencies and escaping references

References may remain inside records, collections, tuples, and captures. Moving a value preserves its carried dependencies; owning a parameter does not imply that it contains no borrows.

- A reference-containing result conservatively retains dependencies from borrowed inputs, including borrows nested inside owned inputs. Returning an existing contained reference can be valid; returning a reference into locally owned storage that will be freed is not. If a result depends on an exclusively borrowed input, retain that exclusive restriction until the result's last use, even for a read-only result. Do not automatically downgrade it to shared access. A reference-free result does not extend the call's borrow this way.
- Calls conservatively account for reference-carrying inputs flowing into mutable arguments that can retain references, even when the result is `void`. These are caller-visible contracts, not permission to export arbitrary sources. The callee must check its exits, including through nested mutable references and intermediate calls: caller-owned destinations cannot retain references to body-local owners.
- Distinguish the temporary access loan to a container from dependencies retained by its contents. Ending a reborrow or temporary handle does not erase dependencies written into its underlying destination. Retained dependencies accumulate across calls and merge at branches and loop backedges. Release them only when local analysis proves the old references can no longer be used, including by disposal, after replacement/destruction or final possible use. A clear-like function does not erase dependencies merely by its name, return type, or caller-side body inspection.
- A borrow cannot extend its source's owning scope. Borrowed temporaries may be used within their enclosing statement; a borrow used in later statements needs a named owner. Within that owner's scope, keep it alive for outstanding borrows rather than freeing it prematurely.

The conservative rules can reject safe programs. ADR 0002's suggestion to pass an argument owned or copied is not a general workaround: owned values may still carry borrows, and copying a mutable destination would lose the intended mutation.

## Callables and generic requirements

Callable types preserve invocation capability through annotations, parameters, returns, records, and control-flow joins:

| Type | Invocation contract |
| --- | --- |
| `() => R` | Repeatable through read-only access; not a purity guarantee. |
| `mut () => R` | Repeatable, requiring exclusive mutable access to the callable. |
| `once () => R` | Consumes the callable; may move non-copying captures out. |

Infer the required mode from a closure. Permit weakening a read-only callable to mutable or once, or mutable to once, but not strengthening a guarantee through an annotation or conversion. Consuming invocation needs ownership, not merely `&mut`.

Borrowing closures read or exclusively mutate captures as required. Owned/escaping closures capture by value under the existing move/implicit-copy rules, not through hidden shared mutable cells. Mutating owned captures requires mutable access to the closure; a named local uses `let mut`. A read-only borrowed callable parameter cannot invoke a callable requiring mutable access. Conflicting concurrent captures are rejected.

Invocation capability, borrow dependencies, and copyability are separate properties. An arrow annotation does not erase dependencies or make a closure implicitly copyable. Explicit copying requires valid capture-environment copy evidence: copying independently owned state can be valid, but duplicating exclusive borrowed captures cannot. Wrapping or annotating a callable cannot bypass this.

Add `where noBorrow(T)` as a compiler-recognized intrinsic, not a user-implementable overload. It requires no borrowed dependencies through fields or captures; it does not imply copyability or concurrent-sharing safety. Check generic bodies before monomorphization: unconstrained `T` may carry borrows. Establish the property from known structure/captures or explicit generic requirements. Erased arrow interfaces cannot invent evidence about their environments; reject operations requiring unavailable evidence. Generic copying still requires `where copy(T)`.

## Alternatives and follow-through

- **References only in parameters, or no references in mutable positions:** simpler checking, but narrows ADR 0002's expressiveness. Rejected for this design direction.
- **Programmer-written lifetime/source annotations:** more precise contracts, but changes the no-lifetime-syntax commitment. Rejected.
- **Runtime borrow checks, implicit mutable sharing, or automatic shared downgrades:** avoid some static restrictions but introduce different runtime/access rules. Not adopted.
- **Caller inspection of callee bodies:** potentially more precise, but makes contracts implementation-dependent. Not adopted.

`prototypes/07-mutation.rill` illustrates the approved syntax and `prototypes/07-mutation-safety.md` records hand-checked expectations and remaining validation obligations. These are design artifacts, not executed compiler tests or a proof.

ADR 0009 records ri-40y's approved synchronization, cross-goroutine, guard, and cancellation/join policies. ADR 0010 records ri-owe's module/scoping and privacy rules, including non-capturing named functions, canonical ownership-hook identity, and protection of required built-in ownership mechanics. ri-8n7 owns standard-library signatures and callable contracts preserving the evidence required by `spawn` and waiting helpers. ri-48g owns explicit checking/cleanup rules, safety rationale, and adversarial cases in the feature RFCs. The approved direction does not automatically accept those RFCs.
