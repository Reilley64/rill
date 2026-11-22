# RFC 0009: User-implementable conversions

| Field | Value |
| --- | --- |
| ID | 0009 |
| Status | Draft |
| Owner | Reilley64. ri-48g tracks this draft. |
| Source inputs | ri-1wl conversion decision (comment 6886119679), [ADR 0017](../adr/0017-user-implementable-conversions.md), [catalogue section 9](../standard-library-contracts.md#9-conversion-numeric-compatibility-parsing-and-formatting) and the later core interface direction (ri-48g, comment 6913810445). [Source reconciliation](#source-reconciliation) identifies related inputs. Repository inputs use commit `f33dcf4cf3f815824eeb897135775d7884f65c1a`. |
| RFC dependencies | Draft RFCs [0002](0002-modules-and-program-loading.md), [0003](0003-expressions-bindings-and-control-flow.md), [0004](0004-functions-overloads-and-generic-requirements.md), [0005](0005-types-records-enums-and-inference.md), [0006](0006-numbers-strings-and-primitive-operations.md), [0007](0007-ownership-borrowing-mutation-and-cleanup.md) and [0008](0008-errors-propagation-and-caused-by-wrapping.md) propose shared rules. Their full texts are not accepted. Required [companions](#required-companion-rules) remain incomplete or missing. |
| Replaces / superseded by | None. This draft does not amend an Accepted RFC or accept another draft. |
| Decision | Not yet decided |
| Reviewed revision | Not yet reviewed |

## Summary and scope

This RFC proposes canonical infallible and fallible conversions that users can implement. It owns conversion pair keys, endpoint permission, selection, target inference and implementation-selected error evidence.

The glossary defines [conversion and numeric conversion](../../GLOSSARY.md#language). Conversion is explicit. Registration does not grant implicit argument coercion, automatic conversion chains or universal serialization.

The source decision already fixes destination-first spelling, canonical aliases, endpoint ownership and protected standard behavior. It also requires symbolic selected-error evidence before monomorphization. This draft preserves those requirements and the later into/tryInto predictability direction.

The project owner [confirmed whole-pair matching, conversion requirement applicability and determined error outputs](#confirmed-policies). Their finite checking procedures and dependency transport still need formal completion. This document remains Draft, not a proof or complete compiler implementation.

RFC 0004 owns language-wide implement syntax and ordinary overload rules. RFC 0006 owns numeric/text values and the core pair catalogue. RFCs 0007/0008 own shared ownership/error rules. This RFC states their conversion-specific use without accepting those drafts.

## Motivation

One source type can have several intentional destinations. Ordinary parameter-only keys cannot distinguish those conversions without changing unrelated family selection. A canonical pair key makes that exception explicit.

Fallible conversions can report different typed errors for different pairs or specializations. Generic code must preserve that selected error rather than assume a fallback's concrete type. An expected error must not select a different implementation.

Explicit source access also matters. Consuming a string, copying through a borrow and returning a source-retaining view have different contracts. Conversion syntax must not conceal those differences.

## Guide-level explanation

Case C1 is a complete proposed module:

```rill
import std::convert::{into, from};

type UserId = { text: string };
type Email = { text: string };
implement into<UserId>(value: string): UserId {
  return UserId { text: value };
}
implement into<Email>(value: string): Email {
  return Email { text: value };
}
implement into<string>(value: &UserId): string {
  return copy(&value.text);
}
function sameText(): bool {
  let id = into<UserId>("u1");
  let email: Email = from("u1");
  return into<string>(&id) == email.text;
}
expect sameText();
```

The first two registrations share a source but have distinct destinations. The from spelling is an alias with the same argument order. The last conversion borrows UserId and explicitly copies its text.

Case C2 is a separate complete module:

```rill
import std::convert::{tryInto, tryFrom};
import std::result::{Result, Ok, Err};
import std::error::display;

type UserId = { text: string };
enum InvalidUserId { Empty }
implement display(failure: &InvalidUserId): string {
  return "empty user id";
}
implement tryInto<UserId>(value: &string): Result<UserId, InvalidUserId> {
  if (value == "") { return Err(InvalidUserId.Empty); }
  return Ok(UserId { text: copy(value) });
}
function accepted(): bool {
  let text = "u2";
  guard let id = tryFrom<UserId>(&text) else { return false; }
  return id.text == text;
}
expect accepted();
```

The implementation chooses InvalidUserId. It does not convert that error into a universal error type. A caller expecting another error must adapt it explicitly after selection.

Case C3 is a complete module:

```rill
import std::convert::tryInto;
import std::result::{Result, Ok, Err};
import std::error::display;

type Token = { number: u32 };
type Box<T> = { value: T };
type Wrapped<T> = { value: T };
enum GeneralError { Rejected }
enum TokenError { Zero }
implement display(failure: &GeneralError): string { return "rejected"; }
implement display(failure: &TokenError): string { return "zero token"; }
implement<T> tryInto<Wrapped<T>>(value: Box<T>): Result<Wrapped<T>, GeneralError> {
  return Ok(Wrapped<T> { value: value.value });
}
implement tryInto<Wrapped<Token>>(value: Box<Token>): Result<Wrapped<Token>, TokenError> {
  if (value.value.number == 0) { return Err(TokenError.Zero); }
  return Ok(Wrapped<Token> { value: value.value });
}
function attempt<To, From, E>(value: From): Result<To,E>
  where tryInto(To,From,E), display(E) {
  return tryInto<To>(value);
}
function selected(): bool {
  let boxed = Box<Token> { value: Token { number: 7 } };
  let result: Result<Wrapped<Token>,TokenError> = attempt<Wrapped<Token>>(boxed);
  guard let converted = result else { return false; }
  return converted.value.number == 7;
}
expect selected();
```

The concrete pair wins over the generic default and selects TokenError. Attempt preserves E symbolically. It does not promise GeneralError merely because the generic default exists.

Token's field supplies u32 context for 7. The other numeric literals have established operand context. No numeric default or mandatory redundant suffix is introduced.

These are specification examples. No Rill compiler or runtime has checked or executed them.

## Syntax and reference semantics

### Canonical declarations, aliases and implementations

The canonical source declarations live in std::convert:

```rill
export function into<To, From>(value: From): To;
export function tryInto<To, From, E>(value: From): Result<To,E>;
```

The from spelling aliases into; tryFrom aliases tryInto. Both aliases preserve destination-first type arguments and the same implementation set. Historical try_into/try_from spellings are superseded, not extra public families.

These names require ordinary imports. Signature metadata can load Result without injecting its source binding. A same-spelled unrelated family has no conversion privileges or canonical evidence.

Use implement for family bodies under RFC 0004. Implementation-owned binders precede the family name. Conversion registration writes its destination explicitly. Infer omitted source/error family arguments from the signature, never the body.

```rill
implement into<UserId>(value: string): UserId { /* body */ }
implement<T> into<Wrapped<T>>(value: Box<T>): Wrapped<T> { /* body */ }
implement tryInto<UserId>(value: &string): Result<UserId,InvalidUserId> { /* body */ }
```

These are header sketches using C1/C2/C3's types, not additional complete declarations. Written family arguments must agree with parameter and result types. An infallible body returns To; a fallible body returns `Result<To,E>`.

Visibility follows the canonical family, not individual implementation exports. Private fields remain private even when endpoint ownership permits registration. No trait, associated-type projection syntax or general return-only overload feature is added.

### Pair collection and canonical identity

Only canonical into and tryInto use destination/source implementation keys. The complete collection identity also includes the canonical family. Infallible and fallible entries therefore remain separate.

Normalize endpoint type identities, reference structure and implementation-binder names across the whole pair. Preserve repeated-variable relationships. E, returns beyond the declared To shape and requirements do not distinguish pair keys.

```text
collection-key = (canonical-family, alpha-normalized(To-pattern, From-pattern))
```

Distinct bodies with identical keys are collection-time duplicates, R0310. Check them even without a call or when their requirements differ. Import aliases and renamed binders do not rescue duplicates.

Repeated graph paths to the same body contribute it once. Different error types are not different fallible registrations for one identical pair pattern. Collection does not execute bodies or requirements as user code.

Use RFC 0002's loaded graph, not every installed/indexed module. Ordinary explicit imports and declared metadata dependencies establish available bodies. Canonical family/type aliases do not create new owners or registrations.

### Endpoint placement permission

A registration is permitted in the canonical family's defining module or either endpoint's defining module. For endpoint permission, peel reference layers and inspect the outer nominal type. Preserve those layers for matching and source access.

A local `Box<T>` wrapper grants permission through that endpoint. `ForeignBox<Local>` does not: its outer nominal owner is foreign. A local error, constraint, bare generic parameter or import grants no endpoint ownership.

A variant retains its parent declaration's defining module. A tuple or standard collection containing a local payload does not become that payload's outer nominal type. Structural similarity does not grant permission.

Unauthorized endpoint placement rejects, R0312. This rule deliberately differs from ordinary families' nested-type permission. It does not impose an outermost-type rule on copy, dispose, show or other ordinary families.

Placement is not a coherence or safety proof. Duplicate, protected-overlap, requirement and ownership checks still apply. Defining an application file named std/convert does not acquire the genuine std origin's identity.

### Calls and independently established endpoints

At a call, establish To explicitly or from immediate unique concrete context. An expected Result can supply its success type for tryInto. Its error component cannot select a body.

Infer From from an independently established source expression or an explicit source argument. Literal fields, annotations and declared inputs can provide ordinary source context. The destination or available conversion count cannot guess a missing source width.

`into<u8>(10)` lacks a source width. `into<u8>(10u16)` has a source type but lacks an infallible numeric pair. `tryInto<u8>(10u16)` uses the standard fallible pair, even when this particular value fits.

Explicit From can supply source context, as in `into<Bytes,List<u8>>([])` under RFC 0006's draft pair catalogue. That differs from guessing List element types from Bytes alone. Explicit E is an equality demand checked after normal fallible selection.

Do not infer To from later statements, default it or search for a sole registered target. Do not use requirement truth or desired E to complete an otherwise unknown endpoint. No automatic conversion chain supplies an absent pair.

Type-check and select without evaluating user code. At runtime, evaluate the source expression once under RFC 0003's sequencing rules. Compilation failure does not perform a speculative conversion or move.

<a id="proposed-whole-pair-matching-and-specificity"></a>

### Whole-pair matching and specificity

Use one substitution shared across both endpoint patterns. Its destination must equal the requested canonical To. Its source parameter must admit the established From under the type companion's permitted input relationships.

```text
matches(I, D, S) iff some substitution theta satisfies:
  theta(I.To) equals D
  S is admissible as theta(I.From)
```

Establish target substitutions first. Bind remaining source variables to the actual source structure before considering permitted fixed-pattern admission. Do not search for wider types to satisfy requirements or choose E.

This distinguishes destination equality from ordinary source argument admission. Owned variant-to-parent and read-only variant-borrow admission remain available under RFC 0005. No numeric width coercion, container covariance or new reference adapter follows.

A registered destination Parent.Variant does not become a Parent-target registration. A body registered for Parent may still return one of Parent's variants under ordinary return typing. That does not change its registered destination.

A whole-enum source conversion can receive a statically known variant through permitted parent admission. This preserves the source program's InputState-to-State example. The caller's source type remains established independently, not guessed from candidate counts.

Compare complete pair matching sets, preserving all shared-variable relationships. A is more specific than B when A's set is a proper subset of B's set. Exclude requirements, E and import order from this comparison.

The correlation in `(Wrapped<T>,Box<T>)` is narrower than independent `(Wrapped<A>,Box<B>)` parameters. Crossed source/destination restrictions can remain incomparable. A narrower destination does not compensate for a wider source.

Protected identity has its separate exact-equality gate below. Do not let source variant admission turn it into implicit parent promotion. The finite subsumption/intersection procedure still needs formal completion with RFCs 0004/0005.

<a id="proposed-requirement-applicability-and-selection"></a>

### Requirement applicability and selection

The conversion inventory describes specificity among requirement-applicable candidates. The project owner confirmed that conversion-only distinction. Ordinary family selection retains RFC 0004's winning-body requirement check and no fallback.

First perform global collection, placement, shape and protected checks. An invalid generic body does not become valid because a call could ignore it. Check every implementation body against its declared assumptions before monomorphization.

For each matching pair, instantiate its requirements from established endpoints and determined output relations. Keep these evidence states distinct:

| State | Selection meaning |
| --- | --- |
| Proved | The candidate is requirement-applicable. |
| Disproved | The candidate is excluded by an established failed obligation. |
| Unresolved | Proof is incomplete or depends on unknown facts; it is not a negative result. |

Among proved candidates, select the unique undominated most-specific pattern. Incomparable undominated applicable patterns reject as ambiguity, R0303. No proved applicable candidate means no successful conversion selection.

An unresolved candidate that could win or create a tie prevents a completed selection. Do not choose a fallback by treating it as disproved. A strictly dominated unresolved candidate need not defeat an independently justified winner.

A separately justified symbolic relation is not a fabricated concrete winner. Do not report a fallback winner while relevant evidence remains unresolved.

For fully established obligations, a disproved specific candidate can leave a general candidate applicable. That is requirement applicability, not ranking by constraint strength. It is not permission to use expected E to exclude the specific candidate.

Generic assumptions can supply explicit justified evidence. An active recursive goal cannot prove itself merely by appearing again. Type-growing searches or exhausted resources must report incomplete checking, not fabricated negative evidence.

Exact cycle/nonmonotonic applicability procedures remain in-scope authoring work. This policy does not impose a new recursion rule on runtime bodies or intrinsic structural property derivation. An incomplete proof cannot clear readiness.

<a id="proposed-determination-of-fallible-outputs"></a>

### Determination of fallible outputs

E is output evidence of the normal selected conversion. It is not an extra pair key, ranking preference or input to candidate error inference. The winning signature must determine it independently of a caller's requested E.

Check that output-influencing type parameters have a determination route at registration. The route starts with endpoint substitutions or fixed declared types. It can also use selected outputs of stated canonical conversion relationships with already established endpoints.

A compound error type `ErrorFor<T>` is determined when its relevant T is determined. A requirement `tryInto(A,B,E)` can determine E from its normal selected output once A/B are established. A requirement display(E) alone cannot choose E.

Do not infer an unknown target A from available implementations to determine E indirectly. Root expected-error context cannot seed a circular output route. Unrelated operation requirements do not become hidden output projections.

A body that leaves its selected error underdetermined rejects at registration. This applies even when the body only returns Ok or never returns. Runtime branch knowledge cannot define an otherwise ambiguous signature.

An output route can remain symbolic for generic endpoints. It need not normalize an overridable nested fallback's error during registration. Instantiated obligations must later establish the actual selected output and its required equalities.

Reject an output schema such as arbitrary E with only display(E) evidence. Preserve schemas that relate E to a nested canonical tryInto output. Neither requires a public associated type or new projection syntax.

Complete determination, equality and mutually dependent evidence judgments still need formal rules. A functional-output claim alone is not a terminating solver or soundness proof.

### Selected-error evidence in generic code

The canonical requirements are `into(To,From)` and `tryInto(To,From,E)`. Both endpoints must be named. A requirement has no value argument from which to infer its missing source.

Check generic bodies before monomorphization with justified signature/access/property/dependency evidence. An unconstrained endpoint relation needs an actual requirement or independently established proof. One visible fallback is not proof of every later specialized call.

`tryInto(To,From,E)` promises that normal selection for those endpoints produces exactly E. Concrete callers first establish endpoints and normal selection. Then infer an unresolved caller E or check an already fixed E for equality.

A mismatch with the winning E rejects, R0313. Do not repeat lookup with another candidate or alter requirements to satisfy the expected error. Alias identity does not change that equality.

A generic body cannot assume GeneralError from an overridable fallback and later execute a TokenError specialization. Preserve E symbolically or state an exact relation that incompatible callers cannot satisfy. Monomorphization instantiates the checked meaning rather than repairing missing evidence.

Additional display, copy, noBorrow, sendable or shareable requirements stay explicit when used. Returning an existing failure does not itself create a new Err. The family's declaration does not gain a universal display(E) requirement through this RFC.

Normal exact-E postfix propagation, explicit mapErr and caused-by wrapping follow RFC 0008. Retaining the selected error as a cause still requires its own four predicates. A conversion cannot manufacture them through its outer result type.

### Protected numeric domain and infallible identity

Both canonical families preserve the standard numeric domain from RFC 0006. Non-trusted registrations cannot replace protected numeric behavior or fill a missing infallible numeric pair with arbitrary semantics. Check protected overlap during collection, R0311, without relying on calls or value ranges.

Numeric protection concerns actual numeric endpoint pairs. A user-owned nominal wrapper around a number remains a distinct endpoint. Its explicit conversion must still obey ordinary safety and its declared contract.

Standard infallible identity applies when requested To and established From are the same canonical type. It takes priority over generic wrappers matching that equal pair. Exclude the standard identity candidate unless this exact gate holds. It does not supply Variant-to-Parent identity.

Reject explicit user infallible identity registrations, R0311. A generic `Box<T>`-to-`Box<U>` conversion can remain registered for nonidentity pairs. At equal endpoints, standard identity wins without evaluating the generic body's requirements.

Identity returns the input under normal move/implicit-copy/reference rules. It does not clone owned storage or duplicate exclusive authority. Ordinary call reborrowing retains all parent/source restrictions; moving a descriptor remains a move.

There is no universal fallible identity or automatic infallible-to-fallible adapter. Non-numeric tryInto identity needs its own permitted registration and determined E. Standard numeric safe/identity fallible entries retain Result and ConversionError.

An explicit into registration supplies no tryInto relation automatically, or vice versa. No uninhabited universal conversion error is introduced. These families do not change ordinary arithmetic, rounding or literal validity.

### Function values, ownership and effects

A conversion becomes a function value only with established endpoints and exact selected signature/error. Explicit family arguments or immediate callable context can provide that information. An unresolved family is not a runtime converter.

Use the same normal selection and E check as a direct call. Preserve validated source admission and callable/dependency evidence. This does not introduce general function-type variance, dynamic lookup or an erased converter registry.

Owned T, read-only &T and exclusive &mut T remain distinct source contracts. No owned-to-borrowed registration adapter or borrowed-to-owned copy appears automatically. Use explicit acquisition/copy and only the reference relationships already permitted by the shared owners.

A result or error can carry references from borrowed inputs or inside owned inputs. Preserve those external sources and exclusive restrictions. Reject references into destroyed local source storage, including a consumed input's own storage.

A reference-valued result is not proof that it points to an owner header. Logical string/byte extents and descriptor slots follow RFCs 0006/0007. No conversion extends an owning scope or forges ownership from a logical view.

Implementations are ordinary functions with permitted effects and suspension. Exclusive inputs can mutate their valid sources. Infallible means a direct result without recoverable conversion failure, not purity, losslessness, totality or immunity to panic.

Argument evaluation, normal cancellation and cleanup preserve actual initialized ownership. Neither source copying nor output naming proves native pointer stability. Target/runtime validation remains separate.

### Core registrations and application boundaries

RFC 0006 owns numeric values, strict parser/UTF-8 rules and the core conversion matrix. Numeric-to-numeric fallible pairs keep ConversionError. Confirmed strict &string-to-number parsing selects ParseError; owned &Bytes-to-string decoding selects Utf8Error.

The broader borrowed-view, byte/List and scalar matrix remains draft detail under the predictability direction. ScalarError is not separately accepted here. This RFC does not allocate new core pairs or silently strengthen their acceptance status.

Load promised core registrations through declared std::convert metadata dependencies. Do not require error-name imports merely to activate them. Naming an error type still requires its ordinary source binding.

No installed-module scan, wildcard import or runtime registry follows. User implementation modules must be present in the actual loaded graph. Indexing their files does not activate conversions.

Keep policy selection, extra context, queries, traversal and mutation explicit. The decodeLossy operation remains named; show remains the formatting contract. No default lossy byte-to-string pair or automatic show-to-string registration is added.

Explicit JSON/SQL/time/path conversions keep their own semantic contracts. Registration is not automatic serializer/codec enrollment, structural field mapping or SQL-expression capture. Named SQL bindInto/tryBind/get helpers use their separately declared conversion evidence.

No argument coercion or automatic A-to-B-to-C chain is introduced. A source already accepted through an ordinary enum relationship is not a new registered conversion chain. No as conversion/assertion syntax follows.

### Required companion rules

| Rule owner | Required contribution | Current dependency state |
| --- | --- | --- |
| [Draft RFC 0002](0002-modules-and-program-loading.md) | Loaded bodies, canonical aliases, endpoint module identity and metadata dependencies. | Identity/order policies are confirmed. Full resolution and dependency/cycle rules remain incomplete. |
| [Draft RFC 0003](0003-expressions-bindings-and-control-flow.md) | Expression/type-argument boundaries, call sequencing and explicit control transfers. | Four policies are confirmed. Exact grammar and control/dataflow judgments remain incomplete. |
| [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md) | Implement syntax, ordinary family boundaries, symbolic evidence and callable contexts. | Three policies are confirmed. Full pattern/evidence procedures remain incomplete. The confirmed conversion-applicability policy does not change ordinary selection. |
| [Draft RFC 0005](0005-types-records-enums-and-inference.md) | Canonical types, permitted variant/reference relationships, generic contexts and layout. | Three policies are confirmed. Complete type/pattern and representation judgments remain incomplete. |
| [Draft RFC 0006](0006-numbers-strings-and-primitive-operations.md) | Numeric protection/values, source widths, core pairs, parsers and logical views. | Its confirmed policies remain distinct from broader draft matrix details. Full grammar and ownership integration remain incomplete. |
| [Draft RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md) | Value/source routes, reference access, initialization and cancellation-safe cleanup. | Five policies are confirmed. Complete caller/selected-signature and nested route judgments remain incomplete. |
| [Draft RFC 0008](0008-errors-propagation-and-caused-by-wrapping.md) | Exact error identity, typed wrappers, causes, display evidence and origins. | Three policies are confirmed. Full sealed evidence, constructor and runtime judgments remain incomplete. |
| [Draft RFC 0010](0010-diagnostics-and-reporting.md), with library/runtime integration incomplete. | Actual core registration graph, trusted admission, ABI and accurate inference/evidence explanations. | Reporting is proposed, not accepted. ri-48g and ri-bv2 retain production/target and editor validation. |

## Valid/invalid examples and diagnostics

C1 through C3 appear in the guide. Later cases are independent unless they name a context. Every outcome is a specification expectation, not compiler output.

Case C4 is an implementation excerpt for the confirmed output-determination policy. Use C3's Wrapped and canonical imports, and a local `type TextBox = { text: string };`:

```rill
implement<T,E> tryInto<Wrapped<T>>(source: &TextBox): Result<Wrapped<T>,E>
  where tryInto(T,&string,E), display(E) {
  let converted = tryInto<T>(&source.text)?;
  return Ok(Wrapped<T> { value: converted });
}
```

T comes from the registered target. The nested canonical relationship determines E symbolically. Source/error views can retain caller-owned TextBox data. No blanket noBorrow constraint or local-source escape is assumed away.

Case C5 describes missing endpoints and numeric context. `into("u1")` without target context rejects, R0301/R0314 as appropriate to the incomplete conversion request. A single loaded destination cannot repair it. `into<u8>(10)` lacks source width, R0003. An established explicit From can provide the missing ordinary input context.

Case C6 describes expected errors. C2's borrowed string pair selects InvalidUserId. A `Result<UserId,GeneralError>` expectation cannot choose another body, R0313. The same applies to explicit E and a function-value result context.

Case C7 describes duplicate collection. Distinct into/from registrations for the same canonical pair reject, R0310. Renamed binders, changed requirements or different fallible E do not distinguish keys. Repeated imports of one body do not create duplicates.

Case C8 describes endpoint permission. A module owning `LocalBox<T>` can implement through that endpoint, including borrowed layers. Owning Local inside `ForeignBox<Local>`, or owning only the error type, does not suffice, R0312. Ordinary copy/dispose nested-type permission remains unchanged.

Case C9 describes the confirmed matching policy. A whole InputState source entry can accept InputState.Ready through ordinary variant admission. A requested target State.Active is not supplied by registration for State alone. Equal-endpoint identity does not automatically promote InputState.Ready to InputState.

Case C10 describes correlated matching. Compare `(Wrapped<T>,Box<T>)` with `(Wrapped<A>,Box<B>)`. The first preserves the shared element relationship and is narrower under the confirmed matching policy. Requirement truth and expected E do not enter that comparison.

Case C11 describes crossed pairs. Assume local `Duo<A,B>` permits registrations. `(Duo<T,i32>,Duo<i32,U>)` and `(Duo<i32,V>,Duo<W,i32>)` overlap at equal i32 elements without ordering. In the fallible family, absent a dominating applicable pattern, the call is ambiguous, R0303. Equal infallible endpoints instead use protected identity.

Case C12 describes conversion applicability. A proved-unsatisfied declared requirement excludes a matching specific candidate under the confirmed conversion-only rule. A proved general candidate can then remain applicable. This is not ordinary-family fallback and cannot hide an invalid implementation body.

Case C13 describes unknown evidence. A specific candidate's requirement remains unresolved for symbolic types or exhausted checking resources. If it could win or tie, do not report successful fallback selection. Unfounded proof cycles and unfinished searches are not established negative facts.

Case C14 describes underdetermined output. A registration has fixed endpoints and arbitrary E constrained only by display(E). Reject its undetermined output under the confirmed policy, even if its body always returns Ok or panics. Caller-selected E does not rescue that registration.

Case C15 describes determination direction. A nested `tryInto(A,B,E)` can determine E only after A/B are established. An unknown A appearing as that requirement's destination is not inferred from loaded candidates. A fixed error constructor `ErrorFor<T>` can depend on endpoint-determined T.

Case C16 describes generic specialization. C3's `Box<Token>` pair selects TokenError. An attempt call demanding GeneralError rejects, R0313, instead of choosing the generic default. The generic helper must not normalize its E to GeneralError before that specialization is known.

Case C17 describes missing generic evidence. Calling into<string>(value:T) without a justified relationship rejects before monomorphization, R0302. Bare `into(string)` does not relate T. Merely declaring E does not establish `tryInto(To,From,E)` or display(E).

Case C18 is a function-value excerpt using C1:

```rill
let convertId: (&UserId) => string = into<string>;
```

The context establishes From and To; the selected borrowed conversion has that signature. An unresolved family without endpoint context rejects. A fallible function value also checks its selected E, not a preferred fallback.

Case C19 describes protected identity. Infallible `LocalBox<T>`-to-itself uses standard identity. An explicit user identity registration rejects, R0311. A general `LocalBox<T>`-to-`LocalBox<U>` body can serve nonidentity pairs without displacing that action or duplicating exclusive references.

Case C20 describes numeric protection. User numeric overrides reject, R0311. `into<u8>(7u16)` has no full-base-range infallible pair, R0314. `tryInto<u8>(300u16)` is valid and returns OutOfRange if executed. Constant value knowledge neither grants an infallible pair nor changes checked numeric semantics.

Case C21 describes independent families. C1's into<UserId>(string) does not supply tryInto<UserId>(string). A non-numeric tryInto identity likewise needs an explicit permitted entry. Numeric safe/identity fallible entries still return Result with ConversionError.

Case C22 describes explicit source access. `into<UserId>(&text)` is not supplied by C1's owned source or C2's separate fallible family. A borrowed UserId-to-string body must copy non-copying text instead of moving it out. An exclusive-source implementation can mutate only through its valid access.

Case C23 describes source escape. Returning &value.text from an owned local UserId source rejects, R0202. Returning an existing external reference carried inside an owned wrapper can remain valid. Borrowed errors follow the same rule; read-only results cannot discard exclusive-source restrictions.

Case C24 describes core activation and error names. Canonical import metadata loads the promised standard numeric-text/owned-decoding entries without separate error-name imports. Names still need source imports when written. Broader RFC 0006 matrix and ScalarError details remain Draft; no extra owned/borrowed adapter follows automatically.

Case C25 describes effects and boundaries. A conversion can suspend, allocate, mutate explicit mutable inputs or panic. Infallibility is not losslessness or purity. Registration supplies no implicit call coercion, conversion chain, show/serializer enrollment or changed JSON numeric policy.

Case C26 describes canonical evidence and reporting. A local namesake into/tryInto supplies no canonical registration or requirement proof. Diagnostics distinguish pair absence, duplicate keys, ambiguity, endpoint permission, protected overlap and selected-error mismatch. No runtime type-name registry or user-code execution supplies missing evidence.

| Case / artifact and revision | Context | Expected outcome | Owning rule | Evidence label / observed result |
| --- | --- | --- | --- | --- |
| C1/C2/C3, this draft | Complete guide modules. | Distinct destinations, aliases, explicit borrowing and specialization-dependent E. | [Declarations](#canonical-declarations-aliases-and-implementations), [selected errors](#selected-error-evidence-in-generic-code). | NotChecked. No Rill execution. |
| C4/C14/C15/C16/C17, this draft | Generic output routes, arbitrary E and required evidence. | Apply confirmed determinacy without fallback-error normalization or missing relations. | [Output determination](#proposed-determination-of-fallible-outputs), [evidence](#selected-error-evidence-in-generic-code). | NotChecked. No finite solver or symbolic evidence proof. |
| C5/C6/C7/C8, this draft | Independent source/target context, E checks, aliases and module ownership. | Reject missing context, mismatch, duplicate pair or unauthorized endpoints. | [Calls](#calls-and-independently-established-endpoints), [collection](#pair-collection-and-canonical-identity), [permission](#endpoint-placement-permission). | NotChecked. No inference/resolver/checker implementation. |
| C9/C10/C11/C12/C13, this draft | Variant admission, correlated/crossed pairs and requirement states. | Apply confirmed whole-pair and conversion-only applicability policies without treating unknown as false. | [Matching](#proposed-whole-pair-matching-and-specificity), [applicability](#proposed-requirement-applicability-and-selection). | NotChecked. The policies are confirmed; complete finite judgments remain missing. |
| C18/C19/C20/C21, this draft | Function values, protected identities/numeric pairs and missing adapters. | Preserve exact signature/error and standard behavior without extra pairs. | [Protection](#protected-numeric-domain-and-infallible-identity), [function values](#function-values-ownership-and-effects). | NotChecked. No target lowering or numeric execution. |
| C22/C23/C24/C25/C26, this draft | Source/callback/native obligations, core loading, effects and diagnostics. | Preserve dependencies and explicit interfaces without hidden registration or inferred capabilities. | [Ownership](#function-values-ownership-and-effects), [core boundaries](#core-registrations-and-application-boundaries). | NotChecked. No borrow/runtime/provider/diagnostic integration proof. |

The [diagnostic registry](../diagnostic-codes.md) fixes cited meanings. Output-determination, recursive proof and detailed matching diagnostics still need exact integration. An incomplete checker cannot report successful conversion evidence.

## Feature interactions

### Ordinary families, types and errors

RFC 0004 owns implement syntax, ordinary duplicate/placement keys and ordinary no-fallback selection. Conversion-specific target/source matching and confirmed applicability belong here. Do not spread these exceptions to all families merely because E appears in a signature.

RFC 0005 supplies input admissibility and canonical type relationships. Target equality does not introduce generic-container covariance or source-width guessing. Construction context and existing typed values remain different.

RFC 0007 owns actual source routes, reborrowing, capture/evidence transport and initialized cleanup. This RFC cannot use an owned argument or result to infer noBorrow. The complete selected-signature/caller transfer judgments remain required.

RFC 0008 owns wrappers, exact propagation and independent cause retention. The tryInto operation checks expected E after selection. Err construction instead uses initial payload context. No conversion error adapter runs implicitly at either boundary.

RFC 0006 owns numeric/core pair values and parser/UTF-8 behavior. Its confirmed strict entry points remain distinct from the broader draft audit. This RFC supplies general extension/selection rules, not separate competing numeric semantics.

### Libraries, runtime and observations

Named SQL helpers can invoke canonical conversions under explicit endpoint/error requirements. JSON/time/path libraries retain their own precision and policy rules. Type registration alone does not enroll an application model into any protocol.

Compiler/test observation must not call conversions to print or inspect values. The accepted RFC 0001 snapshot (ri-48g, comment 6907867211) forbids extra user callbacks or changed ownership during trusted capture. This RFC grants no exception.

Proper-tail eligibility still counts conversion body/caller cleanup and outstanding native sources. A final-looking tryInto followed by adaptation/wrapping may owe further work. No tail lowering or target support follows from this draft.

### Source reconciliation

| Input | Disposition |
| --- | --- |
| ri-1wl final decision (comment 6886119679), [ADR 0017](../adr/0017-user-implementable-conversions.md) | Preserve canonical pair keys, aliases, endpoint permission, generic/specific entries and selected E. The later RFC policy decision confirms matching/applicability/output precision, not the full checking procedures. |
| [Conversion inventory](../prototypes/11-conversions.md), source revision | Preserve target/source independence, variant enum example, duplicate/foreign rejection, exact errors and borrowed dependencies. The confirmed conversion-only applicability policy preserves ordinary-family selection. |
| [ri-skb/#17 source](0004-functions-overloads-and-generic-requirements.md#source-reconciliation), Draft RFC 0004 | Preserve ordinary parameter keys, broad nested-type permission and winning-body requirement checks. Only conversions receive this pair/permission/selection boundary. |
| [ri-a2c/#23 source](0006-numbers-strings-and-primitive-operations.md#source-reconciliation), later core interface decision (ri-48g, comment 6913810445) | Preserve numeric values/protection and confirmed strict parsing/owned UTF-8 decoding through tryInto. Earlier separate parser/decoder names are historical. Broader matrix details remain Draft. |
| RFC 0007 policies (ri-48g, comment 6915849379), RFC 0008 policies (ri-48g, comment 6917170418) | Preserve actual dependencies, initialized cleanup, exact error metadata and separate retained-cause evidence. Complete underlying judgments remain incomplete. |
| ri-3t8/#41 and [diagnostic registry](../diagnostic-codes.md) | Preserve distinct conversion codes and source/evidence explanations. Output-related failures cannot masquerade as error-driven overload ranking or a successful incomplete check. |
| ri-1zg/#33/#40 and [SQL contract](../sql-contracts.md) | Keep explicit application endpoints/codecs and named consumer helpers. No implicit schema, temporal precision conversion, serialization or enrollment is added. |
| [Rust evidence research](../research/0041-q19-rust-conversion-evidence.md), recorded revision | Retain its specialization-output hazard and actual historical Rust-check scope. Stable Rust's overlap rejection is not Rill's approved rule or proof of this checker. No research command is rerun here. |
| [Historical conversion program](../../prototypes/11-conversions.rill), source revision | Preserve its 14 intended-valid expectations and all artifact bytes. New case descriptions do not turn it into an executed Rill suite. |

## Compatibility and migration

Use into/from and tryInto/tryFrom as canonical identity-preserving spellings. Historical snake_case try names are superseded. Do not register separate alias implementations for the same pair.

Existing ordinary family bodies use implement, but keep their ordinary keys and ownership permission. A local return/error type does not grant ordinary registration rights. Conversion endpoint permission is separately defined here.

Numeric/user conversion failures retain their selected typed errors. Replace fallback-error assumptions with symbolic E relationships or explicit post-selection adaptation. Exact-E postfix propagation and cause origins do not change.

The owner requested predictable core into/tryInto entry points. This RFC preserves that direction without converting policy/context/query/mutation operations automatically. Historical programs keep their bytes and recorded migration notes.

The three precision policies are confirmed. No released conversion registry, solver or ABI exists. Concrete instantiation success is not evidence that an unchecked generic body was valid.

## Security/resource implications

A conversion body is ordinary executable code. It can allocate, expose payload data through effects, suspend, block or panic. Registration is not a purity, validation or authorization certificate.

Endpoint permission prevents unrelated registrations but does not prove semantic correctness or memory safety. Trusted numeric/identity protection and actual source/evidence checks remain necessary. Private metadata must justify native addresses and ownership rather than merely name them.

Matching and recursive evidence can consume substantial resources. A limit or unresolved goal must not become successful fallback selection. The confirmed applicability/determination policies still need finite checking procedures and adversarial validation.

Typed failure does not promise rollback or recoverable allocation. Construction/adaptation can fail fatally; intrinsic cancellation follows its own cleanup rules. No tracing GC, hidden reference counting or scope extension is introduced.

## Rationale, alternatives and prior art

Return-only dispatch for every family would weaken existing ordinary guarantees. Canonical conversion identities instead delimit the destination/source exception. Separate explicit families retain infallible and fallible intent without automatic adapters.

Error-based lookup would allow expected types to change which body runs. Selected E instead describes the normal endpoint winner. Symbolic relationships prevent generic bodies from silently assuming one overridable fallback's error.

Exact target matching preserves requested identity. Permitted source admission preserves ordinary enum inputs without inventing conversion chains. Whole-pair sets retain correlations and do not rank constraints or errors.

The confirmed applicability policy distinguishes conversion inventory language from ordinary winning-body checks. Explicit unresolved states prevent negative assumptions from choosing fallback bodies. It still needs complete recursive-proof rules rather than a claim that a worklist proves soundness.

Determined-output checks reject an arbitrary caller-chosen E while allowing symbolic nested conversion errors. No public associated-type projection is needed. Whether the full checking relation is finite remains authoring/validation work.

The recorded Rust research distinguishes stable coherence from unstable specialization. Rill deliberately permits generic/specific overlap and independently chosen errors. Rust's existing facilities do not prove this combination safe or decide Rill's inference rules.

## Implementation implications

Resolve canonical family/type/module identities before pair collection. Preserve shared-variable patterns and source relationships without changing independently established types. Emit duplicate/protected/permission errors independently of use sites.

Call inference needs explicit endpoint and post-selection output phases. Keep unresolved evidence separate from disproved obligations. Candidate ordering and selected E must remain stable under the rules for one analysis context.

Registration needs checked output-determination routes, including dependent error slots. Generic bodies require symbolic witnesses and exact selected-output equality at callers. Code generation cannot replace a checked fallback signature with an incompatible specialization.

Function-value lowering must preserve the selected signature, permitted admission and actual access/dependency evidence. Runtime conversion bodies use ordinary calling, suspension and cleanup. No runtime type-name dispatcher or automatic codec registry is required.

The existing Rust scaffold has no conversion parser, pair solver, evidence checker or runtime implementation. Full ordinary/conversion integration and target acceptance remain open.

## Evidence and checking plan

### Actual evidence

| Artifact / revision | Evidence label | Rule/question exercised | Checker/tool and command/environment | Expected / observed outcome | Limits, negative/unavailable results and gate links |
| --- | --- | --- | --- | --- | --- |
| ri-1wl decision, ADR 0017, catalogue section 9 and repository base `f33dcf4cf3f815824eeb897135775d7884f65c1a` | HandChecked | Pair/evidence contracts, ordinary-family distinction and later core direction. | Drafting agent, 2026-11-16. Read current ri-1wl decision/handoffs, contracts and example inventory. | Reconciled canonical aliases, error specialization and conversion applicability language. | Source review is not a finite solver, soundness proof or full RFC acceptance. |
| C1 through C26, this draft | NotChecked | Context, matching, applicability, output determination and source/error behavior. | No Rill parser, conversion/evidence checker or runtime is available. | Expected outcomes appear above. No program or candidate/evidence trace executed. | Policy approval does not complete formal judgments or companions. |
| Whole-pair/evidence/determination rules | NotChecked | Correlations, unknown goals, dependent outputs and specialization safety. | No formal model, solver proof or reference checker ran. | No termination/completeness/soundness result is claimed. | Required procedures and native/Wasm integration remain incomplete. |
| Historical prototypes and Rust research | NotChecked | Current Rill conversion implementation acceptance. | Artifacts retain their own historical labels and results. | No Rust research check is rerun or relabeled as Rill execution. | Existing Rust outcomes and authored expectations do not validate this combination. |
| RFC documents, register and glossary, local working draft | ToolChecked | Structure, writing limits, links and preserved earlier rules/artifacts. | Drafting agent, 2026-11-16. Temporary Python checks; `mise run check:docs`. | Nine IDs/register rows and required metadata/headings passed. Sentence/paragraph, fence, link and whitespace checks passed. | Document checks do not execute Rill, validate a solver or audit the formal ASD-STE100 dictionary. |

### Planned checks. Not executed evidence

- Check canonical aliases, destination-first registration and separate ordinary-family keys.
- Reject duplicate pairs and unauthorized/protected entries without requiring a call.
- Test independent source widths, immediate target contexts and missing-context failures.
- Compare pair correlations, source variant admission and crossed incomparable patterns.
- Test proved/disproved/unresolved requirements without error-driven fallback.
- Reject unfounded proof cycles without treating unfinished searches as negative evidence.
- Check determined output routes and reject arbitrary caller-selected error binders.
- Preserve nested selected-error slots symbolically across generic specialization.
- Validate exact-E mismatch after normal endpoint selection and through function values.
- Test protected exact identity, numeric gaps and absence of automatic fallible adapters.
- Preserve external references while rejecting consumed-local success/error escapes.
- Test exclusive-source restrictions through returned views, generics and erased interfaces.
- Verify declared core metadata activation without error-name imports or module scans.
- Keep application JSON/time/SQL semantics and explicit consumer evidence separate.
- Validate actual suspension/cancellation/native-source and target lowering behavior.

These require the corresponding checker/runtime and target profiles. Document checks cannot establish the claimed semantic outcomes.

## Open/deferred questions

<a id="review-choices"></a>

### Confirmed policies

The project owner approved all three policies (ri-48g, comment 6918885645) on 2026-11-16. The pre-recording snapshot has SHA-256 `5801d15d4f8d72cbaedb41e60b278d25c5a1a9fbc891c4fc0038a504da2dd007`.

1. Match complete pairs with exact requested destinations and permitted source input admission. Preserve shared-variable correlations and whole-enum source examples. Establish target substitutions before binding fresh source variables to actual source structure. Do not guess wider source types. Protected identity requires actual exact endpoint equality.
2. Establish conversion-only requirement applicability before specificity, distinguishing proved, disproved and unresolved evidence. A disproved specific candidate can leave a general candidate applicable. Unknown potentially winning/tied candidates cannot justify successful fallback. Expected E cannot force applicability or fallback. Ordinary families retain their existing no-fallback rule.
3. Require determined fallible error outputs at registration. Endpoint/fixed types and already keyed canonical selected-output relationships can determine E. Expected root E, display(E) alone or target guessing cannot do so. Reject underdetermined registrations. Preserve dependent E symbolically when specialization can change it.

These approvals do not accept future unwritten rules or complete checking procedures. Finite matching, recursive applicability, determination/equality and selected-signature ownership judgments remain incomplete. Full RFC acceptance and readiness remain open.

### Remaining in-scope authoring work

Complete finite matching/subsumption/intersection and permitted source-admission judgments with the type/function owners. Preserve declared keys separately from source value admission and selected call signatures.

Complete recursive requirement applicability, output determination/equality and specialization-dependent symbolic evidence. Resolve cycles/type-growing obligations without unfounded success or unsupported negative assumptions.

Complete implementation-binder/contextual inference, function-value adapters and caller/selected-signature ownership routes. Validate mandatory protected standard actions, actual core metadata dependencies and nested success/error escapes.

These gaps prevent the [canonical readiness review](README.md#canonical-reviewready-checklist). Required inconsistencies return to map ri-kl0, not silent ordinary-rule changes or weakened source cases.

### Implementation/validation gates and future scope

Implement pair/evidence collection, inference, diagnostics and lowering. Validate actual Rill bodies and target profiles, not only hand-authored expected outcomes.

This RFC introduces no traits, named requirement groups, public projections, implicit casts/chains, universal error or runtime conversion registry. Additional application protocols and conversion pairs require their own justified review.

## Decision and change history

### Readiness record

No canonical readiness review has occurred. This document remains Draft. The drafting agent checked sentence/paragraph limits and applied unslop as the final prose pass. Structure and link checks passed.

These checks cannot supply missing solver/ownership judgments. Formal ASD-STE100 dictionary compliance remains unverified.

### Human decisions and amendments

The project owner requested this conversions RFC on 2026-11-16. That request authorized drafting, not precision policies or full RFC acceptance. Existing source decisions and companion policy approvals retain their own scopes.

The project owner later approved all three precision policies (ri-48g, comment 6918885645). This approval identifies the pre-recording snapshot below. It does not accept the full RFC, supply missing judgments or establish readiness. Full Decision and Reviewed revision remain Not yet decided and Not yet reviewed.

| Date / actor | Kind and scope of change | Reviewed revision or snapshot | Decision link / rationale |
| --- | --- | --- | --- |
| 2026-11-16, drafting agent | Created substantive Draft RFC 0009, register and partial source coverage. | Source base `f33dcf4cf3f815824eeb897135775d7884f65c1a`, current ri-1wl/core direction and linked Draft companions. This is not a reviewed RFC snapshot. | User request to draft user-implementable conversions. No new policy, full RFC or implementation acceptance is inferred. |
| 2026-11-16, project owner | Approved whole-pair matching, conversion-only requirement applicability and determined error outputs. | Pre-recording draft SHA-256 `5801d15d4f8d72cbaedb41e60b278d25c5a1a9fbc891c4fc0038a504da2dd007`. Local snapshot `/tmp/rill-rfc-0009-before-policy-approval.md`. | Three-policy decision (ri-48g, comment 6918885645). Full acceptance, formal judgments, readiness and implementation/target/client validation remain open. |
