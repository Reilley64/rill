# Module, export, and privacy prototype

Design artifacts for [#17](https://github.com/Reilley64/rill/issues/17) and [ADR 0010](../../docs/adr/0010-file-modules-explicit-exports-and-privacy.md), checked by inspection only. There is no compiler to execute the examples yet. With the future tool, use `rill run prototypes/10-modules/main.rill` or `rill test prototypes/10-modules/main.rill`; that entry's directory is the application source root. The toolchain supplies `std`, not a fixture-local standard library.

## Reachable files

```text
main.rill
api.rill                       explicit export-import facade
formatting.rill                exported show family and render requirement
geometry/point.rill             exported type/function pair; family extension
geometry/offset.rill            self/super paths
resources.rill                 private fields, opaque values, private union
shapes.rill                    public union and named variants
```

All seven source files are reachable from `main.rill`. The fixture contains 15 independent expectations: 11 in main and one each in formatting, point, offset, and resources. Standard-library tests are not included in that fixture count. Equivalent imports reach point through several paths but must not duplicate its implementation, identity, or expectation.

Nested library files are not separate entry commands for this fixture: choosing a different entry directory changes the application root in v0. `resources.rill` is at the same directory level as main and can independently use that same root. Its ordinary `demo`/log code is type-checked but not executed when imported or under `rill test`.

## Intended-valid behavior

- `export` is on a function/type declaration or import item, never a separate bottom list. The facade re-exports names and a module alias without creating new nominal/family identities.
- `Position` imports both the public record type and public helper function originally called Point. `Position { ... }` constructs the record; `Position(...)` calls the helper. `OriginalPoint` has the same canonical type identity.
- `api::points` is a canonical path through an exported module binding. The resulting local alias `points` is used for qualification in code, not lookup of later imports.
- `self::point` in offset resolves beside that file; `super::formatting` resolves in its parent/root. A directory alone supplies path segments, not an importable facade.
- Importing a family declaration and defining a valid body extends the canonical family. Point's module owns its parameter type, satisfying the orphan rule. Resources' private Hidden type can also participate in that visible family.
- Extending a family does not implicitly export the local import binding. Render's public requirement refers to formatting's exported show declaration; calling render can use canonical metadata without introducing a local show source binding in main.
- Ticket is public and its label is readable, but its private code prevents external whole-record construction/rebuilding. Its factory, getter, and same-module updater are valid.
- Hidden and HiddenState are genuinely private canonical types. Inferred external values remain opaque, while their compiler-known identities/properties and visible operations still work.
- Shape is a public union. Square and Empty are exported at their origin and have public tags. Main can exhaustively match the result of empty without importing an Empty type name or re-exporting it through api. Public identity is different from a local binding.
- No named function captures runtime lets, and no expectation captures the ordinary start/demo bindings. Normal main startup prints only `(0, 0)` and `hidden 9`; resource demonstration code stays skipped. Tests execute expectations instead of startup.

## Independent rejection cases

These are schematic additions/changes, not active files or executable compile-fail tests. #24 owns exact diagnostics and #28 must turn the cases into an explicit fixture suite.

### Export syntax and source lookup

```ts
export {Position}; // Rejected: no separate export lists.

import api as A;
import A::Position; // No canonical A module in this fixture; not alias lookup.
```

If a canonical `A.rill` did exist, that import would refer to it, not to the preceding local alias. Qualification such as `A::origin()` in ordinary code instead uses the imported binding. Module/item lookup must not depend on import order.

Reject `import geometry;` in this fixture: only a directory exists, with no `geometry.rill` facade. Reject wildcard/default/string imports. Imports and re-exports select only exported bindings, even if a private counterpart shares the spelling in another namespace. A same-namespace collision is not silently ignored; use aliases.

### Module versus item ambiguity

In a separate case, let `api.rill` export a type Widget and also provide `api/Widget.rill` as a distinct source module. Reject `import api::Widget;` for competing type/module identities. `import api::{Widget};` explicitly selects the exported binding. Do not choose a filesystem winner; rename conflicting source paths/exports where needed. The same canonical identity reached through two routes is not a conflict.

### Private fields and opaque representations

```ts
let forged = Ticket { code: 7, label: "x" }; // Rejected outside resources.
let issued = issue("x");
let rebuilt = Ticket { ...issued, label: "y" }; // Rejected outside resources.
let code = issued.code; // Rejected: private field.

let value = hidden(9);
let exposed = value.value; // Rejected: private canonical type is opaque.
```

Reject importing/re-exporting resources' Hidden type name or matching its representation outside that module. A public type merely absent from local imports is not opaque: Empty still has public representation permissions, and an inferred Point still has public fields. Compiler metadata and automatic disposal do not require exposing a private type name or introducing an implicit source import.

### Family identity, duplicates, and privacy

Reject defining a new show implementation in api merely because it re-exports Point: re-exporting does not make api the defining module of Point. The original formatting declaration and the ordinary orphan placement rule still control attachment.

Reject `points::show(...)`: point's ordinary import binding was not re-exported, even though its implementation contributes to formatting's visible family. Use the canonical public family or a deliberate export-import path. A same-spelled unrelated function is not merged into that family.

Reject two distinct bodies in one canonical family with identical parameter-type patterns, including across modules where both pass the orphan rule. Normalize aliases/type-parameter renaming; different return types do not rescue duplicates. Distinct generic and specific patterns remain eligible for most-specific dispatch. Repeated imports of the same canonical body are not duplicate bodies, and incomparable applicable candidates fail at the call rather than selecting by order.

Required built-in guard release, owner-count maintenance, and native-borrow retention cannot be intercepted by an ordinary overload. A schematic `dispose(held: LockGuard<Payload>)` must not replace the genuine guard's protected operation merely because the author owns nested Payload. This is a protected-operation check, not the rejected blanket outermost-type placement rule for all copy/dispose implementations. #23/#28 must specify and validate that boundary, rather than assume private fields are sufficient or invent a new user-hook/mandatory-teardown protocol.

### Public requirements and union boundaries

```ts
function ready<T>(value: &T): bool;
function ready(value: &i32): bool { return true; }

export function check<T>(value: &T): bool where ready(T) {
  return ready(value); // Exported interface is rejected: ready is private.
}
```

Export the required family declaration to make this interface valid. A facade need not re-export it if its origin public path remains available. Importing check does not bind ready in the caller; metadata requirement checking is not source-level visibility.

A named private HiddenState result is valid as an opaque interface. In a separate case, reject exposing its private On/Off structure directly as a public return `On | Off`, or exporting that union declaration without exported variant identities and accessible discriminators. An opaque result cannot be narrowed/coerced by inspecting its hidden union layout. Private fields in otherwise-public variants keep their separate construction/pattern restrictions.

### Startup, roots, and cycles

Reject an expectation using ordinary start/demo runtime bindings even if their initializers could be constant-folded. Reject export of runtime lets. Type-check invalid imported startup code and invalid expectation bodies in normal builds too; skipping execution does not skip checking.

Reject origin-root escapes and application attempts to acquire trusted std identity. Inside std, unprefixed imports use the toolchain root, not the app root. A local file named std cannot replace that origin or its primitive operations. Exact toolchain physical layout is not an index/mod fallback.

Allow declaration cycles and anchored re-export cycles when identities resolve. Reject alias-only cycles with no declaration and impossible recursive value layouts. Check duplicate/ambiguous implementation behavior on the loaded graph, including test-only-used imports; later dead-code elimination cannot remove a module to change dispatch. #28 still owes complete resolution, metadata, and checking rules rather than a presumed proof from these examples.
