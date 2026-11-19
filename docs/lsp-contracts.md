# Rill language server contract

This is a tooling contract, not a language RFC. [#42](https://github.com/Reilley64/rill/issues/42) tracks work on the server. The [diagnostic contract](diagnostic-contracts.md) defines the reports and source facts the server can use. No protocol or editor support is established yet.

## Protocol and scope

Use the official [Language Server Protocol 3.17 specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/) as the initial protocol baseline. Follow the specification's required behavior and the rules for each capability the server supports. If a capability needs a newer protocol version, identify that specification and negotiate it where required. No editor release defines the feature set or acceptance criteria.

Start with lifecycle, document synchronization and diagnostics where the front end can supply correct facts. Add optional features when the server has the analysis results they need. Do not advertise an unimplemented feature or use a placeholder response as evidence. Follow the specification's behavior for unsupported requests. Optional features, editor plug-ins and editor-specific behavior are not language or release prerequisites.

## Launch and lifecycle

The planned `rill lsp` command uses stdio. Send framed JSON-RPC on stdout. Send logs to stderr or through supported protocol messages. Do not send CLI diagnostic JSON Lines on the protocol stream.

Follow initialization, capability negotiation, shutdown and exit in the protocol specification. After restart, rebuild state from the client's open documents and current configuration. Do not overwrite unsaved text. Configuration, source and tool failures must not appear as a successful check.

## Context and analysis

Reuse compiler front-end facts. Do not generate executable code or run startup, expectations or user formatters. Equivalent inputs must have equivalent source-language meaning. Use partial results from unaffected source only when the analysis can justify them.

An analysis context identifies the entry, target, compiler options and captured sources. A workspace folder or working directory does not establish a Rill entry. Use an explicit containing context, a configured applicable default or the sole containing context, in that order. If more than one remains, ask the user to select one. An unconfigured named application source can have a standalone context if its target and options are known. An untitled document can receive local analysis without guessed program facts. Installed standard-library source keeps its separate, read-only origin.

Keep loaded implementation sets and expectations separate across contexts. Searching or indexing another file does not load it into an application. Use the same context for diagnostics, navigation and completion. Disclose incomplete searches and unresolved analysis instead of choosing a default type or overload.

Track document versions and the dependency, configuration and toolchain snapshots that checking used. Invalidated operations must not publish known-stale results. Use protocol cancellation and progress where applicable. Do not promise a fixed response time.

## Diagnostics and positions

Publish diagnostics through one applicable supported path per document. If the server supports pull diagnostics, use the specified result IDs and refresh rules. For push diagnostics, replace or clear previous results when the owning document or context changes. Do not publish duplicate push and pull results for the same facts.

Keep compiler locations as original-source byte ranges. Project them to zero-based protocol positions in the negotiated encoding, with UTF-16 as the protocol default. At a valid scalar boundary, the column is the length of the line prefix in the chosen encoding:

| Encoding | Column unit |
| --- | --- |
| UTF-8 | Bytes |
| UTF-16 | Code units |
| UTF-32 | Unicode scalars |

Project the exact synchronized document bytes. A saved file and an open document with a different byte order mark are different snapshots. Do not clamp a range inside a scalar or CRLF pair, wrap an unrepresentable position, or fabricate a source range for a non-source failure. Report an operational limit when a location cannot be represented. Use encoded URIs. Preserve diagnostic codes, essential explanations and supported related data without creating extra diagnostics for the same problem.

## Optional capabilities

Support a feature only when its results follow the compiler's language rules and its wire behavior follows the protocol. The following are candidates, not a required feature inventory:

- **Navigation and completion.** Preserve canonical identities, aliases, namespaces and privacy. Distinguish a declaration from a selected implementation. Show unresolved candidates as such. Offer legal source, not an invented type or implicit import.
- **Symbols and hierarchies.** Use source declarations, enum relationships and actual call evidence. Do not invent inheritance, runtime dispatch or a complete call graph.
- **Rename and source edits.** Revalidate source versions, dependencies, permissions and the affected contexts. Refuse changes that need unavailable evidence. Organize-imports must preserve loaded modules, implementations, exports and expectations. A textually unused import can have a semantic effect.
- **Formatting.** Preserve meaning, comments and literal contents. If a region cannot be parsed safely, leave it unchanged. Formatting is not an import cleanup or ownership rewrite.

Negotiate edit options and respect the client's stated failure handling. Do not promise atomic edits or treat a failed command as undoing an applied edit. Do not use commands or editor integration to execute arbitrary project code.

## Verification

Use an editor-independent harness to launch the real server and exchange framed protocol messages. Record the server revision, target, supported capabilities, inputs and observed messages. Check the lifecycle, negotiated encoding, unsaved text, invalidation, cancellation and diagnostic clearing. For each advertised feature, check its protocol responses and its meaning on Rill source. Test adverse edit outcomes when source edits are supported.

Only claim host and capability combinations that pass. Editor smoke tests can supplement the protocol harness, but they do not define conformance. Compiler tests, schema validation and mocked messages are not server acceptance. The existing `test:lsp-client` task remains unavailable until it exercises implemented server behavior.
