# Rill examples and case notes

[Rill sources and fixtures](../../prototypes/) are design examples, not an implemented compiler or standard library. Their case notes distinguish expected behavior from checks actually performed. Use the [RFC evidence labels](../rfcs/README.md#examples-and-evidence) when recording results.

The [consolidation record](consolidation.md) maps every combined example's purpose to its current source. It also lists the unchanged domain programs and explains corrections to historical syntax and library calls. Independent rejection snippets intentionally contain invalid code; they are not additions to an intended-valid program.

<a id="early-syntax-studies"></a>

## Combined language examples

| Topic | Source | Case notes |
| --- | --- | --- |
| Hello world, functions, recursion and top-level expectations | [01-basics.rill](../../prototypes/01-basics.rill) | The source includes independent expectation-failure and rejection examples in comments. |
| Static overloads, specialization and enum payloads | [03-overloads.rill](../../prototypes/03-overloads.rill) | [Overload and enum cases](03-overloads.md) |
| Named records, shape matching and owned rebuilding | [05-records-match.rill](../../prototypes/05-records-match.rill) | [Record and shape cases](05-records-match.md) |
| Mutation, borrowing and callable capabilities | [07-mutation.rill](../../prototypes/07-mutation.rill) | [Mutation cases](07-mutation-safety.md) |
| Worker queues, scoped helpers, guards and cancellation | [09-concurrency-safety.rill](../../prototypes/09-concurrency-safety.rill) | [Concurrency cases](09-concurrency-safety.md) |
| Modules, exports and privacy | [10-modules/](../../prototypes/10-modules/) | [Module cases](10-modules.md) |
| Numeric widths, text views and formatting | [11-numbers-strings.rill](../../prototypes/11-numbers-strings.rill) | [Numeric/text cases](11-numbers-strings.md) |
| Error wrapping, guards and Result pipelines | [13-caused-by-errors.rill](../../prototypes/13-caused-by-errors.rill) | [Error cases](13-caused-by-errors.md) |
| Stack and tail-call rules | [14-stacks-recursion.rill](../../prototypes/14-stacks-recursion.rill) | [Stack cases](14-stacks-recursion.md) |

<a id="feature-and-library-examples"></a>

## Library and application examples

| Topic | Source | Case notes |
| --- | --- | --- |
| Core library interfaces | [15-library-contracts.rill](../../prototypes/15-library-contracts.rill) | [Library cases](15-library-contracts.md) |
| Batch-report application | [16-batch-report/](../../prototypes/16-batch-report/) | [Workload and findings](16-batch-report.md) |
| Diagnostic/report fixtures | [17-diagnostics/](../../prototypes/17-diagnostics/) | [Fixtures and checker](17-diagnostics.md) |
| User conversions | [18-conversions.rill](../../prototypes/18-conversions.rill) | [Conversion cases](18-conversions.md) |
| JSON | [19-json.rill](../../prototypes/19-json.rill) | [JSON cases](19-json.md) |
| Streaming I/O | [19-streaming-io.rill](../../prototypes/19-streaming-io.rill) | [I/O cases](19-streaming-io.md) |
| Environment and arguments | [20-environment-inputs.rill](../../prototypes/20-environment-inputs.rill) | [Input cases](20-environment-inputs.md) |
| Filesystem | [20-filesystem.rill](../../prototypes/20-filesystem.rill) | [Filesystem cases](20-filesystem.md) |
| Secure random bytes | [21-secure-random.rill](../../prototypes/21-secure-random.rill) | [Random cases](21-secure-random.md) |
| SQL codecs and control | [22-sql-codecs/](../../prototypes/22-sql-codecs/) | [SQL cases](22-sql-codecs.md) |
| Temporal values | [23-time-values.rill](../../prototypes/23-time-values.rill) | [Time cases](23-time-values.md) |
| Clocks and cancellation | [24-clocks-cancellation.rill](../../prototypes/24-clocks-cancellation.rill) | [Clock/control cases](24-clocks-cancellation.md) |
| Networking | [25-networking.rill](../../prototypes/25-networking.rill) | [Network cases](25-networking.md) |
| TLS values | [27-tls/](../../prototypes/27-tls/) | [TLS cases and checker](27-tls.md) |

Historical numeric prefixes repeat. Use complete filenames, such as 19-json.rill and 19-streaming-io.rill, rather than a number alone. Current contracts and decisions are listed in the [documentation index](../README.md).

## Paths after consolidation

Case notes formerly stored as `prototypes/<name>.md` now live in `docs/prototypes/`. A grouped example's README now uses `docs/prototypes/<group>.md`. The consolidation record lists source merges and renames; all diagnostic fixture and application data paths are unchanged.

Standalone programs moved from `prototypes/backend-30/` to `experiments/backend-30/`, and from `prototypes/26-sqlite-provider/` to `experiments/sqlite-provider/`. Recorded command logs, hashes and ticket snapshots retain their historical paths. Current runbooks use the new layout.

## Executable experiments

[Backend](../../experiments/backend-30/README.md) and [SQLite provider](../../experiments/sqlite-provider/README.md) programs live in [experiments/](../../experiments/README.md). Their native/model execution evidence is distinct from the expected behavior of these Rill sources. Moving or editing a source example does not rerun an experiment or make it compiler-checked.
