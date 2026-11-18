//! Bounded M4.8 checks over real M3 captures. Prelude selectors do not admit core providers.
use compiler::analysis::data::{DataExpression, PathProjection};
use compiler::analysis::scalars::{BodyOwner, NodeFactKind, ScalarFindingKind, ScalarUnsupported};
use compiler::analysis::typed::{CoreBlocker, PublicInterfaceKind, TypedCompletion, analyze_typed};
use compiler::analysis::{AnalysisLimits, InvocationMode, QueryError, Stage, TypeTerm};
use compiler::loader::{DeclarationKind, Graph, Limits, Prelude, Request, Selector, load};
use compiler::parser::NodeKind;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn public_payload_defaults_and_requirements_use_origin_visibility() {
    let fixture = Fixture::new(
        "type Hidden = { value: bool };\n\
         export enum Bad { Tag([Hidden,bool]) }\n\
         export type Box<T = Hidden> = { value: T };\n\
         function privateFamily<T>(x: T): bool;\n\
         export function bad<T>(x: T): bool where privateFamily(T), noBorrow(T);\n\
         export function opaque(x: Hidden): Hidden { return x; }",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let findings = report.public_interface_findings();
    assert_eq!(findings.len(), 3);
    for kind in [
        PublicInterfaceKind::Payload,
        PublicInterfaceKind::Default,
        PublicInterfaceKind::Requirement,
    ] {
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.kind == kind && f.code() == "R0401")
                .count(),
            1
        );
    }
    let hidden = findings
        .iter()
        .find(|f| f.kind == PublicInterfaceKind::Payload)
        .unwrap();
    assert_eq!(hidden.target.name.as_deref(), Some("Hidden"));
    assert_eq!(hidden.site.span.bytes(), b"[Hidden,bool]");
}

#[test]
fn missing_core_does_not_hide_independent_errors_inside_or_after_operations() {
    let fixture = Fixture::new(
        "type Result = { value: bool };\n\
         function probe(r: Result): void {\n\
           guard r else { return; }\n\
           guard let [x,y] = r else (failure) { let seen = failure; let a: bool = 1i32; return; }\n\
           let seen = x;\n\
           for (let item of r) { let seen = item; let b: bool = 2i32; continue; }\n\
           let text = `value ${unknown}`;\n\
           let c: bool = 3i32;\n\
         }\n\
         expect (false?);",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let kinds: Vec<_> = report.bodies().findings().iter().map(|f| f.kind).collect();
    assert_eq!(
        kinds
            .iter()
            .filter(|&&kind| kind == ScalarFindingKind::TypeMismatch)
            .count(),
        3
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|&&kind| kind == ScalarFindingKind::MissingValue)
            .count(),
        1
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|&&kind| kind == ScalarFindingKind::InvalidTarget)
            .count(),
        1
    );
    assert!(!kinds.contains(&ScalarFindingKind::InsufficientContext));
    let core: Vec<_> = report
        .bodies()
        .obligations()
        .iter()
        .filter(|o| o.reason == ScalarUnsupported::CoreAdmission)
        .map(|o| o.syntax)
        .collect();
    assert_eq!(core, [NodeKind::Guard, NodeKind::Guard, NodeKind::For]);
}

#[test]
fn pending_match_input_does_not_hide_independent_arm_checks() {
    let fixture = Fixture::new(
        "function probe(): bool { return match ([true]) { [item] if 1i32 => { let seen = item; let bad: bool = 2i32; return true; }, _ => missing }; }",
    );
    for mode in [false, true] {
        let graph = fixture.load(mode);
        let report = analyze_typed(&graph, AnalysisLimits::default());
        let findings = report.bodies().findings();
        let observed: Vec<_> = findings
            .iter()
            .map(|f| (f.kind, f.site.span.bytes()))
            .collect();
        assert_eq!(
            observed,
            [
                (ScalarFindingKind::TypeMismatch, &b"1i32"[..]),
                (ScalarFindingKind::TypeMismatch, &b"2i32"[..]),
                (ScalarFindingKind::MissingValue, &b"missing"[..]),
            ]
        );
        assert!(
            report
                .bodies()
                .obligations()
                .iter()
                .any(|o| o.reason == ScalarUnsupported::CoreAdmission)
        );
    }
}

#[test]
fn pending_match_result_does_not_diagnose_unsettled_numeric_context() {
    let fixture = Fixture::new("let x = match ([true]) { _ if false => 1, _ => 2i32 };");
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert!(report.bodies().findings().is_empty());
    assert!(
        report
            .bodies()
            .obligations()
            .iter()
            .any(|o| o.reason == ScalarUnsupported::MatchCoverage)
    );
}

#[test]
fn pending_borrowed_binder_preserves_known_siblings_and_nominal_view() {
    let fixture = Fixture::new(
        "type Point = { x: bool }; enum E { A { flag: i32; point: Point; ok: bool }, B } function probe(value: &E): bool { return match (value) { E.A { flag, point } if flag => value.ok, _ => true }; }",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let findings = report.bodies().findings();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].kind, ScalarFindingKind::TypeMismatch);
    assert_eq!(findings[0].site.span.bytes(), b"flag");
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    for name in ["flag", "point"] {
        let binder = module
            .declarations
            .iter()
            .find(|d| d.id.kind == DeclarationKind::Binding && d.name.as_deref() == Some(name))
            .unwrap();
        let NodeFactKind::Binding(facts) = &report
            .bodies()
            .node(module, binder.node)
            .unwrap()
            .unwrap()
            .kind
        else {
            panic!("binding facts for {name}")
        };
        assert_eq!(facts.declaration, &binder.id);
        assert_eq!(facts.ty.is_some(), name == "flag");
        assert_eq!(facts.blocked_by.is_some(), name == "point");
    }
    let match_node = module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Match)
        .unwrap();
    assert!(report.bodies().node(module, match_node).unwrap().is_none());
    let field = module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Field && n.span.bytes() == b"value.ok")
        .unwrap();
    let NodeFactKind::Expression(field) =
        &report.bodies().node(module, field).unwrap().unwrap().kind
    else {
        panic!("field projection")
    };
    assert!(matches!(field.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"));
}

#[test]
fn invalid_payload_does_not_hide_private_siblings_or_unrelated_bodies() {
    let fixture = Fixture::new(
        "type Hidden = { x: bool }; export enum E { A([Hidden,Missing]), B(Hidden), C(Missing) } function bad(): bool { function invalid(e: E): void { } return 1i32; }",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let spans: Vec<_> = report
        .public_interface_findings()
        .iter()
        .map(|f| f.site.span.bytes())
        .collect();
    assert_eq!(spans, [b"Hidden", b"Hidden"]);
    assert!(
        report
            .bodies()
            .findings()
            .iter()
            .any(|f| f.kind == ScalarFindingKind::TypeMismatch)
    );
}

#[test]
fn blocked_destructuring_and_guard_do_not_hide_later_errors_or_prove_fallthrough() {
    let fixture = Fixture::new(
        "function halt<T>(x: T): never; function probe(): void { let [a] = [true]; let knownLater: bool = 1i32; let pending = a; guard true else { halt(1i32); } guard true else { a; } guard true else { if (false) { a; } } guard true else { let normal = true; } } let [b] = [false]; let alsoWrong: bool = 2i32; let pending = b;",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let findings = report.bodies().findings();
    assert_eq!(
        findings.iter().map(|f| f.kind).collect::<Vec<_>>(),
        [
            ScalarFindingKind::TypeMismatch,
            ScalarFindingKind::GuardFallthrough,
            ScalarFindingKind::GuardFallthrough,
            ScalarFindingKind::TypeMismatch
        ]
    );
}

#[test]
fn pipe_uses_callable_values_and_checks_arguments_before_propagation() {
    let fixture = Fixture::new(
        "type Result = { x: i32 }; function sink(x: i32): i32 { return x; } function probe(f: (i32) => i32): i32 { return 1 |> f; } function bad(): void { true |> sink?; false?; } function pending(r: Result): Result { let flag: bool = 3 |> sink?; return r; }",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let pipe = module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Pipe)
        .unwrap();
    let facts = report.bodies().node(module, pipe).unwrap().unwrap();
    assert!(facts.callable.as_ref().unwrap().invocation.is_some());
    let expected = [
        (ScalarFindingKind::TypeMismatch, &b"true"[..]),
        (ScalarFindingKind::InvalidTarget, &b"sink?"[..]),
        (ScalarFindingKind::InvalidTarget, &b"false?"[..]),
    ];
    for (kind, span) in expected {
        assert!(
            report
                .bodies()
                .findings()
                .iter()
                .any(|f| f.kind == kind && f.site.span.bytes() == span),
            "missing {kind:?} at {span:?}"
        );
    }
    assert_eq!(report.bodies().findings().len(), expected.len());
    assert!(
        report
            .bodies()
            .obligations()
            .iter()
            .any(|o| o.reason == ScalarUnsupported::CoreAdmission
                && o.syntax == NodeKind::PipeStep { propagate: true })
    );
}

#[test]
fn a_nested_named_function_shadows_an_outer_callable_value() {
    let fixture = Fixture::new(
        "function probe(f: (bool) => bool): i32 { { function f<T>(x: T): T { return x; } return f(1i32); } }",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let call = module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Call)
        .unwrap();
    let NodeFactKind::Expression(fact) = &report.bodies().node(module, call).unwrap().unwrap().kind
    else {
        panic!("call expression")
    };
    let nested = module
        .declarations
        .iter()
        .find(|d| d.id.kind == DeclarationKind::Function && d.name.as_deref() == Some("f"))
        .unwrap();
    assert_eq!(fact.call.as_ref().unwrap().callee, &nested.id);
    assert!(report.bodies().findings().is_empty());
}

#[test]
fn resource_exhaustion_does_not_erase_independent_lexical_findings() {
    let fixture = Fixture::new("function probe(): void { let text = `text ${missing}`; }");
    let graph = fixture.load(false);
    // This budget permits name lookup but exhausts during expression checking.
    let limited = analyze_typed(
        &graph,
        AnalysisLimits {
            max_entries: 123,
            ..AnalysisLimits::default()
        },
    );
    assert_eq!(limited.completion(), TypedCompletion::ResourceLimit);
    assert!(limited.bodies().findings().iter().any(|f| f.kind == ScalarFindingKind::MissingValue && f.site.span.bytes() == b"missing"));
}

#[test]
fn nested_declarations_keep_lexical_identity_without_runtime_capture() {
    let fixture = Fixture::new(
        "type Item = { outer: bool };\n\
         function root(): bool {\n\
           let value = true;\n\
           if (false) { function unused(): bool { return value; } }\n\
           type Item = { inner: bool };\n\
           function local(x: Item): bool { return x.inner; }\n\
           return local(Item { inner: true });\n\
         }\n\
         function outside(): bool { return local(); }",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let local = module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("local"))
        .unwrap();
    let local_facts = report
        .bodies()
        .body(module, BodyOwner::Declaration(&local.id))
        .unwrap()
        .unwrap();
    assert!(local_facts.flow.returns);
    let findings = report.bodies().findings();
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.kind == ScalarFindingKind::StartupCapture)
            .count(),
        1
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.kind == ScalarFindingKind::MissingValue)
            .count(),
        1
    );
    assert_eq!(findings.len(), 2);
}

#[test]
fn integrated_limits_include_visibility_and_reject_foreign_nodes() {
    let fixture =
        Fixture::new("type Hidden = { x: bool }; export enum E { Tag(Hidden) } expect true;");
    let graph = fixture.load(false);
    let complete_work = analyze_typed(&graph, AnalysisLimits::default()).statistics();
    let limits = AnalysisLimits {
        max_work: complete_work.work,
        max_entries: complete_work.entries,
        max_depth: complete_work.max_depth,
    };
    let report = analyze_typed(&graph, limits);
    assert!(
        report.receipt().is_none(),
        "partial facts do not issue a receipt"
    );
    assert_eq!(report.completion(), TypedCompletion::Incomplete);
    assert_eq!(report.statistics(), complete_work);
    assert!(report.not_performed().contains(&Stage::ResultUse));
    assert_eq!(report.public_interface_findings().len(), 1);
    for short in [
        AnalysisLimits {
            max_work: limits.max_work - 1,
            ..limits
        },
        AnalysisLimits {
            max_entries: limits.max_entries - 1,
            ..limits
        },
    ] {
        let limited = analyze_typed(&graph, short);
        assert_eq!(limited.completion(), TypedCompletion::ResourceLimit);
        assert!(limited.statistics().work <= short.max_work);
        assert!(limited.statistics().entries <= short.max_entries);
    }
    let stopped = analyze_typed(
        &graph,
        AnalysisLimits {
            max_work: 0,
            ..limits
        },
    );
    for stage in [
        Stage::DeclarationTypes,
        Stage::Bodies,
        Stage::Startup,
        Stage::Expectations,
    ] {
        assert!(stopped.not_performed().contains(&stage));
    }
    let foreign = fixture.load(false);
    assert_eq!(
        report.bodies().node(&foreign.modules()[0], 0).unwrap_err(),
        QueryError::InvalidRequest
    );
}

#[test]
fn rejected_header_blocks_uses_but_not_an_independent_body() {
    let fixture = Fixture::new(
        "function broken(x: Missing): bool { return true; }\n\
         function usesBroken(): bool { return broken(true); }\n\
         function other(): bool { return 2i32; }",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert!(report.bodies().annotations().findings().next().is_some());
    let mismatch = report
        .bodies()
        .findings()
        .iter()
        .find(|f| f.kind == ScalarFindingKind::TypeMismatch)
        .unwrap();
    assert_eq!(mismatch.site.span.bytes(), b"2i32");
    assert_eq!(
        report
            .bodies()
            .obligations()
            .iter()
            .filter(|o| o.reason == ScalarUnsupported::DeclarationPrerequisite)
            .count(),
        2
    );
}

#[test]
fn pipe_insertion_types_the_original_input_and_preserves_evaluation_order() {
    let fixture = Fixture::new(
        "function pad(on: bool, width: i32): i32 { return width; } let a = 1 |> pad(true, _); let b = true |> pad(2);",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    for (node, syntax) in module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == NodeKind::Pipe)
    {
        let NodeFactKind::Expression(expression) =
            &report.bodies().node(module, node).unwrap().unwrap().kind
        else {
            panic!("pipe expression")
        };
        let call = expression.call.as_ref().unwrap();
        let input = syntax.children[0];
        let written =
            module.syntax.nodes[module.syntax.nodes[syntax.children[1]].children[1]].children[0];
        assert_eq!(
            call.argument_evaluation_order().collect::<Vec<_>>(),
            [input, written]
        );
        let second = if module.syntax.nodes[input].span.bytes() == b"1" {
            input
        } else {
            written
        };
        assert_eq!(call.arguments[1], second);
        assert!(
            matches!(report.bodies().node(module, second).unwrap().map(|n| &n.kind), Some(NodeFactKind::Expression(e)) if matches!(e.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"))
        );
    }
    assert!(report.bodies().findings().is_empty());
}

#[test]
fn aggregate_noncopying_field_can_settle_borrowed_binder_and_capture_demand() {
    let fixture = Fixture::new(
        "function known(value: &[[string]]): &[string] { let [part] = value; return part; }\n\
         function pending(value: &[[i32]]): &[i32] { let [part] = value; return part; }\n\
         let owned: [string] = [\"text\"]; let callable = () => owned;",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let known = module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("known"))
        .unwrap();
    assert!(
        report
            .bodies()
            .body(module, BodyOwner::Declaration(&known.id))
            .unwrap()
            .is_some()
    );
    assert!(
        report
            .bodies()
            .obligations()
            .iter()
            .any(|o| o.reason == ScalarUnsupported::ImplicitCopy)
    );
    let arrow = module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Arrow)
        .unwrap();
    assert_eq!(
        report.bodies().arrow(module, arrow).unwrap().unwrap().mode,
        Some(InvocationMode::Once)
    );
}

#[test]
fn borrowed_binders_and_captures_classify_substituted_noncopying_nominals() {
    let fixture = Fixture::new(
        "type Box<T> = { element: T; }\n\
         enum Choice<T> { Item(Box<T>), Empty }\n\
         function borrowed(input: &[Box<string>]): &Box<string> { let [item] = input; return item; }\n\
         function variant(input: &[Choice<string>.Item]): &Choice<string>.Item { let [item] = input; return item; }\n\
         function pending(input: &[Choice<string>.Empty]): &Choice<string>.Empty { let [item] = input; return item; }\n\
         let owned: Box<string> = Box { element: \"text\" }; let callable = () => owned;",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    assert!(report.bodies().findings().is_empty());
    assert!(
        report
            .bodies()
            .obligations()
            .iter()
            .any(|o| o.reason == ScalarUnsupported::ImplicitCopy)
    );
    for (name, checked) in [("borrowed", true), ("variant", true), ("pending", false)] {
        let declaration = module
            .declarations
            .iter()
            .find(|d| d.name.as_deref() == Some(name))
            .unwrap();
        assert_eq!(
            report
                .bodies()
                .body(module, BodyOwner::Declaration(&declaration.id))
                .unwrap()
                .is_some(),
            checked,
            "{name}"
        );
    }
    let arrow = module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Arrow)
        .unwrap();
    assert_eq!(
        report.bodies().arrow(module, arrow).unwrap().unwrap().mode,
        Some(InvocationMode::Once)
    );
}

#[test]
fn invalid_nominal_blocks_its_members_without_stopping_independent_source() {
    let fixture = Fixture::new(
        "import helper; type Broken = { value: bool; value: i32 }; function useBroken(x: Broken): bool { return x.value; } let x = Broken { value: true }; let bad: bool = 1i32;",
    );
    fixture.write("app/helper.rill", "let wrong: bool = 2i32;");
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let findings = report.bodies().findings();
    assert_eq!(findings.len(), 2);
    assert!(
        findings
            .iter()
            .all(|f| f.kind == ScalarFindingKind::TypeMismatch)
    );
    assert!(findings.iter().any(|f| f.site.module.path == "helper.rill"));
    assert_eq!(
        report
            .bodies()
            .obligations()
            .iter()
            .filter(|o| o.reason == ScalarUnsupported::DeclarationPrerequisite)
            .count(),
        2
    );
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "rill-m4-8-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let fixture = Self(path);
        for dir in ["app", "std"] {
            fs::create_dir(fixture.0.join(dir)).unwrap();
        }
        fixture.write("app/main.rill", source);
        fixture.write(
            "std/core.rill",
            "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
        );
        fixture
    }
    fn write(&self, name: &str, body: &str) {
        fs::write(self.0.join(name), body).unwrap();
    }
    fn load(&self, test_mode: bool) -> Graph {
        let select = |name: &str| Selector {
            module: "core.rill".into(),
            name: name.into(),
        };
        load(Request {
            entry: self.0.join("app/main.rill"),
            standard_root: self.0.join("std"),
            prelude: Prelude {
                copy: select("copy"),
                dispose: select("dispose"),
            },
            target: "typed-test".into(),
            options: BTreeMap::new(),
            test_mode,
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
fn both_modes_keep_independent_bodies_and_findings_without_core_admission() {
    let fixture = Fixture::new(
        "import helper::{ok};\nexpect ok();\nfunction unused(): bool { return 1i32; }",
    );
    fixture.write(
        "app/helper.rill",
        "export function ok(): bool { return true; }\nlet wrong: bool = 0i32; expect 2i32;",
    );
    for mode in [false, true] {
        let graph = fixture.load(mode);
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert_eq!(report.completion(), TypedCompletion::Incomplete);
        assert_eq!(
            report.core_blockers(),
            [
                CoreBlocker::Roles,
                CoreBlocker::Providers,
                CoreBlocker::Protection,
                CoreBlocker::Representation
            ]
        );
        let helper = graph
            .modules()
            .iter()
            .find(|m| m.key.path == "helper.rill")
            .unwrap();
        let good = helper
            .declarations
            .iter()
            .find(|d| d.id.kind == DeclarationKind::Function)
            .unwrap();
        assert!(
            report
                .bodies()
                .body(helper, BodyOwner::Declaration(&good.id))
                .unwrap()
                .is_some()
        );
        let findings = report.bodies().findings();
        assert!(
            findings
                .iter()
                .any(|f| f.site.module == &helper.key && f.kind == ScalarFindingKind::TypeMismatch)
        );
        assert!(findings.iter().any(
            |f| f.site.module == &helper.key && f.kind == ScalarFindingKind::ExpectationBoolean
        ));
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.kind == ScalarFindingKind::TypeMismatch)
                .count(),
            2
        );
    }
}

#[test]
fn tuple_selectors_preserve_the_actual_component_and_borrow_path() {
    let fixture = Fixture::new(
        "function second(pair: &mut [bool,i32]): &mut i32 { return &mut pair[0x1]; }\n\
         function bad(pair: [bool,i32], i: u64): void { pair[i]; pair[2]; pair[1u64]; }\n\
         type List = { value: i32 }; function fake(x: List): void { x[0]; }",
    );
    let graph = fixture.load(false);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let nodes: Vec<_> = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == NodeKind::Index)
        .collect();
    let (index, syntax) = nodes[0];
    let NodeFactKind::Expression(fact) =
        &report.bodies().node(module, index).unwrap().unwrap().kind
    else {
        panic!("index expression")
    };
    assert!(matches!(fact.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"));
    assert!(
        matches!(fact.data.as_ref(), Some(DataExpression::TupleIndex { base, selector, component: 1 }) if [*base,*selector] == syntax.children.as_slice())
    );
    assert_eq!(
        fact.path.as_ref().unwrap().projections,
        [PathProjection::Dereference, PathProjection::TupleElement(1)]
    );
    assert_eq!(
        report
            .bodies()
            .findings()
            .iter()
            .filter(|f| f.kind == ScalarFindingKind::InvalidTupleIndex)
            .count(),
        3
    );
    assert!(
        report
            .bodies()
            .obligations()
            .iter()
            .any(|o| o.reason == ScalarUnsupported::CoreAdmission && o.site.node == nodes[4].0)
    );
    assert!(
        !report
            .bodies()
            .findings()
            .iter()
            .any(|f| f.kind == ScalarFindingKind::NumericContext)
    );
}
