# M4.2 acceptance record

## Scope and authority

M4.2 slice ri-9y3 remains part of open M4 umbrella ri-b83, in existing draft PR #58. The owner approved D1 through D4 (ri-9y3, comment 6945089123) at exact checkpoint `c26d737439fd4c426d362655ad15984cc70108dc`. The overall M4 starting main is `ee12588f2cba912025900e1ac587586fb9cb6235`.

The approved procedures live in [RFC 0004](../../../docs/rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m42-header-validity-and-prelude-signatures) and [RFC 0005](../../../docs/rfcs/0005-types-records-enums-and-inference.md#proposed-m42-declaration-validity-and-inline-containment). Approval covers existing type-position admission, parent representation for stored variants, honest unknown storage, exact canonical prelude contracts and the three public TDD seams. Full RFC statuses and earlier approvals remain unchanged.

This stage checks member/header uniqueness, required prelude signatures, approved type positions and source-nominal inline containment. It does not certify full declaration or general signature validity, public interfaces, capture layout, target ABI/size, bodies, evidence, ownership or execution. A completed stage receipt is not a checked program or completed M4.

## Reference model before compiler implementation

The separate finite model is [models/m4_2_layout.py](models/m4_2_layout.py), with expected cases in [models/test_m4_2_layout.py](models/test_m4_2_layout.py). The following command passed all eight tests before any M4.2 Rust implementation was written:

```sh
mise exec -- python -m unittest discover -s crates/compiler/tests/models -p 'test_m4_2_layout.py'
```

This is ReferenceExecuted evidence. It includes independently stated summary/edge sets for forwarding, nested wrappers, phantom arguments, empty declarations, direct/mutual/growing cycles, parent variant representation, callable unknowns and references. Visibility does not alter model storage fields; a same-spelled List or Shared has no exemption. The model also checks every two-vertex graph, with a separate direct cycle oracle across all 16 edge subsets and reversed input order.

The model uses transitive closure for cycle classification. The Rust implementation uses iterative strongly connected components and cycle witnesses. Both preserve the original graph origin during forwarded argument traversal. Controlled model limits return resource failure, never finite-layout success or a fabricated cycle proof.

The initial model tests failed for the absent module, missing cycle rejection, unsupported storage forms and absent limits. Each passed after its behavior existed. Later boundary assertions also passed. These bounded cases are not a proof of soundness/completeness or a real Rill execution. The model omits compiler identities, source loading and signature validation; direct compiler tests own those assertions.

## Direct public-interface compiler acceptance

All 17 tests below exist in [m4_2.rs](m4_2.rs) and passed in the focused run. Tests create real source trees, use M3 load and M4.1 elaboration, then cross check_declarations and the declaration/layout queries. They do not test private graph/solver helpers instead of the approved seams.

| Approved requirement | Direct tests | Observations and boundaries |
| --- | --- | --- |
| Canonical contracts and completion | `canonical_prelude_signatures_and_scoped_completion_use_actual_identities` | Actual selected copy/dispose identities match the fixed schemas. Completion lists the exact scoped checks and unperformed bodies/ownership/execution. Signature agreement does not prove runtime behavior. |
| Each required prelude contract | `each_required_prelude_contract_is_checked_even_when_selectors_share_a_family` | A structurally admitted same-family copy/dispose selection cannot pass only one schema check. Both contracts are checked independently, and the incompatible dispose shape rejects. M3 remains unchanged. |
| Signature rejection and request separation | `mismatched_prelude_signatures_reject_and_contract_data_cannot_change_the_policy` | Rejects wrong arity, binder count/relationship, reference access and result, including a same-spelled nominal void. Binder/parameter renaming is valid. Schema swaps, unrelated declarations and foreign captures are InvalidRequest even with zero analysis budgets. Local copy spelling grants no canonical role. |
| Member scopes and unused duplicates | `duplicate_members_and_parameters_reject_even_when_unused` | Rejects duplicate record/private fields, variants, record-payload fields and named parameters. Retains all conflicts and source spans. Distinct scopes can reuse names; repeated underscores and separate type/value namespaces remain valid. |
| Void/never positions | `approved_void_never_positions_do_not_supply_inhabitation_or_hide_cycles` | Admits the approved annotation positions without claiming constructors or reachable values. A never field cannot hide a by-value cycle. Empty enums remain uninhabited and do not remove storage edges elsewhere. |
| Summary forwarding and finite nesting | `layout_summaries_forward_parameters_without_expanding_finite_wrappers` | Preserves ordered binder summaries, actual forwarded arguments and earlier finite proof generations. Accepts Box<Box<i32>> and Phantom<Node> without invented wrapper self-cycles. Rejects enum recursion with an original-source witness. |
| Tuple and all enum payloads | `tuple_and_every_enum_payload_forward_only_their_stored_parameters` | Unions tuple/record variant payload dependencies and preserves parameter positions. Read-only/mutable references stop inline traversal. Does not use function signatures as captured storage. |
| Parent variant representation | `stored_variants_use_parent_summaries_without_erasing_variant_identity` | Implements approved D2, including rejected Choice<Node>.Empty. A phantom argument can remove actual stored recursion. Canonical variant typing facts remain distinct from parent representation analysis. |
| Genuine versus fake indirection | `growing_mutual_and_fake_indirect_source_types_reject_even_when_unused` | Rejects mutual/growing cycles and fake List/Shared indirection, including an exported standard-origin List with private recursive storage. Names, origin and privacy do not grant representation trust. |
| Unknown callable storage | `callable_storage_is_incomplete_but_references_and_unused_arguments_stop_it` | Direct storage, generic applications and transitive nominal storage remain incomplete. References, signature-only callables and unused arguments do not invent capture edges. Known cycle findings survive incomplete storage. Later M4 integration must discharge unknown capture/core obligations. |
| Cycle code and original provenance | `cycles_have_r0406_and_original_unicode_crlf_storage_witnesses` | R0406 retains actual snapshot identity and original Unicode/CRLF byte positions, including private storage. Duplicate/signature findings allocate no new stable code. |
| Forwarding and source routes | `summary_provenance_and_cycle_forwarding_retain_import_routes_after_live_edits` | Retains canonical endpoints, actual forwarded type arguments, alias routes and original spans after live files change. Uses retained facts without filesystem reads or current-file freshness claims. |
| Determinism, witnesses and request identity | `repeated_relocated_queries_keep_ordered_facts_and_capture_isolation` | Preserves semantic summary/edge/witness order and least member witness across repeated/relocated inputs. Fresh captures remain unequal; foreign IDs reject. Genuine out-of-scope field/function identities have no fabricated layout receipt. |
| Controlled limits and partial findings | `controlled_limits_are_inclusive_and_exhaustion_preserves_known_errors` | Exact work/entry/depth budgets complete; one-short and zero budgets return ResourceLimit. Known duplicate and R0406 findings survive later exhaustion. Raised limits do not wrap counters or become language validity rules. |
| Independent cycle cases | `all_two_vertex_graphs_match_the_independent_reference_cycle_cases` | All 16 concrete source graphs match the model's independent cycle outcomes and expected cyclic-component counts. This finite inventory is not a universal proof. |
| Long graphs without host recursion | `long_graphs_and_cycles_use_iterative_containment_checking` | A 240-declaration chain completes at term depth one. Its cyclic version retains a 240-step witness through iterative graph analysis. A low work budget returns ResourceLimit. |
| Earlier-stage incompleteness | `unsupported_annotation_inputs_never_supply_a_declaration_check_receipt` | Defaults, requirements, implementations and missing type arguments still prevent an M4.1 receipt. M4.2 requires that successful receipt type and cannot consume failed partial facts. |

## Owning-RFC provenance and TDD observations

The input provenance is the original M4 main baseline plus the scoped M4.2 checkpoint identified above. RFC 0005 T13/T15/T16 supply direct, nested, phantom and type-growing layout cases. T14's true indirect-container distinction is exercised with real reference boundaries and rejected fake names; no standard container definition or trusted ABI is fabricated. The approved D2 example adds parent-variant storage behavior not inferred from earlier approvals.

RFC 0004's fixed-reference and explicit-header rules, plus library catalogue section 2, supply actual copy/dispose signature relationships. M1/M3 and RFC 0010 supply retained source/capture identity. No prototype or recorded experiment bytes changed.

After model validation, the first Rust tracer failed because analysis::declarations did not exist. The layout query/summary/cycle tracer then failed before that procedure existed. The same-selector regression reproduced an accepted result after only one contract was checked; checking both fixed it. The cycle-code test failed before R0406 was exposed and passed afterward. Focused tests and typechecking ran throughout. Additional direct boundary assertions exercised the same approved interfaces.

No earlier-stage implementation bug fix was needed. M1 through M4.1 source and tests remain unchanged except the new child-module export in analysis.rs. There is no ownership, disposal, MIR cleanup, runtime or editor implementation in these tests.

## Limits and remaining scope

Limits use AnalysisLimits with default 1,000,000 work units, 100,000 charged entries and 64 term traversal levels. The absolute traversal ceiling remains 64. Accounting includes temporary indexes, routes and graph traversal storage without refunds. It is conservative and does not promise exact heap bytes, wall-clock completion or host OOM recovery.

LayoutFacts.complete means storage-summary knowledge, not acyclicity or a valid target representation. Failed attempts expose explicitly partial facts and incomplete secondary diagnostics, never a completed stage receipt. ResourceLimit takes priority over Incomplete, which takes priority over source rejection; known findings remain retained. InvalidRequest validates the supplied contract identities/schemas before analysis.

Direct unknown callable storage remains incomplete. This slice admits no nominal indirection exemptions. Public-interface visibility, implementation-family compatibility, capture representation, target ABI/size, bodies, generic evidence, ownership and execution remain unperformed. Those pending typed obligations remain assigned to later M4 slices; they are not silently moved beyond M4 completion. Unavailable product gates remain unchanged and nonzero.

## Verification and final review

Before final independent review, these commands passed on macOS 27.0.1 AArch64 with mise 2026.11.4, Rust/Cargo 1.98.0, Python 3.13.18 and uv 0.13.8:

- The explicit Python model command above, eight reference tests.
- `mise run test:compiler`, 175 compiler tests comprising 158 unchanged existing tests and 17 M4.2 tests. Reference-model counts are separate.
- `mise run test`, including unchanged CLI unavailable-command checks.
- `mise run check`, including formatting, LLVM-free Clippy and existing tool/fixture/model checks. Documentation validation checked 1637 local destinations with zero errors.
- `mise run build:scaffold`, the unchanged CLI scaffold.
- Focused compiler checks and `mise exec -- cargo check --locked -p compiler --no-default-features` during implementation.
- `mise exec -- cargo tree --locked -p compiler --no-default-features`, with no compiler dependencies.
- `git diff --check` and the earlier-stage preservation audit.

User-global CMake/Ninja/Go lock warnings did not change project locks. No workflows changed, so local check:ci was unrun. M4.2 hosted/other-host checks, product build/suites, release verification, LLVM and native experiments were unrun. Earlier M4.1 hosted evidence retains its exact source revision and failed unavailable runtime gates. No real Rill program executed.

## Independent final review

Both axes reviewed the staged M4.2 diff against checkpoint `c26d737439fd4c426d362655ad15984cc70108dc` before the implementation commit. Neither reviewer edited files or reran the reported checks. The Spec reviewer did not independently verify historical model-first/TDD execution order.

### Standards

No documented-standard violation was identified. One low-priority Data Clumps heuristic identified identical containment-edge and cycle-step provenance shapes. The local correction stores charged clones of ContainmentEdge values in cycle findings. It removes the redundant CycleStep shape without changing the approved interfaces' behavior or accounting. All 17 focused compiler tests passed after this correction.

### Spec

No actionable findings were identified. The staged implementation matched canonical contracts, D1 through D3 storage rules, finite evidence, origin-preserving forwarding, ordered iterative cycle witnesses and incomplete/resource boundaries. The later unstaged provenance consolidation was excluded from this independent staged review.

The correction is locally verified without claiming another delegated review. After it, the eight model tests and test:compiler, test, check and build:scaffold passed again through mise. The compiler count remained 175, and documentation checks found zero errors across 1637 destinations. ri-9y3 and PR #58 record the publication revision. Formal ASD-STE100 dictionary compliance remains unverified. PR #58 remains draft and unmerged; ri-b83 stays open.
