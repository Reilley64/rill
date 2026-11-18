//! Conditional conversion analysis over real M3 captures. Fixtures grant no core admission.
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
         implement<T> tryInto<Target>(value: Wrap<T>): Result<Target,i32> { while (true) {} }
         implement tryInto<Target>(value: Wrap<&bool>): Result<Target,bool> where noBorrow(&bool) { while (true) {} }",
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
