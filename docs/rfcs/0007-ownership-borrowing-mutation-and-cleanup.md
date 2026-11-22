# RFC 0007: Ownership, borrowing, mutation and cleanup

| Field | Value |
| --- | --- |
| ID | 0007 |
| Status | Draft |
| Owner | Reilley64. ri-48g tracks this draft. |
| Source inputs | ri-8hu ownership decision (comment 6308080484), ri-lql mutation decision (comment 6853088155) and ri-40y concurrency decision (comment 6863910595). Later refinements appear under [source reconciliation](#source-reconciliation). Repository inputs use commit `f33dcf4cf3f815824eeb897135775d7884f65c1a`. |
| RFC dependencies | Draft RFCs [0002](0002-modules-and-program-loading.md), [0003](0003-expressions-bindings-and-control-flow.md), [0004](0004-functions-overloads-and-generic-requirements.md), [0005](0005-types-records-enums-and-inference.md) and [0006](0006-numbers-strings-and-primitive-operations.md) propose shared rules. Their full texts are not accepted. Required [concurrency, error and runtime companions](#required-companion-rules) remain incomplete or missing. |
| Replaces / superseded by | None. This draft does not amend an Accepted RFC or accept another draft. |
| Decision | Not yet decided |
| Reviewed revision | Not yet reviewed |

## Summary and scope

This RFC proposes ownership transfer, implicit and explicit copies, borrowing, mutation and cleanup. It also proposes caller/callee dependency contracts, closure capture formation and control-flow state tracking.

The glossary defines [borrow, borrow dependency, reborrow, dispose, callable and borrow-free value](../../GLOSSARY.md#language). Owning a value does not prove that it contains no borrows. A read-only result can retain an exclusive input restriction.

The source decisions already permit references inside values and mutable destinations. This draft does not remove those capabilities to simplify checking. It preserves visible ownership without programmer-written lifetime annotations or runtime borrow checks.

The project owner [confirmed five policies](#confirmed-policies) for layered state, captures, partial moves, cleanup order and explicit disposal. Nested call/storage routes, complex captures and erased evidence still need formal completion. Policy approval does not supply those missing judgments. This document remains Draft, not a soundness proof.

Concurrency protocols, error wrappers, native admission and production ABI require companion rules. This RFC states their ownership obligations without implementing their state machines. No Rill compiler or runtime has validated these examples.

## Motivation

A function can return a borrow or store one through a mutable argument. Its result type alone therefore cannot describe every retained source. A temporary access loan and a stored reference need separate tracking.

An owned container can hold references to other owners. Extracting an existing reference must preserve its original sources. Returning a reference into the container's destroyed storage must still fail.

Cleanup can invoke user code, suspend or wait for children. Its order and cancellation behavior affect observable results and resource safety. Last-use analysis must not move a disposer earlier merely to simplify borrowing or tail calls.

## Guide-level explanation

Case O1 is a complete module:

```rill
type Point = { x: i32; y: i32 };

function increment(value: &mut i32): void {
  *value += 1i32;
}
function shifted(point: Point): Point {
  let mut moved = point;
  increment(&mut moved.x);
  return moved;
}
expect shifted(Point { x: 2, y: 3 }).x == 3i32;
```

The owned parameter binding is immutable. Using a mutable local permits mutation there. Point has implicitly copyable scalar fields, so its ordinary value uses can copy. Mutation does not change that classification.

Point's declared fields supply i32 context for its numeric literals. Their suffixes are optional. This follows [record field checking](0005-types-records-enums-and-inference.md#record-declarations-and-construction) and [numeric width inference](0006-numbers-strings-and-primitive-operations.md#primitive-types-and-width-inference). Unconstrained literals still need a type.

Case O2 is a separate complete module:

```rill
type Point = { x: i32; y: i32 };
function remember(slot: &mut &Point, source: &Point): void {
  *slot = source;
}
function selected(): i32 {
  let first = Point { x: 1, y: 2 };
  let second = Point { x: 9, y: 10 };
  let mut view = &first;
  remember(&mut view, &second);
  return view.x;
}
expect selected() == 9i32;
```

The call's exclusive access to the descriptor slot can end after the call. The slot's contents still depend on the referenced Point sources. Returning void does not erase those dependencies.

Case O3 is a complete module:

```rill
function identity<T>(value: T): T { return value; }
function carried(): i32 {
  let owner = 42i32;
  let view = identity(&owner);
  return *view;
}
expect carried() == 42i32;
```

Identity receives its argument by value. That argument is itself a reference. Moving or copying its descriptor preserves the source dependency. No noBorrow requirement is needed merely to forward it.

These examples state intended outcomes. They are not compiler results or executed tests.

## Syntax and reference semantics

### Ownership, values and places

A plain T value carries its ordinary ownership responsibility. A named function parameter of type T receives that value under the move/implicit-copy rules. A reference parameter receives explicit borrowed access instead.

A place is storage that can hold a value. The proposed model identifies a local root and its field, index or dereference projections. Place identity is checking metadata, not a new source pointer or lifetime annotation.

Ordinary let bindings and owned parameters are immutable. Let mut permits assignment and mutation through owned places. It does not grant mutable access to a Shared payload or change a reference's declared capability.

Track initialization and cleanup responsibility separately from the value's source type. Moving a non-copying value leaves its source place unavailable. A later read or cleanup must not treat that place as initialized.

No uninitialized-local syntax is introduced. Internal partial construction and moved fields still need state tracking. A complete value is not available until its required fields are initialized.

### Copies and consuming operations

Use the existing implicit-copy classification for scalars, read-only views and eligible aggregates. Tuples and records require eligible contained values. A parent enum requires every possible payload to qualify. A known unit variant can qualify independently.

Strings, owned collections, counted handles and exclusive references gain no implicit-copy exception. A named string initialized from a literal still moves. Types with custom disposal must not gain implicit copying through field shape or constant knowledge.

Canonical copy has `copy<T>(&T): T`. It requires the selected implementation's evidence. Copying an owner may produce independent owned data; copying a counted handle preserves the same counted resource. These are not interchangeable promises.

Copying a read-only descriptor preserves its source dependencies. Copying cannot duplicate existing exclusive access. An annotation, record wrapper or generic adapter cannot manufacture that permission.

An unconstrained generic T is not implicitly copyable. Explicit copy(T) evidence permits copy(&value), not silent duplication at ordinary uses.

Canonical dispose has `dispose<T>(T): void`. An explicit canonical disposal ends the supplied value's ownership responsibility. The confirmed operand rule consumes a named place even when its type otherwise permits implicit copying. Other aliases can still retain their own dependencies.

An unrelated function named dispose cannot redirect compiler cleanup. Explicitly dropping one descriptor does not release a source while another descriptor, destructor, child or native operation still needs it.

Custom copy/dispose overlap and implicit-copy eligibility need complete formal reconciliation. Protected negative capabilities remain mandatory. This draft does not assume that a user body proves safe duplication merely because it has the expected result type.

### Proposed M4.4 pattern binding type classification

M4.4 ticket ri-qtl owns this typing prerequisite. The owner approved K1 through K4 (ri-qtl, comment 6957273163) before model/compiler code. This approval does not authorize ownership checking or complete this RFC.

A borrowed pattern binder needs a static choice between the contained value type and a projected reference type. Use the existing implicit-copy classification, not canonical copy(T) evidence. Explicit copying does not authorize silent copying.

Classify established scalar primitives and read-only descriptors under their fixed existing rules. Strings and exclusive references have no implicit-copy exception. An unconstrained symbolic T remains non-implicit-copyable even when an eventual concrete instantiation could copy.

Preserve that symbolic body's classification through monomorphization. Do not retype its pattern binding from later caller values or implementation counts.

Aggregate classification also needs custom-disposer and protected-capability reconciliation. If that classification is unresolved, return an incomplete typed obligation. Do not assume copying from field shape. Do not choose borrowing as a fallback for an unresolved proof.

A positive classification chooses the binder's static value type only. It performs no copy. A non-copying borrowed projection retains its access mode and descriptor/referent layers. Both retain their original source dependencies for later ownership checking.

No classification proves initialized storage, exclusive access, valid partial moves, safe guard commitment or cleanup. Those operations remain outside M4.4.

#### Post-M6 explicit copying

The owner requested this change in ri-bgy. M5 and M6 use the copy rules above. Apply the rules below only after M6.

Remove implicit copying for all types, including scalars, read-only references and aggregates. An owning binding, argument, return or capture moves an existing value. The source place stays unavailable until it is initialized again. To duplicate an existing value, call canonical `copy(&value)` and prove its copy requirement.

Built-in scalar arithmetic and comparison read their operands without moving them. A read does not create a first-class reference value. Write `&value` to obtain a reference value. Reads must still obey exclusive access restrictions. Ordinary function arguments follow their declared ownership contracts.

Post-M6 example, not checked:

```rill
let n = 7i32;
let twice = n + n;
let second = copy(&n);
let moved = n;
// let invalid = n + 1i32; // Rejected: n was moved.
```

An owning pattern transfers a value. A borrowed pattern cannot silently copy a field into an owned binding. Copying a read-only reference explicitly must preserve its dependencies. Exclusive references and protected non-copyable resources remain non-copyable.

Before implementation, specify the affected pattern, generic, capture and standard-library procedures. Check how source, MIR and diagnostics distinguish a read, a borrow, a move and an explicit copy. Migrate live examples separately from historical evidence.

### Borrow acquisition, overlap and reborrowing

Read-only `&T` permits observation without ownership. Exclusive `&mut T` permits mutation without ownership. For overlapping storage, permit multiple read-only loans or one exclusive loan.

Local named fields of the same owned record can establish disjoint storage. A whole-record loan overlaps those fields. Do not infer arbitrary disjointness from collection indices, different reference-valued field names or runtime inequality tests.

A dereferenced source must retain its actual access restrictions. Two descriptor slots can be distinct while pointing to overlapping data. Logical string/byte views also carry their observed extents rather than an assumed owner header.

Passing existing mutable access to a borrowing parameter creates a reborrow. Conflicting use of the parent access stays suspended while the child loan is needed. Restore that access after the child's final possible use.

Moving an exclusive reference into an owned local remains a move. Reborrowing is not an implicit copy of that reference. Read-only invocation or a read-only result does not turn an exclusive source into shared access automatically.

Borrowed temporaries can live through their enclosing statement. A later statement needs a valid named backing owner or an approved static-literal view. Do not extend owning scopes to rescue escaping references.

<a id="proposed-layered-dependency-model"></a>

### Layered dependency model

The checker must distinguish these facts:

| Fact | Required meaning |
| --- | --- |
| Storage state | Whether a root/field currently contains an initialized value. |
| Access loan | Permission to observe or exclusively access a particular storage region. |
| Carried dependencies | Sources and restrictions contained in a value, including nested references and captures. |
| Cleanup obligation | Work that still owns or may use the value and its carried dependencies. |

These are private checking facts, not new programmer annotations. A dependency records its source region and retained access mode. It must also retain enough route information to distinguish a stored reference from access to its descriptor slot.

Reading a descriptor slot needs access to that slot. Copying its contained read-only reference produces the stored reference's dependencies. It does not produce a reference to the slot merely because the copy read it.

Conversely, borrowing that descriptor slot produces a dependency on its storage. Returning `&localDescriptor` is not equivalent to returning localDescriptor by value. The former cannot escape when that local slot ends.

An owned container's storage and its elements' carried sources are separate facts. Extracting an existing reference may outlive the container when its external source remains valid. Returning a reference into the destroyed element slot cannot. This distinction needs a checked value-transfer contract or a justified local projection. A matching result type or operation name alone cannot remove storage dependencies.

Conservatively retain an aggregate's possible carried sources on reference-containing extraction. Named-field storage precision does not promise field-sensitive source provenance. Replacing one field does not automatically clear all aggregate dependencies.

A result derived from exclusive input must retain that exclusive restriction when required by its dependency route. Do not discard it because the visible result type is read-only. Copying an independent noBorrow value is a different operation.

This distinction must survive generic values, opaque representations, logical views and nested mutable references. The complete route/projection judgments remain authoring work. A flat union that rejects every owned-container extraction is not an acceptable substitute.

### Implicit call contracts and callee exits

Callers use declared types, canonical requirements and approved dependency contracts. They do not inspect a callee's body for narrower effects. A renamed clear-like function must not change the caller's checking contract.

For a result that can carry borrows, include the possible sources from borrowed inputs and references carried inside owned inputs. Preserve their access modes. An owned parameter's local container storage does not become an allowed escaping source.

Treat callable receiver access and captures as inputs to these rules. A borrowed repeatable callable can return a valid environment-dependent borrow. A consuming call cannot return a reference into destroyed owned capture storage. Existing carried references remain separate.

A proven noBorrow result does not retain input loans merely because the call borrowed inputs. The proof must include hidden fields, capture environments and trusted metadata. An owned result with unknown contents has no such guarantee.

For mutable arguments that can retain references, account for possible compatible input sources flowing into their reachable storage. Preserve pre-existing stored dependencies unless local analysis proves those old values dead. This applies even when the call returns void.

Do not blindly insert the destination's temporary access loan as a reference stored inside itself. Storage access, existing contents and possible input-to-destination flow need separate routes. The exact nested routing relation still needs formal completion.

At every normal callee exit, check returned values and externally retained destination state. They may depend on valid incoming sources or permitted static backing. They must not retain body-local owners, temporary storage or destroyed owned-parameter storage.

These exit checks also apply to normal cancellation cleanup exits. A planned later overwrite cannot excuse invalid outward state at a possible delivery point.

Forwarding through another function does not remove this exit check. Nested `&mut &T` and reference-containing owned parameters keep the same obligations. Caller-specific hopes about unused output do not permit a callee to export invalid state.

A caller can release old local contents when analysis proves no future use remains. It cannot use an unknown callee's name or eventual monomorphized body as that proof. Construction origins and error forwarding do not alter these contracts.

### Control flow, updates and last use

The confirmed state policy tracks initialized places, live loans, carried dependency sets and outstanding cleanup. Keep value versions distinct when replacing storage.

At a branch join, a value is definitely initialized only when every continuing path initializes it. Retain a source or cleanup obligation when any continuing path can still need it. A never-returning path supplies no continuing state.

Compute loop/backedge state to a fixed point. Reusing a slot does not revive an expired owner instance. Do not forget an earlier iteration's possible retained source or partial move. Known dead versions can leave the state after their final possible use.

A whole-value assignment can replace old dependency facts after the old value's required cleanup and successful installation. A field write uses a conservative update unless every possible retained route is known removed. An aggregate overwrite must not erase dependencies still held by another alias.

Last use includes destructor reads, remaining-field cleanup, captured values and required child/native completion. It is not merely the last explicit identifier occurrence. Ending a temporary reborrow does not end references written into the underlying destination.

Reject a source's scope exit when a still-live dependent value can need it afterward. Do not extend the source's lexical owning scope. A dead descriptor with no remaining observable use is different from an active retaining holder.

### Replacement and same-type rebuilding

Evaluate the assignment place once before the replacement expression. Keep the old value alive during replacement evaluation. Reject a replacement that would retain a borrow into freed old storage.

Ordinary replacement follows the confirmed phases:

1. Evaluate the destination and replacement under normal cancellation rules.
2. Retain ownership of the completed replacement.
3. Defer cancellation delivery when old-value disposal begins.
4. Dispose the old value if its ownership remains in the destination.
5. Install the replacement and record the new initialized state.
6. Deliver pending cancellation afterward against the valid installed state.

Suspension and scheduling remain possible during deferred delivery. Cancellation requests do not abandon a cleanup wait. A fatal panic still provides no unwinding guarantee.

Replacement evaluation can already transfer the old whole value into the replacement computation. Track that move and do not dispose it again from the original slot. Cancellation cleans the values actually owned in the reached state.

Named same-type rebuilding transfers the consumed record's surviving ownership obligations into the rebuilt value. It is not ordinary destruction of the entire old record followed by reconstruction from freed fields. Preserve the approved in-place owned reuse guarantee.

Evaluate base and replacements once, retaining original fields needed by replacement expressions. Dispose replaced field values exactly once. Do not clone retained fields implicitly or invoke the outer disposer on moved ownership merely to obtain the fields.

Borrowed rebuilding does not grant moves out of non-copying borrowed fields. Construction visibility and exact type identity follow RFCs 0002/0005. Detailed rebuilding/partial-initialization cancellation transitions remain necessary formal work.

<a id="proposed-capture-formation-and-erased-evidence"></a>

### Capture formation and erased evidence

A borrowing callable parameter can borrow needed captures. Stored, returned or owned callable values capture values under move/implicit-copy rules. The capture value can itself be an explicitly acquired reference.

For stable free-place expressions such as `&numbers`, capture that borrowed access when the closure forms. This permits an owned callable environment to contain a caller borrow. It does not make that environment borrow-free.

A bare owned free value instead moves or implicitly copies into the environment. Mutating an implicitly copied scalar capture changes the captured copy, not the original local. Mutating owned captures requires the callable's mutable access mode.

Do not move side-effectful body computations to capture creation. Complex indexed places, branch-dependent capture sets and precise field capture still need formal rules. A shared free-place borrow and a copied whole owner are not interchangeable defaults.

This model must preserve direct scoped-helper examples such as two jobs reading `&numbers`. It must also preserve the prototype's stored copied counter. Rejecting either use case requires design review, not a silent simplification.

Invocation capabilities follow RFC 0004. Mode weakening does not erase dependencies or supply copy/noBorrow/sendable/shareable evidence. An erased callable interface keeps actual restrictions while losing guarantees it does not promise.

Check generic bodies before monomorphization. Unconstrained T or F can carry borrows, including exclusive ones. Stated callable and property requirements preserve the evidence needed for permitted operations.

<a id="proposed-destructor-safe-partial-moves"></a>

### Destructor-safe partial moves

Partial construction tracks only initialized fields. Before a complete T exists, cleanup handles those fields rather than calling a user disposer that assumes a complete T.

For an ordinary record without a custom outer disposer, track independent moved fields. Reading or moving the whole record requires its necessary fields initialized. Remaining initialized fields still need cleanup.

For a type with a custom disposer, reject field extraction that leaves the outer value incomplete. A normal cancellation point cannot safely expose such a hole. Whole-value moves, explicit copies and valid same-type rebuilding remain available.

The active custom disposer is a different context. Its outer action has already started and must not run recursively on its own consumed parameter. It may move or explicitly dispose fields under ordinary checking; remaining-field cleanup skips those fields. The active outer value cannot escape as a fresh fully owned T. Field transfers carry field responsibilities, not a new outer-disposer obligation.

This confirmed restriction concerns partial moves, not an outermost-type orphan rule or a ban on custom disposal. Reconcile it with the full pattern/rebuilding contracts before readiness. If this restriction conflicts with an approved case, return that case to design review.

<a id="proposed-deterministic-cleanup-scheduling"></a>

### Deterministic cleanup scheduling

Values normally reach cleanup at their owning scope boundary. Explicit disposal and ownership transfer can end a particular value earlier. The compiler may reclaim non-observable storage after last use when no loan or cleanup still needs it.

Do not advance user disposers or transitive protected actions merely because direct reads ended. A wrapper with no custom disposer can still contain observable field cleanup. Borrow release and storage reclamation do not authorize early guard unlocking.

Within one cleanup boundary, order eligible dependents before the sources they need. Reverse local declaration order breaks ties among initialized bindings. Replacement does not change a binding's declaration rank. For remaining record fields, use reverse declaration order; for tuple/List elements, use reverse position order.

Statement temporaries use reverse successful creation order after required dependencies. Partial construction cleans initialized field temporaries without calling an incomplete outer value's custom disposer. Preserve written evaluation order when identifying those temporaries.

A custom outer disposer runs before remaining-field cleanup. Dependencies it may use stay valid until that action completes. Its moved/disposed fields are not disposed twice.

Map entry order retains the library's unspecified iteration order rather than adding sorting or comparison callbacks. Required dependency order still applies. A default container's implementation must preserve its stated cleanup ordering and ownership obligations.

Do not move cleanup across a source's lexical scope boundary to invent validity. An explicit dispose request cannot silently destroy other live holders first. If required cleanup has no valid order, reject with the dependency explanation rather than leaking or inventing pinning.

The dependency-first policy and tie-breaking order are confirmed. Complete ordering across nested scopes, temporaries, partial construction and custom-body exits still needs formal specification. They do not authorize a catchable failure or garbage collector.

### Protected actions and asynchronous completion

Custom disposal adds cleanup before the compiler disposes remaining initialized owned fields. An empty wrapper disposer cannot suppress a contained guard, counted-owner or native obligation. Protected actions keep their canonical identities through aliases and generic evidence.

Ordinary orphan permission does not authorize interception. Collection must reject non-trusted bodies that replace or tie required guard release, count maintenance or native-source retention. Negative capabilities cannot be manufactured through a generic adapter.

Normal cancellation delivery stays deferred through disposal, replacement installation and required cleanup joins. Dependent children must finish before their sources or guards are released. A request, handle drop or abort request is not completion.

Native cleanup must wait for completion or confirmed abortion before invalidating accessed storage. Retaining an owner alone does not prove address stability or safe Wasm host-pointer use. Runtime metadata and actual provider behavior remain separate gates.

Background spawn requires its established noBorrow/sendable callable contract. Scoped work may carry valid caller borrows and return caller-dependent values after all required execution/cleanup completes. Neither mechanism permits references to destroyed child storage.

Standard/default cleanup needs bounded call-stack use, using iterative state when necessary. User callbacks/disposers remain ordinary code. Worklist data can grow, and cleanup can wait indefinitely. Fatal resource failure does not permit unsafe recursive fallback.

Panic, stack overflow and fatal exhaustion retain no-unwind behavior. Normal top-level completion still has no implicit global background join. Proper tail transfer cannot free caller storage or skip pending cleanup to create eligibility.

### Proposed M4.5 protected-overlap boundary

M4.5 ri-8p9 proposes this collection prerequisite. The owner approved the scoped checkpoint (ri-8p9, comment 6963439018). This approval admits no provider, ownership operation or runtime mechanism.

A catalogue belongs to one graph capture and one admitted metadata identity. Each family record retains actual role-admission evidence and its provenance. It states whether required-winner and prohibited-input inventories are complete. Missing evidence is not a validated empty inventory.

Use two protection record forms. A required-winner record names its canonical family, input pattern, action and actual admitted provider. A prohibited-input record names its canonical family, forbidden pattern and negative-capability reason. The prohibited form has no provider.

Resolve a required provider to its actual implementation declaration in that graph. Check its target family and normalized input key against the record. Validate its required signature schema through actual core admission. A caller assertion cannot create provider authority.

Admission belongs to the particular family/action record. Admission for another action is not a global whitelist. A prohibited record cannot gain a provider through unrelated admission.

Validate the catalogue before checking source bodies. Reject conflicting required winners as invalid admission. Reject required/prohibited domain conflicts too. Do not choose an action by catalogue order.

Use [RFC 0004's comparison procedure](0004-functions-overloads-and-generic-requirements.md#proposed-m45-ordinary-input-procedure) for each applicable family record. Compare original whole-vector patterns, not caller-filtered variants.

For a required-winner record, check every body not admitted for that record. Disjointness is safe. An intersecting body is safe only when the required provider's pattern strictly dominates it. Equal, narrower or incomparable overlapping bodies reject as R0500.

For a prohibited-input record, reject any implementation intersecting the forbidden domain. There is no provider to supply a more-specific escape. A generic adapter's overlap still counts. Requirements and aliases cannot remove the prohibition or manufacture its negative capability.

If unavailable capability information prevents a complete prohibition, retain a pending prohibition obligation. Do not omit it from catalogue completeness. This includes protection whose capture or transitive metadata remains unavailable.

Names, standard-root paths and private fields confer no authority. An actually empty inventory still requires validated per-family admission evidence. User requirements do not affect overlap checking. Retain the action/prohibition, body and comparison witnesses.

Actual canonical admission remains an M4.8 prerequisite when missing. Structural input facts can remain useful, but no completed protected collection or checked call follows. No test may fabricate a standard capability definition or weaken the catalogue to obtain success.

### Proposed M4.6 capture and invocation facts

M4.6 ri-w05 proposes this typed boundary for [arrow checking](0004-functions-overloads-and-generic-requirements.md#callable-formation-and-invocation). Approval remains pending. Keep access demands and carried sources. Do not acquire loans, move captures, execute copies or prove lifetime safety in this stage.

Resolve each written arrow use in its actual lexical environment. A reference to a runtime outer binding records a capture candidate. Static declarations do not capture runtime storage. Record which inner callable forms or uses a captured path. A named function's noncapture restriction remains unchanged.

A stable local, field or explicit dereference path can supply original capture provenance. An explicit `&place` or `&mut place` records that access, not a captured copy of the place's owner. Capture formation retains the source restriction and original path. Complex computed places or unresolved acquisition needed for a mode/property proof remain incomplete. Do not hoist side-effectful computations from an arrow body.

Determine the typed demand from the arrow's reachable possible uses. Check skipped written source without choosing its runtime outcome. Read-only observation needs read-only invocation. Mutation or repeatable mutable invocation of owned capture storage needs mutable access. A use requiring consumption of a non-implicitly-copyable owned capture needs consuming access.

For a by-value use, consult the existing implicit-copy classification. Do not use an explicit copy overload as implicit-copy evidence. An unresolved classification blocks a mode proof if it affects consumption. Keep explicit copy calls as their own evidence goals. Do not replace consumption of a borrowed referent with a capture move.

Combine the strongest required access with the established capture form. An explicit borrowed capture retains its reference access when used. A borrowed receiver cannot grant consumption of its environment. A mutable descriptor does not change a shared reference's referent access. Track every capture site's demand and retained dependency, not only the combined mode.

An arrow with no outer runtime uses has read-only mode. A capture-free named function value also has read-only mode. Do not use the expected receiver mode to infer a stronger capability. Preserve actual capture evidence through permitted weakening. Erasing to a declared callable interface keeps restrictions but loses unstated guarantees.

Pending projections or mode requirements can coexist with an established parameter/result signature. They cannot supply a checked invocation or verified property. Later ownership checking still validates formation, exclusive access, escape and transfers. In particular the typed mode does not permit moving out of a shared borrow.

#### Property and callable evidence

Use the actual prelude predicate identity. Assumptions and proofs retain the canonical subject and originating value facts. A reference proves noBorrow false for its type; a checked scalar or owned string proves noBorrow. A known reference dependency cannot disappear in a record or closure.

A checked scalar or immutable string proves both `sendable` and `shareable`. A shared reference needs a shareable referent for both. An exclusive reference needs a sendable referent for `sendable`. Its read-only sharing needs a shareable referent and preserves exclusivity. These rules implement the source-type decision in [ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md#transfer-sharing-and-scheduling).

For ordinary source tuples and inspectable user nominals, require the property of each contained value type after full substitution. A variant uses its own payload. A parent uses all declared payloads. A phantom parameter supplies no stored value. Apply this rule only to actually established representations. An opaque, native or protected representation needs its own admitted contract, not a shape proof.

Use actual typed capture facts for concrete callable properties. Check every captured source using its actual reference or owned form. Use all relevant paths when a callable value joins from several origins. Erased or unresolved environments supply no positive proof. A callable signature, selected body or lack of fields in a public view does not prove an environment property.

When a structural proof reaches a member, project its value origins along with its substituted type. For `Holder<F>` with a callable field, use the closure origins for that field rather than F's signature. For joined aggregates, use all corresponding field origins. Keep this distinction in proof keys. Missing field origins stay unresolved when the field type alone cannot prove the property.

For a canonical callable predicate, compare the complete argument tuple and result terms by equality. Use the declared receiver type and actual mode. Apply the mode order to an owned callable. A shared reference forwards read-only invocation only if its referent supports it. A mutable reference forwards read-only or repeatable mutable invocation according to its referent's mode. It cannot forward consuming invocation of the referent.

For a reference subject, forwarding can give the descriptor its own callable capability. A once-use of that descriptor still invokes the referent without consuming the environment. A shared descriptor invokes read-only. A mutable descriptor needs the access required by its forwarded mode. Do not insert further implicit dereferences.

Each rule yields proved, disproved or unresolved evidence. Unsupported or cyclic structural obligations are unresolved. Do not derive a missing operation from unavailable core metadata. There is no structural proof of explicit copy, protected guard access or disposal. Preserve every proved property independently. No property implies permission to act on a particular value.

### Proposed M4.8 aggregate classification

M4.8 ri-dm4 proposes this typed prerequisite. It requires separate checkpoint approval. The classification chooses a binder type or capture demand. It does not execute an operation.

For an otherwise eligible nominal value, check canonical disposal and protection facts for its actual static type. Use the admitted inventory and shared input selection, with the original premises. An applicable custom disposer or a protected prohibition makes that type non-implicitly-copyable. Ordinary default field cleanup is not a custom disposer. A custom `copy` body does not make the type implicitly copyable.

Check copy/dispose interception separately during collection. A hypothetical winner, unmet evidence or ambiguous action supplies no positive classification.

For eligible source aggregates without those exclusions, require the classification of every stored component after substitution. A known variant uses its own payload. A parent enum needs all payloads. Apply the disposal and protection check to the actual static aggregate type as well as its contained values.

Stop at a reference descriptor and apply its own rule. Never infer the referent's capability from the descriptor. Use admitted facts for a protected or opaque stored value.

An established non-copying classification selects the projected reference for a borrowed binder. It also makes a by-value use of an owned capture demand consuming invocation. Incomplete classification blocks either judgment if it depends on that fact.

Do not substitute a borrow or an expected invocation mode for missing evidence. Keep the classification of an unconstrained rigid parameter under the existing symbolic rule. Preserve its carried sources independently of the binder or invocation choice.

### Required companion rules

| Rule owner | Required contribution | Current dependency state |
| --- | --- | --- |
| [Draft RFC 0002](0002-modules-and-program-loading.md) | Source visibility, canonical hooks, declarations and protected family collection. | Its policies are confirmed. Resolver details remain incomplete. |
| [Draft RFC 0003](0003-expressions-bindings-and-control-flow.md) | Expression sequencing, control transfers, pattern execution and divergence. | Its four policy groups are confirmed. Complete flow judgments remain incomplete. |
| [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md) | Signature/evidence relationships, callable modes and static candidate selection. | Its precision policies are confirmed. Full symbolic evidence checking remains incomplete. |
| [Draft RFC 0005](0005-types-records-enums-and-inference.md) | Nominal layouts, patterns, generic fields and variant relationships. | Its precision policies are confirmed. Full layout/pattern judgments remain incomplete. |
| [Draft RFC 0006](0006-numbers-strings-and-primitive-operations.md) | Logical text/byte places, static literals and core conversion ownership contracts. | Precision policies and strict entry points are confirmed. Broader matrix and ScalarError remain draft details. |
| Concurrency and runtime | Scheduling, transfer/sharing derivation, child/native state machines, protected registry and private ABI. | No complete RFC assigned. This document states retention obligations, not an implementation. |
| Draft RFCs [0008](0008-errors-propagation-and-caused-by-wrapping.md) and [0010](0010-diagnostics-and-reporting.md), with trusted admission incomplete. | Cause ownership, exact errors, dependency traces, incomplete analysis and opaque property admission. | Both remain Draft. Complete cause/evidence, source/driver and trusted metadata judgments remain incomplete. ri-48g retains formal and production gates. |

## Valid/invalid examples and diagnostics

O1 through O3 appear in the guide. Each later case is independent unless its context says otherwise. Expected validity and outcomes are not observations from a Rill compiler. No cleanup trace below has executed.

Case O4 is a rejected complete function:

```rill
function invalidMove(): string {
  let first = "owned";
  let second = first;
  return first;
}
```

String is non-copying. The move to second makes first unavailable, R0200. Literal initialization does not supply an implicit owner copy.

Case O5 is a complete module:

```rill
type Pair = { left: i32; right: i32 };
function separate(): i32 {
  let mut pair = Pair { left: 1, right: 2 };
  let left = &mut pair.left;
  let right = &mut pair.right;
  *left += 1i32;
  *right += 2i32;
  return pair.left + pair.right;
}
expect separate() == 6i32;
```

Named fields establish disjoint storage. Their loans reach final use before whole-record reading. A whole-record loan still live at either acquisition would conflict, R0201.

Case O6 is a complete module:

```rill
function change(value: &mut i32): void { *value += 1i32; }
function twice(value: &mut i32): void {
  change(value);
  change(value);
}
function result(): i32 {
  let mut value = 1i32;
  twice(&mut value);
  return value;
}
expect result() == 3i32;
```

Each borrowing call reborrows value rather than moving its exclusive descriptor. A separate `let moved = value;` in twice would instead transfer that descriptor.

Case O7 describes indirect overlap. Assume two distinct record fields contain references to possibly overlapping owners. Different field names do not prove different pointees. Conflicting access through them must reject, R0201, unless actual loan evidence establishes disjointness.

Case O8 is a rejected extension of O2:

```rill
function poison(slot: &mut &Point): void {
  let local = Point { x: 1, y: 2 };
  *slot = &local;
}
```

The caller's retained descriptor would refer to a body-local owner, R0202. Returning void or forwarding through remember cannot repair that exit.

Case O9 describes nested storage. Assume `retain(dst: &mut List<&User>, source: &User): void` is a visible checked contract. The callee can retain a caller-provided source. A callee-local User cannot escape through dst. Ending the call loan does not erase the stored source.

Case O10 describes owned-container extraction. Assume a checked List operation yields an existing contained `&User` by value from an owned List<&User>. Returning that reference can be valid when its incoming source remains valid. Returning a reference to the List's destroyed element slot is invalid, R0202. The model must not confuse these two operations.

Case O11 describes exclusive return dependencies. Assume `editAndView(&mut List<i32>): &i32` is a checked contract. The returned read-only view retains the exclusive input restriction. Reading or mutating the original whole List before its last use rejects, R0201. The read-only result is not a shared downgrade.

Case O12 describes a borrowed temporary. A reference to `shifted(Point { ... })` can be used within its statement. Storing that reference for a later statement is invalid when the temporary would end. Direct uninterpolated static string views retain RFC 0006's explicit exception.

Case O13 describes retained-state joins. One branch can store source A and another source B in the same destination. Both possible sources remain at the join until proof removes them. A loop can add sources through a backedge. A clear-like void call is not proof that either source ended.

Case O14 describes replacement. Assume an old owner and completed replacement both contain resources. Cancellation during old disposal remains pending through installation. After installation, delivered cancellation cleans the installed value once. A replacement borrowing freed old storage rejects, R0202/R0203, rather than receiving a masking exception.

Case O15 is a complete module for the confirmed capture policy:

```rill
function counter(): i32 {
  let mut initial = 0i32;
  let mut next: mut () => i32 = () => {
    initial += 1i32;
    return initial;
  };
  let first = next();
  let second = next();
  return first * 10i32 + second;
}
expect counter() == 12i32;
```

The stored closure owns its copied scalar counter. Its mutable invocation changes that copy. This follows the source prototype's stated intended behavior, not a test of a working capture analyzer.

Case O16 describes scoped capture formation. The approved source uses two owned jobs that each read `&numbers`. The confirmed capture policy stores those explicit borrowed accesses in their environments without moving the same List twice. Both dependencies last through joined child cleanup. Background spawn would still require noBorrow and reject that borrowed environment.

Case O17 describes invalid exclusive duplication. Assume a stored closure carries an exclusive caller borrow. An arrow annotation or record wrapper cannot make copying that environment safe. Repeated once invocation also cannot consume merely borrowed owned captures. Mode checks and copy evidence remain separate.

Case O18 describes the confirmed partial-move policy. A field can move from a record with only default field cleanup, leaving tracked remaining fields. The confirmed rule rejects non-copying field extraction from a value awaiting its custom disposer. Active disposal and valid rebuilding use their separate rules. The active custom disposer may move fields and leave the rest for compiler cleanup.

Case O19 describes cleanup ordering. Assume a retaining holder was initialized before a later source in the same cleanup scope. Its disposer reads the retained reference. The confirmed ordering policy disposes the holder before that source, then uses reverse declaration order for independent remaining values. It does not extend a source from an earlier-ending nested scope.

Case O20 describes transitive cleanup. An outer record has no custom disposer but contains a guard. Its direct reads end before the next interpolation expression. The compiler cannot release that guard early merely to avoid a second same-goroutine lock acquisition. Temporary cleanup remains observable.

Case O21 describes protected release. A module owns Payload and imports canonical dispose and LockGuard. Orphan permission for `dispose(LockGuard<Payload>)` does not permit replacing required guard release. Reject a non-trusted replacement during collection, R0500, even without a call.

Case O22 describes explicit disposal and aliases. The confirmed rule makes `dispose(view)` end that named descriptor value. Another copied descriptor can still retain the backing owner. Disposing the owner while that other descriptor remains needed rejects, R0203.

Case O23 describes symbolic evidence. Unconstrained T can contain references, including exclusive ones. A generic noBorrow operation requires the corresponding evidence before monomorphization, R0302. A read-only arrow type or concrete integer caller does not supply it retroactively.

Case O24 describes asynchronous retention. A canceled native operation still accesses an input buffer until completion or confirmed abortion. Handle drop and abort request cannot free that storage. Scoped children likewise finish cleanup before releasing caller sources. These are required outcomes, not a verified provider state machine.

| Case / artifact and revision | Context | Expected outcome | Owning rule | Evidence label / observed result |
| --- | --- | --- | --- | --- |
| O1/O3/O4/O5/O6, this draft | Complete modules and a separate moved-string rejection. | Preserve scalar copies, explicit mutation, carried borrows and reborrowing. | [Values](#ownership-values-and-places), [borrows](#borrow-acquisition-overlap-and-reborrowing). | NotChecked. No compiler execution. |
| O2/O7/O8/O9/O10/O11/O12, this draft | Explicit container/slot/result contracts and independent snippets. | Retain sources, distinguish descriptor storage and reject escape/conflicts. | [Layers](#proposed-layered-dependency-model), [calls](#implicit-call-contracts-and-callee-exits). | NotChecked. Nested route judgments remain incomplete. |
| O13/O14, this draft | Branch/backedge and replacement scenarios. | Preserve possible dependencies and confirmed cancellation/install order. | [Flow](#control-flow-updates-and-last-use), [replacement](#replacement-and-same-type-rebuilding). | NotChecked. No CFG or runtime state machine. |
| O15/O16/O17, this draft | Stored counter, direct scoped jobs and exclusive-capture cases. | Preserve copied-state and explicit-borrow captures without duplicated exclusivity. | [Captures](#proposed-capture-formation-and-erased-evidence). | NotChecked. The capture policy is confirmed; no analyzer exists. |
| O18/O19/O20/O22, this draft | Partial moves, cleanup order, transitive effects and descriptor aliases. | Apply the confirmed partial-move, ordering and consumption policies. | [Partial moves](#proposed-destructor-safe-partial-moves), [cleanup](#proposed-deterministic-cleanup-scheduling), [copies](#copies-and-consuming-operations). | NotChecked. The policies are confirmed. Formal checking remains incomplete. |
| O21/O24, this draft | Protected/native/child contexts. | Reject interception and retain sources until actual release. | [Protection/completion](#protected-actions-and-asynchronous-completion). | NotChecked. No protected registry or provider integration proof. |
| O23, this draft | Generic/property evidence scenario. | Reject missing evidence before monomorphization, R0302. | [Captures/evidence](#proposed-capture-formation-and-erased-evidence), Draft RFC 0004. | NotChecked. No generic checker. |

The [diagnostic registry](../diagnostic-codes.md) fixes cited meanings. Initialization, partial moves, cleanup cycles and capture failures still need exact diagnostic integration. An incomplete checker must not report semantic acceptance or successful resource release.

## Feature interactions

### Types, functions and primitive views

[RFC 0005](0005-types-records-enums-and-inference.md) owns finite layout and static variant relationships. Finite layout does not prove noBorrow or permit exclusive-reference copying. Pattern narrowing retains the original input dependency restrictions.

[RFC 0004](0004-functions-overloads-and-generic-requirements.md) owns static overload/evidence selection and callable modes. The selected implementation's ownership contract must agree with the checked generic body. A specialization cannot repair missing source evidence after monomorphization.

[RFC 0006](0006-numbers-strings-and-primitive-operations.md#canonical-core-conversion-interface) distinguishes borrowed conversion views from independent owned outputs. Ownership follows those actual contracts, not merely the spelling into or tryInto. Logical byte/text extents and local descriptor slots remain distinct.

[RFC 0003](0003-expressions-bindings-and-control-flow.md) owns sequencing and control targets. This RFC supplies initialization and dependency state at those transfers. Early return or propagation must complete normal required cleanup without changing error origins.

### Testing, concurrency and runtime admission

RFC 0001's accepted snapshot (ri-48g, comment 6907867211) owns expectations and trusted observations. Diagnostic capture adds no moves, copies, loans, formatters or lifetime extension. It cannot accelerate or delay ordinary cleanup.

Sendable/shareable remain separate compiler predicates with their own concurrency contract. A readonly callable is not necessarily pure, borrow-free or shareable. A genuine lock guard remains ownership-affine despite worker migration.

Shared ownership is explicit and counted. It does not grant ordinary mutable payload access or trace arbitrary cycles. Allowed counted/channel cycles may leak; this is not a garbage collector or a claim of cycle collection.

The runtime owns actual completion/adoption and source-retention state. Opaque metadata must justify ownership actions rather than fabricate them. Full native/Wasm layout, leases, stack and cancellation protocols remain unvalidated here.

### Source reconciliation

| Input | Disposition |
| --- | --- |
| ri-8hu decision (comment 6308080484), [ADR 0002](../adr/0002-visible-ownership-without-lifetimes.md) | Preserve visible ownership, scope cleanup, explicit counting and owned rebuilding. Later rules replace return-only dependencies, literal-copy assumptions and old copy-call syntax. |
| ri-lql decision (comment 6853088155), [ADR 0007](../adr/0007-explicit-mutation-with-conservative-borrow-dependencies.md) | Preserve mutable access, nested borrows, retained destination sources, callee exits and independent callable properties. The confirmed state policy still needs formal route/transfer judgments. |
| ri-40y decision (comment 6863910595), [ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md) | Preserve deferred disposal/replacement cancellation, required joins and actual native release. Do not invent force-kill, rollback or a global exit join. |
| ri-owe/#23 and [catalogue section 2](../standard-library-contracts.md#2-callable-and-ownership-evidence) | Preserve canonical hooks, broad ordinary placement, protected negative actions and remaining-field cleanup. Full trusted registry and overlap/evidence checking remain incomplete. |
| ri-a2c/#19/#21 and Draft RFCs 0005/0006 | Preserve ordinary base types, static literal views, logical extents, named rebuilding and enum copy classification. No optimizer-dependent ownership or private representation access follows. |
| ri-636 and [stack decision](../adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) | Preserve bounded-stack default cleanup and actual execution-storage retention. A worklist is not a tracing collector or a passed target support gate. |
| ri-3t8 and [diagnostic contract](../diagnostic-contracts.md) | Preserve observation without extra callbacks, retained owners or changed cleanup. Keep source/dependency failures distinct from incomplete checking. |
| ri-vtw and [error decision](../adr/0013-caused-by-errors-with-explicit-wrapping.md) | Preserve unique causes, exact forwarding and retained error evidence. Owned envelopes can still carry typed dependencies; error ownership remains companion scope. |
| [Mutation source](../../prototypes/06-mutation.rill), [independent safety cases](../prototypes/06-mutation-safety.md) and [concurrency source](../../prototypes/04-concurrency-safety.rill), source revision | Preserve their accepted-design obligations, including stored references, copied counters and directly borrowed scoped jobs. They are not executed Rill tests. No artifact bytes change here. |

## Compatibility and migration

Old return-only dependency rules are insufficient. Function exits and calls must account for references retained through mutable arguments and nested owned inputs. Passing an argument owned or copied is not a universal workaround.

Use current `copy(&value)` rather than historical owned-copy convenience. Preserve explicit move/copy/access distinctions through core conversion entry points. Their names do not imply source independence or lifetime extension.

The confirmed cleanup order, partial-move restriction and capture policy fill specification gaps. They are not retroactive proof that historical examples already compile. Any contradiction with an approved required use case returns to design review.

No released ownership-checker or runtime ABI exists. This draft does not choose a concrete closure layout, allocator protocol, guard layout or pointer-pinning mechanism. Existing standalone backend experiments retain their original evidence scope.

## Security/resource implications

Hidden aliasing, local-source escape or premature native release can become memory unsafety. Type shape, private fields and a valid signature alone do not prove correct ownership. Static checks and trusted runtime transitions both need validation.

Dependency graphs, dataflow, cleanup worklists and captured state can consume memory and CPU. A checker that exhausts its analysis resources must report incomplete analysis rather than success. A cleanup allocation failure stays fatal, not an unsafe fallback.

Cleanup and disposal can suspend or block indefinitely. Cancellation is cooperative and does not guarantee rollback or bounded return latency. Pending guards and child/native work must not be abandoned to make progress appear prompt.

No runtime borrow checker, tracing GC or hidden reference counting is introduced. Explicit counted cycles can leak. This draft claims neither complete leak freedom nor a combined soundness proof.

## Rationale, alternatives and prior art

Forbidding references inside owned values would simplify summaries but reject approved programs. Programmer-written lifetimes could improve precision but change the approved source model. Neither is proposed as a hidden restriction or repair.

A flat dependency set cannot distinguish a container slot from an existing reference stored there. The layered model keeps that distinction explicit. It still needs formal route rules. Calling it conservative does not establish safety.

Per-call body inspection could reduce dependencies for clear-like helpers. It would also make caller contracts depend on implementation details. The source decisions instead require signature/evidence-based checking.

Dependency-first cleanup can preserve valid same-boundary holders without extending source scopes. Stable reverse-order ties make independent cleanup predictable. Custom-destructor partial-move restrictions avoid calling user code on incomplete values during cancellation.

The confirmed capture policy preserves explicit shared borrowing for scoped jobs while keeping stored copied counters independent. Treating every owned closure as borrow-free would violate both the source model and generic property checks.

Rust-inspired ownership vocabulary does not prove this checker works. Rill deliberately differs by omitting programmer-written lifetime annotations and using broader ordinary family rules. Supporting decisions and inventories remain provenance, not an imported proof.

## Implementation implications

The compiler needs place identities, initialization/drop state and layered source routes. Keep definite initialization separate from possible dependencies. Call-summary instantiation must preserve actual source modes and scope limits.

Control flow needs monotone merges and fixed points without discarding retained versions at backedges. Projection and overwrite rules must distinguish old access from newly carried data. Missing route information must not silently become an empty dependency set.

Capture lowering must preserve where borrowing or movement occurs. No side-effectful body computation may execute earlier merely to form an environment. Generic/erased metadata must preserve actual restrictions and only justified positive evidence.

Cleanup lowering needs explicit pending/disposal/installed states, dependency ordering and cancellation deferral. Remaining-field cleanup must not recursively reenter the outer disposer. Worklists need correct ownership when suspension or fatal exhaustion occurs.

Trusted operations need canonical admission and non-interception checks. Actual native/child completion must agree with static retention assumptions. The current Rust scaffold supplies none of these product implementations.

## Evidence and checking plan

### Actual evidence

| Artifact / revision | Evidence label | Rule/question exercised | Checker/tool and command/environment | Expected / observed outcome | Limits, negative/unavailable results and gate links |
| --- | --- | --- | --- | --- | --- |
| Source decisions and repository base `f33dcf4cf3f815824eeb897135775d7884f65c1a` | HandChecked | Approved ownership/mutation direction and later cleanup refinements. | Drafting agent, 2026-11-16. Read current ri-8hu/#15/#16 decisions, ADRs and catalogue section 2. | The drafting agent reconciled retained-storage and protected-cleanup requirements. | Provenance review is not a proof or a complete checking algorithm. |
| O1 through O24, this draft | NotChecked | Moves, borrows, capture/state precision and cleanup. | No Rill ownership checker or runtime is available. | Expected outcomes appear above. No compiler trace or cleanup execution is observed. | No case is RillExecuted. Policy approval does not complete formal rules or companions. |
| Layered routes, capture precision and cleanup scheduling | NotChecked | Nested calls, aliasing, partial moves and resource ordering. | No reference model or formal proof has been run. | No termination/completeness/soundness result is claimed. | Complete rules and adversarial checking are required before implementation readiness. |
| Prototype and runtime research artifacts | NotChecked | Executable acceptance of this combined ownership system. | Existing files retain their own historical evidence scopes. | No new reference/native/Rill execution occurred for this draft. | A backend mechanism experiment or authored expected result does not validate this checker. |
| RFC documents and index, local working draft | ToolChecked | Structure, writing limits, local links and preserved prior rules/artifacts. | Drafting agent, 2026-11-16. Temporary Python checks; `mise run check:docs`. | Seven file/register IDs and required metadata/headings passed. Sentence/paragraph, fence and whitespace checks passed. Local destinations resolved without errors. | Document checks do not validate a borrow model, execute Rill or audit the formal ASD-STE100 dictionary. |

### Planned checks. Not executed evidence

- Track whole/field moves and definitely initialized state on every continuing path.
- Test scalar/aggregate implicit copies separately from custom and protected copying.
- Distinguish slot borrows, contained references, logical extents and copied independent owners.
- Accept owned-container reference extraction while rejecting destroyed-storage references.
- Test nested mutable destinations, intermediate calls and callee-local source escape.
- Preserve exclusive restrictions through read-only outputs and erased wrappers.
- Check strong/weak updates, branch unions, loop fixed points and disposal-sensitive last use.
- Test copied counters, direct scoped borrows and side-effectful capture expressions.
- Reject exclusive-capture duplication and missing generic properties before monomorphization.
- Test default and custom partial moves, incomplete construction and same-type rebuilding.
- Model cancellation at every replacement/disposal/installation transition.
- Test dependency-ordered cleanup, reverse-order ties and unresolvable source requirements.
- Reject protected interception through aliases, generic adapters and function values.
- Retain actual child/native sources through completion or confirmed abortion.
- Validate bounded-stack default cleanup and target-specific source/stack stability.
- Exercise diagnostic dependency paths without running user code during checking or observation.

These require a real checker/runtime and separate native/Wasm profile acceptance. Document checks cannot clear those gates.

## Open/deferred questions

<a id="review-choices"></a>

### Confirmed policies

The project owner approved all five policies (ri-48g, comment 6915849379) on 2026-11-16. The pre-recording snapshot has SHA-256 `59d1bca35706045c2f0fda08a1d12847c79f99ceaad8316a17daafe0f7cf4c70`.

1. Separate storage-access loans from carried references. Track definite initialization separately from possible retained sources, including branch merges.
2. Preserve stable explicit free-place borrows in owned environments. Bare owned captures move or implicitly copy. Preserve scoped borrowed jobs and independent copied counters.
3. Prevent partial moves from leaving incomplete outer values awaiting custom disposal. Active disposal can transfer fields; remaining-field cleanup skips moved or disposed fields. Preserve valid rebuilding.
4. Clean eligible dependents before their sources within the same cleanup boundary. Use reverse declaration order for independent locals. Preserve the stated aggregate/temporary ordering. Do not advance observable release.
5. Make explicit canonical dispose consume its named operand, including implicitly copyable descriptors. Other copied aliases retain their dependencies. Ordinary function-call copying remains unchanged.

These approvals do not accept future unwritten rules or a complete checking mechanism. Valid owned-container extraction and retained-slot cases remain required. Complex captures, nested routes, erased evidence and complete cleanup graphs still need formal rules.

### Remaining in-scope authoring work

Complete nested route transfer, callee exit judgments and type-compatible input-to-destination flow without spurious self-dependencies. Resolve projection of contained references without losing exclusive-source restrictions.

Complete callable erasure/copy evidence and custom-disposer interaction with implicit-copy classification. Formalize pattern/guard commitment and source availability across partial moves, replacements and same-type rebuilding.

Specify cleanup graphs across nested scopes, custom-body exits, temporaries and partial initialization. Resolve self-dependent cleanup failures without introducing unapproved pinning, scope extension or hidden collection.

Define the protected operation registry and trusted metadata admission with runtime/concurrency companions. Verify the combined rules against the source inventories and adversarial examples. Unworkable required cases return to map ri-kl0, not silent language weakening.

These gaps prevent the [canonical readiness review](README.md#canonical-reviewready-checklist). Missing rules are not supplied by linking a supporting ADR.

### Implementation/validation gates and future scope

Implement the checker, capture lowering, cleanup state machines and diagnostics. Validate them with actual Rill executions and the target/runtime matrix. No reference model or directional approval clears those obligations.

This RFC adds no lifetime annotation syntax, runtime borrow checks, weak references, tracing GC, public pinning or forced cancellation. It does not redesign standard-library protocols or introduce an implicit global join.

## Decision and change history

### Readiness record

No canonical readiness review has occurred. This document remains Draft. The drafting agent checked sentence/paragraph limits and applied unslop as the final prose pass. Structure and link checks passed.

These checks cannot complete missing judgments or establish soundness. Formal ASD-STE100 dictionary compliance remains unverified.

### Human decisions and amendments

The project owner requested the next foundation RFC on 2026-11-16. That request authorized drafting ownership, borrowing, mutation and cleanup. It did not accept new policies or the full RFC.

The project owner then approved all five policies (ri-48g, comment 6915849379). This approval identifies the pre-recording snapshot below. It does not accept the full RFC, supply missing judgments or establish readiness. Full Decision and Reviewed revision remain Not yet decided and Not yet reviewed.

| Date / actor | Kind and scope of change | Reviewed revision or snapshot | Decision link / rationale |
| --- | --- | --- | --- |
| 2026-11-16, drafting agent | Created substantive Draft RFC 0007, register and partial coverage entries. | Source base `f33dcf4cf3f815824eeb897135775d7884f65c1a` and source decisions cited above. This is not a reviewed RFC snapshot. | User request to continue to the next foundation RFC. No new policy or full RFC is accepted. |
| 2026-11-16, project owner | Approved layered state, capture formation, destructor-safe partial moves, cleanup ordering and explicit-dispose consumption. | Pre-recording draft SHA-256 `59d1bca35706045c2f0fda08a1d12847c79f99ceaad8316a17daafe0f7cf4c70`. Local snapshot `/tmp/rill-rfc-0007-before-policy-approval.md`. | Five-policy decision (ri-48g, comment 6915849379). Full RFC acceptance, complete judgments, readiness and implementation validation remain open. |
| 2026-11-16, drafting agent | Removed redundant numeric suffixes from record field initializers in O1/O2/O5/O8. | Existing record-field and numeric-context rules in Draft RFCs 0005/0006. The five approved ownership policies remain unchanged. | Owner clarification that declared Point fields supply numeric context. Editorial example changes only; no numeric default or execution evidence. |
