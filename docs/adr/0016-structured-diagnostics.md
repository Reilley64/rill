# Structured diagnostics

[Issue #24](https://github.com/Reilley64/rill/issues/24) explicitly confirmed 52 interview decisions and the consolidated design. Use one structured diagnostic model for human, versioned JSONL and LSP adapters, with effect-preserving bounded expectation snapshots. Errors and editor assistance depend on the same source identities, contexts and checking facts. The [current tooling contract](../lsp-contracts.md) replaces the earlier client-specific delivery requirement.

## Contracts and boundaries

- [Diagnostic/report contracts](../diagnostic-contracts.md), [code registry](../diagnostic-codes.md) and [version-1 schema](../diagnostic-report.schema.json) define the reporting interface. Exact prose is not a parsing contract; published codes retain meaning.
- The [language server contract](../lsp-contracts.md) uses the official LSP specification. Verify capabilities and host profiles before claiming support.
- Reuse front-end analysis without LLVM generation or user-code execution. Keep selected entry/target/options, entry-directory roots, isolated read-only std origin and loaded implementation graphs distinct from workspace indexing. No new Rill manifest.
- Preserve canonical declarations/bodies/aliases/namespace identities and current enum, ownership, cause and remaining-field cleanup rules. Later confirmed #41 adds language-wide implement family bodies and specific conversion keys/selected-error evidence; its exceptions do not become general return dispatch. Import cleanup preserves loaded implementations and expectations, not merely textual uses. Refactors require sufficient evidence and an actual client workflow; edits are not unconditionally atomic.
- Snapshots retain diagnostic data, not program owners/loans. No extra user formatter/copy/iterator/comparison/disposal operations or generic requirements. False expectations continue after ordinary cleanup; panic/resource exhaustion remains fatal, without unwind or a guaranteed final report.

## Consequences and evidence

[Tooling issue #42](https://github.com/Reilley64/rill/issues/42) tracks the server when the shared analysis can support it. Closing #24 approved design only. No server acceptance has passed. #28 retains exact checking, cleanup, observation and lowering rules. #30 retains backend evidence.

[Prototype 21](../prototypes/21-diagnostics.md) provides hand-authored source/report/schema examples and adversarial cases. No Rill compiler, runtime or server was executed, no soundness/backend proof is claimed, and future RFC acceptance remains separate.
