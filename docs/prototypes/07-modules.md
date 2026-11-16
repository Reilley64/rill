# Module, export and privacy prototype

Read [the source examples](../../prototypes/07-modules), which follow [ADR 0010](../adr/0010-file-modules-explicit-exports-and-privacy.md) and [ADR 0012](../adr/0012-enums-replace-unions-and-literal-overloads.md). The earlier union representations now use enums. These are inspected design artifacts, not compiler-executed tests.

A future tool would accept `rill run prototypes/07-modules/main.rill` or `rill test prototypes/07-modules/main.rill`. The entry file's directory is the application source root. The toolchain supplies std; the fixture does not contain its own standard library.

## Reachable files

| File | Purpose |
| --- | --- |
| main.rill | Entry code and public operations. |
| api.rill | Explicit re-exports of items and a module alias. |
| formatting.rill | The public show family and render requirement. |
| geometry/point.rill | A public type/function pair and an authorized family extension. |
| geometry/offset.rill | Relative self/super paths. |
| resources.rill | Private fields, opaque records and a private enum. |
| shapes.rill | A public enum wrapping public named records. |

All seven files are reachable from main. They declare 15 independent expectations, with 11 in main and one each in formatting, point, offset and resources. This count excludes standard-library expectations. Importing point through several paths must not duplicate its identity, implementation or expectations.

Selecting a nested file as the entry changes the application root. It is not an alternate command for this fixture. Resources is beside main and can be an entry under the same root. Its ordinary demo code is type-checked, but imports and test mode skip its execution.

## Intended-valid behavior

- Mark exports on declarations or import items. The facade re-exports items and a module alias without creating new type or family identities.
- Position imports both the record type and the helper function named Point. `Position { ... }` constructs the record; `Position(...)` calls the helper. OriginalPoint has the same canonical type identity.
- `api::points` reaches an exported module binding. The local points alias qualifies expressions; later imports still resolve canonical source paths rather than local aliases.
- `self::point` resolves beside offset. `super::formatting` resolves in its parent/root. A directory provides path segments, not an importable module facade.
- Importing a family declaration permits an authorized implementation to extend that family. Point's module owns the parameter type. Resources' private Hidden type can also participate without exposing its fields.
- Extending a family does not re-export its local import. Render's public requirement names formatting's exported show declaration. Main can use that metadata without acquiring a local show source binding.
- Ticket exposes label, but its private code prevents whole-record construction or rebuilding outside resources. Its factory, getter and same-module updater remain valid.
- Hidden and HiddenState are private canonical types. Callers can infer opaque values and use their public operations without inspecting the representations. Compiler metadata still knows their identities and properties.
- Shape is public and wraps public Square and Empty records. Empty remains public at its defining module even though api does not re-export its name. Main can match Shape.Empty without importing the payload type's name. Public representation permission and a local name binding are different things.
- Named functions and expectations do not capture the ordinary start/demo bindings. Normal startup prints only `(0, 0)` and `hidden 9`; imported resource demo code does not run. Test mode runs expectations instead of startup code.

## Independent rejection cases

These snippets describe separate changes or programs. They are not active compile-fail fixtures. #24 owns diagnostic messages; #28 must turn the cases into formal rules and executable tests.

### Export syntax and source lookup

```rill
export {Position}; // Rejected: no separate export lists.

import api as A;
import A::Position; // No canonical A module exists here; imports do not resolve aliases.
```

If a canonical A.rill existed, the second import would refer to it, not the local alias. An expression such as `A::origin()` instead uses that alias. Import order must not change module/item lookup.

Reject `import geometry;` because the fixture has a directory but no geometry.rill facade. Reject wildcard, default and string imports. Imports and re-exports select exported bindings, even when a private counterpart shares the spelling in another namespace. A same-namespace collision requires an alias; it cannot be silently ignored.

### Module versus item ambiguity

In a separate example, let api.rill export a Widget type while api/Widget.rill defines another source module. Reject `import api::Widget;` because the identities compete. `import api::{Widget};` selects the exported item explicitly. Rename conflicting paths or exports instead of selecting a filesystem winner. Reaching the same identity through two paths is not a conflict.

### Private fields and opaque representations

```rill
let forged = Ticket { code: 7, label: "x" }; // Rejected outside resources.
let issued = issue("x");
let rebuilt = Ticket { ...issued, label: "y" }; // Rejected outside resources.
let code = issued.code; // Rejected: private field.

let value = hidden(9);
let exposed = value.value; // Rejected: the private canonical type is opaque.
```

Reject importing or re-exporting Hidden, or matching its representation outside resources. By contrast, a public type merely absent from local imports is not opaque. Empty retains public construction permissions and an inferred Point retains public fields. Metadata and automatic disposal do not require exposing a private name or inserting an implicit source import.

### Family identity, duplicates and privacy

Reject a new show implementation in api whose only justification is that api re-exports Point. Re-exporting a type does not make the facade its defining module. Formatting's original family identity and the ordinary orphan rule still govern attachment.

Reject `points::show(...)`. Point's private import was not re-exported, although its implementation contributes to the public family. Use that family through an exported binding. A local namesake does not merge into it.

Reject distinct bodies in one family with identical parameter patterns, including bodies from different authorized modules. Normalize aliases and type-parameter renaming. Different return types do not rescue duplicates. Generic and concrete patterns can compete under the most-specific rule. Repeated imports of the same body are not duplicates. Incomparable applicable candidates fail at the call rather than selecting by import order.

Ordinary overloads cannot replace required guard release, owner-count maintenance or native-borrow retention. Owning a nested Payload does not authorize replacing disposal for a genuine `LockGuard<Payload>`. This is a protected-operation rule, not a blanket outermost-type placement restriction for all copy/dispose implementations. #28 must validate that boundary, not assume private fields prove it or invent an additional user-hook/mandatory-teardown protocol.

<a id="public-requirements-and-union-boundaries"></a>

### Public requirements and opaque enums

```rill
function ready<T>(value: &T): bool;
implement ready(value: &i32): bool { return true; }

export function check<T>(value: &T): bool where ready(T) {
  return ready(value); // Rejected public interface: ready is private.
}
```

Export the required family declaration to make this valid. A facade need not re-export it if its public origin path remains available. Importing check does not bind ready in the caller. Metadata checking does not bypass source visibility.

Returning private HiddenState as an opaque value is valid. Reject an external match on HiddenState.On or HiddenState.Off, including through aliases. General `On | Off` union syntax is no longer valid, even inside the defining module. A public enum exposes every variant and payload field; individual private variants or payload fields are not permitted. Ordinary public records can still have private fields, as Ticket demonstrates.

### Startup, roots and cycles

Reject expectations that capture ordinary start/demo runtime bindings, even when their initializers could be constant-folded. Reject exports of runtime lets. Normal builds still type-check imported startup code and expectation bodies; skipping execution does not skip checking.

Reject application-root escapes and attempts to acquire trusted std identity. Inside std, unprefixed imports resolve under the toolchain root rather than the application root. A local std file cannot replace that origin or its primitive operations. The toolchain's physical layout does not imply an index/mod fallback.

Allow declaration cycles and anchored re-export cycles whose identities resolve. Reject alias-only cycles without a declaration and impossible recursive value layouts. Check duplicate and ambiguous implementations across the loaded graph, including imports used only by expectations. Later dead-code elimination cannot change dispatch by removing a loaded module. Formal resolution and metadata validation remain #28 work.
