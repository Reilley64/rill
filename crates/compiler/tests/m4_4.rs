//! M4.4 public data/control assertions, not Rill execution or ownership acceptance.
use compiler::analysis::data::{DataExpression, check_data};
use compiler::analysis::declarations::{Contract, PreludeContracts, Schema, check_declarations};
use compiler::analysis::scalars::NodeFactKind;
use compiler::analysis::{AnalysisLimits, TypeTerm, analyze_declaration_types};
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
            "rill-m4-4-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let fixture = Self(path);
        fixture.write("app/main.rill", source);
        fixture.write(
            "std/core.rill",
            "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
        );
        fixture
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    fn load(&self) -> Graph {
        self.load_mode(false)
    }
    fn load_mode(&self, test_mode: bool) -> Graph {
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
            target: "data-test".into(),
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
fn contracts(graph: &Graph) -> PreludeContracts<'_> {
    PreludeContracts {
        copy: Contract {
            declaration: &graph.prelude().copy,
            schema: Schema::BorrowedCopy,
        },
        dispose: Contract {
            declaration: &graph.prelude().dispose,
            schema: Schema::ConsumingDispose,
        },
    }
}
fn inspect(
    source: &str,
    check: impl FnOnce(
        &Graph,
        Result<
            compiler::analysis::data::DataReceipt<'_, '_>,
            compiler::analysis::scalars::ScalarFailure<'_>,
        >,
    ),
) {
    let fixture = Fixture::new(source);
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    check(&graph, check_data(&declarations, AnalysisLimits::default()));
}

#[test]
fn field_projection_keeps_declared_type_and_reference_access() {
    inspect(
        "type Point = { x: i32 }; function f(p: &mut Point): i32 { p.x = 2; return p.x; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            for (node, _) in module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == compiler::parser::NodeKind::Field)
            {
                let NodeFactKind::Expression(e) = &result.node(module, node).unwrap().unwrap().kind
                else {
                    panic!("projection")
                };
                assert_eq!(e.place, compiler::analysis::scalars::Place::Writable);
                let DataExpression::Projection(p) = e.data.as_ref().unwrap() else {
                    panic!("field fact")
                };
                assert_eq!(p.field.name.as_deref(), Some("x"));
            }
        },
    );
}

#[test]
fn established_generic_application_and_rebuild_base_share_one_substitution() {
    inspect(
        "type Box<T> = { value: T }; function make(): Box<i32> { let value = 1i32; return Box { value }; } function update(old: Box<i32>): Box<i32> { return Box { ...old, value: 2 }; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            for (node, _) in module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == compiler::parser::NodeKind::Construction)
            {
                let NodeFactKind::Expression(e) = &result.node(module, node).unwrap().unwrap().kind
                else {
                    panic!("construction")
                };
                let DataExpression::Construction(c) = e.data.as_ref().unwrap() else {
                    panic!("constructor")
                };
                assert_eq!(c.substitution.len(), 1);
                assert!(
                    matches!(c.substitution[0].argument.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
                );
            }
        },
    );
}

#[test]
fn enum_payload_forms_preserve_variant_identity_admission_and_parent_joins() {
    inspect(
        "enum E<T> { Empty, Data(T), Named { value: T } } function f(b: bool): E<i32> { return if (b) E.Empty else E.Data(1); } function g(): E<i32> { return E.Named { value: 2 }; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let mut constructed = 0;
            for (node, _) in module.syntax.nodes.iter().enumerate() {
                if let Some(fact) = result.node(module, node).unwrap()
                    && let NodeFactKind::Expression(e) = &fact.kind
                    && matches!(e.data, Some(DataExpression::Construction(_)))
                {
                    assert!(matches!(e.ty.as_ref(), TypeTerm::Variant { .. }));
                    assert!(e.admission.is_some());
                    constructed += 1;
                }
            }
            assert_eq!(constructed, 3);
        },
    );
}

#[test]
fn tuple_context_fixes_category_and_per_position_literal_types() {
    inspect(
        "function f(): [i32, bool] { return [1, true]; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let node = module
                .syntax
                .nodes
                .iter()
                .position(|n| n.kind == compiler::parser::NodeKind::Bracket)
                .unwrap();
            let NodeFactKind::Expression(e) = &result.node(module, node).unwrap().unwrap().kind
            else {
                panic!("tuple")
            };
            assert!(matches!(e.ty.as_ref(), TypeTerm::Tuple(elements) if elements.len() == 2));
        },
    );
    inspect("let x = [1i32, true];", |_, result| {
        assert_eq!(
            result.unwrap_err().kind,
            compiler::analysis::scalars::ScalarFailureKind::Incomplete
        );
    });
}

#[test]
fn irrefutable_tuple_bindings_retain_canonical_identities_and_borrowed_scalar_types() {
    inspect(
        "function f(t: &[i32, bool]): i32 { let [x, flag] = t; return x; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let x = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("x"))
                .unwrap();
            let node = module
                .syntax
                .nodes
                .iter()
                .position(|n| {
                    n.span.bytes() == b"x" && n.kind == compiler::parser::NodeKind::ValueReference
                })
                .unwrap();
            let NodeFactKind::Expression(e) = &result.node(module, node).unwrap().unwrap().kind
            else {
                panic!("binding use")
            };
            assert_eq!(e.binding, Some(&x.id));
            assert!(matches!(e.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"));
        },
    );
}

#[test]
fn nested_constructor_patterns_and_component_holes_share_irrefutability() {
    inspect(
        "enum E<T> { V(T) } function f(v: E<i32>): i32 { let E.V(x) = v; return x; }",
        |_, result| {
            result.unwrap();
        },
    );
    for payload in ["never, bool", "bool, never"] {
        let pattern = if payload.starts_with("never") {
            "_, true"
        } else {
            "true, _"
        };
        inspect(
            &format!(
                "enum E {{ V({payload}) }} function f(v: E): void {{ let E.V({pattern}) = v; }}"
            ),
            |_, result| {
                let error = result.unwrap_err();
                assert_eq!(
                    error.kind,
                    compiler::analysis::scalars::ScalarFailureKind::SourceRejected
                );
                assert!(
                    error.partial.findings.iter().any(|f| f.kind
                        == compiler::analysis::scalars::ScalarFindingKind::RefutableBinding)
                );
            },
        );
    }
}

#[test]
fn match_checks_static_boolean_coverage_and_contextual_arm_literals() {
    inspect(
        "function f(b: bool): i32 { return match (b) { true => 1, false => 2 }; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let node = module
                .syntax
                .nodes
                .iter()
                .position(|n| n.kind == compiler::parser::NodeKind::Match)
                .unwrap();
            let NodeFactKind::Expression(e) = &result.node(module, node).unwrap().unwrap().kind
            else {
                panic!("match")
            };
            assert!(matches!(e.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"));
        },
    );
    inspect(
        "function f(): i32 { return match (true) { true => 1 }; }",
        |_, result| {
            let error = result.unwrap_err();
            assert!(
                error.partial.findings.iter().any(
                    |f| f.kind == compiler::analysis::scalars::ScalarFindingKind::NonExhaustive
                )
            );
        },
    );
}

#[test]
fn outward_transfers_survive_expression_composition_without_becoming_divergence() {
    inspect(
        "function take(x: i32): void {} function f(b: bool): void { while (true) { take(match (b) { true => { break; }, false => { continue; } }); } } function g(b: bool): i32 { match (b) { true => { return 1; }, false => { return 2; } }; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            for name in ["f", "g"] {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let body = result
                    .body(
                        module,
                        compiler::analysis::scalars::BodyOwner::Declaration(&declaration.id),
                    )
                    .unwrap()
                    .unwrap();
                if name == "f" {
                    assert!(body.flow.normal && body.flow.diverges);
                } else {
                    assert!(body.flow.returns && !body.flow.normal && !body.flow.diverges);
                }
                assert!(body.flow.breaks.is_empty() && body.flow.continues.is_empty());
            }
        },
    );
}

#[test]
fn finite_integer_coverage_uses_distinct_values_without_large_domain_enumeration() {
    let rows = (0..=255)
        .map(|n| format!("{n} => true"))
        .collect::<Vec<_>>()
        .join(",");
    inspect(
        &format!("function f(x: u8): bool {{ return match (x) {{ {rows} }}; }}"),
        |_, result| {
            result.unwrap();
        },
    );
    inspect(
        "function f(x: u128): bool { return match (x) { 0 => true }; }",
        |_, result| {
            let error = result.unwrap_err();
            assert_eq!(
                error.kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
            assert!(
                error.partial.findings.iter().any(
                    |f| f.kind == compiler::analysis::scalars::ScalarFindingKind::NonExhaustive
                )
            );
        },
    );
}

#[test]
fn shadowed_and_skipped_arms_do_not_execute_transfers_but_still_typecheck() {
    for rows in [
        "true => 0i32, true => { break; }, false => 0i32",
        "_ => 0i32, _ => { break; }",
        "true if false => { break; }, _ => 0i32",
    ] {
        inspect(
            &format!("function f(b: bool): void {{ while (true) {{ match (b) {{ {rows} }}; }} }}"),
            |graph, result| {
                let result = result.unwrap();
                let module = graph
                    .modules()
                    .iter()
                    .find(|m| m.key.path == "main.rill")
                    .unwrap();
                let f = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some("f"))
                    .unwrap();
                let body = result
                    .body(
                        module,
                        compiler::analysis::scalars::BodyOwner::Declaration(&f.id),
                    )
                    .unwrap()
                    .unwrap();
                assert!(!body.flow.normal && body.flow.diverges);
            },
        );
    }
    inspect(
        "function f(b: bool): i32 { return match (b) { _ => 1, _ => false }; }",
        |_, result| {
            assert!(result.is_err());
        },
    );
}

#[test]
fn qualified_arm_views_preserve_declared_parent_and_original_binding_identity() {
    inspect(
        "enum E { A { x: i32 }, B } function f(v: E): i32 { return match (v) { E.A {} => v.x, E.B => 0 }; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let parameter = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("v"))
                .unwrap();
            let mut views = 0;
            for (node, _) in module.syntax.nodes.iter().enumerate() {
                if let Some(fact) = result.node(module, node).unwrap()
                    && let NodeFactKind::Expression(e) = &fact.kind
                    && let Some(view) = &e.refinement
                {
                    assert_eq!(e.binding, Some(&parameter.id));
                    assert!(matches!(view.original.as_ref(), TypeTerm::Nominal { .. }));
                    assert!(matches!(e.ty.as_ref(), TypeTerm::Variant { .. }));
                    views += 1;
                }
            }
            assert_eq!(views, 1);
        },
    );
}

#[test]
fn borrowed_field_scrutinees_preserve_noncopy_and_descriptor_layers() {
    inspect(
        "enum E { A { value: string } } type Wrap = { e: E }; function f(w: &Wrap): &string { let E.A { value } = w.e; return value; }",
        |_, result| {
            result.unwrap();
        },
    );
    inspect(
        "type Inner = { n: i32 }; type Outer = { r: &mut Inner }; function f(o: &Outer): void { o.r.n = 1; }",
        |_, result| {
            let error = result.unwrap_err();
            assert!(
                error
                    .partial
                    .findings
                    .iter()
                    .any(|f| f.kind == compiler::analysis::scalars::ScalarFindingKind::ReadOnly)
            );
        },
    );
}

#[test]
fn simple_bindings_keep_value_types_and_do_not_authorize_transfer_or_implicit_borrow() {
    // These are scoped typing facts. Moving the non-copyable fields from borrowed
    // storage still requires rejection by the later ownership checker.
    inspect(
        "type R = { text: string; exclusive: &mut i32; shared: &i32 }; function text(r: &R): string { let x: string = r.text; return x; } function exclusive(r: &R): &mut i32 { let x = r.exclusive; return x; } function shared(r: &R): &i32 { let x = r.shared; return x; } function borrow(r: &R): &string { let x = &r.text; return x; }",
        |_, result| {
            result.unwrap();
        },
    );
    inspect(
        "type R = { text: string }; function f(r: &R): void { let x: &string = r.text; }",
        |_, result| {
            assert_eq!(
                result.unwrap_err().kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
        },
    );
}

#[test]
fn nested_qualified_views_follow_canonical_field_paths_and_protect_ancestors() {
    inspect(
        "enum E { A { x: i32 }, B } type R = { e: E }; function f(r: R): i32 { return match (r) { R { e: E.A {} } => r.e.x, R { e: E.B } => 0 }; }",
        |_, result| {
            result.unwrap();
        },
    );
    inspect(
        "enum E { A { x: i32 }, B } type R = { e: E }; function f(r: &mut R): void { match (r.e) { E.A {} => { *r = R { e: E.B }; }, E.B => {} }; }",
        |_, result| {
            let error = result.unwrap_err();
            assert!(
                error
                    .partial
                    .findings
                    .iter()
                    .any(|f| f.kind == compiler::analysis::scalars::ScalarFindingKind::TagChange)
            );
        },
    );
}

#[test]
fn pattern_dependencies_retain_original_source_and_canonical_field_projection() {
    inspect(
        "type R = { value: i32 }; function f(r: &R): i32 { let R { value } = r; return value; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let field = module
                .declarations
                .iter()
                .find(|d| {
                    d.name.as_deref() == Some("value")
                        && d.id.kind == compiler::loader::DeclarationKind::Field
                })
                .unwrap();
            let node = module
                .syntax
                .nodes
                .iter()
                .position(|n| n.kind == compiler::parser::NodeKind::RecordPattern)
                .unwrap();
            let NodeFactKind::Pattern(p) = &result.node(module, node).unwrap().unwrap().kind else {
                panic!("pattern")
            };
            assert_eq!(p.source.span.bytes(), b"r");
            assert!(
                matches!(p.binders[0].projections.as_slice(), [compiler::analysis::data::PatternProjection::Dereference, compiler::analysis::data::PatternProjection::Field(id)] if *id == &field.id)
            );
        },
    );
}

#[test]
fn missing_core_and_unresolved_patterns_are_not_negative_coverage_or_name_evidence() {
    inspect(
        "enum Option<T> { Some(T), None } function f(v: Option<i32>): void { guard let x = v else { return; } let y = x; }",
        |_, result| {
            let e = result.unwrap_err();
            assert_eq!(
                e.kind,
                compiler::analysis::scalars::ScalarFailureKind::Incomplete
            );
            assert!(
                e.partial
                    .obligations
                    .iter()
                    .any(|o| o.reason
                        == compiler::analysis::scalars::ScalarUnsupported::CoreAdmission)
            );
            assert!(!e.partial.findings.iter().any(|f| f.kind == compiler::analysis::scalars::ScalarFindingKind::MissingValue));
        },
    );
    inspect(
        "enum E { A } enum G<T> { A } function f(x: E): i32 { return match (x) { G.A => 1 }; }",
        |_, result| {
            let e = result.unwrap_err();
            assert_eq!(
                e.kind,
                compiler::analysis::scalars::ScalarFailureKind::Incomplete
            );
            assert!(
                !e.partial.findings.iter().any(
                    |f| f.kind == compiler::analysis::scalars::ScalarFindingKind::NonExhaustive
                )
            );
        },
    );
}

#[test]
fn nonexhaustive_patterns_retain_symbolic_witnesses_and_block_failed_binders() {
    inspect(
        "enum E { V(never, bool) } function f(x: E): void { match (x) { E.V(_, true) => {} }; }",
        |_, result| {
            let e = result.unwrap_err();
            assert!(e.partial.nodes.iter().any(|n| matches!(&n.kind, NodeFactKind::Coverage { coverage, .. } if !coverage.exhaustive())));
        },
    );
    inspect(
        "function f(): void { let [x] = false; let y = x; }",
        |_, result| {
            let e = result.unwrap_err();
            assert_eq!(
                e.kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
            assert!(!e.partial.findings.iter().any(|f| f.kind == compiler::analysis::scalars::ScalarFindingKind::MissingValue));
            assert!(e.partial.nodes.iter().any(|n| matches!(&n.kind, NodeFactKind::Binding(b) if b.ty.is_none() && b.blocked_by.is_some())));
        },
    );
}

#[test]
fn static_template_patterns_share_decoded_text_equality_without_interpolation() {
    inspect(
        "function f(x: string): i32 { return match (x) { `é` => 1, \"e\" => 2, _ => 3 }; }",
        |_, result| {
            result.unwrap();
        },
    );
}

#[test]
fn finite_product_coverage_matches_all_boolean_pair_subsets_and_empty_component_orders() {
    let cases = [
        "[false, false]",
        "[false, true]",
        "[true, false]",
        "[true, true]",
    ];
    for mask in 0u8..16 {
        let rows = cases
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, p)| format!("{p} => true"))
            .collect::<Vec<_>>()
            .join(",");
        inspect(
            &format!("function f(x: [bool, bool]): bool {{ return match (x) {{ {rows} }}; }}"),
            |_, result| {
                assert_eq!(result.is_ok(), mask == 15);
            },
        );
    }
    for empty in ["never", "Empty"] {
        for reversed in [false, true] {
            let payload = if reversed {
                format!("bool, {empty}")
            } else {
                format!("{empty}, bool")
            };
            let one = if reversed { "true, _" } else { "_, true" };
            let two = if reversed { "false, _" } else { "_, false" };
            for covered in [false, true] {
                let rows = if covered {
                    format!("E.V({one}) => true, E.V({two}) => false")
                } else {
                    format!("E.V({one}) => true")
                };
                inspect(
                    &format!(
                        "enum Empty {{}} enum E {{ V({payload}) }} function f(x: E): bool {{ return match (x) {{ {rows} }}; }}"
                    ),
                    |_, result| {
                        assert_eq!(result.is_ok(), covered);
                    },
                );
            }
        }
    }
    inspect(
        "enum Empty {} function f(x: Empty): i32 { return match (x) {}; }",
        |_, result| {
            result.unwrap();
        },
    );
}

#[test]
fn member_anchors_contextualize_match_literals_and_borrows_keep_actual_variant_types() {
    inspect(
        "type R = { n: i32 }; function f(r: R, b: bool): i32 { let x = match (b) { true => r.n, false => 1 }; return x; }",
        |_, result| {
            result.unwrap();
        },
    );
    inspect(
        "enum E { A, B } function f(x: E.A): &E { return &x; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let node = module
                .syntax
                .nodes
                .iter()
                .position(|n| n.kind == compiler::parser::NodeKind::Prefix("&"))
                .unwrap();
            let NodeFactKind::Expression(e) = &result.node(module, node).unwrap().unwrap().kind
            else {
                panic!("borrow")
            };
            assert!(
                matches!(e.ty.as_ref(), TypeTerm::Reference { referent, .. } if matches!(referent.as_ref(), TypeTerm::Variant { .. }))
            );
            assert!(e.admission.is_some());
        },
    );
}

#[test]
fn symbolic_noncopy_classification_is_fixed_and_unresolved_aggregates_cannot_choose_a_fallback() {
    inspect(
        "type Box<T> = { value: T }; function f<T>(box: &Box<T>): &T { let Box { value } = box; return value; }",
        |_, result| {
            result.unwrap();
        },
    );
    inspect(
        "type Point = { n: i32 }; type Box = { point: Point }; function f(box: &Box): void { let Box { point } = box; let later = point; }",
        |_, result| {
            let e = result.unwrap_err();
            assert_eq!(
                e.kind,
                compiler::analysis::scalars::ScalarFailureKind::Incomplete
            );
            assert!(
                e.partial
                    .obligations
                    .iter()
                    .any(|o| o.reason
                        == compiler::analysis::scalars::ScalarUnsupported::ImplicitCopy)
            );
            assert!(!e.partial.findings.iter().any(|f| f.kind == compiler::analysis::scalars::ScalarFindingKind::MissingValue));
        },
    );
}

#[test]
fn qualification_never_retries_value_fields_and_grouping_forces_value_meaning() {
    inspect(
        "enum E { A } type R = { A: i32 }; function f(E: R): E.A { return E.A; } function g(E: R): i32 { return (E).A; }",
        |_, result| {
            result.unwrap();
        },
    );
    inspect(
        "enum E { A } type R = { missing: i32 }; function f(E: R): i32 { return E.missing; }",
        |_, result| {
            assert_eq!(
                result.unwrap_err().kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
        },
    );
}

#[test]
fn privacy_uses_canonical_types_without_requiring_a_local_type_binding() {
    for (library, main, accepted) in [
        (
            "export type R = { value: i32 }; export function make(): R { return R { value: 1 }; }",
            "import api; function f(): i32 { return api::make().value; }",
            true,
        ),
        (
            "type R = { value: i32 }; export function make(): R { return R { value: 1 }; }",
            "import api; function f(): i32 { return api::make().value; }",
            false,
        ),
        (
            "export type R = { value: i32; private hidden: bool };",
            "import api; function f(): api::R { return api::R { value: 1, hidden: true }; }",
            false,
        ),
    ] {
        let fixture = Fixture::new(main);
        fixture.write("app/api.rill", library);
        let graph = fixture.load();
        let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
        let declarations =
            check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
        assert_eq!(
            check_data(&declarations, AnalysisLimits::default()).is_ok(),
            accepted
        );
    }
}

#[test]
fn data_receipts_validate_queries_and_limits_are_inclusive() {
    let fixture = Fixture::new(
        "enum E { A(bool), B } function f(x: E): i32 { return match (x) { E.A(true) => 1, E.A(false) => 2, E.B => 3 }; }",
    );
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let receipt = check_data(&declarations, AnalysisLimits::default()).unwrap();
    let statistics = receipt.statistics();
    let exact = AnalysisLimits {
        max_work: statistics.work,
        max_entries: statistics.entries,
        max_depth: statistics.max_depth,
    };
    check_data(&declarations, exact).unwrap();
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
            check_data(&declarations, short).unwrap_err().kind,
            compiler::analysis::scalars::ScalarFailureKind::ResourceLimit
        );
    }
    let other = fixture.load();
    assert!(receipt.node(&other.modules()[0], 0).is_err());
    assert!(receipt.node(&graph.modules()[0], usize::MAX).is_err());
}

#[test]
fn guarded_rows_never_prove_coverage_and_skipped_operands_keep_no_executed_transfers() {
    inspect(
        "function f(b: bool): i32 { return match (b) { _ if true => 1 }; }",
        |_, result| {
            let e = result.unwrap_err();
            assert!(
                e.partial.findings.iter().any(
                    |f| f.kind == compiler::analysis::scalars::ScalarFindingKind::NonExhaustive
                )
            );
        },
    );
    inspect(
        "function f(b: bool): void { while (true) { false && match (b) { _ => { break; } }; } }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let f = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("f"))
                .unwrap();
            let body = result
                .body(
                    module,
                    compiler::analysis::scalars::BodyOwner::Declaration(&f.id),
                )
                .unwrap()
                .unwrap();
            assert!(!body.flow.normal && body.flow.diverges && body.flow.breaks.is_empty());
        },
    );
    inspect(
        "function f(b: bool): void { while (match (b) { true => { break; }, false => { continue; } }) {} }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let f = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("f"))
                .unwrap();
            let body = result
                .body(
                    module,
                    compiler::analysis::scalars::BodyOwner::Declaration(&f.id),
                )
                .unwrap()
                .unwrap();
            assert!(
                body.flow.normal
                    && body.flow.diverges
                    && body.flow.breaks.is_empty()
                    && body.flow.continues.is_empty()
            );
        },
    );
}

#[test]
fn mutable_variant_borrows_are_invariant_and_only_same_tag_replacement_preserves_views() {
    inspect(
        "enum E { A { x: i32 }, B } function f(v: &mut E): void { match (v) { E.A {} => { *v = E.A { x: 1 }; v.x = 2; }, E.B => {} }; }",
        |_, result| {
            result.unwrap();
        },
    );
    for source in [
        "enum E { A, B } function f(x: &mut E.A): &mut E { return x; }",
        "enum E { A { x: i32 }, B } function f(v: &mut E): void { match (v) { E.A {} => { *v = E.B; }, E.B => {} }; }",
        "enum E { A, B } function f(): E.A { return E.A(); }",
        "type A = { x: i32 }; type B = { x: i32 }; function f(): B { return A { x: 1 }; }",
        "function f(): [[i32]] { let x: [[i8]] = [[1]]; return x; }",
    ] {
        inspect(source, |_, result| {
            assert_eq!(
                result.unwrap_err().kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
        });
    }
}

#[test]
fn narrowings_do_not_escape_arms_or_apply_to_repeated_computed_scrutinees() {
    for source in [
        "enum E { A { x: i32 }, B } function f(v: E): i32 { match (v) { E.A {} => v.x, E.B => 0 }; return v.x; }",
        "enum E { A { x: i32 }, B } function make(): E { return E.B; } function f(): i32 { return match (make()) { E.A {} => make().x, E.B => 0 }; }",
    ] {
        inspect(source, |_, result| {
            assert_eq!(
                result.unwrap_err().kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
        });
    }
}

#[test]
fn captured_data_snapshots_and_both_expectation_modes_reuse_original_graph_facts() {
    for test_mode in [false, true] {
        let source = "type R = { café: i32 };\r\nfunction make(): R { return R { café: 1 }; }\r\nlet item = make();\r\nexpect (make().café == 1);\r\n";
        let fixture = Fixture::new(source);
        let graph = fixture.load_mode(test_mode);
        let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
        let declarations =
            check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
        fixture.write("app/main.rill", "changed invalid source");
        fixture.write("std/core.rill", "changed invalid prelude");
        let first = check_data(&declarations, AnalysisLimits::default()).unwrap();
        let second = check_data(&declarations, AnalysisLimits::default()).unwrap();
        assert_eq!(first.statistics(), second.statistics());
        let module = graph
            .modules()
            .iter()
            .find(|m| m.key.path == "main.rill")
            .unwrap();
        let node = module
            .syntax
            .nodes
            .iter()
            .position(|n| n.kind == compiler::parser::NodeKind::Construction)
            .unwrap();
        let fact = first.node(module, node).unwrap().unwrap();
        assert_eq!(fact.site.span.bytes(), "R { café: 1 }".as_bytes());
    }
}

#[test]
fn flat_match_arm_constraints_remain_iterative_under_small_depth_limits() {
    let expression = format!("1i32{}", " + 1".repeat(2048));
    let fixture = Fixture::new(&format!(
        "function f(b: bool): i32 {{ let n = match (b) {{ _ => {expression} }}; return n; }}"
    ));
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    check_data(
        &declarations,
        AnalysisLimits {
            max_depth: 8,
            ..AnalysisLimits::default()
        },
    )
    .unwrap();
}

#[test]
fn established_sibling_applications_supply_immediate_context_without_field_inference() {
    inspect(
        "enum E<T> { A, B } function f(b: bool): void { let first = if (b) E<i32>.A else E.B; let second = match (b) { true => E<i32>.A, false => E.B }; }",
        |_, result| {
            result.unwrap();
        },
    );
}

#[test]
fn implicit_and_explicit_dereference_views_identify_the_same_canonical_place() {
    for access in ["r.x", "(*r).x"] {
        inspect(
            &format!(
                "enum E {{ A {{ x: i32 }}, B }} function f(r: &E): i32 {{ return match (*r) {{ E.A {{}} => {access}, E.B => 0 }}; }}"
            ),
            |_, result| {
                result.unwrap();
            },
        );
    }
}

#[test]
fn nested_matches_supply_established_context_before_finalizing_fresh_sibling_arms() {
    inspect(
        "function f(b: bool, c: bool): i32 { let x = match (b) { true => 1, false => match (c) { true => 2i32, false => 3i32 } }; return x; }",
        |_, result| {
            result.unwrap();
        },
    );
    inspect(
        "type Box<T> = { value: T }; function f(b: bool, c: bool): Box<i32> { let x = match (b) { true => Box { value: 1 }, false => match (c) { true => Box<i32> { value: 2 }, false => Box<i32> { value: 3 } } }; return x; }",
        |_, result| {
            result.unwrap();
        },
    );
}

#[test]
fn nested_context_preparation_preserves_borrowed_binder_scopes_and_declared_return_targets() {
    inspect(
        "type R = { text: string }; function f(r: &R, b: bool): &string { let y = 1i32; let x = match (b) { true => &r.text, false => match (r) { R { text: y } => y } }; return x; }",
        |_, result| {
            result.unwrap();
        },
    );
    inspect(
        "function f(b: bool, c: bool): i32 { let x = match (b) { true => 1, false => match (c) { true => 2i32, false => { return 3; } } }; return x; }",
        |_, result| {
            result.unwrap();
        },
    );
}

#[test]
fn empty_nested_matches_report_the_depth_needed_for_inclusive_limits() {
    let fixture = Fixture::new(
        "enum Empty {} function f(x: Empty): i32 { return match (match (match (x) {}) {}) {}; }",
    );
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let statistics = check_data(&declarations, AnalysisLimits::default())
        .unwrap()
        .statistics();
    check_data(
        &declarations,
        AnalysisLimits {
            max_work: statistics.work,
            max_entries: statistics.entries,
            max_depth: statistics.max_depth,
        },
    )
    .unwrap();
}

#[test]
fn repeated_nested_pattern_preparation_cannot_turn_unresolved_classification_into_missing_names() {
    inspect(
        "type Point = { n: i32 }; type Box = { point: Point }; function f(b: bool, r: &Box): i32 { let x = match (b) { true => 1i32, false => match (r) { Box { point } => { let y = point; return 0; } } }; return x; }",
        |_, result| {
            let e = result.unwrap_err();
            assert_eq!(
                e.kind,
                compiler::analysis::scalars::ScalarFailureKind::Incomplete
            );
            assert!(!e.partial.findings.iter().any(|f| f.kind == compiler::analysis::scalars::ScalarFindingKind::MissingValue));
        },
    );
}

#[test]
fn named_record_fields_supply_literal_context_and_retain_canonical_order() {
    inspect(
        "type Point = { x: i32; y: bool }; function make(): Point { return Point { y: true, x: 1 }; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let node = module
                .syntax
                .nodes
                .iter()
                .position(|n| n.kind == compiler::parser::NodeKind::Construction)
                .unwrap();
            let NodeFactKind::Expression(e) = &result.node(module, node).unwrap().unwrap().kind
            else {
                panic!("constructor")
            };
            let DataExpression::Construction(c) = e.data.as_ref().unwrap() else {
                panic!("construction facts")
            };
            let point = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("Point"))
                .unwrap();
            assert!(
                matches!(e.ty.as_ref(), TypeTerm::Nominal { declaration, arguments } if **declaration == point.id && arguments.is_empty())
            );
            assert_eq!(
                c.fields
                    .iter()
                    .map(|f| f.field.name.as_deref().unwrap())
                    .collect::<Vec<_>>(),
                ["y", "x"]
            );
            let literal = c.fields[1].expression;
            let NodeFactKind::Expression(value) =
                &result.node(module, literal).unwrap().unwrap().kind
            else {
                panic!("field value")
            };
            assert!(matches!(value.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"));
        },
    );
}
