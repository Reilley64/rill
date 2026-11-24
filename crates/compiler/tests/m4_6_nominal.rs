//! M4.6 nominal application checks over actual M3 captures.
#[path = "support/limit.rs"]
mod limit;

use compiler::analysis::generics::{GenericReport, analyze_generics};
use compiler::analysis::ordinary::analyze_ordinary;
use compiler::analysis::scalars::data::{ArgumentOrigin, DataExpression};
use compiler::analysis::scalars::{
    BodyOwner, NodeFactKind, ScalarFailureKind, ScalarFindingKind, ScalarUnsupported,
};
use compiler::analysis::{AnalysisLimits, FindingKind, TypeTerm, Unsupported};
use compiler::loader::{Limits, Module, Prelude, Request, Selector, load};
use compiler::parser::NodeKind;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn inspect(source: &str, check: impl FnOnce(&Module, &GenericReport<'_>)) {
    inspect_with_limits(source, &[], AnalysisLimits::default(), check);
}

fn inspect_with_limits(
    source: &str,
    companions: &[(&str, &str)],
    limits: AnalysisLimits,
    check: impl FnOnce(&Module, &GenericReport<'_>),
) {
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "rill-m4-6-nominal-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    for dir in ["app", "std"] {
        fs::create_dir_all(fixture.0.join(dir)).unwrap();
    }
    fs::write(fixture.0.join("app/main.rill"), source).unwrap();
    for (name, source) in companions {
        fs::write(fixture.0.join("app").join(name), source).unwrap();
    }
    fs::write(
        fixture.0.join("std/core.rill"),
        "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
    )
    .unwrap();
    let graph = load(Request {
        entry: fixture.0.join("app/main.rill"),
        standard_root: fixture.0.join("std"),
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
        target: "nominal-test".into(),
        options: BTreeMap::new(),
        test_mode: false,
        limits: Limits::default(),
    })
    .unwrap();
    let ordinary = analyze_ordinary(&graph, AnalysisLimits::default());
    let report = analyze_generics(&ordinary, limits);
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    check(module, &report);
}

#[test]
fn a_type_only_application_completes_earlier_dependent_defaults() {
    inspect(
        "type Cell<T = i32> = { value: T }; type Pair<A, B = Cell<A>> = { first: A; second: B };
         let p: Pair<bool> = Pair { first: true, second: Cell { value: true } };",
        |module, report| {
            assert!(
                report.body(module, BodyOwner::Startup).unwrap().is_some(),
                "findings={:?} obligations={:?} annotations={:?}",
                report
                    .findings()
                    .iter()
                    .map(|f| (f.kind, f.site.span.bytes()))
                    .collect::<Vec<_>>(),
                report
                    .obligations()
                    .iter()
                    .map(|o| (o.reason, o.site.span.bytes()))
                    .collect::<Vec<_>>(),
                report
                    .annotations()
                    .findings()
                    .map(|f| (f.kind, f.span.bytes()))
                    .collect::<Vec<_>>()
            );
            let (node, _) = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.kind == NodeKind::NamedType && n.span.bytes() == b"Pair<bool>")
                .unwrap();
            let fact = report
                .node(module, node)
                .unwrap()
                .expect("defaulted annotation");
            let NodeFactKind::Annotation(annotation) = &fact.kind else {
                panic!("type use")
            };
            assert_eq!(annotation.substitution.len(), 2);
            assert_eq!(annotation.defaults.len(), 1);
            assert_eq!(
                annotation.defaults[0].binder,
                annotation.substitution[1].binder
            );
            assert_eq!(annotation.defaults[0].span.bytes(), b"Cell<A>");
            let TypeTerm::Nominal {
                declaration,
                arguments,
            } = annotation.substitution[1].argument.as_ref()
            else {
                panic!("Cell<bool> default")
            };
            assert_eq!(declaration.kind, compiler::loader::DeclarationKind::Record);
            assert!(matches!(arguments[0].as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"));
        },
    );
}

#[test]
fn payload_defaults_are_used_only_after_independent_anchors() {
    inspect(
        "enum Choice<T = i32> { Empty, Value(T, T), Named { item: T } }
         let inferred = Choice.Value(true, false);
         let defaulted = Choice.Named { item: 1 };
         let unit = Choice.Empty;",
        |module, report| {
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_some());
            for (spelling, variant, width, used_default) in [
                ("Choice.Value(true, false)", "Value", "bool", false),
                ("Choice.Named { item: 1 }", "Named", "i32", true),
                ("Choice.Empty", "Empty", "i32", true),
            ] {
                let (node, _) = module
                    .syntax
                    .nodes
                    .iter()
                    .enumerate()
                    .find(|(_, n)| n.span.bytes() == spelling.as_bytes())
                    .unwrap();
                let expr = report.node(module, node).unwrap().expect(spelling);
                let NodeFactKind::Expression(expr) = &expr.kind else {
                    panic!("expression")
                };
                let TypeTerm::Variant { declaration, .. } = expr.ty.as_ref() else {
                    panic!("variant")
                };
                assert!(
                    module
                        .declarations
                        .iter()
                        .any(|d| d.id == **declaration && d.name.as_deref() == Some(variant))
                );
                let Some(DataExpression::Construction(c)) = &expr.data else {
                    panic!("construction")
                };
                assert!(
                    matches!(c.substitution[0].argument.as_ref(), TypeTerm::Primitive(p) if p.name() == width)
                );
                let default = c
                    .argument_sources
                    .iter()
                    .find(|s| matches!(s.origin, ArgumentOrigin::Default));
                assert_eq!(
                    default.map(|s| s.site.span.bytes()),
                    used_default.then_some(b"i32".as_slice())
                );
            }
        },
    );
}

#[test]
fn header_annotations_use_the_same_defaults_as_local_annotations() {
    inspect(
        "type Cell<T = bool> = { value: T }; function quote(input: Cell): Cell { return input; }",
        |module, report| {
            let function = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("quote"))
                .unwrap();
            assert!(
                report
                    .body(module, BodyOwner::Declaration(&function.id))
                    .unwrap()
                    .is_some()
            );
            for (node, _) in module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == NodeKind::NamedType && n.span.bytes() == b"Cell")
            {
                let NodeFactKind::Annotation(ty) =
                    &report.node(module, node).unwrap().unwrap().kind
                else {
                    panic!("annotation")
                };
                assert_eq!(ty.defaults[0].span.bytes(), b"bool");
            }
        },
    );
}

#[test]
fn a_repeating_default_is_pending_but_a_growing_application_exhausts_depth() {
    inspect(
        "type Cycle<T = Cycle> = { marker: bool }; let c: Cycle = Cycle { marker: true };",
        |module, report| {
            assert_eq!(report.completion(), ScalarFailureKind::Incomplete);
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_none());
            assert!(
                report.annotations().obligations().any(|o| o.reason
                    == Unsupported::RecursiveDefault
                    && o.span.bytes() == b"Cycle")
            );
            assert!(report.findings().is_empty());
        },
    );
    inspect_with_limits(
        "type Cell<A> = { value: A }; type Grow<A, B = Grow<Cell<A>>> = { marker: bool };
         let c: Grow<bool> = Grow { marker: true };",
        &[],
        AnalysisLimits {
            max_depth: 24,
            ..AnalysisLimits::default()
        },
        |module, report| {
            limit::assert_generics_limit(report);
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_none());
            assert!(
                !report
                    .annotations()
                    .obligations()
                    .any(|o| o.reason == Unsupported::RecursiveDefault)
            );
        },
    );
}

#[test]
fn rebuilds_preserve_the_exact_base_application_and_reject_conflicting_leading_arguments() {
    inspect(
        "type Pair<A, B> = { first: A; second: B }; function update(base: Pair<bool, i32>): void {
           let fixed = Pair<bool> { ...base, second: 2 };
           let wrong = Pair<i32> { ...base, second: 3i32 };
         }",
        |module, report| {
            let (good, bad) = {
                let mut nodes = module
                    .syntax
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.kind == NodeKind::Construction);
                (nodes.next().unwrap().0, nodes.next().unwrap().0)
            };
            let NodeFactKind::Expression(expr) = &report.node(module, good).unwrap().unwrap().kind
            else {
                panic!("expression")
            };
            let Some(DataExpression::Construction(c)) = &expr.data else {
                panic!("construction")
            };
            let base = c.base.unwrap();
            let NodeFactKind::Expression(base_expr) =
                &report.node(module, base).unwrap().unwrap().kind
            else {
                panic!("base")
            };
            assert_eq!(expr.ty, base_expr.ty);
            assert!(c.argument_sources.iter().any(|s| s.position == 0
                && matches!(s.origin, ArgumentOrigin::Written)
                && s.site.span.bytes() == b"bool"));
            assert!(c.argument_sources.iter().any(|s| s.position == 1
                && matches!(s.origin, ArgumentOrigin::RebuildBase)
                && s.site.node == base));
            assert!(report.node(module, bad).unwrap().is_none());
            assert!(report.findings().iter().any(
                |f| f.kind == ScalarFindingKind::TypeMismatch && f.site.span.bytes() == b"base"
            ));
        },
    );
}

#[test]
fn nominal_defaults_require_a_trailing_suffix_and_cannot_see_later_binders() {
    for (parameters, rejected) in [
        ("A = bool, B", FindingKind::NonTrailingDefault),
        ("A = B, B = bool", FindingKind::InvalidDefaultDependency),
        ("A = A", FindingKind::InvalidDefaultDependency),
    ] {
        inspect(
            &format!(
                "type A = {{ v: bool }}; type B = {{ v: bool }}; type Shape<{parameters}> = {{ v: bool }};"
            ),
            |module, report| {
                assert!(report.annotations().findings().any(|f| f.kind == rejected));
                assert!(report.body(module, BodyOwner::Startup).unwrap().is_none());
            },
        );
    }
}

#[test]
fn never_does_not_anchor_a_nominal_slot_and_pending_list_is_not_a_missing_context_error() {
    inspect(
        "function stop(): never { return stop(); }
         type Box<T> = { value: T }; let stalled = Box { value: stop() };",
        |module, report| {
            assert!(
                report
                    .findings()
                    .iter()
                    .any(|f| f.kind == ScalarFindingKind::InsufficientContext)
            );
            let (node, _) = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .rfind(|(_, n)| n.kind == NodeKind::Call)
                .unwrap();
            let NodeFactKind::Expression(expr) = &report.node(module, node).unwrap().unwrap().kind
            else {
                panic!("never expression")
            };
            assert!(matches!(expr.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "never"));
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_none());
        },
    );
    inspect(
        "type Box<T = i32> = { value: T }; let stalled = Box { value: [true] };",
        |module, report| {
            assert!(report.findings().is_empty());
            assert!(
                report
                    .obligations()
                    .iter()
                    .any(|o| o.reason == ScalarUnsupported::CoreAdmission)
            );
            let (node, _) = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.span.bytes() == b"true")
                .unwrap();
            assert!(report.node(module, node).unwrap().is_some());
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_none());
        },
    );
}

#[test]
fn aliases_retain_the_defining_type_and_do_not_admit_private_fields() {
    inspect_with_limits(
        "import api::{Open as Alias, Closed as Hidden};
         let good: Alias = Alias { value: true };
         let bad = Hidden { value: true, secret: false };",
        &[(
            "api.rill",
            "export type Open<T = bool> = { value: T }; export type Closed<T> = { value: T; private secret: bool };",
        )],
        AnalysisLimits::default(),
        |module, report| {
            let mut nodes = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == NodeKind::Construction);
            let good = nodes.next().unwrap().0;
            let bad = nodes.next().unwrap().0;
            let NodeFactKind::Expression(expr) = &report.node(module, good).unwrap().unwrap().kind
            else {
                panic!("expression")
            };
            let Some(DataExpression::Construction(c)) = &expr.data else {
                panic!("construction")
            };
            assert_eq!(c.bindings[0].name, "Alias");
            assert!(
                matches!(c.ty.as_ref(), TypeTerm::Nominal { declaration, .. } if declaration.module.path == "api.rill")
            );
            assert!(report.node(module, bad).unwrap().is_none());
            assert!(report.findings().iter().any(
                |f| f.kind == ScalarFindingKind::PrivateValue && f.bindings[0].name == "Hidden"
            ));
        },
    );
}

#[test]
fn a_fresh_tuple_field_can_anchor_one_slot_before_defaulting_another() {
    inspect(
        "type Pair<A = i32, B = i32> = { items: [A, B] }; let p = Pair { items: [1, true] };",
        |module, report| {
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_some());
            let (node, _) = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.kind == NodeKind::Construction)
                .unwrap();
            let NodeFactKind::Expression(expr) = &report.node(module, node).unwrap().unwrap().kind
            else {
                panic!("expression")
            };
            let Some(DataExpression::Construction(c)) = &expr.data else {
                panic!("construction")
            };
            assert!(
                matches!(c.substitution[0].argument.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
            );
            assert!(
                matches!(c.substitution[1].argument.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
            );
            let (node, _) = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.kind == NodeKind::Bracket)
                .unwrap();
            let NodeFactKind::Expression(tuple) = &report.node(module, node).unwrap().unwrap().kind
            else {
                panic!("tuple")
            };
            assert!(matches!(tuple.data, Some(DataExpression::Tuple { .. })));
        },
    );
}

#[test]
fn a_fixed_parent_admits_a_variant_but_never_inside_an_existing_tuple() {
    inspect(
        "enum Pick<T> { Take(T), Skip }
         type Holder<T = i32> = { item: T; count: u8 };
         function good(): Pick<i32> { return Pick.Take(1); }
         function wrongVariant(): Pick<i32>.Skip { return Pick.Take(1); }
         function wrongTuple(actual: [Pick<i32>.Take, bool]): void {
           let bad = Holder<[Pick<i32>, bool]> { item: actual, count: 256u8 };
         }
         let conflict = Holder { item: true, count: 256u8 };",
        |module, report| {
            let first = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.kind == NodeKind::Call)
                .unwrap()
                .0;
            let NodeFactKind::Expression(expr) = &report.node(module, first).unwrap().unwrap().kind
            else {
                panic!("variant expression")
            };
            assert!(expr.admission.is_some());
            assert!(
                report
                    .findings()
                    .iter()
                    .any(|f| f.kind == ScalarFindingKind::TypeMismatch
                        && f.site.span.bytes() == b"Pick.Take(1)")
            );
            assert!(
                report
                    .findings()
                    .iter()
                    .any(|f| f.kind == ScalarFindingKind::TypeMismatch
                        && f.site.span.bytes() == b"actual")
            );
            assert_eq!(
                report
                    .findings()
                    .iter()
                    .filter(|f| f.kind == ScalarFindingKind::LiteralRange)
                    .count(),
                2
            );
        },
    );
}

#[test]
fn dependent_written_defaults_resume_with_inferred_earlier_arguments() {
    inspect(
        "type Pair<A, B = A> = { left: A; right: B }; let p = Pair { left: 1u8, right: 2 };",
        |module, report| {
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_some());
            let (node, _) = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.kind == NodeKind::Construction)
                .unwrap();
            let NodeFactKind::Expression(expr) = &report.node(module, node).unwrap().unwrap().kind
            else {
                panic!("expression")
            };
            let Some(DataExpression::Construction(c)) = &expr.data else {
                panic!("construction")
            };
            assert_eq!(c.substitution[0].argument, c.substitution[1].argument);
            assert!(
                matches!(c.substitution[1].argument.as_ref(), TypeTerm::Primitive(p) if p.name() == "u8")
            );
            assert!(c.argument_sources.iter().any(|s| s.position == 0
                && s.origin == ArgumentOrigin::Member
                && s.site.span.bytes() == b"1u8"));
            assert!(c.argument_sources.iter().any(|s| s.position == 1
                && s.origin == ArgumentOrigin::Default
                && s.site.span.bytes() == b"A"));
        },
    );
}

#[test]
fn a_conflicting_member_does_not_erase_independent_literal_validation() {
    inspect(
        "type Pair<T = i32> = { first: T; second: T; count: u8 };
         let bad = Pair { first: true, second: 1i32, count: 256u8 };",
        |module, report| {
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_none());
            assert!(report.findings().iter().any(
                |f| f.kind == ScalarFindingKind::TypeMismatch && f.site.span.bytes() == b"1i32"
            ));
            assert!(
                report
                    .findings()
                    .iter()
                    .any(|f| f.kind == ScalarFindingKind::LiteralRange
                        && f.site.span.bytes() == b"256u8")
            );
            let (node, _) = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.kind == NodeKind::Construction)
                .unwrap();
            assert!(report.node(module, node).unwrap().is_none());
        },
    );
}

#[test]
fn default_inventory_keeps_the_inline_cycle_witness_visible() {
    inspect(
        "type Cell<T = bool> = { next: Cell };",
        |_module, report| {
            use compiler::analysis::ordinary::DeclarationIssue;
            assert!(report.declaration_issues().iter().any(|issue| matches!(issue, DeclarationIssue::InlineCycle { steps } if steps.len() == 1 && steps[0].member.name.as_deref() == Some("next"))));
        },
    );
}

#[test]
fn a_nominal_default_waits_through_a_nested_receiving_application() {
    inspect(
        "type Inner<T> = { value: T };
         type Outer<T = i32> = { child: Inner<T> };
         let nested = Outer { child: Inner { value: 1 } };",
        |module, report| {
            assert!(
                report.body(module, BodyOwner::Startup).unwrap().is_some(),
                "{report:#?}"
            );
            let outer = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .rfind(|(_, n)| n.kind == NodeKind::Construction)
                .unwrap()
                .0;
            let NodeFactKind::Expression(fact) = &report.node(module, outer).unwrap().unwrap().kind
            else {
                panic!("outer construction")
            };
            let Some(DataExpression::Construction(construction)) = fact.data.as_ref() else {
                panic!("outer construction provenance")
            };
            assert!(
                construction
                    .argument_sources
                    .iter()
                    .any(|s| s.origin == ArgumentOrigin::Default)
            );
        },
    );
}

#[test]
fn a_written_non_numeric_receiver_does_not_supply_a_numeric_width() {
    inspect(
        "type Cell<T = bool> = { value: T }; let bad = Cell { value: 1 };",
        |module, report| {
            assert!(report.findings().iter().any(
                |f| f.kind == ScalarFindingKind::NumericContext && f.site.span.bytes() == b"1"
            ));
            assert!(
                !report
                    .findings()
                    .iter()
                    .any(|f| f.kind == ScalarFindingKind::InsufficientContext)
            );
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_none());
        },
    );
}

#[test]
fn a_later_record_field_supplies_the_width_for_an_earlier_field() {
    inspect(
        "type Pair<T> = { first: T; second: T }; let p = Pair { first: 1, second: 2i32 };",
        |module, report| {
            assert!(report.body(module, BodyOwner::Startup).unwrap().is_some());
            let (node, _) = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.kind == NodeKind::Construction)
                .unwrap();
            let fact = report
                .node(module, node)
                .unwrap()
                .expect("checked Pair construction");
            let NodeFactKind::Expression(expr) = &fact.kind else {
                panic!("expression")
            };
            let Some(DataExpression::Construction(c)) = &expr.data else {
                panic!("construction")
            };
            assert_eq!(c.fields.len(), 2);
            assert!(
                matches!(c.substitution[0].argument.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
            );
            let first = report
                .node(module, c.fields[0].expression)
                .unwrap()
                .unwrap();
            let NodeFactKind::Expression(first) = &first.kind else {
                panic!("first field")
            };
            assert_eq!(first.ty, c.substitution[0].argument);
        },
    );
}
