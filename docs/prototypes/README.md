# Rill examples and case notes

[Rill sources and fixtures](../../prototypes/) are maintained examples. Use the language rules and API names for the current milestone. When an approved change lands, update the affected examples. Keep old source and observed results at their recorded revisions. Use the [RFC evidence labels](../rfcs/README.md#examples-and-evidence) to distinguish expected behavior from verified behavior.

The [consolidation record](consolidation.md) maps combined examples to their original purposes. Independent rejection snippets intentionally contain invalid code. Do not include them as entry files in the prototype build.

M6 ri-qlh tracks the first [prototype build and test targets](../../tests/README.md#prototype-build-and-test-targets). Add intended-valid entry files as their required features become available. Record why a program is not yet in that set. Do not report it as passing.

## Core conversion interface migration

The 2026-11-16 interface direction (ri-48g, comment 6913810445) consolidates core conversions under into/tryInto. [Draft RFC 0006](../rfcs/0006-numbers-strings-and-primitive-operations.md#canonical-core-conversion-interface) records the current matrix and migration rules. Strict numeric parsing selects ParseError; strict UTF-8 decoding selects Utf8Error.

Groups 02, 08, 09, 10, 14 and 16 use the canonical interface: `tryInto<N>(&text)`, `tryInto<string>(&bytes)`, `tryInto<&string>(&bytes)`, `into<&Bytes>(&text)`, `into<u32>(value)`, `tryInto<char>(value)` and `into<Bytes>(values)`. Groups 12, 13, 15, 23, 24 and 25 still build Bytes with the removed `Bytes([...])` factory. Migrate them to `into<Bytes>` before executable acceptance. Preserve the examples' intended behavior. Keep obsolete spellings in revision history and recorded reports, not in maintained examples. Changes to the sources do not change earlier verification results.

## Entry files

The [prototype status table](../../crates/compiler/tests/m5/prototype-status.tsv) loads one entry per program: hello-world; each top-level `prototypes/*.rill`; the `main.rill` of a directory that has one (07-modules, 10-batch-report, 16-sql-codecs); otherwise every top-level `.rill` of the directory except `rejections/`. The entries of 21-diagnostics are `empty.rill`, `expect-failures.rill`, `locations.rill`, `valid.rill` and `warning.rill`. The entry of 23-tls is `values.rill`. Regenerate the table with `mise run status:prototypes`.

## Numbering

Number groups consecutively by the authored date of their first source-addition commit in main's history. A consolidated example takes the earliest date of its constituent programs, not its merge or rename date. For equal dates, retain the original numeric order, then sort by filename. Renaming a group does not restart its chronology.

Keep a group's source and case-note numbers aligned. Update the index, references and checker commands together when renumbering. Prototype numbers order examples; they are not issue, ADR or RFC IDs.

<a id="early-syntax-studies"></a>
<a id="feature-and-library-examples"></a>
<a id="combined-language-examples"></a>
<a id="library-and-application-examples"></a>

## Chronological index

| Source | Purpose and case notes | First source commit |
| --- | --- | --- |
| [01-basics.rill](../../prototypes/01-basics.rill) | Hello world, functions, recursion and independent expectations. The source comments include expectation failures and rejections. | [e8977c3](https://github.com/Reilley64/rill/commit/e8977c3) |
| [02-caused-by-errors.rill](../../prototypes/02-caused-by-errors.rill) | [Error wrapping, guards and Result pipelines](02-caused-by-errors.md). | [e8977c3](https://github.com/Reilley64/rill/commit/e8977c3) |
| [03-overloads.rill](../../prototypes/03-overloads.rill) | [Static overloads, specialization and enum payloads](03-overloads.md). | [e8977c3](https://github.com/Reilley64/rill/commit/e8977c3) |
| [04-concurrency-safety.rill](../../prototypes/04-concurrency-safety.rill) | [Worker queues, scoped helpers, guards and cancellation](04-concurrency-safety.md). | [e8977c3](https://github.com/Reilley64/rill/commit/e8977c3) |
| [05-records-match.rill](../../prototypes/05-records-match.rill) | [Named records, shape matching and owned rebuilding](05-records-match.md). | [e8977c3](https://github.com/Reilley64/rill/commit/e8977c3) |
| [06-mutation.rill](../../prototypes/06-mutation.rill) | [Mutation, borrowing and callable capabilities](06-mutation-safety.md). | [aafdf60](https://github.com/Reilley64/rill/commit/aafdf60) |
| [07-modules/](../../prototypes/07-modules/) | [Modules, exports and privacy](07-modules.md). | [34f5fa2](https://github.com/Reilley64/rill/commit/34f5fa2) |
| [08-numbers-strings.rill](../../prototypes/08-numbers-strings.rill) | [Numeric widths, text views and formatting](08-numbers-strings.md). | [4ec9b0e](https://github.com/Reilley64/rill/commit/4ec9b0e) |
| [09-library-contracts.rill](../../prototypes/09-library-contracts.rill) | [Core library interfaces](09-library-contracts.md). | [27418d5](https://github.com/Reilley64/rill/commit/27418d5) |
| [10-batch-report/](../../prototypes/10-batch-report/) | [Batch-report application and findings](10-batch-report.md). | [5da7522](https://github.com/Reilley64/rill/commit/5da7522) |
| [11-conversions.rill](../../prototypes/11-conversions.rill) | [User conversions and error evidence](11-conversions.md). | [ac23d5c](https://github.com/Reilley64/rill/commit/ac23d5c) |
| [12-streaming-io.rill](../../prototypes/12-streaming-io.rill) | [Streaming ownership and partial effects](12-streaming-io.md). | [ac23d5c](https://github.com/Reilley64/rill/commit/ac23d5c) |
| [13-filesystem.rill](../../prototypes/13-filesystem.rill) | [Paths and uncalled filesystem workflows](13-filesystem.md). | [9750d93](https://github.com/Reilley64/rill/commit/9750d93) |
| [14-environment-inputs.rill](../../prototypes/14-environment-inputs.rill) | [Owned environment and argument inputs](14-environment-inputs.md). | [a03fa33](https://github.com/Reilley64/rill/commit/a03fa33) |
| [15-secure-random.rill](../../prototypes/15-secure-random.rill) | [Secure random bytes and application fakes](15-secure-random.md). | [a03fa33](https://github.com/Reilley64/rill/commit/a03fa33) |
| [16-sql-codecs/](../../prototypes/16-sql-codecs/) | [SQL codecs and control examples](16-sql-codecs.md). | [0e9f70c](https://github.com/Reilley64/rill/commit/0e9f70c) |
| [17-json.rill](../../prototypes/17-json.rill) | [JSON grammar, ownership and limits](17-json.md). | [b77fe15](https://github.com/Reilley64/rill/commit/b77fe15) |
| [18-time-values.rill](../../prototypes/18-time-values.rill) | [Temporal values and arithmetic](18-time-values.md). | [b77fe15](https://github.com/Reilley64/rill/commit/b77fe15) |
| [19-clocks-cancellation.rill](../../prototypes/19-clocks-cancellation.rill) | [Manual clocks, tokens and deadlines](19-clocks-cancellation.md). | [b77fe15](https://github.com/Reilley64/rill/commit/b77fe15) |
| [20-stacks-recursion.rill](../../prototypes/20-stacks-recursion.rill) | [Stack and tail-call rules](20-stacks-recursion.md). | [d30cabe](https://github.com/Reilley64/rill/commit/d30cabe) |
| [21-diagnostics/](../../prototypes/21-diagnostics/) | [Diagnostic/report fixtures and checker](21-diagnostics.md). | [d30cabe](https://github.com/Reilley64/rill/commit/d30cabe) |
| [22-networking.rill](../../prototypes/22-networking.rill) | [Addresses and controlled networking](22-networking.md). | [d30cabe](https://github.com/Reilley64/rill/commit/d30cabe) |
| [23-tls/](../../prototypes/23-tls/) | [TLS values, composition and checker](23-tls.md). | [2121a68](https://github.com/Reilley64/rill/commit/2121a68) |
| [24-progress-io.rill](../../prototypes/24-progress-io.rill) | [Channel wakes and single-owner I/O progress](24-progress-io.md). | [d47c1fc](https://github.com/Reilley64/rill/commit/d47c1fca8e01a4c8c98b070a50b553e9f1b323e7) |
| [25-http.rill](../../prototypes/25-http.rill) | [HTTP values and strict HTTP/1 boundaries](25-http.md). | [d47c1fc](https://github.com/Reilley64/rill/commit/d47c1fca8e01a4c8c98b070a50b553e9f1b323e7) |

The stack and diagnostic programs arrived later than their old reserved numbers suggested. Error and concurrency examples retain the earlier chronology of the port and channel programs they absorbed.

## Paths after consolidation

Case notes formerly stored as `prototypes/<name>.md` now live in `docs/prototypes/`. A grouped example's README uses `docs/prototypes/<group>.md`. The consolidation record links current replacements while retaining original filenames in its historical column.

Renumbering moves complete source/fixture groups without changing Rill or data contents. Diagnostic reports retain their original fictitious URIs; the [checker](21-diagnostics.md#reproduce-the-static-checks) resolves them relative to its current directory. Recorded paths identify the original authored evidence, not the current filesystem location.

Standalone programs moved from `prototypes/backend-30/` to `experiments/backend-30/`, and from `prototypes/26-sqlite-provider/` to `experiments/sqlite-provider/`. Recorded command logs, hashes and ticket snapshots retain their historical paths. Current runbooks use the new layout.

## Executable experiments

[Backend](../../experiments/backend-30/README.md) and [SQLite provider](../../experiments/sqlite-provider/README.md) programs live in [experiments/](../../experiments/README.md). Their native/model execution evidence is distinct from the expected behavior of these Rill sources. Moving a source example does not rerun an experiment or make it compiler-checked.
