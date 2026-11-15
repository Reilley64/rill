# Numbers, string views, and adversarial cases

Design artifacts for [#18](https://github.com/Reilley64/rill/issues/18) and [ADR 0011](../docs/adr/0011-explicit-numeric-widths-and-utf8-string-views.md). The `.rill` companion contains 41 intended-valid expectations, checked by inspection only. There is no compiler/runtime or executable compile-fail suite. New standard-library paths/helper names and `Result<T>`'s illustrative default error are **not complete #23 interfaces**. In particular, different error types cannot silently convert for `?`, and target-only generic helpers cannot be implemented as duplicate return-only overloads.

**#41 supersession:** [ADR 0017](../docs/adr/0017-user-implementable-conversions.md) replaces the closed-only conversion model with user-implementable canonical families and the `tryInto`/`tryFrom` spellings. The companion's generic fallible helper now names E and requires `tryInto(To, From, E), display(E)`; it does not assume the default Error or implicit error adaptation. Unrelated helper sketches remain historical where #23 supersedes them. Family bodies now use `implement`; body-less declarations and standalone functions remain `function`.

## Valid fixture coverage

The companion covers width establishment and aliases, generic infallible/fallible evidence, range failure as a Result even for constant input, float rounding/truncation/non-finite rejection, signed minimum, shifts/remainder, an unexecuted integer trap, exact substring copying, byte versus scalar boundaries, descriptor and retained-storage dependencies, static literal views, literal copying, exclusive-reference formatting, sequential formatting snapshots, canonical custom show, scalar validation, strict numeric parsing, UTF-8 decoding/lossy replacement, escapes/raw strings, normalization distinctions and special float rendering.

`decodeUtf8`/`decodeUtf8Lossy` are illustrative **copying** operations from `List<u8>`: no List contiguity or borrowed byte-header representation is assumed. Borrowed validation needs a genuinely specified contiguous source in #23. Byte/scalar iterator types and exact names also remain #22/#23 work; do not manufacture a headerless `&List<u8>` by analogy with primitive string views.

Primitive canonical formatters must not implement themselves by recursively interpolating their own argument. Prototype 3 and module fixture 10 deliberately declare separate show families; primitive templates in those bodies use the canonical standard family, not the locally same-spelled one.

## Independent compile-rejection cases

These are schematic snippets, not active compiler tests. Their expected rejection is a design contract; #24 owns messages and #28 explicit rules.

### Widths, refinements, and conversion availability

```ts
let unknown = 10;                 // No width/default.
let unknown = into<u8>(10);       // Destination does not establish source width.
let tooBig = 256u8;               // Invalid individual literal.
let tooSmall = -129i8;
let unsignedNegation = -0u8;      // Unary negation is not defined on unsigned types.
let mixed = 1i32 + 1i64;          // No implicit typed numeric conversion.
let missingTry = into<u8>(1i32); // Base-type pair is fallible, despite this value.
```

Reject int/float/byte/isize/usize as built-in numeric names and reject `value as u8`. Ordinary import `as` aliases remain valid; the ban concerns conversion/type-assertion syntax. Destination inference can use a unique explicit expected type, never a fallback. Partial generic arguments infer trailing parameters from already-typed inputs.

The four conversion names identify **two canonical user-implementable families** under #41 / [ADR 0017](../docs/adr/0017-user-implementable-conversions.md), with protected standard numeric pairs. Conversion-specific source/target selection does not authorize general return-only dispatch or relax ADR 0010's ordinary duplicate-pattern rule. Reject unsupported standard numeric pairs involving bool, char, records, or references; do not infer such support from the numeric table. Authorized user conversions may involve domain types under ADR 0017's rules. Unicode/byte validation remains separately specified.

```ts
function missingEvidence<To, From>(value: From): To {
  return into<To>(value); // Needs canonical into(To, From), before monomorphization.
}
```

A same-spelled local function or fabricated ordinary requirement must not prove canonical standard conversion availability. Alias pairs must give identical concrete/generic results. Conversion availability does not replace separately needed noBorrow/copy/sendable/shareable evidence.

### Owner access versus text/descriptor access

```ts
function invalidOwner(): &string {
  let owner: string = "local";
  return &owner; // Named owner dies, despite literal initialization.
}

function invalidDescriptor(input: &string): &&string {
  let local = input;
  return &local; // Descriptor slot dies; not the same as returning local by value.
}

function invalidRetain(slot: &mut &string): void {
  let local: string = "local";
  *slot = &local; // Caller storage cannot retain this local owner.
}

function invalidMove(view: &string): string {
  return *view; // Logical read access cannot move/fabricate an owner.
}

function invalidDuplicate(): string {
  let owner: string = "ready";
  let other = owner;
  return owner; // General string moved, even with known literal contents.
}
```

Reject mutation through a read-only view, mutable substring construction, writable raw-byte access bypassing UTF-8 validity, or turning `&mut &string` into mutable backing access. Descriptor reassignment must preserve references retained elsewhere, and mutable-storage call loans cannot erase the backing dependencies they stored. Reborrow `&*view` must preserve exact extent/source, not create a temporary owning header.

Reject owner disposal/moves/mutation while a view is still needed, returning views into temporaries/computed local templates, or returning local descriptor-slot references through generic/erased wrappers. Preserve #15's conservative exclusive restriction on a read-only view derived from exclusive input. Not every physically safe header move is a new language exception.

Direct `&"literal"` (including raw/uninterpolated backtick forms) can have static backing; copying that view through local descriptors is valid. A template containing an expression remains computed, even if it could fold to a constant. Neither static backing nor folding changes `noBorrow(&string)` or permits named-function captures of runtime lets. Generic/record copies of exact literal constants may re-materialize their values but must never duplicate ownership of freeable buffers; a record containing general string stays non-implicitly-copying.

### UTF-8, characters, and source syntax

Reject malformed UTF-8 string construction, a char literal containing multiple scalars, surrogate/out-of-range Unicode escapes, non-ASCII `\xNN` text escapes, and unescaped physical newlines in ordinary quotes. Combining scalars may form one grapheme but still cannot form one char literal. Raw strings remain UTF-8 text, not an arbitrary byte container; no byte-string literals/raw identifiers are added.

Invalid literal scalars are compile errors; invalid **runtime** u32 scalar inputs return None from checked construction. Unassigned/non-printable/noncharacter scalar values remain valid if they are not surrogates or above U+10FFFF. UTF-8 byte encoding is not the u32 code-point representation.

A scalar-boundary slice can still split a grapheme. Explicit byte lookup may return a continuation byte. No integer character indexing or implicit normalization/locale collation is introduced. Source CRLF normalization affects literal source line breaks; runtime decoding preserves CR/LF bytes. No automatic dedent.

## Isolated runtime outcomes, not live expectations

A panic is whole-program fatal under ADR 0005; these cases need separate processes once a compiler exists. Never add them to the intended-valid expectation run as if panic were catchable.

| Operation with valid established types | Expected outcome |
| --- | --- |
| `127i8 + 1i8` when executed | Panic, in all build modes |
| Integer division/remainder by zero | Panic |
| Signed MIN / -1 or MIN % -1 | Panic |
| Shift count equal to or greater than width | Panic, no masked count |
| The same well-typed trap in a branch never executed | No panic and no mandatory compilation rejection |
| Out-of-range single literal, even in an unexecuted branch | Compilation rejection |
| `tryInto<u8>(300u16)` | Err, including when input is constant |
| `tryInto<u8>(-0.5f64)` | Ok(0), truncate before range-checking |
| Float-to-integer NaN/infinity or out-of-range truncated result | Err |
| Integer-to-float / float narrowing | IEEE-rounded value, possibly infinity; not a conversion error |
| Floating division/remainder by zero | Floating infinity/NaN behavior, not integer panic |
| Invalid slice order/bounds/scalar endpoints | None for checked API; explicit panic convenience may differ |
| Invalid UTF-8 | Err for strict decoding, U+FFFD substitution for explicitly lossy decoding |
| Malformed/trailing/whitespace numeric input | Err under strict default parsing |
| Finite decimal parsing rounded to infinity | Err, unlike explicitly spelled infinity |

Folding cannot hoist a trap before effects, reject a well-typed program merely because it proves a trap, or drop a trap because its result is unused. `-128i8` is a valid negated literal; negating an already-established signed minimum is an ordinary checked operation. Constant arithmetic is not arbitrary const execution of user functions.

## Interpolation, suspension, and generic-reference cases

- Compare `${owned}`, `${view}`, and read-only observation of an exclusive string handle. Explicit standard reference-forwarding implementations must preserve extent/capabilities; no blanket auto-deref dispatch or move of the exclusive handle.
- Check `&string`, `&&string`, `&mut string`, and `&mut &string` separately through generic functions and stored aggregates. A generic copy body needs copy(T), not assumed header layout.
- Canonical show must be selected despite a local unrelated show binding. A custom record needs an authorized implementation; private-field metadata must not become automatic reflection.
- Interpolation expressions and formatting occur immediately left-to-right once. The fixture's snapshot mutation is valid after the first formatter's input loan ends; owned output is borrow-free.
- Resource-valued operands keep normal temporary cleanup. If successive operands acquire the same non-reentrant mutex, the first temporary guard can still be alive and the second acquisition can panic. Do not early-unlock to change that result.
- Suspending/effectful show, scoped children, cancellation and native jobs retain all owners/guards until execution/cleanup ends. Returning owned output does not authorize premature input cleanup while the formatter is still running.

## Required validation/handoffs

#23 must finalize real error/signature/module contracts for conversions, parsing, scalar/byte APIs and canonical formatting, including the partial-generic-argument rule, alias identity, primitive bootstrap and supported reference forwarding. Byte APIs need genuine source representations; generic interfaces must retain evidence rather than inspect eventual instantiations.

#28 must write logical-reference/descriptor-storage/backing-source rules, retained mutable-storage and callee-exit checks, literal-copy/widening rules, direct-static-literal exception, dependency-sensitive cleanup, constant-trap acceptance, exact numeric availability and safe LLVM lowering. Test floating/integer boundaries near i128/u128 without lossy intermediate checks; both alias names; NaN/infinity/signed zero; fmod signs; overflow/count edges; float roundtrips including subnormals and maximum finite values. Keep fast-math/reassociation/fusion off unless explicitly requested by a defined operation.

#19 still decides literal-overload policy. Refined knowledge cannot create missing widths or infallible narrowing pairs, duplicate ownership, bypass private representations, or depend on optimization. Hand-check these cases now; convert them into executable compiler/runtime fixtures later. Closing #18 does not prove a checker or accept the future RFCs.
