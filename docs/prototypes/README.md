# Rill examples and case notes

Rill sources and their fixture data live in [prototypes/](../../prototypes/). This directory holds explanations, expected outcomes and case inventories.

No complete Rill compiler is included. A source file's expectations are design examples unless its case notes identify an actual Rill execution. Python/schema/reference checks exercise only their stated layer. Use the [RFC evidence labels](../rfcs/README.md#examples-and-evidence) when recording new results.

The early examples retain historical syntax or library calls where later decisions changed them. Read the [current contracts](../README.md#contracts), [glossary](../../GLOSSARY.md) and linked decisions before treating an old example as valid v0 code. This cleanup moves documentation, not language semantics or source programs.

## Early syntax studies

| Topic | Source |
| --- | --- |
| Hello world | [01-hello-world.rill](../../prototypes/01-hello-world.rill) |
| Results, pipes and guard | [02-result-pipe-guard.rill](../../prototypes/02-result-pipe-guard.rill) |
| Overloading | [03-overloads.rill](../../prototypes/03-overloads.rill) |
| Goroutines and channels | [04-goroutines-channels.rill](../../prototypes/04-goroutines-channels.rill) |
| Records and matching | [05-records-unions-match.rill](../../prototypes/05-records-unions-match.rill) |
| Top-level expectations | [06-expect.rill](../../prototypes/06-expect.rill) |

## Feature and library examples

| Topic | Source | Case notes |
| --- | --- | --- |
| Mutation and borrowing | [07-mutation.rill](../../prototypes/07-mutation.rill) | [Mutation cases](07-mutation-safety.md) |
| Named record construction | [08-named-records.rill](../../prototypes/08-named-records.rill) | [Decision](../adr/0008-records-require-named-construction.md) |
| Concurrency safety | [09-concurrency-safety.rill](../../prototypes/09-concurrency-safety.rill) | [Concurrency cases](09-concurrency-safety.md) |
| Modules and privacy | [10-modules/](../../prototypes/10-modules/) | [Module cases](10-modules.md) |
| Numeric/text types | [11-numbers-strings.rill](../../prototypes/11-numbers-strings.rill) | [Numeric/text cases](11-numbers-strings.md) |
| Enums | [12-enums.rill](../../prototypes/12-enums.rill) | [Enum cases](12-enums.md) |
| Caused-by errors | [13-caused-by-errors.rill](../../prototypes/13-caused-by-errors.rill) | [Error cases](13-caused-by-errors.md) |
| Stack and tail-call rules | [14-stacks-recursion.rill](../../prototypes/14-stacks-recursion.rill) | [Stack cases](14-stacks-recursion.md) |
| Core library interfaces | [15-library-contracts.rill](../../prototypes/15-library-contracts.rill) | [Library cases](15-library-contracts.md) |
| Batch-report program | [16-batch-report/](../../prototypes/16-batch-report/) | [Workload and findings](16-batch-report.md) |
| Diagnostics/report fixtures | [17-diagnostics/](../../prototypes/17-diagnostics/) | [Fixtures and checker](17-diagnostics.md) |
| User conversions | [18-conversions.rill](../../prototypes/18-conversions.rill) | [Conversion cases](18-conversions.md) |
| JSON | [19-json.rill](../../prototypes/19-json.rill) | [JSON cases](19-json.md) |
| Streaming I/O | [19-streaming-io.rill](../../prototypes/19-streaming-io.rill) | [I/O cases](19-streaming-io.md) |
| Environment/arguments | [20-environment-inputs.rill](../../prototypes/20-environment-inputs.rill) | [Input cases](20-environment-inputs.md) |
| Filesystem | [20-filesystem.rill](../../prototypes/20-filesystem.rill) | [Filesystem cases](20-filesystem.md) |
| Secure random bytes | [21-secure-random.rill](../../prototypes/21-secure-random.rill) | [Random cases](21-secure-random.md) |
| SQL codecs and control | [22-sql-codecs/](../../prototypes/22-sql-codecs/) | [SQL cases](22-sql-codecs.md) |
| Temporal values | [23-time-values.rill](../../prototypes/23-time-values.rill) | [Time cases](23-time-values.md) |
| Clocks and cancellation | [24-clocks-cancellation.rill](../../prototypes/24-clocks-cancellation.rill) | [Clock/control cases](24-clocks-cancellation.md) |
| Networking | [25-networking.rill](../../prototypes/25-networking.rill) | [Network cases](25-networking.md) |
| TLS values | [27-tls/](../../prototypes/27-tls/) | [TLS cases and checker](27-tls.md) |

Historical numeric prefixes repeat. Refer to complete filenames, such as `19-json.rill` and `19-streaming-io.rill`, rather than treating a number as an artifact identifier. Published source names remain unchanged.

## Paths after consolidation

Case notes formerly stored as `prototypes/<name>.md` now live at `docs/prototypes/<name>.md`. A grouped example's `README.md` now lives at `docs/prototypes/<group>.md`. Rill source and input/output paths are unchanged.

Standalone programs moved from `prototypes/backend-30/` to `experiments/backend-30/`, and from `prototypes/26-sqlite-provider/` to `experiments/sqlite-provider/`. Old ticket snapshots and recorded command logs retain their original paths.

## Executable experiments

The [backend](../../experiments/backend-30/README.md) and [SQLite provider](../../experiments/sqlite-provider/README.md) programs now live in [experiments/](../../experiments/README.md). Their commands, native/reference results and evidence limits are separate from these Rill source studies.

Recorded command logs and hashes retain the paths used in their original runs. Current runbooks use the new layout. Directory moves do not rerun an experiment or make a source example compiler-checked.
