//! M4.1 declaration-type assertions, not body checking, layout or Rill execution.
use compiler::analysis::{AnalysisLimits, Stage, TypeTerm, analyze_declaration_types};
use compiler::loader::{
    Declaration, DeclarationKind, Graph, Limits, Module, Prelude, Request, Selector, load,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "rill-m4-1-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let f = Self(root);
        f.write(
            "std/core.rill",
            "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
        );
        f.write("app/main.rill", source);
        f
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn request(&self) -> Request {
        Request {
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
            target: "annotation-test".into(),
            options: BTreeMap::new(),
            test_mode: false,
            limits: Limits::default(),
        }
    }
    fn load(&self) -> Graph {
        load(self.request()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn module<'a>(graph: &'a Graph, path: &str) -> &'a Module {
    graph.modules().iter().find(|m| m.key.path == path).unwrap()
}
fn declaration<'a>(module: &'a Module, name: &str, kind: DeclarationKind) -> &'a Declaration {
    module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some(name) && d.id.kind == kind)
        .unwrap()
}

#[test]
fn multiple_type_arguments_and_same_spelled_variants_keep_their_order_and_owner() {
    let f = Fixture::new(
        "type Pair<A, B> = { first: A; second: B }; enum Left { V } enum Right { V } function use(a: Pair<i32, bool>, b: Pair<bool, i32>, c: Left.V, d: Right.V, e: Left): void {}",
    );
    let graph = f.load();
    let main = module(&graph, "main.rill");
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let fact = receipt
        .declaration(&declaration(main, "use", DeclarationKind::Function).id)
        .unwrap()
        .unwrap();
    let compiler::analysis::DeclarationShape::Function { parameters: p, .. } = &fact.shape else {
        panic!()
    };
    assert_ne!(p[0].ty, p[1].ty);
    assert_ne!(p[2].ty, p[3].ty);
    assert_ne!(p[2].ty, p[4].ty); // Exact identity does not perform variant widening.
    let node = main.syntax.nodes[p[0].declaration.node].children[1];
    let usage = receipt.type_use(main, node).unwrap().unwrap();
    assert_eq!(usage.substitution.len(), 2);
    for (position, name, expected) in [(0, "A", "i32"), (1, "B", "bool")] {
        let item = &usage.substitution[position];
        assert_eq!(item.position, position);
        assert_eq!(
            item.binder,
            &declaration(main, name, DeclarationKind::TypeParameter).id
        );
        assert!(matches!(item.argument.as_ref(), TypeTerm::Primitive(i) if i.name() == expected));
    }
}

#[test]
fn private_intermediate_modules_and_unimported_metadata_do_not_supply_type_names() {
    let f = Fixture::new("import api; function bad(value: api::hidden::T, other: T): void {}");
    f.write(
        "app/api.rill",
        "import model as hidden; export function make(): hidden::T { return hidden::T {}; }",
    );
    f.write("app/model.rill", "export type T = {};");
    let graph = f.load();
    let error = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap_err();
    let private = error
        .partial
        .findings()
        .find(|f| f.kind == compiler::analysis::FindingKind::PrivateType)
        .unwrap();
    assert_eq!(private.bindings[0].name, "hidden");
    assert!(matches!(
        private.bindings[0].target,
        compiler::loader::Target::Module(_)
    ));
    assert!(private.candidates.is_empty()); // Do not traverse the private module.
    assert!(
        error
            .partial
            .findings()
            .any(|f| f.kind == compiler::analysis::FindingKind::MissingType
                && f.span.bytes() == b"T")
    );
    assert!(
        error
            .partial
            .declarations()
            .any(|d| d.declaration.name.as_deref() == Some("make"))
    );
}

#[test]
fn long_explicit_annotation_inputs_are_bounded_without_expanding_definitions() {
    let fields = (0..800)
        .map(|i| format!("f{i}: Box<Box<i32>>"))
        .collect::<Vec<_>>()
        .join(";");
    let f = Fixture::new(&format!(
        "type Box<T> = {{ value: T }}; type Large = {{ {fields} }};"
    ));
    let graph = f.load();
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let fact = receipt
        .declaration(
            &declaration(
                module(&graph, "main.rill"),
                "Large",
                DeclarationKind::Record,
            )
            .id,
        )
        .unwrap()
        .unwrap();
    assert!(
        matches!(&fact.shape, compiler::analysis::DeclarationShape::Record { fields } if fields.len() == 800)
    );
    assert_eq!(receipt.statistics().max_depth, 3);
    let limited = analyze_declaration_types(
        &graph,
        AnalysisLimits {
            max_work: 500,
            ..AnalysisLimits::default()
        },
    )
    .unwrap_err();
    assert_eq!(limited.kind, compiler::analysis::FailureKind::ResourceLimit);
}

#[test]
fn malformed_nominal_applications_keep_the_resolved_declaration_and_import_route() {
    let f = Fixture::new(
        "import model::{Box as Crate}; function bad(value: Crate<i32, bool>): void {}",
    );
    f.write("app/model.rill", "export type Box<T> = { value: T };");
    let graph = f.load();
    let error = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap_err();
    let finding = error
        .partial
        .findings()
        .find(|f| f.kind == compiler::analysis::FindingKind::ExcessArguments)
        .unwrap();
    assert_eq!(
        finding.candidates[0].id,
        declaration(module(&graph, "model.rill"), "Box", DeclarationKind::Record).id
    );
    assert_eq!(finding.bindings[0].name, "Crate");
    assert_eq!(finding.span.bytes(), b"Crate<i32, bool>");
}

#[test]
fn all_seventeen_primitive_terms_are_canonical_and_void_never_are_distinct() {
    let names = [
        "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "f32", "f64", "bool",
        "char", "string", "void", "never",
    ];
    let source = format!(
        "function primitives(value: [{}]): void {{}}",
        names.join(",")
    );
    let f = Fixture::new(&source);
    let graph = f.load();
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let fact = receipt
        .declaration(
            &declaration(
                module(&graph, "main.rill"),
                "primitives",
                DeclarationKind::Function,
            )
            .id,
        )
        .unwrap()
        .unwrap();
    let compiler::analysis::DeclarationShape::Function { parameters, .. } = &fact.shape else {
        panic!()
    };
    let TypeTerm::Tuple(items) = parameters[0].ty.as_ref() else {
        panic!()
    };
    for (term, name) in items.iter().zip(names) {
        let TypeTerm::Primitive(intrinsic) = term.as_ref() else {
            panic!()
        };
        assert_eq!(intrinsic.name(), name);
        assert!(
            graph
                .prelude()
                .intrinsics
                .iter()
                .any(|actual| std::ptr::eq(*intrinsic, actual))
        );
    }
    assert_ne!(items[15], items[16]);
    assert!(
        receipt
            .completion()
            .not_performed
            .contains(&Stage::PositionValidity)
    );
}

#[test]
fn retained_unicode_crlf_routes_and_capture_identity_survive_live_changes() {
    use compiler::analysis::QueryError;
    let f = Fixture::new(
        "import api;\r\nfunction café(value: api::Alias): api::Alias { return value; }\r\n",
    );
    f.write("app/model.rill", "export type Café<T> = { value: T };");
    f.write("app/api.rill", "export import model::{Café as Alias};");
    // Use full arguments, as required by the approved annotation-only stage.
    f.write("app/main.rill", "import api;\r\nfunction café(value: api::Alias<i32>): api::Alias<i32> { return value; }\r\n");
    let graph = f.load();
    let other = f.load();
    let main = module(&graph, "main.rill");
    let decl = declaration(main, "café", DeclarationKind::Function);
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let fresh = analyze_declaration_types(&other, AnalysisLimits::default()).unwrap();
    assert_eq!(receipt.context(), graph.id());
    assert_ne!(receipt.context(), fresh.context());
    assert_eq!(
        receipt
            .declaration(
                &declaration(
                    module(&other, "main.rill"),
                    "café",
                    DeclarationKind::Function
                )
                .id
            )
            .unwrap_err(),
        QueryError::InvalidRequest
    );
    assert_eq!(
        receipt
            .type_use(module(&other, "main.rill"), 0)
            .unwrap_err(),
        QueryError::InvalidRequest
    );
    assert_eq!(
        receipt.type_use(main, usize::MAX).unwrap_err(),
        QueryError::InvalidRequest
    );
    let facts = receipt.declaration(&decl.id).unwrap().unwrap();
    let compiler::analysis::DeclarationShape::Function { parameters, result } = &facts.shape else {
        panic!()
    };
    let node = main.syntax.nodes[parameters[0].declaration.node].children[1];
    let usage = receipt.type_use(main, node).unwrap().unwrap();
    assert_eq!(usage.span.bytes(), b"api::Alias<i32>");
    assert_eq!(
        usage.span.start(),
        "import api;\r\nfunction café(value: ".len()
    );
    assert_eq!(usage.span.snapshot().id(), main.snapshot.id());
    assert_eq!(usage.bindings.len(), 2);
    assert_eq!(usage.bindings[1].name, "Alias");
    assert_eq!(usage.bindings[1].routes[0].module.path, "model.rill");
    assert_eq!(parameters[0].ty, *result);
    let foreign = fresh
        .declaration(
            &declaration(
                module(&other, "main.rill"),
                "café",
                DeclarationKind::Function,
            )
            .id,
        )
        .unwrap()
        .unwrap();
    let compiler::analysis::DeclarationShape::Function {
        result: other_result,
        ..
    } = &foreign.shape
    else {
        panic!()
    };
    assert_ne!(result, other_result);
    f.write("app/main.rill", "not valid source any more");
    f.write("app/model.rill", "changed");
    let retained = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    assert_eq!(retained.type_use(main, node).unwrap().unwrap().ty, usage.ty);
    assert!(
        main.snapshot
            .bytes()
            .windows(2)
            .any(|bytes| bytes == b"\r\n")
    );
}

#[test]
fn annotation_limits_are_inclusive_deterministic_and_preserve_known_findings() {
    use compiler::analysis::FailureKind;
    let f = Fixture::new(
        "type Box<T> = { value: T }; function use(value: &mut Box<[i32, bool]>): (i32) => bool {}",
    );
    let graph = f.load();
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let stats = receipt.statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    assert_eq!(
        analyze_declaration_types(&graph, exact)
            .unwrap()
            .statistics(),
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
        AnalysisLimits {
            max_work: 0,
            ..exact
        },
        AnalysisLimits {
            max_entries: 0,
            ..exact
        },
        AnalysisLimits {
            max_depth: 0,
            ..exact
        },
    ] {
        let first = analyze_declaration_types(&graph, limits).unwrap_err();
        let second = analyze_declaration_types(&graph, limits).unwrap_err();
        assert_eq!(first.kind, FailureKind::ResourceLimit);
        assert_eq!(first.statistics, second.statistics);
        assert_eq!(
            first.partial.type_uses().count(),
            second.partial.type_uses().count()
        );
        assert!(!first.complete_diagnostics);
    }
    let unlimited = AnalysisLimits {
        max_work: usize::MAX,
        max_entries: usize::MAX,
        max_depth: usize::MAX,
    };
    assert_eq!(
        analyze_declaration_types(&graph, unlimited)
            .unwrap()
            .statistics(),
        stats
    );
    f.write("app/main.rill", "function bad(value: Missing): void {}");
    let rejected = f.load();
    let full_error = analyze_declaration_types(&rejected, AnalysisLimits::default()).unwrap_err();
    assert_eq!(full_error.kind, FailureKind::TypeRejected);
    let partial = analyze_declaration_types(
        &rejected,
        AnalysisLimits {
            max_work: full_error.statistics.work - 1,
            ..unlimited
        },
    )
    .unwrap_err();
    assert_eq!(partial.kind, FailureKind::ResourceLimit);
    assert!(
        partial
            .partial
            .findings()
            .any(|f| f.span.bytes() == b"Missing")
    );
}

#[test]
fn repeated_and_relocated_analyses_have_the_same_ordered_semantic_facts() {
    let source = "type P = { x: i32 }; function use(value: P): bool {}";
    let first = Fixture::new(source);
    let second = Fixture::new(source);
    let a = first.load();
    let b = second.load();
    let r1 = analyze_declaration_types(&a, AnalysisLimits::default()).unwrap();
    let r2 = analyze_declaration_types(&a, AnalysisLimits::default()).unwrap();
    let r3 = analyze_declaration_types(&b, AnalysisLimits::default()).unwrap();
    for (u, v) in r1.type_uses().zip(r2.type_uses()) {
        assert_eq!(u.ty, v.ty);
        assert_eq!(u.span, v.span);
    }
    let projection = |receipt: &compiler::analysis::Receipt<'_>| {
        receipt
            .type_uses()
            .map(|u| {
                (
                    u.module.origin,
                    u.module.path.clone(),
                    u.span.start(),
                    u.span.end(),
                    u.span.bytes().to_vec(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(projection(&r1), projection(&r3));
    assert_eq!(r1.statistics(), r3.statistics());
    let names: Vec<_> = r1
        .declarations()
        .map(|d| d.declaration.name.as_deref().unwrap())
        .collect();
    assert_eq!(names, ["P", "use", "copy", "dispose"]);
}

#[test]
fn ambiguous_m3_inputs_never_supply_a_graph_to_typed_analysis() {
    let f = Fixture::new("import route::T; function use(value: T): void {}");
    f.write("app/route.rill", "export type T = {};");
    f.write("app/route/T.rill", "");
    let error = load(f.request()).unwrap_err();
    assert_eq!(error.kind, compiler::loader::FailureKind::SourceRejected);
    assert!(error.findings.iter().any(|f| f.code == Some("R0402")));
}

#[test]
fn unused_duplicate_members_are_not_mistaken_for_declaration_validity() {
    let f = Fixture::new("enum E { A, A } type R = { value: i32; value: bool };");
    let graph = f.load();
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    assert!(
        receipt
            .completion()
            .not_performed
            .contains(&Stage::DeclarationValidity)
    );
    let fact = receipt
        .declaration(&declaration(module(&graph, "main.rill"), "E", DeclarationKind::Enum).id)
        .unwrap()
        .unwrap();
    let compiler::analysis::DeclarationShape::Enum { variants } = &fact.shape else {
        panic!()
    };
    assert_eq!(variants.len(), 2);
    assert_ne!(variants[0].declaration.id, variants[1].declaration.id);
}

#[test]
fn unsupported_headers_and_recursive_goals_are_incomplete_not_evidence() {
    use compiler::analysis::{FailureKind, Unsupported};
    for (source, reason) in [
        (
            "type Box<T = i32> = { value: T };",
            Unsupported::GenericDefault,
        ),
        (
            "function recurse<T>(value: T): T where recurse(T);",
            Unsupported::Requirements,
        ),
        (
            "type Box<T> = { value: T }; function grow<T>(value: T): T where grow(Box<T>);",
            Unsupported::Requirements,
        ),
        (
            "function family<T>(value: T): T; implement family(value: i32): i32 { return value; }",
            Unsupported::Implementation,
        ),
        (
            "type Box<T> = { value: T }; function missing(value: Box): void {}",
            Unsupported::NeedsTypeArguments,
        ),
    ] {
        let f = Fixture::new(source);
        let graph = f.load();
        let error = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap_err();
        assert_eq!(error.kind, FailureKind::Incomplete, "{source}");
        assert!(error.partial.obligations().any(|o| o.reason == reason
            && o.span.snapshot().id() == module(&graph, "main.rill").snapshot.id()));
        assert!(error.partial.findings().next().is_none());
        assert!(!error.complete_diagnostics);
    }
}

#[test]
fn invalid_annotation_names_targets_arities_and_binders_reject_without_placeholders() {
    use compiler::analysis::{FailureKind, FindingKind};
    for (source, expected) in [
        (
            "function bad(value: int): void {}",
            FindingKind::MissingType,
        ),
        (
            "function bad(value: List<i32>): void {}",
            FindingKind::MissingType,
        ),
        (
            "function bad(value: i32<bool>): void {}",
            FindingKind::ExcessArguments,
        ),
        (
            "function bad<T>(value: T<i32>): void {}",
            FindingKind::ExcessArguments,
        ),
        (
            "type Box<T> = { value: T }; function bad(value: Box<i32, bool>): void {}",
            FindingKind::ExcessArguments,
        ),
        (
            "function bad<T, T>(value: T): void {}",
            FindingKind::DuplicateBinder,
        ),
        (
            "enum E { A } function bad(value: E.B): void {}",
            FindingKind::MissingVariant,
        ),
        (
            "type E = {}; function bad(value: E.A): void {}",
            FindingKind::InvalidVariantParent,
        ),
        (
            "import model; function bad(value: model): void {}",
            FindingKind::NotAType,
        ),
        (
            "import model; function bad(value: model::Child): void {}",
            FindingKind::MissingType,
        ),
    ] {
        let f = Fixture::new(source);
        f.write("app/model.rill", "export type Visible = {};");
        f.write("app/model/Child.rill", "export type Child = {};");
        let graph = f.load();
        let error = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap_err();
        assert_eq!(error.kind, FailureKind::TypeRejected, "{source}");
        assert!(
            error.partial.findings().any(|f| f.kind == expected),
            "{source}"
        );
        assert!(
            !graph
                .modules()
                .iter()
                .any(|m| m.key.path == "model/Child.rill")
        );
        assert!(!error.complete_diagnostics);
    }
}

#[test]
fn source_binders_then_module_bindings_shadow_the_primitive_prelude() {
    let f = Fixture::new(
        "type i32 = { marker: bool }; function local(value: i32): i32 { return value; } function generic<i32>(left: i32, right: i32): i32 { return left; } function other<i32>(value: i32): i32 { return value; }",
    );
    let graph = f.load();
    let main = module(&graph, "main.rill");
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let get = |name| {
        receipt
            .declaration(&declaration(main, name, DeclarationKind::Function).id)
            .unwrap()
            .unwrap()
    };
    let compiler::analysis::DeclarationShape::Function { result: local, .. } = &get("local").shape
    else {
        panic!()
    };
    assert!(
        matches!(local.as_ref(), TypeTerm::Nominal { declaration: id, .. } if *id == &declaration(main, "i32", DeclarationKind::Record).id)
    );
    let compiler::analysis::DeclarationShape::Function {
        parameters,
        result: generic,
    } = &get("generic").shape
    else {
        panic!()
    };
    assert_eq!(parameters[0].ty, parameters[1].ty);
    assert_eq!(parameters[0].ty, *generic);
    assert!(matches!(
        generic.as_ref(),
        TypeTerm::Parameter { position: 0, .. }
    ));
    let compiler::analysis::DeclarationShape::Function { result: other, .. } = &get("other").shape
    else {
        panic!()
    };
    assert_ne!(generic, other);
    // Core copy/dispose still reference the actual canonical primitive void and their own binders.
    assert_eq!(
        receipt
            .declaration(&graph.prelude().copy)
            .unwrap()
            .unwrap()
            .declaration
            .id,
        graph.prelude().copy
    );
}

// RFC 0005 T13/T15/T16 annotation portions, not recursive-layout acceptance.
#[test]
fn recursive_and_type_growing_nominals_remain_finite_unexpanded_terms() {
    let f = Fixture::new(
        "type Box<T> = { value: T }; type Phantom<T> = { marker: i32 }; type Nested = { value: Box<Box<i32>> }; type Node = { marker: Phantom<Node> }; type Growing<T> = { next: Growing<Box<T>> }; enum Chain { End, Link(Chain) } enum Empty {} type Marker = {}; type A = { next: &B }; type B = { next: &A };",
    );
    let graph = f.load();
    let main = module(&graph, "main.rill");
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let growing = receipt
        .declaration(&declaration(main, "Growing", DeclarationKind::Record).id)
        .unwrap()
        .unwrap();
    let compiler::analysis::DeclarationShape::Record { fields } = &growing.shape else {
        panic!()
    };
    assert!(
        matches!(fields[0].ty.as_ref(), TypeTerm::Nominal { declaration: id, arguments } if *id == &growing.declaration.id && arguments.len() == 1)
    );
    assert!(receipt.completion().not_performed.contains(&Stage::Layout));
    let empty = receipt
        .declaration(&declaration(main, "Empty", DeclarationKind::Enum).id)
        .unwrap()
        .unwrap();
    assert!(
        matches!(&empty.shape, compiler::analysis::DeclarationShape::Enum { variants } if variants.is_empty())
    );
    assert!(receipt.statistics().max_depth < 8);
}

#[test]
fn ignored_source_is_explicitly_unperformed_and_supplied_core_is_not_certified() {
    let f = Fixture::new(
        "function wrong(): i32 { type Local<T = Missing> = { value: T }; let bad = 1; return true; } return; expect 1i32; let startup: Missing = 0;",
    );
    f.write(
        "std/core.rill",
        "export function copy<T>(value: bool): never; export function dispose<T>(value: &T): i32;",
    );
    let graph = f.load();
    let main = module(&graph, "main.rill");
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    assert!(
        receipt
            .completion()
            .not_performed
            .contains(&Stage::SignatureCompatibility)
    );
    for d in &main.declarations {
        if d.scope.is_some() && d.id.kind == DeclarationKind::Record {
            assert!(receipt.declaration(&d.id).unwrap().is_none());
        }
    }
    let local_default = main
        .syntax
        .nodes
        .iter()
        .position(|n| {
            n.kind == compiler::parser::NodeKind::NamedType && n.span.bytes() == b"Missing"
        })
        .unwrap();
    assert!(receipt.type_use(main, local_default).unwrap().is_none());
    assert!(receipt.completion().not_performed.contains(&Stage::Bodies));
    assert!(
        receipt
            .completion()
            .not_performed
            .contains(&Stage::Expectations)
    );
    assert!(receipt.completion().not_performed.contains(&Stage::Startup));
}

#[test]
fn private_qualified_types_retain_the_blocked_binding_without_granting_access() {
    let f = Fixture::new("import model; function use(value: model::Hidden): void {}");
    f.write("app/model.rill", "type Hidden = { value: i32 }; export function make(): Hidden { return Hidden { value: 0 }; }");
    let graph = f.load();
    let error = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap_err();
    assert_eq!(error.kind, compiler::analysis::FailureKind::TypeRejected);
    let finding = error
        .partial
        .findings()
        .find(|f| f.kind == compiler::analysis::FindingKind::PrivateType)
        .unwrap();
    let hidden = declaration(
        module(&graph, "model.rill"),
        "Hidden",
        DeclarationKind::Record,
    );
    assert_eq!(finding.candidates[0].id, hidden.id);
    assert_eq!(finding.bindings[0].name, "Hidden");
    assert!(!finding.bindings[0].public);
    assert_eq!(finding.span.bytes(), b"model::Hidden");
    assert!(
        error
            .partial
            .type_uses()
            .all(|u| u.span.bytes() != b"model::Hidden")
    );
    // Its defining module may write that same private type in an exported signature.
    assert!(
        error
            .partial
            .declarations()
            .any(|d| d.declaration.name.as_deref() == Some("make"))
    );
}

// Syntax G13/G15 type portions, RFC 0004 written callable modes, no invocation.
#[test]
fn structural_types_preserve_tuple_arity_reference_access_and_callable_modes() {
    let f = Fixture::new(
        "function shapes(a: [], b: [i32], c: [i32, bool], d: [bool, i32], e: & &mut i32, f: (i32) => bool, g: mut (i32) => bool, h: once (i32) => bool, i: &(mut ([i32], bool) => never), j: (i32)): void {}",
    );
    let graph = f.load();
    let main = module(&graph, "main.rill");
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let function = receipt
        .declaration(&declaration(main, "shapes", DeclarationKind::Function).id)
        .unwrap()
        .unwrap();
    let compiler::analysis::DeclarationShape::Function {
        parameters: p,
        result,
    } = &function.shape
    else {
        panic!()
    };
    assert!(matches!(p[0].ty.as_ref(), TypeTerm::Tuple(items) if items.is_empty()));
    assert!(matches!(p[1].ty.as_ref(), TypeTerm::Tuple(items) if items.len() == 1));
    assert_ne!(p[2].ty, p[3].ty);
    assert_ne!(p[0].ty, *result);
    assert!(
        matches!(p[4].ty.as_ref(), TypeTerm::Reference { mutable: false, referent } if matches!(referent.as_ref(), TypeTerm::Reference { mutable: true, .. }))
    );
    for (index, mode) in [
        (5, compiler::analysis::InvocationMode::ReadOnly),
        (6, compiler::analysis::InvocationMode::Mutable),
        (7, compiler::analysis::InvocationMode::Once),
    ] {
        assert!(
            matches!(p[index].ty.as_ref(), TypeTerm::Callable { mode: actual, parameters, .. } if *actual == mode && parameters.len() == 1)
        );
    }
    assert_ne!(p[5].ty, p[6].ty);
    assert_ne!(p[6].ty, p[7].ty);
    assert!(
        matches!(p[8].ty.as_ref(), TypeTerm::Reference { referent, .. } if matches!(referent.as_ref(), TypeTerm::Callable { parameters, result, .. } if parameters.len() == 2 && matches!(result.as_ref(), TypeTerm::Primitive(i) if i.name() == "never")))
    );
    assert_eq!(
        p[9].ty,
        match p[1].ty.as_ref() {
            TypeTerm::Tuple(items) => items[0].clone(),
            _ => panic!(),
        }
    );
}

// RFC 0005 T3 annotations and the approved multiple-match lookup rule.
#[test]
fn variants_keep_parent_arguments_payload_shapes_and_ambiguous_candidates() {
    let f = Fixture::new(
        "enum Packet<T> { Data(T), Empty, Record { value: T }, Marker {} } function receive(value: Packet<i32>.Data): Packet<i32>.Empty { return value; }",
    );
    let graph = f.load();
    let main = module(&graph, "main.rill");
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let packet = declaration(main, "Packet", DeclarationKind::Enum);
    let fact = receipt.declaration(&packet.id).unwrap().unwrap();
    let compiler::analysis::DeclarationShape::Enum { variants } = &fact.shape else {
        panic!()
    };
    assert!(matches!(
        variants[0].payload,
        compiler::analysis::Payload::Tuple(_)
    ));
    assert!(matches!(
        variants[1].payload,
        compiler::analysis::Payload::Unit
    ));
    assert!(matches!(
        variants[2].payload,
        compiler::analysis::Payload::Record(_)
    ));
    assert!(
        matches!(variants[3].payload, compiler::analysis::Payload::Record(ref fields) if fields.is_empty())
    );
    let receive = receipt
        .declaration(&declaration(main, "receive", DeclarationKind::Function).id)
        .unwrap()
        .unwrap();
    let compiler::analysis::DeclarationShape::Function { parameters, result } = &receive.shape
    else {
        panic!()
    };
    let TypeTerm::Variant {
        parent,
        declaration: actual,
    } = parameters[0].ty.as_ref()
    else {
        panic!()
    };
    assert_eq!(*actual, &variants[0].declaration.id);
    assert!(
        matches!(parent.as_ref(), TypeTerm::Nominal { declaration, arguments } if *declaration == &packet.id && arguments.len() == 1)
    );
    assert_ne!(parameters[0].ty, *result);

    f.write(
        "app/main.rill",
        "enum E { A, A } function use(value: E.A): void {}",
    );
    let duplicate = f.load();
    let error = analyze_declaration_types(&duplicate, AnalysisLimits::default()).unwrap_err();
    assert_eq!(error.kind, compiler::analysis::FailureKind::TypeRejected);
    let finding = error
        .partial
        .findings()
        .find(|f| f.kind == compiler::analysis::FindingKind::AmbiguousVariant)
        .unwrap();
    assert_eq!(finding.span.bytes(), b"E.A");
    assert_eq!(finding.candidates.len(), 2);
    assert_ne!(finding.candidates[0].id, finding.candidates[1].id);
    assert!(finding.candidates.iter().all(|d| d.span.bytes() == b"A"));
}

// RFC 0005 T1/T4 identity and RFC 0002 M1 alias portions only.
#[test]
fn record_aliases_preserve_nominal_identity_and_ordered_substitution() {
    let f = Fixture::new(
        "import model::{Box as Crate, Point, ScreenPoint}; function use(value: Crate<Point>): Crate<ScreenPoint> { return value; }",
    );
    f.write("app/model.rill", "export type Box<T> = { value: T }; export type Point = { x: i32 }; export type ScreenPoint = { x: i32 };");
    let graph = f.load();
    let main = module(&graph, "main.rill");
    let model = module(&graph, "model.rill");
    let box_decl = declaration(model, "Box", DeclarationKind::Record);
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let function = receipt
        .declaration(&declaration(main, "use", DeclarationKind::Function).id)
        .unwrap()
        .unwrap();
    let compiler::analysis::DeclarationShape::Function { parameters, result } = &function.shape
    else {
        panic!()
    };
    let TypeTerm::Nominal {
        declaration: actual,
        arguments,
    } = parameters[0].ty.as_ref()
    else {
        panic!()
    };
    assert_eq!(*actual, &box_decl.id);
    assert_eq!(arguments.len(), 1);
    assert_ne!(parameters[0].ty, *result); // Equal fields do not merge Point/ScreenPoint.
    let binder = declaration(model, "T", DeclarationKind::TypeParameter);
    let box_fact = receipt.declaration(&box_decl.id).unwrap().unwrap();
    let compiler::analysis::DeclarationShape::Record { fields } = &box_fact.shape else {
        panic!()
    };
    assert_eq!(
        fields[0].ty.as_ref(),
        &TypeTerm::Parameter {
            declaration: &binder.id,
            position: 0
        }
    );
    let node = main.syntax.nodes[parameters[0].declaration.node].children[1];
    let use_fact = receipt.type_use(main, node).unwrap().unwrap();
    assert_eq!(use_fact.substitution[0].binder, &binder.id);
    assert_eq!(use_fact.substitution[0].argument, arguments[0]);
    assert_eq!(use_fact.bindings[0].name, "Crate");
    assert_eq!(use_fact.bindings[0].routes[0].module.path, "model.rill");
}

#[test]
fn explicit_function_types_use_actual_prelude_identities_and_retained_spans() {
    let f = Fixture::new("function identity(value: i32): i32 { return value; }");
    let graph = f.load();
    let main = module(&graph, "main.rill");
    let identity = declaration(main, "identity", DeclarationKind::Function);
    let receipt = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let facts = receipt.declaration(&identity.id).unwrap().unwrap();
    let compiler::analysis::DeclarationShape::Function { parameters, result } = &facts.shape else {
        panic!()
    };
    let primitive = graph
        .prelude()
        .intrinsics
        .iter()
        .find(|i| i.name() == "i32")
        .unwrap();
    assert_eq!(parameters.len(), 1);
    assert_eq!(*parameters[0].ty, TypeTerm::Primitive(primitive));
    assert_eq!(parameters[0].ty, *result);
    let use_fact = receipt
        .type_use(
            main,
            main.syntax.nodes[parameters[0].declaration.node].children[1],
        )
        .unwrap()
        .unwrap();
    assert_eq!(use_fact.span.bytes(), b"i32");
    assert_eq!(use_fact.span.snapshot().id(), main.snapshot.id());
    assert_eq!(use_fact.ty, *result);
    assert_eq!(receipt.completion().completed, Stage::DeclarationTypes);
    for stage in [
        Stage::SignatureCompatibility,
        Stage::Layout,
        Stage::Bodies,
        Stage::Evidence,
        Stage::Ownership,
        Stage::Execution,
    ] {
        assert!(receipt.completion().not_performed.contains(&stage));
    }
    assert_eq!(receipt.declarations().count(), 3); // Actual supplied copy/dispose headers too.
}
