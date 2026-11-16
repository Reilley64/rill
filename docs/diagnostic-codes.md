# Diagnostic code registry. Version 1

[#24](https://github.com/Reilley64/rill/issues/24) confirms these assignments; see the [contracts](diagnostic-contracts.md) and [schema](diagnostic-report.schema.json). These initial assignments identify problem kinds, not exact prose, compiler stages or severities. Future distinct kinds receive new codes; published meanings are not recycled. This is not an exhaustive grammar/checker rejection catalogue or emitted compiler output. `rill explain <code>` provides these meanings offline; future additions require documentation, not a new language rule.

### R0001. Invalid syntax
A source construct cannot be parsed as permitted Rill syntax. Distinguish removed literal-type/general-union spelling from valid literal expressions and enum syntax in the explanation.

### R0002. Invalid individual literal
The literal itself is invalid for its established type/encoding/scalar range. Do not use for otherwise well-typed arithmetic that traps only if executed.

### R0003. Missing numeric type
No annotation, suffix or known parameter/return context establishes the necessary numeric width. Never choose a fallback width.

### R0100. Type or nominal identity mismatch
An operation has incompatible known types, including distinct named types or invalid access-mode/variant-parent conversion. Identify the actual identities and relationship, not only matching field shapes.

### R0101. Non-Boolean expectation
A top-level expectation expression is not bool. There is no truthiness conversion.

### R0102. Misplaced expectation
An expect declaration appears outside module top level.

### R0103. Unused Result
A Result is discarded without the explicit handling/intent permitted by #11. This is a hard language error, not the optional unused-binding warning policy.

### R0200. Use after ownership transfer
An owning value is used after a move/consuming operation. Show the original transfer and invalid use. Implicit copying is not a remedy.

### R0201. Conflicting access
A read/write/borrow conflicts with live exclusive or retained access. Show the access origin and why the restriction remains needed.

### R0202. Escaping dependency
A result or retained caller storage would carry a dependency on an invalid source, including callee-local storage. Show the actual source and dependency path.

### R0203. Required source released
A source/guard/owner would be disposed, replaced or moved while a dependent value/native operation still needs it. Do not shorten required retention for TCO.

### R0300. No applicable parameter pattern
No loaded implementation fits the call's independently established static parameter types/access. This is not ambiguity or return-only dispatch.

### R0301. Incomplete or ambiguous inference
Required type arguments/result context are missing or not unique. Suggest an appropriate annotation, not an arbitrary numeric/default candidate.

### R0302. Missing operation/property requirement
A selected operation or generic body lacks required evidence, such as canonical family/callable/copy/noBorrow/sendable/shareable/display evidence. Identify the real inner dependency and distinguish it from an overload tie-breaker.

### R0303. Incomparable applicable implementations
Distinct applicable parameter patterns lack a unique most-specific implementation. Do not choose by import order, return type or constraint truth.

### R0304. Duplicate canonical implementation
Distinct ordinary bodies have identical canonical parameter patterns during collection, even if aliases, generic parameter names, returns or constraints differ. Repeated routes to the same body are deduplicated instead.

### R0310. Duplicate canonical conversion pair
Under the later #41 decision, distinct conversion bodies have the same canonical destination/source pair pattern. E does not distinguish bodies; permitted generic/specific pairs are not automatically duplicates.

### R0311. Protected conversion overlap
A user implementation displaces protected numeric conversion behavior or the standard infallible identity rule. This is a conversion-specific boundary, not sealing all ordinary families.

### R0312. Unauthorized conversion endpoint ownership
An implementation is outside the canonical family module and both reference-peeled outer nominal endpoint owners. Local error types, nested local payloads, imports, constraints or generic parameters do not grant this conversion permission. Ordinary orphan rules are unchanged.

### R0313. Selected conversion error mismatch
A concrete caller's E is incompatible with the winning conversion implementation's selected E. Do not select a fallback to satisfy E or assume an overridable generic default's error. Generic evidence preserves symbolic E.

### R0314. No applicable conversion pair
No implementation fits the established destination/source pair. Targets need explicit or immediate unique concrete context, and source widths must be independently established. Do not guess targets from candidate counts, defaults or later uses, or chain conversions automatically.

### R0400. Unresolved name or import
A required source binding/import cannot resolve under the actual origin and graph. Hidden metadata is not a source import.

### R0401. Inaccessible declaration or representation
A declaration/field/opaque representation is private or not constructible from the use site. Import/re-export aliases do not grant access.

### R0402. Ambiguous module/item path
Distinct module/item identities compete in an import namespace; explain grouped disambiguation rather than filesystem precedence.

### R0403. Namespace binding collision
Distinct same-scope bindings collide in the same namespace. Preserve the separate type/module and value/function namespaces and coupled aliases.

### R0404. Import origin escape
A path attempts to escape its bounded application/std origin or apply an invalid root rule.

### R0405. Unanchored alias cycle
Alias-only resolution cannot acquire a declaration identity; distinguish valid anchored declaration cycles.

### R0406. Impossible value layout
Recursive value structure cannot have a valid finite layout; source cycles alone are not the error.

### R0500. Protected ownership overlap
A non-trusted implementation could replace/tie a required canonical ownership action, including nested-payload/generic/alias routes. Check this even without calls. It does not seal all defaults.

### R0501. Forbidden capability
An operation attempts a negative/prohibited capability such as copying/forging/transferring a genuine guard. An added user overload/annotation cannot manufacture it.

### R0502. Runtime-let capture by a noncapturing declaration
A named function or expectation reaches enclosing startup/runtime lets without explicit permitted inputs. Constant folding does not grant capture.

### R0600. Unsupported eligible-tail lowering
A semantically eligible transfer cannot be honored by the selected ABI/backend. Identify the call and target; do not silently relabel the transfer as ordinary. An ineligible final-looking call is not this error.

### R0601. Invalid build setting
A build-time option violates its contract (capacity bounds/alignment/representation or saved-state option on native, for example). A dynamically invalid spawn capacity remains runtime panic, not this static diagnostic.

### R0602. Unavailable target or analysis context
The requested target/context/toolchain facts cannot be supplied. Distinguish configuration failure from source invalidity and later unsupported call lowering.

### R0603. Invalid driver invocation
Arguments, report destination mode or code lookup are invalid. Explain usage without a fabricated source location.

### R0604. Driver I/O/report failure
Reading or writing the requested inputs, artifacts or report failed. This is a nonzero operation failure, not successful analysis or automatically an internal compiler defect.

### R0700. Unreachable executable statement
Required flow analysis establishes that a statement cannot execute. This is advisory only. Declarations are not runtime statements, and unreachable code is still checked.

### R0701. Safely proven runtime trap
This optionally explains an otherwise well-typed operation that would trap if executed. It does not require a constant evaluator or change acceptance. Do not confuse IEEE behavior or infallible rounding with a runtime trap.

### R9000. Internal tool failure
The compiler/server cannot trust its own analysis operation/state. This is not a source type error, successful checking or a caught Rill panic. Controlled reporting is best effort; fatal host failure may produce no report.

## Compatibility and examples

All compiler/driver severities use R codes; there is no E/W prefix dependency or v0 Werror promotion. False expectations use `expectationResult`, not one code per expression; ordinary application Err/panic formats remain #23/#22 contracts. Codes, schema and representative [prototype 21](prototypes/21-diagnostics.md) examples are static design artifacts, not an implemented emitter. Exact #28 rules can add documented kinds without recycling these meanings.
