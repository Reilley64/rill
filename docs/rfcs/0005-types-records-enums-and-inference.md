# RFC 0005: Types, records, enums and inference

| Field | Value |
| --- | --- |
| ID | 0005 |
| Status | Draft |
| Owner | Reilley64. [#28](https://github.com/Reilley64/rill/issues/28) tracks this draft. |
| Source inputs | [#9 core decisions](https://github.com/Reilley64/rill/issues/9#issuecomment-6308790321), [#21 named records](https://github.com/Reilley64/rill/issues/21#issuecomment-6853593704) and [#19 enums](https://github.com/Reilley64/rill/issues/19#issuecomment-6870013901). Later refinements appear under [source reconciliation](#source-reconciliation). Repository inputs use commit `f33dcf4cf3f815824eeb897135775d7884f65c1a`. The [policy approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6912492481) confirms generic variant spelling, bracket inference and inline-layout checking. |
| RFC dependencies | [Draft RFC 0002](0002-modules-and-program-loading.md) proposes canonical modules, namespaces and privacy. [Draft RFC 0003](0003-expressions-bindings-and-control-flow.md) proposes expression contexts, binding scopes and control flow. [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md) proposes function inference and generic evidence. Their full texts are not accepted. Other [companions](#required-companion-rules) remain missing. |
| Replaces / superseded by | None. This draft does not amend an Accepted RFC or accept another draft. |
| Decision | Not yet decided |
| Reviewed revision | Not yet reviewed |

## Summary and scope

This RFC proposes type identity, nominal records, enums, structural tuples, construction and local type inference. It also proposes variant relationships, pattern typing and finite-layout checking.

The glossary defines [record, enum, enum variant, borrow and opaque value](../../GLOSSARY.md#language). A named record has its own identity. Matching fields do not make two records interchangeable. An enum owns a closed set of variants.

This draft preserves the approved named-construction and enum decisions. It does not restore anonymous records, primitive singleton types or general unions. It introduces no implicit numeric conversion, reflection or public dyn mechanism.

The project owner confirmed generic variant spelling, contextual bracket inference and the inline-layout policy. These are no longer open policy choices. This document remains Draft. Concrete syntax policies are confirmed. Pattern checking and missing companions still prevent readiness.

Numeric/text behavior and ownership require separate RFCs. A type relationship does not establish safe copying, concurrent sharing or source retention. No Rill compiler has checked or executed this draft's examples.

## Motivation

A programmer must know which declared type a value has. Type annotations must not assign record identity by field shape or discover an enum variant from runtime data.

Generic construction needs enough immediate information to determine type arguments. Empty constructors and bracket literals must not depend on later statements or desired overload results.

Recursive data also needs a finite representation. Checking names alone can reject valid nested wrappers or accept an infinite by-value layout. The specification must distinguish stored values from type arguments that do not occupy inline storage.

## Guide-level explanation

Case T1 is a complete module:

```rill
type Point = { x: i32; y: i32 };
type ScreenPoint = { x: i32; y: i32 };

function origin(): Point {
  return Point { x: 0i32, y: 0i32 };
}

expect origin().x == 0i32;
```

Point and ScreenPoint remain different types. `Point { ... }` supplies identity at construction. An annotation checks that identity rather than inventing it. A bare `{ x: 0i32, y: 0i32 }` is not another construction form.

Case T2 is a separate complete module:

```rill
enum Language {
  English,
  French { greeting: string },
  German(string),
}

function greeting(language: &Language): string {
  return match (language) {
    Language.English => "Hello",
    Language.French { greeting } => copy(greeting),
    Language.German(text) => copy(text),
  };
}

expect greeting(&Language.French { greeting: "Bonjour" }) == "Bonjour";
```

The borrowed match does not move strings out of language. The explicit copies return owned strings. Canonical copy is in the fixed prelude. These are intended results, not observed execution.

Case T3 illustrates the confirmed generic spelling:

```rill
enum Packet<T> { Data(T), Empty }

function emptyPacket(): Packet<i32> {
  return Packet<i32>.Empty;
}
function payload(packet: &Packet<i32>): i32 {
  return match (packet) {
    Packet.Data(value) => *value,
    Packet.Empty => 0i32,
  };
}

expect payload(&Packet<i32>.Data(4i32)) == 4i32;
expect payload(&emptyPacket()) == 0i32;
```

This is a complete proposed module. Apply generic arguments to the parent enum before selecting its variant. Pattern context can supply those arguments without repeating them. The owner confirmed this spelling. No Rill compiler has checked this module.

## Syntax and reference semantics

The owner [confirmed the three syntax policy groups](https://github.com/Reilley64/rill/issues/28#issuecomment-6931939690) in [annex version 1](../grammar/v0-syntax-v1.md). Its [EBNF](../grammar/v0-syntax-v1.ebnf) and commitment/context rules form one policy review unit. Full annex/RFC acceptance and static-semantics completion remain separate.

### Type forms and identity

The following grammar is a sketch. It does not complete lexical tokens, precedence or generic-argument parsing.

```text
record = [export] type name [generics] = {fields} [;]
field = [private] name : type
sum = [export] enum name [generics] {variants}
variant = name | name (types) | name {public-fields}
type = primitive | parameter | nominal-application | variant-type
     | & type | &mut type | tuple-type | callable-type
tuple-type = [type, ...]
variant-type = enum-application . variant-name
```

The ellipsis above is grammar notation for a finite sequence. It does not add source-level variadic generics. The shared syntax decision confirms separators, finite tuple arities and empty enum admission. This separate approval does not accept the full type system.

Primitive names include sized integers, f32/f64, bool, char, string, void and never. Numeric widths and UTF-8 behavior belong to the numeric/text companion. This draft introduces no int, float, byte, isize or usize aliases.

Void has one implicit value and no written literal. Never has no values and describes a path that cannot return normally. A diverging branch supplies no ordinary value to a type join. Complete divergence analysis belongs to RFC 0003.

A nominal application has its canonical declaration identity and ordered type arguments. Imported aliases preserve that identity. Distinct declarations remain different even when their fields, names or representations match.

A variant type has its parent enum application and canonical variant identity. Variants of different enums never become the same type through matching payloads. A variant's payload values do not create further source types.

Tuples have structural identity from their ordered element types and arity. They are not records, Lists or enum payload constructors. Callable identity and invocation-mode relationships follow [RFC 0004](0004-functions-overloads-and-generic-requirements.md#callable-modes-and-requirements).

Unconstrained generic parameters denote distinct symbolic types. They do not promise fields, constructors, copyability or borrow-freedom. Required behavioral and property evidence follows [RFC 0004](0004-functions-overloads-and-generic-requirements.md#requirements-and-symbolic-evidence).

This RFC introduces no type-alias declaration feature. Import aliases remain module bindings. Named record declarations are not structural aliases.

### Proposed M4.1 canonical declaration-type procedure

The owner [approved this scoped M4.1 procedure](https://github.com/Reilley64/rill/issues/56#issuecomment-6943516603) at checkpoint `71cbe3707814114b0f504f0458ada41d54c2593d`. [Slice #57](https://github.com/Reilley64/rill/issues/57) implements explicit declaration types under M4 umbrella #56. Approval does not complete inference, signature validity, layout or this Draft RFC.

The input is one successful M3 Graph and explicit analysis limits. Use that graph's retained M1 snapshots and M2 nodes. Do not read live files. Do not parse again. Do not combine captures.

The stage elaborates module-level record fields, enum payloads and explicit named-function or family parameter/result annotations. It also inventories their type binders. Bodies, startup statements, expectations and declarations inside bodies remain NotPerformed. Their presence does not establish their validity.

#### Environments and source lookup

Collect module-level nominal declarations and ordered binder identities before elaborating annotations. Use the original M3 DeclarationId for each declaration and binder. Reject duplicate binder names within one declaration. Preserve distinct binders from different declarations, even when their names match.

Resolve an unqualified type name through the declaration's binders, then its module's installed type/module bindings, then M3's primitive prelude identities. The scoped checkpoint approval covers this precedence. A source binding can shadow a primitive spelling. Compiler-required primitive references still use their actual prelude identities.

Resolve qualified module segments through installed bindings and exported qualification. Never probe filesystem children. A module target cannot finish a type annotation. A value/function target cannot supply a type.

A parent-applied variant type resolves the enum first. Collect every same-spelled variant in that enum, deduplicated only by DeclarationId. One candidate supplies the variant identity. No candidate rejects as a missing variant. Multiple candidates reject as an ambiguous annotation. Retain each candidate's identity and declaration span. Never select by traversal order.

Preserve the parent application and selected variant DeclarationId. Reject variant selection on a record. Aliases preserve the canonical declaration and defining module. Unused duplicate variant declarations remain outside this slice's declaration-validity checks.

Compiler access to a private signature's metadata creates no source binding. Source-written annotations still need accessible names. An exported operation can mention its own private nominal types under RFC 0002. This stage does not decide representation access or exported-payload admissibility.

#### Canonical type terms

Elaborate the following finite terms without expanding nominal fields:

| Form | Canonical information |
| --- | --- |
| Primitive | Actual primitive identity from this graph's prelude. |
| Parameter | Its original binder DeclarationId and ordered position. |
| Nominal application | Record/enum DeclarationId and ordered argument terms. |
| Variant | Parent enum application and original variant DeclarationId. |
| Tuple | Ordered element terms, including zero and one elements. |
| Reference | Read-only or mutable constructor and referent term. |
| Callable | Written invocation mode, ordered parameter terms and result term. |

Grouping changes no type identity. Equal nominal applications require equal declaration identities and equal argument terms. Tuple equality preserves order and arity. References preserve their access constructor. Callable equality preserves mode, parameter order and result. This exact equality procedure performs no widening, mode weakening, inference or overload comparison.

Require full written arguments for generic applications in this stage. Excess arguments reject as malformed application. Missing arguments produce Incomplete with a needs-inference reason, not a guessed application or new language prohibition.

Generic defaults and where clauses produce Incomplete in this slice. Implementation headers also produce Incomplete until their signature-substitution procedure exists. Retain their original nodes and spans as unsupported obligations. Do not classify these valid syntax forms as type errors.

Void and never remain distinct primitive terms. Elaborating them does not approve their use as stored fields, parameters or inhabited values. Those position-validity judgments remain NotPerformed. Callable terms likewise prove no capture or invocation permission.

For each application, retain the ordered substitution from the declaration's binders to argument terms. Keep written-use provenance separate from canonical type identity. Two written aliases can denote one term while retaining different use spans and import routes.

#### Recursion, completion and provenance

A nominal reference is a finite term, not a request to expand its definition. Direct, mutual and type-growing field references therefore do not start recursive type expansion. This stage neither proves nor rejects finite layout. The existing layout policy retains that responsibility.

Every elaborated use retains its module, syntax-node identity, original Span and canonical target or primitive identity. Retain declaration and binder spans too. Keep M3 import-route provenance available through the originating binding. No fabricated type or internal error placeholder can appear as a valid term.

Return a declaration-type receipt only when every in-scope annotation is elaborated without findings or unsupported obligations. The receipt identifies this stage and graph capture. It records signature compatibility, layout, bodies, inference, coverage, evidence, ownership and execution as NotPerformed.

A receipt is not a checked program or callable implementation. Failed attempts can expose explicitly partial facts, but no completed receipt. Distinguish source type errors, unsupported/inference obligations and resource exhaustion. Preserve known findings when later work becomes incomplete.

Order facts and findings by canonical module order, declaration position and written-use position. Fresh capture identities remain unequal. Determinism compares semantic records, not opaque allocation identities.

Charge annotation visits, lookup steps, type terms, substitution entries and retained provenance before allocation or enqueueing. Bound traversal depth and retained terms as well as total work. Check counters for overflow. Exhaustion returns ResourceLimit, never a missing-name proof or source rejection. Limits do not guarantee wall-clock time or host allocation success.

### Record declarations and construction

A record declaration fixes its field names and types. Fields are public unless marked private. Binding mutability controls permitted mutation; there are no per-field mutable declarations.

Construct a record with its declared type name and named fields. Supply every field exactly once. Reject unknown, duplicate or omitted fields. There is no implicit default value for an omitted field.

Check each value against its declared field type after generic substitution. A constructor may infer type arguments from its fields and immediate expected type. It must not search for another record with matching fields.

A concrete expected type can constrain generic arguments of the named constructor. It cannot turn ScreenPoint construction into Point construction. Numeric field context may supply a literal width, but not convert an already typed incompatible number.

Evaluate field expressions in written order under [RFC 0003](0003-expressions-bindings-and-control-flow.md#evaluation-order). Declaration order does not reorder source effects. This RFC does not fix a target field-layout order or ABI.

Empty named records remain distinct nominal types. Their construction is still explicit, as in `Marker {}`. This is not a written void value or an anonymous record.

### Named rebuilding

The proposed rebuilding form is `RecordType { ...base, field: replacement }`. One leading base supplies omitted fields. Explicit replacement fields must be distinct and declared in the same record type.

The base must have that exact record application. No field-shape conversion or merging of unrelated records occurs. This draft introduces no multiple-spread construction feature.

Evaluate the base once, then replacement expressions in written order. Preserve access to original fields needed by those expressions. Do not move or dispose an original field early merely because its spread appears first.

The current source includes `Circle { ...circle, radius: circle.radius * by }`. Its read of circle remains meaningful while computing the replacement. Rebuilding must preserve the approved same-type owned reuse behavior.

Moves, permitted implicit copies, disposal of replaced fields and cancellation-safe installation need the ownership companion. This RFC does not authorize hidden cloning or new aliasing. A live borrow cannot become valid merely because a rebuild reuses storage.

Whole-record construction and rebuilding respect [RFC 0002's privacy rules](0002-modules-and-program-loading.md#privacy-and-opaque-values). An existing value grants no extra access to private fields.

<a id="enums-and-proposed-generic-variant-spelling"></a>

### Enums and generic variant spelling

An enum declares unit, tuple-payload or record-payload variants. It owns their complete closed set. There are no user-specified integer discriminants, backing-value conversions or automatic membership for unrelated records.

A reusable record must appear inside an explicit variant payload. If Shape declares Circle(Circle), a Circle value is not already a Shape. Construct Shape.Circle(value).

Use the parent application before the variant name when writing generic arguments:

```rill
Packet<i32>.Empty
Packet<i32>.Data(3i32)
```

The same spelling supplies a type, as in `Packet<i32>.Data`. A variant does not introduce another generic argument list. `Packet.Data<i32>` is not a valid alternative spelling.

When arguments are omitted, use payload types and immediate expected context. Apply trailing defaults under [RFC 0004's confirmed inference/default order](0004-functions-overloads-and-generic-requirements.md#generic-arguments-inference-and-defaults). Do not infer missing arguments from later uses or the available implementation count.

A payload-free generic variant needs context, explicit arguments or applicable declared defaults. Bare Packet.Empty cannot establish T in an unconstrained initializer. Generic variant patterns obtain their parent arguments from the scrutinee when known.

Exported enum visibility exposes every variant and payload field under RFC 0002. There are no individually private variants or payload fields. A private enum can remain opaque through exported operations.

This enum sketch does not define the standard Result representation. In particular, Result.Err contains the standard error wrapper, not merely an arbitrary raw E. Complete Option/Result construction and metadata require the error/library companions.

### Permitted type relationships and joins

An exact type matches itself. An owned variant can widen to its own parent enum application. A read-only variant borrow can widen to a read-only borrow of that same parent application.

An explicit parent annotation establishes the parent's static type. Function boundaries use declared parameter and result types. Optimizer knowledge or a runtime discriminator cannot restore a narrower variant type there.

Do not widen `&mut Variant` to `&mut Parent`. Parent replacement could invalidate the caller's variant assumption. Store the value as the parent before borrowing mutably when replacement may change variants.

There is no corresponding automatic relationship between List<Variant> and List<Parent>. Do not infer covariance through arbitrary generic containers, tuples or callable signatures. Constructing a contextually typed container is distinct from converting an existing container.

For example, a fresh List<Language> literal may contain different Language variants. An existing List<Language.French> does not thereby become List<Language>.

For unannotated expression joins, identical types keep their type. Different variants of one enum application join at that parent application. A variant and its own parent also join at the parent.

Different generic arguments do not join into an invented partial application. Packet<i32>.Data and Packet<bool>.Empty do not produce an implicit Packet with unknown arguments. Unrelated records or enums do not produce a structural record or union.

An immediate expected type may check each branch using its permitted relationship. A numeric annotation does not convert an already typed value from another width. Callable mode weakening and dependency merging retain their own companion rules.

No primitive literal becomes a singleton source type. `let text = "fr"` has type string. `let count = 1i32` has type i32. Constant information does not rank overloads or expand type relationships.

<a id="proposed-bracket-literal-inference"></a>

### Bracket-literal inference

List and tuple values share bracket spelling but remain different types. Resolve their construction category before treating one as the other's argument type.

A concrete immediate tuple context fixes arity and per-position types. A concrete List<T> context fixes the element type. Check each element in that context without changing the category afterward.

For a family call with incomplete context, inspect independently viable input patterns before result preferences or requirement truth. Use known source arity and independently typed arguments. Do not rank bodies using the desired return.

A unique required bracket category can supply context. For tuples, viable contexts must also agree on arity and needed element context. An unconstrained type parameter alone supplies neither category.

Conflicting viable List and tuple contexts require an annotation. Conflicting element contexts also require an annotation. Do not try one category, discard an inconvenient result and silently retry another.

A fixed two-job join input can supply a two-position tuple category from its input pattern. Callback argument/result evidence then follows RFC 0004's staged checking. This preserves direct callable-tuple examples without a variadic feature.

Without category context, the confirmed default is List construction. Do not use this default to resolve conflicting input contexts. Infer its element type from a unique permitted join of established element types. A heterogeneous sequence without such a join needs an explicit tuple or other container annotation.

An empty uncontextualized bracket literal cannot determine the List element type. Untyped numeric elements do not acquire a default width. An explicit annotation can resolve either problem when it supplies the missing information.

Nested literals repeat the same process with their own established contexts. This policy still needs complete candidate/context inference rules with RFC 0004. It is not a working solver or permission for return-only dispatch.

### Pattern typing and coverage

Patterns inspect a value whose static type is already established. They do not construct anonymous records or search declarations by matching field names.

A name binds the matched value under the applicable access mode. A wildcard ignores that position under ordinary cleanup rules. Tuple patterns require known tuple arity. They do not destructure a List by implicit conversion.

Record patterns can name accessible fields and omit other fields. Nested brace patterns inspect known nominal payloads. They do not grant access to private fields or reveal a private opaque representation.

Qualified enum patterns establish the matched variant within their arm. A whole-enum scrutinee needs coverage of every possible variant. A statically known variant needs coverage only of its possible payloads.

Primitive literal patterns remain value tests. Removing literal types does not remove these patterns. Numeric/text equality and literal validity need their companion rules.

An arm with a guard does not establish complete coverage by itself. Later arms must cover values for which its guard fails. Matching follows the source arm order and does not select overload bodies by runtime value.

Use constructor coverage for enums, tuples and records. Nested refutable payload patterns must not count as complete coverage of their parent variant. A wildcard or binding can cover the remaining permitted values.

The proposed checking rule requires an irrefutable pattern for an ordinary let binding. A variant pattern can be irrefutable when the initializer already has that variant's static type. General refutable matching belongs in match or the established guard forms.

Complete coverage, guard-binding and move/borrow judgments remain authoring work. In particular, a failing guard must not leave later arms with destroyed payload state. This draft does not claim that coverage alone proves ownership safety.

<a id="proposed-finite-inline-layout-checking"></a>

### Finite inline-layout checking

Every value type needs a finite representation. A direct or mutual by-value cycle cannot obtain one merely because an enum also has a unit variant.

A reference provides indirection, so its referent does not occupy that reference's inline storage. Trusted containers may also supply an actual indirect representation. Their canonical implementation metadata must justify that boundary.

A same-spelled user type is not automatically an indirect List or Shared. Private fields, generic spelling and property predicates do not supply an indirection guarantee.

Use two linked analyses over nominal declarations:

1. Compute which generic parameters each declaration can store inline.
2. Build the resulting nominal by-value containment graph.

An inline-parameter summary lists which type parameters can occupy inline storage. Compute these summaries to a fixed point. A stored parameter contributes its own index. A tuple contributes its stored element types. Enum summaries include every variant payload.

For a stored nominal application, inspect its declaration and the arguments its inline-parameter summary uses. A reference stops inline traversal. Callable parameter/result signatures are not captured storage; actual capture layouts still need checking.

When analyzing a declaration, add edges to nominal types reached by this traversal. Keep that declaration as the graph origin while traversing forwarded arguments. Do not create a declaration self-cycle solely from a finite nested application in its arguments.

Reject a cycle in the resulting inline-containment graph, R0406. The policy also covers recursion that grows type arguments on every expansion. Do not require exact repeated closed type arguments before detecting that cycle.

A wrapper that stores T inline forwards layout dependence on T. A wrapper with no stored T does not. This distinction permits nested wrappers and unused generic arguments without accepting infinitely stored values.

This is a confirmed representation-validity policy, not a target ABI. It needs formal checking against all type constructors and trusted boundaries. Field offsets, tags, niches and calling conventions remain runtime/backend work.

A finite layout can still exceed a target's representation limits. Such target checks must remain distinct from a recursive-layout error. Finite layout does not prove that a value can be constructed or safely borrowed.

### Proposed M4.2 declaration validity and inline containment

The owner [approved this M4.2 procedure and D1 through D4](https://github.com/Reilley64/rill/issues/59#issuecomment-6945089123) at checkpoint `c26d737439fd4c426d362655ad15984cc70108dc`. [Ticket #59](https://github.com/Reilley64/rill/issues/59) records implementation and acceptance. Earlier approvals remain confirmed. This scoped decision does not accept the full RFC.

The input is a completed M4.1 receipt, explicit canonical prelude signature contracts and checking limits. Reuse its graph, declaration identities, canonical terms, substitutions and original spans. Do not accept a failed M4.1 attempt as a completed receipt. Do not read live files.

#### Declaration checks and type positions

Check every module-level record and enum in the receipt, including unused declarations. Reject repeated field names within one record or record-payload variant. Reject repeated variant names within one enum. Keep distinct records, variants and parameter lists as separate member scopes.

Retain all conflicting member identities and declaration spans. A private field still participates in its record's uniqueness check. Apply [RFC 0004's header and prelude checks](0004-functions-overloads-and-generic-requirements.md#proposed-m42-header-validity-and-prelude-signatures) to function and family headers.

The proposed position policy permits void and never wherever an existing type production admits a type. Void has its implicit value; never remains uninhabited. This policy supplies no initializer, constructor or normal continuation. The scoped checkpoint approval covers it.

Do not remove stored fields from containment merely because another field is uninhabited. A by-value cycle still rejects. Empty enums and never do not excuse such a cycle.

Public-interface visibility, constructor field sets, implementation signatures and body validity remain separate checks. This stage does not mark them complete. Existing source-name access checks remain with M4.1.

#### Layout inputs and unsupported storage

Analyze the written fields and every enum payload. Module-level function parameter/result annotations are not stored fields of a nominal declaration. Reference referents do not occupy the reference's inline storage.

Use actual canonical primitive identities as finite leaves. This classification fixes no target size, alignment, tag or ABI. Generic parameters remain symbolic storage dependencies on their ordered binder positions.

This slice accepts no supplied nominal indirection exemptions. Inspect the actual source fields of every record and enum, including standard-origin declarations. No spelling, private field or public metadata flag can make List, Shared or another nominal declaration indirect.

Later genuine opaque/native containers require a separately approved admission procedure and actual implementation metadata. Their omission here is not evidence that a same-spelled source wrapper has their representation.

A callable's parameter/result signature is not its capture layout. A directly stored callable without an established capture-representation contract makes layout incomplete. Do not treat that unknown storage as an empty summary or finite leaf. Stop at references before inspecting their referents.

A callable term inside an unused nominal argument does not occupy storage merely because the argument is written. Inspect only arguments selected by the nominal summary. Actual capture layout and trusted representation admission remain later obligations.

#### Inline-parameter summaries

For each record or enum D, create a summary S[D] over D's ordered binder indices. Start with empty known sets. Also track whether storage analysis is complete. Every enum contributes all of its variant payloads.

For a stored term t in D's fields, compute these contributions:

```text
primitive                 contributes no binder index
parameter D.i             contributes i
reference                 stops traversal
structural tuple          unions its element contributions
nominal N<a0, ..., an>     unions contributions of ai for each i in S[N]
variant E<args>.V          uses the proposed parent-enum rule below
stored callable           marks analysis incomplete; its signature contributes no capture facts
```

The variant notation above describes a canonical term, not new source syntax. For a nominal application, inspect argument subterms without expanding N's field definitions. Preserve D as the originating binder environment.

Scan declarations in canonical order until no known set or completeness state changes. Union known indices monotonically. Propagate incomplete storage through every stored nominal application whose declaration has incomplete storage. Do not propagate it through a stopped reference or unused argument.

Every binder fact needs a finite derivation from written storage or an earlier justified summary fact. A cyclic pending fact cannot justify itself. Empty initial sets implement the least fixed point, not assumed recursive evidence.

There are finitely many declarations and binder indices. A complete analysis can add at most their total binder-index count. Incompleteness can propagate to each declaration once. Limits still bound all scans and retained facts.

#### Proposed stored-variant rule

The approved rule treats a stored variant type as its parent enum application for inline representation. Use the parent's complete inline-parameter summary and containment dependencies. Variant identity remains distinct for typing and overload selection.

This is a new representation-validity choice approved at the M4.2 checkpoint. The earlier parent-enum approval did not itself decide variant-type storage. A selected-payload representation was an alternative, not an equivalent implementation.

For example, consider this independent declaration excerpt:

```rill
enum Choice<T> { Empty, Value(T) }
type Node = { tag: Choice<Node>.Empty };
```

The approved rule rejects Node's inline cycle, R0406. A selected-payload representation could instead make this Empty field finite. This case was NotChecked at checkpoint creation. The [M4.2 acceptance record](../../crates/compiler/tests/m4-2-acceptance.md) records later checking.

#### Containment graph and cycle witnesses

After summary closure, create one vertex per canonical record/enum declaration. Traverse each declaration's stored terms with that declaration as origin O:

```text
primitive or parameter    adds no nominal edge
reference                 stops traversal
structural tuple          visits each stored element
nominal N<args>            adds O -> N, then visits arguments selected by S[N]
variant E<args>.V          follows the approved stored-variant rule
stored callable           preserves an incomplete layout obligation
```

Keep O unchanged when traversing forwarded arguments. Do not change the origin to the wrapper declaration. Repeated visits to Box in finite Box<Box<i32>> therefore create no artificial Box self-cycle.

Deduplicate edges by canonical origin and target. Keep the least witness by storage-member identity, type-use position and forwarding binder-index sequence. Retain its relevant substitutions. Original snapshots and declaration/member spans remain attached. Do not enumerate unbounded cyclic route histories.

Find strongly connected components with an iterative algorithm. A self-edge or component with several vertices proves an inline cycle. Report each cyclic component once, ordered by its least canonical declaration identity. Use R0406.

Within a cyclic component, traverse from its least vertex with an explicit depth-first stack and canonically ordered edges. Retain the first back-edge cycle as its witness. Do not wait for identical closed type arguments to recur.

Retain known cycle findings when other work becomes incomplete. No incomplete summary, unresolved storage obligation or exhausted budget can justify a completed finite-layout result. A finite graph result is not a target size, constructibility or ownership proof.

#### Outcomes, limits and procedure validation

A completed result covers member/header uniqueness, the supplied required prelude signatures, approved type-position policy and source-nominal inline containment. It does not certify full declaration validity, general signature compatibility, capture layouts, public interfaces or bodies.

Reject definite declaration/signature errors and proved cycles. Return Incomplete for unsupported storage. Return InvalidRequest for inconsistent contract inputs or cross-capture identities. Return ResourceLimit when a checked budget prevents completion. Preserve known findings in explicitly partial results.

Validate request identities and contracts first. After that validation, ResourceLimit takes priority over Incomplete, which takes priority over source rejection. This matches M4.1's failure boundary. A known rejection finding remains visible when incomplete work prevents the rest of the requested checks.

Charge member comparisons, term visits, fixed-point scans, summary facts, completeness propagation, edges, substitutions and witnesses before growth. Charge iterative graph work too. Counters must not wrap. Work, retained-entry and traversal limits are implementation budgets, not valid-program restrictions.

Before compiler implementation, validate a separate finite reference model against independently stated expected summaries and containment graphs. Include T13 through T16, mutual cycles, forwarding, unused arguments and type growth. Include the approved stored-variant case and callable/reference boundaries. Record actual outcomes and limitations. Model execution is not compiler or Rill execution.

### Proposed M4.3 never admission

[M4.3 ticket #60](https://github.com/Reilley64/rill/issues/60) records this precision rule. The owner [approved C1 through C4](https://github.com/Reilley64/rill/issues/60#issuecomment-6947086957) at checkpoint e4750b8. Earlier policies and full RFC status remain unchanged.

An expression whose canonical static type is primitive never can satisfy an immediate expected value type without conversion. It produces no normal value. Preserve never as its static type rather than inventing a value of the expected type.

This rule does not establish a numeric width or infer a generic argument from never alone. Unrelated unknown operand types still need context. Apply the existing join rule by omitting never branches from ordinary value joins. Two never branches retain never.

Keep static type and normal-continuation facts separate. A call or operator retains its declared result type even when an earlier operand cannot return. Do not retag that result as never to repair another type mismatch.

For example, assume f(i32):i32 and g(bool):void. The expression g(f(stop())) still has an i32-to-bool mismatch when stop():never. Normal execution cannot reach g, but the declared intermediate type does not change. This is a proposed checking case, NotChecked.

This proposal adds no general container variance or implicit width conversion. Existing variant admission retains its owning rules. An unimplemented permitted relationship must remain incomplete, not become a fabricated mismatch or successful check.

### Required companion rules

| Rule owner | Required contribution | Current dependency state |
| --- | --- | --- |
| [Draft RFC 0002](0002-modules-and-program-loading.md#privacy-and-opaque-values) | Canonical declaration scopes, namespaces, privacy and opaque values. | Identity/order policies are confirmed. Complete resolver rules remain incomplete. |
| [Draft RFC 0003](0003-expressions-bindings-and-control-flow.md#matching-and-patterns) | Expression grammar, arm order, binding scopes, evaluation and control-flow joins. | Its four policy groups are confirmed. Full grammar and flow judgments remain incomplete. |
| [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md#requirements-and-symbolic-evidence) | Function inference, static overloads, output evidence and callable modes. | Its three precision policies are confirmed. Symbolic checking remains incomplete. |
| [Draft RFC 0006](0006-numbers-strings-and-primitive-operations.md), with complete grammar still missing. | Literal tokens, numeric widths, primitive operations and string views. | Proposed, not accepted. Exact type/lexical grammar and logical-place ownership remain incomplete. |
| [Draft RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md) | Borrowed patterns, moves, rebuilding, capture layouts, retained dependencies and disposal. | Proposed, not accepted. Complete pattern/ownership judgments remain incomplete. Finite layout is not an ownership proof. |
| [Draft RFC 0008](0008-errors-propagation-and-caused-by-wrapping.md), with collection contracts still missing. | Actual Option/Result/Err constructors, causes and canonical container contracts. | Errors are proposed, not accepted. Complete constructor/variant judgments and collection contracts remain incomplete. Local Packet examples do not replace standard types. |
| [Draft RFC 0010](0010-diagnostics-and-reporting.md), with runtime/backend still missing. | Exact failures, target layout limits, private representation and native/Wasm lowering. | Reporting is proposed, not accepted. Neither complete source/driver procedures, compiler execution nor target support is established. |

## Valid/invalid examples and diagnostics

T1 through T3 appear in the guide. The following cases are independent unless they name an earlier context. All type and runtime outcomes are expectations, not compiler observations.

Case T4 extends T1 with rejected excerpts:

```rill
let wrong: Point = ScreenPoint { x: 1i32, y: 2i32 };
let anonymous: Point = { x: 1i32, y: 2i32 };
```

The first has incompatible nominal identity, R0100. The second uses removed anonymous construction, R0001. Expected Point context does not repair either expression.

Case T5 is another rejected T1 excerpt:

```rill
let missing = Point { x: 1i32 };
let duplicate = Point { x: 1i32, x: 2i32, y: 3i32 };
```

Both violate the declared field set. Exact diagnostic allocation remains companion work. Do not invent an omitted zero field or use the last duplicate value.

Case T6 is a complete module:

```rill
type Box<T> = { value: T };
function boxed(): Box<i32> {
  return Box { value: 7i32 };
}
expect boxed().value == 7i32;
```

The named constructor and immediate result context agree on Box<i32>. No field-shape search or numeric default is needed.

Case T7 is a constructor excerpt using T3's Packet. `let empty = Packet.Empty;` lacks T and requires context, R0301. `let empty: Packet<i32> = Packet.Empty;` has sufficient immediate context. Later uses cannot repair the first declaration.

Case T8 is a complete module:

```rill
enum Signal { Low, High, Unknown }
function choose(flag: bool): Signal {
  return if (flag) Signal.Low else Signal.High;
}
expect match (choose(true)) {
  Signal.Low => true,
  Signal.High => false,
  Signal.Unknown => false,
};
```

Distinct variants join at Signal, not a two-variant subset union. The declared function result also establishes Signal at its boundary.

Case T9 describes a rejected access relationship. Assume a function needs `&mut Signal` so it can replace the value. An existing `&mut Signal.Low` cannot satisfy that parameter, R0100. Moving a Low value into a Signal binding before borrowing mutably is a different operation.

Case T10 is a literal-context excerpt:

```rill
import std::collection::List;
let pair: [i32, bool] = [1i32, true];
let values: List<i32> = [1i32, 2i32];
let inferred = [1i32, 2i32];
```

Under the confirmed policy, pair is a tuple. The other two are Lists. An unannotated `[1i32, true]` needs tuple context because its elements have no common List type.

Case T11 describes contextual call boundaries. The canonical two-job join pattern can supply tuple context for `[() => 1i32, () => true]`. Competing viable tuple and List input shapes require an annotation. Desired return types cannot select one category. Full callable evidence still needs RFC 0004.

Case T12 is a rejected complete module:

```rill
enum Switch { Off, On }
function enabled(value: Switch): bool {
  return match (value) {
    Switch.On if true => true,
    Switch.Off => false,
  };
}
```

The guarded arm does not establish full On coverage. Add an unguarded On arm or an appropriate wildcard. Exact coverage diagnostics remain companion work.

Case T13 is a rejected layout declaration:

```rill
enum Chain { End, Link(Chain) }
```

The Link payload creates an inline cycle, R0406. End does not make that representation finite. Uncalled functions or unused declarations must not hide the invalid loaded type.

Case T14 is an indirect-layout declaration excerpt:

```rill
import std::collection::List;
type Tree = { children: List<Tree> };
```

This can have finite inline representation only through the genuine container's established indirect representation. It does not create a recursive-layout exemption for any type named List. Construction, disposal and bounded-stack traversal remain separate obligations.

Case T15 is a finite-layout boundary under the confirmed policy:

```rill
type Box<T> = { value: T };
type Phantom<T> = { marker: i32 };
type Nested = { value: Box<Box<i32>> };
type Node = { marker: Phantom<Node> };
```

Nested stores a finite number of Box values. Phantom does not store its type argument. Neither case creates an inline cycle under the confirmed layout policy.

Case T16 is a rejected type-growing layout:

```rill
import std::collection::List;
type Growing<T> = { next: Growing<List<T>> };
```

Changing arguments do not stop the stored Growing constructor from recurring, R0406. The layout analysis must not expand forever waiting for an identical closed application.

Case T17 describes privacy. Assume another module exports a record with a private field and a factory. A client can hold the returned value and use allowed public fields. It cannot reconstruct the whole record or bind the private field, R0401. A private opaque type cannot be destructured at all.

Case T18 describes borrowed dependencies. A record or enum payload can contain `&mut T`. Storing it in a nominal wrapper does not make the wrapper copyable, borrow-free or safely shareable. Passing layout checking does not remove the original source dependency.

Case T19 illustrates named rebuilding in a complete module:

```rill
type Point = { x: i32; y: i32 };
function shifted(point: Point): Point {
  return Point { ...point, x: point.x + 1i32 };
}
expect shifted(Point { x: 2i32, y: 3i32 }).x == 3i32;
```

The old x remains readable while computing the replacement. Checked arithmetic can still panic on overflow. Non-copying fields require the ownership companion's full rebuilding rules.

| Case / artifact and revision | Context | Expected outcome | Owning rule | Evidence label / observed result |
| --- | --- | --- | --- | --- |
| T1/T4/T5/T6, this draft | Named records and independent invalid excerpts. | Preserve identity and field sets. Infer Box<i32> without structural conversion. | [Records](#record-declarations-and-construction), [identity](#type-forms-and-identity). | NotChecked. No parser or type checker. |
| T2/T3/T7, this draft | Complete enum modules and missing-context alternatives. | Preserve payload access. Confirmed parent-applied spelling fixes generic variants. Missing context rejects, R0301. | [Enums](#enums-and-generic-variant-spelling). | NotChecked. The spelling is confirmed; no compiler result is observed. |
| T8/T9, this draft | Variant joins and a mutable-access boundary. | Join at the parent. Reject mutable variant-to-parent widening, R0100. | [Relationships](#permitted-type-relationships-and-joins). | NotChecked. No join/access checker. |
| T10/T11, this draft | Concrete literals and contextual call scenarios. | Use declared categories or the confirmed List default. Ambiguous input shapes need annotation. | [Bracket inference](#bracket-literal-inference). | NotChecked. The policy is confirmed; the complete contextual solver is absent. |
| T12, this draft | Invalid guarded coverage module. | Reject uncovered On values despite the guard expression. | [Patterns](#pattern-typing-and-coverage). | NotChecked. No coverage checker. |
| T13/T14/T15/T16, this draft | Direct, indirect, nested, phantom and type-growing declarations. | Reject inline cycles. Permit finite cases only under the stated representation conditions. | [Layout](#proposed-finite-inline-layout-checking). | NotChecked. No formal model, layout solver or target validation. |
| T17/T18, this draft | Explicit module and borrowed-payload scenarios. | Preserve privacy and ownership dependencies independently of layout. | [Records](#record-declarations-and-construction), [companions](#required-companion-rules). | NotChecked. Ownership and privacy judgments remain incomplete. |
| T19, this draft | Complete scalar rebuilding module. | Read the old field before replacement. Preserve checked overflow behavior. | [Rebuilding](#named-rebuilding). | NotChecked. No Rill execution or full rebuilding checker. |

The [diagnostic registry](../diagnostic-codes.md) fixes the cited code meanings. It does not supply exact messages for these examples. R0406 covers impossible recursive layouts. Field-set and coverage failures still need their diagnostic allocation. All these failures need actual diagnostic implementation.

## Feature interactions

### Modules, functions and expressions

[RFC 0002](0002-modules-and-program-loading.md#privacy-and-opaque-values) owns source access, visibility and canonical origins. This RFC supplies type identity and representation questions after resolution. An alias cannot change the defining module or reveal an opaque type.

[RFC 0004](0004-functions-overloads-and-generic-requirements.md#whole-vector-specificity) owns static overload ranking. This RFC supplies the variant/parent relationships used in those input patterns. Pattern coverage never creates a runtime overload dispatcher.

[RFC 0003](0003-expressions-bindings-and-control-flow.md#matching-and-patterns) owns match evaluation and control transfer. This RFC proposes the scrutinee and pattern type relationships. Branch joins must preserve the actual dependencies defined by the ownership companion.

Expectations follow [RFC 0001's accepted snapshot](https://github.com/Reilley64/rill/issues/28#issuecomment-6907867211). Complete examples use helper functions rather than reading startup lets from an expectation. This draft does not amend accepted expectation rules.

### Ownership, standard types and backend layout

Implicit-copy eligibility depends on the full static value type. A parent enum needs every possible payload to qualify. A known unit variant may qualify independently. Actual copying, disposal and property derivation remain ownership/library rules.

Record and enum generics can contain borrows. Construction, matching and widening preserve those dependencies. A nominal name does not provide noBorrow, sendable or shareable evidence.

A local Packet is not canonical Option or Result. Imports and constructor helper calls retain their actual library identities. Raw enum syntax cannot manufacture private error metadata or protected runtime state.

The inline graph needs trusted indirect representation metadata, not arbitrary public assertions. Backend/native/Wasm work must validate the actual representation and cleanup. No fixed pointer size, tag encoding or object ABI follows from this proposal.

### Source reconciliation

| Input | Disposition |
| --- | --- |
| [RFC 0005 policy approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6912492481) | The owner confirmed generic variant spelling, context-first bracket inference and inline-summary/containment-graph policy. This does not accept the complete RFC or validate its checking mechanisms. |
| [#9 core decision](https://github.com/Reilley64/rill/issues/9#issuecomment-6308790321), [ADR 0004](../adr/0004-named-types-are-nominal.md) | Preserve nominal identity, local inference, void/never and trailing generic defaults. Later decisions replace contextual record construction, literal types and unions. |
| [#21 decision](https://github.com/Reilley64/rill/issues/21#issuecomment-6853593704), [ADR 0008](../adr/0008-records-require-named-construction.md) | Preserve named construction/rebuilding and no field-shape search. Do not restore historical separately named union members. |
| [#19 decision](https://github.com/Reilley64/rill/issues/19#issuecomment-6870013901), [ADR 0012](../adr/0012-enums-replace-unions-and-literal-overloads.md) | Preserve closed enums, variant types, parent joins, static selection, visibility and forbidden mutable widening. The later policy decision fixes generic spelling. Complete formal layout/inference remains unfinished. |
| #17, Draft RFC 0002 and [ADR 0010](../adr/0010-file-modules-explicit-exports-and-privacy.md) | Reuse canonical declaration identity and privacy. Supply finite-layout rules without making imported aliases new types. Complete resolver work remains in RFC 0002. |
| #15/#16, [mutation decision](../adr/0007-explicit-mutation-with-conservative-borrow-dependencies.md) and [concurrency decision](../adr/0009-concurrency-safety-and-cooperative-cancellation.md) | Preserve references inside values, explicit access and separate property evidence. Full borrowing/capture and cleanup judgments remain missing. |
| #18, [numeric/text decision](../adr/0011-explicit-numeric-widths-and-utf8-string-views.md) | Preserve explicit base types and UTF-8 value categories. Later #19 removes singleton source types. Complete literal and primitive-operation semantics remain companion work. |
| #23, [library catalogue](../standard-library-contracts.md) sections 2/3/6 | Preserve structural enum copy eligibility, nominal List/Map and fixed callable tuples. Propose contextual bracket rules without variadic generics or return-only selection. Full inference/evidence remains incomplete. |
| #24, [diagnostic registry](../diagnostic-codes.md) | Preserve nominal mismatch, syntax, missing width/inference and privacy meanings. No compiler diagnostics are claimed as observed. |
| #41 and Draft RFC 0004 | Keep implement syntax and conversion-only exceptions. Type annotations do not create general return-only dispatch or implicit conversion. |
| [Record/match source](../../prototypes/05-records-match.rill) and [overload/enum source](../../prototypes/03-overloads.rill), source revision | Preserve their purposes and independent rejection obligations. Sources remain authored design evidence, not executed Rill tests. No prototype bytes change here. |

## Compatibility and migration

Historical bare record literals require named constructors. Type annotations cannot preserve the removed contextual-construction spelling. Inline anonymous record types also remain removed.

Replace general unions and primitive-literal overload types with the approved enum or ordinary base-type model. Existing literal expressions and primitive-value patterns remain valid under their numeric/text rules.

The confirmed parent-applied generic variant spelling fixes a previously unspecified syntax detail. It does not relabel historical illustrative examples as compiler-accepted programs.

The bracket policy preserves uncontextualized homogeneous Lists and contextually typed tuples. Ambiguous tuple/List input shapes require annotations. Reconcile direct callable-tuple fixtures before treating the inference procedure as complete.

The inline-layout policy must permit finite nesting and unused generic arguments. A blanket repeated-name rejection would reject T15 incorrectly. A repeated-closed-type check alone would miss T16's type growth.

No released Rill ABI exists. This draft does not promise source-compatible serialized layouts or stable native enum tags. Editor and compiler integration remain implementation work.

## Security/resource implications

A forged indirect-type marker could hide recursive storage or invalid native pointers. Trusted representation boundaries need canonical identities and actual implementation validation. Private fields alone are insufficient.

Layout, coverage and contextual inference can consume substantial CPU and memory. Use finite analyses where specified. An exhausted or incomplete checker must not report successful type validation.

A finite type can still carry exclusive borrows, non-copying error causes or goroutine-affine guards. Layout success cannot grant copying, transfer or lifetime extension. Check those properties separately.

Nominal identity reduces accidental interchange of resource-bearing records. It does not itself prove resource safety, native retention or safe disposal. Those mechanisms retain their own specification and validation gates.

## Rationale, alternatives and prior art

Structural records would reduce declarations but make matching fields imply interchangeability. The approved #9/#21 decisions instead give each record a canonical owner and explicit constructor. Tuples retain a structural role without adding anonymous records.

General unions and primitive singleton types could express more type combinations. The approved #19 decision chooses one closed enum model instead. Runtime match handles value branching without changing overload selection.

Parent-applied generic variant spelling keeps type arguments on their owning enum. A second variant argument list would duplicate that ownership. The owner [confirmed this spelling](https://github.com/Reilley64/rill/issues/28#issuecomment-6912492481).

Contextual bracket typing preserves existing List and fixed-tuple use cases. It also requires staged inference and explicit ambiguity failures. A return-preference heuristic would make result expectations choose input categories and conflict with approved overload rules.

Inline-parameter summaries distinguish actual storage from unused arguments. Name-only cycle checks cannot express that distinction. Target size limits remain separate from recursive containment.

[ADR 0012](../adr/0012-enums-replace-unions-and-literal-overloads.md) cites Rust's enum and union references. Rill deliberately differs by allowing variant types in signatures. Those primary references explain the comparison, not a proof of Rill's checker.

## Implementation implications

The compiler needs canonical declaration/type-application records and separate variant identities. Import bindings must refer to those records without creating new nominal types. Source privacy must remain separate from compiler metadata access.

Type construction needs explicit substitution and context records. Bracket inference must preserve its chosen category through callback checking and result validation. A failed output check must not restart category selection.

The layout checker needs inline-parameter summaries and graph-cycle detection. It must account for generic forwarding and trusted indirection rather than expanding recursive types without a bound. The confirmed policy still needs a formal reference model.

Pattern checking needs static scrutinee types, nested coverage and guard-aware fallthrough. Ownership checking must preserve payload state across failed guards and early exits. Neither phase may rely on eventual concrete monomorphized bodies to repair missing evidence.

Runtime representations, capture storage, target limits and bounded cleanup remain separate work. The scaffold does not implement any of these compiler phases.

## Evidence and checking plan

### Actual evidence

| Artifact / revision | Evidence label | Rule/question exercised | Checker/tool and command/environment | Expected / observed outcome | Limits, negative/unavailable results and gate links |
| --- | --- | --- | --- | --- | --- |
| Source decisions and ADRs at source revision | HandChecked | Nominal records, later enum rules and removed historical syntax. | Drafting agent, 2026-11-16. Read current #9/#19/#21 decisions, ADRs and cited catalogue sections. | The drafting agent identified current named-construction and enum rules and used them as source inputs. | This is provenance checking, not formal validation or RFC acceptance. |
| T1 through T19, this draft | NotChecked | Construction, inference, coverage, layout and interactions. | No Rill parser, type checker or ownership checker is available. | Expected outcomes appear above. No compiler result is observed. | No example is RillExecuted. Missing companions remain explicit. |
| Confirmed bracket and inline-layout policies | NotChecked | Context uniqueness, finite containment and generic forwarding. | No formal model or solver has been run. | No completeness, termination or soundness result is claimed. | Needs formal rules and adversarial checks before implementation readiness. |
| Prototype/research inputs at source revision | NotChecked | Executable acceptance of this Rill type system. | Existing artifacts remain supporting inputs with their own evidence scopes. | No new reference or Rill execution was performed for this draft. | Authored expectations and historical experiments do not execute this specification. |
| RFC documents and index, local working draft | ToolChecked | Structure, writing limits, local links and preservation of earlier rules/artifacts. | Drafting agent, 2026-11-16. Temporary Python checks; `mise run check:docs`. | Five file/register identities and required headings passed. The documentation checker found no local link errors. Earlier code and accepted rules stayed unchanged. | Document checks do not parse Rill, validate layout/inference or audit the formal ASD-STE100 dictionary. |

### Planned checks. Not executed evidence

- Parse named records, enums, generic applications and confirmed variant notation.
- Test canonical identities through aliases, re-exports and independent entry graphs.
- Reject structural substitution, anonymous records and invalid field sets.
- Test generic constructor inference, trailing defaults and payload-free variants.
- Test parent joins without subset unions or runtime overload dispatch.
- Reject mutable variant widening through arguments, fields, returns and generic adapters.
- Test concrete, missing and conflicting tuple/List contexts, including direct callable tuples.
- Preserve independent numeric width requirements and distinct typed container identities.
- Check nested patterns, guarded coverage, known variants and inaccessible fields.
- Test direct, mutual, forwarded, phantom, indirect and type-growing layout dependencies.
- Model inline-parameter summaries and graph construction independently before implementing the compiler phase.
- Check target representation limits separately from recursive layout.
- Validate borrowed payloads and rebuilding against the ownership companion.
- Check real diagnostics and editor type information without revealing private representations.

Native and WebAssembly acceptance need real compiler/runtime implementations and profile-specific tests. No target or client support follows from this draft.

## Open/deferred questions

<a id="review-choices"></a>

### Confirmed policies

The owner [approved all three recommendations](https://github.com/Reilley64/rill/issues/28#issuecomment-6912492481). Generic variant spelling, bracket inference and inline-layout policy are no longer open choices. This approval does not accept the complete RFC or finish its grammar and checking rules.

### Remaining in-scope authoring work

The type/pattern syntax, variant qualification and tuple/empty-enum shapes are confirmed in the shared syntax annex. Complete contextual inference with the function/callback rules, including overlapping input patterns and nested bracket literals.

Formalize inline-summary transfer rules and trusted representation boundaries. Check the graph procedure against every type constructor, generic forwarding and target-specific opaque metadata.

Complete pattern coverage and refutable-binding rules with RFC 0003. Specify guard-bound payload access and ownership commitment before fallthrough. Finish rebuilding, captures and dependency judgments with the ownership companion.

Identify exact companion revisions for joint review. These gaps prevent the [canonical readiness review](README.md#canonical-reviewready-checklist). Links to supporting ADRs are not complete proposed semantics.

### Implementation/validation gates and future scope

Implement and validate the lexer/parser, type checker, layout analysis, ownership checker and diagnostic phases. Keep compiler/runtime and target/client gates distinct from document approval.

This RFC adds no general unions, singleton types, anonymous records, reflection, user tag encodings or variance declaration syntax. It promises none of these after v0. Numbers/text and ownership are the next foundation topics, not silently included accepted annexes.

## Decision and change history

### Readiness record

No canonical readiness review has occurred. This document remains Draft. The drafting agent checked sentence/paragraph limits and applied unslop as the final prose pass. Structure and link checks passed.

These checks cannot complete missing grammar, inference or ownership rules. Formal ASD-STE100 dictionary compliance remains unverified.

### Human decisions and amendments

On 2026-11-17, the owner requested the complete syntax grammar. The owner then [confirmed all three syntax groups](https://github.com/Reilley64/rill/issues/28#issuecomment-6931939690) at the two identified annex snapshots. Existing lexical/source and semantic approvals retain their exact scopes. Full annex/RFC acceptance, readiness and implementation remain separate.

The project owner authorized working through the foundation RFCs one at a time on 2026-11-16. This draft follows the recommendation to start with types. The drafting request did not approve new policies. The owner later [confirmed the three recommendations](https://github.com/Reilley64/rill/issues/28#issuecomment-6912492481). No complete RFC revision has been accepted.

| Date / actor | Kind and scope of change | Reviewed revision or snapshot | Decision link / rationale |
| --- | --- | --- | --- |
| 2026-11-16, drafting agent | Created substantive Draft RFC 0005, register and partial coverage entries. | Source base `f33dcf4cf3f815824eeb897135775d7884f65c1a` and current source decisions cited above. This is not a reviewed RFC snapshot. | User request to work through the recommended foundations one at a time. No complete RFC or precision policy was accepted at creation. |
| 2026-11-16, project owner | Approved generic variant spelling, bracket inference and the inline-layout policy. | Three recommendations presented in chat. Pre-recording draft SHA-256: `5ce735ca83ed54a51677911fad6febb75a16bac2d6053bdc83e535961e75604f`. No complete RFC revision was accepted. | [Policy approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6912492481). Status remains Draft. Grammar, formal checking, companions and readiness remain incomplete. |
