# Prototype consolidation

This change consolidates the examples from [the pre-consolidation revision](https://github.com/Reilley64/rill/tree/c8ca1cb/prototypes). It preserves their purposes, not every historical spelling or output string. The user authorized changes to the Rill programs and their semantics for this cleanup. The examples now follow the existing enum and library decisions; this change adds no language rule or RFC acceptance.

No Rill compiler is included. The examples below are inspected design programs. Source counting, link checks and Python model/fixture checks do not execute them.

## Combined examples

The former filenames identify programs in the linked revision, not current paths.

| Former program | Current program | Preserved purpose |
| --- | --- | --- |
| 01-hello-world.rill and 06-expect.rill | [01-basics.rill](../../prototypes/01-basics.rill) | Hello-world startup, pure addition, independent top-level expectations, a deliberately false expectation and separate bool/top-level rejection examples. |
| 03-overloads.rill | [03-overloads.rill](../../prototypes/03-overloads.rill) and [01-basics.rill](../../prototypes/01-basics.rill) | Integer, boolean and record formatting; generic List formatting and concrete List<Point> specialization. Recursive Fibonacci and its three expectations move to basics. |
| 12-enums.rill | [03-overloads.rill](../../prototypes/03-overloads.rill) | All 16 original enum expectations retain their order. They cover static variant selection, fallback, match narrowing, inferred/annotated types, joins, owned payloads, generic inference and mutation. |
| 05-records-unions-match.rill and 08-named-records.rill | [05-records-match.rill](../../prototypes/05-records-match.rill) | Nested/guarded matching, area and growth, owned collection mapping, named construction, inference, spread rebuilding, generic records and nominal/anonymous-record rejections. |
| 02-result-pipe-guard.rill and 13-caused-by-errors.rill | [02-caused-by-errors.rill](../../prototypes/02-caused-by-errors.rill) | Port parsing and bounds, reserved-port guard, Result pipes/combinators, error rendering, nested and explicit wrapping, exact-type propagation and skipped failure expressions. |
| 04-goroutines-channels.rill and 09-concurrency-safety.rill | [04-concurrency-safety.rill](../../prototypes/04-concurrency-safety.rill) | A producer and two workers share a receiver, transfer messages and close queues through disposal. A heartbeat competes with a timer and is explicitly stopped. Plain heterogeneous jobs compute an average. Fallible jobs distinguish all, race, any and allSettled. The original seven guard/cancellation expectations remain. |

## Corrections during consolidation

- Literal-type language/shop overloads now use enum variants. Overload selection is static; explicit matches branch on runtime variants.
- Shape unions now use explicit enum wrapping. Module examples preserve public payload identities and opaque private state without exposing a private enum representation.
- Named string bindings use explicit copy when both owners remain needed. Copy borrows its source; it does not fabricate an owner by dereferencing a view.
- Numeric/text examples use parseNumber, Bytes, decodeUtf8, decodeLossy and the canonical string byte view rather than provisional List decoders or invented parser metadata.
- Channel constructors specify capacity. Sender/Shared copies borrow their sources. Scoped helpers take fixed tuples, and plain jobs use join rather than the Result-only all.
- Borrowed List iteration yields references. The mutation and tail-call examples now dereference those scalar elements explicitly.
- Concurrency cases accept unspecified winners. They no longer infer a scheduling order from small sleep differences. Stop still joins execution and cleanup.
- Shape totals use a numeric interval instead of an exact float-output string. Greeting capitalization and shop/shape representations may differ from the historical demonstrations; their selection and matching purposes remain.

These corrections use the [current contracts](../README.md#contracts). Independent rejection snippets deliberately retain invalid forms and must remain separate from intended-valid source.

## Retained examples

The remaining programs keep their separate purposes:

| Program or group | Purpose and disposition |
| --- | --- |
| [06-mutation.rill](../../prototypes/06-mutation.rill) | Reborrowing, disjoint fields, callable modes, generic dependencies and retained mutable storage. All 13 expectations remain. |
| [07-modules/](../../prototypes/07-modules/) | Seven-file identity/export/privacy/startup graph. All 15 expectations remain; union layouts and borrowed-copy calls are corrected. |
| [08-numbers-strings.rill](../../prototypes/08-numbers-strings.rill) | All 41 numeric, UTF-8, view/dependency and ordered-formatting expectations remain with current library calls. |
| [09-library-contracts.rill](../../prototypes/09-library-contracts.rill) | The core library integration cases remain byte-identical. |
| [10-batch-report/](../../prototypes/10-batch-report/) | The application workflow and its two Rill files/data remain byte-identical. |
| [11-conversions.rill](../../prototypes/11-conversions.rill) | All 14 endpoint-selection and error-evidence expectations remain. Only an obsolete reservation comment was removed. |
| [12-streaming-io.rill](../../prototypes/12-streaming-io.rill) | Streaming ownership, partial effects and typed progress remain byte-identical. |
| [13-filesystem.rill](../../prototypes/13-filesystem.rill) | Lexical paths and uncalled filesystem workflows remain byte-identical. |
| [14-environment-inputs.rill](../../prototypes/14-environment-inputs.rill) | Owned startup data, ordering, byte/text boundaries and errors remain byte-identical. |
| [15-secure-random.rill](../../prototypes/15-secure-random.rill) | Empty production calls and explicit application fakes remain byte-identical. |
| [16-sql-codecs/](../../prototypes/16-sql-codecs/) | All three codec/control sources remain byte-identical. They are not database drivers. |
| [17-json.rill](../../prototypes/17-json.rill) | JSON grammar, ownership, precision, limits and error cases remain byte-identical. |
| [18-time-values.rill](../../prototypes/18-time-values.rill) | Temporal value and arithmetic cases remain byte-identical. |
| [19-clocks-cancellation.rill](../../prototypes/19-clocks-cancellation.rill) | Manual-clock, token and deadline cases remain byte-identical. |
| [20-stacks-recursion.rill](../../prototypes/20-stacks-recursion.rill) | All 14 tail, non-tail, indirect-call, local-retention and sized-spawn expectations remain. Scalar iteration and borrowed text copy are corrected. |
| [21-diagnostics/](../../prototypes/21-diagnostics/) | All 14 Rill fixtures retain their exact bytes, including CRLF, coordinates, intentional failures and independent rejections. The report/schema checks remain separate from compiler execution. |
| [22-networking.rill](../../prototypes/22-networking.rill) | Address/control values and uncalled network workflows remain byte-identical. |
| [23-tls/values.rill](../../prototypes/23-tls/values.rill) | TLS values and uncalled composition helpers remain byte-identical. No handshake or security validation is implied. |

The [case-note index](README.md) links the independent rejection inventories as well as the programs. Backend/provider programs remain under [experiments/](../../experiments/README.md), not mixed into these source studies. Their original recorded artifacts are unchanged.
