# Named types are nominal

`type Point = { x: int; y: int };` creates a new type, as in Rust, even though Rill's syntax comes from TypeScript, where `type` is only an alias. Two named types with the same fields are different types, and a record literal takes its type from context. Record types written inline, such as `{ index: int; value: T }`, and tuples stay structural. Named types are nominal so that every type an overload or `dispose` attaches to has a module that defines it: #10's orphan rule needs that module, and a structural `File` would let any `{ fd, path }` literal be disposed.

## Considered options

- **Structural, as in TypeScript.** Private fields could block accidental `File` values, but the orphan rule would have no defining module to point to. Rejected.
