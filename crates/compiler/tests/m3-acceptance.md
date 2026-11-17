# M3 acceptance record

## Scope and authority

[M3 ticket #54](https://github.com/Reilley64/rill/issues/54) records the starting commit `04fd25ff9e8b4e02621802a111263947dbeb53b0`, after PR #53 merged. The owner [approved checkpoint a41e7cc](https://github.com/Reilley64/rill/issues/54#issuecomment-6939478619), including O1 through O7, all three public test seams and the four implementation slices.

The tests below exercise the real Rust loader/resolver in `m3.rs`, using temporary files and the public `load`, `lookup_export` and `validate_inputs` interfaces. They capture source through M1 and parse through M2. These are compiler-resolution assertions, not executions of Rill programs. A resolved graph is not a checked program. Full RFC statuses, scoped prior approvals and #28's readiness/formal/production obligations remain separate.

## RFC 0002 cases

Every test name in this table exists in [m3.rs](m3.rs). All listed tests passed on the recorded host. Assertions outside the approved M3 scope remain explicitly later-stage work.

| Case | Actual direct test | Observed M3 result | Still later-stage |
| --- | --- | --- | --- |
| M1 | `m1_facade_preserves_identity_and_owner` | Three application modules load once, plus the explicit standard prelude. Facade aliases retain nominal identity and defining-module ownership. | Expectation evaluation. |
| M2 | `m2_ungrouped_competition_grouped_export` | Ungrouped file/type competition yields R0402 with canonical candidates. Grouped selection uses exports without loading the competing child. | Typed use of the selected declaration. |
| M3 | `m3_import_roots_ignore_local_aliases` | A prior local alias cannot redefine import lookup. A separately existing A.rill supplies its own canonical declaration. | Expression/local-use resolution. |
| M4 | `m4_m5_metadata_retains_private_fields_and_written_generic_requirements` | Private-field flags, defining-module ownership, enum members and nested declarations remain in source records. | R0401 for external construction, rebuilding and field access. |
| M5 | `m4_m5_metadata_retains_private_fields_and_written_generic_requirements` | Written exported requirements and private family declaration metadata survive collection. | Typed public-requirement visibility and its R0401 rejection. |
| M6 | `m6_repeated_routes_collect_one_canonical_implementation_body` | Facade and direct routes contribute one implementation body record. A private family import is not re-exported by an extension. | Body typing and placement permission. |
| M7 | `m6_m7_implementation_records_keep_canonical_family_and_distinct_bodies` | Distinct bodies through P/Q aliases retain the same family identity and separate body/signature records, including differing written returns. | R0304 canonical typed-pattern rejection, binder normalization, signature validity and coherence. The resolved result does not accept those bodies as a checked program. |
| M8 | `m8_anchored_cycles_retain_one_identity_and_multiple_anchors_collide`; `m8_unanchored_cycles_and_dependent_aliases_are_cyclic_not_missing` | Declaration/module anchors resolve. Multiple anchors remain ambiguous. Empty alias cycles and their acyclic dependents retain R0405, not fabricated R0400. | R0406 recursive layouts. |
| M9 | `entry_link_cannot_choose_another_application_root`; `m9_escaping_file_directory_and_standard_links_reject` | Entry, application file/directory and standard links escaping their origins yield R0404. | Filesystem sandbox or concurrent-writer security proof. |
| M10 | `m10_relocated_roots_and_import_order_keep_canonical_inventories`; `canonical_order_uses_unsigned_utf8_paths_not_discovery_or_locale` | Relocation and import order do not change canonical module/declaration inventory order. ASCII, Greek and nested paths follow unsigned UTF-8 order, with app before std. | Execution order and unrun host profiles. |
| M11 | `m11_expectation_only_dependency_stays_loaded_in_both_modes` | Ordinary/test requests retain expectation-only modules, implementation records and once-per-module expectations. | Checking, dispatch, DCE and expectation execution. |
| M12 | `m12_module_and_function_both_install_and_m15_private_half_stays_private` | A file-module and exported function coexist under one alias in different namespaces. | Calls and typed qualification. |
| M13 | `m13_collision_and_failed_group_publish_no_imports`; `module_startup_binders_and_group_aliases_participate_in_collisions` | One-sided collisions, startup binders and grouped aliases yield R0403. Failed attempts expose only provisional candidates, never installed imports or a Graph. Same-target aliases remain idempotent. | Typed local-use/shadowing checks. |
| M14 | `m14_standard_unprefixed_has_no_application_fallback_and_roots_are_explicit`; `repeated_super_paths_stay_within_each_origin` | Explicit std/facade, self and repeated super use the selected origin. Standard lookup never falls back into app. Escaping relative paths reject. | Installed-toolchain selection and compatibility. |
| M15 | `m12_module_and_function_both_install_and_m15_private_half_stays_private`; `private_intermediate_modules_are_not_missing_or_public_routes` | Both public/private namespace arrangements import only their public half. Private intermediate barriers yield R0401 and do not invalidate a unique public alternative. | Opaque inferred values and typed representation access. |

## Other required behavior

| Requirement | Actual tests and observations |
| --- | --- |
| M1/M2 reuse | `entry_and_explicit_prelude_reuse_m1_m2_snapshots` checks the actual M1 snapshot ID inside M2 syntax. `m1_m2_failures_propagate_without_successful_prefixes` checks invalid UTF-8 byte extents, unused loaded syntax errors and parser resource failure. |
| Canonical identity | `internal_links_share_identity_hard_links_do_not_and_relative_paths_are_canonical` checks actual symbolic and hard links. Internal aliases share a key; distinct hard-link paths retain distinct nominal declarations. Self/super use canonical siblings. `overlapping_roots_keep_application_and_standard_identities_separate` keeps two origins even for one physical source. |
| Competing routes and O1 | `file_export_facade_routes_deduplicate_by_canonical_identity` checks canonical reduction across file/export routes and distinct competing facades. `no_child_autoload_or_unrelated_scan_but_candidate_facades_join_graph` retains parsed candidate facades and excludes unrelated malformed files and filesystem children. |
| Explicit prelude | `explicit_prelude_requires_public_bodyless_families` rejects missing/private/standalone selectors and escaping selector paths. `prelude_intrinsic_identities_and_canonical_operations_survive_shadowing` checks fixed primitive/predicate records and canonical copy/dispose identity under local shadowing. Typed signatures and runtime trust remain unchecked. |
| Family targets | `m6_m7_implementation_records_keep_canonical_family_and_distinct_bodies` rejects standalone function targets. `private_qualified_implementation_targets_are_not_missing` retains R0401 without a false R0400. |
| Capture versions | `source_changes_and_new_candidates_keep_old_snapshots` checks changed bytes, new negative-probe competitors, immutable earlier spans and distinct capture/declaration IDs. `changed_links_roots_and_removed_dependencies_invalidate_capture` checks link retargeting, new escapes, changed selected roots and removed dependencies. Restored observations can compare unchanged; no transaction or continuous-freshness claim follows. |
| Host input | `host_failures_are_not_missing_candidates` exercises missing entry/root, broken links, link loops and non-regular Unix socket sources. `unreadable_competing_facade_prevents_a_unique_result` exercises a genuinely denied read on this host. `missing_child_after_valid_directory_link_is_a_missing_import` separates an absent child from a broken target. |
| Exact spelling | `exact_case_and_unicode_spelling_never_silently_match` accepts exact stored spellings and rejects alternate case/normalization lookups instead of merging them. This host reports explicit InputFailure for those alternate spellings. `link_target_spelling_limitations_are_not_mislabeled_as_broken_links` also checks that an OS-resolvable spelling mismatch in a link target reports the exact-spelling limitation, not a nonexistent broken link. |
| Resource outcomes | `finite_limits_are_incomplete_not_source_errors_and_boundaries_are_inclusive` exercises module, source/total byte, work, probe, directory-entry, parser and replay budgets. Exact source/module boundaries succeed; one-byte-short storage fails as ResourceLimit. `parser_work_is_reported_and_constrains_the_shared_loader_budget` covers the additive M2 work statistic. `acyclic_link_work_ceiling_is_resource_incomplete_not_a_link_loop` covers the implementation's 64-link ceiling. |
| Determinism and termination | `long_alias_graphs_terminate_deterministically_and_respect_limits` checks an 80-module alias chain, repeat captures, an unanchored cycle and a low-work failure. `findings_follow_origin_order_not_absolute_root_order` orders app findings before std despite oppositely sorted absolute roots. |

## Observed commands and limits

The validation host is macOS 27.0.1 AArch64, build 26A434. mise 2026.11.4 selects Rust/Cargo 1.98.0, Python 3.13.18 and uv 0.13.8. The final command results and implementation revision are also recorded in #54 and the draft PR.

- `mise exec -- cargo test --locked -p compiler --no-default-features` passed 138 tests, comprising two unit, 23 M1, 19 M2, 47 syntax cases and 47 M3 tests.
- `mise run check` passed formatting, LLVM-free Clippy/workspace tests, documentation and existing scaffold/fixture checks. These fixture/tool results retain their own evidence scope, not Rill execution.
- `mise run build:scaffold` passed. It builds the unavailable-product CLI scaffold, not a working Rill toolchain.
- Product `build`, `test:language`, `test:stdlib`, `test:runtime`, `test:lsp-client` and `verify:release` each exited 2 as unavailable. No successful empty product gate was introduced.
- `mise exec -- cargo tree --locked -p compiler --no-default-features` showed no dependencies. No Cargo, mise or uv lock changed. User-level CMake/Ninja/Go lock warnings remained unrelated to the repository's selected base tools.

The only earlier-stage source extension is `Syntax::work`, exposing M2's existing charged-operation counter. The loader limits each parse by its remaining shared work allowance and charges the returned count. No grammar, parser commitment, recovery or source-span behavior changes. The M3 public loader regression checks this statistic, and the complete existing M1/M2 suite remains intact.

The exercised filesystem cannot establish distinct alternate-case or normalization-equivalent files for the requested spellings. The negative tests disclose that host limitation and reject rather than merging identities. Positive distinct-pair behavior on case-sensitive/normalization-sensitive filesystems remains unrun. Linux and macOS x86-64 were unrun during local validation. The hosted publication results below supersede that limit for their named scaffold profiles. Windows-native loading remains unsupported.

Permission denial, symbolic links and hard links were available on this host. The permission test reports a limitation if another host's privileges bypass denial. Arbitrary syscall failure injection, hostile concurrent change/restore schedules, host OOM recovery and wall-clock limits were not established. No standard library was shipped and no Rill user code, LLVM backend or LSP client ran.

## Hosted publication evidence

At implementation revision `862704bea6af46e1446b3b7942e690f877b116c1`, the [pull-request scaffold run](https://github.com/Reilley64/rill/actions/runs/44033556098) passed every configured runner profile. Its logs show all 47 M3 tests passing on ubuntu-24.04, ubuntu-24.04-arm, macos-15 and macos-15-intel. The [push scaffold run](https://github.com/Reilley64/rill/actions/runs/44033436859) also passed. These runs include check, build:scaffold and check:ci. The unchanged workflow pins mise 2026.11.8; the local run above used 2026.11.4.

The [native-acceptance run](https://github.com/Reilley64/rill/actions/runs/44033556021) failed on all four profiles at the unchanged unavailable test:runtime gate, each with exit 2. Its logs were inspected. These failures do not invalidate the scoped resolver results, but they remain failed native/runtime acceptance. No backend, runtime, standard-library execution or client support follows from green scaffold jobs.

Distinct alternate-case/normalization file-pair validation remains unrun. The four hosted profiles establish their exercised Rust/filesystem tests, not arbitrary filesystems or physical-CPU/backend guarantees. These observations identify the tested source revision rather than claiming future documentation-only revisions have already completed CI.

## Final review corrections

The final review compared the branch with starting commit `04fd25ff9e8b4e02621802a111263947dbeb53b0`. Both independent axes reviewed implementation commit `95b5e2c`. Corrections were made locally after review, without a second delegated review.

### Standards

The review found one resource-accounting violation and one naming concern. Grouped resolution now charges predecessor/witness copies before cloning. Alias-component collection checks its size before allocation. Shared MISSING, PRIVATE and CYCLIC cause constants replace cross-file numeric bits. `grouped_selection_after_long_qualification_has_bounded_resolution_copies` exercises a long qualified route with 160 selected exports and an explicit low-work outcome. This public regression and code inspection do not instrument allocator internals.

### Spec

The review found four behavior/provenance gaps, all corrected within M3:

- `link_target_parent_steps_follow_native_resolution_not_lexical_cleanup` first failed because lexical cleanup selected the wrong source. It now matches the actual filesystem target. Parent steps occur after preceding symbolic links resolve, with confined target-boundary checks.
- `missing_explicit_standard_facade_is_a_missing_import_not_host_failure` first returned InputFailure. It now reports source-level R0400, while missing selected roots, entry files and explicitly required prelude source files remain input failures.
- `for_and_match_pattern_binders_have_distinct_containing_scope_records` first omitted loop/arm binders. It now records loop, arm, guard-success and guard-error binders without claiming pattern typing or local-use resolution.
- `private_route_witness_retains_binding_owner_target_and_blocked_position` now checks diagnostic-only private binding spans, modules, canonical targets, namespaces and blocked positions. Private witnesses never enter public candidate sets. `privacy_witnesses_use_actual_namespace_halves_after_closure` and `public_group_base_does_not_inherit_private_counterpart_failures` cover private/public namespace interactions. The latter first exposed a false R0401 from an irrelevant private counterpart and now retains only R0400.

The original reviewed revision passed 39 M3 tests. The corrected revision passes the 47 M3 tests and the unchanged 91 M1/M2/unit tests reported above. These are resolution/compiler tests, not typed or runtime acceptance.

Workflows did not change, so check:ci was not run locally. The hosted scaffold workflow did run it successfully at the source revision identified above. Product build, language/stdlib/runtime/client acceptance and release gates remain unavailable and nonzero. Historical prototypes and recorded experiment evidence remain unchanged. This record clears none of those later-stage or support obligations.
