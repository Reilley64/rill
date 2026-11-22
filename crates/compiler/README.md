# Compiler front end

This crate implements M1 source snapshots/lexing, the M2 parser and M3 module resolution/declaration collection. It depends on neither LLVM nor LSP. Parsing success does not establish successful source checking or Rill execution. Product commands and acceptance gates remain unavailable.

The parser follows both [syntax-annex artifacts](../../docs/grammar/v0-syntax-v1.md), including the linked EBNF. Their confirmed syntax policies do not imply full annex or owning-RFC acceptance. M2 ticket ri-lpw records the baseline, acceptance evidence and limitations.

## Strict parser entry points

```rust
use compiler::lexer::lex;
use compiler::parser::{parse, EntryPoint, ParseLimits};
use compiler::source::Snapshot;

let source = Snapshot::capture("synthetic://example", b"Packet<i32>.Data(3i32)").unwrap();
let lexical = lex(&source);
let syntax = parse(&source, &lexical, EntryPoint::Expression, ParseLimits::default()).unwrap();
assert_eq!(syntax.nodes[syntax.root].span.bytes(), source.bytes());
```

`parser::parse` consumes an existing M1 `Lexed` result for the supplied immutable `Snapshot`. It does not scan source characters again. Use `EntryPoint::Module`, `Expression`, `Type` or `Pattern` to select the grammar. Every successful result consumes all significant input and exact EOF. There is no successful-prefix entry point.

The parser requires M1's `Complete` status without lexical diagnostics. It also validates snapshot identity and the ordered token/trivia partition through EOF. Callers must supply M1 lexical facts, not synthesize token kinds or decoded literal contents.

`ParseFailure` contains an original-source span and one failure category:

| Category | Meaning |
| --- | --- |
| `Lexical` | M1 did not supply complete, error-free lexical input. Inspect the supplied `Lexed` status and diagnostics for the lexical reason. This includes unavailable lexical analysis. |
| `InputMismatch` | The streams do not belong to this snapshot or do not partition it through exact EOF. |
| `Syntax { expected }` | The selected grammar failed, or significant input remains. EOF failures use the snapshot's zero-width EOF span. The expectation text is an implementation diagnostic, not a new language rule or stable diagnostic-code registry. |
| `ResourceLimit` | The parser exhausted an implementation budget. This is incomplete analysis, not accepted source or a language syntax restriction. |

Before input validation completes, resource failures use the supplied snapshot's EOF span. They never borrow a span from an unvalidated lexical stream.

Failure returns no accepted prefix or partial syntax tree. The parser inserts no delimiters and performs no editor recovery or incremental parsing. A misplaced expectation rejects with a module-level-placement expectation; no local test function is invented.

## Source-preserving syntax results

`Syntax` retains the snapshot and a copy of all M1 lexical facts, including trivia, exact literal spellings and decoded-source provenance. Its `work` field reports charged parser operations, including discarded lookahead. M3 uses this additive statistic for its shared work budget. Existing M2 syntax and failure behavior remain unchanged. It stores nodes in a flat, postorder arena. `root` and each node's `children` are arena indices. This also avoids recursive destruction of long binary/postfix chains.

Every node has a `NodeKind` and an original half-open byte `Span`. Its span covers its written syntax and internal trivia. The module spans the entire snapshot, including leading/trailing trivia. Fragment nodes exclude trivia outside their written syntax.

Children retain written order:

| Node category | Children |
| --- | --- |
| Binary, wrapping, pipe and assignment | Left/input node, then right/step node. |
| Prefix, propagation and grouping | The operand or grouped node. |
| Call, index and field | Callee/base, then arguments, index or field name. |
| Value reference, named type and pattern path | Qualified root, optional type arguments, and any permitted variant selection. Expression field selections have explicit `Field` nodes. |
| Construction | Constructor path, optional leading `Spread`, then initializer fields. Shorthand fields contain their written name only. |
| Arrow | `ArrowHead`, then expression or block body. The head contains `ArrowParameters` and any result annotation. |
| Match | Scrutinee, then arms. Each arm contains its pattern, optional `MatchGuard`, then expression or block body. |
| Template | Text and hole nodes. A hole contains its one expression and spans both original lexical delimiters. |
| Pattern | Written subpatterns or path. Record field nodes distinguish shorthand from explicit patterns. A `NamePattern` retains its path rather than resolving a constructor. |
| Declaration and statement | Written names, binders, parameters, types, requirements, expressions and bodies, each in source order. Optional syntax has no invented child. |

`TokenView` entries cover every significant token, including EOF, in order. `token` indexes the original M1 token. `span` selects the exact consumed piece. Type parsing can view a leading `>` from `>>`, `>=` or `>>=`, and a leading `&` from `&&`. Original lexical tokens never change. Views and arena nodes from an abandoned checkpoint do not appear in the result.

The parser uses only syntax for commitment. It never retries a committed generic, arrow or question-mark parse based on names, types or expected results. Match guards reserve their current top-level arrow delimiter. Nested delimiters establish separate arrow contexts. Each pipe argument list owns its direct-placeholder allowance.

## Resource limits

`ParseLimits::default()` permits 64 nested parsing entries and 1,000,000 work units. Callers can lower either budget. Nesting has an absolute implementation ceiling of 64 to protect the host stack, even if a caller requests more. Flat left-associative chains do not consume nesting per element.

Work units charge lexical-partition items, consumed token views, node creation and nested parsing entries. Failed tentative scans retain their work charge. These counters bound parser operations, not wall-clock time, source-byte storage or allocation success. M1 lexing occurs before this call. Abrupt host exhaustion need not produce a final failure report.

## Acceptance tests

Run the LLVM-free compiler suite with the pinned tools:

```sh
mise exec -- cargo test --locked -p compiler --no-default-features
```

- `tests/m1.rs` and existing source/lexer unit tests retain M1 acceptance. M2 changes neither M1 implementation nor its tests.
- `tests/m2_g_cases.rs` has one named test per G1 through G47, including both sides of contrast cases. These are parser assertions, not Rill program executions.
- `tests/m2.rs` adds tree/span assertions, the complete question-mark start set, generic follow boundaries, trivia, nested delimiters, malformed/EOF cases, source identity, deterministic generated inputs and controlled resource exhaustion.

Semantic parser checks remain excluded. Examples such as nontrailing generic defaults, empty bracket values, top-level returns, unresolved constructors and `Ok()` parse without proving their legality. M3 supplies the module-resolution subset below. Typing, expression/local-use resolution, ownership, control targets, coverage, runtime behavior and IDE/client acceptance require later milestones.

## M3 loader and resolver

The owner approved O1 through O7 and the three test seams at checkpoint `a41e7cc955e67329d6dc3a8a69de667e0af204ac`. Ticket ri-1fk (comment 6939478619) records approval. [RFC 0002](../../docs/rfcs/0002-modules-and-program-loading.md#proposed-m3-resolution-procedure) owns the scoped contract. Its full status remains Draft. A resolved graph is not a successfully checked program.

| Interface | Contract |
| --- | --- |
| `loader::load(Request)` | Supply an entry path, standard root, explicit copy/dispose prelude selectors, target/options, execution-mode input and limits. Capture sources through M1 and parse through M2. Resolve the complete required graph before publishing any imported binding. |
| `Graph::lookup_export(module, namespace, segments)` | Select exact exported names from an established module key. It performs no filesystem access or child autoload. The namespace is explicit. |
| `loader::validate_inputs(graph, limits)` | Replay roots, positive/negative path observations and source bytes without mutating retained snapshots. Success means unchanged observations, not simultaneous filesystem capture. |

`Graph` exposes read-only module, declaration, binding, implementation, prelude and import-outcome records. Module keys contain the origin and canonical root-relative UTF-8 path. Declaration identities also retain an opaque capture identity, source position and kind. Never merge declaration identities from separate captures. Source spans retain their actual M1 snapshot IDs and canonical native source paths. URI encoding and reporter transport are not implemented here.

Native parent steps apply after preceding symbolic links resolve, not through lexical path cleanup. Discovery includes every parsed candidate-prefix facade, even without a winning export. Its declarations and expectations participate under approved O1. Directory enumeration only verifies exact spelling. It does not enroll unrelated sources. Grouped members select exports only. Ungrouped paths consider all applicable file/export routes and deduplicate targets within each namespace.

Re-export closure uses finite canonical candidate sets. Iterative alias-component analysis separates anchored cycles, missing exports, private barriers and unanchored cycles. An acyclic alias depending on an unanchored cycle retains R0405. Whole-graph publication prevents failed grouped imports or re-exports from supplying installed downstream bindings. A rejected attempt exposes provisional candidates, not a `Graph` or installed half-binding.

Implementation records identify the original family, defining module and source signature/body nodes. They do not prove valid signatures, typed duplicate-pattern rejection, orphan permission, conversions or protected operations. Member records retain privacy and owner information for later checking. Named function captures, private-field operations and public generic requirements remain unchecked.

### Explicit standard and prelude inputs

Each `Request` requires a standard source root and two root-relative `.rill` selectors naming exported body-less families. Tests supply minimal `core.rill` families. The compiler records its fixed primitive/predicate identities separately. Local shadowing does not replace the selected canonical copy/dispose identities.

Prelude admission checks source structure and visibility, not typed signature compatibility. Target and option inputs identify the capture; M3 does not validate backend or standard-library compatibility. Selected standard paths never grant native-provider or protected-operation authority. There is no shipped standard library, empty-prelude mode, implicit helper import or installed-toolchain discovery.

### Failures and resource limits

`FailureKind` distinguishes InvalidRequest, SourceRejected, InputFailure, SourceChanged and ResourceLimit. Findings preserve original spans and canonical candidate provenance. Diagnostic-only privacy witnesses retain the private binding span, module, namespace, target and blocked route position. They never enter public candidate sets or grant source access. R0400 through R0405 retain their module meanings. R0406 layout checking is not implemented. Lexical diagnostics retain M1 codes; syntax and non-family-target messages do not introduce a new stable diagnostic registry.

Rejected and incomplete attempts retain explicitly partial module/import facts. Secondary diagnostics can remain incomplete. No failure type exposes successful export lookup or installed imports. An unreadable alternative is not a missing candidate. Changed captures cannot become source success through a retry.

Default limits are 4,096 modules, 64 MiB per source, 256 MiB retained source bytes, 10,000,000 work units, 100,000 probes and 1,000,000 inspected directory entries. Capture and completion replay share counters. Cumulative read bytes cannot exceed twice the retained-source allowance. M2 parser work participates in the shared budget, with its existing nesting and per-parse limits. Each probe also has a 64-link expansion ceiling. Exceeding that implementation ceiling returns ResourceLimit; a detected filesystem link loop returns InputFailure.

These limits are not language restrictions or wall-clock guarantees. Host calls can block, allocations can fail abruptly, and source capture is not a filesystem sandbox or transaction. Changes restored between observations can go undetected. Later `validate_inputs` cannot certify continuous freshness.

### M3 acceptance and host limits

[The acceptance record](tests/m3-acceptance.md) maps RFC cases M1 through M15 and identity/order requirements to real filesystem tests in `tests/m3.rs`. Tests cross the public loader, qualification and retained-input-validation interfaces. They do not execute Rill code.

The recorded host is macOS 27.0.1 AArch64. Its filesystem is case-insensitive and normalization-insensitive for the exercised names. Exact alternate-spelling lookups reject with an explicit InputFailure instead of merging identities. Distinct case/normalization file pairs cannot be validated on this filesystem. The [hosted publication record](tests/m3-acceptance.md#hosted-publication-evidence) reports passing scaffold and M3 tests on all four configured Linux/macOS runner profiles at source revision 862704b. Distinct alternate-case/normalization file-pair validation remains unrun. These are scoped resolver results, not backend/runtime support. Windows-native loading is unsupported. Permission-denial tests disclose privileged-host bypasses rather than counting them as demonstrated denial.
