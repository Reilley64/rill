# Caused-by error cases

Read the source examples in [`prototypes/02-caused-by-errors.rill`](../../prototypes/02-caused-by-errors.rill).

These examples follow [ADR 0013](../adr/0013-caused-by-errors-with-explicit-wrapping.md) and the [published Result/error contracts](../standard-library-contracts.md#4-option-result-origins-and-causes). The source combines the earlier port pipeline and caused-by examples. It declares ten intended-valid expectations, inspected but not executed by a Rill compiler or runtime.

The port example uses parseNumber, guards, pipes, andThen and map. Three expectations preserve successful port boundaries. A fourth checks that an invalid numeric input retains a parse-error cause. The remaining six preserve nested wrapping, explicit construction, success/failure and skipped panic behavior. Error payloads remain ordinary named records; no removed union or literal-type rule is restored.

## Expected behavior

| Case | Required outcome |
| --- | --- |
| `Ok(value) ? outer` | Evaluate the left side once, yield its value and skip the right-hand expression. |
| `Err(inner) ? outer` | Evaluate the right-hand expression once, retain the complete inner wrapper as cause and return the new failure early. |
| The right-hand expression has effects, moves a value or panics. | These actions occur only on failure. Later executable counter/resource tests must verify exactly-once evaluation. |
| Inner and outer error types differ. | The enclosing Result has the outer error type. The retained cause preserves the inner type and must satisfy the cause-retention requirements. |
| A user error has no message or cause field. | Accept it when canonical borrowed display is available. |
| Wrapping nests. | Preserve every cause and its original construction location. |
| Explicit `Err(outer, cause)` | Same cause-preservation contract; the new location is that Err construction site. |
| Postfix `result?` | Propagate the existing failure unchanged, with matching error type. |
| Ordinary `return result;` | No metadata mutation, source-location replacement, or trace append. |
| Display one payload | Use its display overload only; no required message or reflected type-and-fields dump. |
| Print/interpolate a wrapper | The standard renderer uses every payload formatter and includes the full cause chain and origins. |
| Format a borrowed wrapper repeatedly | Read access; no consumed payload/cause and no input borrow retained in output string. |
| Wrapping failure path | Later statements in the function are not executed. |

The lazy-success example places a panic in the skipped wrapping expression. Inspection predicts no panic, but no Rill execution verifies it. The port example uses the public cause accessor; it does not invent a digit/index parser API or depend on exact rendered text. The other failure expectations inspect success/failure rather than private layout or source-coordinate fields.

## Independent rejection cases

- Reject infix wrapping on Option because it has no error to attach. Postfix Option propagation remains valid.
- Reject a non-Result left operand, a non-Result enclosing return type or an incompatible outer error.
- Reject an error payload without canonical display, including a generic constructor missing `where display(E)`. An unrelated local namesake provides no canonical evidence.
- Reject treating a bare type name or handler as an implicitly called constructor. The wrapping operand is an ordinary error value. An explicit factory call is valid.
- Reject postfix propagation between different error types. Use explicit wrapping or adaptation.
- Reject a borrow-free or transferable claim based only on the outer payload when an inner value carries borrowed or goroutine-affine state. Cause retention must independently satisfy its property requirements.
- Reject returning a wrapper that borrows destroyed local storage, or releasing an owner while any node still depends on it.

## RFC and runtime validation obligations

#28 must formalize grammar, precedence and associativity with #12. Distinguish postfix propagation from infix wrapping and parenthesize compound pipes when that distinction would otherwise be unclear. Specify how ordinary outer values, including unit and payload variants, check against the enclosing error type. Wrapping is not a mapper callback, ternary expression, recovery operation or default value.

The catalogue defines typed payload access and a uniquely owned heterogeneous Cause without adding public dyn/trait or downcast facilities. #28 must validate its representation, retained formatter dispatch, helper construction-site attribution, copy/dispose behavior and dependency/property evidence before monomorphization. Cause retention requires display, noBorrow, sendable and shareable evidence for the inner value. Erasure cannot hide borrows or unsafe transfer/sharing. Generic forwarding and library combinators preserve origins; constructing a new Err records a new origin.

Later tests must count both operand evaluations on success and failure, check skipped moves and early-return cleanup, and retain sources through suspendable formatting, wrapping, disposal and cancellation. Test origins through helpers, mixed typed causes, long chains, repeated read-only formatting and standard rendering. Ordinary propagation must add no trace entries, and formatting must not fall back to reflected fields. The fatal-panic and no-unwinding rules remain unchanged.

If a representation or checking rule cannot implement these confirmed semantics consistently, return that v0-blocking question to the map rather than treating this inventory as a proof.
