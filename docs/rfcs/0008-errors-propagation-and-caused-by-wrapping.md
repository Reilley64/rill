# RFC 0008: Errors, propagation and caused-by wrapping

| Field | Value |
| --- | --- |
| ID | 0008 |
| Status | Draft |
| Owner | Reilley64. [#28](https://github.com/Reilley64/rill/issues/28) tracks this draft. |
| Source inputs | [#29 error decision](https://github.com/Reilley64/rill/issues/29), [#23 library decision](https://github.com/Reilley64/rill/issues/23#issuecomment-6872129850) and [#24 diagnostic decision](https://github.com/Reilley64/rill/issues/24#issuecomment-6885970775). [Source reconciliation](#source-reconciliation) identifies related decisions and artifacts. Repository inputs use commit `f33dcf4cf3f815824eeb897135775d7884f65c1a`. |
| RFC dependencies | Draft RFCs [0002](0002-modules-and-program-loading.md), [0003](0003-expressions-bindings-and-control-flow.md), [0004](0004-functions-overloads-and-generic-requirements.md), [0005](0005-types-records-enums-and-inference.md), [0006](0006-numbers-strings-and-primitive-operations.md) and [0007](0007-ownership-borrowing-mutation-and-cleanup.md) propose shared rules. Their full texts are not accepted. Required [companions](#required-companion-rules) remain incomplete or missing. |
| Replaces / superseded by | None. This draft does not amend an Accepted RFC or accept another draft. |
| Decision | Not yet decided |
| Reviewed revision | Not yet reviewed |

## Summary and scope

This RFC proposes typed error wrappers, Result/Option error operations, construction origins, propagation, explicit causes and standard runtime rendering. It also owns the sealed heterogeneous-cause boundary and error-specific evidence obligations.

The glossary defines [error wrapper, retained cause and caused-by wrapping](../../GLOSSARY.md#language). A typed primary error may carry valid borrows. Retaining it as an erased cause requires additional evidence, independently of its outer wrapper.

The source decisions already fix unchanged propagation, lazy error-value wrapping and uniquely owned causes. This draft preserves RFC 0003's confirmed precedence and whitespace-insensitive question-mark parsing. It does not introduce exception unwinding, automatic error conversion or propagation traces.

The project owner [confirmed origin attribution, closed-constructor typing and formatter sequencing](#confirmed-policies). Complete dependency judgments, trusted metadata admission and runtime integration still need work. This document remains Draft, not an accepted error specification or safety proof.

General conversion coherence and compiler-report schemas remain separate review units. This RFC does not redesign their APIs or claim that a linked draft supplies missing rules.

## Motivation

Returning an error should not add context that the programmer did not write. Explicit wrapping should preserve the original cause rather than replace it with a string. Typed payloads must remain usable without mandatory message fields or reflection.

Ownership matters even when a failure appears to be a simple outer value. A hidden cause must not conceal an invalid borrow or goroutine-affine resource. Standard rendering must retain every cause without growing its own stack with chain depth.

Construction, forwarding and payload transformation also need distinct origin behavior. Otherwise helpers, inlining and combinators could change observations without changing the error's meaning.

## Guide-level explanation

Case R1 is a complete proposed module:

```rill
import std::result::{Result, Ok, Err, isOk};
import std::error::{display, cause};
import std::option::Option;

type ReadFailed = { code: u32 };
type LoadFailed = {};

implement display(failure: &ReadFailed): string {
  return `read failed: ${failure.code}`;
}
implement display(failure: &LoadFailed): string {
  return "load failed";
}
function read(succeed: bool): Result<u32, ReadFailed> {
  if (!succeed) { return Err(ReadFailed { code: 13 }); }
  return Ok(42);
}
function load(succeed: bool): Result<u32, LoadFailed> {
  let result = read(succeed) ? LoadFailed {};
  return Ok(result);
}
function hasCause(): bool {
  guard let result = load(false) else (failure) {
    return match (cause(&failure)) {
      Option.Some(_) => true,
      Option.None => false,
    };
  }
  return false;
}
expect isOk(&load(true));
expect hasCause();
```

ReadFailed has no message or cause field. Its code field supplies u32 context for 13. The declared Result supplies context for Ok's success value. No numeric default is needed.

Load returns the successful number unchanged. On failure, it constructs LoadFailed once and returns immediately with the original failure as cause. The operator does not recover, invoke a handler or supply a default number.

Case R2 is a separate complete module:

```rill
import std::result::{Result, Err};
import std::error::{display, cause};
import std::option::Option;

type BorrowedFailure = { detail: &string };
implement display(failure: &BorrowedFailure): string {
  return copy(failure.detail);
}
function fail(detail: &string): Result<u32, BorrowedFailure> {
  return Err(BorrowedFailure { detail: detail });
}
function inspect(): bool {
  let owner = "caller-owned detail";
  guard let result = fail(&owner) else (failure) {
    return match (cause(&failure)) {
      Option.Some(_) => false,
      Option.None => true,
    };
  }
  return false;
}
expect inspect();
```

This primary typed failure retains the caller's string. Ordinary forwarding can preserve that dependency. Erasing BorrowedFailure into a retained cause would fail noBorrow evidence, even if the new outer error were borrow-free.

Case R3 is a complete module illustrating the confirmed origin policy:

```rill
import std::result::{Result, Err};
import std::error::error;

function createFailure(): Result<u32> {
  return Err(error("created here"));
}
function forwardFailure(): Result<u32> {
  return createFailure();
}
```

The error helper creates an Error payload, not an error wrapper. Err creates the wrapper inside createFailure. The forwardFailure helper preserves that origin under the confirmed policy; it does not attribute construction to its own call site.

All examples state intended behavior. No Rill compiler or runtime has checked or executed them.

## Syntax and reference semantics

### Public values, paths and namespaces

The required public shapes follow the library decision:

```rill
// std::option
export enum Option<T> {
  Some(T),
  None,
}
// std::result
export enum Result<T, E = Error> {
  Ok(T),
  Err(Err<E>),
}
```

`Err<E>` is a separate nominal record with private fields. Its type name and canonical value-construction name occupy separate namespaces. Result's Err variant contains a valid wrapper; it does not expose those private fields.

The canonical default E is std::error::Error. That default does not inject an Error source binding. Ordinary imports still provide Result, Err, error and related public names.

| Module | Required exports in this RFC's scope |
| --- | --- |
| std::option | Option, Some, None, map, andThen, isSome, asRef, asMut. |
| std::result | Result, Ok, Err, map, mapErr, andThen, isOk, ok, err. |
| std::error | Error, error, Cause, SourceLocation, display, value, intoValue, origin, cause, formatLocation, formatError, formatCause. |

No broad std import, public dyn mechanism or new implicit prelude name follows. Namespace and alias rules belong to RFC 0002. Named records and qualified enum variants follow RFC 0005.

### Wrappers, payloads and access

An `Err<E>` owns its typed E, construction SourceLocation and optional uniquely owned Cause. E needs canonical display evidence for construction. It needs no injected fields or particular record/enum shape.

Primary E does not universally require noBorrow, sendable or shareable. Its actual dependencies and restrictions remain visible through the wrapper. The complete value, including opaque metadata, must justify any claimed property.

| Operation | Signature or required behavior |
| --- | --- |
| error | `error(string): Error`; owns the message text. |
| display | `display<E>(&E): string`; canonical family, one payload only. |
| value | `value<E>(&Err<E>): &E`; borrowed inspection. |
| intoValue | `intoValue<E>(Err<E>): E`; transfers E and disposes the remaining origin/cause state. |
| origin | `origin<W>(&W): &SourceLocation`; canonical family for `Err<E>` and Cause. |
| cause | `cause<W>(&W): Option<&Cause>`; canonical family for `Err<E>` and Cause. |
| formatLocation | `formatLocation(&SourceLocation): string`; independent owned observation text. |
| formatError | `formatError<E>(&Err<E>): string where display(E)`; full standard rendering. |
| formatCause | `formatCause(&Cause): string`; full retained-chain rendering. |

Borrowed accessors do not clone or consume a wrapper. Their results retain the sources needed by the observed wrapper. The intoValue operation intentionally discards metadata rather than forwarding an Err unchanged.

SourceLocation remains opaque. It owns its required data or uses justified immutable compiler metadata. It cannot hide a reference to an expired source buffer or user temporary.

SourceLocation exposes no fixed public coordinate fields or machine-readable format. The formatLocation text is not a parser interface. Compiler diagnostics use their separately specified source/snapshot coordinates.

No standard copy of Err or Result follows from copyable payloads. A cause can contain non-copyable state. Reconstructing an error through a user operation is not a promise of unchanged wrapper forwarding.

<a id="proposed-closed-err-construction-and-typing"></a>

### Closed Err construction and typing

Canonical Err supports exactly one or two value arguments. The following schemas describe the closed operation, not ordinary overload declarations:

```text
Err<T, E>(value: E): Result<T, E>
  requires display(E)
Err<T, E, C>(value: E, inner: Err<C>): Result<T, E>
  requires display(E), display(C), noBorrow(C), sendable(C), shareable(C)
```

Use success type T first, error type E second, and cause type C third. Only the two-argument form admits C. Omitted arguments use immediate context and independently typed values, not new defaults.

A known expected `Result<T,E>` supplies its success and error context. Check the payload against that E under ordinary permitted relationships. Without established E context, infer E from the independently typed payload.

An established E can supply numeric context for an unsuffixed payload. An untyped number alone cannot choose E's width. T still needs explicit or immediate context; error values do not determine a success type.

An explicitly supplied type argument must agree with the established context. Do not retry with another E after an incompatible payload. No desired return type ranks an ordinary display implementation or repairs missing evidence.

`Err<u32, string>("failed")` produces a string-error Result under the confirmed constructor policy. `Err<u32>("failed")` infers E=string when no incompatible context fixes it. `Err<u32, i32>(1)` has numeric context; `Err<u32>(1)` has no numeric width.

A default-Error Result does not accept Err("failed") by implicit string conversion. Write Err(error("failed")) instead. Error's nominal identity remains distinct from string.

Initial payload checking can use a permitted variant-to-parent relationship when E is already established. An existing `Err<Variant>` does not become `Err<Parent>` by container covariance. After construction, the wrapper's E is fixed.

Evaluate value arguments once, from left to right. The two-argument form consumes an owned `Err<C>` and preserves its complete existing chain. It does not accept a borrowed wrapper merely because C is copyable.

Err is not a general variable-arity/default-argument feature or ordinary first-class function value. Adapt it with an arrow whose body performs construction. Import aliases preserve its canonical identity and closed construction behavior.

Raw `Result<T,E>.Err(wrapper)` requires an already valid `Err<E>`. It records no second origin and cannot fabricate private wrapper state. Ok similarly adds no error origin and follows ordinary contextual generic inference.

The ordinary helper has `Ok<T,E=Error>(value:T):Result<T,E>`. It needs an actual T value. When T=void, pass a void-valued expression. No general Ok() convenience or arbitrary zero-argument void-value syntax is introduced.

<a id="proposed-construction-origin-attribution"></a>

### Construction-origin attribution

Err(value) and Err(value,inner) record the source construction expression. Anchor the origin at that written constructor call's source start. Canonical aliases retain the alias call's location, not its import declaration.

Infix wrapping records its question-mark token as the new outer origin. Error payload construction alone records no Err origin. Raw variant injection, returns and ordinary moves add none.

A user-written factory that calls Err records the Err expression inside that factory. Calling the factory does not replace that origin with the caller's location. A forwarding helper preserves the wrapper it receives or returns.

An arrow adapting Err likewise constructs at its own body expression. Compiler-generated constructor helpers do not become user-visible construction sites. Inlining, monomorphization and optimization preserve original source-map attribution.

Locations refer to the source snapshot that produced the executable. Do not recover origins by reading a potentially changed source file during formatting. This policy does not add public paths, line fields or stack traces.

Native/provider construction and generated source need explicit approved attribution metadata. Their complete admission rules remain companion work. A missing map must not silently invent a successful source-location observation.

### Postfix propagation and normal return

RFC 0003 owns question-mark parsing, precedence and control targets. Parse before typing without type-based retries. Whitespace/comments do not distinguish postfix propagation from infix wrapping.

For owned `Result<T,E>`, postfix ? yields T on success. On failure, move the same `Err<E>` into the enclosing `Result<U,E>` failure variant. U need not equal T; the failure carries no U success value.

E must be the same canonical type. No implicit error conversion, Err covariance or display-based adaptation applies. The enclosing callable must have the required Result-returning context.

The source must permit moving the selected failure. A read-only borrowed Result supplies no owned wrapper for propagation. Use borrowed queries/guards for inspection rather than cloning the cause.

Postfix ? on `Option<T>` yields T or returns None from an enclosing `Option<U>`. None does not acquire an origin or become an Err. This RFC adds no implicit Option-to-Result conversion.

Ordinary return of an existing Result preserves its wrapper's origin and chain. Return-value typing still applies to the complete result. Neither return nor propagation appends history or captures a backtrace.

Normal early exits perform the required ownership cleanup. They are not fatal unwinding or rollback. Accepted RFC 0001 forbids propagation that exits an expectation; helpers/nested closures keep their own contexts.

### Infix caused-by wrapping

`inner ? outer` is Result-only wrap-and-immediate-propagation. The right operand is an ordinary error value expression. It is not an implicitly called constructor, mapper, recovery branch or default success value.

1. Evaluate the left Result once.
2. On success, yield its success value without evaluating or moving the right operand.
3. On failure, retain ownership of the existing inner wrapper.
4. Evaluate the right error value once.
5. Construct the outer wrapper at the operator's source location.
6. Transfer the complete inner wrapper into its retained cause.
7. Return the new failure from the enclosing Result-returning callable.

The outer value must fit the enclosing error type under construction's permitted relationships. The inner error type may differ without conversion. Its retained-cause evidence remains mandatory before monomorphization.

The successful expression type comes from the left Result. The enclosing success type can differ when later code constructs that function's successful result. A wrapping failure never supplies an error value to the local success binding.

Do not execute or consume outer state on the success path. Normal cleanup still handles owned values that remain at their scope boundary. Skipping the RHS does not suppress unrelated required disposal.

If the RHS suspends, its pending inner wrapper remains owned and valid. If normal cancellation exits the operation before construction completes, clean the values owned in that reached state. Do not attach the same cause twice or publish partial wrappers.

A panic in an evaluated RHS stays fatal. A panic in a skipped RHS does not execute. Static type/evidence checks still apply to unexecuted source.

### Retained causes and sealed evidence

Retaining `Err<C>` requires canonical display(C), noBorrow(C), sendable(C) and shareable(C). The requirements concern the actual inner payload, not just the outer error. Existing retained descendants already satisfy the same invariant.

Cause owns the inner payload, original location, next cause and required formatter/disposal metadata. It is sealed and uniquely owned. No public Cause constructor, downcast, runtime registry or dyn interface follows.

At retention, preserve the justified typed formatter and disposal operations through erasure. Do not recover them by a type-name search during rendering. Named display implementations carry no implicit runtime-let captures.

Intrinsic property evidence must survive generic forwarding and opaque interfaces. A user overload cannot certify noBorrow, sendable or shareable. Owning an outer wrapper does not supply missing inner evidence.

Primary `Err<C>` may retain valid caller borrows without these extra erasure guarantees. Ordinary forwarding preserves them. Use an explicit suitable owned adaptation when a borrowed/affine error cannot become a cause.

The proposed property derivation for `Err<E>` must include E, SourceLocation and optional Cause. Cause's sealed invariant supplies its borrow/transfer/sharing guarantees, not copyability. Hidden representation data needs actual trusted justification.

A candidate internal representation uses compiler-generated typed nodes owning payload, location and next link. Typed metadata supplies rendering and release operations without exposing type erasure publicly. This is an implementation sketch, not an accepted ABI or completed safety argument.

### Consuming combinators and borrowed queries

| Operation | Signature and callback requirement |
| --- | --- |
| Option map | `map<T,U,F>(Option<T>,F): Option<U>`; callableOnce(F,[T],U). |
| Option andThen | `andThen<T,U,F>(Option<T>,F): Option<U>`; callableOnce(F,[T],`Option<U>`). |
| Option inspection | `isSome(&Option<T>): bool`; `asRef(&Option<T>): Option<&T>`; `asMut(&mut Option<T>): Option<&mut T>`. |
| Result map | `map<T,U,E,F>(Result<T,E>,F): Result<U,E>`; callableOnce(F,[T],U). |
| Result andThen | `andThen<T,U,E,F>(Result<T,E>,F): Result<U,E>`; callableOnce(F,[T],`Result<U,E>`). |
| Result mapErr | `mapErr<T,E,D,F>(Result<T,E>,F): Result<T,D>`; callableOnce(F,[E],D), display(D). |
| Result inspection | `isOk(&Result<T,E>): bool`; `ok(&Result<T,E>): Option<&T>`; `err(&Result<T,E>): Option<&Err<E>>`. |

Consuming operations own their input and callback. Evaluate argument expressions under ordinary left-to-right call rules. A callback body runs at most once, only for its selected variant. Creating that callable argument is not itself lazy.

The map and andThen operations forward an existing failure unchanged. They do not rebuild its payload, origin or cause. The andThen operation also forwards a callback-produced failure unchanged.

The mapErr operation changes E to D while preserving the existing origin and optional cause. It adds no new causal layer or construction site. This differs from both Err(newValue,oldWrapper) and intoValue(oldWrapper).

During mapErr's callback, keep the original metadata under an initialized internal ownership state. Normal cancellation disposes that state and remaining callback-owned data once. Do not invoke ordinary cleanup on a public wrapper with a missing payload.

A returned value cannot borrow destroyed callback captures, moved payload storage or local owners. Incoming carried references remain distinct from those storage loans. Complete judgments depend on RFC 0007, not the spelling mapErr.

Borrowed queries preserve dependencies and grant no ownership of a failure. Mutable Option views preserve exclusive access and allow no reference duplication. No borrowed Result rewrap may clone its optional unique cause.

<a id="proposed-standard-rendering-sequence"></a>

### Standard rendering sequence

The display operation formats one typed payload through a borrow and returns owned string text. It may suspend, mutate permitted external state or panic. There is no reflected fallback or per-instance formatter argument.

For the standard renderer, use outer-to-inner chain order. At each node, invoke its justified payload formatter exactly once. Render that node's origin before advancing to its immediate cause. Complete that node's normal temporary cleanup before invoking the next node's formatter.

Render every explicit retained cause without truncating the normal chain. Standard formatError starts at its typed outer wrapper. Standard formatCause starts at the supplied retained node. These calls borrow input and return independent owned text.

The standard show bridges use formatError for `Err<E>` and formatCause for Cause. Required display(E) evidence remains explicit. Ordinary show identity, placement and reference-forwarding rules remain; this RFC does not seal all formatting families.

The sequence policy does not fix punctuation, whitespace, source-coordinate text or a machine-readable message grammar. User-supplied newlines remain payload text, not synthetic causes. Do not parse output to recover cause identities or locations.

Hold every required input/source restriction through suspendable formatting and cleanup. Returning a string does not extend input loans. Callback effects are not transactional and need not be pure or deterministic.

Bound standard traversal/release stack use independently of chain depth. Iterative state/worklists are permitted; their memory can grow with explicit data. User formatters/disposers retain ordinary call behavior and can recurse or block.

Allocation, callback panic or fatal exhaustion provides no completed-rendering guarantee. Do not truncate silently, substitute reflected output or use unsafe recursive fallback. Diagnostic-preview budgets do not apply to this separate runtime renderer.

### Panic, cancellation and reporting boundaries

Recoverable errors use explicit Result flow. Panic, integer traps and fatal allocation/stack failures retain their no-unwind policy. A well-typed panic is not a catchable Err.

Intrinsic cancellation is a control/cleanup path, not an implicit Result failure. Explicit cancellation-token operations can return their separately typed outcomes. Neither behavior supplies rollback or bounded cleanup latency.

Native operations can report typed resource failures without making Rill allocator failure recoverable. Error construction itself can still fail fatally. Required child/native cleanup must finish before releasing accessed sources.

False expectations remain ordinary test failures under Accepted RFC 0001. Compiler diagnostics and trusted expectation previews must not call user display/show or retain live error wrappers. Their bounded observations do not replace normal full cause rendering.

### Proposed M4.6 exact error evidence

[M4.6 #63](https://github.com/Reilley64/rill/issues/63) proposes these equality facts for [RFC 0004's ordinary proof procedure](0004-functions-overloads-and-generic-requirements.md#symbolic-bodies-and-evidence-goals). Approval remains pending. This stage does not implement conversion selection or admit an error wrapper.

An ordinary requirement can name an exact error argument even when its input is symbolic. Preserve the canonical family, input terms and exact output relation. For example, `read(R,E)` retains both R and E. It is not evidence for any unspecified error type.

In a generic body, use the written requirement or an independently justified relation. Do not assume a concrete E from a currently visible fallback. At instantiation, use the normal ordinary winner. Compare its selected output against the full substituted contract. A mismatch fails that requirement without retrying another implementation.

Check an equality of known error terms by canonical identity and full arguments. The same rigid E can establish equality. Distinct rigid terms do not become equal through spelling, parent admission or a shared bound. An unresolved selected term supplies no equality. Keep the two terms and both source sites in the witness.

This equality can be tested independently of a standard-library wrapper. Operations on actual `Result`, `Option`, `Err` and Cause require their admitted identities and contracts. Preserve pending obligations when those inputs are missing. Do not substitute record resemblance for admission. Positive constructor, propagation, cause-retention and result-use integration remain in M4.8. M4.7 owns conversion-selected E and its separate diagnostics.

### Proposed M4.8 Option, Result and Err checking

[M4.8 #65](https://github.com/Reilley64/rill/issues/65) proposes this handoff to the shared typed checker. It requires separate checkpoint approval. These judgments need the admitted roles and applicable contracts from [RFC 0002](0002-modules-and-program-loading.md#proposed-m48-core-admission-and-interface-checks).

Validate the exact Option and Result variant schemas, Result's canonical default, and the distinct Err type and closed constructor roles. Do not infer those roles from representation. Check raw variant construction with the ordinary named data rules. `Result.Err` consumes a supplied wrapper value under the later ownership rules. It does not create an origin.

Use the existing [closed Err procedure](#proposed-closed-err-construction-and-typing) only for the admitted value declaration. Alias calls retain the written call site. Check `display` with ordinary evidence. Check retained cause properties with compiler predicates and the actual inner payload type.

Keep the typed inner wrapper, exact C and cause evidence in the facts. A missing or unsupported proof cannot synthesize cause metadata or replace C. Keep no constructed wrapper fact for an incomplete operation.

Supply the canonical variant identities and substitutions to [RFC 0003's guard checking](0003-expressions-bindings-and-control-flow.md#proposed-m44-match-and-guard-control). Retain the success payload type and the failure path before introducing pattern bindings. For a Result error binding, supply `Err<E>` with the actual container access.

For postfix propagation, check the operand once and require an owned Option or Result type. The nearest callable must return the same container kind. For Result, compare E exactly with the enclosing E. Keep the original Err identity and no new origin. Do not require equality between operand and enclosing success types.

Propagation that leaves startup or an expectation is invalid. A nested callable keeps its own return context.

For infix wrapping, check the owned left Result and the outer error expression once each. A borrowed left Result does not supply an owned inner wrapper. Use the enclosing Result's E for the immediate outer expression context. Require canonical `display(E)` for the new wrapper and the retained cause evidence on the inner C.

Keep the left success type and separate early-return edge. Record the question-mark token as the new origin. Type checking the right operand does not assert its evaluation on the success path.

The report can retain operand, result, origin, branch and evidence facts. It cannot assert construction of a runtime owner, a valid move, a release order or rendering. Use [RFC 0003](0003-expressions-bindings-and-control-flow.md#proposed-m48-result-use-checking) for the separate use obligation after construction or propagation.

### Required companion rules

| Rule owner | Required contribution | Current dependency state |
| --- | --- | --- |
| [Draft RFC 0002](0002-modules-and-program-loading.md) | Canonical identities, namespace aliases, private wrapper access and standard metadata loading. | Filesystem/order policies are confirmed. Complete resolver/protected metadata rules remain incomplete. |
| [Draft RFC 0003](0003-expressions-bindings-and-control-flow.md) | Parsing, precedence, evaluation order, control targets, guards and explicit Result discard. | Four policy groups are confirmed. Complete expression/pattern/unused-result judgments remain incomplete. |
| [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md) | Generic evidence, callable requirements, ordinary selection and first-class adapters. | Three precision policies are confirmed. Complete symbolic evidence procedures remain incomplete. |
| [Draft RFC 0005](0005-types-records-enums-and-inference.md) | Nominal wrappers, enum variants, generic contexts and finite layouts. | Three precision policies are confirmed. Formal typing/representation boundaries remain incomplete. |
| [Draft RFC 0006](0006-numbers-strings-and-primitive-operations.md) | Owned strings, canonical show/interpolation and typed primitive failures. | Its confirmed policies remain distinct from the broader draft conversion matrix. Complete formatting/ownership integration remains incomplete. |
| [Draft RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md) | Moves, source routes, callable captures, initialized cleanup and deferred cancellation. | Five policies are confirmed. Complete dependency/cleanup judgments remain incomplete. |
| [Draft RFC 0009](0009-user-implementable-conversions.md) | Endpoint keys, selection, coherence and preserved selected E. | Proposed, not accepted. Complete pair/applicability/determination judgments remain incomplete. This RFC adds no automatic error adapter or universal conversion error. |
| Runtime, concurrency and trusted admission | Actual sealed storage, source maps, callback dispatch, child/native completion and bounded-stack cleanup. | No complete RFC assigned. An internal typed-node sketch is not validated storage or target support. |
| [Draft RFC 0010](0010-diagnostics-and-reporting.md), with full LSP still missing. | Dependency explanations, source snapshots, code meanings and inert bounded observations. | Reporting is proposed, not accepted. Complete source/driver and observation procedures remain incomplete. [#42](https://github.com/Reilley64/rill/issues/42) retains complete editor delivery. |

## Valid/invalid examples and diagnostics

R1 through R3 appear in the guide. Every following case is independent or names its required context. All outcomes are specification expectations, not compiler observations.

Case R4 is a contextual-construction excerpt under the confirmed constructor policy:

```rill
let missing = Err(ReadFailed { code: 13 });
let explicit = Err<u32>(ReadFailed { code: 13 });
let numeric = Err<u32, i32>(1);
let untyped = Err<u32>(1);
```

Assume R1's imports/type/display. Missing lacks a success type, R0301. Explicit and numeric establish both needed types. Untyped lacks a numeric width, R0003. No minimum width or default success type applies.

Case R5 describes string/default-Error identity. In a `Result<u32,string>` context, Err("failed") constructs a string error. In a `Result<u32>` context, it is a nominal mismatch, R0100. Import error and use Err(error("failed")) for nominal Error.

Case R6 describes fixed wrappers and variants. A constructor with established parent error context may accept a permitted variant payload. An existing `Err<Variant>` cannot propagate as `Err<Parent>`, R0100. No generic covariance repairs that failure.

Case R7 describes aliases and construction adapters. A renamed canonical Err call records that written call under the confirmed origin policy. Taking Err as an ordinary callback rejects; an explicit arrow can construct it. The arrow's Err body is its construction site, not each adapter invocation.

Case R8 describes origins through helpers. R3's Err belongs to createFailure, not forwardFailure or its caller. A second explicit Err(outer,inner) captures its own construction site. Infix wrapping captures its operator. Inlining must preserve these distinctions without promising formatted-coordinate equality.

Case R9 describes different success types. Assume read():`Result<u32,ReadFailed>` and an enclosing `Result<string,ReadFailed>`. `let number = read()?;` can yield u32 before later Ok string construction. A failure forwards the same `Err<ReadFailed>` without requiring equal success types.

Case R10 describes invalid enclosing contexts. Result propagation from a void function, Option propagation into Result and infix wrapping on Option reject. A helper/nested callable supplies only its own declared return context. It cannot return from an outer expectation.

Case R11 describes borrowed failures. Assume observed:&`Result<u32,ReadFailed>`. A query can yield &`Err<ReadFailed>`. Neither that reference nor a read-only whole-result borrow supplies an owned failure for propagation/cause retention. Do not implicitly clone it; report the type/access or ownership failure.

Case R12 describes lazy wrapping effects. Assume the RHS calls a factory, moves an owner or panics. A successful left Result skips the entire RHS. Failure evaluates it once and returns immediately. Later statements do not execute on that failure path.

Case R13 describes primary borrowed errors. R2 is intended valid while its caller-owned text remains valid. Wrapping that failure as an erased cause rejects missing noBorrow evidence, R0302. A borrow-free outer Error does not change that requirement.

Case R14 describes affine causes. Assume C owns a genuine non-sendable guard or another affine resource. Direct typed handling follows its existing access restrictions. Retention requires all four C predicates and rejects missing evidence, R0302. A user predicate overload or annotation cannot grant those intrinsic capabilities.

Case R15 is a generic-construction excerpt:

```rill
function fail<T,E>(value: E): Result<T,E> where display(E) {
  return Err(value);
}
```

It needs canonical imported Result/Err/display bindings. Unchanged primary construction does not add noBorrow(E). A two-argument generic cause helper additionally needs display(C), noBorrow(C), sendable(C) and shareable(C). Missing evidence rejects before monomorphization, R0302.

Case R16 describes retained evidence. A multi-type cause chain keeps each value, origin, next link and justified formatter/release metadata. An erased wrapper cannot discard an inner restriction or recover dispatch from a type-name string. No public downcast or reflection operation follows.

Case R17 describes combinator forwarding. Map and andThen preserve the same failure; their callback bodies do not run for Err. Callback argument expressions still evaluate, and unused owned environments still clean up. This differs from the skipped infix RHS.

Case R18 describes mapErr. A callback changes E into D while the wrapper keeps its existing location and cause chain. No new origin or cause node appears. Cancellation during the callback cleans pending metadata and callback-owned state once. Payload/capture references cannot escape destroyed callback storage, R0202.

Case R19 describes owned extraction. The intoValue operation transfers E and intentionally disposes the wrapper's old cause/origin state. It is not a preservation operation. A later fresh Err of that payload records a fresh origin and has no automatic old cause.

Case R20 describes error inspection and copies. Value/origin/cause/err accessors borrow without cloning. Copyable E or T does not supply standard wrapper copying, R0302 when required copy evidence is missing. A later use after consuming the wrapper rejects, R0200.

Case R21 describes renderer ordering. Assume outer and inner display implementations record observable events and can suspend. The confirmed standard sequence invokes outer once, completes its node work/temporary cleanup, then invokes inner once. No fixed rendered string, purity or bounded completion time is expected.

Case R22 describes long chains and failure. Standard traversal/rendering/default release uses bounded own call-stack depth while retaining every explicit cause. Worklist allocation may fail fatally; user callbacks may recurse or panic. Neither event permits silent truncation or a successful partial result.

Case R23 describes diagnostics. Inspecting an Err during a failed expectation adds no display/show/copy/iterator callback or input lifetime extension. Opaque/truncated diagnostic previews remain separate from formatError. Assigned compiler codes are not application-error tags.

Case R24 describes fatal and cancellation boundaries. Panic in a mapErr callback or evaluated wrapping RHS remains fatal. Intrinsic cancellation runs required cleanup rather than constructing an Error automatically. Native resource failure can return a typed error, but failure while constructing it can still be fatal.

| Case / artifact and revision | Context | Expected outcome | Owning rule | Evidence label / observed result |
| --- | --- | --- | --- | --- |
| R1/R2/R3, this draft | Complete guide modules. | Lazy wrapping, primary borrowed errors and confirmed helper attribution. | [Wrappers](#wrappers-payloads-and-access), [wrapping](#infix-caused-by-wrapping), [origins](#proposed-construction-origin-attribution). | NotChecked. No Rill execution. |
| R4/R5/R6/R7/R8, this draft | Explicit constructor, namespace, variant and origin contexts. | Apply confirmed argument/context and source-anchor policies without implicit conversion. | [Construction](#proposed-closed-err-construction-and-typing), [origins](#proposed-construction-origin-attribution). | NotChecked. The policies are confirmed; no parser/type checker exists. |
| R9/R10/R11/R12, this draft | Propagation, borrowed access and lazy early-return scenarios. | Preserve ownership, exact E and approved control targets. | [Propagation](#postfix-propagation-and-normal-return), [wrapping](#infix-caused-by-wrapping). | NotChecked. No control/cleanup execution. |
| R13/R14/R15/R16, this draft | Borrowed/affine/generic/erased error contexts. | Preserve direct dependencies; reject missing retained-cause evidence. | [Sealed evidence](#retained-causes-and-sealed-evidence). | NotChecked. No symbolic checker or sealed-runtime proof. |
| R17/R18/R19/R20, this draft | Consuming callbacks, extraction, queries and copying. | Preserve or deliberately discard metadata as specified; keep actual ownership state. | [Combinators](#consuming-combinators-and-borrowed-queries), [access](#wrappers-payloads-and-access). | NotChecked. No callback/cancellation/cleanup execution. |
| R21/R22/R23/R24, this draft | Formatter effects, deep chains, diagnostics and fatal/control failures. | Apply confirmed node order without truncation, added observations or panic catching. | [Rendering](#proposed-standard-rendering-sequence), [boundaries](#panic-cancellation-and-reporting-boundaries). | NotChecked. No renderer, stack profile, trusted observer or provider validation. |

The [diagnostic registry](../diagnostic-codes.md) fixes the cited meanings. Exact syntax, constructor-value and invalid-propagation diagnostic integration remains authoring work. A missing checker or incomplete analysis must not report semantic acceptance.

## Feature interactions

### Shared rule ownership

RFC 0003 owns question-mark parsing and expression/control sequencing. This RFC supplies error-specific construction, propagation typing and retained-cause contracts. It does not reopen the confirmed syntax-based parsing decision.

RFC 0005 owns nominal identity, variant relationships and ordinary field checking. A contextual constructor can check its new payload against established E. It cannot reinterpret an already sealed Err or convert a named Error from string.

RFCs 0004/0007 own symbolic requirements, captures, source routes and cleanup states. This RFC identifies the actual error/cause obligations they must preserve. A selected body or opaque layout alone is not that proof.

RFC 0006 owns canonical show/interpolation sequencing and numeric/text conversion behavior. Its selected ParseError, Utf8Error and ConversionError remain distinct typed errors. The broader matrix/ScalarError details remain draft proposals, not acceptance inferred here.

### Libraries, concurrency and observation

Structured library errors can contain multiple original Err values as typed fields. A standard cause chain does not replace those APIs or invent multi-cause branching. Their display implementations retain their own contracts; there is no reflected traversal of hidden payloads.

Scoped work must keep valid caller-dependent errors until required child cleanup completes. Background transfer still needs the whole job/result's actual evidence. This RFC adds no global join, forced cancellation or rollback.

The accepted [RFC 0001 snapshot](https://github.com/Reilley64/rill/issues/28#issuecomment-6907867211) owns expectation behavior and trusted observation. Its diagnostic budgets do not bound full runtime cause text. Checking must not invoke runtime rendering through a user display callback.

### Source reconciliation

| Input | Disposition |
| --- | --- |
| [#29 decision](https://github.com/Reilley64/rill/issues/29), [ADR 0013](../adr/0013-caused-by-errors-with-explicit-wrapping.md) | Preserve typed wrappers, construction origins, lazy value wrapping and exact forwarding. Later #23 narrows erasure to independently safe retained causes. |
| [#23 final decision](https://github.com/Reilley64/rill/issues/23#issuecomment-6872129850), [catalogue section 4](../standard-library-contracts.md#4-option-result-origins-and-causes), [ADR 0015](../adr/0015-language-required-library-contracts.md) | Incorporate opaque wrappers/locations, explicit Error construction, closed Err, callback/query contracts and mapErr preservation. The later RFC policy decision confirms type-argument order and origin/format sequencing, not the full error system. |
| [#9 core decision](https://github.com/Reilley64/rill/issues/9#issuecomment-6308790321), #11/#12, Draft RFC 0003 | Preserve Result/Option control and explicit unused-Result handling. Later decisions replace recoverable propagation reports and old constructor conveniences. Complete parsing remains with RFC 0003. |
| #15/#16, [Draft RFC 0007 policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6915849379) | Preserve source dependencies, initialized disposal, remaining-field cleanup and deferred cancellation. Full route and cleanup judgments remain incomplete. |
| #19/#21/#41 and Draft RFCs 0004/0005 | Use enums, named records, canonical family evidence and implement bodies. No literal types, anonymous records, general union representation or return-only dispatch returns. |
| #22 and [stack decision](../adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) | Bound standard chain traversal/default release without truncating explicit causes. User callbacks remain ordinary code; target/ABI support still needs validation. |
| [#24 final decision](https://github.com/Reilley64/rill/issues/24#issuecomment-6885970775), [diagnostic contracts](../diagnostic-contracts.md) | Keep opaque runtime locations distinct from structured compiler ranges. Trusted previews call no user formatters and do not alter sources or causes. |
| [Panic decision](../adr/0005-panics-end-the-program-without-unwinding.md), later runtime/network refinements | Preserve fatal no-unwind behavior, limited emergency reporting and typed native operation failures. Do not promise recoverable allocator failure or termination of an embedding host. |
| [Historical error program](../../prototypes/02-caused-by-errors.rill), [case inventory](../prototypes/02-caused-by-errors.md), source revision | Preserve their lazy/cause/forwarding obligations and historical bytes. Their parseNumber spelling needs the recorded tryInto migration; no Rill execution or fixed output-format promise follows. |

## Compatibility and migration

Historical Report/propagation-trace descriptions are superseded. Use unchanged forwarding when no new context is intended. Use explicit wrapping for a new outer error and construction origin.

A default Error still needs error(message). Err("text") does not gain an implicit nominal conversion. Existing typed error values need canonical display implementations, not required message fields or reflected dumps.

Historical cause retention that ignores borrow/transfer/sharing evidence is invalid under the later source decision. Primary borrowed typed errors remain possible. A new outer owned value cannot hide unsafe inner state.

Historical parser helpers migrate through RFC 0006's confirmed tryInto entry points. This draft does not rewrite prototype programs or recorded results. New examples use current interfaces and contextual numeric fields without claiming those programs ran.

The three precision policies are confirmed. There is no released error ABI or implemented rendering protocol to preserve. A canonical identity, static function pointer or private field alone does not validate sealed storage.

## Security/resource implications

Payload text, origins and cause chains can disclose sensitive data. Formatting supplies no automatic redaction, upload or safe terminal escaping guarantee. Application sinks must apply their own output policies; compiler reporting has separate rules.

Retaining all explicit causes can consume unbounded total memory and output. Bounded standard stack use is not a memory quota or bounded completion guarantee. Formatters/disposers can suspend, recurse, block or panic.

Cause erasure must not hide borrowed/affine state or duplicate unique ownership. Trusted construction, dispatch and cleanup metadata need actual validation on each supported target. Native/Wasm pointer and saved-state guarantees do not follow from these signatures.

Fatal allocation failure can prevent error construction or reporting. No complete panic text, final summary, rollback or cleanup is guaranteed after fatal termination. Cooperative cancellation has its own required release path.

## Rationale, alternatives and prior art

Automatic propagation traces would add metadata merely for passing an error onward. The confirmed design records explicit context instead. Fatal panic traces remain a separate runtime concern.

Requiring message/cause fields would constrain every user error's representation. Canonical borrowed display permits ordinary typed payloads without reflection. Erased Cause metadata preserves per-node behavior without a public dyn interface.

The source decision compares Roc's operator shape but selects a lazy error value instead of a handler. A handler can inspect the inner error through explicit guard code. Infix wrapping itself remains simple wrap-and-return behavior.

The confirmed origin policy separates real user factories from compiler constructor helpers. The confirmed type-argument policy resolves success/error context without default widths or extra ordinary overload rules. Ordered rendering makes effectful callback visits predictable without fixing human text.

None of these choices proves the sealed representation or dependency system sound. Required conflicts return to design review rather than weakening retained-source guarantees.

## Implementation implications

Resolve canonical constructor identity and namespace before lowering. Preserve explicit type arguments and payload context without body-selection retries. Attach original source anchors independently of inlining and hidden constructor machinery.

Lower propagation as an owned failure transfer, not a new Err construction. Lower wrapping with a pending owned inner wrapper and exactly one lazy RHS. Every normal cancellation edge needs valid ownership/drop state.

The mapErr implementation needs separate pending payload/metadata states while invoking its callback. Extraction and partial construction must not publish invalid public wrappers. Remaining-field cleanup must not repeat transferred causes or callback-owned data.

Sealed retention needs type-correct storage, layout, formatter and release metadata. Preserve proven property/dependency evidence before monomorphization and through every erased boundary. No public API or unsafe type-name registry can substitute for this proof.

Standard traversal/release must not recurse with chain depth. Callback execution still uses normal calling and suspension rules. The Rust scaffold implements none of this error system.

## Evidence and checking plan

### Actual evidence

| Artifact / revision | Evidence label | Rule/question exercised | Checker/tool and command/environment | Expected / observed outcome | Limits, negative/unavailable results and gate links |
| --- | --- | --- | --- | --- | --- |
| #29/#23/#24 decisions and repository base `f33dcf4cf3f815824eeb897135775d7884f65c1a` | HandChecked | Source ownership, origins, retained-cause requirements and diagnostic separation. | Drafting agent, 2026-11-16. Read current decisions, ADRs and catalogue section 4. | Reconciled the later retained-cause boundary and preserved exact forwarding/lazy wrapping. | Source review is not full formal checking or implementation acceptance. |
| R1 through R24, this draft | NotChecked | Constructor typing, propagation, causes, callbacks and rendering. | No Rill parser, checker or runtime is available. | Expected outcomes appear above. No program, source-location trace or formatting sequence executed. | Policy approval does not complete formal rules or companions. No case is RillExecuted. |
| Sealed node sketch and state transitions | NotChecked | Heterogeneous storage, evidence, cancellation and bounded-stack release. | No reference model, formal proof or runtime/provider integration ran. | No representation or soundness result is claimed. | Complete judgments and actual native/Wasm validation remain required. |
| Historical prototypes and authored case inventory | NotChecked | Execution of the combined current error contract. | Existing artifacts retain their original evidence labels and bytes. | No historical report is relabeled as a current executed Rill test. | Old helper spellings and expected text are provenance, not new API or rendering-format authority. |
| RFC documents and register, local working draft | ToolChecked | Structure, writing limits, local links and preservation of earlier rules/artifacts. | Drafting agent, 2026-11-16. Temporary Python checks; `mise run check:docs`. | Eight IDs/register rows and required metadata/headings passed. Sentence/paragraph, fence, link and whitespace checks passed. | Document checks do not parse/execute Rill, prove sealed storage or audit the formal ASD-STE100 dictionary. |

### Planned checks. Not executed evidence

- Check closed constructor arity, explicit type-argument positions and contextual payload typing.
- Reject missing success/numeric types and implicit string-to-Error conversion.
- Preserve alias/factory/arrow source anchors through inlining and monomorphization.
- Forward Result failures with exact E and independent success types.
- Check Option propagation without adding origins or implicit Result conversion.
- Reject borrowed-wrapper cloning and missing generic display/cause evidence.
- Count both wrapping operands on success, failure, suspension and early cancellation.
- Preserve every typed cause, origin and formatter/release witness through erasure.
- Test map/andThen forwarding separately from mapErr and intoValue behavior.
- Test pending metadata cleanup and invalid callback/payload-source escapes.
- Count node formatter effects and normal cleanup in the confirmed traversal order.
- Exercise long chains without standard recursive traversal or silent truncation.
- Separate trusted diagnostic observations from full user-code runtime rendering.
- Distinguish typed native failures, fatal allocation/panic and cancellation control paths.
- Validate target-specific source maps, storage, dispatch and child/native release integration.

These checks need the relevant checker/runtime and target profiles. Document structure or a source inspection cannot clear them.

## Open/deferred questions

<a id="review-choices"></a>

### Confirmed policies

The project owner [approved all three policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6917170418) on 2026-11-16. The pre-recording snapshot has SHA-256 `73cae5228169bd9f91b846aff420ea7360f66cce21e64a3cc5bdcb2eb11eab59`.

1. Record canonical Err at its written call's source start and infix wrapping at its question-mark token. Real factories/adapters record their own Err expressions. Forwarding preserves the original origin. Aliases, inlining and compiler-generated helpers do not replace those source sites.
2. Use success T first, error E second and cause C third for explicit closed Err arguments. Only the two-value-argument form admits C. Immediate Result context, typed payloads and explicit arguments establish types under the stated rules. Established E can supply numeric context without a default width or implicit string-to-Error conversion.
3. Render standard chains outer-to-inner with one payload-formatter call per visited node. Complete that node's origin work before the next formatter. Complete its normal temporary cleanup before the next formatter. Human text remains unspecified as a parsing format.

These approvals do not accept future unwritten rules or a completed error system. Native/generated attribution, public Some/None bindings and full evidence/dependency/cleanup judgments remain incomplete. Full RFC acceptance and readiness remain open.

### Remaining in-scope authoring work

Complete constructor/expression grammar, contextual inference and variant/guard judgments with the shared owners. Keep exact error identity separate from ordinary initial payload admission.

Complete public Some/None binding forms with the namespace/type owners. Qualified Option variants already follow RFC 0005.

Complete sealed property derivation, witness transport and nested dependency routes before and after erasure. Define safe partial-construction/mapErr/retention cleanup at every normal cancellation edge.

Specify native/generated origin admission and source-map retention without public coordinate fields or invalid source-buffer borrows. Complete bounded-stack storage/release and trusted metadata integration with the runtime companions.

These gaps prevent the [canonical readiness review](README.md#canonical-reviewready-checklist). If required semantics cannot coexist, return the conflict to [map #1](https://github.com/Reilley64/rill/issues/1). A supporting signature or private representation does not resolve it automatically.

### Implementation/validation gates and future scope

Implement the parser/type/evidence rules, wrapper storage, source maps, combinators and renderer. Validate actual cleanup and target behavior rather than counting document examples as passes.

This RFC adds no exceptions/catch, automatic propagation reports, public reflection/downcasts, universal error conversion or recoverable Rill allocator. It adds no new driver status or compiler/LSP report schema.

## Decision and change history

### Readiness record

No canonical readiness review has occurred. This document remains Draft. The drafting agent checked sentence/paragraph limits and applied unslop as the final prose pass. Structure and link checks passed.

These checks cannot supply missing judgments or validate sealed ownership. Formal ASD-STE100 dictionary compliance remains unverified.

### Human decisions and amendments

The project owner requested this errors RFC on 2026-11-16. That request authorized drafting, not precision policies or full RFC acceptance. Source decisions and companion policy approvals retain their own scopes.

The project owner later [approved all three precision policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6917170418). This approval identifies the pre-recording snapshot below. It does not accept the full RFC, supply missing judgments or establish readiness. Full Decision and Reviewed revision remain Not yet decided and Not yet reviewed.

| Date / actor | Kind and scope of change | Reviewed revision or snapshot | Decision link / rationale |
| --- | --- | --- | --- |
| 2026-11-16, drafting agent | Created substantive Draft RFC 0008, register and partial source coverage. | Source base `f33dcf4cf3f815824eeb897135775d7884f65c1a`, current decisions cited above and linked Draft companions. This is not a reviewed RFC snapshot. | User request to draft errors, propagation and caused-by wrapping. No new policy, full RFC or implementation acceptance is inferred. |
| 2026-11-16, project owner | Approved construction origins, closed Err generic/context typing and standard formatter sequencing. | Pre-recording draft SHA-256 `73cae5228169bd9f91b846aff420ea7360f66cce21e64a3cc5bdcb2eb11eab59`. Local snapshot `/tmp/rill-rfc-0008-before-policy-approval.md`. | [Three-policy decision](https://github.com/Reilley64/rill/issues/28#issuecomment-6917170418). Full acceptance, formal judgments, readiness and implementation/target/client validation remain open. |
