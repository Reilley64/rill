# Documentation

Rill's design is recorded in the documents below. Source-ticket approval, RFC acceptance and implementation evidence are separate. Use the [RFC register](rfcs/README.md#proposal-register) for proposal status.

## Start here

- Use the [development guide](development.md) for the Rust workspace, mise tasks and scaffold limits.
- Read the [implementation plan](implementation-plan.md) before starting the first compiler milestone.
- Read [GLOSSARY.md](../GLOSSARY.md) for Rill's terms.
- Use the [RFC conventions and template](rfcs/README.md) when writing a proposal.
- Read the [syntax annex](grammar/v0-syntax-v1.md) with its [EBNF](grammar/v0-syntax-v1.ebnf) for the confirmed grammar policy groups.
- Read [ADRs](adr/) for decisions and their rationale. Some historical numbers repeat. Identify an ADR by its full filename and decision link.
- Use the [example index](prototypes/README.md) for source examples, case notes and checker commands.

## Contracts

| Topic | Documents |
| --- | --- |
| Core library, ownership interfaces, environment and secure randomness | [Standard-library contracts](standard-library-contracts.md) |
| JSON | [Values, parsing, formatting and limits](json-contracts.md) |
| Time | [Temporal values](time-contracts.md), [clocks and cancellation](clock-cancellation-contracts.md) |
| Networking | [Addresses, DNS and TCP](networking-contracts.md) |
| Resumable I/O progress | [Single-owner progress, channel wakes and standard drivers](progress-io-contracts.md) |
| HTTP | [Shared values, strict HTTP/1 codec and explicit client/server drivers](http-contracts.md) |
| TLS | [TLS contracts](tls-contracts.md), [native provider boundary](tls-native-boundary.md) |
| SQL | [Standard SQL contracts](sql-contracts.md) |
| SQLite provider | [Provider contracts](sqlite-provider-contracts.md), [binding metadata](sqlite-provider-binding-v1.md), [metadata schema](sqlite-provider-binding-v1.schema.json) |
| Diagnostics and reports | [Diagnostic contracts](diagnostic-contracts.md), [code registry](diagnostic-codes.md), [report schema](diagnostic-report.schema.json) |
| Editor integration | [LSP contracts](lsp-contracts.md) |
| First backend (M6 slice 1) | [LLVM binding and provisioning](adr/0030-inkwell-on-a-source-built-static-llvm-23.md), [string representation](adr/0031-owned-strings-are-pointer-length-capacity.md), [allocator interface](adr/0032-generated-code-allocates-through-three-runtime-functions.md), [entry shim and entry stack](adr/0033-the-runtime-owns-main-and-the-entry-stack.md), [calling convention for slice 1](adr/0034-llvm-default-c-calling-convention-for-slice-1.md), [panic and exit statuses](adr/0035-panic-ends-the-program-with-status-101.md), [enum layout, aggregate passing, List and drop glue](adr/0036-enum-layout-aggregate-passing-list-handle-and-drop-glue.md) (draft ADRs) |

The HTTP contract records ri-n5i's design only. ri-19d retains implementation/conformance/security evidence; ri-hw5/#44 retain lower progress/TLS delivery.

## Examples and evidence

[Rill sources](../prototypes/) are design examples and fixtures. Their [case notes](prototypes/README.md) describe expected behavior and the checks performed. A declared expectation is not evidence that a Rill compiler ran it.

[Standalone experiments](../experiments/README.md) execute models or native programs. Their reports identify the tested revision, tools, targets, results and limitations. Recorded commands and hashes describe the original run, including paths that predate a directory move.

[Research](research/README.md) records primary sources and design comparisons. Supporting research does not amend a contract by itself.

## Agent guidance

[AGENTS.md](../AGENTS.md) routes coding agents to issue, domain, toolchain and repository-maintenance instructions. Those instructions are separate from the language and library contracts.
