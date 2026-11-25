//! M4.6 evidence over real M3 captures, not Rill execution.
#[path = "support/limit.rs"]
mod limit;

use compiler::analysis::generics::{GoalBlocker, GoalState, GoalVerdict, analyze_generics};
use compiler::analysis::ordinary::analyze_ordinary;
use compiler::analysis::scalars::BodyOwner;
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
            "rill-m4-6-evidence-{}-{}",
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
    fn write_module(&self, name: &str, source: &str) {
        fs::write(self.0.join("app").join(name), source).unwrap();
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
fn a_selected_clause_determines_open_output_before_checking_equality() {
    let fixture = Fixture::new(
        "function inner<T,E>(value: T): E;
         implement<U> inner(value: U): i32 { return 1i32; }
         function outer<T,E>(value: T): E;
         implement<U,E> outer(value: U): E where inner(U,E) { while (true) {} }
         let pending: bool = outer(true);",
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
    assert!(
        matches!(call.selected_result.as_deref(), Some(TypeTerm::Primitive(id)) if id.name() == "i32"),
        "{call:?}"
    );
    assert!(matches!(
        call.output_agreement,
        compiler::analysis::generics::OutputAgreement::Disagrees { .. }
    ));
    let goal = report.goal(call.goal.as_ref().unwrap()).unwrap();
    assert_eq!(
        goal.state,
        GoalState::Unresolved(GoalBlocker::OrdinaryAdmission)
    );
    assert!(
        goal.dependencies
            .iter()
            .any(|id| report.goal(id).unwrap().source.span.bytes() == b"inner(U,E)")
    );
    assert!(report.node(module, node).unwrap().is_none());
}

/// Ruling 23 (item 1) and C15 item 1: an ordinary call's `OutputAgreement::Disagrees`
/// is a type mismatch (R-code) at the use. The exact run publishes it; no limited run does.
#[test]
fn a_limited_generics_report_publishes_no_output_disagreement() {
    let fixture = Fixture::new(
        "function inner<T,E>(value: T): E;
         implement<U> inner(value: U): i32 { return 1i32; }
         function outer<T,E>(value: T): E;
         implement<U,E> outer(value: U): E where inner(U,E) { while (true) {} }
         let pending: bool = outer(true);",
    );
    let graph = fixture.load();
    let ordinary = analyze_ordinary(&graph, AnalysisLimits::default());
    let exact = analyze_generics(&ordinary, AnalysisLimits::default());
    assert!(
        limit::conversion_use_flaws(&graph, &exact).contains(&"publishes an output disagreement"),
        "the exact run publishes the disagreement"
    );
    let stats = exact.statistics();
    let mut limited = 0;
    for max_work in 0..stats.work {
        let report = analyze_generics(
            &ordinary,
            AnalysisLimits {
                max_work,
                max_entries: stats.entries,
                max_depth: stats.max_depth,
            },
        );
        if report.completion() == compiler::analysis::scalars::ScalarFailureKind::ResourceLimit {
            limited += 1;
            let flaws = limit::conversion_use_flaws(&graph, &report);
            assert!(flaws.is_empty(), "{flaws:?} at {:?}", report.statistics());
            limit::assert_generics_limit(&report);
        }
    }
    assert!(limited > 0);
}

#[test]
fn a_free_source_output_resumes_a_dependency_without_setting_its_output() {
    use compiler::analysis::generics::OutputAgreement;
    let fixture = Fixture::new(
        "function seed<V,W>(value: V): W;
         implement seed(value: bool): i32 { return 1i32; }
         function open<T,V = bool,W = u8>(value: T): [V,W];
         implement<U,V,W> open(value: U): [V,W] where seed(V,W) { while (true) {} }
         let fixed: [bool,i32] = open(true);
         let wrong: [bool,u8] = open(true);
         let defaulted = open(true);",
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
    let calls: Vec<_> = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == NodeKind::Call)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(calls.len(), 3);
    for (i, node) in calls.into_iter().enumerate() {
        let call = report.ordinary_call(module, node).unwrap().unwrap();
        let Some(TypeTerm::Tuple(fields)) = call.selected_result.as_deref() else {
            panic!("selected tuple result after resuming evidence")
        };
        assert_eq!(fields.len(), 2);
        assert!(matches!(fields[0].as_ref(), TypeTerm::Primitive(id) if id.name() == "bool"));
        assert!(matches!(fields[1].as_ref(), TypeTerm::Primitive(id) if id.name() == "i32"));
        assert_eq!(
            matches!(call.output_agreement, OutputAgreement::Agrees),
            i != 1
        );
        if i == 1 {
            assert!(matches!(
                call.output_agreement,
                OutputAgreement::Disagrees { .. }
            ));
        }
        let independent = report.goal(call.goal.as_ref().unwrap()).unwrap();
        assert_eq!(
            independent.state,
            GoalState::Unresolved(GoalBlocker::SelectedOutput)
        );
        assert!(independent.output.is_none());
        let instantiated = report.goal(call.instantiation.as_ref().unwrap()).unwrap();
        use compiler::analysis::generics::{GoalJudgment, GoalVerdict};
        let GoalJudgment::Instantiate {
            determination,
            free,
            ..
        } = &instantiated.judgment
        else {
            panic!("proof at this use")
        };
        assert_eq!(determination, &independent.id);
        assert_eq!(
            free.len(),
            usize::from(i != 2),
            "the pending output must not be bound by the source"
        );
        if let Some(anchor) = free.first() {
            assert!(
                matches!(anchor.argument.as_ref(), TypeTerm::Primitive(id) if id.name() == "bool")
            );
        }
        assert_eq!(
            instantiated.state,
            GoalState::Unresolved(GoalBlocker::OrdinaryAdmission)
        );
        assert_eq!(instantiated.hypothetical, Some(GoalVerdict::Proved));
        assert!(
            instantiated
                .dependencies
                .iter()
                .any(|id| report.goal(id).unwrap().source.span.bytes() == b"seed(V,W)")
        );
        assert_eq!(call.defaults.len(), usize::from(i == 2));
        if i == 2 {
            assert_eq!(call.defaults[0].span.bytes(), b"bool");
        }
        assert!(report.node(module, node).unwrap().is_none());
    }
}

#[test]
fn a_contextual_family_value_resumes_the_same_selected_output_work() {
    use compiler::analysis::generics::OutputAgreement;
    let fixture = Fixture::new(
        "function seed<V,W>(value: V): W;
         implement seed(value: bool): i32 { return 1i32; }
         function open<T,V,W>(value: T): [V,W];
         implement<U,V,W> open(value: U): [V,W] where seed(V,W) { while (true) {} }
         let pending: (bool) => [bool,i32] = open;",
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
        .position(|n| n.kind == NodeKind::ValueReference && n.span.bytes() == b"open")
        .unwrap();
    let value = report.ordinary_value(module, node).unwrap().unwrap();
    assert!(matches!(value.output_agreement, OutputAgreement::Agrees));
    assert!(report.goal(&value.goal).unwrap().output.is_none());
    assert!(report.node(module, node).unwrap().is_none());
}

#[test]
fn a_later_default_can_read_the_output_determined_after_an_earlier_default() {
    let fixture = Fixture::new(
        "function seed<V,W>(value: V): W;
         implement seed(value: bool): i32 { return 1i32; }
         function open<T,V = bool,W = u8,K = W>(value: T): [V,W,K];
         implement<U,V,W,K> open(value: U): [V,W,K] where seed(V,W) { while (true) {} }
         let pending = open(true);",
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
    let Some(TypeTerm::Tuple(fields)) = call.selected_result.as_deref() else {
        panic!("default, determination and then dependent default")
    };
    assert_eq!(fields.len(), 3);
    for (field, expected) in fields.iter().zip(["bool", "i32", "i32"]) {
        assert!(matches!(field.as_ref(), TypeTerm::Primitive(id) if id.name() == expected));
    }
    assert_eq!(
        call.defaults
            .iter()
            .map(|d| d.span.bytes())
            .collect::<Vec<_>>(),
        [&b"bool"[..], &b"W"[..]]
    );
    assert!(report.node(module, node).unwrap().is_none());
}

#[test]
fn a_compound_written_default_cannot_partially_bind_a_selected_output() {
    let fixture = Fixture::new(
        "function seed<V,W>(value: V): W;
         implement seed(value: bool): i32 { return 1i32; }
         function open<T,S = [bool,u8]>(value: T): S;
         implement<U,V,W> open(value: U): [V,W] where seed(V,W) { while (true) {} }
         let pending = open(true);",
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
    assert!(call.defaults.is_empty() && call.selected_result.is_none());
    let use_goal = report.goal(call.instantiation.as_ref().unwrap()).unwrap();
    assert!(use_goal.output.is_none() && use_goal.hypothetical.is_none());
}

#[test]
fn a_cycle_does_not_prevent_an_independent_waiting_output_from_resuming() {
    let fixture = Fixture::new(
        "function seed<V,W>(value: V): W;
         implement seed(value: bool): i32 { return 1i32; }
         function cycling<T,K>(value: T): K;
         implement<T,K> cycling(value: T): K where cycling(T,K) { while (true) {} }
         function open<T,V,W,K>(value: T): [V,W,K];
         implement<U,V,W,K> open(value: U): [V,W,K] where cycling(U,K), seed(V,W) { while (true) {} }
         let pending: [bool,i32,u8] = open(true);",
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
    let use_goal = report.goal(call.instantiation.as_ref().unwrap()).unwrap();
    assert_eq!(use_goal.state, GoalState::Unresolved(GoalBlocker::Cycle));
    assert!(use_goal.output.is_none() && use_goal.hypothetical.is_none());
    let seed = use_goal
        .dependencies
        .iter()
        .map(|id| report.goal(id).unwrap())
        .find(|g| g.source.span.bytes() == b"seed(V,W)")
        .unwrap();
    assert!(seed.output.is_some());
    assert!(report.node(module, node).unwrap().is_none());
}

#[test]
fn a_settled_nonmatching_symbolic_argument_is_not_pending() {
    use compiler::analysis::scalars::{ScalarFindingKind, ScalarUnsupported};
    let fixture = Fixture::new(
        "function run<F>(cb: F, value: bool): bool
             where callable(F,[i32],bool) { return cb(value); }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    assert!(
        report
            .findings()
            .iter()
            .any(|f| f.kind == ScalarFindingKind::TypeMismatch
                && f.site.span.bytes() == b"cb(value)")
    );
    assert!(
        !report
            .obligations()
            .iter()
            .any(|o| o.reason == ScalarUnsupported::CallableInvocation)
    );
}

#[test]
fn an_output_producing_cycle_retains_the_active_edge_without_guessing_a_result() {
    let fixture = Fixture::new(
        "function read<T,E>(value: T): E;
         implement<U,E> read(value: U): E where read(U,E) { while (true) {} }
         let pending: bool = read<bool>(true);",
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
    assert!(call.selected_result.is_none());
    let goal = report.goal(call.goal.as_ref().unwrap()).unwrap();
    assert_eq!(goal.state, GoalState::Unresolved(GoalBlocker::Cycle));
    let back_edge = report.goal(&goal.dependencies[0]).unwrap();
    assert_eq!(back_edge.cycle, [goal.id.clone(), back_edge.id.clone()]);
    assert!(goal.hypothetical.is_none());
    assert!(report.node(module, node).unwrap().is_none());
}

#[test]
fn equal_callable_signatures_do_not_merge_different_capture_origins() {
    let fixture = Fixture::new(
        "function keep<T>(value: T): T where noBorrow(T) { return value; }
         function probe(value: i32): void {
             let safe = keep(() => 1i32);
             let borrowed = keep(() => { let seen = &value; return 1i32; });
         }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let direct: Vec<_> = report
        .goals()
        .iter()
        .filter(|goal| {
            goal.site.span.bytes().starts_with(b"keep(")
                && goal.source.span.bytes() == b"noBorrow(T)"
        })
        .collect();
    assert_eq!(direct.len(), 2);
    assert_eq!(direct[0].state, GoalState::Proved);
    assert_eq!(direct[1].state, GoalState::Disproved);
    let safe = direct[0].arrow_origin.as_ref().unwrap();
    let borrowed = direct[1].arrow_origin.as_ref().unwrap();
    assert!(safe.captures.is_empty());
    assert!(
        graph
            .modules()
            .iter()
            .flat_map(|m| &m.declarations)
            .any(|d| d.id == *borrowed.captures[0].path.root && d.name.as_deref() == Some("value"))
    );
    assert_eq!(direct[1].dependencies.len(), 1);
}

#[test]
fn callable_assumption_types_rigid_invocation_and_flexible_arrow() {
    let fixture = Fixture::new(
        "function run<F,R>(cb: F): R where callable(F,[i32],R) { return cb(1); }
         let checked = run((x) => x == 1i32);",
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
    assert!(
        report.body(module, BodyOwner::Startup).unwrap().is_some(),
        "{:?}",
        report.obligations()
    );
    let run = module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("run"))
        .unwrap();
    assert!(
        report
            .body(module, BodyOwner::Declaration(&run.id))
            .unwrap()
            .is_some()
    );
    let (node, _) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == NodeKind::Call && n.span.bytes() == b"cb(1)")
        .unwrap();
    let invocation = report
        .node(module, node)
        .unwrap()
        .unwrap()
        .callable
        .as_ref()
        .unwrap()
        .invocation
        .as_ref()
        .unwrap();
    let assumed = report
        .goal(invocation.assumption.as_ref().unwrap())
        .unwrap();
    assert_eq!(assumed.state, GoalState::Assumption);
    assert_eq!(assumed.source.span.bytes(), b"callable(F,[i32],R)");
}

#[test]
fn same_arity_callable_assumptions_match_the_whole_argument_vector() {
    let fixture = Fixture::new(
        "function run<F>(cb: F): void
             where callable(F,[i32,bool],bool), callable(F,[u8,u8],i32) {
             let flag: bool = cb(1, true);
             let count: i32 = cb(2, 3u8);
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
    let run = module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("run"))
        .unwrap();
    assert!(
        report
            .body(module, BodyOwner::Declaration(&run.id))
            .unwrap()
            .is_some()
    );
    for (call, source) in [
        (&b"cb(1, true)"[..], &b"callable(F,[i32,bool],bool)"[..]),
        (&b"cb(2, 3u8)"[..], &b"callable(F,[u8,u8],i32)"[..]),
    ] {
        let node = module
            .syntax
            .nodes
            .iter()
            .position(|n| n.kind == NodeKind::Call && n.span.bytes() == call)
            .unwrap();
        let invocation = report
            .node(module, node)
            .unwrap()
            .unwrap()
            .callable
            .as_ref()
            .unwrap()
            .invocation
            .as_ref()
            .unwrap();
        assert_eq!(
            report
                .goal(invocation.assumption.as_ref().unwrap())
                .unwrap()
                .source
                .span
                .bytes(),
            source
        );
    }
}

#[test]
fn an_expected_result_does_not_choose_a_symbolic_callable_input() {
    use compiler::analysis::scalars::ScalarUnsupported;
    let fixture = Fixture::new(
        "function run<F>(cb: F): bool
             where callable(F,[i32],bool), callable(F,[u8],i32) { return cb(1); }",
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
    assert!(report.node(module, node).unwrap().is_none());
    assert!(report.findings().is_empty());
    assert!(
        report
            .obligations()
            .iter()
            .any(|o| o.site.node == node && o.reason == ScalarUnsupported::CallableInvocation)
    );
}

#[test]
fn a_callable_clause_blocks_a_result_default_until_its_arrow_is_ready() {
    let fixture = Fixture::new(
        "function run<F,R = i32,A = bool>(cb: F): R where callable(F,[A],R) { while (true) {} }
         let inferred = run((x) => true);",
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
    let compiler::analysis::scalars::NodeFactKind::Expression(fact) = &report
        .node(module, node)
        .unwrap()
        .expect("checked call")
        .kind
    else {
        panic!("call")
    };
    assert!(matches!(fact.ty.as_ref(), TypeTerm::Primitive(id) if id.name() == "bool"));
    let defaults = &fact.call.as_ref().unwrap().defaults;
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].span.bytes(), b"bool");
}

#[test]
fn projecting_a_scalar_keeps_the_captured_reference_dependency() {
    let fixture = Fixture::new(
        "type Cell = { value: i32 };
         function keep<T>(value: T): T where noBorrow(T) { return value; }
         function probe(cell: Cell): void {
             let r = &cell;
             let bad = keep(() => r.value == 1i32);
         }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let goal = report
        .goals()
        .iter()
        .find(|goal| goal.arrow_origin.is_some() && goal.source.span.bytes() == b"noBorrow(T)")
        .unwrap();
    assert_eq!(goal.state, GoalState::Disproved);
    let dependency = report.goal(&goal.dependencies[0]).unwrap();
    assert!(
        matches!(&dependency.judgment, compiler::analysis::generics::GoalJudgment::Requirement { arguments, .. } if matches!(arguments[0].as_ref(), TypeTerm::Reference { .. }))
    );
}

#[test]
fn scalar_and_written_predicate_evidence_check_a_standalone_call() {
    let fixture = Fixture::new(
        "function keep<T>(x: T): T where noBorrow(T) { return x; }
         function relay<T>(x: T): T where noBorrow(T) { return keep(x); }
         let b = keep(true);",
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
    for name in ["keep", "relay"] {
        let declaration = module
            .declarations
            .iter()
            .find(|d| d.name.as_deref() == Some(name))
            .unwrap();
        assert!(
            report
                .body(module, BodyOwner::Declaration(&declaration.id))
                .unwrap()
                .is_some(),
            "{report:#?}"
        );
    }
    for (node, _) in module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == NodeKind::Call)
    {
        let fact = report.node(module, node).unwrap().expect("checked call");
        let compiler::analysis::scalars::NodeFactKind::Expression(expression) = &fact.kind else {
            panic!("call expression")
        };
        assert!(matches!(
            expression.ty.as_ref(),
            TypeTerm::Parameter { .. } | TypeTerm::Primitive(_)
        ));
    }
    assert!(report.body(module, BodyOwner::Startup).unwrap().is_some());

    let proved: Vec<_> = report
        .goals()
        .iter()
        .filter(|g| g.state == GoalState::Proved)
        .collect();
    assert_eq!(proved.len(), 2);
    let assumption = report.goal(&proved[0].dependencies[0]).unwrap();
    assert_eq!(assumption.state, GoalState::Assumption);
    assert_eq!(assumption.source.span.bytes(), b"noBorrow(T)");
    assert_eq!(proved[0].source.span.bytes(), b"noBorrow(T)");
    assert_eq!(proved[0].site.span.bytes(), b"keep(x)");
    assert!(proved[1].assumptions.is_empty());
    let other = fixture.load();
    let other = analyze_ordinary(&other, limits);
    let other = analyze_generics(&other, limits);
    assert_eq!(
        other.goal(&assumption.id).unwrap_err(),
        compiler::analysis::QueryError::InvalidRequest
    );
}

#[test]
fn selected_requirement_cannot_seed_its_own_conditional_proof() {
    let fixture = Fixture::new(
        "function read<R,E>(x: R): E;
         implement<T> read(x: T): bool where read(T, bool) { return true; }
         function tryRead(x: i32): bool where read(i32, bool) { return true; }
         function keep<T>(x: T): T where noBorrow(T) { return x; }
         let unread = tryRead(1i32);
         let independent = keep(true);",
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
    let cycle = report
        .goals()
        .iter()
        .find(|g| !g.cycle.is_empty())
        .expect("closing cycle witness");
    assert_eq!(cycle.state, GoalState::Unresolved(GoalBlocker::Cycle));
    assert!(
        cycle.assumptions.is_empty(),
        "selected requirements are dependencies"
    );
    assert_eq!(cycle.source.span.bytes(), b"read(T, bool)");
    for id in &cycle.cycle {
        assert!(report.goal(id).unwrap().premise.is_some());
    }
    let (call, _) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == NodeKind::Call)
        .unwrap();
    assert!(report.node(module, call).unwrap().is_none());
    let independent = report
        .goals()
        .iter()
        .find(|g| g.site.span.bytes() == b"keep(true)")
        .expect("independent source after the blocked call");
    assert_eq!(independent.state, GoalState::Proved);
    assert!(
        report
            .node(module, independent.site.node)
            .unwrap()
            .is_some()
    );
    assert!(
        report.findings().is_empty(),
        "no settled absence under hypothetical role"
    );
}

#[test]
fn conditional_ordinary_proof_compares_the_selected_error_argument_without_fallback() {
    let fixture = Fixture::new(
        "function read<R,E>(x: R): E where shareable(R);
         implement<T> read(x: T): bool { return true; }
         implement read(x: i32): string where sendable(i32) { return \"selected\"; }
         implement read(x: i64): bool where noBorrow(&i64) { return true; }
         function check<R,E>(x: R): bool where read(R,E) { return true; }
         function agrees(): bool { return check<i32,string>(1i32); }
         function mismatch(): bool { return check<i32,bool>(1i32); }
         function fails(): bool { return check<i64,bool>(1i64); }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    for (call, verdict, output_agrees, winner, selected_goal, output) in [
        (
            b"check<i32,string>(1i32)".as_slice(),
            GoalVerdict::Proved,
            true,
            1,
            b"sendable(i32)".as_slice(),
            "string",
        ),
        (
            b"check<i32,bool>(1i32)".as_slice(),
            GoalVerdict::Disproved,
            false,
            1,
            b"sendable(i32)".as_slice(),
            "string",
        ),
        (
            b"check<i64,bool>(1i64)".as_slice(),
            GoalVerdict::Disproved,
            true,
            2,
            b"noBorrow(&i64)".as_slice(),
            "bool",
        ),
    ] {
        let proof = report
            .goals()
            .iter()
            .find(|g| g.site.span.bytes() == call && !g.equalities.is_empty())
            .expect("exact proof attempt");
        assert_eq!(
            proof.state,
            GoalState::Unresolved(GoalBlocker::OrdinaryAdmission)
        );
        assert_eq!(proof.hypothetical, Some(verdict));
        let equality = proof
            .equalities
            .iter()
            .find(|e| e.position == Some(1))
            .unwrap();
        assert_eq!(equality.agrees, output_agrees);
        assert_eq!(equality.requested_site.span.bytes(), b"E");
        assert!(matches!(equality.actual.as_ref(), TypeTerm::Primitive(id) if id.name() == output));
        let determination = report.goal(&proof.dependencies[0]).unwrap();
        let selected = determination.selection.as_ref().expect("ordinary winner");
        assert_eq!(
            *selected.implementation,
            graph.implementations()[winner].declaration
        );
        let sources: Vec<_> = determination
            .dependencies
            .iter()
            .map(|id| report.goal(id).unwrap().source.span.bytes())
            .collect();
        assert_eq!(sources, [b"shareable(R)".as_slice(), selected_goal]);
    }
    assert!(report.findings().is_empty());
}

#[test]
fn an_exact_symbolic_assumption_determines_the_output_without_a_concrete_winner() {
    let fixture = Fixture::new(
        "function read<R,E>(x: R): E;
         implement<T> read(x: T): bool { return true; }
         function check<R,E>(x: R): bool where read(R,E) { return true; }
         function compare<T,E,F>(x: T): bool where read(T,E) { return check<T,F>(x); }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let proof = report
        .goals()
        .iter()
        .find(|g| g.site.span.bytes() == b"check<T,F>(x)" && !g.equalities.is_empty())
        .expect("assumption-derived symbolic equality");
    assert_eq!(proof.hypothetical, Some(GoalVerdict::Disproved));
    let determination = report.goal(&proof.dependencies[0]).unwrap();
    assert!(determination.selection.is_none());
    let assumption = report.goal(&determination.dependencies[0]).unwrap();
    assert_eq!(assumption.state, GoalState::Assumption);
    assert_eq!(assumption.source.span.bytes(), b"read(T,E)");
    assert!(
        !proof
            .equalities
            .iter()
            .find(|e| e.position == Some(1))
            .unwrap()
            .agrees
    );
}

#[test]
fn type_growth_exhausts_a_limit_instead_of_reporting_a_cycle_or_absence() {
    let fixture = Fixture::new(
        "function read<R,E>(x: R): E;
         implement<T> read(x: T): bool where read([T], bool) { return true; }
         function tryRead(x: i32): bool where read(i32, bool) { return true; }
         let unread = tryRead(1i32);",
    );
    let graph = fixture.load();
    let ordinary = analyze_ordinary(&graph, AnalysisLimits::default());
    let limits = AnalysisLimits {
        max_depth: 10,
        ..AnalysisLimits::default()
    };
    let report = analyze_generics(&ordinary, limits);
    limit::assert_generics_limit(&report);
    assert_eq!(report.statistics().max_depth, limits.max_depth);
    let determinations: Vec<_> = report
        .goals()
        .iter()
        .filter(|g| {
            matches!(
                g.judgment,
                compiler::analysis::generics::GoalJudgment::Determine { .. }
            )
        })
        .collect();
    assert!(determinations.len() >= 4);
    assert!(
        determinations.last().unwrap().output.is_some(),
        "known output retained before the next dependency exhausts depth"
    );
    assert!(
        report
            .goals()
            .iter()
            .all(|g| g.cycle.is_empty() && g.hypothetical.is_none())
    );
    assert!(report.findings().is_empty());
}

#[test]
fn structural_predicates_use_substituted_fields_and_the_actual_reference_contract() {
    let fixture = Fixture::new(
        "type Stored<T,P> = { member: T };
         function owned<T>(x: T): bool where noBorrow(T) { return true; }
         function transfer<T>(x: T): bool where sendable(T) { return true; }
         function share<T>(x: T): bool where shareable(T) { return true; }
         function phantom<T>(x: Stored<bool,&T>): bool { return owned(x); }
         function borrowed(x: Stored<&bool,bool>): bool { return owned(x); }
         function fromShared<T>(x: &T): bool where shareable(T) { return transfer(x); }
         function fromExclusive<T>(x: &mut T): bool where sendable(T) { return transfer(x); }
         function sharedExclusive<T>(x: &mut T): bool where shareable(T) { return share(x); }
         function missing<T>(x: &T): bool where sendable(T) { return transfer(x); }",
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
    for (name, checked) in [
        ("phantom", true),
        ("borrowed", false),
        ("fromShared", true),
        ("fromExclusive", true),
        ("sharedExclusive", true),
        ("missing", false),
    ] {
        let declaration = module
            .declarations
            .iter()
            .find(|d| d.name.as_deref() == Some(name))
            .unwrap();
        assert_eq!(
            report
                .body(module, BodyOwner::Declaration(&declaration.id))
                .unwrap()
                .is_some(),
            checked,
            "{name}: {:#?}",
            report.findings()
        );
    }
    let field = report
        .goals()
        .iter()
        .find(|g| g.state == GoalState::Disproved && g.source.span.bytes() == b"member: T")
        .expect("borrow retained at the field source");
    assert_eq!(field.site.span.bytes(), b"owned(x)");
    assert_eq!(
        report
            .findings()
            .iter()
            .map(|f| f.kind.code())
            .collect::<Vec<_>>(),
        [Some("R0302"), Some("R0302")]
    );
}

#[test]
fn implementation_assumptions_substitute_family_binders_before_adding_local_requirements() {
    let fixture = Fixture::new(
        "function own<T>(x: T): bool where noBorrow(T) { return true; }
         function derive<T>(x: T): bool where noBorrow(T);
         implement<U> derive(x: U): bool where sendable(U) { return own(x); }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let proof = report
        .goals()
        .iter()
        .find(|g| g.site.span.bytes() == b"own(x)")
        .expect("checked implementation expression");
    assert_eq!(proof.state, GoalState::Proved);
    let assume = report.goal(&proof.dependencies[0]).unwrap();
    assert_eq!(assume.source.span.bytes(), b"noBorrow(T)");
    assert_eq!(proof.assumptions.len(), 2);
    let local = report.goal(&proof.assumptions[1]).unwrap();
    assert_eq!(local.source.span.bytes(), b"sendable(U)");
    let compiler::analysis::generics::GoalJudgment::Requirement { arguments, .. } =
        &assume.judgment
    else {
        panic!("family assumption after substitution")
    };
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let binder = module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("U"))
        .unwrap();
    assert!(
        matches!(arguments[0].as_ref(), TypeTerm::Parameter { declaration, .. } if **declaration == binder.id)
    );
    assert!(report.node(module, proof.site.node).unwrap().is_some());
    assert!(
        report
            .body(
                module,
                BodyOwner::Declaration(&graph.implementations()[0].declaration)
            )
            .unwrap()
            .is_none(),
        "ordinary role is pending"
    );
}

#[test]
fn callable_predicates_preserve_exact_result_and_forwarded_mode_without_capture_proof() {
    let fixture = Fixture::new(
        "function onceUse<F>(f: F): bool where callableOnce(F, [i32], bool) { return true; }
         function needsMutable<F>(f: F): bool where callableMut(F, [i32], bool) { return true; }
         function owned<F>(f: F): bool where noBorrow(F) { return true; }
         function shared(f: &((i32) => bool)): bool { return onceUse(f); }
         function sharedMutable(f: &(mut (i32) => bool)): bool { return needsMutable(f); }
         function exclusiveMutable(f: &mut (mut (i32) => bool)): bool { return needsMutable(f); }
         function exclusiveOnce(f: &mut (once (i32) => bool)): bool { return onceUse(f); }
         function wrongResult(f: (i32) => i32): bool { return onceUse(f); }
         function erased(f: (i32) => bool): bool { return owned(f); }",
    );
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let states: Vec<_> = report
        .goals()
        .iter()
        .filter(|g| g.state != GoalState::Assumption)
        .map(|g| g.state)
        .collect();
    assert_eq!(
        states,
        [
            GoalState::Proved,
            GoalState::Disproved,
            GoalState::Proved,
            GoalState::Disproved,
            GoalState::Disproved,
            GoalState::Unresolved(GoalBlocker::CallableEnvironment)
        ]
    );
}

#[test]
fn imported_requirement_metadata_retains_its_declaration_route() {
    let fixture = Fixture::new("import helper::{check}; let value = check(true);");
    fixture.write_module("helper.rill", "import relations::{witness as bound}; export function check<T>(x:T): bool where bound(T) { return true; }");
    fixture.write_module("relations.rill", "export function witness<T>(x:T): bool;");
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    let proof = report
        .goals()
        .iter()
        .find(|g| g.site.span.bytes() == b"check(true)" && !g.routes.is_empty())
        .expect("captured import route");
    assert_eq!(proof.site.module.path, "main.rill");
    assert_eq!(proof.source.module.path, "helper.rill");
    assert_eq!(proof.source.span.bytes(), b"bound(T)");
    assert_eq!(proof.routes[0].name, "bound");
    let compiler::analysis::generics::GoalJudgment::Requirement { target, .. } = proof.judgment
    else {
        panic!("ordinary goal")
    };
    assert!(
        matches!(target, compiler::analysis::ordinary::RequirementTarget::Family(id) if id.module.path == "relations.rill")
    );
    assert!(proof.premise.is_some());
    assert_ne!(proof.state, GoalState::Disproved);
}

#[test]
fn retained_goal_work_and_entries_have_independent_exact_boundaries() {
    let fixture = Fixture::new(
        "function keep<T>(x:T): T where noBorrow(T), sendable(T) { return x; }
         let b = keep(true);",
    );
    let graph = fixture.load();
    let ordinary = analyze_ordinary(&graph, AnalysisLimits::default());
    let report = analyze_generics(&ordinary, AnalysisLimits::default());
    let statistics = report.statistics();
    let exact = AnalysisLimits {
        max_work: statistics.work,
        max_entries: statistics.entries,
        max_depth: statistics.max_depth,
    };
    let measured = analyze_generics(&ordinary, exact);
    assert_eq!(measured.statistics(), statistics);
    assert_eq!(
        measured
            .goals()
            .iter()
            .filter(|g| g.state == GoalState::Proved)
            .count(),
        2
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
    ] {
        let limited = analyze_generics(&ordinary, short);
        limit::assert_generics_limit(&limited);
    }
}

/// Metering rule 8: a `ResourceLimit` generics report publishes no finding in
/// any list (body, conditional, declaration issue, annotation). The exact cap
/// publishes them; every cap below it, on each axis, publishes none.
#[test]
fn resource_limit_publishes_no_finding_list() {
    let sources = [
        "type Gone = { x: Missing }; type Loop = { a: Loop }; let v: i32 = unknown;
         function g<T = Absent>(x: Nope): T;",
        "function f<T>(x: Missing<T>, y: Other): T; function k<T = Zed>(x: T): T;",
    ];
    for (n, source) in sources.into_iter().enumerate() {
        let fixture = Fixture::new(source);
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
        assert!(replay.annotations().findings().count() >= 2, "source {n}");
        assert_eq!(
            replay.declaration_issues().len(),
            usize::from(n == 0),
            "source {n}"
        );

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
}

/// Withholding the issues of a `ResourceLimit` ordinary report moves no cap:
/// `analyze_generics` still meters them, so its statistics on that input are
/// the ones measured before rule 8 reached `analyze_ordinary` (402 work gave
/// 404 work and 92 entries then, 2 more than the ordinary report's own).
#[test]
fn withheld_ordinary_issues_are_still_metered_by_generics() {
    let fixture = Fixture::new(
        "type Gone = { x: Missing }; type Loop = { a: Loop }; let v: i32 = unknown;
         function g<T = Absent>(x: Nope): T;",
    );
    let graph = fixture.load();
    let ordinary = analyze_ordinary(
        &graph,
        AnalysisLimits {
            max_work: 417,
            ..AnalysisLimits::default()
        },
    );
    assert_eq!(
        (ordinary.statistics().work, ordinary.statistics().entries),
        (417, 105)
    );
    assert!(ordinary.declaration_issues().is_empty());
    let report = analyze_generics(&ordinary, AnalysisLimits::default());
    limit::assert_generics_limit(&report);
    let statistics = report.statistics();
    assert_eq!(
        (statistics.work, statistics.entries, statistics.max_depth),
        (419, 107, 2)
    );
}
