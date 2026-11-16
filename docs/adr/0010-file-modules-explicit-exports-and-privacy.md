# File modules, explicit exports, and nominal privacy

[Issue #17](https://github.com/Reilley64/rill/issues/17) uses one source file per module, explicit item-site exports/imports, and defining-module privacy. Import aliases preserve canonical type/family identity, while opaque values and compiler metadata do not grant source access to private representations. This completes the module direction needed by ADRs 0003, 0008, and 0009; it does not implement a resolver/checker or accept the future feature RFCs.

## Files, roots, and loading

Each `.rill` file defines a module; directories supply path segments. No inline modules, automatic `index.rill`/`mod.rill` fallback, user project manifest, configurable search paths, or package resolution in v0. A directory without a module file is only a path prefix, not an importable facade.

The application source root is the entry file's directory. Unprefixed application imports start there; `self::` starts beside the importing file and `super::` at its parent, without escaping the origin root. `std` has a separate reserved toolchain root and explicit root/facade mapping. Unprefixed imports inside `std` use that root, replacing the application-root rule for standard-library source. Relative paths cannot escape it and standard lookup never falls back into application files. Application files with the same names gain no trusted standard identity. The physical toolchain layout is not a filename fallback rule.

Names/paths are case-sensitive. Canonicalize module identity across equivalent paths and aliases without merging distinct origins or declarations. Load the entry, fixed prelude dependencies, and transitive imports/re-exports once; do not compile unrelated files merely found on disk. Collect declarations before checking bodies. Declaration cycles can work; unresolved alias-only cycles and impossible recursive value layouts are errors. Later dead-code elimination cannot change overload dispatch or which implementations were collected.

## Exports, imports, and namespaces

Declarations are private to the defining module unless exported directly on the item:

```ts
// geometry/point.rill
export type Point = { x: i32; y: i32 };
export function origin(): Point {
  return Point { x: 0, y: 0 };
}

// api.rill
export import geometry::point::{Point as Position};
export import geometry::point as points;
```

Use `export function`, `export type`, and `export import`; no separate/bottom export lists, export-from form, wildcards, default imports/exports, or string-path imports. Single/group imports and `as` aliases remain. Only exported source bindings may be imported/re-exported; a public same-spelled type/function does not leak its private counterpart. Re-exports preserve original defining-module permissions and identity, and do not seal other public paths behind the facade.

Import paths start from an origin root or `std::`/`self::`/`super::`, not local aliases introduced by other imports. Aliases are for qualification in source expressions/types. Qualification traverses the imported module's exported bindings, not implicit filesystem autoload. Source modules can be imported directly by path, subject to item privacy; no project/parent-directory privilege grants access to a private declaration or field.

A grouped import `import api::{Widget};` selects exported bindings from the resolved `api` module, not a child file. Ungrouped paths support module/item resolution only when unambiguous under the namespaces. If `api.rill` exports type `Widget` and `api/Widget.rill` is another module, `import api::Widget;` has competing type/module identities and is an error, not filesystem or import-order precedence. Repeated paths to the same identity are deduplicated, not competing definitions.

Types/modules and values/functions have separate namespaces. The syntax `Point { ... }` looks up a type; `Point(...)` a function. A name exported in both spaces imports both public bindings, including under `as`. Same-namespace collisions require aliases or errors; imports do not partially ignore a collision. Inner lexical shadowing is allowed, but same-scope duplicate declarations are not, apart from valid overload declarations/implementations. Neither shadowing nor a similarly named application type changes compiler ownership hooks or trusted standard operations.

## Prelude and execution

A small fixed prelude provides primitive types, compiler predicates, and canonical `copy`/`dispose` overload declarations. Other standard-library names need imports. Compiler-generated cleanup uses the canonical disposal operation, not a shadowing local spelling. Source name lookup and compiler-required metadata access are separate: private values do not need source-importable type names merely to be disposed or checked.

No exported runtime `let` bindings or implicit module initializers in v0. Named functions remain non-capturing under ADR 0009. Parse and type-check loaded ordinary startup code and expectation bodies in both build/test modes. Code that will not run must still pass these checks. Normal execution runs only the entry file's startup. `rill test` instead runs independent expectations once per loaded module; expectations may use declarations/helpers, not capture ordinary startup bindings. Ordinary executables omit expectations and their test-only executable code. Imports used only by tests still participate in the static graph and family collection.

## Editor context is not a module-loading rule

[ADR 0016](0016-structured-diagnostics-and-complete-openidea-lsp.md) / #24 retain entry-directory origins, std isolation and loaded-graph family collection for editor analysis. A workspace search index cannot add implementations to a program. Explicit entry/target/options contexts stay separate; named loose application files can be identified standalone entries, but installed toolchain source keeps its read-only std origin. Editor settings are not a new Rill manifest.

Organize-imports preserves loaded modules, implementations, public bindings and expectations, not merely textual references. Navigation/rename respects canonical aliases, both namespaces, privacy and search bounds; viewing a private declaration is not permission to construct/use its representation. Full selected-client LSP delivery remains #42, not an implemented resolver or blanket visibility exception.

## Fields and opaque interfaces

Record fields are public by default. `private` fields are accessible only in the defining module:

```ts
export type Ticket = { private code: i32; label: string };
```

Outside the defining module, private fields cannot be read, changed, or bound by patterns. Any private field prevents direct whole-record construction or spread rebuilding, even if a valid old value exists. Use exported factories/updaters. Public field access still follows ordinary ownership/borrowing rules. An alias or a visible overload does not grant implementation-module field privileges.

A genuinely private nominal type may occur in an exported operation's signature as an opaque inferred value. Clients can hold, pass, dispose, and use visible operations on it, while the compiler retains identity/layout/property metadata. Clients cannot name or construct the private type, inspect its fields, or destructure its representation, even if the original field declarations default to public. Lack of a local binding for an otherwise-public type is not opacity; privacy belongs to the canonical defining declaration, not its import alias.

[ADR 0012](0012-enums-replace-unions-and-literal-overloads.md) (#19) replaces general unions with enums. An exported enum exposes all variants and payload fields under the enum's visibility; v0 has no individually private enum variants/payload fields. Public payload types must satisfy public-interface visibility rules. Origin public paths suffice, and aliases preserve canonical identity; facades need not separately re-export variants. A private enum returned through exported operations remains opaque, without variant inspection, representation narrowing, or structural conversion that bypasses opacity. Use a private enum or an ordinary record's private field when representation must be hidden. Ordinary record-field privacy is unchanged.

Ordinary overload families named as requirements in exported generic interfaces must themselves have publicly nameable exported declarations. Do not expose hidden family requirements in v0 public contracts. Importing an operation does not inject its requirement names into the caller's source scope; the compiler checks canonical metadata. This restriction does not prohibit opaque private value types or private implementation dependencies inside bodies.

## Later implementation and conversion refinement (#41)

[ADR 0017](0017-user-implementable-conversions.md) changes family bodies language-wide to `implement`, preserving declaration-wide visibility and canonical import identity. For the canonical conversion families only, destination/source pair patterns replace ordinary parameter-pattern keys and either endpoint's defining module can implement; reference-peeled outer nominal ownership is required. Identical pair patterns remain duplicates, concrete specializations can beat generic defaults, and incomparable applicable patterns are call ambiguities. Numeric pairs and infallible identity are protected. This does not change the ordinary orphan/duplicate rules or impose the rejected outermost-type rule on copy/dispose below.

## Overload identity, collection, and protected operations

An imported original declaration, including an alias, identifies an extended family; matching spelling does not merge unrelated functions. The declaring module controls the family's visibility and implementations inherit it. An extension's metadata participates wherever the family is visible, but extending it does not automatically re-export that module's private import binding: use `export import` for a new public name path. Implementations are not individually exported/hidden.

Every implementation in a loaded module contributes to its family. Preserve the ordinary orphan rule: the implementation module must own the declaration or a type occurring in its parameter types. Import/re-export aliases confer no ownership. Orphan placement limits extension but does not prevent all conflicts, contrary to ADR 0003's original rationale.

Reject distinct bodies with the same canonical family and parameter-type patterns during collection, even without a call. Normalize aliases and type-parameter renaming; return types do not create parameter-based overloads. Importing the same canonical body repeatedly does not duplicate it. Generic versus concrete patterns remain distinct and use most-specific dispatch; incomparable applicable matches produce call errors, never import-order selection.

Required built-in ownership mechanics are protected separately from ordinary replaceable default implementations. A genuine guard's release, owner-count maintenance, and native-borrow retention cannot be replaced by an ordinary overload, even when the broad orphan rule would otherwise permit a nested-payload specialization. Reject an implementation that would replace such a protected operation. The rejected blanket outermost-type rule for all `copy`/`dispose` implementations is not adopted; ordinary defaults remain specializable. [ADR 0015](0015-language-required-library-contracts.md) (#23) now records the explicitly confirmed canonical-action/overlap boundary and cleanup of remaining initialized owned fields after custom disposal. This additional field-cleanup rule was approved later, not already established by #17. #28 must validate it through wrappers, generics, erased interfaces and native states; private fields alone still do not solve interception.

## Alternatives and follow-through

- **Automatic module initialization/exported runtime globals:** rejected; startup and independent expectations remain distinct, with checked non-capturing named functions.
- **Bottom export lists/export-from/wildcards/defaults:** rejected in favor of item-site export and export-import.
- **Structural/contextual record identity or re-export-created type/family ownership:** rejected; ADR 0008's nominal rules remain.
- **Filesystem/import-order precedence, caller-specific overload subsets, or rejecting all declaration cycles:** not adopted.
- **Blanket outermost-type ownership-hook placement:** explicitly rejected. Duplicate definitions and protected runtime operations are separate checks.

`prototypes/10-modules/main.rill` and its reachable files illustrate the rules; `docs/prototypes/10-modules.md` records expected acceptance/rejection and resolver cases. They are hand-checked design artifacts, not executed tests. #23's [public catalogue](../standard-library-contracts.md) now records library/prelude/ownership interfaces and standard-library placement; #24 owns exact diagnostic presentation; #28 must reconcile source tickets and specify canonical resolution, metadata visibility, generic checking, duplicate/overlap rules, and protected-operation validation before implementation readiness.
