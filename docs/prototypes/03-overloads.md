# Enum and overload design checks

Read [the source examples](../../prototypes/03-overloads.rill), which combine the earlier overload and enum programs.

[ADR 0012](../adr/0012-enums-replace-unions-and-literal-overloads.md) defines enum-based selection. The source has 21 intended-valid expectations. The first 16 retain the enum cases; five add scalar, record, generic-list and specialized-list formatting. Inspection is not compiler execution, a checker soundness proof or RFC acceptance.

## Intended-valid inventory

| Expectations | Expected reason |
| --- | --- |
| 1 to 4 | A statically known French variant selects its implementation regardless of payload. English and German use the general implementation. |
| 5 to 6 | A whole-enum parameter uses the fallback. Matching establishes French and enables its overload. |
| 7 to 8 | Inference retains French; an explicit Language annotation widens it. |
| 9 to 10 | Different-variant branches join at Language, even when the call supplies a known boolean. |
| 11 to 12 | Record and tuple variants carry owned payloads. A variant-only match is exhaustive for that variant type. |
| 13 to 14 | A payload or expected type determines generic arguments, including for a payload-free constructor. |
| 15 to 16 | A mutable variant permits payload mutation. A mutable parent-enum slot permits variant replacement. |
| 17 to 21 | Static parameter types select integer, boolean and Point implementations. The List<Point> pattern beats the generic List<T> implementation. |

## Independent rejection and boundary cases

Each snippet below is an independent design case, not an addition to the valid source. Language and greeting refer to that source's declarations. ri-3t8 owns exact diagnostic wording.

### Removed type forms, not removed expressions

Reject each primitive-literal annotation/parameter independently:

```rill
function oldGreeting(language: "fr"): string { return "Bonjour"; }
function zero(n: 0): bool { return true; }
function truth(value: true): bool { return value; }
let language: "fr" = "fr";
```

Reject general union syntax:

```rill
type Shape = Circle | Rect;
```

`let language = "fr"` remains valid with type string. Primitive value matching such as `match (flag) { true => ..., false => ... }` remains valid. Numeric literals still need a base-width context or suffix. There is no fallback width or refinement-based infallible conversion. Optimizer knowledge does not give a string binding singleton-type copy or overload permissions. The library catalogue distinguishes literal expressions from named string bindings; ri-48g must validate that distinction.

### No implicit runtime dispatcher

```rill
function onlyFrench(language: &Language): string;
implement onlyFrench(language: &Language.French): string { return "Bonjour"; }

function bad(language: &Language): string {
  return onlyFrench(language);
}
```

Reject the call because no implementation accepts the static Language type. A body-less declaration is not a fallback. Even implementations for every variant do not make the whole-enum call valid. A French match arm can call onlyFrench, and a statically French call needs no parent-enum fallback.

### No payload-value or structural dispatch

Reject passing German to a parameter typed Language.French. A different enum's French variant is also not Language.French, even with identical fields. A standalone record with a `crazy: string` field is not a French variant. A payload field named `kind` does not assign enum identity. Matching a particular payload string may control an explicit branch, but does not create a new overload parameter type.

### Exhaustiveness and guards

Reject a match on Language with only a French arm. Reject French-with-guard plus English/German arms if the guard can be false and no other arm covers French. Accept a wildcard covering the remaining cases. A match on Language.French needs only its French arm. Primitive guards/value patterns do not reintroduce primitive literal types.

### Generic inference and joins

Reject `let value = Box.Empty;` without a context that determines T. Later uses cannot repair inference by look-ahead. The explicitly annotated Box<string> example is valid. Distinct enums cannot join into an invented `A | B`; use an explicit enclosing enum. ri-48g must formalize generic variant notation, inference and joins. Box illustrates those rules; the library catalogue owns the actual Option and Result definitions.

### Mutable widening is unsound

```rill
function badWidening(): string {
  let mut french = Language.French { crazy: "original" };
  replaceLanguage(&mut french);
  return french.crazy;
}
```

Reject the call to replaceLanguage. `&mut Language.French` cannot become `&mut Language`, even if the borrow ends before the field access. An annotation, generic helper, record field, callable or reborrow cannot bypass this restriction. Read-only widening permits observation, not discriminant replacement. A mutable French value may change its payload. A mutable parent-enum slot may change variants only when no live variant or payload loan conflicts.

### Public and private enums

An exported enum exposes every variant and payload field. Reject individually private variants or payload fields, and reject public payloads that violate interface visibility. A public operation may return a private enum as an opaque value. Clients cannot name its variants, construct it, inspect payloads or match the hidden representation. Aliases do not grant those permissions. Ordinary records may still have private fields, including fields containing enums.

### Generic/indirect and ownership follow-through

ri-48g must cover direct and indirect calls, expected-type selection of function values, generic requirements, alias identity, duplicate patterns and incomparable matches across all parameters. Selection uses static types, not runtime discriminants or payload values.

Cover owned, read-only and exclusive payload access, borrowed joins and retained result dependencies. Read-only variant widening does not establish mutable-reference or arbitrary-container covariance. Preserve noBorrow, sendable, shareable, copy and protected-disposal checks through payloads and erased interfaces. Enum construction cannot hide an exclusive reference or permit replacement while a protected payload loan remains live.

## Earlier artifacts

The original early examples predated the enum decision. The [consolidation record](consolidation.md) maps them to current examples and describes the removal of literal-type overloads and union-based shapes. Original revisions remain in Git. Independent rejection snippets intentionally retain invalid syntax. Updating these design examples does not establish compiler correctness or accept a feature RFC.
