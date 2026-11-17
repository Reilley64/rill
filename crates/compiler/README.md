# Compiler front end

This crate implements M1 source snapshots/lexing and the M2 parser. It depends on neither LLVM nor LSP. Parsing success does not establish successful source checking or Rill execution. Product commands and acceptance gates remain unavailable.

The parser follows both [syntax-annex artifacts](../../docs/grammar/v0-syntax-v1.md), including the linked EBNF. Their confirmed syntax policies do not imply full annex or owning-RFC acceptance. [M2 ticket #52](https://github.com/Reilley64/rill/issues/52) records the baseline, acceptance evidence and limitations.

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

`Syntax` retains the snapshot and a copy of all M1 lexical facts, including trivia, exact literal spellings and decoded-source provenance. It stores nodes in a flat, postorder arena. `root` and each node's `children` are arena indices. This also avoids recursive destruction of long binary/postfix chains.

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

Semantic checks remain excluded. Examples such as nontrailing generic defaults, empty bracket values, top-level returns, unresolved constructors and `Ok()` parse without proving their legality. Resolution, typing, ownership, control targets, coverage, runtime behavior and IDE/client acceptance require their later milestones.
