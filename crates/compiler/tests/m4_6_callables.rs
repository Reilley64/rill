//! M4.6 public queries over real M3 captures, not Rill execution.
use compiler::analysis::generics::{analyze_generics, callables::CaptureForm};
use compiler::analysis::ordinary::analyze_ordinary;
use compiler::analysis::scalars::data::{Access, PathProjection};
use compiler::analysis::scalars::{BodyOwner, NodeFactKind, ScalarFindingKind, ScalarUnsupported};
use compiler::analysis::{AnalysisLimits, InvocationMode, TypeTerm};
use compiler::loader::{Graph, Limits, Module, Prelude, Request, Selector, load};
use compiler::parser::{NodeId, NodeKind};
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

fn check(
    source: &str,
    assert: impl FnOnce(&Graph, &compiler::analysis::generics::GenericReport<'_>),
) {
    let fixture = Fixture::new(source);
    let graph = fixture.load();
    let limits = AnalysisLimits::default();
    let ordinary = analyze_ordinary(&graph, limits);
    let report = analyze_generics(&ordinary, limits);
    assert(&graph, &report);
}

fn app(graph: &Graph) -> &Module {
    graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap()
}

fn last_node(module: &Module, kind: NodeKind) -> NodeId {
    module
        .syntax
        .nodes
        .iter()
        .rposition(|node| node.kind == kind)
        .unwrap()
}

#[test]
fn callback_parameter_default_runs_before_a_result_default_that_the_body_can_replace() {
    check(
        "function f<R = i32, A = bool>(cb: (A) => R): R { while (true) {} }
           let value = f((x) => true);",
        |graph, report| {
            let module = app(graph);
            let node = last_node(module, NodeKind::Call);
            let fact = report
                .node(module, node)
                .unwrap()
                .expect("typed standalone call");
            let NodeFactKind::Expression(expr) = &fact.kind else {
                panic!("call expression")
            };
            assert!(matches!(expr.ty.as_ref(), TypeTerm::Primitive(id) if id.name() == "bool"));
            let call = expr.call.as_ref().unwrap();
            assert_eq!(call.defaults.len(), 1);
            assert_eq!(call.defaults[0].binder, call.substitution[1].binder);
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_some());
        },
    );
}

#[test]
fn mandatory_reads_can_run_through_another_callback() {
    check(
        "function f<A, R = i32>(first: (A) => R, second: (R) => A): R { while (true) {} }
           let value = f((x) => 1i32, (x) => true);",
        |graph, report| {
            let module = app(graph);
            let node = last_node(module, NodeKind::Call);
            let NodeFactKind::Expression(expr) =
                &report.node(module, node).unwrap().expect("typed call").kind
            else {
                panic!("call expression")
            };
            let call = expr.call.as_ref().unwrap();
            assert_eq!(call.defaults.len(), 1);
            assert_eq!(call.defaults[0].binder, call.substitution[1].binder);
            assert!(
                matches!(call.substitution[0].argument.as_ref(), TypeTerm::Primitive(id) if id.name() == "bool")
            );
        },
    );
}

#[test]
fn a_default_waits_for_its_written_dependencies_and_for_other_slot_producers() {
    check(
        "function f<R = i32, A = R>(cb: (A) => R): R { while (true) {} }
           let value = f((x) => true);",
        |graph, report| {
            let module = app(graph);
            let node = last_node(module, NodeKind::Call);
            assert!(report.node(module, node).unwrap().is_none());
            assert!(
                report
                    .findings()
                    .iter()
                    .any(|f| f.kind == ScalarFindingKind::InsufficientContext)
            );
            assert!(
                report
                    .findings()
                    .iter()
                    .all(|f| f.kind != ScalarFindingKind::TypeMismatch)
            );
        },
    );
}

#[test]
fn a_written_default_can_contextualize_arithmetic_through_a_standalone_result() {
    check(
        "function inner<T>(x: T): T { return x; }
           function outer<T = i32>(x: T): T { return x; }
           let value = outer(inner((1 + 2)));",
        |graph, report| {
            let module = app(graph);
            let node = last_node(module, NodeKind::Call);
            let NodeFactKind::Expression(expr) =
                &report.node(module, node).unwrap().expect("typed call").kind
            else {
                panic!("call expression")
            };
            assert!(matches!(expr.ty.as_ref(), TypeTerm::Primitive(id) if id.name() == "i32"));
            assert_eq!(expr.call.as_ref().unwrap().defaults.len(), 1);
        },
    );
}

#[test]
fn a_written_callable_default_supplies_an_otherwise_missing_arrow_shape() {
    check(
        "function useValue<F = (bool) => bool>(x: F): F { return x; }
           let value = useValue((x) => x);",
        |graph, report| {
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let node = module
                .syntax
                .nodes
                .iter()
                .rposition(|n| n.kind == NodeKind::Call)
                .unwrap();
            let NodeFactKind::Expression(expr) =
                &report.node(module, node).unwrap().expect("typed call").kind
            else {
                panic!("call expression")
            };
            assert_eq!(expr.call.as_ref().unwrap().defaults.len(), 1);
            let TypeTerm::Callable {
                parameters, result, ..
            } = expr.ty.as_ref()
            else {
                panic!("callable")
            };
            assert!(matches!(result.as_ref(), TypeTerm::Primitive(id) if id.name() == "bool"));
            assert_eq!(parameters, std::slice::from_ref(result));
        },
    );
}

#[test]
fn block_returns_join_in_the_arrow_not_the_enclosing_function() {
    check(
        "function make(): bool {
             let callback = (x: bool) => { if (x) { return 1i32; } return 2i32; };
             return callback(true) == 1i32;
           }",
        |graph, report| {
            let module = app(graph);
            let make = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("make"))
                .unwrap();
            assert!(
                report
                    .body(module, BodyOwner::Declaration(&make.id))
                    .unwrap()
                    .is_some(),
                "{:?} {:?}",
                report.findings(),
                report.obligations()
            );
        },
    );
}

#[test]
fn a_named_value_default_waits_for_a_ready_sibling_arrow() {
    check(
        "function identity<T = i32>(x: T): T { return x; }
           function apply<T>(make: () => T, map: (T) => T): T { return map(make()); }
           let value = apply(() => true, identity);",
        |graph, report| {
            let module = app(graph);
            let node = last_node(module, NodeKind::Call);
            let NodeFactKind::Expression(expr) =
                &report.node(module, node).unwrap().expect("typed call").kind
            else {
                panic!("call expression")
            };
            assert!(matches!(expr.ty.as_ref(), TypeTerm::Primitive(id) if id.name() == "bool"));
            let named = last_node(module, NodeKind::ValueReference);
            let facts = report
                .node(module, named)
                .unwrap()
                .unwrap()
                .callable
                .as_ref()
                .unwrap();
            assert!(facts.function_value.as_ref().unwrap().defaults.is_empty());
        },
    );
}

#[test]
fn a_standalone_generic_name_forms_a_monomorphic_value_from_its_calling_contract() {
    check(
        "function identity<T>(x: T): T { return x; }
           let f: (bool) => bool = identity;
           let value = f(true);",
        |graph, report| {
            let module = app(graph);
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_some());
        },
    );
}

#[test]
fn capture_mode_comes_from_the_reachable_uses_not_the_receiver_type() {
    check(
        "let mut n = 1i32;
           let f: once () => i32 = () => { n += 1i32; return n; };
           let s = \"owned\";
           let g = () => s;
           let h = () => { if (false) { n += 1i32; } return n; };",
        |graph, report| {
            let module = app(graph);
            let mut modes = Vec::new();
            for (node, _) in module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == NodeKind::Arrow)
            {
                let fact = report.node(module, node).unwrap().expect("typed arrow");
                let NodeFactKind::Expression(expr) = &fact.kind else {
                    panic!("arrow expression")
                };
                let TypeTerm::Callable { mode, .. } = expr.ty.as_ref() else {
                    panic!("callable type")
                };
                modes.push(*mode);
            }
            assert_eq!(
                modes,
                [
                    InvocationMode::Mutable,
                    InvocationMode::Once,
                    InvocationMode::ReadOnly
                ]
            );
        },
    );
}

#[test]
fn explicit_borrow_capture_keeps_its_field_path_and_invokes_through_a_shared_descriptor() {
    check(
        "type Pair = { left: i32; right: i32 };
           let pair = Pair { left: 1i32, right: 2i32 };
           let read: () => &i32 = () => &pair.left;
           let descriptor = &read;
           let first = descriptor();",
        |graph, report| {
            let module = app(graph);
            let node = last_node(module, NodeKind::Arrow);
            let arrow = report
                .arrow(module, node)
                .unwrap()
                .expect("arrow formation");
            assert_eq!(arrow.mode, Some(InvocationMode::ReadOnly));
            let [capture] = arrow.captures.as_slice() else {
                panic!("one stable source")
            };
            assert_eq!(capture.form, CaptureForm::ExplicitBorrow { mutable: false });
            let pair = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("pair"))
                .unwrap();
            let field = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("left"))
                .unwrap();
            assert_eq!(capture.path.root, &pair.id);
            assert_eq!(capture.path.projections, [PathProjection::Field(&field.id)]);
            let call = last_node(module, NodeKind::Call);
            let fact = report.node(module, call).unwrap().unwrap();
            let invocation = fact.callable.as_ref().unwrap().invocation.as_ref().unwrap();
            assert_eq!(invocation.access, Access::ReadOnly);
            assert_eq!(
                invocation.path.as_ref().unwrap().projections,
                [PathProjection::Dereference]
            );
            let receiver = report.node(module, invocation.receiver).unwrap().unwrap();
            assert_eq!(
                receiver
                    .callable
                    .as_ref()
                    .unwrap()
                    .arrow
                    .as_ref()
                    .unwrap()
                    .site
                    .node,
                node
            );
        },
    );
}

#[test]
fn an_unclassified_aggregate_use_keeps_signature_and_sources_without_proving_a_mode() {
    check(
        "type Pair = { left: i32; right: i32 };
           let pair = Pair { left: 1i32, right: 2i32 };
           let unresolved = () => pair;",
        |graph, report| {
            let module = app(graph);
            let node = last_node(module, NodeKind::Arrow);
            let arrow = report
                .arrow(module, node)
                .unwrap()
                .expect("partial arrow fact");
            assert!(matches!(arrow.result.as_ref(), TypeTerm::Nominal { .. }));
            assert!(arrow.mode.is_none());
            assert_eq!(arrow.captures.len(), 1);
            assert!(arrow.captures[0].demand.is_none());
            assert!(
                report
                    .obligations()
                    .iter()
                    .any(|o| o.site.node == node && o.reason == ScalarUnsupported::CaptureMode)
            );
            assert!(report.node(module, node).unwrap().is_none());
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_none());
        },
    );
}

#[test]
fn a_mutable_descriptor_forwards_repeatable_mutable_invocation() {
    check(
        "let mut n = 1i32;
           let mut callback = () => { n += 1i32; return n; };
           let descriptor = &mut callback;
           let value = descriptor();",
        |graph, report| {
            let module = app(graph);
            let call = last_node(module, NodeKind::Call);
            let fact = report
                .node(module, call)
                .unwrap()
                .expect("typed invocation");
            let invocation = fact.callable.as_ref().unwrap().invocation.as_ref().unwrap();
            assert_eq!(invocation.access, Access::Writable);
            assert_eq!(invocation.mode, InvocationMode::Mutable);
        },
    );
}

#[test]
fn borrowed_receivers_do_not_forward_a_consuming_contract() {
    for (reference, body) in [("&", "&f"), ("&mut", "&mut f")] {
        check(
            &format!(
                "let mut f: once () => bool = () => true;
                         let descriptor: {reference}(once () => bool) = {body};
                         let invalid = descriptor();"
            ),
            |graph, report| {
                let module = app(graph);
                let call = last_node(module, NodeKind::Call);
                assert!(
                    report
                        .findings()
                        .iter()
                        .any(|f| f.site.node == call && f.kind == ScalarFindingKind::ReadOnly)
                );
                assert!(report.node(module, call).unwrap().is_none());
            },
        );
    }
}

#[test]
fn an_arrow_can_be_bound_and_invoked_without_a_static_callee() {
    check("let f = () => true; let value = f();", |graph, report| {
        let module = app(graph);
        let node = last_node(module, NodeKind::Call);
        let fact = report
            .node(module, node)
            .unwrap()
            .expect("typed invocation");
        let NodeFactKind::Expression(expr) = &fact.kind else {
            panic!("call expression")
        };
        assert!(matches!(expr.ty.as_ref(), TypeTerm::Primitive(id) if id.name() == "bool"));
        assert!(expr.call.is_none(), "not a declaration call");
        assert!(report.body(module, BodyOwner::Startup).unwrap().is_some());
    });
}
