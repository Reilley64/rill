//! M4.6 public queries over real M3 captures, not Rill execution.
use compiler::analysis::generics::analyze_generics;
use compiler::analysis::ordinary::analyze_ordinary;
use compiler::analysis::scalars::{BodyOwner, NodeFactKind, ScalarFindingKind, ScalarUnsupported};
use compiler::analysis::{AnalysisLimits, TypeTerm};
use compiler::loader::{Graph, Limits, Prelude, Request, Selector, load};
use compiler::parser::NodeKind;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "rill-m4-6-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let fixture = Self(path);
        for dir in ["app", "std"] {
            fs::create_dir(fixture.0.join(dir)).unwrap();
        }
        fs::write(fixture.0.join("app/main.rill"), source).unwrap();
        fs::write(
            fixture.0.join("std/core.rill"),
            "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
        )
        .unwrap();
        fixture
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
            target: "generics-test".into(),
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
fn generic_body_checks_its_rigid_parameter_without_concrete_callers() {
    let fixture = Fixture::new("function identity<T = i32>(x: T): T { let y = x; return y; }");
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let function = module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("identity"))
        .unwrap();
    let binder = module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("T"))
        .unwrap();
    let local = module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("y"))
        .unwrap();
    let body = report
        .body(module, BodyOwner::Declaration(&function.id))
        .unwrap()
        .expect("rigid generic body checked");
    let result = body.result.as_ref().unwrap();
    assert!(
        matches!(result.as_ref(), TypeTerm::Parameter { declaration, position: 0 }
        if **declaration == binder.id)
    );
    assert!(body.flow.returns);
    let (node, _) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .rfind(|(_, n)| n.kind == NodeKind::ValueReference)
        .unwrap();
    let fact = report.node(module, node).unwrap().unwrap();
    let NodeFactKind::Expression(expr) = &fact.kind else {
        panic!("value expression")
    };
    assert_eq!(&expr.ty, result);
    assert_eq!(expr.binding, Some(&local.id));
    assert_eq!(fact.site.node, node);
}

#[test]
fn standalone_call_infers_bool_before_the_i32_default() {
    let fixture = Fixture::new(
        "function identity<T = i32>(value: T): T { return value; }
         let inferred = identity(true);",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let (node, _) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == NodeKind::Call)
        .unwrap();
    let fact = report
        .node(module, node)
        .unwrap()
        .expect("inferred call type");
    let NodeFactKind::Expression(expr) = &fact.kind else {
        panic!("call expression")
    };
    assert!(matches!(expr.ty.as_ref(), TypeTerm::Primitive(id) if id.name() == "bool"));
    let call = expr.call.as_ref().expect("standalone call witness");
    assert_eq!(call.substitution.len(), 1);
    assert_eq!(call.substitution[0].argument, expr.ty);
    assert!(report.body(module, BodyOwner::Startup).unwrap().is_some());
}

#[test]
fn a_ready_written_default_can_supply_width_but_no_numeric_default_is_invented() {
    for (declaration, expected) in [
        (
            "function identity<T = i32>(value: T): T { return value; }",
            Some("i32"),
        ),
        ("function identity<T>(value: T): T { return value; }", None),
    ] {
        let fixture = Fixture::new(&format!("{declaration} let inferred = identity(2);"));
        let graph = fixture.load();
        let limits = AnalysisLimits::default();
        let ordinary = analyze_ordinary(&graph, limits);
        let report = analyze_generics(&ordinary, limits);
        let module = graph
            .modules()
            .iter()
            .find(|m| m.key.path == "main.rill")
            .unwrap();
        let (node, _) = module
            .syntax
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.kind == NodeKind::Call)
            .unwrap();
        let actual = report.node(module, node).unwrap().map(|fact| {
            let NodeFactKind::Expression(expr) = &fact.kind else {
                panic!("call expression")
            };
            let TypeTerm::Primitive(id) = expr.ty.as_ref() else {
                panic!("literal width")
            };
            id.name()
        });
        assert_eq!(actual, expected);
        if expected.is_some() {
            let fact = report.node(module, node).unwrap().unwrap();
            let NodeFactKind::Expression(expr) = &fact.kind else {
                panic!("call expression")
            };
            let call = expr.call.as_ref().unwrap();
            assert_eq!(call.defaults.len(), 1);
            assert_eq!(call.defaults[0].binder, call.substitution[0].binder);
        }
    }
}

#[test]
fn a_fixed_outer_parent_admits_variant_without_changing_a_free_slot() {
    let fixture = Fixture::new(
        "enum Color { Red, Blue }
         function retain<T>(color: Color, value: T): T { return value; }
         let kept = retain(Color.Red, true);",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let input = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&input, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let (node, _) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == NodeKind::Call)
        .unwrap();
    let call = report
        .node(module, node)
        .unwrap()
        .expect("parent admits the variant");
    let NodeFactKind::Expression(expr) = &call.kind else {
        panic!("call expression")
    };
    assert!(matches!(expr.ty.as_ref(), TypeTerm::Primitive(id) if id.name() == "bool"));
    let actual = expr.call.as_ref().unwrap().arguments[0];
    let argument = report.node(module, actual).unwrap().unwrap();
    let NodeFactKind::Expression(expr) = &argument.kind else {
        panic!("variant expression")
    };
    assert!(matches!(expr.ty.as_ref(), TypeTerm::Variant { .. }));
    assert!(expr.admission.is_some());
}

#[test]
fn a_default_cannot_repair_conflicting_exact_input_anchors() {
    let fixture = Fixture::new(
        "function same<T = i32>(left: T, right: T): T { return left; }
         let invalid = same(true, 1i32);",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let (node, _) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == NodeKind::Call)
        .unwrap();
    assert!(report.node(module, node).unwrap().is_none());
    assert!(
        report
            .findings()
            .iter()
            .any(|f| f.kind == ScalarFindingKind::TypeMismatch)
    );
    assert!(report.body(module, BodyOwner::Startup).unwrap().is_none());
}

#[test]
fn ordinary_source_context_needs_all_viable_alternatives_to_agree() {
    use compiler::analysis::ordinary::{HypotheticalOutcome, InputPremise};
    let fixture = Fixture::new(
        "function pair<A,B>(left: A, right: B): bool;
         implement<U,V> pair(left: U, right: V): bool { return true; }
         implement<T> pair(left: T, right: T): bool { return false; }
         let open = pair(1i32, 2);
         let fixed = pair(1i32, 2i32);",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let mut calls = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == NodeKind::Call);
    let (open, _) = calls.next().unwrap();
    let (fixed, _) = calls.next().unwrap();
    let partial = report.ordinary_call(module, open).unwrap().unwrap();
    assert_eq!(partial.premise, InputPremise::HypotheticalOrdinaryRole);
    assert_eq!(partial.candidates.len(), 2);
    assert!(partial.inputs[1].is_none());
    assert_eq!(partial.outcome, None);
    let selected = report.ordinary_call(module, fixed).unwrap().unwrap();
    let repeated = graph
        .implementations()
        .iter()
        .max_by_key(|i| i.declaration.start)
        .unwrap();
    assert_eq!(
        selected.outcome,
        Some(HypotheticalOutcome::Winner(&repeated.declaration))
    );
    assert!(report.node(module, fixed).unwrap().is_none());
}

#[test]
fn ordinary_tuple_context_commits_category_then_filters_written_arity() {
    use compiler::analysis::ordinary::{HypotheticalOutcome, InputPremise};
    let fixture = Fixture::new(
        "function grouped<W>(work: W): bool;
         implement grouped(work: [i32,bool]): bool { return true; }
         implement grouped(work: [u8,u8,u8]): bool { return false; }
         let selected = grouped([1, true]);",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let (call, _) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == NodeKind::Call)
        .unwrap();
    let selected = report.ordinary_call(module, call).unwrap().unwrap();
    let pair = graph
        .implementations()
        .iter()
        .min_by_key(|i| i.declaration.start)
        .unwrap();
    assert_eq!(
        selected.outcome,
        Some(HypotheticalOutcome::Winner(&pair.declaration))
    );
    let (_, bracket) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == NodeKind::Bracket)
        .unwrap();
    let literal = report.node(module, bracket.children[0]).unwrap().unwrap();
    let NodeFactKind::Expression(expr) = &literal.kind else {
        panic!("argument literal")
    };
    assert_eq!(
        expr.ordinary_context,
        Some(InputPremise::HypotheticalOrdinaryRole)
    );
    assert!(matches!(expr.ty.as_ref(), TypeTerm::Primitive(id) if id.name() == "i32"));
    assert!(report.node(module, call).unwrap().is_none());
}

#[test]
fn a_symbolic_call_requires_one_winner_across_its_whole_input_domain() {
    use compiler::analysis::ordinary::HypotheticalOutcome;
    let fixture = Fixture::new(
        "function pick<T,R>(value: T): R;
         implement<U> pick(value: U): i32 { return 1i32; }
         implement pick(value: bool): bool { return false; }
         function open<T>(x: T): void { let pending = pick(x); }
         function closed<T>(x: &T): void { let result = pick(x); }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let mut calls = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == NodeKind::Call);
    let open = report
        .ordinary_call(module, calls.next().unwrap().0)
        .unwrap()
        .unwrap();
    assert_eq!(open.candidates.len(), 2);
    assert_eq!(open.outcome, None);
    assert!(open.selected_result.is_none());
    let closed = report
        .ordinary_call(module, calls.next().unwrap().0)
        .unwrap()
        .unwrap();
    assert_eq!(closed.candidates.len(), 1);
    assert_eq!(
        closed.outcome,
        Some(HypotheticalOutcome::Winner(
            closed.candidates[0].implementation
        ))
    );
    assert!(
        matches!(closed.selected_result.as_deref(), Some(TypeTerm::Primitive(id)) if id.name() == "i32")
    );
}

#[test]
fn an_output_argument_checks_but_does_not_filter_the_winning_header() {
    use compiler::analysis::generics::OutputAgreement;
    use compiler::analysis::ordinary::HypotheticalOutcome;
    let fixture = Fixture::new(
        "function pick<T,R>(value: T): R;
         implement<U> pick(value: U): i32 { return 1i32; }
         implement pick(value: bool): bool { return false; }
         let invalid = pick<bool,i32>(true);",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let (node, _) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == NodeKind::Call)
        .unwrap();
    let facts = report.ordinary_call(module, node).unwrap().unwrap();
    let expected_winner = graph
        .implementations()
        .iter()
        .max_by_key(|i| i.declaration.start)
        .unwrap();
    assert_eq!(
        facts.outcome,
        Some(HypotheticalOutcome::Winner(&expected_winner.declaration))
    );
    let OutputAgreement::Disagrees { actual, required } = &facts.output_agreement else {
        panic!("exact output mismatch")
    };
    assert!(matches!(actual.as_ref(), TypeTerm::Primitive(id) if id.name() == "bool"));
    assert!(matches!(required.as_ref(), TypeTerm::Primitive(id) if id.name() == "i32"));
    assert!(report.node(module, node).unwrap().is_none());
    assert!(report.findings().is_empty());
}

#[test]
fn a_contextual_literal_rejection_keeps_its_ordinary_premise() {
    use compiler::analysis::ordinary::InputPremise;
    let fixture = Fixture::new(
        "function small<T>(value: T): bool;
         implement small(value: u8): bool { return true; }
         let invalid = small(256);",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    assert!(report.findings().is_empty());
    assert!(
        report
            .conditional_findings()
            .iter()
            .any(|f| f.premise == InputPremise::HypotheticalOrdinaryRole
                && f.finding.kind == ScalarFindingKind::LiteralRange)
    );
}

#[test]
fn an_implementation_body_keeps_rigid_local_facts_without_admission() {
    use compiler::analysis::generics::BodyCompletion;
    let fixture = Fixture::new(
        "function identity<T>(value: T): T;
         implement<U> identity(value: U): U { return value; }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let owner = BodyOwner::Declaration(&graph.implementations()[0].declaration);
    assert_eq!(
        report.body_status(module, owner).unwrap().completion,
        BodyCompletion::Incomplete
    );
    assert!(report.body(module, owner).unwrap().is_none());
    let (node, _) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == NodeKind::ValueReference)
        .unwrap();
    let fact = report.node(module, node).unwrap().unwrap();
    let NodeFactKind::Expression(expr) = &fact.kind else {
        panic!("implementation parameter")
    };
    let TypeTerm::Parameter { declaration, .. } = expr.ty.as_ref() else {
        panic!("rigid type")
    };
    assert!(
        module
            .declarations
            .iter()
            .any(|d| d.id == **declaration && d.name.as_deref() == Some("U"))
    );
}

#[test]
fn an_unadmitted_family_cannot_supply_a_checked_call() {
    let fixture = Fixture::new(
        "function via<T>(value: T): T;
         implement<U> via(value: U): U { return value; }
         let pending = via(true);",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let (node, _) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == NodeKind::Call)
        .unwrap();
    assert!(report.node(module, node).unwrap().is_none());
    assert!(report.findings().is_empty());
    assert!(
        report
            .obligations()
            .iter()
            .any(|o| o.site.node == node && o.reason == ScalarUnsupported::FamilySelection)
    );
}
