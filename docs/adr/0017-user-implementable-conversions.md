# User-implementable conversions and explicit family implementations

[Issue #41](https://github.com/Reilley64/rill/issues/41) records the human-confirmed conversion contract and language-wide `implement` syntax. Extend the closed numeric operations from #18/#23 without general return-type overload dispatch, traits, implicit casts or changing numeric semantics. The interview and consolidated contract were explicitly confirmed. This is a design decision, not an implemented compiler/library, executable acceptance suite or safety proof. Number 0016 is reserved by #24's separately prepared diagnostics decision in map #1.

## Language-wide implementation syntax

A body-less `function` declares a family; `implement` supplies a body for an existing canonical family. Standalone functions still use `function`. Implementation-owned generic binders precede the family name; explicit family arguments follow it. Infer omitted family arguments from the signature, never the body. Implementations obey the declared shape; inferred/written arguments and return types must agree.

```rill
export function show<T>(value: &T): string;
implement show(value: &UserId): string { /* body */ }
implement<T> show(value: &Box<T>): string where show(T) { /* body */ }
implement into<UserId>(value: string): UserId { /* body */ }
implement tryInto<UserId>(value: &string): Result<UserId, InvalidUserId> { /* body */ }
```

The last two resolve imported canonical conversion declarations. Source and error arguments are inferable from the signature; destination is explicit in conversion registration. Imports/re-exports/aliases preserve identity; a same-spelled unrelated family never registers or supplies evidence. Implementations inherit family visibility and are not individually exported/hidden. Public generic requirements name exported canonical families. Existing prototypes migrate genuine family bodies, not standalone helpers. This syntax applies language-wide; it does not change ordinary parameter-pattern duplicate keys, orphan rules, return patterns or dispatch.

## Canonical public declarations and aliases

```rill
// std::convert; explicit imports, not the fixed prelude
export function into<To, From>(value: From): To;
export function tryInto<To, From, E>(value: From): Result<To, E>;
// from aliases into; tryFrom aliases tryInto, preserving identity and argument order.
```

`tryInto`/`tryFrom` replace historical `try_into`/`try_from`, not additional implementation sets or retained compatibility spellings. `from` is an alternative spelling, not a reversed type-argument order. The canonical family imports its Result signature metadata; source examples using Result still import its binding normally.

Only these canonical conversion operations use a `(To, From)` implementation key. Multiple targets for one source are distinct entries. `E` is determined output evidence of the selected fallible implementation, never an additional dispatch key. Ordinary families still reject duplicate parameter patterns regardless of returns/constraints. No traits, associated-type projection, reflection, public dyn or general return-only overload exception is introduced.

## Calls, inference and first-class use

Use `into<Target>(value)` or `tryInto<Target>(value)`; omitted trailing source/error arguments are inferred. An immediate concrete unique expected type can establish the destination, including the success type of an expected Result. Do not choose a destination because only one conversion is loaded, infer it from later statements, default it, chain conversions or guess source widths. `into<u8>(10)` still lacks an independent numeric source width. After endpoint selection, an expected error must match the selected error; it cannot select a fallback.

A conversion can be a function value when explicit arguments or an immediate function-type context establish endpoints and the exact selected signature/error. An unresolved family is not a runtime converter. Existing ownership/callable restrictions apply; this introduces no runtime registry or erased conversion facility.

## Collection, ownership permission and specificity

Collect loaded-module implementations once, preserving canonical aliases and deduplicating repeated paths to the same body. Normalize aliases and generic parameter renaming. Reject distinct identical conversion pair patterns during collection even without a call; differing requirements or error types do not rescue duplicates.

The defining module of the canonical family, source endpoint or destination endpoint may implement. For endpoint permission, peel reference layers and inspect the outer nominal type: a local `Box<T>` counts; `ForeignBox<Local>` does not. A bare generic parameter, local error type, constraint, import or re-export grants no endpoint ownership. This conversion-specific endpoint rule does not replace ordinary families' parameter-based orphan rule or impose an outermost-type rule on ordinary copy/dispose.

Among matching implementations with satisfied requirements, choose the unique most-specific source/destination pattern. Specific `Box<UserId> -> string` beats generic `Box<T> -> string`; generic entries serve as defaults. Incomparable applicable patterns cause a call ambiguity. Constraint strength, expected error, import order and dead-code elimination do not rank candidates. A selected implementation's error mismatch is a diagnostic, not fallback selection.

All built-in numeric pairs remain standard-owned/protected. Standard `into<T,T>` identity is protected and takes precedence whenever endpoints are equal, even if a generic user wrapper pattern also matches. Explicit user identity registrations are rejected; a generic wrapper remains usable on its nonidentity pairs. Identity returns its input with ordinary move/implicit-copy/reference rules; it does not clone backing storage or duplicate exclusive references.

## Generic evidence and implementation-defined errors

```rill
function convert<To, From>(value: From): To where into(To, From) {
  return into<To>(value);
}
function attempt<To, From, E>(value: From): Result<To, E>
  where tryInto(To, From, E), display(E) {
  return tryInto<To>(value);
}
function stringify<T>(value: T): string where into(string, T) {
  return into<string>(value);
}
function stringifyBorrowed<T>(value: &T): string where into(string, &T) {
  return into<string>(value);
}
```

Each canonical requirement relates both endpoints. Calls and registrations can omit source arguments because a value parameter supplies them; a standalone requirement has no such value. `tryInto(To, From, E)` promises that normal selection for these endpoints produces exactly E. Generic bodies are checked before monomorphization against symbolic signature/access/property/dependency evidence. Concrete callers must establish those requirements against the winning implementation. E can be inferred from that selected implementation, not supplied to choose an alternative error.

A generic body must not normalize an overridable fallback's error to a concrete type and later execute a specialization with another error. Preserve E symbolically or demand exact equality as a requirement; an incompatible instantiation fails evidence checking. Monomorphization does not repair missing evidence or reinterpret a checked body. [Rust research](../research/0041-q19-rust-conversion-evidence.md) explains the associated-error analogy and specialization hazard: stable Rust rejects these overlapping implementations, and unstable specialization is not a proof of Rill's rule. #28 must formalize candidate selection, equality/inference and evidence transport.

Custom E uses normal canonical display evidence for Err construction, and generic examples state `display(E)` explicitly. Postfix `?` forwards only the exact expected E with unchanged metadata; use explicit mapErr or causal wrapping to change it. No mandatory error fields, implicit conversion, new propagation origin or relaxation of cause-retention properties is added.

A user `into` does not automatically provide `tryInto`; explicitly register both if desired. There is no universal fallible identity or new uninhabited error solely for it. Standard numeric fallible pairs, including numeric identity/infallible pairs, retain Result with ConversionError and return Ok where infallible. Other fallible identity entries need explicit registration and a concrete error; they do not replace protected infallible identity.

## Numeric, ownership and application boundaries

Numeric full-base-range availability, checked narrowing, float rounding/possible infinity and truncation-before-range-check remain unchanged. Infallibility is not losslessness. ConversionError remains the numeric OutOfRange/NotFinite error, not a universal user-conversion error. Parsing remains explicit parseNumber with ParseError; it is not automatically registered. No as conversion/assertion syntax or default widths.

`From = T`, `&T` and `&mut T` are distinct owned/read-only/exclusive contracts. Borrow acquisition and copying are explicit; no automatic borrowed adapters or moves out of noncopying borrowed data. Destinations and errors retain actual dependencies, including nested references carried by owned inputs. Reject references into destroyed local source storage; preserve externally backed dependencies and exclusive-access restrictions. Conversions cannot fabricate noBorrow/sendable/shareable or extend lifetimes.

Implementations are ordinary functions: they may allocate, have permitted effects, suspend or mutate explicit mutable inputs. Infallible means a direct result rather than recoverable conversion failure, not purity, termination or immunity to fatal panic.

No argument coercion, implicit conversion chains, display/show registration, structural record mapping, serializer, SQL-expression capture or database mapping follows from registration. Explicit application JSON conversions may use their own typed errors. Preserve #31's distinct JSON numeric extraction/construction semantics rather than importing numeric-to-numeric rounding rules.

## SQL/JSON integration (#40)

[ADR 0021](0021-partial-standard-sql-contract.md) and the [SQL contract](../sql-contracts.md) explicitly add named bindInto/tryBind/get helpers invoking canonical conversions under declared evidence. This is not automatic enrollment, implicit chaining or another return-only overload exception. User SQL Parameter/JSON Value codecs live in endpoint-owning modules; Into/TryInto, ownership and selected-error relationships remain distinct. #33 temporal types now join the SQL subset with exact checked mappings, not automatic time/string/zone conversions. #43 retains native-provider work; these examples do not implement a serialization framework or overwrite separate JSON/time drafts.

## Supersession and evidence

This supersedes #10/#17's family-body spelling language-wide, and narrowly extends their conversion duplicate/orphan key rules. It supersedes #18/#23's closed numeric-only extension boundary, snake_case aliases and fixed error across the whole fallible family, not their numeric behavior or numeric ConversionError. Ordinary family rules, nominal records/enums, explicit ownership and error/cause contracts remain.

[Prototype 18](../../prototypes/18-conversions.rill) and its [independent case inventory](../../prototypes/18-conversions.md) cover the public contract. Existing prototypes are migrated for family syntax and conversion names; unrelated explicitly historical constructs remain historical. #28 owns formal inference/coherence/borrow validation and conversion RFC coverage. Any unworkable checking mechanism returns to map #1, not a claimed proof or permission to silently restrict the approved design.
