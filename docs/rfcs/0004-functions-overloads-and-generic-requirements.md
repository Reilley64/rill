# RFC 0004: Functions, overloads and generic requirements

| Field | Value |
| --- | --- |
| ID | 0004 |
| Status | Draft |
| Owner | Reilley64. ri-48g tracks this draft. |
| Source inputs | ri-skb decisions (comment 6308621702), with later refinements in [source reconciliation](#source-reconciliation). Repository inputs use commit `f33dcf4cf3f815824eeb897135775d7884f65c1a`. The ri-1wl decision (comment 6886119679) supplies current implementation syntax and conversion boundaries. The policy decision (ri-48g, comment 6911486381) confirms whole-vector specificity, inference before defaults and declaration-wide requirements. |
| RFC dependencies | [RFC 0002](0002-modules-and-program-loading.md) proposes module identity, visibility and graph collection. [RFC 0003](0003-expressions-bindings-and-control-flow.md) proposes expression contexts, return targets and evaluation order. Their full texts remain Draft. Other [companions](#required-companion-rules) remain missing. |
| Replaces / superseded by | None. This draft does not accept or amend another RFC. |
| Decision | Not yet decided |
| Reviewed revision | Not yet reviewed |

## Summary and scope

This RFC proposes function declarations, family implementations, ordinary overload selection and explicit generic requirements. It also proposes contextual function values and compiler-checked callable requirements.

A body-less overload declaration identifies one family. An implementation supplies one body for that canonical family. The glossary defines [function, overload declaration and callable](../../GLOSSARY.md#language).

The module proposal owns canonical declaration identity, imports, visibility and loaded graph membership. This RFC owns parameter patterns, ordinary placement permission, duplicate keys, candidate comparison and evidence use.

Canonical conversions have different keys and endpoint permissions. This RFC preserves those exceptions without owning their complete conversion contract. It does not add traits, methods, public `dyn`, implicit conversions or named requirement groups.

The project owner confirmed whole-vector specificity, inference before defaults and declaration-wide requirements. This document remains Draft. Generic evidence algorithms, exact grammar and missing companions prevent readiness. No Rill parser or checker has validated the examples.

## Motivation

A programmer must know which body a call selects. Import order, runtime values and optimizer choices must not select different bodies.

A generic function must state the operations it needs. The compiler must check its body before producing concrete instances. A successful integer caller cannot justify an unchecked operation on an unconstrained type parameter.

Library functions also accept arrows and overloaded names. Those values need argument, result and invocation contracts. An annotation must not invent permission to copy captures, share them or consume borrowed state.

The earlier source decisions leave checking details for ri-48g. This proposal makes ordinary selection steps explicit and separates them from conversion exceptions. It records remaining evidence work instead of claiming a complete checker.

## Guide-level explanation

A standalone function needs no family declaration. Its signature states every parameter type and its result type.

```rill
function identity<T>(value: T): T {
  return value;
}

expect identity(7i32) == 7i32;
```

This is a complete module, case F1. The expectation states its intended result. No Rill compiler has executed it.

Declare a family before supplying bodies for it. Use `implement`, not another body-bearing `function`.

```rill
type Box<T> = { value: T };

function label<T>(value: &T): string;

implement label(value: &i32): string {
  return "integer";
}

implement<T> label(value: &Box<T>): string where label(T) {
  return label(&value.value);
}

implement label(value: &Box<bool>): string {
  return "boxed flag";
}

expect label(&Box { value: 7i32 }) == "integer";
expect label(&Box { value: true }) == "boxed flag";
```

This complete module is case F2. The generic Box implementation needs evidence for `label(T)`. The Box<bool> implementation has a more specific input pattern and needs no label<bool> body.

This local `label` family is not canonical template formatting. Interpolation still uses canonical standard `show`. Matching names do not register unrelated implementations.

A generic helper states its requirements on its own signature:

```rill
function describe<T>(value: &T): string where label(T) {
  return label(value);
}
```

This excerpt is case F3. It extends F2 and uses that module's `label` declaration. `label(T)` names its family type argument, not the borrowed value-parameter type `&T`.

Requirements do not change overload priority. A missing requirement on the winning implementation is an error. It does not select a less-specific implementation.

## Syntax and reference semantics

The owner confirmed the three syntax policy groups (ri-48g, comment 6931939690) in [annex version 1](../grammar/v0-syntax-v1.md). Its [EBNF](../grammar/v0-syntax-v1.ebnf) and commitment/context rules form one policy review unit. Full annex/RFC acceptance and static-semantics completion remain separate.

### Function and implementation forms

The following grammar is a declaration sketch. `type`, `name`, `block` and expression tokens need the complete grammar companion. Brackets and braces below are grammar notation, not Rill source tokens.

```text
standalone = [export] function name [generics] (parameters) : type [requirements] block
family     = [export] function name [generics] (parameters) : type [requirements] ;
body       = implement [implementation-generics] name [family-arguments]
             (parameters) : type [requirements] block
parameters = [parameter {, parameter}]
parameter  = name : type
requirements = where requirement {, requirement}
requirement = family-name (type-arguments)
            | intrinsic-predicate
```

Every named function, family and implementation writes all parameter types and its result type. A standalone function has one body and no overload declaration. A family declaration has no body, including no default body.

An implementation must resolve its name to an existing family identity. A standalone function does not become a family when another body uses its name. A second declaration cannot create a second family under one existing binding.

Implementation-owned binders precede the family name, as in `implement<T> label(...)`. Explicit family arguments follow it, as in `implement label<Box<bool>>(...)`. Infer omitted family arguments from the written signature, never from the body.

Do not export an implementation separately. It inherits its family's visibility. An implementation does not re-export the import binding through which it found the family.

The sketch permits requirements on declarations and bodies. Declaration requirements are caller obligations shared by every implementation. A body may add implementation-specific requirements. Its checked assumptions include both sets after signature substitution. The project owner confirmed this policy (ri-48g, comment 6911486381).

There are no ordinary optional, default-valued, named or variadic value parameters. A parameter's generic type may be a fixed tuple. That does not create variadic generics or multiple arities.

Arrow expressions may infer parameter and result types from immediate context. Callable type modes are [defined below](#callable-modes-and-requirements). The shared syntax annex records confirmed typed-arrow and generic-argument parsing policies.

### Proposed M4.1 explicit header elaboration

The owner approved this first-stage procedure (ri-b83, comment 6943516603) at checkpoint `71cbe3707814114b0f504f0458ada41d54c2593d`. M4.1 slice ri-11w remains part of open M4 umbrella ri-b83. Full RFC status and earlier policy approvals remain unchanged.

Use [RFC 0005's canonical declaration-type procedure](0005-types-records-enums-and-inference.md#proposed-m41-canonical-declaration-type-procedure) for named-function and body-less family annotations. Preserve parameter order, binder identities and the explicit result term. Preserve the canonical family identity independently of its source alias.

This stage elaborates types; it does not check function bodies or implementation compatibility. A callable annotation records its written invocation mode. It supplies no capture evidence or invocation permission.

A where clause is an unsupported obligation in this slice. Return Incomplete rather than a completed declaration-type receipt. Do not treat a requirement as proved, disproved or missing evidence. Recursive and type-growing requirements receive the same incomplete classification without starting proof search.

Generic defaults and implementation headers also remain incomplete. Their later procedures must preserve inference before defaults and whole-signature substitution. Exact type equality does not replace alpha-normalized family patterns or whole-vector specificity.

The canonical copy/dispose declarations remain those selected by M3's explicit prelude request. Their annotation terms can be elaborated like other family headers. This does not validate their required library signatures or authorize trusted operations.

No operation gains compiler privileges through its spelling or standard-root location. This stage uses no semantic standard-library definitions or trusted-operation registry. Later operation checks need actual canonical declarations and explicit supplied contracts.

### Proposed M4.2 header validity and prelude signatures

The owner approved these M4.2 checks (ri-9y3, comment 6945089123) after M4.1 elaboration at checkpoint `c26d737439fd4c426d362655ad15984cc70108dc`. Ticket ri-9y3 records the slice. Full RFC status and previous approvals remain unchanged.

Check every module-level named-function and family header in the retained receipt. Reject repeated value-parameter names within one header. Exclude `_`, which binds no usable name. Preserve each conflicting declaration identity and source span. Type parameters and value parameters remain separate namespaces.

This procedure does not check bodies or implementation-family compatibility. Defaults, requirements and implementation headers remain incomplete under M4.1. Public generic requirements, selection, symbolic evidence and function-value compatibility retain their later procedures.

The request explicitly supplies the canonical copy/dispose identities and the following required signature contracts:

```text
copy:    <P0>(&P0) -> P0
dispose: <P0>(P0)  -> canonical-void
```

Each schema has exactly one type binder and one value parameter. P0 denotes that declaration's own binder at position zero. The copy parameter has one fixed read-only reference layer. Dispose consumes its parameter type without adding a reference layer.

Use the actual M3 prelude declaration identities. Reject absent, foreign-capture or substituted contract identities as InvalidRequest. The supplied schemas must equal the two contracts above. Caller data cannot weaken those required shapes.

Compare actual signatures after binder renaming by ordered position. Preserve tuple, nominal, reference and callable constructors during comparison. Parameter names do not affect signature compatibility. Primitive void uses its actual compiler identity, not a same-spelled user declaration.

A structurally admitted prelude declaration with the wrong signature rejects this stage. Retain the declaration span and the mismatched parameter/result type facts. No new stable diagnostic code is assigned by this proposal.

The [library catalogue](../standard-library-contracts.md#2-callable-and-ownership-evidence) supplies the existing contract provenance. These checks do not admit implementations, protected actions or native providers. A signature match proves neither copyability nor correct disposal behavior. It grants no runtime trust.

### Proposed M4.3 explicit standalone calls

M4.3 ticket ri-3gd records this bounded call procedure. The owner approved C1 through C4 (ri-3gd, comment 6947086957) at checkpoint e4750b8. Full RFC status and earlier approvals remain unchanged.

Resolve a direct standalone callee through the caller's lexical/module context. Preserve its canonical function identity. Qualification uses installed exports, without filesystem probes or child autoload.

A nongeneric standalone callee supplies its declared parameter and result types. A generic call in this slice must supply every type argument explicitly. Elaborate those arguments in the caller's type environment. Use one ordered substitution across the complete signature and argument vector.

Check value arity and every argument against the substituted parameter type. Declared parameter context can establish literal widths. A desired result only checks the resulting signature; it cannot select another function or retry another body.

Check every standalone body symbolically once against its own declaration. A call uses the declared signature rather than inspecting the callee's body. Runtime recursion is not recursive evidence search. Concrete callers do not repair an unconstrained generic body's primitive operations.

A body-less family is not a standalone fallback. Ordinary family selection, function values, callable invocation, callbacks, requirements, defaults and omitted generic inference remain incomplete until their owning later procedures exist. Do not infer a callable signature from implementation counts.

These call facts retain actual type arguments, the shared substitution and original callee/argument/result source sites. They establish no move, implicit copy, access loan, capture property or trusted implementation permission.

### Proposed M4.5 ordinary input procedure

M4.5 ri-8p9 follows M4.4 implementation `8abca7a6b933b44c6abc29419b2f324dc51a97a8`. The owner approved P1 through P4 (ri-8p9, comment 6963439018) at checkpoint `3392baaac8f2ee50e86167700349273a76ce61c4`. This approval is for scoped implementation. Full RFC status and earlier approvals remain unchanged.

The owner confirmed exact wildcard capture (ri-8p9, comment 6962750287). Repeated implementation variables bind identical established static terms. They do not choose a shared admissible parent to repair unequal occurrences.

Thus `(T,T)` does not match `(E.Left,E.Right)`. Fixed `(E,E)` can admit both variants. Parent-typed `(E,E)` arguments still match `(T,T)`.

Fixed `(E,E)` and repeated `(T,T)` can overlap without dominance. The former admits mixed variants; the latter admits equal types outside E. A matching call remains ambiguous without another dominating pattern. This consequence belongs to the confirmed precision, not a new per-parameter score.

#### Captured inputs and header compatibility

Use the actual loaded M3 graph before dead-code elimination. Preserve family, implementation, binder, nominal and module identities. Retain original syntax, spans and installed import routes. Do not reopen files or autoload names.

The original M4.1 entry deliberately leaves implementation headers incomplete. Propose a separate ordinary-header entry over the shared type elaborator. Do not require that unsupported source to first obtain the original M4.1 receipt. Preserve existing entry behavior and earlier tests.

Reuse the validated M3 declaration inventory and shared M4.1 canonical type machinery. Run applicable M4.2 checks on that extended header view. These include member uniqueness, canonical prelude schemas, existing type positions and source-nominal inline containment.

Check record/payload fields, variants and named parameters even without calls. Extend parameter uniqueness to implementation headers. Retain each prerequisite's completion and findings. An unresolved or rejected prerequisite prevents a completed collection artifact.

Elaborate module-level family and implementation parameter/result types in their own binder environments. Retain written defaults and requirement references separately. Type elaboration does not prove a requirement or apply a default.

A family header introduces flexible substitution positions for compatibility checking. Implementation-owned binders remain rigid during that check. Explicit written family arguments establish the corresponding leading positions.

Solve the remaining family positions from the complete written parameter/result signature. Use one substitution across that signature. Preserve every fixed constructor, reference layer, invocation mode and repeated family occurrence. Do not use value admission, body contents or requirements to repair signature disagreement.

Use bounded first-order equations with an occurs check. Nominal definitions never expand. A family position can contain a finite term using implementation-owned binders. A rigid implementation binder cannot become a concrete type merely to repair an incompatible header.

Reject inconsistent signature equations. Report unresolved family positions as Incomplete. Do not supply unused/result-only positions from guessed arguments or premature defaults. M4.6 owns default application and general call inference.

Keep requirement targets, arguments and source positions when they can be resolved canonically. Unresolved requirement metadata cannot prove evidence or absence. Requirement truth and body checking remain unperformed here. Public-interface visibility obligations retain their existing owners.

#### Canonical keys and admitted static shapes

Build an ordered input vector from each implementation's written parameter types. Replace its input-occurring binders by indices assigned at first occurrence across the entire vector. Preserve repeated indices across positions and nested terms.

Canonical keys retain primitive identities, nominal applications, variant identities, tuple arities, reference mutability and callable signatures/modes. Imported aliases retain their defining identities. Parameter names, results, requirements and unused binders add no ordinary key component.

A pattern variable captures one exact static term at its occurrence. Later occurrences require that same term. A fixed constructor instead checks its own permitted input relation. Do not widen a variable's captured term to satisfy another occurrence.

For comparison, expand only fixed outer input admissions into finite alternatives. An owned parent-enum pattern includes its exact parent type and each declared variant application. A read-only reference to that parent includes the corresponding read-only variant references.

Preserve the parent arguments in every alternative. Mutable references remain invariant. Tuples and generic arguments gain no covariance. Do not expand nominal field definitions or solve inhabitation.

At a fixed outer callable type, enumerate only permitted invocation-mode weakening. A read-only expected mode accepts only read-only actual mode. A mutable expected mode accepts read-only or mutable actual mode. A consuming expected mode accepts all three actual modes.

Preserve callable parameter/result types exactly. A callable inside a reference, tuple or nominal argument gains no additional variance. These type comparisons prove no capture or invocation-access property.

Take the finite cross-product across the whole input vector. Keep each variable index shared in every alternative. Deduplicate identical alternatives. Source order remains diagnostic provenance, never selection priority.

Canonical never remains an ordinary exact type term in type-position comparison. Its no-value admission is a separate expression rule, not wildcard binding or an added type-pattern alternative. A bare canonical never result supplies no inferred input shape by itself. Independently typed intermediates retain their declared types despite lost continuation. Thus an i32-result call still supplies i32 after a never argument.

An input query must distinguish established type contracts from unresolved or bare-never expression positions. A never contract must come from an independently supplied receiving contract. Merely declaring a source binding or callee result as never supplies no missing shape. Bare never cannot establish omitted generic arguments, widths or receiving shapes.

#### Finite inclusion, intersection and witnesses

An alternative is a finite constructor vector with correlated variables. Use separate variable namespaces for each implementation and comparison. Internal pattern variables are not Error/Unknown value types.

To test inclusion of alternative A in alternative B, match B against A's symbolic vector. Treat A's variables as distinct rigid symbols. One consistent substitution for B must explain every position. Do not solve A's symbols or forget equality between repeated occurrences.

For finite alternative sets, require each A alternative to be covered by a B alternative. This proposal relies on the unrestricted static type-term domain. A finite collection of proper constructor restrictions cannot exhaust an unconstrained variable.

Use fresh structural tuple arities to justify that boundary when needed. Choose arities absent from the compared finite patterns. Give distinct rigid variables distinct fresh shapes. These are symbolic type witnesses, not invented nominal declarations, core definitions or runtime values.

The separate model must challenge this finite-union rule before Rust implementation. Include correlated and nested cases, callable modes and all fixed enum admissions. A counterexample returns this procedure to design review. It does not authorize a weaker approximation.

A is strictly more specific when inclusion holds only from A to B. Equal matching sets do not establish strict priority. Crossed vectors remain incomparable. Equivalent sets with different canonical keys do not become ordered by traversal order.

For intersection, rename both variable namespaces apart. Unify the complete alternatives with one occurs-checked substitution. Constructor or equality conflicts disprove that alternative pair. One finite successful pair supplies an intersection witness. Residual variables can use valid type terms such as canonical bool for this witness. That choice is not source generic inference or a numeric default.

Report disjointness only after every pair is proved not to intersect. Missing admission information, unknown metadata and exhausted limits remain unresolved. Keep the responsible source identities and any already proved witnesses.

These witnesses concern static type patterns. They prove neither inhabited values nor executed calls. Do not use recursive storage emptiness to eliminate an input pattern.

#### Collection, placement and protected overlap

Collect each actual implementation identity once through all loaded routes. Resolve its target to one canonical family. A standalone function is not an implicit family. Preserve each route without duplicating the body.

Reject distinct ordinary bodies with the same canonical input key, even without calls. Returns, requirements and binder spelling cannot rescue duplicates. Reject this condition before call evidence checking, with R0304.

Check ordinary placement using canonical defining modules. The family module grants permission. Otherwise, a parameter must contain a nominal identity defined by the implementation module. Traverse nested arguments, references, tuples and callable types without expanding nominal definitions.

A variant's defining enum supplies its owner. A bare binder grants no ownership. Return-only and requirement-only nominal identities grant no placement permission. Imports and re-exports do not transfer ownership.

Require established family-role classification before authoritative ordinary collection or source diagnostics. Canonical conversions retain their separate pair rules. Missing role admission cannot justify treating a possible conversion family as ordinary. A same-spelled ordinary family gains no conversion exception.

Conditional ordinary views may compute hypothetical keys and comparisons while role classification is pending. Attach the ordinary-role premise to every derived fact. Do not label those facts as conclusive collection, source rejection or checked calls.

Use the [protected-overlap contract](0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m45-protected-overlap-boundary) for actual canonical protected actions. Ordinary placement alone grants no replacement permission. Check unused implementations too.

RFC 0007 owns required-provider and forbidden-input protection records. It defines their validation and the decision from intersection/dominance results. This section supplies comparison witnesses, not a second protected-overlap rule.

A missing or partial protected catalogue keeps clearance Incomplete. Do not replace it with an empty caller whitelist. Known ordinary duplicate/placement findings remain available, but no completed protected collection artifact follows.

#### Input selection and completion boundaries

Compute selection only after the relevant headers, keys, candidate inventory and ordinary placement checks complete. Distinguish conditional ordinary views from conclusive input facts. An input winner is not an executable or evidence-complete call.

A pending role permits only a hypothetical ordinary outcome with that premise attached. It cannot produce a conclusive ordinary winner, absence, ambiguity or source diagnostic. Known ordinary role permits input facts while protection/evidence clearance remains separately pending.

Validate every requested identity against the same graph capture. Validate complete type applications and vector arity. Reject foreign identities and malformed requests separately from source rejection.

Ground established input contracts use the finite alternatives above. Missing shapes, unresolved contextual literals/callbacks and symbolic specialization-dependent arguments return Incomplete in this slice. They cannot prove no matching implementation. M4.6 must discharge these call obligations inside M4.

These are type-vector queries, not a second source-call typer. They cannot infer receiving contracts from implementation counts or retag source values. M4.6 must establish source-call contracts while preserving actual types and explicit admissions separately.

Determine input-relevant family positions from the declaration's complete parameter terms. This includes callable result terms nested inside a parameter type. Apply explicit input arguments through each candidate's retained family-header substitution.

Use the resulting implementation-binder equations only to filter that candidate's input match. Preserve exact variable capture while checking those equations. Do not substitute the constrained vector into the specificity table.

Always rank the original normalized patterns and their original admission alternatives. At `f<E>`, constraining generic `(T)` to E cannot make it outrank fixed `(E)`. Filtering does not redefine either candidate's matching set for priority.

An argument absent from the declaration's input terms is output-only. Retain it as a selected-signature agreement obligation. It cannot filter candidates or resolve ambiguity. A desired result is not a selection input.

Preserve variable markers when applying explicit input constraints. A preconstrained variable still requires exact capture; it does not become a fixed parent-admission node.

Match each candidate with one correlated substitution across the entire vector. Exclude only proved mismatches. An unresolved potentially relevant candidate prevents a conclusive winner or absence result.

A sole matching candidate supplies a winner fact only after all other candidates are proved mismatches. Otherwise, require one candidate strictly more specific than every other settled match. Record every comparison used by that proof.

Prove absence only after every candidate's match has been disproved. Prove ambiguity only after all candidate matches and all possible winner comparisons have settled. An unresolved candidate might dominate existing matches; two incomparable matches alone do not settle ambiguity.

Any required unfinished header, match or comparison keeps that outcome Incomplete. Resource exhaustion yields ResourceLimit. Hypothetical outcomes retain their role premise and cannot emit source diagnostics.

With an established ordinary role, proved absence has meaning R0300. Settled matches without a dominating winner have meaning R0303. Protection and evidence obligations still prevent completed collection/call artifacts.

Do not check requirement truth to rank candidates. Retain declaration-wide and selected-body requirements after selection. Missing selected evidence cannot trigger fallback. Actual requirement satisfaction, output equality and source-call integration remain M4.6 obligations.

Keep analysis completion separate from useful partial input facts. Missing core/protected admission forbids a completed global collection receipt. Analysis and selection reports retain this overall incomplete state. Their partial facts distinguish hypothetical outcomes from conclusive input relations with separate pending clearance. Consumers cannot treat it as a checked call or full typed artifact.

#### Bounded implementation and validation

Use explicit worklists for equations, alternatives, comparisons and candidate visits. Charge temporary substitutions, paths, products, witnesses and caches before growth. Key caches by graph, comparison mode, binder namespaces and complete input terms. Admission-dependent entries also retain the exact catalogue identity.

Bound work, retained entries and depth independently. Check arithmetic before increasing counters. Keep the existing absolute depth ceiling. ResourceLimit precedes Incomplete, then SourceRejected; retained secondary findings are not exhaustive.

The proposed public seams are ordinary analysis, canonical family-fact queries and input-vector selection. The implementation plan defines their scope and expected failure distinctions. They expose original source routes and proof witnesses without creating source bindings.

Before Rust, validate normalization, inclusion, intersection and selection with a separate finite model. Compare bounded ground enumerations against symbolic comparisons. Include fresh-arity separation and adversarial correlations. Model execution is not a universal proof or Rill execution.

See the [M4.5 acceptance record](../../crates/compiler/tests/m4-5-acceptance.md) for the reference model, compiler assertions and unrun acceptance gates. Neither the model nor these compiler assertions execute Rill.

### Bodies, state and invocation

Named functions receive runtime state through parameters. They cannot capture module startup lets or enclosing runtime lets. Arrows can capture state under the ownership rules. Static function and type names are not captured runtime lets.

Each function can suspend. Callers wait for its result without `async`, `await` or a public Future type. Read-only invocation does not mean purity, termination or freedom from suspension.

Block bodies use explicit return. Expression arrows return their expression. Void fallthrough, never-returning paths and propagation targets belong to [RFC 0003](0003-expressions-bindings-and-control-flow.md#return-propagation-and-wrapping).

Evaluate the callee and arguments under [RFC 0003's evaluation order](0003-expressions-bindings-and-control-flow.md#evaluation-order). Overload resolution adds no runtime probing, repeated arguments or hidden conversion chain.

Ordinary function values can represent runtime choices between functions with one established signature. Calling such a value is not runtime overload selection by argument type. This RFC does not forbid function records or checked arrows.

### Generic arguments, inference and defaults

A named declaration introduces ordered type parameters with `<T, U>`. Each use has one substitution for those parameters. Repeated occurrences of a parameter refer to the same type.

Calls may supply an explicit leading sequence of type arguments. Infer omitted trailing arguments from independently typed arguments and immediate expected context. Do not use later statements to repair an earlier call.

Trailing type parameters may have defaults. First use explicit arguments and available inference. Then apply a declared default to a still-unresolved trailing parameter. Substitute earlier arguments into that default. Defaults do not override known types or repair conflicts.

The project owner confirmed this inference/default order (ri-48g, comment 6911486381). Numeric types still need independent width context or suffixes. No default numeric width, implicit cast or primitive singleton type follows from generic inference.

Immediate expected results may complete generic substitution. They must not rank ordinary implementations or distinguish bodies with identical parameter patterns. A selected body's result must agree with the completed declaration signature.

Explicit family arguments constrain occurrences in input patterns. An output-only argument cannot remove a matching input pattern or rank candidates. Check its agreement after selection.

A loose family can expose an output parameter absent from its inputs. For example, `first<C, R>(from: C): Option<R>` lets a selected input pattern determine R. Resolve the input pattern before checking that output relationship.

A conflicting expected result is an error after selection. It cannot force a less-specific body. Missing or non-unique type information requires an annotation rather than a guessed candidate.

The shared syntax decision confirms rejection of implementation-binder defaults and type holes. It confirms generic-use syntax, not new partial application or first-class polymorphic values. The earlier sketch alone did not approve those choices.

### Family shape and canonical patterns

An ordinary implementation specializes its declaration's type parameters. Preserve parameter count, fixed reference constructors and the declared result pattern under the same substitution.

A substitution may itself contain references. Thus a declaration parameter C can specialize to `&List<T>`. A declaration parameter `&T` must remain `&` applied to its substituted T. Do not remove that fixed read-only layer.

Implementation parameter names need not match declaration parameter names. Names do not affect candidate identity or priority. Return types and requirements do not create ordinary dispatch keys.

Canonical parameter patterns retain nominal declaration identities, enum variants, reference structure, tuples and type constructors. Normalize imported aliases and implementation-binder renaming. Preserve repeated-variable relationships across the complete parameter list.

For duplicate comparison, rename variables by first occurrence across the parameter list. `(T, T)` equals `(U, U)` after renaming. `(T, U)` is a different pattern. A binder used only in requirements or results does not distinguish ordinary input keys.

A whole-enum type differs from each variant type. Primitive literal values are not types or overload patterns. Equal record fields do not make distinct nominal records equal.

Implementations have no `when` conditions. Use a branch or match inside the selected body for value-dependent behavior.

### Collection and placement permission

Use the loaded graph and canonical identities from [RFC 0002](0002-modules-and-program-loading.md#family-collection-and-metadata-boundaries). Repeated routes to one body contribute it once. Distinct bodies with identical canonical ordinary parameter patterns are collection-time errors, R0304.

Check duplicates even without a call. Different returns, requirements, aliases or binder names do not rescue them. Generic and specific patterns may remain distinct without guaranteeing unambiguous calls.

An ordinary implementation is permitted in the family's defining module. Otherwise, its module must define a nominal type occurring anywhere in a parameter type. Occurrence includes nested type arguments and reference layers.

A return-only local type grants no ordinary placement permission. A bare generic parameter has no defining module. Imports and re-exports preserve the original type owner rather than granting ownership to the importing module.

This is the ordinary orphan rule. It does not require an outermost local type. A local Payload inside ForeignBox<Payload> can grant ordinary permission. Conversion endpoint permission deliberately differs.

Placement permission is not a coherence proof. Duplicate, ambiguity, protected-operation and ownership checks still apply.

<a id="proposed-whole-vector-specificity"></a>

### Whole-vector specificity

A parameter vector is the ordered list of all parameter patterns for one implementation. Its matching set contains static argument-type vectors that fit one consistent substitution. Apply only the type companion's permitted argument relations.

Compare vectors before testing implementation requirements. Do not include return types, requirement truth or import order in their matching sets.

A is more specific than B when every vector matching A also matches B. At least one vector matching B must not match A. Compare the complete vectors, preserving repeated-variable equality across positions.

This definition preserves the all-parameter rule. A narrower first parameter does not compensate for a wider second parameter. For `(Point, T)` and `(U, Point)`, neither vector dominates the other. A `(Point, Point)` call is ambiguous without another dominating pattern.

The definition also resolves correlated variables. `(T, T)` is more specific than `(A, B)`. It accepts equal pairs only, while the latter accepts unequal pairs. The project owner confirmed this precision (ri-48g, comment 6911486381). Earlier source decisions did not define that case.

At a concrete call, use each argument's established static type. A known enum variant can match its variant pattern or an allowed parent pattern. A whole-enum argument cannot match variant-only patterns. Use `match` for runtime branching.

Owned and read-only variant widening follow the type contract. Mutable variant access must not widen to mutable parent-enum access. Do not introduce covariance through arbitrary containers or callable types.

This set definition states the confirmed relation, not an executable comparison algorithm. The checker still needs a finite symbolic method for the full type grammar. Declared requirements must not affect that comparison.

### Ordinary call selection

Use this order for an ordinary family call:

1. Resolve the canonical family in the caller's module context.
2. Establish explicit type arguments and independently known argument types.
3. Match ordinary input patterns using the permitted type relations.
4. Find the unique pattern more specific than every other matching pattern.
5. Complete the selected signature's permitted contextual inference.
6. Check declaration and selected-body requirements.
7. Check result agreement, access and retained dependencies.

A sole matching pattern is the winner. No matching pattern means R0300. Multiple matching patterns without one dominating winner mean R0303. Missing or ambiguous inference means R0301, not arbitrary selection.

A missing requirement on the selected body means R0302. Do not discard it and retry a less-specific body. Constraint truth is not an overload tie-breaker. A body-less declaration never supplies a fallback implementation.

Contextual arrows and overloaded callback arguments need the staged process below. The ordered steps do not demand complete callback types before their receiving input pattern is known.

### Contextual callbacks and function values

First select a receiving input shape from independently established inputs. For map, distinguish `List<T>` from `&List<T>` before contextualizing its callback. The owned form supplies T; the borrowed form supplies `&T` as callback input.

Then use the selected callable requirement to establish callback argument and result relationships. An arrow can infer missing parameter types there. Its checked body can establish its result. Do not inspect an eventual monomorphized body to invent those relationships.

An overloaded name used as a value takes its declaration's type. Explicit family arguments or immediate callable context must complete its substitution. Then select the body under the same ordinary input and requirement rules.

Do not guess a function-value type because only one implementation is loaded. An unresolved family is not a runtime dispatcher. Adapt a different signature with an explicit arrow rather than changing the family declaration's type.

Direct tuple and List literals need a unique established literal interpretation. Expected return preferences alone cannot change one into the other. Unresolved receiving shapes or callback contexts require annotations.

The library catalogue defines fixed 2-, 3- and 4-item callable tuples. Each position preserves its own input, result and capture evidence. This RFC does not add an ellipsis type or a dynamic work-list dispatcher.

Complete inference for direct callable tuples, nested arrows and overlapping callback-dependent input patterns still needs formal rules. The examples below record both resolved and unresolved contexts.

### Requirements and symbolic evidence

A requirement `family(A, B)` names the canonical declaration and its ordered type arguments. It is not a same-spelled structural promise. For `show<T>(&T): string`, `show(T)` supplies T, not `&T`.

Requirements retain the selected signature, access, property and dependency evidence. An output-related argument remains part of that relationship. For `first(C, R)`, evidence must establish the selected R, not merely that some first implementation exists.

Check generic bodies with symbolic type parameters before monomorphization. Use stated requirements and justified structural intrinsic evidence. An unconstrained parameter may contain owned values, borrowed values or exclusive references. A current integer caller does not narrow its declaration.

A generic body cannot assume a concrete result from an overridable fallback. Preserve the output symbolically or require exact equality. Concrete callers must establish that relationship against the normal winning implementation.

Monomorphization instantiates a checked body. It does not infer missing requirements, strengthen capture properties or reinterpret an unchecked operation. Requirement checking must preserve ordinary selection and its no-fallback rule.

List every required operation or intrinsic explicitly. ri-tb9 (comment 6853754987) excludes named requirement groups and aliases in v0. Repeating `eq(T), hash(T), copy(T), show(T)` is intentional.

Public generic requirements must name exported canonical family declarations. Origin public paths are sufficient. A facade need not re-export every requirement family. Metadata does not import those names into caller source.

The proposed evidence checker must not accept an unfounded requirement cycle as proof. A repeated goal needs already justified evidence, not its own pending result. This does not forbid runtime function recursion or using an explicit caller assumption.

Type-growing goals can exceed available checking resources without repeating. Report incomplete analysis or a resource failure rather than successful evidence or a fabricated missing implementation. Exact cycle handling and resource diagnostics need the checking companion.

Formal rules still need to cover specialization-dependent requirements on constructed generic types. A fallback proof alone cannot justify every later specialization. This gap does not authorize weakening source contracts or adding hidden body rechecking.

### Callable modes and requirements

Callable types retain one invocation contract:

| Type form | Required invocation access |
| --- | --- |
| `(A) => R` | Read-only, repeatable access. |
| `mut (A) => R` | Exclusive mutable, repeatable access. |
| `once (A) => R` | Ownership consumed by invocation. |

Infer the necessary mode from arrow captures and their use. Read-only capability can weaken to mutable or once capability. Mutable capability can weaken to once capability. The reverse conversions are invalid.

These mode conversions preserve argument/result agreement and dependencies. They do not add arbitrary function-type variance. A named mutable callable binding needs mutable access when invoked. Consuming a callable requires ownership, not merely `&mut` access.

Generic functions retain concrete F through compiler predicates:

```rill
where callable(F, [A], R)
where callableMut(F, [A], R)
where callableOnce(F, [A], R)
```

The brackets list argument types. `[]` means no arguments. Read-only capability satisfies all three predicates. Mutable-repeatable capability satisfies mutable and consuming requirements. A consuming-only callable does not satisfy repeatable requirements.

Invocation still needs the required access to F. A borrowed `&F` can forward read-only invocation. An exclusive `&mut F` can forward repeatable mutable invocation. It cannot consume F's owned captures through that borrow.

Callable requirements are compiler predicates, not user-implementable families. The same applies to `noBorrow`, `sendable` and `shareable`. User overloads or same-spelled declarations cannot forge them.

Invocation mode does not establish purity, copyability, borrow-freedom or concurrent sharing. Capture evidence remains separate. Generic F preserves that evidence; an erased arrow interface cannot invent omitted environment guarantees.

Moving or storing a callable preserves its actual borrowed dependencies. An annotation cannot duplicate exclusive captures. Exact capture formation, copying, erasure and callee-exit rules need the ownership companion.

### Proposed M4.6 inference and evidence procedure

M4.6 ri-w05 follows M4.5 implementation `830a651d88bf1703bbfa877873ebd2289e52adcc`. This procedure and its public interfaces need scoped approval. The full RFC remains Draft.

The owner confirmed the fallback context rule (ri-w05, comment 6964354024). An unconstrained viable input pattern does not supply the same receiving context as a constrained one. A specialized pattern alone cannot resolve that disagreement.

#### Captured headers and local equations

Reuse the loaded graph and declaration facts. Check applicable M4.2 prerequisites on the extended header inventory. Do not require the earlier bounded entries to accept generic or implementation-bearing source.

Complete the declarations needed for the body, including default syntax and requirement metadata. Follow [RFC 0002's metadata boundary](0002-modules-and-program-loading.md#proposed-m46-requirement-metadata-boundary). Use the [type-application procedure](0005-types-records-enums-and-inference.md#proposed-m46-nominal-application-inference) for annotations and nominal construction. Continue M4.5 header equations from their known positions. Defaults may fill unused family positions after all written signature equations settle. They cannot repair a header conflict.

Use a new substitution frame for each use. Retain source sites for written arguments, expected types, independent inputs, defaults and selected equations. A frame owns flexible slots for its declaration's omitted positions. All enclosing body parameters are rigid. Never equate distinct rigid parameters by widening either one.

Solve one set of first-order equations across the complete type terms. Decompose equal fixed constructors. Preserve reference layers, invocation mode, arity, nominal identity and correlations. Check for a cycle before binding a slot to a term. A cyclic or incompatible equation rejects; a slot awaiting information is not a value type.

Apply explicit leading arguments first. Use the immediate fixed contract and independently established inputs. For a standalone callable, its declared result and immediate expected result may supply equations. Do not pass a desired ordinary-family result into its input frame before selection.

A free receiving slot captures the exact established type. A fixed outer receiving shape may use an existing admission. Solve its inner arguments by equality. For example, compare an admitted variant's parent arguments against the fixed parent pattern. Never invent an admission inside a tuple element of an already typed tuple.

Two independent anchors for one otherwise free slot must agree. Do not pick a first anchor and widen the second. An explicitly fixed receiver can use ordinary admission without changing an argument's actual type. Keep that admission as a separate fact.

Do not bind a slot or choose an input from bare never. If a receiving type already exists, use it for the input contract of the noncontinuing argument. Retain never as that argument's actual type. Independently typed intermediate expressions retain their declared types despite noncontinuation.

#### Worklist and defaults

Register each fresh argument, field, literal and arrow once. Record its waiting prerequisites. Collect fully known source types and written nested annotations before finalizing dependent expressions. An existing typed argument is never retyped from a receiving contract.

Advance ready constraints until no slot or waiting expression can advance without new context. Use the resolved portions of fixed parameter, field or callable types. An explicit arrow parameter annotation can constrain a known structural receiver before the arrow body is ready. Do not inspect a callee or implementation body to supply a hint.

For each deferred constraint, record the slots it must read before it can advance and the slots it may solve. Include aliases and dependent equations. Drain the ready work before testing defaults. An unavailable premise is not a completed inference pass; keep its possible writes pending.

A default is ready if its slot is still free, its written earlier dependencies are established, and every remaining producer for that slot must first read that same slot. Count indirect reads through other waiting constraints. A producer with an unavailable prerequisite or an unknown dependency does not satisfy this test. A producer blocked only on another slot can still infer this one, so it blocks this default.

Apply the first ready default in binder order, then drain ready work again. Skip blocked defaults, without changing the order of their written dependencies. Never override a solution or use a default to repair a conflict. With no ready work or default, retain the blocked dependencies and report missing context only if the relevant premises have settled.

For example, `f<R=i32,A=bool>(cb: (A) => R)` called with `x => true` has a deferred arrow that needs A and can infer R. Default A to bool first. The arrow then binds R to bool, so the i32 default is not used. If A's default depended on the unresolved R, this path would need independent context.

A receiving default can supply literal context after this process. It is a written generic default, not a numeric-width rule. Distinguish a declared default from a type inferred from an already typed argument.

Check the chosen fresh expression with its established context once. Keep the established facts and mark that expression finalized. Failure never restarts the search with a different width, category, callable signature or implementation. Missing required local context with settled premises has R0301 meaning. A valid number without width in an otherwise established scalar context retains R0003 meaning.

#### Source-call context and ordinary selection

Resolve the callee from the actual lexical bindings, then installed source routes. A local callable shadows a static function name. Written type arguments do not bypass that shadow. Do not reinterpret a conversion or special constructor as an ordinary family while its role is unresolved.

First establish contexts from the declaration and explicit input arguments. Use independent argument types to solve shared declared slots. Do not use a candidate's distinct result pattern or non-callable requirements. Apply ordinary projection only for a needed context that the declaration still leaves open. Only a declaration default for an input-relevant slot can supply context before selection, and only under the readiness rule above.

Obtain all original M4.5 alternatives for the family. For each alternative, match the fully established input positions with one shared substitution. Also impose explicit input-relevant family equations through its retained header substitution. Use exact equations for that filter. Leave deferred input positions unbound.

For established symbolic inputs, use the finite coverage and intersection procedure below on the known positions, with the same correlations and filters. A concrete pattern is not disproved just because a caller term is rigid. Use a substituted receiving projection only if it is fixed across the surviving matches. Otherwise keep that candidate as a blocker for the affected projection.

An alternative leaves the context set only when this limited check disproves it or when [RFC 0005's bracket procedure](0005-types-records-enums-and-inference.md#bracket-category-before-elements) excludes its tuple arity. Keep unresolved alternatives as blockers, including those whose metadata is incomplete. Do not use requirements, a fresh expression's trial type, or desired results to remove an alternative. Do not rank partially known vectors.

Project the original receiving terms under the substitution from each surviving alternative. The expanded alternatives describe actual admitted inputs, not receiving contexts. For example, all variant admissions of one fixed parent parameter still propose that parent.

An open slot, unsupported shape or missing projection cannot supply a context. For brackets, use RFC 0005 to settle category and arity before testing element contexts. For other projections, require agreement on each needed shape and canonical type term. Unrelated unknown slots do not block an already fixed projection.

Require every remaining alternative to supply the same projection for each context the source needs. A specialized flexible slot can contribute a type only from an independent anchor. A fixed written parameter or canonical callable clause can itself describe a receiving shape. Require this term witness even for a sole surviving body. The count alone supplies no type or specialization.

For example, a written fixed i32 parameter can supply an agreed width. An unconstrained T in a lone implementation cannot. A bare never expression is not an anchor for either inference or pattern elimination.

For example, in `pair<A,B>` with `(A,B)` and `(T,T)` patterns, `pair(1i32, 2)` lacks the second width. The first pattern is still viable. `pair(1i32, 2i32)` supplies an independent width. A declaration `pair<T>(left: T, right: T)` can itself supply that receiving context from the first argument.

When an argument finishes, add its established type to the vector. Repeat limited matching only on newly established constraints. Do not discard a finalized expression or reuse its output to replace its committed context. All-alternative agreement can only become more precise as independent facts settle.

Apply [M4.5 whole-vector selection](#input-selection-and-completion-boundaries) after every input needed for selection is established. Rank the original patterns. A conditional ordinary context carries its role premise through checking and selection. Missing role or required protection clearance prevents checked ordinary call completion.

After selection, solve the remaining selected signature positions. First use the declared header and independently established callback or goal outputs. Canonical callable clauses can now equate a result slot to the checked callable result. This is an exact equation, not a requirement-based ranking rule.

After selection and ready inference, an explicit output-only argument or immediate expected result can constrain a free ordinary position. A slot is free for this purpose only if no remaining signature, source or evidence inference can determine it. A waiting determination that could solve the slot makes it pending, not free, even if that determination is cyclic or needs another unknown input.

An expected result can fill a free slot used only as an input to a later proof. It cannot supply the proof or solve an output position of that proof on its behalf. Compare expectations with independently determined outputs by equality. Apply remaining ready defaults last. An expected outer result cannot type a deferred callback in a way that would choose an ordinary winner.

Use one complete substitution for the selected body and declaration. Keep declaration and selected-body requirements. No other body's requirements can become alternatives. A missing operation, output mismatch or pending proof never triggers fallback.

#### Callable formation and invocation

A callable receiver's known parameter types can supply arrow context. A canonical `callable`, `callableMut` or `callableOnce` clause can describe the same relationship for a flexible callable slot. Use the actual predicate identity. A same-spelled family or general requirement is not a contextual clause.

For a callable clause, find its receiver by exact identity at the argument position or fixed tuple path. Apply the current substitution to its argument tuple and result. Merge applicable declared callable contexts at that path by equality of the terms they supply. Conflicting contexts block the affected projection; they do not remove the candidate. A required mode is a later demand, not proof of the arrow's mode.

Test agreement across all viable alternatives for each needed parameter projection. An open callable type F does not block a parameter projection fixed by a clause on F. An open result R does not block known inputs; the arrow can infer that result. Use a contextual result only if the declaration fixes it or every viable alternative supplies the same fixed term. A missing clause and no structural receiving type cannot supply context.

In the [borrowed `map` schema](../standard-library-contracts.md#3-collections-equality-and-traversal), a known `&List<T>` can leave only the borrowed pattern. Its `callableMut(F,[&T],R)` supplies `&T` even before F and R are known. For [scoped `join`](../standard-library-contracts.md#6-background-and-scoped-work), commit tuple category and written arity first. Then use `callableOnce(Fi,[],Ri)` at each matching tuple path to supply zero parameters independently of other paths and outputs.

These are context examples, not core admission. Without the actual identities and contracts, the facts remain conditional or pending. A callable clause contributes a written relation, not proof, candidate priority or a variadic rule.

A binding or partially applied type expression does not make a polymorphic function value. For a named standalone or overloaded function value, instantiate its declaration from explicit arguments and its immediate callable context. Establish a named value's complete declared callable type without inspecting implementation results.

For a family, establish the input portion from written arguments and caller input context first. Then select from that parameter vector. Check the result and evidence afterward. Do not let the expected callable result establish a missing dispatch input. In `let f = label`, one loaded `label` implementation cannot supply a missing input type.

An arrow's written annotations and established receiving form must agree. Infer missing parameter types from the agreed contract. Create a new lexical scope with actual parameter identities. Check the arrow body only after its parameter types are known. Use an agreed callback result type if already fixed. Otherwise infer expression results or join written return contributions by the existing type and control rules.

For a block arrow, a continuing path supplies void and a return target belongs to that arrow. Do not let an inner loop transfer or nested arrow return establish its result. If no return or normal fallthrough supplies a result, proven divergence supplies never only without a fixed result contract. An independently typed return expression retains its static type despite noncontinuation. Check all written source even when some paths cannot execute.

Record the arrow's actual callable signature and its separate capture facts. Use [RFC 0007's typed capture boundary](0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m46-capture-and-invocation-facts) for the invocation demand. Callable mode weakening preserves exact parameter and result terms. It preserves dependency restrictions. An unknown capture demand prevents a proved mode conversion.

Keep the callable type used in ordinary comparison separate from its value evidence. Two arrows with the same signature can have different environments. Keep concrete origins when binding F. If one F is fed by several values, a property proof must cover every contributing origin. Do not let one empty environment certify another. An erased callable type supplies no hidden environment facts.

For a call through a value, use its established callable type or a matching declared callable goal for symbolic F. Match the complete argument list. For shared receiver access, require read-only mode. For mutable reference receiver access, permit repeatable mutable or read-only mode. Do not consume an owned environment through a reference. These checks retain access demands, not proof of ownership or loans.

#### Symbolic bodies and evidence goals

Check generic functions and implementations in their own rigid binder environments. Use a declared signature, not observed callers, to type parameters and returns. Check each body before instantiation. A regular call to a declared function never rechecks its source body to strengthen the call contract.

Validate each written requirement's canonical identity, argument count and term kinds. Keep a checked assumption for each valid declaration-wide requirement. For an implementation, substitute the family header in declaration-wide assumptions first. Then add its written requirements. Record their distinct origins. An unresolved requirement declaration is not an assumption.

An explicit assumption proves exactly its goal under that environment. It does not clear an unresolved family role or protected admission. A callable assumption can also prove a weaker mode demand with the same receiver, parameters and result. A whole result equation must agree by exact equality. Do not infer a type equality from an unproved property or choose between conflicting assumption outputs by source order.

A matching ordinary assumption can supply a symbolic call contract. Elaborate its input vector and its input-position filters from the written goal. Match the canonical family and established input vector by exact equality, including correlations. Match the call's explicit filters by equality too. If the filters differ, require a finite proof that they admit the same actual input alternatives per candidate over this vector.

For example, with generic `(T)` and fixed `(E)` where `E.V` is a variant, `f<E.V>(E.V)` and `f<E>(E.V)` can choose different bodies. The first filter excludes the fixed parent body; the second excludes exact capture of the variant by T. One assumption cannot serve both calls merely because they have the same actual input type.

Use matching assumptions' exact output arguments, with no preferred output selection. If applicable assumptions disagree on a needed output, the relationship is not established. Callers must discharge the same full goal by normal ordinary selection at concrete instantiation.

If no assumption supplies the relationship, permit a uniform ordinary winner proof. Form a symbolic input vector from the actual input types. Use one variable per rigid caller parameter for this comparison only. Do not mutate the rigid body type environment.

For each candidate alternative, append its header terms for the explicit input-relevant positions used by the call. Append the corresponding actual terms to the symbolic input vector. This encodes explicit filtering without changing the original ordering. Use the M4.5 procedure on these augmented vectors for coverage and intersection.

A candidate supplies a uniform winner only if its filtered alternatives include the full symbolic vector. Its original pattern must dominate every other candidate whose filtered alternatives intersect that vector. Unknown comparisons block this proof. Retain the matched alternative and exact substitution of every originally captured variable. Do not assert that a concrete fallback wins merely because a rigid term failed a concrete match.

If incompatible specializations remain possible, preserve a pending selected-output relationship. Do not use it as a known type in an exact equality. Source that needs that relationship must state an exact requirement. A fully admitted, settled environment without the needed assumption has R0302 meaning. This is lack of a generic contract, not a claim about every instantiation. Missing role, unfinished contexts or exhausted checks cannot establish that negative.

Separate output determination from a request to prove one full family goal. A determination request contains the canonical family and established inputs. Include explicit input-position bindings, origins and the assumption environment. Do not include a preferred output.

For a full family goal, substitute its type arguments into the declared input positions and retain the resulting explicit input filters. Run ordinary selection on that vector with those filters. Determine the selected output without using the requested output to set a free position. Compare the complete result and output positions by equality. In an inference frame, an open caller output may instead use the independently determined term.

To determine a required output, use the selected header and exact relations with established inputs. A canonical callable clause uses its established callable signature. An ordinary family clause can request determination only after all its input positions are established. Propagate its independently determined output as an equation. An already justified full assumption may supply the same relation.

Revisit waiting equations when those dependencies finish. Use defaults only after available determining equations settle. Do not issue proof until the selected signature and all requirements settle.

For an ordinary call or goal, require both declaration and selected implementation requirements. Compare selected type arguments and outputs by exact equality, not value admission. Distinguish a fixed declared call result from an output that varies with specialization. Retain the selected header, its substitution and the origin of each required equality.

#### Proof worklist and completion

Key proof work by capture, admitted role, canonical target, judgment kind and assumption environment. Determination keys retain ordered input terms and the explicit bindings that filter input positions. Two calls with the same vector but different explicit filters must not share a determination. Full proof keys retain all requested terms.

Include any value origins needed for callable properties. A name alone does not identify a goal. Evidence from another environment is not a proof in this one.

Freeze the caller's checked assumption environment when starting the judgment. All dependency edges use that environment. The selected contract's requirements become goals, not assumptions. The same requirements are assumptions only when checking that implementation's own written body.

First check explicit assumptions. Then check justified intrinsic rules or the one selected ordinary contract. Produce finite dependency edges to required goals. Record a successful derivation only after all required dependencies and equalities complete. Record a disproved goal only from a completed negative check, such as a settled mismatch. Preserve the source sites for both.

Visit goals with an explicit worklist. Mark a goal active while its dependencies are unfinished. A repeated active goal stays unresolved unless an independent assumption or completed derivation already proves it. Retain the cycle path. Do not use a cycle as a proof or negative evidence. Finite recursion of function bodies is separate from evidence recursion.

A growing sequence of distinct goals may exhaust the work, entry or depth limit without repeating an active key. Report ResourceLimit, not an active cycle or a missing implementation. Retain the first unfinished goal and proved facts that fit the budget. Cache completed derivations only under their full keys. Reusing an unresolved attempt never changes it into a completed proof.

For compiler predicates, use [the typed evidence rules](0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m46-capture-and-invocation-facts). Use [RFC 0008's exact error relationship](0008-errors-propagation-and-caused-by-wrapping.md#proposed-m46-exact-error-evidence). Canonical conversion applicability, selected E determination and conversion evidence remain M4.7. Do not use ordinary absence or fallback rules for conversions.

Keep a call's typed arguments, contexts, applicable role, selected substitution, goal proofs and exact agreement together. A failed or unresolved call does not have a checked result term. A known declared signature may remain as separate partial metadata. A failed outer call cannot supply an inferred type to another expression.

Bound work, retained entries and depth independently. Charge equations, deferred nodes, comparison products, goal edges, witnesses and caches before growth. Order visits deterministically by source capture and term positions. Preserve ResourceLimit before Incomplete, then SourceRejected. Do not infer negative facts from a report with unresolved premises.

The [M4.6 checkpoint](../implementation-plan.md#m46-design-checkpoint) defines the public queries and planned acceptance. Apply model-first checks to inference scheduling, partial context agreement and evidence cycles. Model execution would not establish Rill execution. No M4.6 model or compiler implementation has run at this checkpoint.

### Protected operations and conversion boundaries

Ordinary placement permission does not authorize replacement of protected ownership actions. Canonical guard release, counted-owner maintenance and native-source retention keep their trusted identities. Check potential replacement or ties during collection, even without calls.

Do not use requirements, aliases, generic adapters or expected function values to bypass this check. Less-specific ordinary defaults remain permitted only when they cannot displace a required protected action. Do not seal all standard families or impose conversion-style outermost ownership everywhere.

The complete trusted action registry and its ownership proof remain companion work. Required remaining-field cleanup also remains mandatory after custom disposal. Private fields alone do not prove these guarantees.

Only canonical conversion families use `(To, From)` keys. Their selected fallible error E is output evidence, not another key. Endpoint ownership uses outer nominal types after removing reference layers. Local nested payloads and local error types do not grant conversion endpoint permission.

Canonical conversion calls can use explicit destinations or permitted immediate concrete result context. They cannot select a target from candidate counts. Protected numeric pairs and infallible identity remain closed. No ordinary-family return-only exception follows from these rules.

The conversion companion must own endpoint inference, symbolic E equality and protected pair selection. [ADR 0017](../adr/0017-user-implementable-conversions.md) is supporting input, not an accepted companion RFC.

Canonical Err construction has a separate location-capturing one-/two-argument exception. It does not authorize ordinary arity overloading or default value parameters. Use an arrow for first-class construction, as required by the error contract.

### Required companion rules

| Rule owner | Required contribution | Current dependency state |
| --- | --- | --- |
| [Draft RFC 0002](0002-modules-and-program-loading.md#family-collection-and-metadata-boundaries) | Canonical modules, family visibility, public requirements and loaded-body membership. | Its identity/order policies are confirmed. Full resolver and cycle rules remain incomplete. |
| [Draft RFC 0003](0003-expressions-bindings-and-control-flow.md#return-propagation-and-wrapping) | Return targets, expression arrows, pipe contexts, grouping and evaluation order. | Its four policy groups are confirmed. Complete grammar and flow checks remain incomplete. |
| [Draft RFC 0005](0005-types-records-enums-and-inference.md), with complete grammar still missing. | Type identity, variant relations, generic applications and tuple/List literal interpretation. | Proposed, not accepted. Exact callable/type grammar and complete contextual inference remain incomplete. |
| [Draft RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md) | Captures, callable erasure/copying, argument/result dependencies, mutable retained storage and callee exits. | Proposed, not accepted. Full transfer/erasure judgments and the combined checker remain incomplete. |
| Draft RFCs [0008](0008-errors-propagation-and-caused-by-wrapping.md) and [0009](0009-user-implementable-conversions.md). | Pair keys, selected E, identity protection, Err construction and exact propagation. | Both remain Draft. Complete constructor/evidence judgments and conversion coherence remain incomplete. RFC 0009's conversion-applicability proposal does not change ordinary no-fallback selection. |
| Library and runtime integration | Concrete callable interfaces, protected action admission, cleanup and trusted native obligations. | No RFC assigned. This draft does not deliver the library or runtime. |
| [Draft RFC 0010](0010-diagnostics-and-reporting.md) | Evidence traces, incomplete analysis, canonical failures and candidate facts. | Reporting is proposed, not accepted. Complete source, driver and evidence procedures remain incomplete. |

## Valid/invalid examples and diagnostics

F1 through F3 appear in the guide. Each later case is independent unless it names another case's context. All outcomes below are expectations. No Rill compiler output or execution is claimed.

Case F4 is a rejected complete module:

```rill
function label(value: i32): string { return "integer"; }
function label(value: bool): string { return "flag"; }
```

Two standalone definitions cannot create a family. Add one body-less declaration and use `implement` for its bodies.

Case F5 is a rejected complete module:

```rill
function choose<T>(value: T): string;
implement<T> choose(value: T): string { return "first"; }
implement<U> choose(value: U): string where copy(U) { return "second"; }
```

The bodies have identical ordinary patterns after binder renaming. Reject them during collection, R0304, even without a call. The extra requirement cannot distinguish them.

Case F6 is a complete module with an ambiguous call:

```rill
type Point = { x: i32 };
function pair<A, B>(left: A, right: B): string;
implement<T> pair(left: Point, right: T): string { return "left"; }
implement<U> pair(left: U, right: Point): string { return "right"; }

expect pair(Point { x: 1i32 }, Point { x: 2i32 }) == "left";
```

Neither implementation dominates across both parameters. Reject the call with R0303 instead of choosing the first body. The expectation does not execute.

Case F7 is a complete module exercising the confirmed correlated-variable rule:

```rill
function pair<A, B>(left: A, right: B): string;
implement<A, B> pair(left: A, right: B): string { return "different allowed"; }
implement<T> pair(left: T, right: T): string { return "same type"; }

expect pair(1i32, 2i32) == "same type";
expect pair(1i32, true) == "different allowed";
```

The first call selects the equal-type pattern under the confirmed whole-vector rule. The second cannot match that pattern. These are expected results, not observed execution.

Case F8 adds an independent excerpt to F2:

```rill
function needsCopy<T>(value: &T): string where copy(T) {
  let duplicate = copy(value);
  return "copied";
}
```

This is a valid generic contract under ordinary copy evidence. Removing `where copy(T)` makes its generic body invalid, R0302. An integer-only set of current callers cannot repair it.

Case F9 is a complete module:

```rill
enum Language { English, French }
function greeting(value: &Language): string;
implement greeting(value: &Language): string { return "general"; }
implement greeting(value: &Language.French): string { return "French"; }

function known(): string {
  let specific = Language.French;
  return greeting(&specific);
}
function widened(): string {
  let general: Language = Language.French;
  return greeting(&general);
}
expect known() == "French";
expect widened() == "general";
```

The runtime variant is the same. Static argument types select different bodies. Removing the general body makes the call inside widened invalid, R0300. Even complete variant implementation coverage cannot synthesize a dispatcher.

Case F10 extends F2 with a first-class value:

```rill
function renderValue(): string {
  let render: (&i32) => string = label;
  return render(&7i32);
}
expect renderValue() == "integer";
```

The immediate function type establishes the declaration's T. The selected label body has the expected signature. A separate `let render = label;` lacks enough type context and requires an annotation, R0301.

Case F11 is a contextual callback excerpt. Assume F2's family and these additional imports and bindings:

```rill
import std::collection::{List, map};

let values: List<i32> = [1i32, 2i32];
let labels: List<string> = map(&values, label);
```

The borrowed List input establishes callback input `&i32`. Its result context and label declaration establish string output. No conversion or runtime family dispatcher is needed.

Case F12 describes two literal contexts, not complete programs. A declared two-item tuple of checked arrows can supply a fixed-arity join helper. A direct ambiguous bracket literal must not become that tuple solely because a desired result prefers join. The complete literal/context algorithm remains NotChecked.

Case F13 is a rejected function excerpt:

```rill
function strengthen(work: mut () => i32): () => i32 {
  return work;
}
```

The result promises read-only invocation, but the input requires exclusive mutable invocation. An annotation cannot strengthen that mode. A separate attempt to invoke `once () => string` through `&mut` also lacks consuming ownership.

Case F14 is a rejected generic function excerpt:

```rill
function assumeBorrowFree<T>(value: T): T {
  return requireNoBorrow(value);
}
```

Assume `requireNoBorrow<T>(T): T where noBorrow(T)` is a visible checked helper. The body lacks noBorrow(T), R0302. Add that explicit requirement when the public contract needs it.

Case F15 describes an ordinary-placement comparison. Assume module local defines Payload and imports foreign Box and the target family. A parameter `Box<Payload>` can grant ordinary placement permission. A local type appearing only in the result cannot. Conversion endpoints use their separate outermost-type rule.

Case F16 describes protected ownership checking. Assume a user module owns Payload and imports canonical dispose and LockGuard. An implementation for `LockGuard<Payload>` must not replace required guard release. Reject it during collection, R0500, even without calls. Ordinary orphan permission does not authorize that replacement.

Case F17 is an inference excerpt:

```rill
function same<T = i32>(value: T): T { return value; }
let inferred = same(true);
let explicit = same<i32>(3i32);
```

The inferred result is bool, not the default i32. Explicit arguments establish i32 in the second call. A conflict remains an error rather than a reason to replace inferred types with defaults.

Case F18 describes a selected-requirement failure. Assume a generic default matches every input and a more-specific input implementation requires copy(T). The call matches the specific pattern but lacks its copy evidence. Reject it with R0302; do not execute the generic default.

Case F19 describes selected conversion output evidence. Assume `tryInto(To, From, E)` and the normal winning endpoint implementation. A caller demanding another E must fail equality checking, R0313. It cannot select a fallback with the desired error. Full conversion rules remain companion scope.

Case F20 is a rejected complete module:

```rill
let message = "startup";
function readMessage(): string { return message; }
```

The named function cannot capture the startup let. Pass state explicitly or use an arrow with a checked capture contract.

| Case / artifact and revision | Context | Expected outcome | Owning rule | Evidence label / observed result |
| --- | --- | --- | --- | --- |
| F1/F2, this draft | Complete standalone and family modules. | Identity returns its input. Box input selects generic or specific label. | [Forms](#function-and-implementation-forms), [selection](#ordinary-call-selection). | NotChecked. No Rill execution. |
| F3/F8/F14, this draft | Generic excerpts with stated family/helper context. | Listed evidence permits the operation. Missing evidence rejects before monomorphization, R0302. | [Evidence](#requirements-and-symbolic-evidence). | NotChecked. No generic checker. |
| F4/F5, this draft | Independent invalid modules. | Reject undeclared overloading or canonical duplicate bodies. F5 is R0304. | [Forms](#function-and-implementation-forms), [collection](#collection-and-placement-permission). | NotChecked. No collection diagnostic observed. |
| F6/F7, this draft | Independent two-parameter modules. | Reject crossed specificity, R0303. Confirmed repeated-variable ordering selects F7's equal-type body. | [Specificity](#whole-vector-specificity). | NotChecked. The policy is confirmed; no checker result is observed. |
| F9, this draft | Complete enum module and a separately described body removal. | Select by static variant/parent type. Missing parent implementation rejects, R0300. | [Specificity](#proposed-whole-vector-specificity). | NotChecked. No runtime dispatcher test. |
| F10/F11/F12, this draft | First-class and callback contexts, including an unresolved literal boundary. | Resolve the stated contexts. Require annotations for incomplete contexts, R0301. | [Callbacks](#contextual-callbacks-and-function-values). | NotChecked. Full literal inference remains incomplete. |
| F13, this draft | Invalid mode-conversion excerpt and borrowed-once boundary. | Reject stronger invocation guarantees or consumption without ownership. | [Modes](#callable-modes-and-requirements). | NotChecked. Ownership diagnostic not observed. |
| F15/F16, this draft | Named hypothetical module contexts. | Preserve ordinary nested-type permission but reject protected replacement, R0500. | [Placement](#collection-and-placement-permission), [protection](#protected-operations-and-conversion-boundaries). | NotChecked. No overlap checker. |
| F17, this draft | Generic inference excerpt. | Infer bool before using defaults. Preserve explicit i32. | [Inference](#generic-arguments-inference-and-defaults). | NotChecked. The default-order policy is confirmed; no inference result is observed. |
| F18/F19, this draft | Selected ordinary requirement and conversion-error scenarios. | Reject missing selected evidence or incompatible E. Never fall back. | [Selection](#ordinary-call-selection), [boundaries](#protected-operations-and-conversion-boundaries). | NotChecked. No selection/evidence engine. |
| F20, this draft | Invalid startup-capture module. | Reject the named function's access to a runtime let. | [State](#bodies-state-and-invocation), Draft RFC 0002. | NotChecked. No lexical/capture checker. |

The [diagnostic registry](../diagnostic-codes.md) fixes the cited code meanings. It does not prescribe exact message bytes for these new examples. Shape, arity, access and return mismatches still need their precise checking diagnostics.

## Feature interactions

### Modules and expectations

[RFC 0002](0002-modules-and-program-loading.md#family-collection-and-metadata-boundaries) owns declaration identity and loaded-body membership. This RFC owns the ordinary patterns compared after collection. Separate entry graphs do not share accidental implementations through an editor index.

Imported aliases preserve family identity. Public requirements retain exported declaration identities without importing their names into caller source. Test-only-used imports still contribute implementations under the same graph rules.

[RFC 0001](0001-top-level-expectations.md) owns test expectations at its accepted snapshot (ri-48g, comment 6907867211). Helper functions and nested arrows keep their own return contexts. This proposal does not change accepted expectation boundaries or diagnostic capture.

### Expressions, ownership and cleanup

[RFC 0003](0003-expressions-bindings-and-control-flow.md#pipes-and-placeholders) owns pipe insertion and evaluation order. Resolve the resulting call with the same static family and callback rules. A placeholder changes parameter position, not candidate priority or evaluation order.

Moves, implicit copies, explicit copies and borrow acquisition remain ownership operations. Selection cannot fabricate an automatic borrowed adapter. A mutable result dependency must preserve its exclusive restriction, even when its visible result is read-only.

Calls retain dependencies through results, mutable argument storage and captures. Owning an input does not prove it has no borrows. Callee-exit and transitive last-use rules remain required companion work.

Custom disposal cannot suppress remaining-field cleanup. Generic evidence and function-value selection must preserve protected actions. Proper tail calls also require no remaining caller-owned cleanup or borrowed storage obligations. This RFC does not complete their backend contract.

### Library outputs and conversion evidence

Loose collection results and callable predicates must preserve associated signature relationships without adding an associated-type language feature. The catalogue's `map`, `first`, iteration and fixed tuple helpers are checking obligations, not implemented functions.

Canonical display, show and copy are distinct behavioral families. Compiler property predicates remain separate from those families. Formatting or error construction requirements cannot appear merely because diagnostics observe a value.

Conversions and streaming I/O can determine output errors through selected signatures. Preserve those errors symbolically before monomorphization. An expected error checks the winner; it does not rank ordinary or conversion bodies.

### Source reconciliation

| Input | Disposition |
| --- | --- |
| RFC 0004 policy approval (ri-48g, comment 6911486381) | The owner confirmed whole-vector specificity, inference before trailing defaults and shared declaration requirements. This is policy approval, not full RFC acceptance or checker validation. |
| ri-skb original decision (comment 6308621702), [ADR 0003](../adr/0003-overloads-need-a-declaration.md) | Preserve declarations, static all-parameter selection, explicit signatures, generic requirements and contextual function values. Later decisions replace literal overloads and body spelling. |
| ri-457 core decision (comment 6308790321) | Preserve local inference, explicit block returns, expression arrows and trailing generic defaults. Later explicit-width and enum rules replace primitive/singleton sketches. |
| ri-lql final decision (comment 6853088155), [ADR 0007](../adr/0007-explicit-mutation-with-conservative-borrow-dependencies.md) | Preserve invocation modes, non-forgeable noBorrow evidence and checks before monomorphization. Complete ownership/evidence transport remains companion work. |
| ri-40y final decision (comment 6863910595), [ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md) | Preserve distinct sendable/shareable properties, capture evidence and non-capturing named functions. No public trait can forge these properties. |
| ri-owe final decision (comment 6866705217), Draft RFC 0002 | Preserve canonical aliases, declaration visibility, collection-time duplicates and ordinary parameter-occurrence placement. Orphan permission alone does not prove coherence. |
| ri-kty final decision (comment 6870013901), [ADR 0012](../adr/0012-enums-replace-unions-and-literal-overloads.md) | Use static enum variants and parent fallbacks. Remove primitive literal overloads and implicit runtime dispatch. Preserve mutable-variant restrictions. |
| ri-tb9 decision (comment 6853754987) | Repeat individual requirements. No named constraint groups or aliases in v0. |
| ri-8n7 final decision (comment 6872129850), catalogue sections 2/3/6 | Preserve callable predicates, loose family outputs, fixed tuple arities and protected ownership actions. Formal literal/callback inference and the complete protected registry remain incomplete. |
| ri-3t8 final decision (comment 6885970775), [diagnostic contract](../diagnostic-contracts.md) | Preserve separate inference, no-match, missing-evidence, ambiguity and duplicate failures. Navigation must report canonical declarations and honest body candidates. No editor implementation is claimed. |
| ri-1wl final decision (comment 6886119679), [ADR 0017](../adr/0017-user-implementable-conversions.md) | Adopt language-wide implement syntax. Keep conversion-only pair keys, endpoint permission and selected-error evidence distinct from ordinary rules. |
| Source revision's [overload inventory](../prototypes/03-overloads.md), [conversion inventory](../prototypes/11-conversions.md) and [mutation cases](../prototypes/06-mutation-safety.md) | Use as authored design inputs and adversarial obligations. They are not executed Rill tests or normative annexes. No artifact bytes change here. |

## Compatibility and migration

Earlier family bodies written with `function` must use `implement`. Keep standalone functions and body-less declarations as `function`. The source revision already contains this migration in its current examples.

Primitive singleton parameters and general union dispatch do not return. Use nominal enum variants and explicit runtime matching. Replace assumptions about unknown runtime literal selection with static type selection.

An ordinary return-only factory cannot gain multiple bodies merely by exposing a result type parameter. Canonical conversions retain their narrow exception. Canonical Err construction retains its separate arity/location contract.

The confirmed whole-vector rule fixes correlated-parameter ordering. Existing patterns must follow that rule. This approval does not establish that older examples already passed checking.

No released Rill compiler ABI exists. Source checking, evidence metadata and runtime callable representation still need implementation. This RFC does not adopt a native function-pointer ABI, dictionary ABI or runtime registry.

## Security/resource implications

User implementations must not intercept required guard release, owner counts or outstanding native-source retention. Trusted identities and overlap checks need enforcement beyond module spelling and private fields.

Requirements can form recursive or type-growing graphs. Candidate matching, overlap checks and monomorphization can use substantial memory and CPU. No constant-time checking or fixed expansion bound is promised.

An exhausted checker must not report successful evidence. A cached result must retain its graph, type and analysis context. Reusing another entry graph's candidates could select unauthorized or incompatible bodies.

Read-only callability does not prove that a function avoids I/O, mutation behind trusted synchronization or suspension. Consuming a callback does not prove borrow-freedom. Concurrency APIs must require their separate transfer and sharing evidence.

This draft does not prove memory safety, bounded stack use, rollback or target support. Runtime/backend, provider and ownership validation gates remain separate. A failed mechanism must return to design review rather than silently weaken the contract.

## Rationale, alternatives and prior art

Undeclared overloads reduce declaration text but hide the common signature and identity. The ri-skb decision instead requires one explicit family declaration. It also requires generic bodies to state their behavioral requirements.

Runtime type dispatch could handle unknown variants but would change the static overload model. Explicit `match` keeps runtime branching visible. Records of functions remain available for open runtime behavior with fixed checked signatures.

Constraint-based tie-breaking could choose between otherwise equal patterns. It would also turn evidence truth into overload priority and make duplicate rules depend on proof search. The approved sources exclude that behavior.

The confirmed whole-vector relation preserves parameter correlations and rejects crossed specificity. A per-position comparison that forgets repeated variables cannot distinguish equal-pair patterns from independent-pair patterns. Policy approval does not provide an implemented checker.

Named requirement groups could shorten repeated signatures. ri-tb9 (comment 6853754987) defers them beyond v0. This draft does not introduce traits through another spelling.

The [Rust conversion research](../research/0041-q19-rust-conversion-evidence.md) explains selected output evidence and specialization hazards. Its cited primary sources distinguish stable coherence from unstable specialization. Stable Rust rejects overlaps that this Rill design permits. Rust is not proof of this selection or evidence system.

## Implementation implications

The compiler needs canonical family records, normalized parameter vectors and graph-scoped body identities. Collection must separate duplicate checking, ordinary placement and protected-operation checking. Diagnostics must preserve which phase failed.

Selection needs a symbolic comparison over the complete type grammar. Do not approximate whole-vector specificity with a first-parameter score or candidate declaration order. Correlated variables and enum relations need explicit tests.

The checker needs staged callback inference and symbolic evidence records. A record must preserve selected input/output relationships, invocation access, properties and dependencies. It must not embed an overridable fallback's output as unconditional evidence.

Requirements must survive imports and exported generic signatures without injecting source bindings. Instantiation checks must agree with the already checked body. The missing formal evidence algorithm remains a blocker, not permission to check bodies only after specialization.

Lowering must preserve callable modes, move/copy decisions and capture cleanup. Runtime callable representation and private ABI remain separate proposals. The scaffold's compiler package does not implement any of these rules.

## Evidence and checking plan

### Actual evidence

| Artifact / revision | Evidence label | Rule/question exercised | Checker/tool and command/environment | Expected / observed outcome | Limits, negative/unavailable results and gate links |
| --- | --- | --- | --- | --- | --- |
| Source decisions and repository base `f33dcf4cf3f815824eeb897135775d7884f65c1a` | HandChecked | Current syntax and ordinary/conversion boundaries. | Drafting agent, 2026-11-16. Read current GitHub decisions, ADRs and catalogue sections cited above. | Later implement/enum/evidence refinements were identified and incorporated as source inputs. | This is source reconciliation, not proof of the proposed checking algorithm. |
| F1 through F20, this draft | NotChecked | Parsing, selection, generic requirements and callable ownership. | No Rill parser or checker is available. | Expected results are specified above. No compiler result is observed. | No example is RillExecuted. Literal inference and evidence algorithms remain incomplete. |
| Whole-vector specificity and symbolic evidence | NotChecked | Pattern comparison, correlation and sound instantiation. | No formal model or solver has been run. | No completeness, decidability or soundness result is claimed. | Requires adversarial specification/checker work before implementation readiness. |
| Existing prototype and research artifacts at source revision | NotChecked | Executable acceptance of the relevant Rill system. | Existing files were used as supporting input only. | No new reference or Rill execution was performed for this draft. | Historical reference experiments retain their own evidence scope. They do not execute this RFC. |
| RFC documents and index, local working draft | ToolChecked | Document structure, metadata, writing limits, links and preserved source artifacts. | Drafting agent, 2026-11-16. A temporary local Python script checked structure and prose limits. `mise run check:docs` checked links/whitespace. | Four register rows and matching RFC IDs passed. Local destinations resolved without errors. Accepted RFC 0001 rules and prototype/experiment files stayed unchanged. | These checks do not parse Rill, prove semantics or audit the formal ASD-STE100 dictionary. |

### Planned checks. Not executed evidence

- Parse standalone functions, declarations and implement bodies with explicit and inferred family arguments.
- Test fixed reference layers, result-pattern agreement and malformed arities.
- Test generic defaults after inference. Include explicit conflicts and independently missing numeric widths.
- Normalize aliases and renamed binders. Test repeated-variable identity and unused result-only binders.
- Reject ordinary duplicates during collection, including unreachable and uncalled bodies.
- Test generic/specific, correlated and crossed parameter patterns across every parameter position.
- Test variant, parent and mutable-variant access without runtime dispatch.
- Check selected requirements after ranking. Verify that missing evidence never selects a fallback.
- Check generic bodies before monomorphization with symbolic outputs and explicit assumptions.
- Test specialization-dependent outputs, recursive requirements and type-growing goals. Separate incomplete analysis from a semantic rejection.
- Test contextual overloaded names, typed arrows, borrowed Lists and direct fixed callable tuples.
- Test capture-mode weakening, exclusive capture duplication and borrowed consuming invocation.
- Test ordinary nested-type placement against conversion endpoint ownership.
- Reject protected replacement through aliases, adapters, generic requirements and function values.
- Preserve real caller and callee dependencies through returns, mutable storage, captures and cleanup.
- Test canonical diagnostics and their candidate facts in separate loaded entry graphs.

The future parser/checker suite must cover all supported source-analysis profiles. Native and WebAssembly execution additionally require the runtime/backend contract and real product tests. No target or client acceptance is claimed here.

## Open/deferred questions

### Review choices and remaining authoring work

The project owner confirmed all three policies (ri-48g, comment 6911486381). Whole-vector specificity, declaration-wide requirements and inference before defaults are no longer open policy choices. This does not accept the complete RFC or finish its checking rules.

The shared syntax annex records confirmed declaration positions, typed arrows, angle commitments and implementation-binder policies. Complete direct tuple/List inference and evidence procedures separately. No successful Rill parse follows from the declaration sketch or EBNF alone.

Specify a finite pattern comparison procedure over the full type relations. Complete symbolic evidence derivation for constructed generic types, specialization-dependent requirements and selected output equality. Preserve checked-body meaning across instantiation.

Complete cycle/resource handling without treating an unfinished proof as evidence. Define protected action admission and overlap checks with the ownership/runtime companion. Finish callable erasure, dependency and callee-exit rules there.

These are in-scope semantic and specification blockers for the [canonical checklist](README.md#canonical-reviewready-checklist). Missing companion text is not an accepted rule supplied by a link.

### Implementation/validation gates

Implement the parser, collection, inference, evidence, ownership and diagnostic phases. Validate adversarial positive and negative cases against actual Rill tooling. Runtime/backend support and full editor acceptance remain separate gates.

### Future/out-of-scope work

This RFC adds no traits, methods, public dyn, named requirement groups, variadic generics or implicit runtime overload dispatcher. It promises no future adoption of those features. General conversion semantics and standard-library APIs require their own proposals.

## Decision and change history

### Readiness record

No canonical readiness review has occurred. This document remains Draft. The drafting agent checked sentence/paragraph limits and applied unslop as the final prose pass. Structure and link checks passed. These checks cannot complete missing grammar, evidence rules or dependency review. Formal ASD-STE100 dictionary compliance remains unverified.

### Human decisions and amendments

On 2026-11-17, the owner requested the complete syntax grammar. The owner then confirmed all three syntax groups (ri-48g, comment 6931939690) at the two identified annex snapshots. Existing lexical/source and semantic approvals retain their exact scopes. Full annex/RFC acceptance, readiness and implementation remain separate.

The project owner requested this RFC on 2026-11-16. That request authorizes drafting, not acceptance. Source-ticket decisions remain provenance. The project owner later confirmed the three precision policies (ri-48g, comment 6911486381). No complete RFC revision has been accepted.

| Date / actor | Kind and scope of change | Reviewed revision or snapshot | Decision link / rationale |
| --- | --- | --- | --- |
| 2026-11-16, drafting agent | Created substantive Draft RFC 0004 and its register/coverage entries. | Source base `f33dcf4cf3f815824eeb897135775d7884f65c1a` and current decision comments cited above. This is not a reviewed RFC snapshot. | User request to write Functions, overloads and generic requirements. Full decision remains open. |
| 2026-11-16, project owner | Approved all three recommendations. Whole-vector specificity, inference before defaults and declaration-wide requirements are confirmed. | Three recommendations presented in chat. Pre-recording draft SHA-256: `53aeec8b419b9e2100e44203b6b97fea4ee34ac63201b75467e0608c5a86d29c`. No complete RFC revision was accepted. | Policy approval (ri-48g, comment 6911486381). Status remains Draft. Grammar, evidence rules, companions and readiness remain incomplete. |
