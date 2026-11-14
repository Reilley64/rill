# Enums replace unions and literal overloads in v0

[Issue #19](https://github.com/Reilley64/rill/issues/19) replaces primitive-literal types/overloads and `A | B` union types with nominal, closed enums. Enums support unit, tuple, and record variants, with variant-specific overloads selected only from static types. This provides one sum-type mechanism without making runtime arguments fail merely because a narrower overload exists.

This is a confirmed language-design direction, not a compiler implementation, safety proof, or acceptance of the future feature RFCs. ADR 0011 is reserved by #18's separately prepared numbers/strings decision; this decision follows it.

## Declarations, construction, and variant types

```rill
enum Language {
  English,
  French { crazy: string },
  German(string),
}

let english = Language.English;
let french = Language.French { crazy: "bonjour" };
let german = Language.German("hallo");
```

The enum owns a closed set of nominal variants. Variants from different enums are different types even if names or payloads match. The enum's defining module owns its variants; aliases/re-exports preserve canonical identity and existing overload/orphan permissions. No implicit integer/string backing-value conversion, user-specified numeric discriminants, or enum arithmetic is introduced.

`Language` names the parent enum type; `Language.French` names a variant type in annotations and overload parameters. This deliberately differs from Rust, whose variants cannot be used as standalone type specifiers. Payload values do not create further variant types: every French payload belongs to the same `Language.French` type. Ordinary named records and tuples remain; an enum may wrap a reusable record explicitly rather than implicitly admitting it as a union member.

Generic enums use existing type parameters, trailing defaults, and inference:

```rill
enum Option<T> {
  Some(T),
  None,
}

enum Result<T, E = Error> {
  Ok(T),
  Err(E),
}
```

These illustrate generic enum syntax, not complete standard-library definitions: #23 still owns canonical paths, Error, cause/trace storage, constructors/helpers, and the contracts of `?`/`guard`. Payload-free constructors require an expected type or explicit type arguments when generic arguments cannot otherwise be inferred. Existing numeric-width inference from #18 still applies; enum inference cannot invent a numeric width.

## Static selection and inference

```rill
function greeting(language: Language): string;

function greeting(language: Language): string {
  return "Hello";
}

function greeting(language: Language.French): string {
  return language.crazy;
}

let french = Language.French { crazy: "bonjour" };
let language: Language = Language.French { crazy: "bonjour" };
// greeting(french) selects the French implementation.
// greeting(language) selects the general implementation.
```

A constructor/inferred binding preserves its variant type. An explicit parent-enum annotation widens it. A join of different variants of the same enum has the parent enum type, not an implicit subset union. Function boundaries use their declared parameter/return types; optimizer knowledge of a particular runtime variant does not narrow them.

Filter candidates by the argument's static type, then apply the existing all-parameter most-specific order: a variant beats its parent enum, concrete types beat type parameters, and more-specific generic patterns beat less-specific patterns. A whole-enum argument does not make a variant-only implementation applicable. A general implementation remains usable even when variant implementations exist. Missing applicable implementations and incomparable best matches remain errors. Even a complete set of variant implementations does not synthesize a whole-enum implementation or runtime dispatcher.

Existing body-less declarations, parameter/reference shape rules, return rules, explicit generic requirements, expected-type selection of overloaded function values, canonical collection/duplicates, visibility, and protected operations are unchanged. Neither generic monomorphization nor an indirect call may introduce runtime dispatch or rediscover a variant from a payload value. #28 must spell out these interactions and check them with examples.

## Patterns and runtime branching

```rill
function selectGreeting(language: Language): string {
  return match (language) {
    Language.English => "Hello",
    Language.French { crazy } => crazy,
    Language.German(text) => text,
  };
}
```

Use qualified unit, record, and tuple variant patterns. Matching establishes the variant type of the scrutinee within the arm, allowing its overload there. Patterns retain existing guards, wildcards, and borrow-aware field binding; guards alone do not establish exhaustive coverage. Reject non-exhaustive matches. Ordinary record-field and primitive-value patterns remain valid: removing literal *types* does not remove literal expressions or value matching.

Enum variant identity is built in; no handwritten literal `kind` field is needed. Fields named `kind` may still be ordinary data, but do not confer variant identity. `match` remains the way to branch on a runtime variant, not overload resolution.

## Visibility, ownership, and widening

Variants and payload fields share the enum's visibility. Exporting an enum exposes its complete variant set and payload fields; v0 has no individually private enum variants/payload fields. Publicly exposed payload types must satisfy the existing public-interface visibility rules. A private enum can remain opaque through exported operations; an ordinary record's private field can also hide an enum. Ordinary record privacy is unchanged.

Owned variant values can widen to their parent enum, and read-only variant borrows can be viewed as read-only parent-enum borrows. Do not infer analogous widening through arbitrary generic containers. In particular, `&mut Language.French` cannot become `&mut Language`: the latter would permit replacement with another variant while a caller still assumes French. Store the value as `Language` before borrowing mutably if replacement may change variants. Payload mutation that preserves a variant remains allowed. An exclusive handle cannot gain this forbidden capability through annotations, stored values, generics, or reborrowing.

Existing ownership, copying, disposal, retained/result dependencies, callable modes, noBorrow/sendable/shareable evidence, cancellation-safe replacement, and protected built-in operations still apply to contained payloads. Narrowing or widening does not duplicate ownership, erase dependencies, or promise a new in-place-layout optimization. #23/#28 must reconcile full payload/parent interfaces and adversarial cases before implementation readiness.

## Superseded rules and alternatives

- No primitive singleton/literal types in v0, including strings, numbers, booleans, and characters. Literal expressions infer their ordinary base types under existing width/context rules. Thus `let language = "fr"` has type `string`, not `"fr"`. This supersedes #9's literal-preserving inference and #18's literal-type refinements, not #18's numeric widths, base-range conversion rules, UTF-8 rules, or evaluation semantics. Compiler constant information is not a singleton source type or overload permission. #23/#28 must reconcile static literal materialization/copy contracts without reintroducing literal types or duplicating freeable ownership.
- No primitive-literal overload parameters such as `language: "fr"`, `n: 0`, or `flag: true`. Use a base-type implementation with value matching, or an enum when a named finite vocabulary is appropriate. String-literal types/overloads may be reconsidered after v0; this is not a commitment to add them. Other primitive-literal mechanisms are not silently retained.
- No general `A | B` union types or separately declared record-member unions. This supersedes the union/tag portions of #9, #17, #21 and ADRs 0004/0008/0010; nominal records, named record construction, tuple rules, and ordinary record privacy remain.
- Keeping both enums and general unions was rejected for v0: two construction/narrowing models are not justified yet. Enums explicitly wrap reusable records instead. General unions may be reconsidered later.
- The original #10 rule rejected an unknown runtime value when a literal implementation might match it. Keeping literal overloads with static-only applicability was considered, then rejected in favor of enum variants. Implicit runtime overload dispatch remains rejected.
- Fixed string/integer backing values and payload-free-only enums were considered but do not meet the chosen Rust-style payload design.

## Evidence and follow-through

See `prototypes/12-enums.rill` and `prototypes/12-enums.md` for intended-valid expectations and independent rejection/adversarial cases. They are hand-checked design artifacts, not executed compiler tests. The prototype README identifies older examples whose literal/union forms are historical; do not treat their previous validity claims as v0 acceptance of removed syntax.

#23 must reconcile Option/Result, trace/cause wrappers, tagged selection/message results and all required public library signatures. #24 owns diagnostics for removed literal/union syntax, non-exhaustive matching, missing whole-enum implementations, nominal/variant mismatches, generic inference, visibility and mutable-widening errors. #28 must reconcile earlier source tickets and fixtures, formalize variant typing/joins, widening/layout and borrow safety, generic/function-value selection, metadata and exhaustive checking, and include accepted/rejected adversarial examples. Resolve any unworkable interface/checking rule through the map rather than claim this direction proves a checker sound.

Rust references: [enums and constructors](https://doc.rust-lang.org/reference/items/enumerations.html) and [untagged unions](https://doc.rust-lang.org/reference/items/unions.html). Rust's unsafe untagged storage union is not a proposed Rill feature.
