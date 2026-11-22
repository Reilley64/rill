# M4.3 acceptance record

## Scope and authority

M4.3 slice ri-3gd belongs to open M4 umbrella ri-b83. All eight slices stay in existing draft PR #58. The owner approved C1 through C4 (ri-3gd, comment 6947086957) at checkpoint `e4750b8579ea11aa80f76f86f93014c78854fac0`, before implementation. M4's overall starting main remains `ee12588f2cba912025900e1ac587586fb9cb6235`.

The approved owners are RFC [0003](../../../docs/rfcs/0003-expressions-bindings-and-control-flow.md#proposed-m43-scalar-and-static-control-procedure), [0004](../../../docs/rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m43-explicit-standalone-calls), [0005](../../../docs/rfcs/0005-types-records-enums-and-inference.md#proposed-m43-never-admission) and [0006](../../../docs/rfcs/0006-numbers-strings-and-primitive-operations.md#proposed-m43-literal-and-primitive-checking). Full RFC statuses and earlier scoped approvals remain unchanged. The checkpoint reviews and executed documentation checks remain historical design evidence, not final implementation review.

This slice checks supported scalar expressions, lexical bindings, static place requirements, explicit standalone calls and finite control proofs. It checks supported function bodies, loaded startup statements and expectations in both modes. A ScalarReceipt is not full source validity, completed typed analysis, ownership acceptance or Rill execution.

## Direct public-interface acceptance

All 28 tests below exist in [m4_3.rs](m4_3.rs). They pass in the focused LLVM-free compiler run. Fixtures supply actual source files, M3 requests and explicit canonical prelude selectors. Tests require successful M4.1/M4.2 receipts before crossing the approved scalar entry, body query and node query. No test calls private inference helpers.

| Approved requirement | Direct tests | Observed scope and boundaries |
| --- | --- | --- |
| Entry, completion and capture | `empty_void_body_has_scoped_completion_not_ownership_or_full_body_acceptance` | An empty void body completes only this scalar scope. Full body and ownership stages remain unperformed. The receipt retains the actual graph capture. |
| Literal return context | `return_context_establishes_literal_width_and_original_integer_evidence` | A declared i16 return establishes an unsuffixed literal's width. The fact retains exact magnitude and original snapshot/span. |
| Initializer scope and targets | `lexical_bindings_keep_canonical_targets_and_initializer_sees_outer_binding` | Inner initialization resolves the outer binding before introducing its own name. Later uses retain the correct canonical source identities. |
| Constraint closure | `primitive_constraints_wait_for_independently_typed_operands_before_literal_finalization` | Either independently typed operand can establish numeric context. The checker does not prematurely report missing width for the other literal. |
| Signed minima and rounding | `directly_negated_signed_minimum_and_float_rounding_keep_literal_evidence` | Grouped -128 at i8 admits the direct-literal exception. Floating underflow preserves negative zero. The f32 tie case has independently stated rounded bits. |
| Explicit correlated calls | `explicit_calls_retain_one_correlated_substitution_and_parameter_literal_context` | One explicit type argument supplies one canonical binder substitution across both parameters and the result. Parameter types contextualize argument literals. |
| Static access and assignment | `references_preserve_descriptor_and_referent_access_and_assignment_context` | Mutable-reference dereference is a writable referent, not mutable descriptor ownership. Compound/ordinary assignment supplies immediate type context. No loan or replacement-safety proof is claimed. |
| Never and short-circuiting | `literal_true_loop_proves_never_and_short_circuit_retains_a_normal_boolean_result` | A supported literal-true loop discharges the finite never-body proof. A skipped never operand does not remove a normal false result. |
| Exit merging | `conditional_noncontinuation_and_loop_transfers_survive_flow_merges` | Known conditional divergence survives normal alternatives. Current-loop continues are consumed at that loop's boundary. |
| Scalar joins | `scalar_conditional_joins_contextualize_literals_and_omit_only_canonical_never` | Declared and established sibling context supplies width. Only canonical never omits an ordinary join value. No already typed numeric value widens. |
| Startup and expectations | `startup_and_expectations_are_typed_in_both_modes_without_running_helpers` | Both modes check startup and Boolean expectations. A helper call uses its declaration rather than its body's constant result. A typed skipped zero division is not evaluated. |
| Intermediate mismatches | `never_operands_do_not_erase_intermediate_declared_result_mismatches` | The approved g(f(stop())) case retains actual i32 versus expected bool despite noncontinuation. R0101 and missing-width R0003 meanings remain distinct. |
| M1 text facts | `uninterpolated_templates_and_character_text_retain_m1_decoding_and_original_bytes` | Character, raw text and static template types reuse captured literal facts. The check does not establish backing-source or ownership permission. |
| Unresolved calls and privacy routes | `unresolved_family_calls_keep_canonical_identity_without_false_missing_literal_context` | Family calls remain incomplete with their actual identity. Pending argument context does not produce a fabricated R0003. Private qualification retains both actual alias/target bindings. |
| All integer boundaries | `every_integer_width_checks_exact_boundaries_without_a_numeric_default` | All ten integer widths check exact maxima and overflowing positive values. Signed minima pass. Computed/subtraction magnitudes, incompatible categories, float overflow, unsigned negation and width conflicts reject appropriately. |
| Closed primitive matrix | `closed_primitive_matrix_accepts_widths_and_mixed_text_observations_not_spelling_privilege` | All twelve numeric widths cover arithmetic. Integer bitwise/shifts and all owned/view/mutable text comparison pairs are checked. Char relations admit the approved order contract. References require explicit dereference outside the text rule. Nominal names and unconstrained binders gain no primitive operation. |
| Finite control policy | `finite_return_and_guard_proofs_keep_static_errors_and_actual_loop_targets` | Explicit else returns, guards, immutable Boolean facts and loop transfers supply supported proofs. Separate correlated tests and comparison evaluation do not. Inner-loop breaks cannot discharge an outer guard. Dead source still rejects invalid literals/types. Arithmetic traps stay typed, not executed or inferred as divergence. |
| Lexical shadowing with type arguments | `explicit_type_arguments_cannot_bypass_a_local_value_shadowing_a_standalone` | Explicit type application cannot skip a local binding to select a same-named module function. Unsupported value-callable/type-application work stays incomplete. |
| Failed binding causes | `failed_initializers_preserve_binder_causes_and_do_not_invent_missing_names` | Known errors retain original binder identities and blocked causes. Dependent uses do not become false absence proofs. Duplicates, pre-introduction uses and startup capture reject. Type/value namespaces remain separate. A repeated generic parameter cannot accept incompatible explicit arguments. |
| Unsupported contexts | `unsupported_source_never_completes_or_proves_missing_names_or_numeric_context` | Data/pattern/callback/family/generic/formatting/error/access gaps prevent a receipt, including unreachable forms. Scope-changing gaps block dependent checking. Independent supported bodies remain checkable. |
| Additional annotation uses | `body_annotation_nodes_expose_original_symbolic_type_uses_without_mutating_input` | The node query exposes original body annotations and canonical binders. Earlier immutable type receipts remain unchanged. |
| Controlled limits | `limits_are_inclusive_and_resource_exhaustion_retains_known_findings` | Exact work/entry/depth limits complete; one-short limits fail as resources. An earlier known literal error survives later exhaustion without becoming accepted source or missing evidence. |
| Flat chains | `long_flat_expression_chains_do_not_consume_host_recursion_depth` | A 2048-operation flat chain completes with depth budget eight. Expression collection, propagation and flow facts do not recurse along that AST chain. |
| Deferred type relationships and recursion | `permitted_unimplemented_type_relations_are_not_fabricated_mismatches` | Owned/read-only variant and callable-mode relationships stay incomplete for their later owners. Mutable variant widening rejects. Concrete and explicitly polymorphic function recursion uses declared signatures, not recursive proof search. |
| Original capture and request validation | `captured_unicode_crlf_facts_survive_disk_changes_and_foreign_queries_reject` | Unicode/CRLF/escaped-scalar evidence retains original byte positions after live source/prelude changes. Repeated statistics agree. Foreign modules/body identities and invalid indices reject. No continuous freshness claim follows. |
| Imported source and canonical roles | `imported_startup_is_checked_in_both_modes_and_local_copy_names_gain_no_core_role` | Imported startup literals are checked in both modes without execution. Qualified calls retain actual alias routes and defining identities. A local function named copy gains no selected prelude role or ownership authority. |
| Missing operand contract | `never_alone_does_not_choose_a_primitive_operand_contract` | Two never operands cannot select a comparison's otherwise undetermined primitive input contract. An annotation is required, not a default choice. |
| Selected negation contract | `never_admission_cannot_select_an_unsigned_unary_minus_contract` | All five unsigned contexts reject negation of a never operand. All five signed widths and both floating widths retain their declared result types without normal continuation. Never admission cannot extend the closed operator matrix. |

These are compiler-analysis assertions. The suite does not execute Rill operations, compare runtime text/char values, observe arithmetic traps or run expectations. Expected literal bits and integer bounds are fixed contract cases, not an executable reference model.

## TDD and earlier-stage preservation

The first public tracer failed for the absent analysis::scalars module. The next failed for missing node/literal evidence before contextual return checking existed. Later tracers failed before binding targets, primitive context propagation, negation, explicit substitutions, static places, Boolean/exit facts, joins, startup/expectations and static templates existed. Focused test runs and Cargo typechecking ran during these vertical changes.

Regressions then exposed conditional divergence loss, unconsumed local continues, explicit-type-argument shadow bypass and false missing-name findings after unsupported binding scopes. Each failed before its local correction. The final never-only comparison tracer rejected no input contract until that missing-context check existed. An initial lexical-target test had an incorrect hand-counted byte offset, corrected against the unchanged source. An interpolation test spelling was corrected to M1's actual ${...} delimiter. Neither fixture correction changed parser/language behavior.

Further boundary tables strengthen the same public seams after each implemented feature. They are not claimed as separately red-first source implementations. No separate M4.3 model-first sequence is claimed or required by C4. The earlier eight-test M4.2 model retains its independent reference scope.

No M1/M2/M3 behavior or test changes were needed. M4.1 adds the child module export and Clone for immutable written-use/substitution records. M4.2 exposes its existing input receipt only to sibling analysis modules. These changes permit canonical type reuse and node-query annotation provenance; they add no type semantics, dependencies or caller-supplied proof bypass.

## Partial facts, budgets and remaining obligations

Inference slots never become valid Error/Unknown types. They bind only established canonical terms. Fresh reference construction waits for its referent/context, and source initializers cannot recursively bind themselves. Explicit substitution applies once per call rather than constructing cyclic inference terms. Nominal definitions never expand in expression checking. Bounded structural traversal retains the absolute 64-level ceiling.

Default limits remain 1,000,000 charged work units, 100,000 entries and depth 64. Accounting includes temporary stacks/queues, normalization digits, substitutions, source/type visits, retained nodes, annotation clones, routes and exit sets. Counters charge before growth without refunds. They do not guarantee exact heap bytes, wall-clock completion or host OOM recovery.

A failure exposes partial facts and incomplete secondary diagnostics, never a ScalarReceipt. ResourceLimit takes priority over Incomplete, then SourceRejected. Known source errors do not create an unsupported obligation merely because dependent binding types are blocked. Unsupported scope-changing syntax prevents dependent lookup from proving missing names. Named local declaration visibility remains unperformed rather than guessed from M3 inventory.

Remaining M4 work includes data/pattern/variant checking, ordinary collection/selection, omitted generic inference/defaults, callable creation/invocation and capture evidence, conversions, formatting/error/iterator/result-use/core protocols, nested-source integration and mutable temporary policy. Full target/capture layout and source-interface obligations remain tracked. This slice grants no physical storage, loan, move/copy, disposal, cleanup, concurrency, runtime, LLVM or editor authority.

## Verification and final review

Before independent final review, the following checks passed on macOS 27.0.1 AArch64 with mise 2026.11.4 and Rust/Cargo 1.98.0:

- `mise run test:compiler`, all 202 compiler tests, including all 27 M4.3 public-interface tests and the unchanged 175 earlier tests.
- `mise run test`, including the unchanged CLI unavailable-command assertions.
- `mise run check`, including LLVM-free formatting/Clippy, existing tool/fixture tests and documentation checks. The documentation checker passed 1652 local destinations with zero errors.
- `mise run build:scaffold`, the unchanged CLI scaffold.
- `mise exec -- cargo clippy --locked -p compiler --no-default-features --all-targets -- -D warnings` and incremental Cargo check commands.
- The explicit M4.2 Python model command, all eight reference tests. This remains separate model evidence, not M4.3 or Rill execution.
- `mise exec -- cargo tree --locked -p compiler --no-default-features`, with no compiler dependencies.
- `git diff --check` and the changed-path audit. No earlier-stage tests, prototypes, experiments, workflows, locks or dependency manifests changed.

User-global CMake/Ninja/Go lock warnings did not change repository locks.

## Independent final review

Both axes reviewed the eleven staged files against checkpoint e4750b8 before the implementation commit. Standards found no documented-standard violations or actionable heuristic findings. That reviewer did not rerun checks or independently verify TDD history.

Spec found one P2 defect in selected unary-minus contract validation. An unsigned context could select an unsigned contract when the actual operand was never. The reviewer built the LLVM-free library and ran five temporary public-interface probes. No repository files changed during that review. The reviewer did not rerun full checks or independently verify TDD history.

The local correction validates the selected unary-minus contract independently of the actual operand. The new public regression first failed for the reported case. It now rejects all five unsigned contexts and preserves all signed/floating contexts. All 28 focused tests passed after correction.

The original staged reviews exclude the later correction, regression and review record. No second delegated review is claimed. Checkpoint reviews remain separate from these implementation reviews.

After correction, the model command and all four required mise tasks passed again. The compiler suite passed all 203 tests, including 28 M4.3 tests and the unchanged 175 earlier tests. The separate M4.2 model passed eight tests. Post-correction documentation validation passed 1652 destinations with zero errors. Formatting, LLVM-free Clippy and diff checks passed. These follow-up results do not claim that either reviewer reviewed the correction.

No product command routing or workflow changed. Product, runtime, release, LLVM/native, hosted/other-host and client acceptance remain unrun for this slice. Existing unavailable product gates remain unchanged and nonzero. Historical M4.1 hosted evidence retains its source revision and failed unavailable runtime gates. Formal ASD-STE100 vocabulary compliance remains unverified.
