# Numbers, string views, and adversarial cases

Read the source examples in [`prototypes/08-numbers-strings.rill`](../../prototypes/08-numbers-strings.rill).

These examples follow ri-a2c, [ADR 0011](../adr/0011-explicit-numeric-widths-and-utf8-string-views.md) and the [published library contracts](../standard-library-contracts.md). The companion still contains 41 intended-valid expectations. They have been inspected, not executed by a compiler/runtime or a compile-fail runner. Library imports now use the published Bytes, parsing and string APIs. Different error types still require explicit adaptation, and ordinary return-only overloads remain invalid.

[ADR 0017](../adr/0017-user-implementable-conversions.md) replaces the old closed-only conversions with user-implementable canonical families and tryInto/tryFrom. The generic fallible helper names E and requires `tryInto(To, From, E), display(E)`. It assumes neither a default error nor implicit adaptation. Family bodies use implement; declarations and standalone functions use function.

## Valid fixture coverage

The numeric cases cover width inference, aliases, generic conversion evidence, constant-input range failure, rounding, truncation, non-finite rejection, signed minima, shifts and remainder. A well-typed integer trap sits in an unexecuted branch.

The text cases cover exact substring copies, byte/scalar boundaries, descriptor and retained-storage dependencies, static literal views and explicit copies of named owners. They also cover exclusive-reference formatting, sequential snapshots, custom canonical show, scalar validation, strict parsing, UTF-8 decoding and lossy replacement. Literal cases distinguish raw strings, escapes, normalization and special float rendering.

Decoding now reads an explicitly constructed Bytes value. decodeUtf8 produces owned text; decodeLossy replaces invalid sequences. String byte lookup first obtains the canonical bytes view. Nothing assumes List contiguity or invents a borrowed List header. Generic copy borrows logical T through `copy(&T)`, including a substring view.

A primitive canonical formatter cannot implement itself by interpolating the same primitive through that formatter. The [overload](../../prototypes/03-overloads.rill) and [module](../../prototypes/07-modules/formatting.rill) examples instead declare separate show families. Templates inside those bodies still use canonical standard formatting.

## Independent compile-rejection cases

Each snippet is a separate schematic rejection case, not an active compiler test. ri-3t8 owns messages and ri-48g formalizes the checking rules.

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

Reject int, float, byte, isize and usize as built-in numeric names. Reject `value as u8`; ordinary import aliases using as remain valid. A unique expected type may determine a destination, but there is no fallback. Partial generic calls infer trailing parameters from already-typed inputs.

The four names identify two canonical user-implementable families under [ADR 0017](../adr/0017-user-implementable-conversions.md). Standard numeric pairs remain protected. Conversion-specific endpoint selection does not authorize general return-only dispatch or relax ordinary duplicate-pattern rules. Do not infer standard conversions for bool, char, records or references from the numeric table. Authorized user conversions may use domain types; Unicode and byte validation have their own contracts.

```ts
function missingEvidence<To, From>(value: From): To {
  return into<To>(value); // Needs canonical into(To, From), before monomorphization.
}
```

A local namesake or fabricated ordinary requirement cannot prove canonical conversion availability. Alias pairs must behave identically in concrete and generic calls. Conversion evidence does not replace separately required noBorrow, copy, sendable or shareable evidence.

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

Reject mutation through a read-only view, mutable substrings and writable byte access that bypasses UTF-8 validity. `&mut &string` permits descriptor reassignment, not mutation of the backing text. Reassignment must preserve references retained elsewhere. Ending a mutable-storage call loan cannot erase the backing dependencies it stored. Reborrowing `&*view` preserves the exact extent and source rather than creating an owning header.

Reject disposing, moving or mutating an owner while its view remains needed. Reject returning views into temporaries or computed local templates, including through generic or erased wrappers. A reference to a local descriptor slot cannot escape either. Preserve the conservative exclusive restriction on a read-only view derived from exclusive input. A physically safe header move does not itself establish a language exception.

A direct literal borrow, including a raw or uninterpolated backtick form, may use static backing. Its view can pass through local descriptors. A template with an expression remains computed even if an optimizer could fold it. Static backing and folding do not change `noBorrow(&string)` or permit named functions to capture runtime lets.

Literal expressions and named bindings have different ownership rules. A named string binding moves unless explicitly copied, so repeatedLiteral now calls `copy(&tag)` before using both owners. Generic and record copies still require their normal evidence; neither constants nor optimization can duplicate ownership of freeable storage.

### UTF-8, characters, and source syntax

Reject malformed UTF-8 strings, multi-scalar char literals, surrogate or out-of-range escapes, non-ASCII `\xNN` text escapes and unescaped newlines in ordinary quotes. Combining scalars may form one grapheme, but not one char literal. Raw strings remain UTF-8 text rather than arbitrary byte containers. These rules add neither byte-string literals nor raw identifiers.

Invalid literal scalars are compile errors. Checked construction returns None for invalid runtime u32 inputs. Unassigned, non-printable and noncharacter scalars remain valid when they are neither surrogates nor above U+10FFFF. UTF-8 bytes and u32 code points are different representations.

A scalar-boundary slice may split a grapheme, and byte lookup may return a continuation byte. There is no integer character indexing, implicit normalization or locale collation. Source CRLF normalization applies to literal line breaks; runtime decoding preserves CR/LF bytes. Literals do not dedent automatically.

## Isolated runtime outcomes, not live expectations

Panic ends the whole program under ADR 0005. Runtime panic tests therefore need separate processes once a compiler exists. They cannot join the intended-valid expectation run as catchable failures.

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

Constant folding cannot move a trap before effects, reject a well-typed program merely because a trap is provable, or discard a trap whose result is unused. `-128i8` is a valid negated literal. Negating an already-established signed minimum is instead a checked operation. Constant arithmetic does not provide arbitrary compile-time execution of user functions.

## Interpolation, suspension, and generic-reference cases

- Compare `${owned}`, `${view}`, and read-only observation of an exclusive string handle. Explicit standard reference-forwarding implementations must preserve extent/capabilities; no blanket auto-deref dispatch or move of the exclusive handle.
- Check `&string`, `&&string`, `&mut string`, and `&mut &string` separately through generic functions and stored aggregates. A generic copy body needs copy(T), not assumed header layout.
- Canonical show must be selected despite a local unrelated show binding. A custom record needs an authorized implementation; private-field metadata must not become automatic reflection.
- Interpolation expressions and formatting occur immediately left-to-right once. The fixture's snapshot mutation is valid after the first formatter's input loan ends; owned output is borrow-free.
- Resource-valued operands keep normal temporary cleanup. If successive operands acquire the same non-reentrant mutex, the first temporary guard can still be alive and the second acquisition can panic. Do not early-unlock to change that result.
- Suspending/effectful show, scoped children, cancellation and native jobs retain all owners/guards until execution/cleanup ends. Returning owned output does not authorize premature input cleanup while the formatter is still running.

## Required validation/handoffs

The published catalogue now defines error types, signatures and module homes for conversion, parsing, byte/scalar APIs and formatting. Its partial-generic-argument, alias, primitive-bootstrap and reference-forwarding rules are design inputs, not implementation proof. Byte APIs require real source representations. Generic bodies must retain evidence rather than rediscover it from eventual instantiations.

ri-48g must formalize logical references, descriptor slots and backing sources. It must cover retained mutable storage, callee exits, literal-copy and widening rules, direct static literal borrows, dependency-sensitive cleanup, constant traps, numeric availability and LLVM lowering.

Test boundaries near i128/u128 without lossy intermediate checks. Cover both alias names, NaN, infinity, signed zero, fmod signs, overflow and shift-count edges. Float roundtrips must include subnormals and maximum finite values. Keep fast-math, reassociation and fusion disabled unless a defined operation explicitly requests them.

ADR 0012 rejects primitive-literal types and overload parameters. Refined knowledge cannot create missing widths, infallible narrowing pairs, duplicate owners or access to private representations. Turn these inspected design cases into executable compiler/runtime fixtures later. Closing a source ticket proves neither a checker nor RFC acceptance.
