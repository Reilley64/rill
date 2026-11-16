# #41 Q19: Rust conversion evidence and implementation-defined errors

Research input for [Rill #41](https://github.com/Reilley64/rill/issues/41), not a Rill policy decision or implementation claim. The question supplies proposed `into<To, From>(value: From): To` and `tryInto<To, From, E>(value: From): Result<To, E>`, requirements `tryInto(To, From, E)`, endpoint-based selection (not error-based), and generic/specific implementations that may choose different errors. These are interview premises, not the older closed numeric contracts in `docs/standard-library-contracts.md`.

No existing research-note directory/convention was present in this checkout; this note creates `docs/research/` rather than putting research in an ADR.

## Short answer: must `From` be specified?

**In the generic contract, the source/destination relationship must be established; it need not use the `From` trait specifically. At a call, the source type usually need not be written explicitly.** Distinguish three meanings:

- **Trait name:** Rust recommends implementing `From`/`TryFrom`; blanket implementations supply `Into`/`TryInto`. For generic consumers it recommends `Into`/`TryInto` bounds, which also accept direct implementations of those traits. `Src: Into<To>` is valid evidence without an explicit `To: From<Src>` bound; the implication is not reversible. [S1-S4]
- **Source in the requirement:** `To: From<Src>` or `Src: Into<To>` relates **both endpoints**, with one endpoint on the left of `:`. A bare destination is not a promise to convert from every source. The source may be a concrete type rather than another generic parameter. Trait bounds generally are not inferred from a function body; Rust checks the body using its declared/otherwise established evidence. A universally available blanket impl can itself establish an obligation without an extra bound. [S5, S8]
- **Source at the call:** inference commonly determines it from the argument. `u16::from(7u8)`, `let n: u16 = 7u8.into()`, and `Into::<u16>::into(7u8)` do not spell the source type as a generic argument. Explicit disambiguation is possible: `<u16 as From<u8>>::from(7u8)`. Without enough information to determine a target, annotation/disambiguation is needed. For a free Rust function declaring two ordinary type parameters, explicitly supplying type arguments requires both positions: `into::<u16, _>(7u8)`, **not** `into::<u16>(7u8)`. Rill's proposed leading-only syntax is a separate language choice. [S1-S3, S6]

Rust permits contextual inference beyond Rill's proposed immediate-context rules; these examples do not establish identical inference scope in the two languages.

## What Rust's conversion contract actually contains

Simplified stable trait shapes (attributes omitted):

```rust
trait From<Src>: Sized {
    fn from(value: Src) -> Self;
}
trait Into<To>: Sized {
    fn into(self) -> To;
}
trait TryFrom<Src>: Sized {
    type Error;
    fn try_from(value: Src) -> Result<Self, Self::Error>;
}
trait TryInto<To>: Sized {
    type Error;
    fn try_into(self) -> Result<To, Self::Error>;
}
```

`Error` is an **associated type**, not another trait input parameter. For a fixed implementing type and trait argument, coherence permits one implementation and hence one associated error; different endpoint pairs can have different errors. The traits do not require `Error: std::error::Error`, `Display`, or a universal conversion-error type. [S1-S4, S7]

Stable blanket relationships are ordinary generic implementations, **not specialization**: [S1-S4]

- `To: From<Src>` supplies `Src: Into<To>`.
- `To: TryFrom<Src>` supplies `Src: TryInto<To>` with the **same** associated error.
- `Src: Into<To>` supplies `To: TryFrom<Src>` with an uninhabited error (`Infallible` in the pinned Rust 1.85 source).
- `From<T> for T` supplies identity conversion and participates in those relationships.

Consequently, on stable Rust an infallible conversion can preclude a separate custom `TryFrom` for the same pair: it would overlap the standard blanket implementation. This is not a default that a more-specific user implementation overrides. [S1, S7]

## Generic errors: preserve, constrain, or convert

These are different contracts, all expressible without making the error a dispatch input: [S3-S5, S9]

```rust
// Preserve the implementation's chosen error; no separately named E needed.
fn attempt<To, Src>(value: Src)
    -> Result<To, <To as TryFrom<Src>>::Error>
where
    To: TryFrom<Src>,
{
    To::try_from(value)
}

// Name E and require exact equality with the chosen implementation's error.
fn attempt_e<To, Src, E>(value: Src) -> Result<To, E>
where
    To: TryFrom<Src, Error = E>,
{
    To::try_from(value)
}

// Convert the chosen error into the caller's error as a separate obligation.
fn propagate<To, Src, E>(value: Src) -> Result<To, E>
where
    To: TryFrom<Src>,
    E: From<<To as TryFrom<Src>>::Error>,
{
    Ok(To::try_from(value)?)
}
```

The `TryInto` equivalents use `Src: TryInto<To>` and `<Src as TryInto<To>>::Error`, or `Src: TryInto<To, Error = E>`. Equality is checked against the selected implementation's error: it does not create an alternative conversion for the same endpoints with a requested error. A caller can leave `E` inferred when the endpoint evidence determines it. `?` on `Result` converts errors using `From::from`; it does not silently coerce an arbitrary error into any requested `E`. [S3-S5, S9]

## Evidence is checked before monomorphization

Rust checks generic bodies with a parameter environment containing their assumptions. Calling a trait operation generates an obligation; a declared bound can discharge that obligation even when no concrete implementation can yet be chosen. Callers must satisfy the bounds. An unused generic function with an unsupported conversion is still rejected: future concrete calls cannot repair its missing evidence. [S5, S8]

The compiler guide distinguishes this check from code generation: type checking establishes that trait selection will succeed; selection can be repeated with concrete types at codegen to obtain the implementation. Its pipeline places type checking and generic MIR before monomorphization. This is **not** a claim that every Rust compile-time check occurs before monomorphization, nor that the guide's old-style solver internals define every new-solver detail. [S8]

## Stable coherence versus unstable specialization

Stable Rust allows generic and concrete impls **when they do not overlap**. It rejects two impls with a nonempty intersection; different associated errors do not make their endpoint patterns disjoint. For example, generic `TryFrom<Wrapper<T>> for Destination` and concrete `TryFrom<Wrapper<u8>> for Destination` conflict even if their `Error` definitions differ. Orphan rules additionally restrict which crates may provide impls; owning one endpoint is not a license for every possible generic impl (uncovered-parameter ordering matters). [S7]

[RFC 1210](https://rust-lang.github.io/rfcs/1210-impl-specialization.html) describes **unstable specialization**, not stable conversion behavior. Its "Hazard: interactions with type checking" example is directly relevant: if a generic implementation reports `Output = Box<T>` but a specialization reports `Output = bool`, a generic body cannot assume the former and later execute the latter. The RFC's item-level `default` design treats an overridable associated type opaquely rather than normalizing it to the fallback definition in generic code. Non-default items are final; specializing methods is not automatically permission to change an associated type. [S10]

This mechanism is not a settled stable precedent to copy wholesale. Tracking issue #31844 remains open, explicitly records soundness problems in full specialization, and lists unresolved associated-type specialization/projection questions. `min_specialization` is a restricted unstable subset, not a solution for arbitrary error-type overrides: rustc's feature gate permits its default specialization of functions, **not `default type Error = ...`**. Do not confuse associated-type defaults in trait definitions (another unstable feature) with specializing an associated type in an impl. [S11-S13]

## Implications/options for Rill, not decisions

1. **Keep endpoint evidence explicit, source spelling inferable.** `where into(To, From)` names the relationship a generic body needs; calls can infer `From` from an independently typed input. Rust supports this distinction, not the necessity of spelling a source generic argument at every call.
2. **Treat the error as determined output evidence.** Under the stated premise, `tryInto(To, From, E)` can mean "the conversion selected by these endpoints has error E," with an equality check/inference relationship. It must not become "find some other candidate whose error matches E." Rust's associated-error model illustrates this separation without requiring Rill traits or associated-type syntax.
3. **Choose how generic code preserves that output.** Options include a named `E` tied to the requirement, or a new projection/output-binding facility. The first resembles `attempt_e`; the second resembles `attempt`. Neither requires fixing all conversions to one universal error.
4. **Changing E under specialization needs an explicit soundness rule.** A generic wrapper must preserve the selected conversion's error symbolically, or require a specific error equality that every accepted instantiation satisfies. It cannot assume a generic fallback's concrete error and silently substitute a more-specific implementation with a different E later. An equality mismatch should fail evidence checking, not select the fallback merely to satisfy the requested error (which would violate the supplied endpoint-only selection premise).
5. **Error adaptation is separate from availability/equality.** If Rill permits converting the selected error into another error, that requires its own evidence and must respect Rill's existing explicit propagation/cause policy. Rust's `?` behavior is an example, not a recommendation to change that policy.
6. **Automatic infallible-to-fallible forwarding has overlap consequences.** Rust's stable blanket impl blocks competing custom fallible impls for covered pairs; Rill's permitted generic/specific overrides need their own documented coherence and output-evidence semantics rather than claiming Rust already provides them stably.

## Primary sources and verification

- **S1:** [std `From`](https://doc.rust-lang.org/std/convert/trait.From.html) and [pinned Rust 1.85 core conversion definitions/impls](https://github.com/rust-lang/rust/blob/1.85.0/library/core/src/convert/mod.rs). The pinned baseline distinguishes longstanding stable contracts from changing nightly source (including the representation/spelling of the uninhabited error).
- **S2:** [std `Into`](https://doc.rust-lang.org/std/convert/trait.Into.html), especially generic bounds and blanket implementations.
- **S3:** [std `TryFrom`](https://doc.rust-lang.org/std/convert/trait.TryFrom.html), associated error and generic implementations.
- **S4:** [std `TryInto`](https://doc.rust-lang.org/std/convert/trait.TryInto.html), associated error and bound recommendations.
- **S5:** Rust Reference: [trait-bound satisfaction](https://doc.rust-lang.org/reference/trait-bounds.html#bound-satisfaction), [associated types and associated-type bindings](https://doc.rust-lang.org/reference/items/associated-items.html#associated-types), [implied bounds](https://doc.rust-lang.org/reference/trait-bounds.html#implied-bounds) (lifetime implication does not generally imply trait bounds).
- **S6:** Rust Reference: [expression paths/turbofish and qualified paths](https://doc.rust-lang.org/reference/paths.html#paths-in-expressions); official [E0107](https://doc.rust-lang.org/error_codes/E0107.html) and [E0282](https://doc.rust-lang.org/error_codes/E0282.html) explanations.
- **S7:** Rust Reference: [trait-implementation coherence, overlap, and orphan rules](https://doc.rust-lang.org/reference/items/implementations.html#trait-implementation-coherence).
- **S8:** rustc dev guide: [trait resolution, major concepts](https://rustc-dev-guide.rust-lang.org/traits/resolution.html#major-concepts), [parameter environments](https://rustc-dev-guide.rust-lang.org/typing-parameter-envs.html), [compiler overview](https://rustc-dev-guide.rust-lang.org/overview.html), [monomorphization](https://rustc-dev-guide.rust-lang.org/backend/monomorph.html).
- **S9:** Rust Reference: [the question-mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator), specifically the `Result::Err(From::from(e))` behavior.
- **S10:** [Official specialization RFC 1210](https://rust-lang.github.io/rfcs/1210-impl-specialization.html), "Hazard: interactions with type checking" and "The `default` keyword."
- **S11:** [Official specialization tracking issue #31844](https://github.com/rust-lang/rust/issues/31844), open when inspected; RFC acceptance is not stabilization.
- **S12:** [Official `min_specialization` implementation PR #68970](https://github.com/rust-lang/rust/pull/68970) and [rustc applicability checks](https://github.com/rust-lang/rust/blob/master/compiler/rustc_hir_analysis/src/impl_wf_check/min_specialization.rs).
- **S13:** [Pinned rustc feature gate](https://github.com/rust-lang/rust/blob/1.85.0/compiler/rustc_ast_passes/src/feature_gate.rs#L450-L477): explicitly limits `min_specialization` default items to functions and separately gates associated-type defaults.

Source investigation used official repository text and GitHub issue/PR bodies. Local supplemental checks with `rustc 1.101.0-nightly (41089d07a 2026-10-27)` compiled/ran the generic examples above (without unstable features) and the shown call forms. Rejection checks confirmed missing generic evidence, incomplete two-parameter turbofish (E0107), overlapping generic/concrete `TryFrom` (E0119), and `default type` under only `min_specialization` (E0658). These checks support the cited rules; they are not a claim to have tested a current stable toolchain or validated Rill behavior.
