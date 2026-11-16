# Structured diagnostics and complete OpenIDEA-used LSP support

[Issue #24](https://github.com/Reilley64/rill/issues/24) explicitly confirmed 52 interview decisions and the consolidated design. Use one structured diagnostic model for human, versioned JSONL and LSP adapters, with effect-preserving bounded expectation snapshots. Prioritize complete applicable LSP support used by OpenIDEA v0.1.44 rather than a diagnostics-only milestone. Errors and editor assistance depend on the same source identities, contexts and checking facts.

## Contracts and boundaries

- [Diagnostic/report contracts](../diagnostic-contracts.md), [code registry](../diagnostic-codes.md) and [version-1 schema](../diagnostic-report.schema.json) define the reporting interface. Exact prose is not a parsing contract; published codes retain meaning.
- [LSP contract and acceptance matrix](../lsp-contracts.md) pins OpenIDEA v0.1.44 (`bf36cc08019a1be78c151245c67dcd872a9c37b5`), LSP 3.17 core plus negotiated newer used methods, and macOS/Linux x86-64/AArch64 hosts. Minimal language/server registration is included; non-LSP editor/grammar, Run/Test/DAP, new notebook semantics and implicit Windows support are not.
- Reuse front-end analysis without LLVM generation or user-code execution. Keep selected entry/target/options, entry-directory roots, isolated read-only std origin and loaded implementation graphs distinct from workspace indexing. No new Rill manifest.
- Preserve canonical declarations/bodies/aliases/namespace identities and current enum, ownership, cause and remaining-field cleanup rules. Later confirmed #41 adds language-wide implement family bodies and specific conversion keys/selected-error evidence; its exceptions do not become general return dispatch. Import cleanup preserves loaded implementations and expectations, not merely textual uses. Refactors require sufficient evidence and an actual client workflow; edits are not unconditionally atomic.
- Snapshots retain diagnostic data, not program owners/loans. No extra user formatter/copy/iterator/comparison/disposal operations or generic requirements. False expectations continue after ordinary cleanup; panic/resource exhaustion remains fatal, without unwind or a guaranteed final report.

## Consequences and evidence

The selected client drops some detailed diagnostic fields, retains some stale display state and negotiates abort rather than transactional edits. Preserve the full diagnostic data, supply essential message fallbacks and test actual application paths. Advertised capabilities do not prove behavior. The [source research](../research/openidea-language-server-capabilities.md) is not runtime validation.

[Delivery/acceptance #42](https://github.com/Reilley64/rill/issues/42) carries the entire prioritized scope as a **v0 tooling-readiness gate**. No matrix row has passed. Closing #24 approves design only; completed server delivery is not a prerequisite for drafting #28 RFCs. #28 retains exact checking/cleanup/observation/lowering rules; #30 retains backend evidence. An inconsistent required contract returns to the map, not silent feature reduction or an assumed new architecture.

[Prototype 17](../prototypes/17-diagnostics.md) provides hand-authored source/report/schema examples and adversarial cases. No Rill compiler, runtime or server was executed, no soundness/backend proof is claimed, and future RFC acceptance remains separate.
