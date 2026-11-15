# Overloads need a declaration and resolve at compile time

Rill has no traits, so overloading carries per-type behavior. To keep that checkable, a name can only have overloads after a body-less overload declaration, such as `function show<T>(value: &T): string;`, fixes their shape. Every overload is the declaration with some types made more specific, and every call resolves to exactly one overload at compile time. A generic overload names what it needs with `where show(T)`, an overloaded name passed as a value takes the declaration's type, and overloads may only be written in the declaration's module or in a module that defines a type in their parameters, which limits extension without by itself preventing all conflicts.

## Enum-variant refinement (#19)

[ADR 0012](0012-enums-replace-unions-and-literal-overloads.md) supersedes primitive literal types/overloads and the unknown-runtime-value error rule. A statically known enum variant can select its variant implementation; a parent-enum argument selects an applicable general implementation, never a runtime dispatcher. Existing all-parameter specificity, declarations, generics, family identity, and collection rules remain.

## Library requirements and closed operations (#23)

[ADR 0015](0015-language-required-library-contracts.md) and the [public catalogue](../standard-library-contracts.md) provide callable/callableMut/callableOnce predicates, ordinary canonical iterate/next families, loose collection family shapes, and distinct plain join/fallible all APIs. Fixed tuple arities do not add variadic generics. Closed numeric conversion/parsing operations are not user return-only overloads. Canonical Err construction is an explicitly approved location-capturing one-/two-argument exception, not a general relaxation of ordinary parameter-count rules; adapt it with an arrow for first-class use.

## Module and conflict rules

[ADR 0010](0010-file-modules-explicit-exports-and-privacy.md) makes imports/re-exports preserve canonical family identity. Implementations inherit declaration-wide visibility, but an extension does not implicitly re-export its local import binding. Collect implementations from the loaded module graph once; reject distinct bodies with identical canonical parameter patterns during collection, and reject incomparable applicable matches at a call. Generic/specific patterns remain distinct and import order never chooses a winner. Orphan placement alone is not a coherence proof.

Public generic requirements must name exported family declarations. Required built-in ownership mechanics are separately protected against replacement; the blanket outermost-type restriction proposed for all copy/dispose overloads was rejected. Ordinary defaults remain specializable under the existing orphan rule, subject to duplicate and protected-operation checks.

[ADR 0011](0011-explicit-numeric-widths-and-utf8-string-views.md) adds closed compiler-known numeric conversions: from/into and try_from/try_into are two canonical operations with alias names and destination/source type arguments, not ordinary bodies distinguished by return types. Generic availability requirements are checked before monomorphization. Canonical template show uses the ordinary public family/orphan rules; same-spelled unrelated families do not affect interpolation. #19 still owns literal-overload policy.

## Considered options

- **Undeclared overloads, checked per instantiation (C++ templates, Zig).** No new syntax, but a generic signature doesn't say what it needs, and errors appear inside generic bodies. #5 found that inferring the requirements is NP-hard.
- **Run-time checks for literal overloads.** #2 first allowed `fib(n: 0)` to be chosen at run time when the value isn't known. Rejected in favor of compile-time selection. #10 initially also rejected broad runtime arguments when a literal implementation might match; #19/ADR 0012 supersedes that rule and removes primitive-literal overloads. Use `match` for runtime value/variant branching.
