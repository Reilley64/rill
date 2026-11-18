//! Ordinary input analysis against captured M3 graphs, without core admission.
use compiler::analysis::ordinary::{Completion, DeclarationIssue, analyze_ordinary};
use compiler::analysis::{AnalysisLimits, Type, TypeTerm, analyze_declaration_types};
use compiler::loader::{Graph, Limits, Prelude, Request, Selector, load};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "rill-m4-5-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let f = Self(path);
        f.write("app/main.rill", source);
        f.write(
            "std/core.rill",
            "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
        );
        f
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    fn load(&self) -> Graph {
        load(Request {
            entry: self.0.join("app/main.rill"),
            standard_root: self.0.join("std"),
            prelude: Prelude {
                copy: Selector {
                    module: "core.rill".into(),
                    name: "copy".into(),
                },
                dispose: Selector {
                    module: "core.rill".into(),
                    name: "dispose".into(),
                },
            },
            target: "ordinary-test".into(),
            options: BTreeMap::new(),
            test_mode: false,
            limits: Limits::default(),
        })
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn captured_headers_share_a_rigid_substitution_without_claiming_admission() {
    let f = Fixture::new(
        "function f<T>(x: T): T;
         implement<U> f(x: U): U { return x; }",
    );
    let graph = f.load();
    let body = &graph.implementations()[0];
    let report = analyze_ordinary(&graph, AnalysisLimits::default());
    assert_eq!(report.completion(), Completion::Incomplete);
    let family = report.family(&body.family).unwrap().unwrap();
    assert_eq!(family.implementations.len(), 1);
    use compiler::analysis::ordinary::PendingAdmission;
    assert_eq!(
        family.pending_admission(),
        &[
            PendingAdmission::FamilyRole,
            PendingAdmission::RequiredWinners,
            PendingAdmission::ProhibitedInputs
        ]
    );
    let implementation = &family.implementations[0];
    assert_eq!(implementation.source.declaration, body.declaration);
    let substitution = implementation.substitution.as_deref().unwrap();
    assert_eq!(substitution.len(), 1);
    assert!(matches!(
        substitution[0].argument.as_ref(),
        TypeTerm::Parameter { declaration, position: 0 }
            if **declaration == implementation.header.binders[0].declaration.id
    ));
    assert!(analyze_declaration_types(&graph, AnalysisLimits::default()).is_err());
}

#[test]
fn defaults_and_requirements_preserve_metadata_without_proving_evidence() {
    let f = Fixture::new(
        "import api::{check as testable};
         function f<T,R = bool>(x: T): R where testable(T), noBorrow(T);
         implement<U> f(x: U): bool where testable(U), missing(U) { return true; }",
    );
    f.write("app/api.rill", "export function check<T>(x: T): bool;");
    let graph = f.load();
    let report = analyze_ordinary(&graph, AnalysisLimits::default());
    let family = report
        .family(&graph.implementations()[0].family)
        .unwrap()
        .unwrap();
    let metadata = report
        .header_metadata(&family.header.declaration.id)
        .unwrap()
        .unwrap();
    assert_eq!(metadata.defaults.len(), 1);
    assert_eq!(
        metadata.defaults[0].binder,
        &family.header.binders[1].declaration.id
    );
    assert_eq!(metadata.defaults[0].span.bytes(), b"bool");
    let requirement = &metadata.requirements[0];
    use compiler::analysis::ordinary::RequirementTarget;
    assert!(
        matches!(requirement.target, Some(RequirementTarget::Family(id)) if id.module.path == "api.rill")
    );
    let intrinsic = graph
        .prelude()
        .intrinsics
        .iter()
        .find(|id| id.name() == "noBorrow")
        .unwrap();
    assert!(
        matches!(metadata.requirements[1].target, Some(RequirementTarget::Predicate(id)) if std::ptr::eq(id, intrinsic))
    );
    assert_eq!(requirement.routes[0].name, "testable");
    assert!(matches!(
        requirement.arguments[0].as_deref(),
        Some(TypeTerm::Parameter { position: 0, .. })
    ));
    let body = report
        .header_metadata(&graph.implementations()[0].declaration)
        .unwrap()
        .unwrap();
    assert!(body.requirements[0].target.is_some());
    assert!(body.requirements[1].target.is_none());
    assert_eq!(report.completion(), Completion::Incomplete);
}

#[test]
fn uncalled_implementation_and_record_members_still_require_uniqueness() {
    let f = Fixture::new(
        "type Pair = { x: bool; x: bool };
         function f<T>(x: T, y: T): T;
         implement<U> f(x: U, x: U): U { return x; }",
    );
    let graph = f.load();
    let report = analyze_ordinary(&graph, AnalysisLimits::default());
    let duplicates: Vec<_> = report
        .declaration_issues()
        .iter()
        .filter_map(|issue| {
            if let DeclarationIssue::DuplicateMembers { scope, members } = issue {
                Some((scope.id.kind, members.len()))
            } else {
                None
            }
        })
        .collect();
    assert!(duplicates.contains(&(compiler::loader::DeclarationKind::Implementation, 2)));
    assert!(duplicates.contains(&(compiler::loader::DeclarationKind::Record, 2)));
}

#[test]
fn canonical_keys_ignore_aliases_and_result_only_binders_but_preserve_correlation() {
    let f = Fixture::new(
        "import types::{Box as P, Box as Q};
         function f<A,B,C>(x: A, y: B): C;
         implement<T,R> f(x: P<T>, y: T): R { return x; }
         implement<U> f(x: Q<U>, y: U): bool { return true; }
         implement<V,W> f(x: P<V>, y: W): bool { return true; }",
    );
    f.write("app/types.rill", "export type Box<T> = { value: T };");
    let graph = f.load();
    let report = analyze_ordinary(&graph, AnalysisLimits::default());
    let family = report
        .family(&graph.implementations()[0].family)
        .unwrap()
        .unwrap();
    let bodies = &family.implementations;
    assert_eq!(bodies.len(), 3);
    assert_eq!(bodies[0].input_key, bodies[1].input_key);
    assert_ne!(bodies[0].input_key, bodies[2].input_key);
    assert_eq!(family.duplicate_keys.len(), 1);
    assert_eq!(family.duplicate_keys[0].len(), 2);
}

#[test]
fn fixed_parent_and_repeated_wildcards_overlap_without_dominance() {
    let f = Fixture::new(
        "enum E { Left, Right }
         function f<A,B>(x: A, y: B): bool;
         implement f(x: E, y: E): bool { return true; }
         implement<T> f(x: T, y: T): bool { return true; }
         implement<A,B> f(x: A, y: B): bool { return true; }",
    );
    let graph = f.load();
    let report = analyze_ordinary(&graph, AnalysisLimits::default());
    let family = report
        .family(&graph.implementations()[0].family)
        .unwrap()
        .unwrap();
    assert_eq!(family.implementations[0].alternatives.len(), 9);
    let relation = &family.comparisons[0];
    assert_eq!(relation.left, &graph.implementations()[0].declaration);
    assert_eq!(relation.right, &graph.implementations()[1].declaration);
    assert!(!relation.left_in_right);
    assert!(!relation.right_in_left);
    assert!(relation.intersection.is_some());
    assert!(family.comparisons[2].left_in_right);
    assert!(!family.comparisons[2].right_in_left);
}

fn primitive<'g>(graph: &'g Graph, name: &str) -> Type<'g> {
    let id = graph
        .prelude()
        .intrinsics
        .iter()
        .find(|id| id.name() == name)
        .unwrap();
    std::sync::Arc::new(TypeTerm::Primitive(id))
}

fn named_type<'g>(graph: &'g Graph, name: &str) -> Type<'g> {
    let id = &graph
        .modules()
        .iter()
        .flat_map(|m| &m.declarations)
        .find(|d| d.name.as_deref() == Some(name))
        .unwrap()
        .id;
    std::sync::Arc::new(TypeTerm::Nominal {
        declaration: id,
        arguments: Vec::new(),
    })
}

fn variant_type<'g>(graph: &'g Graph, name: &str) -> Type<'g> {
    let id = &graph
        .modules()
        .iter()
        .flat_map(|m| &m.declarations)
        .find(|d| d.name.as_deref() == Some(name))
        .unwrap()
        .id;
    std::sync::Arc::new(TypeTerm::Variant {
        parent: named_type(graph, "E"),
        declaration: id,
    })
}

#[test]
fn established_inputs_report_only_hypothetical_selection_without_role_admission() {
    use compiler::analysis::ordinary::{
        HypotheticalOutcome, InputContract::Established, select_inputs,
    };
    let f = Fixture::new(
        "enum E { Left, Right }
         function f<A,B>(x: A, y: B): bool;
         implement f(x: E, y: E): bool { return true; }
         implement<T> f(x: T, y: T): bool { return true; }",
    );
    let graph = f.load();
    let [first, second] = graph.implementations() else {
        panic!("expected two captured bodies")
    };
    let limits = AnalysisLimits::default();
    let report = analyze_ordinary(&graph, limits);
    let left = variant_type(&graph, "Left");
    let right = variant_type(&graph, "Right");
    let mixed = select_inputs(
        &report,
        &first.family,
        &[Established(left.clone()), Established(right)],
        &[],
        limits,
    )
    .unwrap();
    assert_eq!(mixed.completion, Completion::Incomplete);
    use compiler::analysis::ordinary::InputPremise;
    assert_eq!(mixed.premise, InputPremise::HypotheticalOrdinaryRole);
    assert_eq!(
        mixed.outcome,
        Some(HypotheticalOutcome::Winner(&first.declaration))
    );
    let equal = select_inputs(
        &report,
        &first.family,
        &[Established(left.clone()), Established(left)],
        &[],
        limits,
    )
    .unwrap();
    assert_eq!(equal.outcome, Some(HypotheticalOutcome::Ambiguous));
    assert_eq!(
        equal
            .matches
            .iter()
            .map(|m| m.implementation)
            .collect::<Vec<_>>(),
        [&first.declaration, &second.declaration]
    );
}

#[test]
fn explicit_input_constraints_filter_without_rewriting_keys_or_using_output_constraints() {
    use compiler::analysis::ordinary::{
        HypotheticalOutcome, InputContract::Established, select_inputs,
    };
    let f = Fixture::new(
        "enum E { Left, Right }
         function f<T,R>(x: T): R;
         implement<U> f(x: U): i32 { return 0i32; }
         implement f(x: E): bool { return true; }",
    );
    let graph = f.load();
    let [generic, fixed] = graph.implementations() else {
        panic!("expected two captured bodies")
    };
    let limits = AnalysisLimits::default();
    let report = analyze_ordinary(&graph, limits);
    let e = named_type(&graph, "E");
    let i32 = primitive(&graph, "i32");
    let run = |input, explicit: Vec<_>| {
        select_inputs(
            &report,
            &generic.family,
            &[Established(input)],
            &explicit,
            limits,
        )
        .unwrap()
    };
    let result = run(e.clone(), vec![e.clone(), i32.clone()]);
    assert_eq!(
        result.outcome,
        Some(HypotheticalOutcome::Winner(&fixed.declaration))
    );
    assert_eq!(result.matches.len(), 2);
    assert_eq!(result.pending_output_arguments, [1]);
    let result = run(variant_type(&graph, "Left"), vec![e.clone()]);
    assert_eq!(
        result.outcome,
        Some(HypotheticalOutcome::Winner(&fixed.declaration))
    );
    assert_eq!(result.matches.len(), 1);
    assert_eq!(
        run(e, vec![i32]).outcome,
        Some(HypotheticalOutcome::NoMatch)
    );
}

#[test]
fn placement_uses_defining_ownership_inside_inputs_not_results_or_imports() {
    use compiler::analysis::ordinary::Placement;
    let f = Fixture::new(
        "import api::{f, g, h, Remote as Alias};
         type Local = {};
         implement f(x: (&[Local]) => bool): bool { return true; }
         implement g(x: Alias): Local { return Local {}; }
         implement<T> h(x: T): bool { return true; }",
    );
    f.write("app/api.rill", "export type Remote = {}; export function f<T>(x: T): bool; export function g<T,R>(x: T): R; export function h<T>(x: T): bool;");
    let graph = f.load();
    let report = analyze_ordinary(&graph, AnalysisLimits::default());
    let captures: Vec<_> = graph
        .implementations()
        .iter()
        .map(|body| report.family(&body.family).unwrap().unwrap())
        .collect();
    assert!(
        matches!(captures[0].implementations[0].placement, Placement::ParameterOwner(owner) if owner.module.path == "main.rill")
    );
    assert_eq!(
        captures[1].implementations[0].placement,
        Placement::OutsideOwners
    );
    assert_eq!(
        captures[2].implementations[0].placement,
        Placement::OutsideOwners
    );
    assert!(
        captures[0]
            .routes
            .iter()
            .any(|binding| binding.module.path == "main.rill" && binding.name == "f")
    );
}

#[test]
fn signature_equations_never_repair_conflicts_with_defaults_or_admission() {
    use compiler::analysis::ordinary::Compatibility;
    let f = Fixture::new(
        "enum E { Left }
         function repeat<T = bool>(x: T): T;
         implement repeat(x: E.Left): E { return E.Left; }
         function explicit<T>(x: &T): T;
         implement<U> explicit<U>(x: &U): i32 { return 0i32; }
         function unused<T = bool>(): bool;
         implement unused(): bool { return true; }",
    );
    let graph = f.load();
    let report = analyze_ordinary(&graph, AnalysisLimits::default());
    let statuses: Vec<_> = graph
        .implementations()
        .iter()
        .map(|record| {
            let item = &report
                .family(&record.family)
                .unwrap()
                .unwrap()
                .implementations[0];
            assert!(item.substitution.is_none());
            item.compatibility
        })
        .collect();
    assert_eq!(
        statuses,
        [
            Compatibility::Incompatible,
            Compatibility::Incompatible,
            Compatibility::Incomplete
        ]
    );
}

#[test]
fn coupled_equations_can_prove_disjointness_after_an_occurs_check() {
    let f = Fixture::new(
        "type Box<T> = {};
         function f<X,Y>(x: X, y: Y): bool;
         implement<T> f(x: T, y: Box<T>): bool { return true; }
         implement<U> f(x: Box<U>, y: U): bool { return false; }",
    );
    let graph = f.load();
    let report = analyze_ordinary(&graph, AnalysisLimits::default());
    let relation = &report
        .family(&graph.implementations()[0].family)
        .unwrap()
        .unwrap()
        .comparisons[0];
    assert_eq!(relation.intersection, None);
    assert!(!relation.left_in_right && !relation.right_in_left);
}

#[test]
fn owned_and_shared_outer_admissions_do_not_apply_inside_mutable_or_tuple_shapes() {
    let f = Fixture::new(
        "enum E { Left, Right }
         function f<A,B,C,D>(a: A, b: B, c: C, d: D): bool;
         implement f(a: &E, b: &mut E, c: [E], d: once(E) => E): bool { return true; }",
    );
    let graph = f.load();
    let report = analyze_ordinary(&graph, AnalysisLimits::default());
    let family = report
        .family(&graph.implementations()[0].family)
        .unwrap()
        .unwrap();
    // Three shared-reference choices times three outer callable modes.
    assert_eq!(family.implementations[0].alternatives.len(), 9);
}

#[test]
fn malformed_types_reject_but_missing_shapes_and_bare_never_do_not_prove_absence() {
    use compiler::analysis::ordinary::{
        HypotheticalOutcome, InputContract, SelectionBlocker, select_inputs,
    };
    let f = Fixture::new(
        "type Box<T> = {};
         enum E { Left }
         enum Other { Right }
         function local(): void { type Scoped = {}; }
         function f<T>(x: T): bool;",
    );
    let graph = f.load();
    let foreign = f.load();
    let family = &graph
        .modules()
        .iter()
        .flat_map(|m| &m.declarations)
        .find(|d| d.name.as_deref() == Some("f"))
        .unwrap()
        .id;
    let limits = AnalysisLimits::default();
    let report = analyze_ordinary(&graph, limits);
    let run = |input| select_inputs(&report, family, &[input], &[], limits);
    assert!(run(InputContract::Established(named_type(&graph, "Box"))).is_err());
    assert!(run(InputContract::Established(variant_type(&graph, "Right"))).is_err());
    assert!(run(InputContract::Established(primitive(&foreign, "bool"))).is_err());
    assert!(select_inputs(&report, family, &[], &[], limits).is_err());
    for input in [InputContract::Unresolved, InputContract::BareNever] {
        let result = run(input).unwrap();
        assert_eq!(result.completion, Completion::Incomplete);
        assert_eq!(result.blocker, Some(SelectionBlocker::InputShape));
        assert_eq!(result.outcome, None);
    }
    let scoped = run(InputContract::Established(named_type(&graph, "Scoped"))).unwrap();
    assert_eq!(scoped.blocker, Some(SelectionBlocker::InputShape));
    assert_eq!(scoped.outcome, None);
    let result = run(InputContract::Established(primitive(&graph, "never"))).unwrap();
    assert_eq!(result.outcome, Some(HypotheticalOutcome::NoMatch));
    assert_eq!(result.completion, Completion::Incomplete);
}

#[test]
fn exhausted_products_keep_known_headers_but_cannot_supply_negative_input_evidence() {
    use compiler::analysis::ordinary::{InputContract::Established, select_inputs};
    let f = Fixture::new(
        "enum E { Left, Right }
         function f<A,B,C,D>(a: A, b: B, c: C, d: D): bool;
         implement f(a: E, b: E, c: E, d: E): bool { return true; }",
    );
    let graph = f.load();
    let family_id = &graph.implementations()[0].family;
    let limits = AnalysisLimits::default();
    let report = analyze_ordinary(&graph, limits);
    let exact = AnalysisLimits {
        max_work: report.statistics().work,
        max_entries: report.statistics().entries,
        max_depth: report.statistics().max_depth,
    };
    assert_eq!(
        analyze_ordinary(&graph, exact).completion(),
        Completion::Incomplete
    );
    for short in [
        AnalysisLimits {
            max_work: exact.max_work - 1,
            ..exact
        },
        AnalysisLimits {
            max_entries: exact.max_entries - 1,
            ..exact
        },
        AnalysisLimits {
            max_depth: exact.max_depth - 1,
            ..exact
        },
    ] {
        assert_eq!(
            analyze_ordinary(&graph, short).completion(),
            Completion::ResourceLimit
        );
    }
    let partial = analyze_ordinary(
        &graph,
        AnalysisLimits {
            max_entries: exact.max_entries - 200,
            ..exact
        },
    );
    assert_eq!(partial.completion(), Completion::ResourceLimit);
    let body = &partial.family(family_id).unwrap().unwrap().implementations[0];
    assert_eq!(
        body.source.declaration,
        graph.implementations()[0].declaration
    );
    let e = named_type(&graph, "E");
    let selected =
        select_inputs(&partial, family_id, &vec![Established(e); 4], &[], limits).unwrap();
    assert_eq!(selected.completion, Completion::ResourceLimit);
    assert_eq!(selected.outcome, None);
}

#[test]
fn predicates_cannot_be_supplied_as_types_even_inside_explicit_arguments() {
    use compiler::analysis::ordinary::{InputContract::Established, select_inputs};
    let f = Fixture::new(
        "function f<T>(x: T): bool;
         implement<U> f(x: U): bool { return true; }",
    );
    let graph = f.load();
    let id = &graph.implementations()[0].family;
    let limits = AnalysisLimits::default();
    let analysis = analyze_ordinary(&graph, limits);
    let predicate = primitive(&graph, "noBorrow");
    assert!(
        select_inputs(
            &analysis,
            id,
            &[Established(predicate.clone())],
            &[],
            limits
        )
        .is_err()
    );
    let malformed = std::sync::Arc::new(TypeTerm::Tuple(vec![predicate]));
    let valid = &[Established(primitive(&graph, "bool"))];
    assert!(select_inputs(&analysis, id, valid, &[malformed], limits).is_err());
}

#[test]
fn incomplete_types_do_not_hide_independent_member_uniqueness_findings() {
    let f = Fixture::new(
        "type Pair = { x: Missing; x: bool };
         enum E { V { y: Missing; y: bool }, V }
         function f<T>(x: T, y: T): T;
         implement<U> f(x: Missing, x: U): U { return x; }",
    );
    let graph = f.load();
    let report = analyze_ordinary(&graph, AnalysisLimits::default());
    let duplicates: Vec<_> = report
        .declaration_issues()
        .iter()
        .filter_map(|issue| {
            if let DeclarationIssue::DuplicateMembers { scope, .. } = issue {
                Some(scope.id.kind)
            } else {
                None
            }
        })
        .collect();
    use compiler::loader::DeclarationKind::{Enum, Implementation, Record, Variant};
    assert_eq!(duplicates.len(), 4, "{duplicates:?}");
    for kind in [Enum, Implementation, Record, Variant] {
        assert!(duplicates.contains(&kind), "{duplicates:?}");
    }
}

#[test]
fn moving_a_family_header_never_erases_it_at_an_entry_limit() {
    let f = Fixture::new("function f<T>(x: T): T;");
    let graph = f.load();
    let id = &graph
        .modules()
        .iter()
        .flat_map(|m| &m.declarations)
        .find(|d| d.name.as_deref() == Some("f"))
        .unwrap()
        .id;
    let limits = AnalysisLimits::default();
    let full = analyze_ordinary(&graph, limits);
    let mut previously_known = false;
    for max_entries in 0..=full.statistics().entries {
        let partial = analyze_ordinary(
            &graph,
            AnalysisLimits {
                max_entries,
                ..limits
            },
        );
        let known = partial.family(id).unwrap().is_some()
            || partial
                .type_facts()
                .declarations()
                .any(|fact| fact.declaration.id == *id);
        assert!(
            !previously_known || known,
            "lost the family header at {max_entries} entries"
        );
        previously_known = known;
    }
    assert!(previously_known);
}

#[test]
fn wide_admission_products_charge_copies_to_work_with_entries_to_spare() {
    let width = 96;
    let tuple = vec!["bool"; width].join(",");
    let f = Fixture::new(&format!(
        "enum E {{ Left, Right }}
         function f<A,B,C,D,U>(a: A, b: B, c: C, d: D, tuple: U): bool;
         implement f(a: E, b: E, c: E, d: E, tuple: [{tuple}]): bool {{ return true; }}"
    ));
    let graph = f.load();
    let family_id = &graph.implementations()[0].family;
    let limits = AnalysisLimits::default();
    let full = analyze_ordinary(&graph, limits);
    let full_body = &full.family(family_id).unwrap().unwrap().implementations[0];
    assert_eq!(full_body.alternatives.len(), 81);

    // Even without accounting for fixed bookkeeping, each alternative copies
    // all tuple elements. Entries alone must not pay for that work.
    let work = 81 * width - 1;
    let partial = analyze_ordinary(
        &graph,
        AnalysisLimits {
            max_work: work,
            ..limits
        },
    );
    assert_eq!(partial.completion(), Completion::ResourceLimit);
    assert!(partial.statistics().entries < limits.max_entries);
    let partial_body = &partial.family(family_id).unwrap().unwrap().implementations[0];
    assert!(partial_body.alternatives.is_empty());
}
