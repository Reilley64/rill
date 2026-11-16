# Documentation

Rill's design is recorded in the documents below. Source-ticket approval, RFC acceptance and implementation evidence are separate. Use the [RFC register](rfcs/README.md#proposal-register) for proposal status.

## Start here

- Use the [development guide](development.md) for the Rust workspace, mise tasks and scaffold limits.
- Read [GLOSSARY.md](../GLOSSARY.md) for Rill's terms.
- Use the [RFC conventions and template](rfcs/README.md) when writing a proposal.
- Read [ADRs](adr/) for decisions and their rationale. Some historical numbers repeat. Identify an ADR by its full filename and decision link.
- Use the [example index](prototypes/README.md) for source examples, case notes and checker commands.

## Contracts

| Topic | Documents |
| --- | --- |
| Core library, ownership interfaces, environment and secure randomness | [Standard-library contracts](standard-library-contracts.md) |
| JSON | [Values, parsing, formatting and limits](json-contracts.md) |
| Time | [Temporal values](time-contracts.md), [clocks and cancellation](clock-cancellation-contracts.md) |
| Networking | [Addresses, DNS and TCP](networking-contracts.md) |
| TLS | [TLS contracts](tls-contracts.md), [native provider boundary](tls-native-boundary.md) |
| SQL | [Standard SQL contracts](sql-contracts.md) |
| SQLite provider | [Provider contracts](sqlite-provider-contracts.md), [binding metadata](sqlite-provider-binding-v1.md), [metadata schema](sqlite-provider-binding-v1.schema.json) |
| Diagnostics and reports | [Diagnostic contracts](diagnostic-contracts.md), [code registry](diagnostic-codes.md), [report schema](diagnostic-report.schema.json) |
| Editor integration | [LSP contracts](lsp-contracts.md) |

HTTP work is tracked in [#39](https://github.com/Reilley64/rill/issues/39). Its current ticket and any later published contracts determine that work's status.

## Examples and evidence

[Rill sources](../prototypes/) are design examples and fixtures. Their [case notes](prototypes/README.md) describe expected behavior and the checks performed. A declared expectation is not evidence that a Rill compiler ran it.

[Standalone experiments](../experiments/README.md) execute models or native programs. Their reports identify the tested revision, tools, targets, results and limitations. Recorded commands and hashes describe the original run, including paths that predate a directory move.

[Research](research/README.md) records primary sources and design comparisons. Supporting research does not amend a contract by itself.

## Agent guidance

[AGENTS.md](../AGENTS.md) routes coding agents to issue, domain, toolchain and repository-maintenance instructions. Those instructions are separate from the language and library contracts.
