# Records, shapes and matching

[The source](../../prototypes/05-records-match.rill) combines the earlier shape and named-record examples. Its 13 intended-valid expectations have been inspected, not compiled or executed.

## Source coverage

Shape is a closed enum that explicitly wraps the named Circle and Rect records. Matching selects a variant; record patterns inspect its payload. The source retains the nested origin pattern, guarded large-circle and square cases, a wildcard fallback, area calculation and owned map/grow/sum workflow.

Growing a shape rebuilds its named payload with a spread. The total-area expectation uses a narrow numeric interval instead of promising a particular decimal rendering of a binary float. Primitive formatting remains covered by the [numeric/text examples](08-numbers-strings.md).

The named-record cases cover construction, an inferred value from a typed function, an explicitly annotated binding, named spread rebuilding and a generic Box. Point now uses f64 so the same record supports geometry. ScreenPoint has identical fields but remains a different nominal type.

## Independent rejection cases

The source comments retain separate invalid examples. Reject anonymous record values, including those with an expected Point type or passed by borrow. Reject an anonymous structural parameter type and an unnamed spread rebuild. Reject substituting ScreenPoint for Point merely because their fields match.

General union declarations and literal-type tags remain invalid. An enum wraps a record explicitly; matching fields do not create enum membership. These cases follow [named construction](../adr/0008-records-require-named-construction.md) and the later [enum decision](../adr/0012-enums-replace-unions-and-literal-overloads.md).

Expected rejections are not executable compiler tests. The [diagnostic registry](../diagnostic-codes.md) owns problem kinds, and ri-48g still owes formal checking and execution evidence.
