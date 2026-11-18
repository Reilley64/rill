//! M4.3 public scalar/control assertions, not Rill execution or ownership acceptance.
use compiler::analysis::declarations::{Contract, PreludeContracts, Schema, check_declarations};
use compiler::analysis::scalars::{BodyOwner, LiteralValue, NodeFactKind, check_scalars};
use compiler::analysis::{AnalysisLimits, Stage, analyze_declaration_types};
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
            "rill-m4-3-{}-{}",
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
            target: "scalar-test".into(),
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
            compiler::analysis::scalars::ScalarReceipt<'_, '_>,
            compiler::analysis::scalars::ScalarFailure<'_>,
        >,
    ),
) {
    let fixture = Fixture::new(source);
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    check(
        &graph,
        check_scalars(&declarations, AnalysisLimits::default()),
    );
}

#[test]
fn lexical_bindings_keep_canonical_targets_and_initializer_sees_outer_binding() {
    inspect(
        "function f(x: i16): i16 { let y: i16 = x; { let y = y; } return y; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let targets: Vec<_> = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == compiler::parser::NodeKind::ValueReference)
                .map(
                    |(node, _)| match &result.node(module, node).unwrap().unwrap().kind {
                        NodeFactKind::Expression(e) => e.binding.unwrap().start,
                        _ => panic!("value expression"),
                    },
                )
                .collect();
            assert_eq!(targets, [11, 30, 30]);
        },
    );
}

#[test]
fn primitive_constraints_wait_for_independently_typed_operands_before_literal_finalization() {
    inspect(
        "function f(): void { let x = 1 + 2i32; let y = 3i16 + 4; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let widths: Vec<_> = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == compiler::parser::NodeKind::Literal)
                .map(
                    |(node, _)| match &result.node(module, node).unwrap().unwrap().kind {
                        NodeFactKind::Expression(e) => match e.ty.as_ref() {
                            compiler::analysis::TypeTerm::Primitive(p) => p.name(),
                            _ => panic!("primitive"),
                        },
                        _ => panic!("expression"),
                    },
                )
                .collect();
            assert_eq!(widths, ["i32", "i32", "i16", "i16"]);
        },
    );
}

#[test]
fn directly_negated_signed_minimum_and_float_rounding_keep_literal_evidence() {
    inspect(
        "function f(): void { let x: i8 = -(128); let z = -1e-999f32; let r = 16777217f32; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let evidence: Vec<_> = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == compiler::parser::NodeKind::Literal)
                .map(
                    |(node, _)| match &result.node(module, node).unwrap().unwrap().kind {
                        NodeFactKind::Expression(e) => e.literal.unwrap(),
                        _ => panic!("literal"),
                    },
                )
                .collect();
            assert_eq!(
                evidence[0].value,
                LiteralValue::Integer {
                    magnitude: 128,
                    negative: true
                }
            );
            assert!(evidence[0].negation.is_some());
            assert_eq!(
                evidence[1].value,
                LiteralValue::Float32 { bits: 0x80000000 }
            );
            assert_eq!(
                evidence[2].value,
                LiteralValue::Float32 { bits: 0x4b800000 }
            );
        },
    );
}

#[test]
fn explicit_calls_retain_one_correlated_substitution_and_parameter_literal_context() {
    inspect(
        "function pair<T>(a: T, b: T): T { return a; } function f(): i32 { return pair<i32>(1, 2); }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let call_node = module
                .syntax
                .nodes
                .iter()
                .position(|n| n.kind == compiler::parser::NodeKind::Call)
                .unwrap();
            let NodeFactKind::Expression(e) =
                &result.node(module, call_node).unwrap().unwrap().kind
            else {
                panic!("call")
            };
            let call = e.call.as_ref().unwrap();
            let pair = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("pair"))
                .unwrap();
            let binder = module
                .declarations
                .iter()
                .find(|d| d.id.kind == compiler::loader::DeclarationKind::TypeParameter)
                .unwrap();
            assert_eq!(call.callee, &pair.id);
            assert_eq!(call.substitution.len(), 1);
            assert_eq!(call.substitution[0].binder, &binder.id);
            assert_eq!(call.arguments.len(), 2);
            for &arg in &call.arguments {
                let NodeFactKind::Expression(argument) =
                    &result.node(module, arg).unwrap().unwrap().kind
                else {
                    panic!("argument")
                };
                assert_eq!(argument.ty, call.substitution[0].argument);
            }
        },
    );
}

#[test]
fn references_preserve_descriptor_and_referent_access_and_assignment_context() {
    inspect(
        "function f(p: &mut i16): i16 { let mut x: i16 = 1; x += 2; let r = &mut x; *p = *r; return *p; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let derefs: Vec<_> = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == compiler::parser::NodeKind::Prefix("*"))
                .map(
                    |(node, _)| match &result.node(module, node).unwrap().unwrap().kind {
                        NodeFactKind::Expression(e) => e.place,
                        _ => panic!("place"),
                    },
                )
                .collect();
            assert_eq!(derefs, [compiler::analysis::scalars::Place::Writable; 3]);
        },
    );
}

#[test]
fn literal_true_loop_proves_never_and_short_circuit_retains_a_normal_boolean_result() {
    inspect(
        "function stop(): never { while (true) {} } function f(): bool { return false && stop(); }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let stop = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("stop"))
                .unwrap();
            let body = result
                .body(module, BodyOwner::Declaration(&stop.id))
                .unwrap()
                .unwrap();
            assert!(!body.flow.normal);
            assert!(body.flow.diverges);
            let node = module
                .syntax
                .nodes
                .iter()
                .position(|n| n.kind == compiler::parser::NodeKind::Binary("&&"))
                .unwrap();
            let fact = result.node(module, node).unwrap().unwrap();
            assert!(fact.flow.normal);
            let NodeFactKind::Expression(expression) = &fact.kind else {
                panic!("logical")
            };
            assert_eq!(
                expression.boolean,
                Some(compiler::analysis::scalars::BooleanSet::FALSE)
            );
        },
    );
}

#[test]
fn conditional_noncontinuation_and_loop_transfers_survive_flow_merges() {
    inspect(
        "function stop(): never { while (true) {} } function f(b: bool): bool { return b && stop(); } function g(b: bool): void { while (b) { if (b) { continue; } } }",
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
                .position(|n| n.kind == compiler::parser::NodeKind::Binary("&&"))
                .unwrap();
            let flow = &result.node(module, node).unwrap().unwrap().flow;
            assert!(flow.normal && flow.diverges);
            let g = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("g"))
                .unwrap();
            assert!(
                result
                    .body(module, BodyOwner::Declaration(&g.id))
                    .unwrap()
                    .unwrap()
                    .flow
                    .continues
                    .is_empty()
            );
        },
    );
}

#[test]
fn scalar_conditional_joins_contextualize_literals_and_omit_only_canonical_never() {
    inspect(
        "function stop(): never { while (true) {} } function f(b: bool): i32 { return if (b) 1 else stop(); } function g(b: bool): void { let x = if (b) 2 else 3i16; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let joined: Vec<_> = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == compiler::parser::NodeKind::IfExpression)
                .map(
                    |(node, _)| match &result.node(module, node).unwrap().unwrap().kind {
                        NodeFactKind::Expression(e) => match e.ty.as_ref() {
                            compiler::analysis::TypeTerm::Primitive(p) => p.name(),
                            _ => panic!("scalar"),
                        },
                        _ => panic!("join"),
                    },
                )
                .collect();
            assert_eq!(joined, ["i32", "i16"]);
        },
    );
}

#[test]
fn startup_and_expectations_are_typed_in_both_modes_without_running_helpers() {
    for test_mode in [false, true] {
        let fixture = Fixture::new(
            "let x: i16 = 1; function helper(): bool { return true; } expect helper(); expect false && (1i32 / 0i32 == 0i32);",
        );
        let graph = fixture.load_mode(test_mode);
        let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
        let declarations =
            check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
        let result = check_scalars(&declarations, AnalysisLimits::default()).unwrap();
        let module = graph
            .modules()
            .iter()
            .find(|m| m.key.path == "main.rill")
            .unwrap();
        assert!(
            result
                .body(module, BodyOwner::Startup)
                .unwrap()
                .unwrap()
                .flow
                .normal
        );
        let expectations: Vec<_> = module
            .declarations
            .iter()
            .filter(|d| d.id.kind == compiler::loader::DeclarationKind::Expectation)
            .collect();
        assert_eq!(expectations.len(), 2);
        for expectation in expectations {
            assert!(
                result
                    .body(module, BodyOwner::Declaration(&expectation.id))
                    .unwrap()
                    .is_some()
            );
        }
        let helper_call = module
            .syntax
            .nodes
            .iter()
            .position(|n| n.kind == compiler::parser::NodeKind::Call)
            .unwrap();
        let NodeFactKind::Expression(e) = &result.node(module, helper_call).unwrap().unwrap().kind
        else {
            panic!("helper")
        };
        assert_eq!(
            e.boolean,
            Some(compiler::analysis::scalars::BooleanSet::BOTH)
        );
    }
}

#[test]
fn never_operands_do_not_erase_intermediate_declared_result_mismatches() {
    inspect(
        "function stop(): never { while (true) {} } function f(x: i32): i32 { return x; } function g(x: bool): void {} function bad(): void { g(f(stop())); }",
        |_, result| {
            let failure = result.unwrap_err();
            assert_eq!(
                failure.kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
            let mismatch = failure
                .partial
                .findings
                .iter()
                .find(|f| f.site.span.bytes() == b"f(stop())")
                .unwrap();
            assert!(
                matches!(mismatch.actual.as_ref().unwrap().as_ref(), compiler::analysis::TypeTerm::Primitive(p) if p.name() == "i32")
            );
            assert!(
                matches!(mismatch.expected.as_ref().unwrap().as_ref(), compiler::analysis::TypeTerm::Primitive(p) if p.name() == "bool")
            );
        },
    );
    inspect("expect 1i32;", |_, result| {
        assert!(
            result
                .unwrap_err()
                .partial
                .findings
                .iter()
                .any(|f| f.kind.code() == Some("R0101"))
        );
    });
    inspect("function f(): bool { return 1; }", |_, result| {
        assert!(
            result
                .unwrap_err()
                .partial
                .findings
                .iter()
                .any(|f| f.kind.code() == Some("R0003"))
        );
    });
}

#[test]
fn uninterpolated_templates_and_character_text_retain_m1_decoding_and_original_bytes() {
    inspect(
        "function f(): void { let a = `é`; let b = '\\u{1F4A1}'; let c = r#\"raw\"#; }",
        |graph, result| {
            let result = result.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            for (node, syntax) in module.syntax.nodes.iter().enumerate().filter(|(_, n)| {
                matches!(
                    n.kind,
                    compiler::parser::NodeKind::Template | compiler::parser::NodeKind::Literal
                )
            }) {
                let NodeFactKind::Expression(e) = &result.node(module, node).unwrap().unwrap().kind
                else {
                    panic!("literal")
                };
                let evidence = e.literal.as_ref().unwrap();
                if syntax.kind == compiler::parser::NodeKind::Template {
                    assert_eq!(evidence.value, LiteralValue::StaticTemplate);
                } else {
                    assert_eq!(evidence.token.unwrap().span, syntax.span);
                }
            }
        },
    );
}

#[test]
fn unresolved_family_calls_keep_canonical_identity_without_false_missing_literal_context() {
    inspect(
        "function family(x: i32): i32; function f(): i32 { return family(1); }",
        |graph, result| {
            let failure = result.unwrap_err();
            assert_eq!(
                failure.kind,
                compiler::analysis::scalars::ScalarFailureKind::Incomplete
            );
            let family = graph
                .modules()
                .iter()
                .flat_map(|m| &m.declarations)
                .find(|d| d.name.as_deref() == Some("family"))
                .unwrap();
            assert!(
                failure
                    .partial
                    .obligations
                    .iter()
                    .any(|o| o.declaration == Some(&family.id))
            );
            assert!(
                !failure
                    .partial
                    .findings
                    .iter()
                    .any(|f| f.kind.code() == Some("R0003"))
            );
        },
    );
    let fixture = Fixture::new("import util as u; function f(): i32 { return u::secret(); }");
    fixture.write("app/util.rill", "function secret(): i32 { return 1; }");
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let failure = check_scalars(&declarations, AnalysisLimits::default()).unwrap_err();
    let private = failure
        .partial
        .findings
        .iter()
        .find(|f| f.kind.code() == Some("R0401"))
        .unwrap();
    assert_eq!(private.bindings.len(), 2);
    assert_eq!(private.bindings[0].name, "u");
    assert_eq!(private.bindings[1].name, "secret");
}

#[test]
fn every_integer_width_checks_exact_boundaries_without_a_numeric_default() {
    for (width, maximum, overflow, minimum) in [
        ("i8", "127", "128", Some("-128")),
        ("i16", "32767", "32768", Some("-32768")),
        ("i32", "2147483647", "2147483648", Some("-2147483648")),
        (
            "i64",
            "9223372036854775807",
            "9223372036854775808",
            Some("-9223372036854775808"),
        ),
        (
            "i128",
            "170141183460469231731687303715884105727",
            "170141183460469231731687303715884105728",
            Some("-170141183460469231731687303715884105728"),
        ),
        ("u8", "255", "256", None),
        ("u16", "65535", "65536", None),
        ("u32", "4294967295", "4294967296", None),
        ("u64", "18446744073709551615", "18446744073709551616", None),
        (
            "u128",
            "340282366920938463463374607431768211455",
            "340282366920938463463374607431768211456",
            None,
        ),
    ] {
        inspect(&format!("let x: {width} = {maximum};"), |_, r| {
            r.unwrap();
        });
        inspect(&format!("let x: {width} = {overflow};"), |_, r| {
            let failure = r.unwrap_err();
            assert_eq!(
                failure.kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
            assert!(
                failure
                    .partial
                    .findings
                    .iter()
                    .any(|f| f.kind.code() == Some("R0002"))
            );
        });
        if let Some(minimum) = minimum {
            inspect(&format!("let x: {width} = {minimum};"), |_, r| {
                r.unwrap();
            });
        }
    }
    for source in [
        "let x = 1;",
        "let x = 1.0;",
        "let x = -128;",
        "let x = 1 == 2;",
        "let mut x = 1; x = 2i32;",
    ] {
        inspect(source, |_, r| {
            assert!(
                r.unwrap_err()
                    .partial
                    .findings
                    .iter()
                    .any(|f| f.kind.code() == Some("R0003"))
            );
        });
    }
    for source in [
        "let x: i8 = -(128 + 0);",
        "let x: i8 = 0 - 128;",
        "let x: i8 = 128;",
        "let x: i32 = 1.5;",
        "let x: f32 = 0xff;",
        "let x = 1e999f64;",
    ] {
        inspect(source, |_, r| {
            assert!(
                r.unwrap_err()
                    .partial
                    .findings
                    .iter()
                    .any(|f| f.kind.code() == Some("R0002"))
            );
        });
    }
    for source in [
        "let x: u8 = -0;",
        "let x: i64 = 1i32;",
        "let x: bool = 1i32;",
    ] {
        inspect(source, |_, r| {
            assert!(
                r.unwrap_err()
                    .partial
                    .findings
                    .iter()
                    .any(|f| f.kind.code() == Some("R0100"))
            );
        });
    }
}

#[test]
fn closed_primitive_matrix_accepts_widths_and_mixed_text_observations_not_spelling_privilege() {
    for width in [
        "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "f32", "f64",
    ] {
        let source: String = ["+", "-", "*", "/", "%"]
            .iter()
            .enumerate()
            .map(|(n, op)| {
                format!("function f{n}(a: {width}, b: {width}): {width} {{ return a {op} b; }}")
            })
            .collect();
        inspect(&source, |_, r| {
            r.unwrap();
        });
    }
    for width in [
        "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128",
    ] {
        inspect(
            &format!(
                "function f(a: {width}, b: {width}): {width} {{ return ~(a & b | a ^ b); }} function g(a: {width}): {width} {{ return a << 1; }} function h(a: {width}): {width} {{ return a >> 1; }}"
            ),
            |_, r| {
                r.unwrap();
            },
        );
    }
    let mut text = String::new();
    let mut n = 0;
    for a in ["string", "&string", "&mut string"] {
        for b in ["string", "&string", "&mut string"] {
            for op in ["==", "!=", "<", "<=", ">", ">="] {
                text += &format!("function f{n}(a: {a}, b: {b}): bool {{ return a {op} (b); }}");
                n += 1;
            }
        }
    }
    inspect(&text, |_, r| {
        r.unwrap();
    });
    inspect(
        "function f(a: char, b: char): bool { return a < b || a == b; } type i32 = {}; let x = 1i32;",
        |_, r| {
            r.unwrap();
        },
    );
    for source in [
        "function f(a: &i32, b: &i32): bool { return a == b; }",
        "function f(a: &&string, b: &string): bool { return a == b; }",
        "function f(a: bool, b: bool): bool { return a < b; }",
        "function f(a: f32, b: f32): f32 { return a & b; }",
        "function f(a: i32, b: i64): bool { return a == b; }",
        "function f(a: i32, b: i64): i32 { return a + b; }",
        "type i32 = {}; function f(a: i32): bool { return a == a; }",
        "function f<T>(a: T): T { return a + a; }",
    ] {
        inspect(source, |_, r| {
            assert_eq!(
                r.unwrap_err().kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
        });
    }
    inspect(
        "function f(a: &&string, b: &string): bool { return *a == b; }",
        |_, r| {
            r.unwrap();
        },
    );
}

#[test]
fn finite_return_and_guard_proofs_keep_static_errors_and_actual_loop_targets() {
    for source in [
        "function f(b: bool): i32 { if (b) { return 1; } else { return 2; } }",
        "function f(b: bool): i32 { guard b else { return 1; } return 2; }",
        "function f(): never { let b = true; while (!(!b)) {} }",
        "function f(): never { while (false || true && true) {} }",
        "function f(b: bool): void { while (true) { guard b else { break; } continue; } }",
        "function stop(): never { while (true) {} } function f(b: bool): i32 { guard b else { stop(); } return 1; }",
        "function f(): i8 { return 127i8 + 1i8; } let x = 1i32 / 0i32; let y = 1i8 << 33; let z = -(-128i8);",
    ] {
        inspect(source, |_, r| {
            r.unwrap();
        });
    }
    for (source, kind) in [
        (
            "function choose(b: bool): i32 { if (b) { return 1i32; } if (!b) { return 2i32; } }",
            compiler::analysis::scalars::ScalarFindingKind::ReturnCoverage,
        ),
        (
            "function f(): never { let mut b = true; while (b) {} }",
            compiler::analysis::scalars::ScalarFindingKind::ReturnCoverage,
        ),
        (
            "function f(): never { while (1i32 == 1i32) {} }",
            compiler::analysis::scalars::ScalarFindingKind::ReturnCoverage,
        ),
        (
            "function stop(): never { while (true) {} } function f(b: bool): never { while (true) { if (b) { break; } stop(); } }",
            compiler::analysis::scalars::ScalarFindingKind::ReturnCoverage,
        ),
        (
            "function f(b: bool): void { guard b else { while (true) { break; } } }",
            compiler::analysis::scalars::ScalarFindingKind::GuardFallthrough,
        ),
        (
            "function f(): void { break; }",
            compiler::analysis::scalars::ScalarFindingKind::InvalidTarget,
        ),
        (
            "function f(): void { continue; }",
            compiler::analysis::scalars::ScalarFindingKind::InvalidTarget,
        ),
        (
            "return;",
            compiler::analysis::scalars::ScalarFindingKind::InvalidTarget,
        ),
        (
            "function f(): void { return; let x = 128i8; }",
            compiler::analysis::scalars::ScalarFindingKind::LiteralRange,
        ),
        (
            "function f(): void { while (false) { let x = 128i8; } }",
            compiler::analysis::scalars::ScalarFindingKind::LiteralRange,
        ),
        (
            "function f(): never { if (false) { return 1i32; } while (true) {} }",
            compiler::analysis::scalars::ScalarFindingKind::TypeMismatch,
        ),
        (
            "type never = {}; function fake(): never { while (true) {} } function f(): i32 { fake(); }",
            compiler::analysis::scalars::ScalarFindingKind::ReturnCoverage,
        ),
    ] {
        inspect(source, |_, r| {
            let f = r.unwrap_err();
            assert_eq!(
                f.kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
            assert!(f.partial.findings.iter().any(|f| f.kind == kind));
        });
    }
}

#[test]
fn explicit_type_arguments_cannot_bypass_a_local_value_shadowing_a_standalone() {
    inspect(
        "function f<T>(x: T): T { return x; } function g(f: i32): i32 { return f<i32>(1); }",
        |_, r| {
            assert_eq!(
                r.unwrap_err().kind,
                compiler::analysis::scalars::ScalarFailureKind::Incomplete
            );
        },
    );
    inspect(
        "function g(f: i32): i32 { let x = f<i32>; return x; }",
        |_, r| {
            assert_eq!(
                r.unwrap_err().kind,
                compiler::analysis::scalars::ScalarFailureKind::Incomplete
            );
        },
    );
}

#[test]
fn failed_initializers_preserve_binder_causes_and_do_not_invent_missing_names() {
    inspect(
        "function f(): void { let x = 1; let x = 2i32; let y = x; }",
        |_, r| {
            let failure = r.unwrap_err();
            assert_eq!(
                failure.kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
            assert!(failure.partial.findings.iter().any(
                |f| f.kind == compiler::analysis::scalars::ScalarFindingKind::DuplicateBinding
            ));
            assert!(!failure.partial.findings.iter().any(|f| f.kind == compiler::analysis::scalars::ScalarFindingKind::MissingValue));
            assert!(failure.partial.nodes.iter().any(|n| matches!(&n.kind, NodeFactKind::Binding(b) if b.ty.is_none() && b.blocked_by.is_some())));
        },
    );
    for source in [
        "let y = x; let x = 1i32;",
        "function f(): void { let x = x; }",
    ] {
        inspect(source, |_, r| {
            assert!(
                r.unwrap_err()
                    .partial
                    .findings
                    .iter()
                    .any(|f| f.kind.code() == Some("R0400"))
            );
        });
    }
    for source in [
        "let x = true; expect x;",
        "let x = 1i32; function f(): i32 { return x; }",
    ] {
        inspect(source, |_, r| {
            assert!(
                r.unwrap_err()
                    .partial
                    .findings
                    .iter()
                    .any(|f| f.kind.code() == Some("R0502"))
            );
        });
    }
    for source in [
        "function f<T>(T: T): T { let x: T = T; return x; }",
        "let x = 1i32; function helper(x: i32): i32 { return x; } let y = helper(x);",
    ] {
        inspect(source, |_, r| {
            r.unwrap();
        });
    }
    inspect(
        "function pair<T>(a: T, b: T): T { return a; } function f(): i32 { return pair<i32>(1, true); }",
        |_, r| {
            assert!(
                r.unwrap_err()
                    .partial
                    .findings
                    .iter()
                    .any(|f| f.kind.code() == Some("R0100"))
            );
        },
    );
}

#[test]
fn unsupported_source_never_completes_or_proves_missing_names_or_numeric_context() {
    for source in [
        "let x = [1];",
        "type R = { n: i32 }; let x = R { n: 1 };",
        "function f(): void { return; let x = [1i32]; }",
        "function f(): void { let x = () => { return true; }; }",
        "function f(value: i32): void { guard let x = value else { return; } let y = x; }",
        "function f(value: [i32, bool]): void { let [x, y] = value; let z = x; }",
        "function f(): void { helper(); function helper(): void {} }",
        "function f<T>(x: T): T { return x; } let x = f(1);",
        "function f(work: (i32) => bool): bool { return work(1); }",
        "function f(): i32 { return 1; } let x = f;",
        "function f(): bool { return noBorrow<i32>(); }",
        "function f(): void { let x = `value ${1}`; }",
        "function f(): void { let x = &mut 1i32; }",
        "function f(): i32 { return 1; } let x = f()?;",
    ] {
        inspect(source, |_, r| {
            let failure = r.unwrap_err();
            assert_eq!(
                failure.kind,
                compiler::analysis::scalars::ScalarFailureKind::Incomplete,
                "{source}"
            );
            assert!(!failure.complete_diagnostics);
            assert!(
                !failure.partial.findings.iter().any(|f| matches!(
                    f.kind,
                    compiler::analysis::scalars::ScalarFindingKind::MissingValue
                        | compiler::analysis::scalars::ScalarFindingKind::NumericContext
                )),
                "{source}"
            );
        });
    }
}

#[test]
fn body_annotation_nodes_expose_original_symbolic_type_uses_without_mutating_input() {
    let fixture = Fixture::new("function f<T>(x: T): T { let y: T = x; return y; }");
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let node = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .rfind(|(_, n)| n.kind == compiler::parser::NodeKind::NamedType)
        .unwrap()
        .0;
    assert!(types.type_use(module, node).unwrap().is_none());
    let result = check_scalars(&declarations, AnalysisLimits::default()).unwrap();
    let NodeFactKind::Annotation(annotation) = &result.node(module, node).unwrap().unwrap().kind
    else {
        panic!("annotation")
    };
    let binder = module
        .declarations
        .iter()
        .find(|d| d.id.kind == compiler::loader::DeclarationKind::TypeParameter)
        .unwrap();
    assert!(
        matches!(annotation.ty.as_ref(), compiler::analysis::TypeTerm::Parameter { declaration, position: 0 } if **declaration == binder.id)
    );
    assert_eq!(annotation.span, &module.syntax.nodes[node].span);
    assert!(types.type_use(module, node).unwrap().is_none());
}

#[test]
fn limits_are_inclusive_and_resource_exhaustion_retains_known_findings() {
    let fixture = Fixture::new("function f(x: i32): i32 { let y: i32 = x + 1; return y; }");
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let stats = check_scalars(&declarations, AnalysisLimits::default())
        .unwrap()
        .statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    assert_eq!(
        check_scalars(&declarations, exact).unwrap().statistics(),
        stats
    );
    for limits in [
        AnalysisLimits {
            max_work: stats.work - 1,
            ..exact
        },
        AnalysisLimits {
            max_entries: stats.entries - 1,
            ..exact
        },
        AnalysisLimits {
            max_depth: stats.max_depth - 1,
            ..exact
        },
    ] {
        let failure = check_scalars(&declarations, limits).unwrap_err();
        assert_eq!(
            failure.kind,
            compiler::analysis::scalars::ScalarFailureKind::ResourceLimit
        );
        assert!(!failure.complete_diagnostics);
    }
    let source = format!(
        "function bad(): void {{ let x = 128i8; }} function more(): void {{ let x = 1i32{}; }}",
        " + 1".repeat(2048)
    );
    let fixture = Fixture::new(&source);
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let failure = check_scalars(
        &declarations,
        AnalysisLimits {
            max_work: 1000,
            ..AnalysisLimits::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        failure.kind,
        compiler::analysis::scalars::ScalarFailureKind::ResourceLimit
    );
    assert!(
        failure
            .partial
            .findings
            .iter()
            .any(|f| f.kind.code() == Some("R0002"))
    );
}

#[test]
fn long_flat_expression_chains_do_not_consume_host_recursion_depth() {
    let source = format!(
        "function f(): i32 {{ return 1i32{}; }}",
        " + 1".repeat(2048)
    );
    let fixture = Fixture::new(&source);
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let result = check_scalars(
        &declarations,
        AnalysisLimits {
            max_work: 4_000_000,
            max_entries: 1_000_000,
            max_depth: 8,
        },
    )
    .unwrap_or_else(|f| panic!("{:?} {:?}", f.kind, f.statistics));
    assert!(result.statistics().max_depth <= 8);
}

#[test]
fn permitted_unimplemented_type_relations_are_not_fabricated_mismatches() {
    for source in [
        "enum E { A, B } function f(a: E.A): E { return a; }",
        "enum E { A, B } function f(a: &E.A): &E { return a; }",
        "enum E { A, B } function f(b: bool, a: E.A, c: E.B): void { let x = if (b) a else c; }",
        "function f(a: (i32) => bool): once(i32) => bool { return a; }",
    ] {
        inspect(source, |_, r| {
            let failure = r.unwrap_err();
            assert_eq!(
                failure.kind,
                compiler::analysis::scalars::ScalarFailureKind::Incomplete
            );
            assert!(
                failure
                    .partial
                    .obligations
                    .iter()
                    .any(|o| o.reason
                        == compiler::analysis::scalars::ScalarUnsupported::TypeRelation)
            );
            assert!(
                !failure
                    .partial
                    .findings
                    .iter()
                    .any(|f| f.kind.code() == Some("R0100"))
            );
        });
    }
    inspect(
        "enum E { A, B } function f(a: &mut E.A): &mut E { return a; }",
        |_, r| {
            assert_eq!(
                r.unwrap_err().kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
        },
    );
    for source in [
        "function f(n: i32): i32 { return f(n); }",
        "function f<T>(x: T): never { return f<&T>(&x); }",
    ] {
        inspect(source, |_, r| {
            r.unwrap();
        });
    }
}

#[test]
fn captured_unicode_crlf_facts_survive_disk_changes_and_foreign_queries_reject() {
    let source = "function café(): char {\r\n return '\\u{1F4A1}';\r\n}";
    let fixture = Fixture::new(source);
    let graph = fixture.load();
    fixture.write("app/main.rill", "invalid changed source");
    fixture.write("std/core.rill", "invalid changed prelude");
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let result = check_scalars(&declarations, AnalysisLimits::default()).unwrap();
    let repeated = check_scalars(&declarations, AnalysisLimits::default()).unwrap();
    assert_eq!(result.statistics(), repeated.statistics());
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let node = module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == compiler::parser::NodeKind::Literal)
        .unwrap();
    let fact = result.node(module, node).unwrap().unwrap();
    assert_eq!(fact.site.span.start(), source.find('\'').unwrap());
    let NodeFactKind::Expression(e) = &fact.kind else {
        panic!("literal")
    };
    let compiler::lexer::TokenKind::Literal(decoded) = &e.literal.unwrap().token.unwrap().kind
    else {
        panic!("decoded M1 literal")
    };
    assert_eq!(decoded.decoded[0].value, '💡');
    assert_eq!(decoded.decoded[0].source.bytes(), b"\\u{1F4A1}");
    assert_eq!(fact.site.span.snapshot().id(), module.snapshot.id());
    let foreign_fixture = Fixture::new(source);
    let foreign = foreign_fixture.load();
    let foreign_module = foreign
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let foreign_function = foreign_module
        .declarations
        .iter()
        .find(|d| d.id.kind == compiler::loader::DeclarationKind::Function)
        .unwrap();
    assert!(result.node(foreign_module, node).is_err());
    assert!(result.node(module, usize::MAX).is_err());
    assert!(
        result
            .body(module, BodyOwner::Declaration(&foreign_function.id))
            .is_err()
    );
}

#[test]
fn imported_startup_is_checked_in_both_modes_and_local_copy_names_gain_no_core_role() {
    for test_mode in [false, true] {
        let fixture = Fixture::new("import util as u; let x = u::helper(1);");
        fixture.write(
            "app/util.rill",
            "export function helper(x: i16): i16 { return x; } let invalid = 128i8;",
        );
        let graph = fixture.load_mode(test_mode);
        let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
        let declarations =
            check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
        let failure = check_scalars(&declarations, AnalysisLimits::default()).unwrap_err();
        assert!(
            failure
                .partial
                .findings
                .iter()
                .any(|f| f.site.module.path == "util.rill" && f.kind.code() == Some("R0002"))
        );
        let call = failure
            .partial
            .nodes
            .iter()
            .find_map(|n| match &n.kind {
                NodeFactKind::Expression(e) => e.call.as_ref(),
                _ => None,
            })
            .unwrap();
        assert_eq!(call.callee.module.path, "util.rill");
        assert_eq!(call.bindings[0].name, "u");
        assert_eq!(call.bindings[1].name, "helper");
    }
    inspect(
        "function copy(x: i32): i32 { return x; } let x = copy(1);",
        |graph, r| {
            let r = r.unwrap();
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let node = module
                .syntax
                .nodes
                .iter()
                .position(|n| n.kind == compiler::parser::NodeKind::Call)
                .unwrap();
            let NodeFactKind::Expression(e) = &r.node(module, node).unwrap().unwrap().kind else {
                panic!("call")
            };
            assert_ne!(e.call.as_ref().unwrap().callee, &graph.prelude().copy);
            assert!(r.not_performed().contains(&Stage::Ownership));
        },
    );
}

#[test]
fn never_admission_cannot_select_an_unsigned_unary_minus_contract() {
    for width in ["u8", "u16", "u32", "u64", "u128"] {
        inspect(
            &format!("function stop(): never {{ while (true) {{}} }} let x: {width} = -stop();"),
            |_, result| {
                let failure = result.unwrap_err();
                assert_eq!(
                    failure.kind,
                    compiler::analysis::scalars::ScalarFailureKind::SourceRejected
                );
                assert!(
                    failure
                        .partial
                        .findings
                        .iter()
                        .any(|f| f.site.span.bytes() == b"-stop()"
                            && f.kind
                                == compiler::analysis::scalars::ScalarFindingKind::TypeMismatch)
                );
            },
        );
    }
    for width in ["i8", "i16", "i32", "i64", "i128", "f32", "f64"] {
        inspect(
            &format!("function stop(): never {{ while (true) {{}} }} let x: {width} = -stop();"),
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
                    .position(|n| n.kind == compiler::parser::NodeKind::Prefix("-"))
                    .unwrap();
                let fact = result.node(module, node).unwrap().unwrap();
                let NodeFactKind::Expression(expression) = &fact.kind else {
                    panic!("negation")
                };
                assert!(
                    matches!(expression.ty.as_ref(), compiler::analysis::TypeTerm::Primitive(p) if p.name() == width)
                );
                assert!(!fact.flow.normal);
            },
        );
    }
}

#[test]
fn never_alone_does_not_choose_a_primitive_operand_contract() {
    inspect(
        "function stop(): never { while (true) {} } let x = stop() == stop();",
        |_, r| {
            let failure = r.unwrap_err();
            assert_eq!(
                failure.kind,
                compiler::analysis::scalars::ScalarFailureKind::SourceRejected
            );
            assert!(
                failure.partial.findings.iter().any(|f| f.kind
                    == compiler::analysis::scalars::ScalarFindingKind::InsufficientContext)
            );
        },
    );
}

#[test]
fn return_context_establishes_literal_width_and_original_integer_evidence() {
    let fixture = Fixture::new("function f(): i16 { return 7; }");
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let result = check_scalars(&declarations, AnalysisLimits::default()).unwrap();
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let (node, syntax) = module
        .syntax
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.kind == compiler::parser::NodeKind::Literal)
        .unwrap();
    let fact = result.node(module, node).unwrap().unwrap();
    let NodeFactKind::Expression(expression) = &fact.kind else {
        panic!("expression fact")
    };
    assert!(
        matches!(expression.ty.as_ref(), compiler::analysis::TypeTerm::Primitive(p) if p.name() == "i16")
    );
    assert_eq!(
        expression.literal.as_ref().unwrap().value,
        LiteralValue::Integer {
            magnitude: 7,
            negative: false
        }
    );
    assert_eq!(fact.site.span, &syntax.span);
    assert_eq!(fact.site.span.snapshot().id(), module.snapshot.id());
}

#[test]
fn empty_void_body_has_scoped_completion_not_ownership_or_full_body_acceptance() {
    let fixture = Fixture::new("function f(): void {}");
    let graph = fixture.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let declarations =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let result = check_scalars(&declarations, AnalysisLimits::default()).unwrap();
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let function = module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("f"))
        .unwrap();
    let body = result
        .body(module, BodyOwner::Declaration(&function.id))
        .unwrap()
        .unwrap();
    assert!(body.flow.normal);
    assert!(result.not_performed().contains(&Stage::Ownership));
    assert!(result.not_performed().contains(&Stage::Bodies));
    assert_eq!(result.context(), graph.id());
}
