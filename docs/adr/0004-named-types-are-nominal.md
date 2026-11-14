# Named types are nominal

**Union and primitive-literal rules superseded by [ADR 0012](0012-enums-replace-unions-and-literal-overloads.md).** Enums replace `A | B` and literal `kind` tags in v0; primitive literal types and literal-preserving inference are removed. Nominal record identity remains.

**Construction and inline structural-record rules superseded by [ADR 0008](0008-records-require-named-construction.md).** Nominal typing and its defining-module rationale remain. The historical text below allowed contextual record literals and inline structural records; #21 now requires named record construction and separately named record variants for tagged unions. Tuple rules are unchanged.

**Numeric placeholders superseded by [ADR 0011](0011-explicit-numeric-widths-and-utf8-string-views.md).** The historical int/float spellings below are no longer built-ins. Explicit numeric bases are required; immutable literal refinements remain, while mutable initializers widen to their established base. Precise string literals can be re-materialized without duplicating ownership of a freeable general-string buffer.

`type Point = { x: int; y: int };` creates a new type, as in Rust, even though Rill's syntax comes from TypeScript, where `type` is only an alias. Two named types with the same fields are different types, and a record literal takes its type from context. Record types written inline, such as `{ index: int; value: T }`, and tuples stay structural. Named types are nominal so that every type an overload or `dispose` attaches to has a module that defines it: #10's orphan rule needs that module, and a structural `File` would let any `{ fd, path }` literal be disposed.

## Considered options

- **Structural, as in TypeScript.** Private fields could block accidental `File` values, but the orphan rule would have no defining module to point to. Rejected.
