# Caused-by errors with explicit wrapping

**Later interface and safety refinement:** [ADR 0015](0015-language-required-library-contracts.md) (#23) retains this origin/explicit-cause model but requires display/noBorrow/sendable/shareable when erasing a payload as a retained cause. Primary borrowed errors may still propagate directly. It defines opaque Err<E>/Cause, closed canonical construction, explicit default `error(message)` and non-copyable standard envelopes; older message conveniences/layout sketches below are historical where they conflict. No propagation traces are restored.

[Issue #29](https://github.com/Reilley64/rill/issues/29) confirms that recoverable errors carry causes automatically, including user-defined typed errors, but propagation is not a report-building operation. Keep `Result<T, E = Error>`: `Err` carries the typed value, its construction location, and an optional cause of any other error type. Messages and special fields/interfaces on user types are not required. This replaces the propagation-trace decisions in #2, #7, #9, #11, and #13; cause support remains universal rather than moving exclusively into the default `Error` type.

## Values, causes, and formatting

Keep `Err(value)` and `Err(value, cause)`. The latter owns the original error wrapper, preserving its value, construction location, and complete underlying chain even when its error type differs from the new outer value's type. Constructing an `Err` captures its source location, not a full stack backtrace. Postfix `?` and ordinary returns propagate the existing wrapper unchanged: no new location, trace append, or cause node.

A type used as an error must support the canonical standard overload declaration:

```ts
function display<T>(error: &T): string;
```

Each error type provides its own implementation; generic error construction requires `where display(E)`, checked before monomorphization. The default `Error` supplies an implementation. There is no required message field, automatic reflected record dump, per-instance formatter argument, or new trait. Normal family identity, visibility, and orphan rules apply. The library path belongs to #23; this is not another implicit prelude name.

`display` formats only its typed error value. Standard formatting renders that text, its construction location, and every caused-by node recursively, using each node's own formatter. Ordinary printing/interpolation of the wrapper shows the full chain without converting it into a separate report. The canonical `show` interpolation family from ADR 0011 remains unchanged; #23 supplies the standard wrapper-formatting bridge to `display`. Formatting borrows rather than consumes error values, and output text retains no input borrow. As with ordinary formatting, existing suspension/effect, evaluation-order, and cleanup rules apply.

## Caused-by wrapping expression

`inner ? outer` is a Result-only early-return expression. Its right operand is an error **value expression**, not an implicitly invoked constructor or error-mapping callback:

```ts
let config = readConfig(&path) ? ConfigLoadFailed;
```

Here `ConfigLoadFailed` denotes an error value under the ordinary name/constructor rules. Payload-bearing error expressions also work; this syntax adds no special bare-type-name construction rule.

1. Evaluate `inner` exactly once.
2. On success, yield its success value. Do not evaluate or move `outer` on that path.
3. On failure, evaluate `outer` exactly once, construct an `Err` with that value at the infix `?` source location, attach the complete original error wrapper as its cause, and return early from the enclosing function.

Conceptually, the example means:

```ts
guard let config = readConfig(&path) else (cause) {
  return Err(ConfigLoadFailed, cause);
}
```

The success type comes from `inner`; the returned error type comes from `outer` and must fit the enclosing function's Result error type. Inner and outer error types may differ; the cause preserves the inner type rather than implicitly converting it. The enclosing function must permit that Result early return. Postfix `inner?` still requires matching error types and propagates unchanged. Option's existing postfix `?` stays; infix caused-by wrapping is not defined on `none`. Explicitly convert an Option to a Result when an error is needed.

Roc's infix `?` inspired the shape, but Roc calls a mapping handler. Rill deliberately takes a lazy error value and automatically retains the cause instead. Compound pipe/operator examples should parenthesize their operands until #28 specifies the complete grammar, precedence, and associativity alongside #12; no ternary or handler form is introduced.

## Alternatives and follow-through

- **Automatic report/propagation traces:** rejected. Moving through a function adds no meaningful error context; explicit wrapping adds the context and preserves the cause.
- **Cause support only on default Error:** rejected. User-created typed errors get the same caused-by support, with no required message.
- **Handler on the right of infix ?:** rejected in favor of a simple error value. Explicit guard handling remains available when the outer value must inspect the inner payload.
- **Automatic record formatting:** rejected in favor of the explicit borrowed `display` contract. The standard renderer, not each custom formatter, traverses the chain.

#23 defines complete Result/Err/default-Error interfaces, typed payload access, heterogeneous cause ownership/formatting, canonical display/show integration, and existing message conveniences. Reconcile #19's enum representation rather than reviving old inline/tagged unions. #24 owns diagnostics for invalid wrapping and missing display evidence; runtime error rendering belongs to #23, not an expansion of compiler-diagnostic scope.

#28 must specify/check source-location attribution through helpers, early-return typing and grammar, and owned heterogeneous causes without losing borrowing, copy/dispose, noBorrow/sendable/shareable, or formatter evidence through generic/erased interfaces. Owning a wrapper does not prove its payload or causes borrow-free. Do not infer transfer safety from the outer error alone or introduce a public dyn/trait mechanism as an unstated consequence. Validate single evaluation, lazy right-hand effects/moves, nested causes, cancellation/resource cleanup, and unchanged postfix/return behavior. Any inconsistent required contract returns to the map as a v0-blocking question.

[The error examples](../../prototypes/13-caused-by-errors.rill) and [case inventory](../prototypes/13-caused-by-errors.md) are hand-checked design artifacts, not an executed compiler/runtime suite or a safety proof. ADR 0005's fatal-panic message/trace and no-unwinding policy are unchanged. RFC acceptance remains separate from this confirmed design decision.
