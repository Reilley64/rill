# Caused-by error cases

Design artifacts for [ADR 0013](../docs/adr/0013-caused-by-errors-with-explicit-wrapping.md). The companion has six intended-valid expectations, checked by inspection only. There is no Rill compiler/runtime here, and neither those expectations nor this inventory have been executed. The provisional standard display path and complete Result interfaces belong to #23; #19 owns enum representation. These ordinary-record error payloads do not reopen union/literal policy.

## Expected behavior

| Case | Required outcome |
| --- | --- |
| `Ok(value) ? outer` | Yield value; evaluate the left side once and skip the RHS completely. |
| `Err(inner) ? outer` | Evaluate the RHS once, retain the complete inner wrapper as cause, and return the new failure early. |
| RHS performs effects, moves a value, or panics | Effects/move/panic happen only on failure; use a counter/resource fixture later to verify exactly once. |
| Typed inner and outer errors differ | Allowed; the enclosing Result has the outer error type and the cause retains the inner type. |
| User error has no message or cause field | Allowed with the canonical borrowed display overload. |
| Nested wrapping | Retain every cause and each original construction location. |
| Explicit `Err(outer, cause)` | Same cause-preservation contract; the new location is that Err construction site. |
| Postfix `result?` | Propagate the existing failure unchanged, with matching error type. |
| Ordinary `return result;` | No metadata mutation, source-location replacement, or trace append. |
| Display one payload | Use its display overload only; no required message or reflected type-and-fields dump. |
| Print/interpolate a wrapper | The standard renderer uses every payload formatter and includes the full cause chain and origins. |
| Format a borrowed wrapper repeatedly | Read access; no consumed payload/cause and no input borrow retained in output string. |
| Wrapping failure path | Later statements in the function are not executed. |

The success-only panic RHS in the companion checks lazy evaluation by inspection, not by execution. Its failed Results are tested only for success/failure; payload accessors, representation, source metadata queries, and exact output formatting are deliberately not invented.

## Independent rejection cases

- Infix wrapping on Option/none: it has no underlying error to attach. Existing postfix Option propagation stays.
- A non-Result left operand, a non-Result enclosing return contract, or an incompatible outer error type.
- An error payload without a matching canonical display overload, including missing `where display(E)` in a generic constructor. A same-spelled unrelated local function supplies no canonical evidence.
- Treating a bare type name or a handler function as an implicitly invoked RHS constructor. The operand is an ordinary error value expression; an explicitly called factory is fine.
- Postfix propagation between different error types: use explicit wrapping or explicit existing adaptation, not automatic conversion.
- Claiming a returned cause/wrapper borrow-free or transferable merely because the outer payload is, while a retained inner payload/cause carries borrowed or goroutine-affine state.
- Returning a wrapper whose outer value or retained cause borrows destroyed local storage, or freeing an owner while any node still depends on it.

## RFC and runtime validation obligations

#28 specifies complete grammar/precedence/associativity with #12, distinguishing postfix propagation from infix wrapping and parenthesizing compound pipes in examples until then. Spell out how an ordinary outer value (including #19 unit/payload enum values) is type-checked against the enclosing error contract. Wrapping is not Roc's mapper-callback form, a ternary, or recovery/defaulting.

#23/#28 must define/check owned heterogeneous cause representation, typed payload access, formatter retention/dispatch without a new public dyn/trait facility, helper construction-site attribution, copy/dispose and dependency/property evidence before monomorphization. Erasure must not hide inner borrows or unsafe transfer/sharing. Preserve origins when merely forwarding through generic functions and library combinators; explicitly creating a new Err gets a new origin.

Later executable tests must count left/RHS evaluations on both paths, check skipped moves and early-return cleanup, and preserve every source/resource through suspendable formatting, wrapping, disposal, and cancellation. Test selected-origin accuracy through helpers, mixed typed causes, long chains, shared/read-only formatting, and exact standard rendering. Full propagation traces and reflected formatter fallback must remain absent. Fatal panic trace/no-unwinding policy is independently unchanged.

If a representation or checking rule cannot implement these confirmed semantics consistently, return that v0-blocking question to the map rather than treating this inventory as a proof.
