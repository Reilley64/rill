//! M4.6 public queries over real M3 captures, not Rill execution.
#[path = "support/limit.rs"]
mod limit;

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
    assert_eq!(
        partial.premise,
        Some(InputPremise::HypotheticalOrdinaryRole)
    );
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
fn borrowed_pattern_projects_callback_input_before_inferring_selected_output() {
    use compiler::analysis::ordinary::HypotheticalOutcome;
    let fixture = Fixture::new(
        "type Box<T> = { value: T };
         function mapLike<C,F,R>(source: C, transform: F): R;
         implement<T,F,R> mapLike(source: Box<T>, transform: F): R
             where callableMut(F,[T],R) { while (true) {} }
         implement<T,F,R> mapLike(source: &Box<T>, transform: F): R
             where callableMut(F,[&T],R) { while (true) {} }
         function probe(borrowed: &Box<i32>): void {
             let pending = mapLike(borrowed, (element) => true);
         }",
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
    let call = report
        .ordinary_call(module, node)
        .unwrap()
        .unwrap_or_else(|| panic!("conditional source call: {:?}; declarations: {:?}; type findings: {:?}; type obligations: {:?}; body obligations: {:?}", report.completion(), ordinary.declaration_issues(), ordinary.type_facts().findings().collect::<Vec<_>>(), ordinary.type_facts().obligations().collect::<Vec<_>>(), report.obligations()));
    let context = &call.callbacks[0];
    assert_eq!(context.parameters.len(), 1);
    let Some(TypeTerm::Reference {
        mutable: false,
        referent,
    }) = context.parameters[0].as_deref()
    else {
        panic!("borrowed callback context")
    };
    assert!(matches!(referent.as_ref(), TypeTerm::Primitive(id) if id.name() == "i32"));
    assert!(context.result.is_none());
    let arrow = report.arrow(module, context.argument).unwrap().unwrap();
    assert_eq!(arrow.parameters.first(), context.parameters[0].as_ref());
    assert_eq!(arrow.premise, call.premise);
    let specialized = graph
        .implementations()
        .iter()
        .max_by_key(|i| i.declaration.start)
        .unwrap();
    assert_eq!(
        call.outcome,
        Some(HypotheticalOutcome::Winner(&specialized.declaration))
    );
    assert!(
        matches!(call.selected_result.as_deref(), Some(TypeTerm::Primitive(id)) if id.name() == "bool")
    );
    assert!(report.node(module, node).unwrap().is_none());
}

#[test]
fn only_a_free_selected_output_can_use_context_or_a_written_default() {
    let fixture = Fixture::new(
        "function make<T,R = i32>(value: T): R;
         implement<U,R> make(value: U): R where noBorrow(R) { while (true) {} }
         let inferred: bool = make(true);
         let defaulted = make(true);",
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
    for (kind, count) in [("bool", 0), ("i32", 1)] {
        let (node, _) = calls.next().unwrap();
        let call = report.ordinary_call(module, node).unwrap().unwrap();
        assert!(
            matches!(call.selected_result.as_deref(), Some(TypeTerm::Primitive(id)) if id.name() == kind)
        );
        assert_eq!(call.defaults.len(), count);
        let goal = report.goal(call.goal.as_ref().unwrap()).unwrap();
        assert!(
            goal.output.is_none(),
            "independent determination cannot borrow the caller's expectation"
        );
        use compiler::analysis::generics::{GoalBlocker, GoalState, GoalVerdict};
        assert_eq!(call.instantiated_requirements.len(), 1);
        let proof = report.goal(&call.instantiated_requirements[0]).unwrap();
        assert_eq!(
            proof.state,
            GoalState::Unresolved(GoalBlocker::OrdinaryAdmission)
        );
        assert_eq!(proof.hypothetical, Some(GoalVerdict::Proved));
        assert_eq!(proof.source.span.bytes(), b"noBorrow(R)");
        assert!(report.node(module, node).unwrap().is_none());
    }
}

#[test]
fn a_family_input_default_supplies_width_without_becoming_an_explicit_filter() {
    let fixture = Fixture::new(
        "function size<T = i32>(value: T): bool;
         implement<U> size(value: U): bool { return true; }
         let defaulted = size(1);
         let anchored = size(1u8);",
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
    let defaulted = report
        .ordinary_call(module, calls.next().unwrap().0)
        .unwrap()
        .unwrap();
    assert!(
        matches!(defaulted.inputs[0].as_deref(), Some(TypeTerm::Primitive(id)) if id.name() == "i32")
    );
    assert_eq!(defaulted.defaults.len(), 1);
    assert!(defaulted.explicit.is_empty());
    let anchored = report
        .ordinary_call(module, calls.next().unwrap().0)
        .unwrap()
        .unwrap();
    assert!(
        matches!(anchored.inputs[0].as_deref(), Some(TypeTerm::Primitive(id)) if id.name() == "u8")
    );
    assert!(anchored.defaults.is_empty());
}

#[test]
fn each_tuple_callback_uses_its_own_written_path() {
    use compiler::analysis::generics::{GoalBlocker, GoalState};
    let fixture = Fixture::new(
        "function joinLike<F,G,A,B>(tasks: [F,G]): [A,B]
             where callableOnce(F,[],A), callableOnce(G,[],B);
         implement<F,G,A,B> joinLike(tasks: [F,G]): [A,B] { while (true) {} }
         let pending = joinLike([() => true, () => 42i32]);",
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
    let call = report.ordinary_call(module, node).unwrap().unwrap();
    assert_eq!(call.callbacks.len(), 2);
    assert!(
        call.callbacks
            .iter()
            .all(|c| c.parameters.is_empty() && c.result.is_none())
    );
    for (context, expected) in call.callbacks.iter().zip(["bool", "i32"]) {
        let arrow = report.arrow(module, context.argument).unwrap().unwrap();
        assert!(matches!(arrow.result.as_ref(), TypeTerm::Primitive(id) if id.name() == expected));
    }
    let Some(TypeTerm::Tuple(fields)) = call.selected_result.as_deref() else {
        panic!("selected pair of results")
    };
    for (field, expected) in fields.iter().zip(["bool", "i32"]) {
        assert!(matches!(field.as_ref(), TypeTerm::Primitive(id) if id.name() == expected));
    }
    let goal = report.goal(call.goal.as_ref().unwrap()).unwrap();
    assert_eq!(
        goal.state,
        GoalState::Unresolved(GoalBlocker::OrdinaryAdmission)
    );
    assert!(report.node(module, node).unwrap().is_none());
}

#[test]
fn nested_tuple_callbacks_get_the_contract_at_each_committed_path() {
    let fixture = Fixture::new(
        "function joinDeep<F,G,A,B>(tasks: [[F],[bool,[G]]]): [A,B]
             where callableOnce(F,[i32],A), callableOnce(G,[bool],B);
         implement<F,G,A,B> joinDeep(tasks: [[F],[bool,[G]]]): [A,B] { while (true) {} }
         let pending = joinDeep(([[(x) => x == 1i32],[true,[(b) => { if (b) { return 1i32; } return 2i32; }]]]));",
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
    let node = module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Call)
        .unwrap();
    let call = report.ordinary_call(module, node).unwrap().unwrap();
    assert_eq!(call.callbacks.len(), 2);
    for (context, expected) in call.callbacks.iter().zip(["i32", "bool"]) {
        assert!(
            matches!(context.parameters[0].as_deref(), Some(TypeTerm::Primitive(id)) if id.name() == expected)
        );
        let arrow = report.arrow(module, context.argument).unwrap().unwrap();
        assert_eq!(arrow.parameters.first(), context.parameters[0].as_ref());
    }
    let Some(TypeTerm::Tuple(fields)) = call.selected_result.as_deref() else {
        panic!("selected pair of results")
    };
    for (field, expected) in fields.iter().zip(["bool", "i32"]) {
        assert!(matches!(field.as_ref(), TypeTerm::Primitive(id) if id.name() == expected));
    }
    assert!(report.node(module, node).unwrap().is_none());
}

#[test]
fn a_typed_tuple_sibling_anchors_the_nested_callback_parameter() {
    let fixture = Fixture::new(
        "function visit<T,F,R>(tasks: [T,[F]]): R where callable(F,[T],R);
         implement<T,F,R> visit(tasks: [T,[F]]): R { while (true) {} }
         let pending = visit([1i32,[(x) => x == 2i32]]);",
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
    let node = module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Call)
        .unwrap();
    let call = report.ordinary_call(module, node).unwrap().unwrap();
    assert!(
        matches!(call.callbacks[0].parameters[0].as_deref(), Some(TypeTerm::Primitive(id)) if id.name() == "i32")
    );
    assert!(
        matches!(call.selected_result.as_deref(), Some(TypeTerm::Primitive(id)) if id.name() == "bool")
    );
    assert!(report.node(module, node).unwrap().is_none());
}

#[test]
fn nested_tuple_arity_filters_only_after_category_agrees() {
    use compiler::analysis::ordinary::HypotheticalOutcome;
    let fixture = Fixture::new(
        "function visit<C>(value: C): bool;
         implement visit(value: [[(i32) => bool]]): bool { return true; }
         implement visit(value: [[(u8) => bool, bool]]): bool { return false; }
         let success = visit([[(x) => x == 1i32]]);
         let wrong = visit([[(x) => x == 1i32, true, false]]);",
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
    let success = report
        .ordinary_call(module, calls.next().unwrap().0)
        .unwrap()
        .unwrap();
    assert_eq!(success.callbacks.len(), 1);
    assert!(
        matches!(success.callbacks[0].parameters[0].as_deref(), Some(TypeTerm::Primitive(id)) if id.name() == "i32")
    );
    assert!(matches!(
        success.outcome,
        Some(HypotheticalOutcome::Winner(_))
    ));
    let wrong = report
        .ordinary_call(module, calls.next().unwrap().0)
        .unwrap()
        .unwrap();
    assert!(wrong.callbacks.is_empty());
    assert!(wrong.outcome.is_none());
    assert!(report.findings().is_empty());
    assert!(
        report
            .conditional_findings()
            .iter()
            .any(|f| f.finding.kind == ScalarFindingKind::Arity
                && f.finding.site.span.bytes() == b"[(x) => x == 1i32, true, false]")
    );
    assert!(
        !report
            .obligations()
            .iter()
            .any(|o| o.reason == ScalarUnsupported::CoreAdmission)
    );
}

#[test]
fn a_bad_nested_tuple_cannot_become_a_typed_input_under_exhaustion_or_projection() {
    let fixture = Fixture::new(
        "function visit<C>(value: C): bool;
         implement visit(value: [[i32,(i32) => bool]]): bool { return true; }
         let wrong = visit([[true]]);",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let node = module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Call)
        .unwrap();
    let report = analyze_generics(&ordinary, limits);
    let call = report.ordinary_call(module, node).unwrap().unwrap();
    assert!(call.inputs[0].is_none() && call.outcome.is_none() && call.goal.is_none());
    assert!(
        report
            .conditional_findings()
            .iter()
            .any(|f| f.finding.kind == ScalarFindingKind::Arity
                && f.finding.site.span.bytes() == b"[true]")
    );
    for max_entries in 320..=520 {
        let report = analyze_generics(
            &ordinary,
            AnalysisLimits {
                max_entries,
                ..limits
            },
        );
        assert!(
            !report
                .findings()
                .iter()
                .any(|f| f.kind == ScalarFindingKind::Arity),
            "{max_entries}"
        );
    }
}

#[test]
fn read_only_invocation_inside_a_projected_arrow_keeps_the_premise() {
    let fixture = Fixture::new(
        "function visit<F,R>(value: [[F]]): R where callable(F,[&(once () => i32)],R);
         implement<F,R> visit(value: [[F]]): R { while (true) {} }
         let pending = visit([[(f) => f()]]);",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    assert!(
        !report
            .findings()
            .iter()
            .any(|f| f.kind == ScalarFindingKind::ReadOnly)
    );
    assert!(
        report
            .conditional_findings()
            .iter()
            .any(|f| f.finding.kind == ScalarFindingKind::ReadOnly
                && f.finding.site.span.bytes() == b"f()")
    );
}

#[test]
fn conflicting_nested_callback_receivers_cannot_supply_a_parameter() {
    let fixture = Fixture::new(
        "function visit<C>(value: C): bool;
         implement visit(value: [[(i32) => bool]]): bool { return true; }
         implement visit(value: [[(u8) => bool]]): bool { return false; }
         let pending = visit([[(x) => x == 1i32]]);",
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
    let node = module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Call)
        .unwrap();
    let call = report.ordinary_call(module, node).unwrap().unwrap();
    assert!(call.callbacks.iter().all(|c| c.parameters[0].is_none()));
    assert_eq!(call.candidates.len(), 2);
    assert!(call.inputs[0].is_none() && call.outcome.is_none());
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
fn arrow_contract_diagnostics_retain_the_hypothetical_premise() {
    use compiler::analysis::ordinary::InputPremise;
    let fixture = Fixture::new(
        "function apply<F>(callback: F): bool;
         implement apply(callback: (i32) => bool): bool { return true; }
         function wrong(): void { let a = apply((x: bool) => true); }
         function uncovered(): void { let b = apply((x) => {}); }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    assert!(report.findings().is_empty(), "{:?}", report.findings());
    for kind in [
        ScalarFindingKind::TypeMismatch,
        ScalarFindingKind::ReturnCoverage,
    ] {
        assert!(
            report
                .conditional_findings()
                .iter()
                .any(|f| f.premise == InputPremise::HypotheticalOrdinaryRole
                    && f.finding.kind == kind)
        );
    }
}

#[test]
fn a_contextual_family_value_ranks_inputs_then_compares_the_result() {
    use compiler::analysis::generics::OutputAgreement;
    let fixture = Fixture::new(
        "function pick<T,R>(value: T): R;
         implement<U> pick(value: U): i32 { return 1i32; }
         implement pick(value: bool): bool { return false; }
         let pending: (bool) => i32 = pick;",
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
        .find(|(_, n)| n.kind == NodeKind::ValueReference && n.span.bytes() == b"pick")
        .unwrap();
    let value = report.ordinary_value(module, node).unwrap().unwrap();
    let goal = report.goal(&value.goal).unwrap();
    let winner = graph
        .implementations()
        .iter()
        .max_by_key(|i| i.declaration.start)
        .unwrap();
    assert_eq!(
        goal.selection.as_ref().unwrap().implementation,
        &winner.declaration
    );
    assert!(
        matches!(&value.output_agreement, OutputAgreement::Disagrees { actual, required } if matches!(actual.as_ref(), TypeTerm::Primitive(id) if id.name() == "bool") && matches!(required.as_ref(), TypeTerm::Primitive(id) if id.name() == "i32"))
    );
    assert!(report.node(module, node).unwrap().is_none());
}

#[test]
fn a_written_exact_output_assumption_can_cover_an_equivalent_unfiltered_call() {
    use compiler::analysis::generics::{GoalBlocker, GoalState, OutputAgreement};
    let fixture = Fixture::new(
        "function read<R,E>(value: R): E;
         implement<U> read(value: U): i32 { return 1i32; }
         implement read(value: bool): bool { return false; }
         function useRead<R,E>(value: R): E where read(R,E) { return read(value); }",
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
    let call = report.ordinary_call(module, node).unwrap().unwrap();
    assert_eq!(
        call.outcome, None,
        "the currently loaded fallback is not a uniform winner"
    );
    assert!(matches!(call.output_agreement, OutputAgreement::Agrees));
    let goal = report.goal(call.goal.as_ref().unwrap()).unwrap();
    assert_eq!(
        goal.state,
        GoalState::Unresolved(GoalBlocker::OrdinaryAdmission)
    );
    assert_eq!(goal.assumptions.len(), 1);
    assert_eq!(goal.dependencies, goal.assumptions);
    let Some(TypeTerm::Parameter { declaration, .. }) = call.selected_result.as_deref() else {
        panic!("exact symbolic output")
    };
    assert!(
        module
            .declarations
            .iter()
            .any(|d| d.id == **declaration && d.name.as_deref() == Some("E"))
    );
    assert!(report.node(module, node).unwrap().is_none());
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

/// Metering rule 8 for the conditional finding list: the exact cap publishes
/// the `Arity` finding; every cap below it, on each axis, publishes none.
#[test]
fn resource_limit_publishes_no_conditional_finding() {
    let fixture = Fixture::new(
        "function visit<C>(value: C): bool;
         implement visit(value: [[i32,(i32) => bool]]): bool { return true; }
         let wrong = visit([[true]]);",
    );
    let graph = fixture.load();
    let ordinary = analyze_ordinary(&graph, AnalysisLimits::default());
    let report = analyze_generics(&ordinary, AnalysisLimits::default());
    let stats = report.statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    let replay = analyze_generics(&ordinary, exact);
    assert_eq!(replay.statistics(), stats);
    assert!(!replay.conditional_findings().is_empty());

    for work in 0..stats.work {
        let limited = analyze_generics(
            &ordinary,
            AnalysisLimits {
                max_work: work,
                ..exact
            },
        );
        limit::assert_generics_limit(&limited);
    }
    for entries in 0..stats.entries {
        let limited = analyze_generics(
            &ordinary,
            AnalysisLimits {
                max_entries: entries,
                ..exact
            },
        );
        limit::assert_generics_limit(&limited);
    }
    for depth in 0..stats.max_depth {
        let limited = analyze_generics(
            &ordinary,
            AnalysisLimits {
                max_depth: depth,
                ..exact
            },
        );
        limit::assert_generics_limit(&limited);
    }
}
