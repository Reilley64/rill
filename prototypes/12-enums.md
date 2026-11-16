# Enum and overload design checks (#19)

Source: [ADR 0012](../docs/adr/0012-enums-replace-unions-and-literal-overloads.md). `12-enums.rill` has **16 intended-valid expectations**. These outcomes were hand-checked, not compiled/executed: this repository has no Rill compiler or fixture runner. This is not a combined-checker soundness proof or acceptance of the future RFCs.

## Intended-valid inventory

| Expectations | Expected reason |
| --- | --- |
| 1–4 | Known French selects the variant implementation regardless of payload; English/German use the general implementation. |
| 5–6 | A declared whole-enum parameter uses the general implementation; matching establishes French and enables its overload. |
| 7–8 | Inference retains French; an explicit Language annotation widens it. |
| 9–10 | Different-variant branches join at Language, even when called with a known boolean. |
| 11–12 | Record and tuple variants carry owned payloads; a variant-only match is exhaustive for that variant type. |
| 13–14 | Payload/expected-type inference determines generic arguments, including a payload-free constructor. |
| 15–16 | A mutable variant permits payload mutation; a mutable parent-enum slot permits variant replacement. |

## Independent rejection and boundary cases

The snippets below are separate design cases, not uncommentable additions to the valid file. References to Language/greeting use that file's declarations. Exact diagnostic wording belongs to #24.

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

`let language = "fr"` remains valid with type string. Primitive value matching such as `match (flag) { true => ..., false => ... }` remains valid. Numeric literal expressions still need #18's base-width context/suffix; there is no fallback width or refinement-based infallible conversion. A string binding does not acquire singleton-type copy/overload permissions merely because an optimizer knows its contents. #23/#28 must reconcile #18's literal-materialization contract explicitly.

### No implicit runtime dispatcher

```rill
function onlyFrench(language: &Language): string;
implement onlyFrench(language: &Language.French): string { return "Bonjour"; }

function bad(language: &Language): string {
  return onlyFrench(language);
}
```

Reject the call: no implementation accepts the static Language type. The body-less declaration is not a fallback implementation. Adding implementations for English and German still does not make the whole-enum call valid. Accept `onlyFrench` inside a French match arm. A missing fallback must not invalidate a statically French call.

### No payload-value or structural dispatch

Reject passing German to a parameter typed Language.French. A different enum's French variant is also not Language.French, even with identical fields. A standalone record with a `crazy: string` field is not a French variant. A payload field named `kind` does not assign enum identity. Matching a particular payload string may control an explicit branch, but does not create a new overload parameter type.

### Exhaustiveness and guards

Reject a match on Language with only a French arm. Reject French-with-guard plus English/German arms if the guard can be false and no other arm covers French. Accept a wildcard covering the remaining cases. A match on Language.French needs only its French arm. Primitive guards/value patterns do not reintroduce primitive literal types.

### Generic inference and joins

Reject an unconstrained `let value = Box.Empty;`: neither a payload nor an expected type determines T, and later uses must not repair inference by look-ahead. Accept the explicitly annotated Box<string> example. Different enums do not join into an invented `A | B`; require a declared enclosing enum with explicit wrapping. #28 must specify generic variant type-argument notation, inference and joins consistently with the existing generic rules; #23 owns full Option/Result definitions, not this illustrative Box.

### Mutable widening is unsound

```rill
function badWidening(): string {
  let mut french = Language.French { crazy: "original" };
  replaceLanguage(&mut french);
  return french.crazy;
}
```

Reject the call to replaceLanguage: `&mut Language.French` cannot become `&mut Language`, even if the borrow ends before field access. Hiding the same widening behind an annotation, generic function, record field, callable interface or reborrow must not make it legal. Read-only widening is valid; it permits observation, not changing the discriminant. Payload mutation within French is valid. A parent-enum mutable slot can change variants, but a still-live variant/payload loan must prevent conflicting replacement under the existing borrow rules.

### Public and private enums

An exported enum exposes every variant and payload field; reject individually private enum variants/payload fields. Reject a public payload type that violates public-interface visibility. A canonically private enum returned by a public operation remains opaque: clients cannot name its variants, construct it, inspect payloads or match its representation. Re-export/import aliases do not grant such permission. Ordinary records may still have private fields, including a field containing an enum.

### Generic/indirect and ownership follow-through

#28 must include direct calls, expected-type-selected overloaded function values, generic requirements, canonical alias identity/duplicates, and all-parameter incomparable matches. All use static type evidence, not runtime discriminant discovery or payload-value specialization. Include owned/read-only/exclusive payloads, borrowed variant joins and retained/result dependencies; do not infer mutable-reference or arbitrary-container covariance from read-only variant widening. Existing noBorrow/sendable/shareable/copy and protected-disposal checks remain necessary through enum payloads and erased interfaces. No new enum syntax can hide an exclusive reference or let replacement run while a protected payload loan is still live.

## Earlier artifacts

Prototypes 1–10 predate this decision. Their general unions, literal-type overloads/tags, and old Option/Result/tagged-message spellings are historical where they conflict with ADR 0012. Their unrelated ownership/concurrency/module evidence remains useful. #23/#28 own the cross-feature library/fixture reconciliation; do not count those historical forms as accepted v0 tests or describe this focused fixture as their complete migration.
