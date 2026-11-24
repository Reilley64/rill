//! Conditional conversion analysis over real M3 captures. Fixtures grant no core admission.
#[path = "support/limit.rs"]
mod limit;

use compiler::analysis::conversions::{RegistrationIssueKind, RoleHypothesis, analyze_conversions};
use compiler::analysis::ordinary::{Completion, analyze_ordinary};
use compiler::analysis::{AnalysisLimits, QueryError};
use compiler::loader::{Binding, Graph, Limits, Origin, Prelude, Request, Selector, Target, load};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Self {
        let f = Self(std::env::temp_dir().join(format!(
            "rill-m4-7-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&f.0).unwrap();
        f.write("app/main.rill", source);
        f.write(
            "std/core.rill",
            "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
        );
        f.write(
            "std/convert.rill",
            "export function into<To, From>(value: From): To;",
        );
        f.write("std/aliases.rill", "export import convert::{into as from};");
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
            target: "conversion-test".into(),
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

fn standard_export<'g>(graph: &'g Graph, name: &str) -> &'g Binding {
    graph
        .bindings()
        .iter()
        .find(|b| b.public && b.module.origin == Origin::Standard && b.name == name)
        .unwrap()
}

#[test]
fn collection_checks_duplicate_pairs_outer_owners_and_protection_without_calls() {
    let fixture = Fixture::new(
        "import std::convert::into;
         import std::aliases::from;
         import foreign::Box;
         type Local = {};
         implement into<Local>(value: bool): Local { return Local {}; }
         implement from<Local>(value: bool): Local { return Local {}; }
         implement into<i32>(value: &Box<Local>): i32 { return 0i32; }
         implement into<bool>(value: &Local): bool { return true; }
         implement into<Local>(value: Local): Local { return value; }
         implement into<u8>(value: u16): u8 { return 0u8; }",
    );
    fixture.write("app/foreign.rill", "export type Box<T> = { value: T };");
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let binding = standard_export(&graph, "into");
    let Target::Declaration(id) = &binding.target else {
        panic!("captured family")
    };
    let report =
        analyze_conversions(&ordinary, &[RoleHypothesis::Infallible(binding)], limits).unwrap();
    let family = report.family(id).unwrap().unwrap();
    assert_eq!(family.registrations.len(), 6);
    assert_eq!(
        family
            .issues
            .iter()
            .filter(|i| i.kind == RegistrationIssueKind::DuplicatePair)
            .count(),
        1
    );
    let unauthorized = family
        .issues
        .iter()
        .filter(|i| i.kind == RegistrationIssueKind::OutsideEndpointOwners)
        .collect::<Vec<_>>();
    assert_eq!(unauthorized.len(), 2);
    assert!(unauthorized.iter().any(|i| {
        i.span
            .bytes()
            .windows(b"&Box<Local>".len())
            .any(|s| s == b"&Box<Local>")
    }));
    assert_eq!(
        family
            .issues
            .iter()
            .filter(|i| i.kind == RegistrationIssueKind::ProtectedIdentity)
            .count(),
        1
    );
    assert_eq!(
        family
            .issues
            .iter()
            .filter(|i| i.kind == RegistrationIssueKind::ProtectedNumericPair)
            .count(),
        1
    );
    assert_eq!(report.completion(), Completion::Incomplete);
}

#[test]
fn fallible_registration_requires_an_error_determination_route() {
    use compiler::analysis::conversions::OutputDetermination;
    let fixture = Fixture::new(
        "import std::convert::tryInto;
         import std::result::Result;
         type Wrap<T> = { value: T };
         type NoRoute = {};
         type Pending = {};
         implement<T,E> tryInto<Wrap<T>>(text: string): Result<Wrap<T>,E>
             where tryInto(T,&string,E), noBorrow(E) { while (true) {} }
         implement<E> tryInto<NoRoute>(text: string): Result<NoRoute,E>
             where noBorrow(E) { while (true) {} }
         implement<E> tryInto<Pending>(text: string): Result<Pending,E>
             where unresolved(bool,string,E) { while (true) {} }",
    );
    fixture.write(
        "std/convert.rill",
        "import result::Result; export function tryInto<To,From,E>(value: From): Result<To,E>;",
    );
    fixture.write(
        "std/result.rill",
        "export enum Result<T,E> { Ok(T), Err(E) }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let binding = standard_export(&graph, "tryInto");
    let Target::Declaration(id) = &binding.target else {
        panic!("captured family")
    };
    let report = analyze_conversions(
        &ordinary,
        &[RoleHypothesis::Fallible {
            family: binding,
            result: standard_export(&graph, "Result"),
        }],
        limits,
    )
    .unwrap();
    let family = report.family(id).unwrap().unwrap();
    assert_eq!(family.registrations.len(), 3);
    assert_eq!(
        family.registrations[0].output,
        OutputDetermination::Determined
    );
    assert_eq!(
        family.registrations[0].routes[0].span.bytes(),
        b"tryInto(T,&string,E)"
    );
    assert_eq!(
        family.registrations[1].output,
        OutputDetermination::Underdetermined
    );
    assert_eq!(family.registrations[2].output, OutputDetermination::Pending);
    assert!(
        family
            .issues
            .iter()
            .any(|i| i.kind == RegistrationIssueKind::UnderdeterminedError
                && i.declaration == &family.registrations[1].source.declaration)
    );
}

#[test]
fn pair_queries_keep_destination_bound_variables_exact_and_filter_receivers() {
    use compiler::analysis::DeclarationShape;
    use compiler::analysis::conversions::{PairOutcome, PairRequest};
    let fixture = Fixture::new(
        "import std::convert::into;
         enum Input { Tag }
         type Wrap<T> = { value: T };
         type Observed = { goal: Wrap<Input>; source: Input.Tag; receiver: Input };
         implement<T> into<Wrap<T>>(value: T): Wrap<T> { return Wrap<T> { value: value }; }
         implement into<Wrap<Input>>(value: Input): Wrap<Input> { return Wrap<Input> { value: value }; }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let binding = standard_export(&graph, "into");
    let Target::Declaration(id) = &binding.target else {
        panic!("captured family")
    };
    let report =
        analyze_conversions(&ordinary, &[RoleHypothesis::Infallible(binding)], limits).unwrap();
    let observed = ordinary
        .type_facts()
        .declarations()
        .find(|d| d.declaration.name.as_deref() == Some("Observed"))
        .unwrap();
    let DeclarationShape::Record { fields } = &observed.shape else {
        panic!("record")
    };
    let mut request = PairRequest {
        destination: &fields[0].ty,
        source: &fields[1].ty,
        source_filter: None,
        expected_error: None,
    };
    let pair = report.query_pair(id, request, limits).unwrap();
    assert_eq!(pair.completion, Completion::Incomplete);
    let fixed = &graph.implementations()[1].declaration;
    assert_eq!(pair.outcome, Some(PairOutcome::Winner(fixed)));
    assert_eq!(pair.matches.len(), 1);
    assert_eq!(pair.matches[0].implementation, fixed);
    assert_ne!(
        pair.matches[0].alternative, 0,
        "record the admitted source alternative"
    );

    request.source_filter = Some(&fields[2].ty);
    assert_eq!(
        report.query_pair(id, request, limits).unwrap().outcome,
        Some(PairOutcome::Winner(fixed))
    );
    request.source_filter = Some(&fields[1].ty);
    assert_eq!(
        report.query_pair(id, request, limits).unwrap().outcome,
        Some(PairOutcome::NoMatch)
    );
    let low = report
        .query_pair(
            id,
            request,
            AnalysisLimits {
                max_work: 0,
                ..limits
            },
        )
        .unwrap();
    assert_eq!(low.completion, Completion::ResourceLimit);
    assert!(low.outcome.is_none());

    request.destination = &fields[2].ty;
    request.source = &fields[2].ty;
    assert_eq!(
        report.query_pair(id, request, limits).unwrap().outcome,
        Some(PairOutcome::RequiredIdentity {
            filter_conflict: true
        })
    );
}

#[test]
fn protected_numeric_gaps_differ_from_missing_providers() {
    use compiler::analysis::{
        TypeTerm,
        conversions::{PairBlocker, PairOutcome, PairRequest},
    };
    use std::sync::Arc;
    let fixture = Fixture::new("import std::convert::into;");
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let binding = standard_export(&graph, "into");
    let Target::Declaration(id) = &binding.target else {
        panic!("captured family")
    };
    let report =
        analyze_conversions(&ordinary, &[RoleHypothesis::Infallible(binding)], limits).unwrap();
    let primitive = |name| {
        Arc::new(TypeTerm::Primitive(
            graph
                .prelude()
                .intrinsics
                .iter()
                .find(|p| p.name() == name)
                .unwrap(),
        ))
    };
    let u8_ = primitive("u8");
    let u16_ = primitive("u16");
    let mut request = PairRequest {
        destination: &u8_,
        source: &u16_,
        source_filter: None,
        expected_error: None,
    };
    let gap = report.query_pair(id, request, limits).unwrap();
    assert_eq!(gap.outcome, Some(PairOutcome::NumericGap));
    assert!(!gap.blockers.contains(&PairBlocker::RequiredNumericProvider));
    request.destination = &u16_;
    request.source = &u8_;
    let unavailable = report.query_pair(id, request, limits).unwrap();
    assert!(unavailable.outcome.is_none());
    assert!(
        unavailable
            .blockers
            .contains(&PairBlocker::RequiredNumericProvider)
    );
    assert_eq!(unavailable.completion, Completion::Incomplete);
}

#[test]
fn a_disproved_specific_candidate_leaves_the_general_pair_applicable() {
    use compiler::analysis::DeclarationShape;
    use compiler::analysis::conversions::{
        Applicability, ErrorAgreement, PairOutcome, PairRequest,
    };
    let fixture = Fixture::new(
        "import std::convert::tryInto;
         import std::result::Result;
         type Target = {};
         type Wrap<T> = { value: T };
         type Observed = { goal: Target; actual: Wrap<&bool>; unrelated: bool };
         function unrelated<E>(value: E): void {}
         implement<T> tryInto<Target>(value: Wrap<T>): Result<Target,i32> { while (true) {} }
         implement tryInto<Target>(value: Wrap<&bool>): Result<Target,bool> where missing(bool), noBorrow(&bool) { while (true) {} }",
    );
    fixture.write(
        "std/convert.rill",
        "import result::Result; export function tryInto<To,From,E>(value: From): Result<To,E>;",
    );
    fixture.write(
        "std/result.rill",
        "export enum Result<T,E> { Ok(T), Err(E) }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let binding = standard_export(&graph, "tryInto");
    let Target::Declaration(id) = &binding.target else {
        panic!("captured family")
    };
    let roles = [RoleHypothesis::Fallible {
        family: binding,
        result: standard_export(&graph, "Result"),
    }];
    let report = analyze_conversions(&ordinary, &roles, limits).unwrap();
    let observed = ordinary
        .type_facts()
        .declarations()
        .find(|d| d.declaration.name.as_deref() == Some("Observed"))
        .unwrap();
    let DeclarationShape::Record { fields } = &observed.shape else {
        panic!("record")
    };
    let request = PairRequest {
        destination: &fields[0].ty,
        source: &fields[1].ty,
        source_filter: None,
        expected_error: Some(&fields[2].ty),
    };
    let pair = report.query_pair(id, request, limits).unwrap();
    assert_eq!(
        pair.outcome,
        Some(PairOutcome::Winner(&graph.implementations()[0].declaration))
    );
    assert_eq!(pair.matches[0].applicability, Applicability::Proved);
    assert_eq!(pair.matches[1].applicability, Applicability::Disproved);
    let signature = pair
        .selected_signature
        .as_ref()
        .expect("selected, fully substituted signature");
    assert_eq!(
        signature.site.span.bytes(),
        b"implement<T> tryInto<Target>(value: Wrap<T>): Result<Target,i32>"
    );
    assert_eq!(signature.parameter, fields[1].ty);
    let goal = pair.goal(&pair.matches[1].requirements[0]).unwrap();
    use compiler::analysis::generics::{GoalBlocker, GoalState, GoalVerdict};
    assert_eq!(
        goal.state,
        GoalState::Unresolved(GoalBlocker::ConversionAdmission)
    );
    assert_eq!(goal.hypothetical, Some(GoalVerdict::Disproved));
    assert_eq!(goal.source.span.bytes(), b"noBorrow(&bool)");
    let other = report.query_pair(id, request, limits).unwrap();
    assert_eq!(
        other.goal(&goal.id).unwrap_err(),
        QueryError::InvalidRequest
    );
    let ErrorAgreement::Disagrees { actual, required } = pair.error_agreement else {
        panic!("compare after normal selection")
    };
    assert!(
        matches!(actual.as_ref(), compiler::analysis::TypeTerm::Primitive(p) if p.name() == "i32")
    );
    assert_eq!(required, fields[2].ty);
    let rigid = ordinary
        .type_facts()
        .declarations()
        .find(|d| d.declaration.name.as_deref() == Some("unrelated"))
        .unwrap();
    let DeclarationShape::Function { parameters, .. } = &rigid.shape else {
        panic!("function")
    };
    let compare_rigid = report
        .query_pair(
            id,
            PairRequest {
                expected_error: Some(&parameters[0].ty),
                ..request
            },
            limits,
        )
        .unwrap();
    assert_eq!(compare_rigid.outcome, pair.outcome);
    assert!(matches!(
        compare_rigid.error_agreement,
        ErrorAgreement::Disagrees { .. }
    ));
    assert_eq!(pair.completion, Completion::Incomplete);
}

#[test]
fn only_a_potentially_maximal_pending_candidate_blocks_a_winner() {
    use compiler::analysis::DeclarationShape;
    use compiler::analysis::conversions::{PairBlocker, PairOutcome, PairRequest};
    let fixture = Fixture::new(
        "import std::convert::into;
         type Box<T> = { value: T };
         type Target1 = {};
         type Target2 = {};
         type Observed = { first: Target1; second: Target2; actual: Box<bool> };
         implement<T> into<Target1>(value: Box<T>): Target1 where missing(T) { return Target1 {}; }
         implement into<Target1>(value: Box<bool>): Target1 { return Target1 {}; }
         implement<T> into<Target2>(value: Box<T>): Target2 { return Target2 {}; }
         implement into<Target2>(value: Box<bool>): Target2 where missing(bool) { return Target2 {}; }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let binding = standard_export(&graph, "into");
    let Target::Declaration(id) = &binding.target else {
        panic!("captured family")
    };
    let report =
        analyze_conversions(&ordinary, &[RoleHypothesis::Infallible(binding)], limits).unwrap();
    let observed = ordinary
        .type_facts()
        .declarations()
        .find(|d| d.declaration.name.as_deref() == Some("Observed"))
        .unwrap();
    let DeclarationShape::Record { fields } = &observed.shape else {
        panic!("record")
    };
    let mut request = PairRequest {
        destination: &fields[0].ty,
        source: &fields[2].ty,
        source_filter: None,
        expected_error: None,
    };
    let first = report.query_pair(id, request, limits).unwrap();
    assert_eq!(
        first.outcome,
        Some(PairOutcome::Winner(&graph.implementations()[1].declaration))
    );
    request.destination = &fields[1].ty;
    let second = report.query_pair(id, request, limits).unwrap();
    assert!(second.outcome.is_none());
    assert!(second.blockers.contains(&PairBlocker::Applicability));
}

#[test]
fn a_selected_nested_pair_determines_error_and_a_cycle_cannot_guess_it() {
    use compiler::analysis::conversions::{
        Applicability, ErrorAgreement, PairOutcome, PairRequest,
    };
    use compiler::analysis::{DeclarationShape, TypeTerm};
    let fixture = Fixture::new(
        "import std::convert::tryInto;
         import std::result::Result;
         type Outer = {};
         type Inner = {};
         type Cycle = {};
         type Growth = {};
         type Box<T> = { value: T };
         type Observed = { outer: Outer; input: string; desired: bool; cycle: Cycle; growth: Growth; boxed: Box<bool> };
         implement tryInto<Inner>(value: bool): Result<Inner,i32> { while (true) {} }
         implement<E> tryInto<Outer>(value: string): Result<Outer,E>
             where tryInto(Inner,bool,E) { while (true) {} }
         implement<E> tryInto<Cycle>(value: string): Result<Cycle,E>
             where tryInto(Cycle,string,E) { while (true) {} }
         implement<T,E> tryInto<Growth>(value: Box<T>): Result<Growth,E>
             where tryInto(Growth,Box<Box<T>>,E) { while (true) {} }",
    );
    fixture.write(
        "std/convert.rill",
        "import result::Result; export function tryInto<To,From,E>(value: From): Result<To,E>;",
    );
    fixture.write(
        "std/result.rill",
        "export enum Result<T,E> { Ok(T), Err(E) }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let binding = standard_export(&graph, "tryInto");
    let Target::Declaration(id) = &binding.target else {
        panic!("captured family")
    };
    let report = analyze_conversions(
        &ordinary,
        &[RoleHypothesis::Fallible {
            family: binding,
            result: standard_export(&graph, "Result"),
        }],
        limits,
    )
    .unwrap();
    let observed = ordinary
        .type_facts()
        .declarations()
        .find(|d| d.declaration.name.as_deref() == Some("Observed"))
        .unwrap();
    let DeclarationShape::Record { fields } = &observed.shape else {
        panic!("record")
    };
    let mut request = PairRequest {
        destination: &fields[0].ty,
        source: &fields[1].ty,
        source_filter: None,
        expected_error: Some(&fields[2].ty),
    };
    let chosen = report.query_pair(id, request, limits).unwrap();
    assert_eq!(
        chosen.outcome,
        Some(PairOutcome::Winner(&graph.implementations()[1].declaration))
    );
    assert!(
        matches!(chosen.selected_error.as_deref(), Some(TypeTerm::Primitive(p)) if p.name() == "i32")
    );
    assert!(matches!(
        chosen.error_agreement,
        ErrorAgreement::Disagrees { .. }
    ));
    assert!(
        chosen
            .goals()
            .iter()
            .any(|g| g.source.span.bytes() == b"tryInto(Inner,bool,E)" && g.output.is_some())
    );
    request.destination = &fields[3].ty;
    let cycle = report.query_pair(id, request, limits).unwrap();
    assert!(cycle.outcome.is_none() && cycle.selected_error.is_none());
    assert_eq!(cycle.matches[0].applicability, Applicability::Unresolved);
    assert!(cycle.goals().iter().any(|g| !g.cycle.is_empty()));
    request.destination = &fields[4].ty;
    request.source = &fields[5].ty;
    let growing = report
        .query_pair(
            id,
            request,
            AnalysisLimits {
                max_depth: 9,
                ..limits
            },
        )
        .unwrap();
    assert_eq!(growing.completion, Completion::ResourceLimit);
    assert!(growing.outcome.is_none() && growing.selected_error.is_none());
    assert!(growing.goals().iter().all(|g| g.cycle.is_empty()));
}

fn node_with(module: &compiler::loader::Module, bytes: &[u8]) -> usize {
    module
        .syntax
        .nodes
        .iter()
        .position(|n| n.span.bytes() == bytes)
        .unwrap()
}

#[test]
fn source_uses_require_immediate_endpoints_and_keep_the_selected_descriptor() {
    use compiler::analysis::conversions::PairOutcome;
    let fixture = Fixture::new(
        "import std::convert::into;
         type Target = {};
         enum E { Tag }
         implement into<Target>(value: E): Target { return Target {}; }
         implement into<Target>(value: i32): Target { return false; }
         implement into<Target>(value: (bool) => bool): Target { return Target {}; }
         function callback(): Target { return into<Target,(bool) => bool>((x) => x); }
         function immediate(): Target { return into(E.Tag); }
         function missing(): void { let v = into(E.Tag); }
         function noWidth(): Target { return into<Target>(1); }
         function width(): Target { return into<Target,i32>(2); }
         function value(): void { let f: (E.Tag) => Target = into<Target>; }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let inventory = analyze_conversions(
        &ordinary,
        &[RoleHypothesis::Infallible(standard_export(&graph, "into"))],
        limits,
    )
    .unwrap();
    let report = inventory.bodies(limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let uses: Vec<_> = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(node, _)| report.conversion_use(module, node).unwrap())
        .collect();
    let chosen = uses
        .iter()
        .find(|u| u.site.span.bytes() == b"into(E.Tag)" && u.destination.is_some())
        .unwrap();
    assert_eq!(
        chosen.outcome,
        Some(PairOutcome::Winner(&graph.implementations()[0].declaration))
    );
    assert!(report.node(module, chosen.site.node).unwrap().is_none());
    assert!(uses.iter().any(|u| u.site.span.bytes() == b"into(E.Tag)"
        && u.destination.is_none()
        && u.goal.is_none()));
    assert!(
        uses.iter()
            .any(|u| u.site.span.bytes() == b"into<Target>(1)" && u.actual_source.is_none())
    );
    assert!(
        uses.iter()
            .any(|u| u.site.span.bytes() == b"into<Target,i32>(2)"
                && u.outcome == Some(PairOutcome::Winner(&graph.implementations()[1].declaration)))
    );
    let value = uses.iter().find(|u| u.descriptor.is_some()).unwrap();
    assert!(matches!(value.descriptor_agreement, Some(false)));
    assert!(
        report
            .findings()
            .iter()
            .any(|f| f.site.span.bytes() == b"false"),
        "check the destination-owned implementation independently of ordinary placement"
    );
    assert!(uses.iter().any(
        |u| u.site.span.bytes() == b"into<Target,(bool) => bool>((x) => x)"
            && u.outcome == Some(PairOutcome::Winner(&graph.implementations()[2].declaration))
    ));
}

#[test]
fn a_fallible_source_result_can_fill_a_waiting_exact_equation_only_conditionally() {
    use compiler::analysis::TypeTerm;
    use compiler::analysis::scalars::NodeFactKind;
    let fixture = Fixture::new(
        "import std::convert::tryInto;
         import std::result::Result;
         type Target = {};
         implement tryInto<Target>(value: bool): Result<Target,i32> { while (true) {} }
         function keep<D,E>(value: Result<D,E>): Result<D,E> { return value; }
         function consume<T>(value: T): T;
         implement<T> consume(value: T): T { return value; }
         function inferred(): void { let v = keep(tryInto<Target>(true)); }
         function ordinary(): void { let v = consume(tryInto<Target>(true)); }
         function mismatch(): Result<Target,bool> { return tryInto(false); }",
    );
    fixture.write(
        "std/convert.rill",
        "import result::Result; export function tryInto<To,From,E>(value: From): Result<To,E>;",
    );
    fixture.write(
        "std/result.rill",
        "export enum Result<T,E> { Ok(T), Err(E) }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let inventory = analyze_conversions(
        &ordinary,
        &[RoleHypothesis::Fallible {
            family: standard_export(&graph, "tryInto"),
            result: standard_export(&graph, "Result"),
        }],
        limits,
    )
    .unwrap();
    let report = inventory.bodies(limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let waiting = node_with(module, b"keep(tryInto<Target>(true))");
    assert!(report.node(module, waiting).unwrap().is_none());
    let facts = report
        .conditional_conversion_node(module, waiting)
        .unwrap()
        .unwrap();
    let NodeFactKind::Expression(facts) = &facts.kind else {
        panic!("call")
    };
    let call = facts.call.as_ref().unwrap();
    assert!(
        matches!(call.substitution[1].argument.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
    );
    let ordinary_node = node_with(module, b"consume(tryInto<Target>(true))");
    let ordinary = report
        .ordinary_call(module, ordinary_node)
        .unwrap()
        .unwrap();
    assert_eq!(
        ordinary.premise,
        Some(compiler::analysis::ordinary::InputPremise::HypotheticalOrdinaryAndConversionRoles)
    );
    assert_eq!(
        report
            .goal(ordinary.goal.as_ref().unwrap())
            .unwrap()
            .premise,
        ordinary.premise
    );
    let wrong = report
        .conversion_use(module, node_with(module, b"tryInto(false)"))
        .unwrap()
        .unwrap();
    assert!(matches!(
        wrong.error_agreement,
        compiler::analysis::conversions::ErrorAgreement::Disagrees { .. }
    ));
    assert!(wrong.selected_result.is_some());
}

#[test]
fn symbolic_selection_needs_uniform_output_or_an_exact_assumption() {
    use compiler::analysis::conversions::PairOutcome;
    use compiler::analysis::generics::{GoalBlocker, GoalState};
    let fixture = Fixture::new(
        "import std::convert::tryInto;
         import std::result::Result;
         type Outer = {};
         type Inner = {};
         type Stable = {};
         type Box<T> = { value: T };
         implement<T> tryInto<Inner>(value: Box<T>): Result<Inner,i32> { while (true) {} }
         implement tryInto<Inner>(value: Box<bool>): Result<Inner,bool> { while (true) {} }
         implement<T,E> tryInto<Outer>(value: Box<T>): Result<Outer,E>
             where tryInto(Inner,Box<T>,E) { while (true) {} }
         function open<T>(value: Box<T>): void { let x = tryInto<Outer>(value); }
         function assumed<T,E>(value: Box<T>): void where tryInto(Inner,Box<T>,E) { let x = tryInto<Outer,Box<T>>(value); }
         function conflicting<T>(value: Box<T>): void where tryInto(Inner,Box<T>,i32), tryInto(Inner,Box<T>,bool) { let x = tryInto<Inner>(value); }
         implement<T> tryInto<Stable>(value: Box<T>): Result<Stable,i32> { while (true) {} }
         implement tryInto<Stable>(value: Box<bool>): Result<Stable,bool> where noBorrow(&bool) { while (true) {} }
         function stable<T>(value: Box<T>): void { let x = tryInto<Stable>(value); }
         function concrete(value: Box<bool>): void { let x = tryInto<Outer>(value); }",
    );
    fixture.write(
        "std/convert.rill",
        "import result::Result; export function tryInto<To,From,E>(value: From): Result<To,E>;",
    );
    fixture.write(
        "std/result.rill",
        "export enum Result<T,E> { Ok(T), Err(E) }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let inventory = analyze_conversions(
        &ordinary,
        &[RoleHypothesis::Fallible {
            family: standard_export(&graph, "tryInto"),
            result: standard_export(&graph, "Result"),
        }],
        limits,
    )
    .unwrap();
    let report = inventory.bodies(limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let uses: Vec<_> = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(node, _)| report.conversion_use(module, node).unwrap())
        .collect();
    let open: Vec<_> = uses
        .iter()
        .filter(|u| u.site.span.bytes() == b"tryInto<Outer>(value)")
        .collect();
    assert_eq!(open.len(), 2);
    assert!(open[0].selected_result.is_none() && open[0].outcome.is_none());
    assert_eq!(
        open[1].outcome,
        Some(PairOutcome::Winner(&graph.implementations()[2].declaration))
    );
    let assumed = uses
        .iter()
        .find(|u| u.site.span.bytes() == b"tryInto<Outer,Box<T>>(value)")
        .unwrap();
    let proof = report.goal(assumed.goal.as_ref().unwrap()).unwrap();
    assert_eq!(
        assumed.outcome,
        Some(PairOutcome::Winner(&graph.implementations()[2].declaration))
    );
    let output = proof.output.as_ref().unwrap();
    assert!(matches!(
        output.arguments[2].as_ref(),
        compiler::analysis::TypeTerm::Parameter { .. }
    ));
    let nested = proof
        .dependencies
        .iter()
        .map(|id| report.goal(id).unwrap())
        .find(|g| g.output.is_some())
        .unwrap();
    assert!(
        nested.selection.is_none(),
        "a relation does not select a body"
    );
    assert!(!nested.dependencies.is_empty());
    let stable = uses
        .iter()
        .find(|u| u.site.span.bytes() == b"tryInto<Stable>(value)")
        .unwrap();
    assert_eq!(
        stable.outcome,
        Some(PairOutcome::Winner(&graph.implementations()[3].declaration))
    );
    let conflicting = uses
        .iter()
        .find(|u| u.site.span.bytes() == b"tryInto<Inner>(value)")
        .unwrap();
    assert_eq!(
        report
            .goal(conflicting.goal.as_ref().unwrap())
            .unwrap()
            .state,
        GoalState::Unresolved(GoalBlocker::ConflictingAssumptions)
    );
}

#[test]
fn filter_equivalence_is_per_candidate_not_by_union() {
    use compiler::analysis::conversions::{ErrorAgreement, PairOutcome};
    let fixture = Fixture::new(
        "import std::convert::tryInto;
         import std::result::Result;
         type Target = {};
         enum Source { Tag }
         implement tryInto<Target>(value: Source): Result<Target,i32> { while (true) {} }
         implement tryInto<Target>(value: Source.Tag): Result<Target,bool> { while (true) {} }
         function run<E>(value: Source.Tag): void where tryInto(Target,Source.Tag,E) {
             let unfiltered = tryInto<Target>(value);
             let filtered = tryInto<Target,Source.Tag,E>(value);
         }",
    );
    fixture.write(
        "std/convert.rill",
        "import result::Result; export function tryInto<To,From,E>(value: From): Result<To,E>;",
    );
    fixture.write(
        "std/result.rill",
        "export enum Result<T,E> { Ok(T), Err(E) }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let inventory = analyze_conversions(
        &ordinary,
        &[RoleHypothesis::Fallible {
            family: standard_export(&graph, "tryInto"),
            result: standard_export(&graph, "Result"),
        }],
        limits,
    )
    .unwrap();
    let report = inventory.bodies(limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let use_at = |span: &[u8]| {
        report
            .conversion_use(module, node_with(module, span))
            .unwrap()
            .unwrap()
    };
    let unfiltered = use_at(b"tryInto<Target>(value)");
    assert!(unfiltered.outcome.is_none() && unfiltered.selected_result.is_some());
    let filtered = use_at(b"tryInto<Target,Source.Tag,E>(value)");
    assert_eq!(
        filtered.outcome,
        Some(PairOutcome::Winner(&graph.implementations()[1].declaration))
    );
    assert!(matches!(
        filtered.error_agreement,
        ErrorAgreement::Disagrees { .. }
    ));
}

#[test]
fn invalid_conversion_role_and_exhausted_inventory_do_not_supply_evidence() {
    use compiler::analysis::generics::{GoalBlocker, GoalState};
    let fixture = Fixture::new(
        "import std::convert::into;
         function need(x: bool): bool where into(bool) { return x; }
         function test(): bool { return need(true); }",
    );
    fixture.write("std/convert.rill", "export function into<T>(value: T): T;");
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let binding = standard_export(&graph, "into");
    let role = RoleHypothesis::Infallible(binding);
    let inventory = analyze_conversions(&ordinary, &[role], limits).unwrap();
    let Target::Declaration(id) = &binding.target else {
        panic!("captured family")
    };
    assert_eq!(
        inventory.family(id).unwrap().unwrap().issues[0].kind,
        RegistrationIssueKind::FamilySignature
    );
    let report = inventory.bodies(limits);
    assert!(
        report
            .goals()
            .iter()
            .any(|g| g.site.span.bytes() == b"need(true)"
                && matches!(
                    g.state,
                    GoalState::Unresolved(GoalBlocker::ConversionPair(_))
                )
                && g.hypothetical.is_none())
    );
    let exhausted = analyze_conversions(
        &ordinary,
        &[],
        AnalysisLimits {
            max_work: 0,
            ..limits
        },
    )
    .unwrap();
    assert_eq!(exhausted.completion(), Completion::ResourceLimit);
}

#[test]
fn a_conversion_error_can_determine_a_standalone_requirement_output() {
    use compiler::analysis::TypeTerm;
    use compiler::analysis::scalars::NodeFactKind;
    let fixture = Fixture::new(
        "import std::convert::tryInto;
         import std::result::Result;
         type Target = {};
         implement tryInto<Target>(value: bool): Result<Target,i32> { while (true) {} }
         function adapt<D,S,E = bool>(value: S): Result<D,E> where tryInto(D,S,E) { while (true) {} }
         function family<S,E>(value: S): Result<Target,E> where tryInto(Target,S,E);
         implement<S,E> family(value: S): Result<Target,E> { while (true) {} }
         function run(): void {
             let v = adapt<Target>(true);
             let w = family(true);
             let wrong = adapt<Target,bool,bool>(false);
         }",
    );
    fixture.write(
        "std/convert.rill",
        "import result::Result; export function tryInto<To,From,E>(value: From): Result<To,E>;",
    );
    fixture.write(
        "std/result.rill",
        "export enum Result<T,E> { Ok(T), Err(E) }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let inventory = analyze_conversions(
        &ordinary,
        &[RoleHypothesis::Fallible {
            family: standard_export(&graph, "tryInto"),
            result: standard_export(&graph, "Result"),
        }],
        limits,
    )
    .unwrap();
    let report = inventory.bodies(limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let node = node_with(module, b"adapt<Target>(true)");
    assert!(report.node(module, node).unwrap().is_none());
    let conditional = report
        .conditional_conversion_node(module, node)
        .unwrap()
        .unwrap();
    let NodeFactKind::Expression(facts) = &conditional.kind else {
        panic!("call")
    };
    let call = facts.call.as_ref().unwrap();
    assert!(call.defaults.is_empty());
    assert!(
        matches!(call.substitution[2].argument.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
    );
    assert_eq!(
        report
            .goals()
            .iter()
            .filter(|g| g.site.node == node && g.output.is_some())
            .count(),
        1
    );
    let wrong = node_with(module, b"adapt<Target,bool,bool>(false)");
    assert!(report.node(module, wrong).unwrap().is_none());
    assert!(
        report
            .conditional_conversion_node(module, wrong)
            .unwrap()
            .is_none()
    );
    assert!(
        report
            .goals()
            .iter()
            .any(|g| g.site.node == wrong && g.equalities.iter().any(|e| !e.agrees))
    );
    let family = report
        .ordinary_call(module, node_with(module, b"family(true)"))
        .unwrap()
        .unwrap();
    assert_eq!(
        family.premise,
        Some(compiler::analysis::ordinary::InputPremise::HypotheticalOrdinaryAndConversionRoles)
    );
    let root = report.goal(family.goal.as_ref().unwrap()).unwrap();
    let result = &root.output.as_ref().unwrap().result;
    assert!(
        matches!(result.as_ref(), TypeTerm::Nominal { arguments, .. }
        if matches!(arguments[1].as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"))
    );
}

#[test]
fn independent_arithmetic_errors_do_not_inherit_a_sibling_conversion_premise() {
    let fixture = Fixture::new(
        "import std::convert::into;
         type Target = {};
         implement into<Target>(value: bool): Target { return Target {}; }
         function run(): void { let v: [Target,u8] = [into<Target,bool>(false), 1u8 + false]; }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let inventory = analyze_conversions(
        &ordinary,
        &[RoleHypothesis::Infallible(standard_export(&graph, "into"))],
        limits,
    )
    .unwrap();
    let report = inventory.bodies(limits);
    assert!(
        report
            .findings()
            .iter()
            .any(|f| f.site.span.bytes() == b"1u8 + false")
    );
    assert!(
        !report
            .conditional_findings()
            .iter()
            .any(|f| f.finding.site.span.bytes() == b"1u8 + false")
    );
}

#[test]
fn aliases_collect_one_family_with_distinct_destination_keys() {
    let fixture = Fixture::new(
        "import std::convert::{into as convert};
         import std::aliases::from;
         type User = { text: string };
         type Email = { text: string };
         implement convert<User>(s: string): User { return User { text: s }; }
         implement from<Email>(s: string): Email { return Email { text: s }; }
         function into<T>(s: T): T;",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let binding = standard_export(&graph, "from");
    let Target::Declaration(id) = &binding.target else {
        panic!("real family attachment")
    };
    let report =
        analyze_conversions(&ordinary, &[RoleHypothesis::Infallible(binding)], limits).unwrap();
    assert_eq!(report.completion(), Completion::Incomplete);
    let family = report.family(id).unwrap().unwrap();
    assert_eq!(family.registrations.len(), 2);
    assert_ne!(family.registrations[0].key, family.registrations[1].key);
    assert!(
        matches!(family.premise, RoleHypothesis::Infallible(export) if std::ptr::eq(export, binding))
    );
    assert!(family.routes.iter().any(|route| route.name == "convert"));
    let ordinary_family = ordinary.family(id).unwrap().unwrap();
    assert_eq!(
        ordinary_family.duplicate_keys.len(),
        1,
        "the ordinary view is separate"
    );
    let local = graph
        .bindings()
        .iter()
        .find(|b| b.module.origin == Origin::Application && b.name == "into")
        .unwrap();
    let Target::Declaration(local) = &local.target else {
        panic!("local family")
    };
    assert!(report.family(local).unwrap().is_none());

    let other = fixture.load();
    assert_eq!(
        analyze_conversions(
            &ordinary,
            &[RoleHypothesis::Infallible(standard_export(&other, "from"))],
            limits
        )
        .unwrap_err(),
        QueryError::InvalidRequest
    );
}

/// Metering rule 8 for `ConversionReport::bodies`, which returns the same
/// report as `analyze_generics`: the exact cap publishes the body finding;
/// every cap below it, on each axis, publishes none.
#[test]
fn conversion_bodies_publish_no_finding_at_resource_limit() {
    let fixture = Fixture::new(
        "import std::convert::into;
         type Target = {};
         implement into<Target>(value: i32): Target { return false; }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let inventory = analyze_conversions(
        &ordinary,
        &[RoleHypothesis::Infallible(standard_export(&graph, "into"))],
        limits,
    )
    .unwrap();
    let report = inventory.bodies(limits);
    let stats = report.statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    let replay = inventory.bodies(exact);
    assert_eq!(replay.statistics(), stats);
    assert!(!replay.findings().is_empty());
    for work in 0..stats.work {
        let limited = inventory.bodies(AnalysisLimits {
            max_work: work,
            ..exact
        });
        limit::assert_generics_limit(&limited);
    }
    for entries in 0..stats.entries {
        let limited = inventory.bodies(AnalysisLimits {
            max_entries: entries,
            ..exact
        });
        limit::assert_generics_limit(&limited);
    }
    for depth in 0..stats.max_depth {
        let limited = inventory.bodies(AnalysisLimits {
            max_depth: depth,
            ..exact
        });
        limit::assert_generics_limit(&limited);
    }
}
