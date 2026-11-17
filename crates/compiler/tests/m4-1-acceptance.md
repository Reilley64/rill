# M4.1 acceptance record

## Scope and authority

[M4.1 slice #57](https://github.com/Reilley64/rill/issues/57) belongs to open [M4 umbrella #56](https://github.com/Reilley64/rill/issues/56). The branch starts at fetched main `ee12588f2cba912025900e1ac587586fb9cb6235`, after PR #55 merged. The owner [approved O1 through O4](https://github.com/Reilley64/rill/issues/56#issuecomment-6943516603) and all three public TDD seams at checkpoint `71cbe3707814114b0f504f0458ada41d54c2593d`.

RFCs [0005](../../../docs/rfcs/0005-types-records-enums-and-inference.md#proposed-m41-canonical-declaration-type-procedure) and [0004](../../../docs/rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m41-explicit-header-elaboration) own this scoped procedure. Full RFC statuses and earlier approvals remain unchanged. Historical absent-implementation evidence is not a current status claim.

Tests in [m4_1.rs](m4_1.rs) create real source trees, call M3 load and cross the approved analysis entry, declaration query and type-use query. They inspect the compiler's actual annotation facts. They do not invoke private solver helpers or execute Rill programs. A receipt means declaration-type elaboration only, not complete M4, a checked program, ownership safety or execution.

## Approved requirement mapping

Every named test below exists in m4_1.rs. The initial focused suite passed all 20 tests. Positive, rejected and boundary assertions stay within annotation elaboration; later-stage cases are not represented as passed.

| Approved requirement | Actual direct tests | Observations and limits |
| --- | --- | --- |
| Primitive identities | `explicit_function_types_use_actual_prelude_identities_and_retained_spans`; `all_seventeen_primitive_terms_are_canonical_and_void_never_are_distinct`; `invalid_annotation_names_targets_arities_and_binders_reject_without_placeholders` | Uses actual M3 prelude identities for all 17 primitives. Distinguishes void and never. Rejects int, missing List and illegal type arguments. Position legality and numeric inference remain unperformed. |
| Nominal identity and aliases | `record_aliases_preserve_nominal_identity_and_ordered_substitution`; `multiple_type_arguments_and_same_spelled_variants_keep_their_order_and_owner`; invalid-annotation test above | Imported aliases preserve declaration identities. Point and ScreenPoint differ despite equal fields. Type arguments retain order. Excess arguments reject; missing arguments are incomplete. No assignment or construction compatibility is claimed. |
| Binders and correlations | `source_binders_then_module_bindings_shadow_the_primitive_prelude`; `multiple_type_arguments_and_same_spelled_variants_keep_their_order_and_owner`; invalid-annotation test | Repeated occurrences share a binder identity. Separate same-spelled binders remain distinct. A/B substitutions retain their original identities and positions. Duplicate binders reject. No overload-vector ranking occurs. |
| Enums and variants | `variants_keep_parent_arguments_payload_shapes_and_ambiguous_candidates`; `multiple_type_arguments_and_same_spelled_variants_keep_their_order_and_owner`; `unused_duplicate_members_are_not_mistaken_for_declaration_validity`; invalid-annotation test | Preserves parent applications, variant ownership and unit/tuple/record payload forms. Missing variants and selection on records reject. E.A with duplicate A variants rejects with both identities/spans. Unused duplicates remain explicitly unchecked. |
| Tuples, references and callables | `structural_types_preserve_tuple_arity_reference_access_and_callable_modes` | Distinguishes zero/one-position tuples, element order, void, nested reference access and all written callable modes. Grouping preserves exact type identity. No mode weakening, invocation, capture inference or variance is claimed. |
| Source lookup and visibility | `source_binders_then_module_bindings_shadow_the_primitive_prelude`; `private_qualified_types_retain_the_blocked_binding_without_granting_access`; `private_intermediate_modules_and_unimported_metadata_do_not_supply_type_names`; invalid-annotation test | Uses binder/module/prelude precedence and installed exports only. Retains private diagnostic evidence without traversing private modules. Own-module private signatures remain nameable. Metadata creates no caller binding. Existing child files do not autoload. Field/representation access remains unchecked. |
| Unsupported obligations and missing context | `unsupported_headers_and_recursive_goals_are_incomplete_not_evidence` | Defaults, requirements, implementations and missing arguments return Incomplete, never source-invalidity merely because unsupported. Recursive and growing requirements do not start proof search or supply negative evidence. This does not test a later evidence solver. |
| Type rejection and diagnostic provenance | `malformed_nominal_applications_keep_the_resolved_declaration_and_import_route`; private-type and invalid-annotation tests | Known nominal targets and private barriers retain source spans, declaration candidates and binding routes. No invalid placeholder becomes a valid type. Secondary diagnostics remain explicitly incomplete. No new stable diagnostic code is allocated. |
| Recursion and finite annotation terms | `recursive_and_type_growing_nominals_remain_finite_unexpanded_terms`; `long_explicit_annotation_inputs_are_bounded_without_expanding_definitions` | Direct/mutual/growing references remain finite terms without field expansion. Empty records/enums remain distinct shapes. An 800-field explicit input elaborates within budgets. This is not finite-layout or recursive-evidence acceptance. |
| Ambiguity | `variants_keep_parent_arguments_payload_shapes_and_ambiguous_candidates`; `ambiguous_m3_inputs_never_supply_a_graph_to_typed_analysis` | Duplicate variants reject at annotation lookup. M3 route ambiguity yields no analyzable Graph. Typed overload ambiguity remains M4.5. |
| Snapshots, spans and capture isolation | `retained_unicode_crlf_routes_and_capture_identity_survive_live_changes`; explicit-function test | Retains Unicode/CRLF byte positions and alias routes. Cross-capture IDs/modules and invalid node indices reject as request misuse. New live bytes do not alter retained facts. No live-filesystem freshness claim follows. |
| Deterministic order and resource limits | `repeated_and_relocated_analyses_have_the_same_ordered_semantic_facts`; `annotation_limits_are_inclusive_deterministic_and_preserve_known_findings`; long-input test | Repeated retained-graph types compare equally. Relocation preserves ordered semantic/source projections without equating capture IDs. Exact work/entry/depth limits complete; one-short limits fail as ResourceLimit. Known findings survive later exhaustion. No wall-clock or OOM guarantee. |
| Explicit completion and actual supplied core | `ignored_source_is_explicitly_unperformed_and_supplied_core_is_not_certified`; `unused_duplicate_members_are_not_mistaken_for_declaration_validity`; explicit-function test | Lists declaration/signature/position validity, layout, bodies, startup, expectations, inference, coverage, evidence, ownership and execution as unperformed. Even structurally admitted but wrong copy/dispose library signatures gain no certification. No standard definition or trusted operation is fabricated. |

## Owning-RFC adaptations

The source baseline for these adaptations is `ee12588f2cba912025900e1ac587586fb9cb6235`, plus the approved M4.1 procedure at `71cbe3707814114b0f504f0458ada41d54c2593d`.

- RFC 0005 T1/T4 and RFC 0002 M1 supply nominal/alias identity cases. Tests compare annotation facts, not record assignment or construction outcomes.
- RFC 0005 T3 supplies parent-applied variant annotations and payload shapes. Tests do not establish constructor inference or matching.
- RFC 0004 F1/F7 supply symbolic binder/correlation prerequisites. Tests preserve identities and substitutions without claiming family selection.
- Syntax G13/G15/G17 supply nested reference, callable, finite tuple and empty nominal shapes. M2 retains syntax acceptance ownership.
- RFC 0005 T13/T15/T16 supply direct, nested, phantom and growing references. Tests record their finite annotation terms; layout remains NotPerformed.
- RFC 0002 M2/M15 and RFC 0010 source identities supply ambiguity, visibility and provenance boundaries. No field-access, reporter or client result is claimed.

Historical prototype/example bytes remain unchanged. No ownership or runtime expectation is converted into a typing assertion. In particular, arithmetic traps, loan transfer, capture validity, disposal and error-wrapper behavior are not checked here.

## TDD observations

The first tracer failed to compile because compiler::analysis did not exist. Its minimal implementation elaborated explicit function types and the actual supplied prelude headers. Subsequent public tests first failed for absent nominal/substitution, variant, structural-type and private-diagnostic behavior, then passed after those features existed.

The completion-marker test first failed because DeclarationValidity was not listed. The receipt now marks it unperformed. The malformed nominal diagnostic test failed with no canonical candidate; the correction retains its declaration and import route. Additional positive/rejected/boundary assertions exercised the implemented behaviors through the same approved seams.

Focused m4_1 tests and Cargo checking ran during implementation. No earlier-stage source fix was needed. Source, lexer, parser, loader and all existing M1 through M3 tests remain unchanged, apart from exporting the new analysis module in lib.rs.

## Verification and remaining gates

Before final review, these commands passed on macOS 27.0.1 AArch64 with mise 2026.11.4, Rust/Cargo 1.98.0, Python 3.13.18 and uv 0.13.8:

- `mise run test:compiler`, 158 tests comprising the unchanged 138 existing tests and 20 M4.1 tests.
- `mise run test`, implemented LLVM-free Rust suites, including eight unchanged CLI process tests.
- `mise run check`, formatting, Clippy, documentation and existing tool/fixture/model checks. The documentation check validated 1625 local destinations with zero errors.
- `mise run build:scaffold`, the unchanged CLI scaffold.
- `mise exec -- cargo check --locked -p compiler --no-default-features` and focused Clippy with warnings denied.
- `mise exec -- cargo tree --locked -p compiler --no-default-features`, showing no compiler dependencies.
- `git diff --check` and the baseline preservation audit.

User-global CMake/Ninja/Go lock warnings did not change repository locks or selected base tools. Workflows did not change, so check:ci was not run. Other-host profiles were not run locally. Product build, language/stdlib/runtime/client suites, release verification, LLVM provisioning/smoke and native experiments were not run locally either. The later hosted evidence appears below. Existing infrastructure tests preserve unavailable product-command behavior. None of these observations establishes Rill execution.

Publication identifies the committed revision in #57 and the scoped draft PR. The independent review results follow below.

Unsupported in-scope forms remain generic defaults, where clauses, implementation headers and generic applications missing written arguments. Bodies and their nested declarations, startup and expectations are outside this stage. Incomplete obligations never produce a receipt.

The result borrows its immutable graph. Type-use provenance references the actual M3 bindings rather than cloning unbounded route histories. Default budgets are 1,000,000 work units, 100,000 retained/index entries and 64 traversal levels. The absolute traversal ceiling is 64. Counters are checked before growth. Host allocation failure, syscall latency and exact heap-byte bounds are not guaranteed.

## Final independent review

Both review axes examined the staged implementation and documentation against starting commit `ee12588f2cba912025900e1ac587586fb9cb6235`. Staged review allowed corrections before the implementation commit. Neither reviewer edited files or reran the reported checks.

### Standards

No documented-standard violation was identified. One low-priority Duplicated Code heuristic identified repeated member-annotation elaboration in fields and value parameters. The local correction reuses member_types for both, including storage and traversal accounting. The public interfaces and annotation rules remain unchanged. The focused 20-test suite passed after this correction.

### Spec

No findings were identified. The review checked the approved procedure against actual M3 identities, bindings, collection and retained-source behavior. It confirmed the annotation-only completion boundary and the separation of incomplete/resource outcomes from source rejection.

The correction was verified locally without another delegated review. After the correction, test:compiler, test, check and build:scaffold all passed again through mise. The compiler count remained 158 and the documentation check again found zero errors across 1625 destinations. No earlier-stage fix or policy change was required. #57 and the draft PR identify the publication revision. Formal ASD-STE100 dictionary compliance remains unverified.

## Hosted publication evidence

The [completed PR scaffold run](https://github.com/Reilley64/rill/actions/runs/44056791144) tested implementation revision `7602ed55f8b344646218e8cc0af8089bfee18efb`. Every configured job passed: ubuntu-24.04, ubuntu-24.04-arm, macos-15 and macos-15-intel. Inspected logs show all 20 M4.1 tests passing on each profile. The jobs also passed check, build:scaffold, check:ci and committed-lock preservation.

The [completed native-acceptance run](https://github.com/Reilley64/rill/actions/runs/44056790855) tested the same implementation revision. All four jobs failed. Each inspected log shows the unchanged unavailable test:runtime gate and process exit 2. These native/runtime gates remain failed, not passed or waived. Release verification did not run after that failure.

The checks used the workflow-pinned mise 2026.11.8. These results establish the named scaffold/compiler tests only. They do not establish runtime, target-backend, library execution, client or release acceptance. They supersede the initial incomplete hosted-check observations in #57 and PR #58, not historical local results.

This follow-up changes only the acceptance record. It does not claim that its later documentation commit has already completed hosted checks. PR #58 remains draft pending owner review and explicit merge authorization. M4.2 work has not started.

## Remaining milestone boundary

M4.2 through M4.8 remain unimplemented. No ownership, cleanup/MIR, runtime, backend or editor behavior was added. No successful rill check/build/run/test command is authorized. Unavailable product gates and historical evidence remain unchanged. The umbrella stays open, and this PR must not merge automatically.
