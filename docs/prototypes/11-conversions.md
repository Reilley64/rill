# #41 conversion contract: hand-checked cases

Read the source examples in [`prototypes/11-conversions.rill`](../../prototypes/11-conversions.rill).

[ADR 0017](../adr/0017-user-implementable-conversions.md) records the confirmed decision. The [companion source](../../prototypes/11-conversions.rill) contains 14 intended-valid expectations. Neither these expectations nor the independent cases below have been executed by a Rill compiler. The [diagnostic case notes](21-diagnostics.md) and [ADR 0016](../adr/0016-structured-diagnostics.md) describe #24's diagnostic work. [Rust research](../research/0041-q19-rust-conversion-evidence.md) is comparative evidence, not a Rill implementation proof.

## Intended-valid coverage

The companion converts string to two destinations and covers both explicit and inferred destinations. It checks from/tryFrom alias identity, record and enum conversions, consumed sources and explicitly copied borrowed sources. Borrowed source storage remains usable after copying.

Other cases cover a custom displayed typed error, generic wrappers and defaults, and a concrete specialization that chooses another error. They also cover exact generic error evidence, concrete function values and protected infallible identity. Numeric cases cover standard widening, range checks, truncation and safe fallible compatibility. They inspect failures rather than assuming Result/Err equality or copying envelopes.

The following are additional independent intended-valid cases. Their signatures are schematic; do not load them as additional declarations in the companion:

```rill
// In the module defining Counter; explicit exclusive source access.
implement into<i32>(value: &mut Counter): i32 {
  value.number += 1i32;
  return value.number;
}
// Owned wrapper contains an external borrow; return preserves that external dependency.
implement into<&Point>(value: BorrowedPoint): &Point {
  return value.point;
}
// In a local Box module: generic conversion can include equal endpoints.
// Standard identity wins when T == U; the wrapper implementation handles other pairs.
implement<T, U> into<Box<U>>(value: Box<T>): Box<U>
  where into(U, T) {
  return Box<U> { value: into<U>(value.value) };
}
```

The last generic body requires abstract `into(U,T)` evidence, rather than assuming a particular element implementation. Returned references/errors retain dependencies; generic bodies need any additional property requirements they use. Public implementations cannot expose an unnameable public family requirement or gain private-field access merely by being extensions.

## Independent rejection and adversarial inventory

Each case is isolated. Rejection means a static diagnostic unless explicitly described as runtime failure; no test should depend on another rejected case being accepted.

| Case | Example / expected boundary |
| --- | --- |
| Missing destination | `into("u1")` without immediate expected type rejects, even if only one target implementation is loaded. |
| Ambiguous destination context | Context that leaves UserId versus Email unresolved rejects; no candidate-count or later-use inference. |
| Missing source width | `into<u8>(10)` rejects; destination does not establish the source width. |
| Wrong expected error | `Result<UserId,GeneralError>` cannot accept the companion's `tryInto<UserId>(&text)`. |
| Error-driven fallback | `Result<Wrapped<UserId>,GeneralError> = attempt<Wrapped<UserId>>(boxed)` rejects: the specific entry produces InvalidUserId. |
| Missing generic evidence | A generic body calling `into<string>(value)` without `into(string,T)` rejects before any instantiation, including when unused. |
| Missing source in evidence | `where into(string)` does not relate the unknown source and does not mean every T converts. |
| Missing fallible relationship | Merely declaring generic E does not establish `tryInto(To,From,E)` or display(E). |
| Missing canonical display | Constructing `Err(LocalError.Bad)` without canonical display(LocalError) rejects. A same-spelled unrelated display is insufficient. |
| Implicit error propagation | Returning a conversion failure via `?` into another E rejects; use explicit mapErr or causal wrapping with normal requirements. |
| Duplicate pair | Two bodies for `into<UserId>` from string reject during collection, even without a call. |
| Alias duplicate | Registering the same pair through into and from is also a duplicate. |
| Renamed generic duplicate | A body converting Box<T> to Wrapped<T> is identical after normalization to one converting Box<U> to Wrapped<U>. |
| Constraint/error duplicate | Changing only requirements or fallible E does not distinguish pair patterns. |
| Incomparable overlap | Local `Pair<T,i32> -> string` and `Pair<i32,U> -> string` are ambiguous at Pair<i32,i32>; neither pattern is more specific. Generic/concrete overlap alone is valid, not a rejection. |
| Foreign pair | An unrelated module cannot register a conversion from string to u32 by defining a local error or importing the family. |
| Nested-local orphan | Owning Local does not authorize conversion from ForeignBox<Local> to string. |
| Bare-generic orphan | Owning neither outer endpoint prohibits `implement<T> into<string>(value:T)`, even with a local constraint. |
| Protected numeric override | Any user replacement of numeric pairs rejects; matching public spellings does not grant trusted status. |
| Protected identity override | Explicit user into<UserId> from UserId rejects. Generic wrappers matching identity cannot displace the standard action. |
| Missing fallible adapter | User into from string to Email does not supply tryInto<Email>; no chosen universal E. |
| Missing universal fallible identity | into<UserId>(id) works, but tryInto<UserId>(id) needs an explicit fallible entry. Numeric identities retain their standard entries. |
| Owned versus borrowed | `into<UserId>(&text)` is not supplied by the companion's owned string entry or its fallible borrowed entry. |
| Moving from a borrow | A conversion from &UserId cannot move its noncopying text field; copy explicitly. |
| Local-source escape | `implement into<&string>(value:UserId):&string { return &value.text; }` rejects: owned source storage is destroyed on exit. |
| Borrowed error escape | A failure referring into consumed local input storage rejects just like a borrowed success. |
| Exclusive dependency retained | A returned view depending on &mut source blocks conflicting source access until the result dies, even if the view itself is read-only. |
| Borrow is not noBorrow | A converted externally backed reference retains its dependency; wrapping it does not allow sending it as a noBorrow message or erasing it as a cause. |
| No automatic chaining | Registered conversions from A to B and B to C do not establish a conversion from A to C. |
| No argument coercion | A function taking UserId does not accept string merely because an explicit conversion exists. |
| Ordinary return-only duplicate | Two ordinary family bodies with identical parameter patterns still reject despite different return types or explicit family arguments. |
| Unresolved function value | A bare conversion family without enough endpoint context does not become a dynamic converter. |
| Numeric compatibility | into<u8>(300u16) rejects; tryInto<u8>(300u16) is valid and fails at runtime, even for a constant. |
| Distinct numeric semantics | Truncation when converting a numeric float to an integer does not authorize truncating fractional JSON integer extraction. #31 remains separate. |

## RFC/checker follow-through

#28 must formalize pattern subsumption and intersection, constraint applicability without constraint-priority ranking, and selected-error inference and equality. It must specify how evidence passes through generic and specific implementations. In particular, an abstract generic body cannot treat a fallback's error as final when a specialization can change it.

The rules must preserve identity/numeric protection, loaded-graph visibility, first-class signature selection, nested owned/borrowed error dependencies and local-source escape checking. These notes do not establish a checker proof. If a necessary mechanism is unworkable, return it to map #1 rather than silently selecting by E or abandoning approved specificity.
