# Compiler implementation plan

## State and authority

The owner authorized this plan and a committed documentation baseline on 2026-11-17. Compiler milestone M1 has not started. This plan defines implementation scope and acceptance, not new language semantics or completed validation.

Use the committed baseline containing this plan, not a moving branch, as implementation input. Record its full commit SHA in the implementation ticket before coding. The baseline preserves Draft RFC statuses and their scoped human policy approvals.

ri-48g retains the RFC collection, coverage/readiness and existing formal/production follow-through. Its writing deliverable does not become compiler implementation. Create and claim a separate implementation ticket when coding is authorized.

## Inputs

| Input | Applicable scope |
| --- | --- |
| [Development guide](development.md), [ADR 0026](adr/0026-toolchain-repository-and-mise.md) | Private crate ownership, pinned mise/Cargo/uv tools, LLVM-free analysis and unavailable product gates. |
| Lexical/source approval (ri-48g, comment 6929923825) | Exact source/lexer policies across RFCs 0002, 0003, 0006 and 0010. |
| [Source snapshots](rfcs/0002-modules-and-program-loading.md#source-snapshots-and-lexical-input) and [tokens/trivia](rfcs/0003-expressions-bindings-and-control-flow.md#lexical-tokens-and-trivia) | Immutable original bytes, source spans, identifiers, comments and longest token matching. |
| [Literal scanning](rfcs/0006-numbers-strings-and-primitive-operations.md#literal-scanning-and-token-boundaries) and [coordinate procedure](rfcs/0010-diagnostics-and-reporting.md#original-byte-coordinate-procedure) | Numeric/raw/template modes, controls, decoded provenance and byte/scalar/editor projection. |
| Syntax approval (ri-48g, comment 6931939690), [annex](grammar/v0-syntax-v1.md), [EBNF](grammar/v0-syntax-v1.ebnf) | M2 parser policy. M1 preserves lexer tokens rather than applying parser commitment rules. |
| [Test ownership](../tests/README.md) and [RFC evidence rules](rfcs/README.md#examples-and-evidence) | Component-local tests, later cross-stage suites and truthful acceptance records. |

The syntax policy review used EBNF SHA-256 `158c9b74bb2f36dbf992e454ebc10d57bfac9bb3859b04a394d61d29e0bd94e7`. Its companion SHA-256 was `0d7eeef8bf1abddebf6c04501eaf2572ea0f94f528ac69b933109afb9a3fdbf6`. Later status comments do not change those production rules or extend approval.

Full RFC/annex acceptance remains separate. Return conflicting required behavior to design review rather than adapting the specification silently.

## Ordered milestones

| Milestone | Owning implementation | Completion boundary and prerequisites |
| --- | --- | --- |
| M1. Source and lexer | `crates/compiler/` | Immutable snapshots, validated spans, coordinates and complete lexical behavior pass the acceptance matrix below. No parser or LLVM dependency. |
| M2. Parser | `crates/compiler/` | Apply both syntax-annex artifacts, build source-preserving syntax results and test all commitment/context cases. Recovery policy needs an explicit contract before recovery/client claims. |
| M3. Resolution and declaration collection | `crates/compiler/` | First complete candidate/cycle, namespace, scope and metadata procedures under RFC 0002. Validate canonical loaded-graph behavior independently of editor indexing. |
| M4. Typed analysis | `crates/compiler/` | First complete inference, pattern/coverage, overload/evidence and selected-output procedures under the owning RFCs. Produce checked facts rather than inventing successful types for incomplete analysis. |
| M5. Ownership and cleanup lowering | `crates/compiler/` | First complete dependency transfer, branch/loop, exit, capture and initialized-cleanup judgments. Validate MIR obligations and adversarial accepted/rejected cases. |
| M6. Execution and tools | Backend/runtime plus IDE/LSP/CLI owners | Implement only after each semantic/ABI prerequisite is complete. Native/Wasm/provider support and full ri-bv2 client acceptance retain separate gates. |

M2 through M6 are sequencing markers, not detailed implementation tickets or claims of ready specifications. Their missing rules do not block M1. Full standard-library or LSP delivery is not a lexer prerequisite.

## M1 scope and interface

Implement source storage and lexing in `crates/compiler/`. Keep source facts reusable by later compiler and IDE callers. No LSP transport or LLVM types belong in this interface.

Accept explicit source identity and input bytes. Tests can supply synthetic identities without implementing a filesystem resolver or loaded graph. Freeze the original bytes before exposing spans or lexical results.

Expose a small interface for snapshot/span/mapping queries and lexical results. Return token/trivia spans, retained literal spelling/mode data, lexical diagnostics and explicit completion/failure state. Internal storage choices need not become public compiler APIs.

Tests and callers cross that same interface. Private scanner helpers can vary without exposing every state transition. No speculative plugin, alternate lexer or source-provider framework is required.

Use the approved Unicode 17.0.0 property membership, not the host's current character classification. Lock the selected implementation and record its table provenance. Tests must remain offline and reproducible.

Keep numeric magnitudes/spellings exact until later checking. Do not reject a signed-minimum magnitude before its enclosing syntax/type context exists. Literal decoding preserves original-byte provenance instead of changing source offsets.

Malformed input produces truthful diagnostics/invalid data, not an accepted prefix labeled as a valid file. Controlled host/resource limitations remain distinct from source errors. Fatal host exhaustion need not produce a final report.

M1 excludes AST construction, module resolution, type/evidence/ownership checking and user-code execution. It also excludes parser-owned generic/ampersand token views and postfix/infix question-mark selection. Interpolation scanning adds no runtime formatter calls.

## M1 acceptance matrix

Every row needs direct implemented Rust tests with positive, rejected and boundary inputs where applicable. Test names and fixture locations must be listed in the completion record. Placeholder tests and source-inventory counts do not satisfy this matrix.

| Area | Required observations | Existing design cases to translate |
| --- | --- | --- |
| Snapshots and spans | Bytes stay immutable; identity is not reused for different source; ranges validate bounds/order; EOF is exact. Changed caller input cannot alter saved bytes. | RFC 0002 X1 through X4. Controlled size-limit paths are not empty success. |
| Strict UTF-8 | Reject malformed sequences even inside comments/literals. Retain the first invalid byte extent without replacement decoding or fabricated coordinates. | Invalid-source cases plus RFC 0006's established span rules. |
| Identifiers and keywords | Match pinned XID membership, underscore and exact keyword spelling. Preserve distinct normalized-looking names and original bytes. | RFC 0003 L1 through L4/L13/L15/L17. |
| Trivia and punctuation | Preserve BOM, newline/comment spans, nested comments and EOF failures. Match longest punctuation without joining across trivia. | RFC 0003 L5 through L16, lexical portions only. |
| Numeric tokens | Retain full candidates, bases, suffixes and exact spelling. Reject malformed candidates without valid-prefix salvage or width inference. | RFC 0006 S1 through S7. Parser/type conclusions remain outside M1. |
| Text and escapes | Check raw delimiters, fixed escapes, scalar/control admission and physical-content normalization. Preserve decoded-to-original provenance. | RFC 0006 S8 through S19. No runtime codec/conversion validation is implied. |
| Templates | Distinguish text/hole/nested modes, quoted/comment braces, escaped openers and unfinished input. Emit no invented closer or rerun expression. | RFC 0006 S11 through S14/S20 and nested-mode neighbors. |
| Coordinates | Validate byte/scalar/UTF-8/UTF-16/UTF-32 projections, tabs, astral scalars, empty/final lines and unavailable endpoints. | RFC 0010 Y1 through Y5. No live client or filesystem freshness claim. |
| Progress and determinism | Every scan advances or finishes. Repeated inputs produce equivalent token/trivia/error facts; counters/spans do not wrap. | Truncations, malformed delimiters and bounded generated byte inputs. |

Use direct expected kinds, spellings, spans, decoded pieces and diagnostic meanings. Avoid depending on incidental Debug formatting. A focused snapshot can aid review, but must not replace assertions about the required facts.

Keep new component fixtures beside compiler tests, for example `crates/compiler/tests/fixtures/lex/`. That is a planned location, not an existing suite. Preserve historical `prototypes/` and `experiments/` bytes; copy/adapt named cases with provenance when needed.

Broader conformance inputs/runners belong under `tests/` when they cross compiler stages. Neither a fixture-schema check nor an ordinary Rust test count establishes Rill program execution.

## Verification and exit criteria

The planned direct test command is:

```sh
mise exec -- cargo test --locked -p compiler --no-default-features
```

Run applicable fast checks through the existing tasks:

```sh
mise run check
mise run build:scaffold
```

These commands alone cannot certify M1 while its implementation/tests are absent. Existing stub/manifest/CLI tests retain their infrastructure scope. No setup:llvm or native experiment reproduction is required for M1.

Before declaring M1 complete:

- Map every acceptance row to real tests and their observed results.
- Verify LLVM-free compilation and dependency direction.
- Record the implementation commit, host, tool versions and commands.
- Preserve approved policy behavior and original-source positions.
- List unsupported, incomplete and unrun checks explicitly.
- Review the diff for unrelated changes and historical-artifact preservation.

Keep product build, language/stdlib/runtime/client suites and release verification nonzero while their real scopes are unavailable. M1 does not enable a successful `rill check`, compile/run/test command or language server. Parser tests later do not prove complete source checking either.

## Completion record

M1 implementation: Not started. Acceptance matrix: NotChecked. No parser, lexer or source-mapper result is claimed by this planning document.

The documentation baseline records existing policy decisions without upgrading full RFC statuses, evidence labels or target/client support. The implementation ticket will record its exact base and later observed evidence separately.
