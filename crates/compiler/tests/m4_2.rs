//! M4.2 declaration/layout assertions; no Rill execution or ownership checking.
use compiler::analysis::declarations::{
    Check, Contract, PreludeContracts, Schema, check_declarations,
};
use compiler::analysis::{AnalysisLimits, Receipt, analyze_declaration_types};
use compiler::loader::{
    Declaration, DeclarationKind, Graph, Limits, Prelude, Request, Selector, load,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "rill-m4-2-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let f = Self(path);
        f.write("app/main.rill", source);
        f.write(
            "std/core.rill",
            "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
        );
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
            target: "declaration-test".into(),
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
fn types(graph: &Graph) -> Receipt<'_> {
    analyze_declaration_types(graph, AnalysisLimits::default()).unwrap()
}
fn decl<'a>(graph: &'a Graph, name: &str, kind: DeclarationKind) -> &'a Declaration {
    graph
        .modules()
        .iter()
        .flat_map(|m| &m.declarations)
        .find(|d| d.name.as_deref() == Some(name) && d.id.kind == kind)
        .unwrap()
}

#[test]
fn tuple_and_every_enum_payload_forward_only_their_stored_parameters() {
    let f = Fixture::new(
        "type Box<T> = { value: T }; enum E<A, B, C> { Unit, Tuple(B, &mut A), Record { value: C } } type Forward<X, Y, Z> = { item: E<X, Y, Z> }; type Node = { item: E<&Node, Box<i32>, bool> }; type Borrowed = { next: &mut Borrowed };",
    );
    let graph = f.load();
    let types = types(&graph);
    let result = check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    for (name, kind) in [
        ("E", DeclarationKind::Enum),
        ("Forward", DeclarationKind::Record),
    ] {
        assert_eq!(
            result
                .layout(&decl(&graph, name, kind).id)
                .unwrap()
                .unwrap()
                .inline_parameters
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            [1, 2]
        );
    }
    let node = result
        .layout(&decl(&graph, "Node", DeclarationKind::Record).id)
        .unwrap()
        .unwrap();
    assert_eq!(node.edges.len(), 2);
    assert!(!node.edges.contains_key(&node.declaration.id));
    assert!(
        result
            .layout(&decl(&graph, "Borrowed", DeclarationKind::Record).id)
            .unwrap()
            .unwrap()
            .edges
            .is_empty()
    );
}

#[test]
fn unsupported_annotation_inputs_never_supply_a_declaration_check_receipt() {
    for source in [
        "type Box<T = i32> = { value: T };",
        "function f<T>(value: T): T where f(T);",
        "function f<T>(value: T): T; implement f(value: i32): i32 { return value; }",
        "type Box<T> = { value: T }; function f(value: Box): void {}",
    ] {
        let f = Fixture::new(source);
        let graph = f.load();
        let failure = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap_err();
        assert_eq!(failure.kind, compiler::analysis::FailureKind::Incomplete);
        // check_declarations requires the successful Receipt type, never these partial facts.
        assert!(failure.partial.obligations().next().is_some());
    }
}

#[test]
fn controlled_limits_are_inclusive_and_exhaustion_preserves_known_errors() {
    use compiler::analysis::declarations::{CheckFailureKind, CheckFinding};
    let f = Fixture::new("type A<T> = { value: [T, &T] }; type B = { value: A<i32> };");
    let graph = f.load();
    let types = types(&graph);
    let result = check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let stats = result.statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    assert_eq!(
        check_declarations(&types, contracts(&graph), exact)
            .unwrap()
            .statistics(),
        stats
    );
    for limit in [
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
        let first = check_declarations(&types, contracts(&graph), limit).unwrap_err();
        let second = check_declarations(&types, contracts(&graph), limit).unwrap_err();
        assert_eq!(first.kind, CheckFailureKind::ResourceLimit);
        assert_eq!(first.statistics, second.statistics);
        assert!(!first.complete_diagnostics);
    }
    let unlimited = AnalysisLimits {
        max_work: usize::MAX,
        max_entries: usize::MAX,
        max_depth: usize::MAX,
    };
    assert_eq!(
        check_declarations(&types, contracts(&graph), unlimited)
            .unwrap()
            .statistics(),
        stats
    );
    f.write(
        "app/main.rill",
        "type Dup = { x: i32; x: bool }; type Loop = { next: Loop };",
    );
    let graph = f.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let rejected = check_declarations(&types, contracts(&graph), unlimited).unwrap_err();
    let partial = check_declarations(
        &types,
        contracts(&graph),
        AnalysisLimits {
            max_work: rejected.statistics.work - 1,
            ..unlimited
        },
    )
    .unwrap_err();
    assert_eq!(partial.kind, CheckFailureKind::ResourceLimit);
    assert!(
        partial
            .partial
            .findings()
            .any(|f| matches!(f, CheckFinding::DuplicateMembers { .. }))
    );
    f.write(
        "app/main.rill",
        "type A = { next: A }; type B = { next: B };",
    );
    let graph = f.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let rejected = check_declarations(&types, contracts(&graph), unlimited).unwrap_err();
    let partial = check_declarations(
        &types,
        contracts(&graph),
        AnalysisLimits {
            max_work: rejected.statistics.work - 1,
            ..unlimited
        },
    )
    .unwrap_err();
    assert_eq!(partial.kind, CheckFailureKind::ResourceLimit);
    assert!(
        partial
            .partial
            .findings()
            .any(|f| f.code() == Some("R0406"))
    );
}

#[test]
fn all_two_vertex_graphs_match_the_independent_reference_cycle_cases() {
    let pairs = [(0, 0), (0, 1), (1, 0), (1, 1)];
    for mask in 0usize..16 {
        let mut source = String::new();
        for from in 0..2 {
            source.push_str(&format!("type N{from} = {{"));
            for (bit, &(left, right)) in pairs.iter().enumerate() {
                if left == from && mask & (1 << bit) != 0 {
                    source.push_str(&format!("edge{bit}: N{right};"));
                }
            }
            source.push_str("};");
        }
        let f = Fixture::new(&source);
        let graph = f.load();
        let types = types(&graph);
        let result = check_declarations(&types, contracts(&graph), AnalysisLimits::default());
        let expected_cycle = mask & 1 != 0 || mask & 8 != 0 || mask & 6 == 6;
        if expected_cycle {
            let failure = result.unwrap_err();
            assert_eq!(
                failure.kind,
                compiler::analysis::declarations::CheckFailureKind::SourceRejected
            );
            let expected_components = if mask & 6 == 6 {
                1
            } else {
                usize::from(mask & 1 != 0) + usize::from(mask & 8 != 0)
            };
            assert_eq!(
                failure
                    .partial
                    .findings()
                    .filter(|f| f.code() == Some("R0406"))
                    .count(),
                expected_components
            );
        } else {
            assert!(result.is_ok(), "mask {mask}");
        }
    }
}

#[test]
fn summary_provenance_and_cycle_forwarding_retain_import_routes_after_live_edits() {
    let f = Fixture::new("import model::{Box as Renamed}; type Node = { nested: Renamed<Node> };");
    f.write("app/model.rill", "export type Box<T> = { value: T };");
    let graph = f.load();
    let types = types(&graph);
    f.write("app/main.rill", "invalid live source");
    f.write("app/model.rill", "also changed");
    let failure =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap_err();
    let steps = failure
        .partial
        .findings()
        .find_map(|f| match f {
            compiler::analysis::declarations::CheckFinding::InlineCycle { steps } => Some(steps),
            _ => None,
        })
        .unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].source.annotation.span.bytes(), b"Renamed<Node>");
    assert_eq!(steps[0].source.annotation.bindings[0].name, "Renamed");
    assert_eq!(
        steps[0].source.annotation.bindings[0].routes[0].module.path,
        "model.rill"
    );
    assert_eq!(
        steps[0].forwarding[0].declaration,
        &decl(&graph, "Box", DeclarationKind::Record).id
    );
    assert!(
        matches!(steps[0].forwarding[0].argument.as_ref(), compiler::analysis::TypeTerm::Nominal { declaration, .. } if *declaration == steps[0].origin)
    );
}

#[test]
fn repeated_relocated_queries_keep_ordered_facts_and_capture_isolation() {
    use compiler::analysis::declarations::DeclarationReceipt;
    let source = "type Box<T> = { value: T }; type R = { first: Box<i32>; later: Box<bool> };";
    let f = Fixture::new(source);
    let relocated = Fixture::new(source);
    let graph = f.load();
    let other = relocated.load();
    let types = types(&graph);
    let foreign_types = analyze_declaration_types(&other, AnalysisLimits::default()).unwrap();
    let first = check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let repeated =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let foreign =
        check_declarations(&foreign_types, contracts(&other), AnalysisLimits::default()).unwrap();
    let projection = |r: &DeclarationReceipt<'_, '_>| {
        r.layouts()
            .map(|l| {
                (
                    l.declaration.id.module.clone(),
                    l.declaration.id.start,
                    l.inline_parameters.keys().copied().collect::<Vec<_>>(),
                    l.edges
                        .values()
                        .map(|e| {
                            (
                                e.target.module.clone(),
                                e.target.start,
                                e.source.member.span.start(),
                                e.source.annotation.span.start(),
                                e.forwarding.iter().map(|s| s.parameter).collect::<Vec<_>>(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(projection(&first), projection(&repeated));
    assert_eq!(projection(&first), projection(&foreign));
    assert_eq!(first.statistics(), repeated.statistics());
    assert_ne!(first.context(), foreign.context());
    let edge = first
        .layout(&decl(&graph, "R", DeclarationKind::Record).id)
        .unwrap()
        .unwrap()
        .edges
        .values()
        .next()
        .unwrap();
    assert_eq!(edge.source.member.name.as_deref(), Some("first"));
    assert_eq!(
        first
            .layout(&decl(&other, "R", DeclarationKind::Record).id)
            .unwrap_err(),
        compiler::analysis::QueryError::InvalidRequest
    );
    assert_eq!(
        first.declaration(&other.prelude().copy).unwrap_err(),
        compiler::analysis::QueryError::InvalidRequest
    );
    let field = decl(&graph, "first", DeclarationKind::Field);
    assert!(first.declaration(&field.id).unwrap().is_none());
}

#[test]
fn long_graphs_and_cycles_use_iterative_containment_checking() {
    let length = 240;
    let chain = |cycle| {
        (0..length)
            .map(|i| {
                if i + 1 < length {
                    format!("type N{i} = {{ next: N{} }};", i + 1)
                } else if cycle {
                    format!("type N{i} = {{ next: N0 }};")
                } else {
                    format!("type N{i} = {{ value: i32 }};")
                }
            })
            .collect::<String>()
    };
    let f = Fixture::new(&chain(false));
    let graph = f.load();
    let types = types(&graph);
    let receipt = check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    assert_eq!(receipt.layouts().count(), length);
    assert_eq!(receipt.statistics().max_depth, 1);
    f.write("app/main.rill", &chain(true));
    let graph = f.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let failure =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap_err();
    let steps = failure
        .partial
        .findings()
        .find_map(|f| match f {
            compiler::analysis::declarations::CheckFinding::InlineCycle { steps } => Some(steps),
            _ => None,
        })
        .unwrap();
    assert_eq!(steps.len(), length);
    assert_eq!(
        check_declarations(
            &types,
            contracts(&graph),
            AnalysisLimits {
                max_work: 50,
                ..AnalysisLimits::default()
            }
        )
        .unwrap_err()
        .kind,
        compiler::analysis::declarations::CheckFailureKind::ResourceLimit
    );
}

#[test]
fn duplicate_members_and_parameters_reject_even_when_unused() {
    use compiler::analysis::declarations::{CheckFailureKind, CheckFinding};
    for source in [
        "type R = { private x: i32; x: bool; x: char };",
        "enum E { A, A }",
        "enum E { A { x: i32; x: bool } }",
        "function f(x: i32, x: bool): void {}",
    ] {
        let f = Fixture::new(source);
        let graph = f.load();
        let types = types(&graph);
        let failure =
            check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap_err();
        assert_eq!(failure.kind, CheckFailureKind::SourceRejected, "{source}");
        let members = failure
            .partial
            .findings()
            .find_map(|f| match f {
                CheckFinding::DuplicateMembers { members, .. } => Some(members),
                _ => None,
            })
            .unwrap();
        assert!(members.len() >= 2);
        assert_ne!(members[0].id, members[1].id);
        assert!(
            members
                .windows(2)
                .all(|pair| pair[0].span.start() < pair[1].span.start())
        );
    }
    let f = Fixture::new(
        "type A = { x: i32 }; type B = { x: bool }; enum E { A { x: i32 }, B { x: bool } } function valid<T>(T: T, _: void, _: never): void {}",
    );
    let graph = f.load();
    let types = types(&graph);
    let result = check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    assert!(result.declarations().all(|d| d.members_unique));
}

#[test]
fn mismatched_prelude_signatures_reject_and_contract_data_cannot_change_the_policy() {
    use compiler::analysis::declarations::{CheckFailureKind, CheckFinding};
    let copy = "export function copy<T>(value: &T): T;";
    let dispose = "export function dispose<T>(value: T): void;";
    for core in [
        format!("export function copy<T>(value: T): T; {dispose}"),
        format!("export function copy<T>(value: &mut T): T; {dispose}"),
        format!("export function copy<T>(value: &T): i32; {dispose}"),
        format!("export function copy<T,U>(value: &T): T; {dispose}"),
        format!("export function copy<T>(): T; {dispose}"),
        format!("{copy} export function dispose<T>(value: &T): void;"),
        format!("type void = {{}}; {copy} {dispose}"),
    ] {
        let f = Fixture::new("");
        f.write("std/core.rill", &core);
        let graph = f.load();
        let types = types(&graph);
        let failure =
            check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap_err();
        assert_eq!(failure.kind, CheckFailureKind::SourceRejected, "{core}");
        assert!(failure.partial.findings().any(|f| matches!(f, CheckFinding::PreludeSignature { actual, .. } if actual.declaration.span.snapshot().id() == graph.modules().iter().find(|m| m.key.path == "core.rill").unwrap().snapshot.id())));
    }
    let f = Fixture::new("function copy<T>(value: &T): T { return value; }");
    f.write("std/core.rill", "export function copy<Renamed>(other: &Renamed): Renamed; export function dispose<Item>(_: Item): void;");
    let graph = f.load();
    let types = types(&graph);
    let result = check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let local = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap()
        .declarations
        .iter()
        .find(|d| d.id.kind == DeclarationKind::Function)
        .unwrap();
    assert_eq!(
        result.declaration(&local.id).unwrap().unwrap().prelude,
        None
    );
    let foreign = f.load();
    for bad in [
        PreludeContracts {
            copy: Contract {
                declaration: &graph.prelude().copy,
                schema: Schema::ConsumingDispose,
            },
            ..contracts(&graph)
        },
        PreludeContracts {
            copy: Contract {
                declaration: &local.id,
                schema: Schema::BorrowedCopy,
            },
            ..contracts(&graph)
        },
        contracts(&foreign),
    ] {
        let failure = check_declarations(
            &types,
            bad,
            AnalysisLimits {
                max_work: 0,
                max_entries: 0,
                max_depth: 0,
            },
        )
        .unwrap_err();
        assert_eq!(failure.kind, CheckFailureKind::InvalidRequest);
        assert_eq!(failure.statistics.work, 0);
        assert!(failure.partial.findings().next().is_none());
    }
}

#[test]
fn approved_void_never_positions_do_not_supply_inhabitation_or_hide_cycles() {
    let f = Fixture::new(
        "type UnitHolder = { value: void }; type BottomHolder = { value: never }; enum Empty {} type EmptyHolder = { value: Empty }; function types(value: never, unit: void, view: &never): void {}",
    );
    let graph = f.load();
    let types = types(&graph);
    let result = check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    assert!(
        result
            .layout(&decl(&graph, "UnitHolder", DeclarationKind::Record).id)
            .unwrap()
            .unwrap()
            .edges
            .is_empty()
    );
    assert!(
        result
            .completion()
            .not_performed
            .contains(&compiler::analysis::Stage::Bodies)
    );
    f.write(
        "app/main.rill",
        "type BottomCycle = { impossible: never; next: BottomCycle };",
    );
    let graph = f.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    assert!(
        check_declarations(&types, contracts(&graph), AnalysisLimits::default())
            .unwrap_err()
            .partial
            .findings()
            .any(|f| f.code() == Some("R0406"))
    );
}

#[test]
fn callable_storage_is_incomplete_but_references_and_unused_arguments_stop_it() {
    use compiler::analysis::declarations::CheckFailureKind;
    let f = Fixture::new(
        "type Phantom<T> = { marker: i32 }; type Safe = { view: &(() => i32); unused: Phantom<() => bool> }; function take(work: (i32) => bool): void {}",
    );
    let graph = f.load();
    let types = types(&graph);
    check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    f.write("app/main.rill", "type Box<T> = { value: T }; type Capture<T> = { work: (T) => T }; type Host = { inner: Capture<i32> }; type Applied = { work: Box<() => bool> }; type Cycle = { next: Cycle };");
    let graph = f.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let failure =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap_err();
    assert_eq!(failure.kind, CheckFailureKind::Incomplete);
    for name in ["Capture", "Host", "Applied"] {
        let layout = failure
            .partial
            .layouts()
            .find(|l| l.declaration.name.as_deref() == Some(name))
            .unwrap();
        assert!(!layout.complete); // Storage-summary completeness, not an acyclicity flag.
        assert!(
            failure
                .partial
                .obligations()
                .any(|o| o.declaration == &layout.declaration.id)
        );
    }
    let capture = failure
        .partial
        .layouts()
        .find(|l| l.declaration.name.as_deref() == Some("Capture"))
        .unwrap();
    assert!(capture.inline_parameters.is_empty()); // Signature T is not a captured T.
    assert!(
        failure
            .partial
            .findings()
            .any(|f| f.code() == Some("R0406"))
    );
}

#[test]
fn stored_variants_use_parent_summaries_without_erasing_variant_identity() {
    use compiler::analysis::declarations::CheckFailureKind;
    let f =
        Fixture::new("enum Choice<T> { Empty, Value(T) } type Node = { tag: Choice<Node>.Empty };");
    let graph = f.load();
    let types = types(&graph);
    let failure =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap_err();
    assert_eq!(failure.kind, CheckFailureKind::SourceRejected);
    let node = failure
        .partial
        .layouts()
        .find(|l| l.declaration.name.as_deref() == Some("Node"))
        .unwrap();
    assert!(node.edges.contains_key(&node.declaration.id));
    assert!(
        node.edges
            .contains_key(&decl(&graph, "Choice", DeclarationKind::Enum).id)
    );
    f.write("app/main.rill", "enum Choice<T> { Empty, Value(T) } type Phantom<T> = { marker: i32 }; type Node = { tag: Choice<Phantom<Node>>.Empty };");
    let graph = f.load();
    let types = analyze_declaration_types(&graph, AnalysisLimits::default()).unwrap();
    let result = check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let node = result
        .layout(&decl(&graph, "Node", DeclarationKind::Record).id)
        .unwrap()
        .unwrap();
    assert!(!node.edges.contains_key(&node.declaration.id));
    let raw = types.declaration(&node.declaration.id).unwrap().unwrap();
    assert!(
        matches!(&raw.shape, compiler::analysis::DeclarationShape::Record { fields } if matches!(fields[0].ty.as_ref(), compiler::analysis::TypeTerm::Variant { .. }))
    );
}

#[test]
fn growing_mutual_and_fake_indirect_source_types_reject_even_when_unused() {
    for source in [
        "type A = { next: B }; type B = { next: A };",
        "type Box<T> = { value: T }; type Growing<T> = { next: Growing<Box<T>> };",
        "type List<T> = { private next: List<T> };",
        "type Shared = { private next: Shared };",
    ] {
        let f = Fixture::new(source);
        let graph = f.load();
        let types = types(&graph);
        assert!(
            check_declarations(&types, contracts(&graph), AnalysisLimits::default())
                .unwrap_err()
                .partial
                .findings()
                .any(|f| f.code() == Some("R0406")),
            "{source}"
        );
    }
    let f = Fixture::new("import std::model::List; function unused(value: &List): void {}");
    f.write(
        "std/model.rill",
        "export type List = { private next: List };",
    );
    let graph = f.load();
    let types = types(&graph);
    let failure =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap_err();
    let layout = failure
        .partial
        .layouts()
        .find(|l| l.declaration.name.as_deref() == Some("List"))
        .unwrap();
    assert_eq!(
        layout.declaration.id.module.origin,
        compiler::loader::Origin::Standard
    );
    assert!(
        failure
            .partial
            .findings()
            .any(|f| f.code() == Some("R0406"))
    );
}

#[test]
fn cycles_have_r0406_and_original_unicode_crlf_storage_witnesses() {
    let f = Fixture::new("type Café = {\r\n private next: Café\r\n};");
    let graph = f.load();
    let types = types(&graph);
    let failure =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap_err();
    let finding = failure
        .partial
        .findings()
        .find(|f| f.code() == Some("R0406"))
        .unwrap();
    let compiler::analysis::declarations::CheckFinding::InlineCycle { steps } = finding else {
        panic!()
    };
    assert_eq!(steps[0].source.annotation.span.bytes(), "Café".as_bytes());
    assert_eq!(
        steps[0].source.annotation.span.start(),
        "type Café = {\r\n private next: ".len()
    );
    assert!(!steps[0].source.member.public);
    assert_eq!(
        steps[0].source.annotation.span.snapshot().id(),
        graph
            .modules()
            .iter()
            .find(|m| m.key.path == "main.rill")
            .unwrap()
            .snapshot
            .id()
    );
}

// RFC 0005 T13/T15/T16 and the approved summary/containment procedure.
#[test]
fn layout_summaries_forward_parameters_without_expanding_finite_wrappers() {
    let f = Fixture::new(
        "type Box<T> = { value: T }; type Forward<A, B> = { item: Box<B> }; type Phantom<T> = { marker: i32 }; type Nested = { value: Box<Box<i32>> }; type Node = { marker: Phantom<Node> };",
    );
    let graph = f.load();
    let types = types(&graph);
    let receipt = check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    let box_id = &decl(&graph, "Box", DeclarationKind::Record).id;
    let forward = receipt
        .layout(&decl(&graph, "Forward", DeclarationKind::Record).id)
        .unwrap()
        .unwrap();
    assert_eq!(
        forward
            .inline_parameters
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [1]
    );
    let proof = &forward.inline_parameters[&1];
    assert_eq!(proof.forwarding[0].declaration, box_id);
    assert_eq!(proof.forwarding[0].parameter, 0);
    assert!(proof.forwarding[0].summary_generation < proof.generation);
    let nested = receipt
        .layout(&decl(&graph, "Nested", DeclarationKind::Record).id)
        .unwrap()
        .unwrap();
    assert_eq!(nested.edges.keys().copied().collect::<Vec<_>>(), [box_id]);
    assert_eq!(
        nested.edges[box_id].source.annotation.span.bytes(),
        b"Box<Box<i32>>"
    );
    assert_eq!(
        nested.edges[box_id].source.member.name.as_deref(),
        Some("value")
    );
    let node = receipt
        .layout(&decl(&graph, "Node", DeclarationKind::Record).id)
        .unwrap()
        .unwrap();
    assert_eq!(node.edges.len(), 1);
    assert!(node.inline_parameters.is_empty());
    assert!(receipt.layout(&graph.prelude().copy).unwrap().is_none());

    f.write("app/main.rill", "enum Chain { End, Link(Chain) }");
    let cyclic = f.load();
    let cyclic_types = analyze_declaration_types(&cyclic, AnalysisLimits::default()).unwrap();
    let failure = check_declarations(&cyclic_types, contracts(&cyclic), AnalysisLimits::default())
        .unwrap_err();
    assert_eq!(
        failure.kind,
        compiler::analysis::declarations::CheckFailureKind::SourceRejected
    );
    let cycle = failure
        .partial
        .findings()
        .find_map(|f| match f {
            compiler::analysis::declarations::CheckFinding::InlineCycle { steps } => Some(steps),
            _ => None,
        })
        .unwrap();
    assert_eq!(cycle.len(), 1);
    assert_eq!(cycle[0].origin, cycle[0].target);
    assert_eq!(cycle[0].source.annotation.span.bytes(), b"Chain");
}

#[test]
fn each_required_prelude_contract_is_checked_even_when_selectors_share_a_family() {
    let f = Fixture::new("");
    let graph = load(Request {
        entry: f.0.join("app/main.rill"),
        standard_root: f.0.join("std"),
        prelude: Prelude {
            copy: Selector {
                module: "core.rill".into(),
                name: "copy".into(),
            },
            dispose: Selector {
                module: "core.rill".into(),
                name: "copy".into(),
            },
        },
        target: "same-prelude".into(),
        options: BTreeMap::new(),
        test_mode: false,
        limits: Limits::default(),
    })
    .unwrap();
    let types = types(&graph);
    let failure =
        check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap_err();
    assert_eq!(
        failure.kind,
        compiler::analysis::declarations::CheckFailureKind::SourceRejected
    );
    assert!(failure.partial.findings().any(|f| matches!(
        f,
        compiler::analysis::declarations::CheckFinding::PreludeSignature {
            schema: Schema::ConsumingDispose,
            ..
        }
    )));
}

#[test]
fn canonical_prelude_signatures_and_scoped_completion_use_actual_identities() {
    let f = Fixture::new("function identity<T>(value: T): T { return value; }");
    let graph = f.load();
    let types = types(&graph);
    let receipt = check_declarations(&types, contracts(&graph), AnalysisLimits::default()).unwrap();
    assert_eq!(receipt.context(), graph.id());
    assert_eq!(
        receipt
            .declaration(&graph.prelude().copy)
            .unwrap()
            .unwrap()
            .prelude,
        Some(Schema::BorrowedCopy)
    );
    assert_eq!(
        receipt
            .declaration(&graph.prelude().dispose)
            .unwrap()
            .unwrap()
            .prelude,
        Some(Schema::ConsumingDispose)
    );
    assert!(
        receipt
            .declaration(&decl(&graph, "identity", DeclarationKind::Function).id)
            .unwrap()
            .unwrap()
            .members_unique
    );
    for check in [
        Check::MemberUniqueness,
        Check::PreludeSignatures,
        Check::ExistingTypePositions,
        Check::SourceNominalInlineContainment,
    ] {
        assert!(receipt.completion().completed.contains(&check));
    }
    assert!(
        receipt
            .completion()
            .not_performed
            .contains(&compiler::analysis::Stage::Bodies)
    );
    assert!(
        receipt
            .completion()
            .not_performed
            .contains(&compiler::analysis::Stage::Ownership)
    );
    assert!(
        receipt
            .completion()
            .not_performed
            .contains(&compiler::analysis::Stage::Execution)
    );
}
