# Compiler front end

This crate implements M1 source snapshots/lexing, the M2 parser, M3 module resolution/declaration collection, M4.1 explicit declaration types and M4.2 declaration/inline-containment checks, and M4.3 scoped scalar/control typing. It depends on neither LLVM nor LSP. Parsing success does not establish successful source checking or Rill execution. Product commands and acceptance gates remain unavailable.

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

The owner approved O1 through O7 and the three test seams at checkpoint `a41e7cc955e67329d6dc3a8a69de667e0af204ac`. [Ticket #54](https://github.com/Reilley64/rill/issues/54#issuecomment-6939478619) records approval. [RFC 0002](../../docs/rfcs/0002-modules-and-program-loading.md#proposed-m3-resolution-procedure) owns the scoped contract. Its full status remains Draft. A resolved graph is not a successfully checked program.

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

## M4.1 explicit declaration types

The owner approved O1 through O4 at checkpoint `71cbe3707814114b0f504f0458ada41d54c2593d`. [Slice #57](https://github.com/Reilley64/rill/issues/57) belongs to open [M4 umbrella #56](https://github.com/Reilley64/rill/issues/56). RFCs [0005](../../docs/rfcs/0005-types-records-enums-and-inference.md#proposed-m41-canonical-declaration-type-procedure) and [0004](../../docs/rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m41-explicit-header-elaboration) own the scoped procedure. Their full statuses remain Draft.

`analysis::analyze_declaration_types(&Graph, AnalysisLimits)` consumes the retained M3 graph without filesystem reads or reparsing. It elaborates every module-level record/enum annotation and named-function/family parameter/result annotation. The result borrows the graph, so all canonical identities and source snapshots remain available.

A successful `Receipt` means only declaration-type elaboration completed. `completion()` lists declaration validity, signature compatibility, position validity, layout, bodies, startup, expectations, inference, coverage, evidence, ownership and execution as NotPerformed. The receipt is not a checked program or callable implementation.

| Query or fact | Meaning |
| --- | --- |
| `receipt.declaration(&DeclarationId)` | Read-only binders and ordered record fields, enum payloads or explicit function parameter/result types. A genuine out-of-scope declaration returns None. A foreign or unknown identity returns InvalidRequest. |
| `receipt.type_use(&Module, NodeId)` | Canonical annotation term, ordered nominal substitution, original span and source-binding provenance. Use the actual module from the retained graph. Foreign modules and invalid node indices return InvalidRequest. An existing non-annotation node returns None. |
| `declarations()` and `type_uses()` | Deterministically ordered read-only iteration over the same facts. Declaration order follows M3 identity order. Type-use order follows module, original byte start and node index. |
| `TypeTerm` | Actual graph-scoped primitive, binder, nominal or variant identities; structural tuples, references and written callable modes. Exact equality performs no widening, inference or callable-mode conversion. |

Source type lookup checks declaration binders, then installed module bindings, then primitive prelude identities. Module qualification uses installed exported bindings only. It never autoloads children. Variant qualification rejects multiple same-spelled variants instead of choosing by traversal order. Private barriers retain diagnostic-only binding/candidate evidence.

Generic applications need full written arguments in this stage. Missing arguments, generic defaults, where clauses and implementation headers return Incomplete with original-node obligations. Requirements are neither proved nor disproved. Excess arguments, duplicate binders and invalid source type names/targets produce type findings. No Error or Unknown term represents a valid type.

Failure exposes explicitly partial facts and incomplete secondary diagnostics, never a completed receipt. Resource exhaustion takes priority over incomplete obligations, which take priority over source rejection. Known findings remain available in every failure category. Queries do not reinterpret failures as checked artifacts.

Nominal terms never expand their fields. Recording `Growing<Box<T>>` therefore does not run a recursive evidence or layout search. Unused duplicate members, void/never position legality, finite layout and implementation compatibility remain unchecked. Supplied copy/dispose annotations receive no library-compatibility or trusted-operation certification. This slice fabricates no standard-library definitions.

Default limits permit 1,000,000 work units, 100,000 retained/index entries and 64 annotation traversal levels. Traversal has an absolute ceiling of 64. Entries include analysis indexes, type terms, written uses, members, substitutions, provenance references and findings. Checked counters charge before growth. Work charges scans, traversal and indexed lookups. Limits do not promise wall-clock bounds, precise heap-byte quotas or host OOM recovery.

The [acceptance record](tests/m4-1-acceptance.md) maps the public-seam tests in `tests/m4_1.rs`. Run them with `mise exec -- cargo test --locked -p compiler --no-default-features --test m4_1`. No Rill code, LLVM, ownership mechanism or editor behavior executes in this suite. M4.1 leaves all product commands and unavailable gates unchanged.

## M4.2 declaration and source-nominal containment checks

The owner [approved D1 through D4](https://github.com/Reilley64/rill/issues/59#issuecomment-6945089123) at checkpoint `c26d737439fd4c426d362655ad15984cc70108dc`. [Slice #59](https://github.com/Reilley64/rill/issues/59) remains in the same draft M4 PR #58. The umbrella stays open. The proposed sections in RFCs 0004/0005 own the scoped rules; full RFC statuses remain unchanged.

`analysis::declarations::check_declarations` requires a completed M4.1 Receipt, explicit PreludeContracts and AnalysisLimits. Both canonical prelude identities and their fixed schemas are mandatory. A foreign identity, unrelated declaration or schema swap returns InvalidRequest before source checking. A wrong actual copy/dispose signature produces a source finding. Signature agreement grants no runtime trust or protected-operation permission.

The stage checks duplicate record fields, enum variants, record-payload fields and named value parameters, including unused declarations. Repeated `_` parameters bind no name. It admits void/never annotations without supplying inhabitation, constructors or normal continuation.

The layout procedure computes least-fixed-point inline-parameter summaries, then canonical containment edges. It preserves the originating declaration when traversing forwarded arguments. It rejects inline cycles with R0406, including type-growing cycles, without expanding nominal definitions. References stop traversal. Stored variant types use their parent's complete representation. No nominal name or private field grants indirection.

A directly stored callable without an established representation contract produces Incomplete. Signature parameter/results are not capture storage. Reference referents and unused nominal arguments do not introduce stored dependencies. Known findings remain visible when other work is incomplete or reaches a resource limit.

| Interface or fact | Scope |
| --- | --- |
| `receipt.declaration(&DeclarationId)` | Checked member uniqueness and any required prelude schema agreement. Original M4.1 type facts remain available. |
| `receipt.layout(&DeclarationId)` | Inline-parameter evidence and containment edges for one canonical record/enum. Function and genuine out-of-scope member identities return None. Foreign/unknown IDs reject as request misuse. |
| `layout.inline_parameters` | Ordered binder indices with written-storage provenance, forwarding arguments and earlier summary-generation references. A pending fact cannot prove itself. |
| `layout.edges` | Canonical origin/target identities and the least written witness under the approved ordering. Forwarding retains actual argument types and selected summary evidence. |
| `layout.complete` | Storage-summary knowledge only. Even a complete summary can have a proved cycle; this flag is not layout validity or successful checking. |
| Failure partial facts | Known declarations, summaries, edges, cycle/member/signature findings and unresolved storage obligations. No completed receipt is exposed. |

Completed checks are MemberUniqueness, PreludeSignatures, ExistingTypePositions and SourceNominalInlineContainment. Full declaration/general signature validity, full layout including captures/target ABI, public interfaces, bodies, evidence, ownership and execution remain unperformed. The receipt is not complete source checking.

Limits use the existing AnalysisLimits shape. Work and entry counters charge before growth. Entry accounting includes temporary indexes, route reservations and graph traversal storage without refunds, so limits are conservative. Containment graph algorithms are iterative; term traversal has the existing absolute 64-level ceiling. These budgets are not source restrictions, precise heap quotas or wall-clock/OOM guarantees.

[The M4.2 acceptance record](tests/m4-2-acceptance.md) maps real tests in m4_2.rs. The separate Python model in tests/models was validated before compiler implementation. Its ReferenceExecuted results do not replace direct public-interface Rust tests or establish Rill execution. Neither layer enables a product command or adds M5 work.

## M4.3 scoped scalar and control typing

The owner [approved C1 through C4](https://github.com/Reilley64/rill/issues/60#issuecomment-6947086957) at checkpoint `e4750b8579ea11aa80f76f86f93014c78854fac0`. [Slice #60](https://github.com/Reilley64/rill/issues/60) stays in draft PR #58. RFCs 0003/0004/0005/0006 own the bounded procedure. Full RFC statuses remain unchanged.

`analysis::scalars::check_scalars` requires a completed M4.2 DeclarationReceipt and AnalysisLimits. It checks supported standalone bodies, loaded startup statements and expectations without executing Rill. It reuses captured M1/M2 facts and canonical M3/M4 types, including body annotation uses. It does not reopen sources or reinterpret syntax.

| Query | Scoped facts |
| --- | --- |
| `receipt.body(module, BodyOwner)` | Supported scalar/control completion for an actual function, expectation or module startup context. Retains finite exit facts and declared result context. |
| `receipt.node(module, NodeId)` | Canonical expression types, literal evidence, explicit call substitutions, binding causes, original annotation uses and control targets. Statement/annotation facts are not expression values. |

Queries reject foreign modules, body identities and invalid node indices. Missing node facts are not fabricated types. Incomplete attempts return no ScalarReceipt. Partial facts retain known findings and explicit obligations; secondary diagnostics remain incomplete.

Literal checking has no numeric default. Finite contextual propagation precedes literal finalization. The primitive matrix uses concrete canonical primitives, not nominal spellings. Never admission does not retag declared intermediate results. Explicit standalone calls use one substitution across their complete signature, without body inspection or overload retries.

Boolean/exit analysis follows the approved finite proof. It uses literals, logical operations and immutable Boolean initializer facts. It omits cross-statement correlations and arithmetic/comparison evaluation. It checks written source even after normal continuation ends. Unsupported scope-changing constructs block dependent checking rather than fabricate missing-name proofs.

Static places distinguish writable, read-only and value contexts. Descriptor borrowing does not silently become referent borrowing. These facts establish no physical owner address, access loan, initialized storage, move, implicit copy or cleanup permission.

Data/pattern protocols, ordinary families, omitted generic inference, callable creation/invocation, interpolation, errors and remaining core integration stay incomplete. Nested declarations and mutable temporary acquisition also retain later M4 obligations. `not_performed()` lists full body/signature/position/inference/layout/coverage/evidence/ownership/execution stages. A scoped receipt is not a checked program or completed M4.

The [M4.3 acceptance record](tests/m4-3-acceptance.md) maps direct public-interface tests and separate verification evidence. No product routing, native/LLVM dependency, shipped standard definition or M5 behavior is added.

## M4.4 scoped data and pattern typing

The owner [approved K1 through K4](https://github.com/Reilley64/rill/issues/61#issuecomment-6957273163) before implementation and later [clarified simple binding value types](https://github.com/Reilley64/rill/issues/61#issuecomment-6959565199). The [M4.4 acceptance record](tests/m4-4-acceptance.md) separates the finite reference model, direct compiler assertions and remaining gates.

`analysis::data::check_data` takes completed M4.2 input and AnalysisLimits. It extends the existing scalar/control engine. Its DataReceipt has the same body/node query boundaries, not a second parser or a dependency on scalar-only success for data-bearing source.

Node facts retain named construction and evaluation order, complete substitutions, canonical field projections, actual variants and explicit admissions. Pattern facts retain source sites, binder identities, source types and projection dependencies. Qualified arm views preserve declared parents outside their scope and retain later tag-stability requirements. Lexical assignment checks do not prove alias or loan safety.

Static constructor coverage handles nested products, finite scalar domains and guarded rows. Only Root zero-constructor domains are vacuous. Nested zero-constructor Components are symbolic holes that cannot erase another column. Uncovered witnesses describe declared shapes, not inhabited Rill values. Full expression exit facts distinguish returns and actual loop targets from divergence, including skipped-source alternatives.

A simple name binding keeps its initializer's value type. Reading a borrowed string field into an owning string does not gain an implicit copy or transfer permission. Borrowed destructuring uses the separate fixed classification rule. Unresolved aggregate classification remains incomplete.

Tuple-context brackets need no fabricated library definition. List construction, indexing and Option/Result guards retain missing canonical core admission explicitly. General inference/defaults, families, callbacks, conversions and other remaining typed obligations stay within M4.5 through M4.8. This receipt grants no ownership, storage initialization, implicit-copy execution, cleanup, runtime, codegen or editor acceptance.

## M4.5 ordinary input analysis

The owner [approved P1 through P4](https://github.com/Reilley64/rill/issues/62#issuecomment-6963439018) before implementation. The [M4.5 acceptance record](tests/m4-5-acceptance.md) separates model results, compiler assertions and unrun gates.

`analysis::ordinary::analyze_ordinary` takes an M3 graph and AnalysisLimits. It reuses declaration elaboration and M4.2 prerequisites without requiring the bounded M4.1 entry to accept implementation headers. It preserves the original headers and checks each implementation against one complete family substitution. Written defaults and requirements remain separate from input matching.

Query `analysis.family(id)` for canonical implementation records, installed routes, normalized input keys, placement and comparisons. Repeated variables capture exact established static terms. Fixed outer parent, shared-reference and callable-mode admissions do not extend into nested invariant types. A comparison witness identifies the admitted alternatives; it does not prove an inhabited value.

`select_inputs` takes established static input contracts and optional leading family arguments. Explicit input-relevant arguments filter matches. They do not change the original patterns' priority. Output-only arguments and selected requirements remain pending. This entry does not supply a source expression context or use a callee's body.

M3 supplies no canonical role and protection catalogue. The report keeps those obligations and labels every settled selection hypothetical. A missing candidate, unfinished prerequisite or exhausted budget cannot establish absence. An ordinary input fact is not a checked call, a provider clearance, or completed source checking. Actual admission remains in M4.8.

## M4.6 generic inference and symbolic evidence

`analysis::generics::analyze_generics` uses the ordinary inventory and the shared scalar and data checker. It checks supported bodies with rigid declaration binders. Each call or nominal application has its own substitution. Independent input constraints run before a ready written default. Unsuffixed numbers still need a receiving type.

| Query | Facts |
| --- | --- |
| `body`, `body_status`, `node` | Scope completion, supported types and callable invocations, and remaining obligations. |
| `ordinary_call`, `ordinary_value` | Conditional input context, selection and output agreement. They cannot clear an unadmitted role. |
| `arrow` | Signature and capture sites, original storage, access demands and any known invocation mode. An unresolved mode can leave a signature without a checked invocation. |
| `goal`, `goals` | Exact judgments, caller assumptions, dependencies, result witnesses and blockers. `goal` rejects IDs issued by another report. |
| `annotations`, `declaration_issues` | Captured annotation and default provenance, plus retained declaration and layout issues. |

The checker keeps independent output determination separate from a request to prove a particular result. It does not try another ordinary body when selected evidence fails. A repeated active goal stays unresolved; exhaustion on a growing type is a resource limit. Neither is proof of absence.

The [M4.6 verification record](tests/m4-6-acceptance.md) identifies the reviewed and tested revisions and lists the remaining inference cases. Core role and protection admission still belong to M4.8. These facts grant no ownership or execution permission.
