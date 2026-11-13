# Records require named construction

[Issue #21](https://github.com/Reilley64/rill/issues/21) makes record identity explicit at construction: declare a nominal record type and construct it with `Point { x: 0, y: 0 }`. There are no anonymous structural record types or values, and tagged unions use separately named record variants. This supersedes ADR 0004's contextual record-literal and inline structural-record rules, while retaining nominal typing and the existing tuple rules.

## Construction and inference

```ts
type Point = { x: int; y: int };

let origin = Point { x: 0, y: 0 };
let checked: Point = Point { x: 0, y: 0 };
```

The constructor supplies the expression's type; a binding annotation checks it rather than conferring a nominal identity. `let origin = { x: 0, y: 0 };` and `let origin: Point = { x: 0, y: 0 };` are both errors. Inline anonymous record types in parameters, fields, returns, and union variants are also rejected: declare and use named record types instead. A record's declaration body still uses braces, as above.

Use named construction in function returns, arguments, nested fields, and collections, not just bindings. Inference from already typed expressions such as `let origin = makePoint();` remains valid. There is no look-ahead inference from later uses, no search for a type with matching fields, and no structural conversion between record types. Tuples remain available under their existing rules; the prohibition is on anonymous records, not anonymous functions or tuples.

## Unions, patterns, and rebuilding

```ts
type Circle = { kind: "circle"; center: Point; radius: float };
type Empty = { kind: "empty" };
type Shape = Circle | Empty;

let shape: Shape = Circle {
  kind: "circle",
  center: Point { x: 0, y: 0 },
  radius: 1.0,
};
```

Construct the selected named member, not `Shape { ... }`; a union is not a record constructor. Keep explicit literal `kind` fields and the existing member-to-union rules; this introduces neither automatic tag synthesis nor an `enum`. Standard-library unions likewise need named record variants rather than inline structural record members.

Patterns such as `{ kind: "circle", radius }` remain valid because they inspect existing values rather than construct records. A matching arm still narrows the value to its named member type.

Rebuild with a named constructor, for example `Circle { ...shape, radius: radius * by }` after narrowing to `Circle`. Bare `{ ...shape, ... }` is not a second construction spelling. Named rebuilding preserves the ownership/copy/disposal rules and existing in-place reuse guarantees for an owned value rebuilt as the same type. A named constructor is an expression after `=>`, so it needs no record-versus-block parentheses. A bare `{` after `=>` remains a block, not an anonymous record.

## Alternatives and follow-through

- **Context supplies identity (`let p: Point = { ... }`):** close to TypeScript, but expression identity depends on its use site and provides a second construction route. Rejected.
- **Anonymous structural records alongside nominal ones:** convenient for one-off data, but adds a second record model and implicit-conversion questions. Rejected.
- **Inline union variants constructed through a union name:** less declaration boilerplate, but adds a special union-construction rule. Rejected in favor of separately named variants.

This changes syntax and nominal record identity, not access permissions: constructors still have to respect field visibility and module rules (#17), ownership/borrows (#15), and the defining-module rule for overloads (#10). #23 must use named record types for language-required library results and tagged messages, including variants of `Option`/`Result` and any index/value or tagged-selection records. Existing helper calls such as `Ok(...)`, `Err(...)`, and `tag(...)` remain function calls; their required backing records must be named.

`prototypes/03-overloads.rill`, `05-records-unions-match.rill`, and `07-mutation.rill` use the new construction form. `prototypes/08-named-records.rill` includes intended-valid construction/inference examples and commented-out rejection cases. These are design artifacts, not executed compiler tests. #24 owns exact diagnostics; when an expected record type is known, suggest its named constructor rather than guessing from later uses or matching fields. #28 must reconcile the superseded rules in the final feature RFCs.
