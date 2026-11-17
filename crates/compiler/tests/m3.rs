//! M3 loader/resolver assertions, not typed checking or Rill execution.
use compiler::loader::{
    FailureKind, Limits, Lookup, Namespace, Origin, Prelude, Request, Selector, Target, load,
    validate_inputs,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "rill-m3-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let fixture = Self(root);
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
            target: "test-host".into(),
            options: BTreeMap::new(),
            test_mode: false,
            limits: Limits::default(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn has_code(error: &compiler::loader::Failure, code: &str) -> bool {
    error.findings.iter().any(|d| d.code == Some(code))
}

#[test]
fn m8_unanchored_cycles_and_dependent_aliases_are_cyclic_not_missing() {
    let f = Fixture::new();
    f.write("app/a.rill", "export import b::{X};");
    f.write("app/b.rill", "export import a::{X};");
    f.write("app/main.rill", "import a::{X};");
    let error = load(f.request()).unwrap_err();
    assert_eq!(error.kind, FailureKind::SourceRejected);
    for name in ["a.rill", "b.rill", "main.rill"] {
        assert!(
            error.findings.iter().any(|d| d.code == Some("R0405")
                && d.span
                    .as_ref()
                    .unwrap()
                    .snapshot()
                    .identity()
                    .ends_with(name)),
            "missing cyclic cause for {name}: {error:?}"
        );
    }
    assert!(!has_code(&error, "R0400"));
}

#[test]
fn m1_facade_preserves_identity_and_owner() {
    let f = Fixture::new();
    f.write("app/geometry/point.rill", "export type Point = { x: i32 }; export function origin(): Point { return Point { x: 0 }; }");
    f.write(
        "app/api.rill",
        "export import geometry::point::{Point as Position, origin};",
    );
    f.write(
        "app/main.rill",
        "import api::{Position, origin}; expect origin().x == 0;",
    );
    let graph = load(f.request()).unwrap();
    assert_eq!(
        graph
            .modules()
            .iter()
            .map(|m| m.key.path.as_str())
            .collect::<Vec<_>>(),
        ["api.rill", "geometry/point.rill", "main.rill", "core.rill"]
    );
    let api = &graph.modules()[0].key;
    let point = &graph.modules()[1].key;
    let Lookup::Found(Target::Declaration(position)) =
        graph.lookup_export(api, Namespace::TypeModule, &["Position"])
    else {
        panic!("expected nominal export")
    };
    let Lookup::Found(Target::Declaration(original)) =
        graph.lookup_export(point, Namespace::TypeModule, &["Point"])
    else {
        panic!("expected original")
    };
    assert_eq!(position, original);
    assert_eq!(&position.module, point);
    assert_eq!(graph.expectations().len(), 1);
    assert_eq!(
        graph
            .bindings()
            .iter()
            .filter(|b| b.module.path == "main.rill" && b.import.is_some())
            .count(),
        2
    );
}

#[cfg(unix)]
#[test]
fn entry_link_cannot_choose_another_application_root() {
    let f = Fixture::new();
    f.write("outside.rill", "expect true;");
    fs::create_dir(f.0.join("app")).unwrap();
    std::os::unix::fs::symlink("../outside.rill", f.0.join("app/main.rill")).unwrap();
    let error = load(f.request()).unwrap_err();
    assert_eq!(error.kind, FailureKind::SourceRejected);
    assert!(error.findings.iter().any(|d| d.code == Some("R0404")));
}

#[test]
fn m2_ungrouped_competition_grouped_export() {
    let f = Fixture::new();
    f.write("app/api.rill", "export type Widget = {};");
    f.write("app/api/Widget.rill", "");
    f.write("app/main.rill", "import api::Widget;");
    let error = load(f.request()).unwrap_err();
    assert!(has_code(&error, "R0402"));
    let candidates = &error.partial.imports[0].candidates;
    assert_eq!(candidates.len(), 2);
    assert!(
        candidates
            .iter()
            .all(|c| c.namespace == Namespace::TypeModule && !c.routes.is_empty())
    );
    f.write("app/main.rill", "import api::{Widget};");
    let graph = load(f.request()).unwrap();
    assert!(
        !graph
            .modules()
            .iter()
            .any(|m| m.key.path == "api/Widget.rill")
    );
}

#[test]
fn m3_import_roots_ignore_local_aliases() {
    let f = Fixture::new();
    f.write("app/api.rill", "export type Hidden = {};");
    f.write("app/main.rill", "import api as A; import A::{Hidden};");
    assert!(has_code(&load(f.request()).unwrap_err(), "R0400"));
    f.write("app/A.rill", "export type Hidden = {};");
    let graph = load(f.request()).unwrap();
    let imported = graph
        .bindings()
        .iter()
        .find(|b| b.module.path == "main.rill" && b.name == "Hidden")
        .unwrap();
    let Target::Declaration(id) = &imported.target else {
        panic!("expected declaration")
    };
    assert_eq!(id.module.path, "A.rill");
}

#[test]
fn m12_module_and_function_both_install_and_m15_private_half_stays_private() {
    let f = Fixture::new();
    f.write("app/api.rill", "export function Widget(): void {} type Secret = {}; export type Public = {}; function Public(): void {} type Value = {}; export function Value(): void {}");
    f.write("app/api/Widget.rill", "export type Child = {};");
    f.write(
        "app/main.rill",
        "import api::Widget as W; import api::{Public as P, Value as V};",
    );
    let graph = load(f.request()).unwrap();
    let bindings: Vec<_> = graph
        .bindings()
        .iter()
        .filter(|b| b.module.path == "main.rill")
        .collect();
    assert_eq!(bindings.iter().filter(|b| b.name == "W").count(), 2);
    assert_eq!(bindings.iter().filter(|b| b.name == "P").count(), 1);
    assert_eq!(
        bindings.iter().find(|b| b.name == "P").unwrap().namespace,
        Namespace::TypeModule
    );
    assert_eq!(bindings.iter().filter(|b| b.name == "V").count(), 1);
    assert_eq!(
        bindings.iter().find(|b| b.name == "V").unwrap().namespace,
        Namespace::ValueFunction
    );
    let api = &graph
        .modules()
        .iter()
        .find(|m| m.key.path == "api.rill")
        .unwrap()
        .key;
    assert_eq!(
        graph.lookup_export(api, Namespace::TypeModule, &["Secret"]),
        Lookup::Private
    );
    f.write("app/main.rill", "import api::{Secret};");
    assert!(has_code(&load(f.request()).unwrap_err(), "R0401"));
}

#[test]
fn m13_collision_and_failed_group_publish_no_imports() {
    let f = Fixture::new();
    f.write(
        "app/api.rill",
        "export type X = {}; export function X(): void {}",
    );
    f.write("app/main.rill", "function X(): void {} import api::{X};");
    let error = load(f.request()).unwrap_err();
    assert!(has_code(&error, "R0403"));
    assert_eq!(
        error
            .partial
            .imports
            .iter()
            .find(|i| i.module.path == "main.rill")
            .unwrap()
            .candidates
            .len(),
        2
    );
    f.write("app/bad.rill", "export import api::{X, Missing};");
    f.write("app/main.rill", "export import bad::{X};");
    let error = load(f.request()).unwrap_err();
    assert!(has_code(&error, "R0400"));
    assert!(!error.partial.complete_diagnostics);
    // PartialGraph deliberately exposes candidates, never installed bindings.
    assert!(
        error
            .partial
            .imports
            .iter()
            .any(|i| i.module.path == "main.rill" && i.candidates.len() == 2)
    );
}

#[test]
fn m8_anchored_cycles_retain_one_identity_and_multiple_anchors_collide() {
    let f = Fixture::new();
    f.write("app/a.rill", "export type X = {}; export import b::{X};");
    f.write("app/b.rill", "export import a::{X};");
    f.write("app/main.rill", "import a::{X}; import b::{X};");
    let graph = load(f.request()).unwrap();
    let a = &graph
        .modules()
        .iter()
        .find(|m| m.key.path == "a.rill")
        .unwrap()
        .key;
    let b = &graph
        .modules()
        .iter()
        .find(|m| m.key.path == "b.rill")
        .unwrap()
        .key;
    assert_eq!(
        graph.lookup_export(a, Namespace::TypeModule, &["X"]),
        graph.lookup_export(b, Namespace::TypeModule, &["X"])
    );
    f.write("app/b.rill", "export type X = {}; export import a::{X};");
    let error = load(f.request()).unwrap_err();
    assert!(has_code(&error, "R0402"));
    assert!(!has_code(&error, "R0405"));
    f.write("app/a.rill", "export import b as B;");
    f.write("app/b.rill", "export import a as A;");
    f.write("app/main.rill", "import a;");
    load(f.request()).unwrap();
}

#[test]
fn m6_m7_implementation_records_keep_canonical_family_and_distinct_bodies() {
    let f = Fixture::new();
    f.write(
        "app/family.rill",
        "export type Point = {}; export function work<T>(value: &T): bool;",
    );
    f.write("app/api.rill", "export import family::{work};");
    f.write("app/main.rill", "import api::{work as run}; import family::{work as run, Point as P, Point as Q}; implement run(value: &P): bool { return true; } implement run(value: &Q): string { return \"different return\"; }");
    let graph = load(f.request()).unwrap();
    assert_eq!(graph.implementations().len(), 2);
    let first = &graph.implementations()[0];
    let second = &graph.implementations()[1];
    assert_eq!(first.family, second.family);
    assert_eq!(first.family.module.path, "family.rill");
    assert_eq!(first.declaration.module.path, "main.rill");
    assert_ne!(first.declaration, second.declaration);
    let main = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    assert!(
        std::str::from_utf8(main.syntax.nodes[first.signature_node].span.bytes())
            .unwrap()
            .contains("&P")
    );
    assert!(
        std::str::from_utf8(main.syntax.nodes[second.signature_node].span.bytes())
            .unwrap()
            .contains("&Q): string")
    );
    // Alias-equivalent patterns and differing returns remain records, not a claimed R0304 check.
    f.write("app/main.rill", "function work(value: i32): bool { return true; } implement work(value: i32): bool { return true; }");
    let error = load(f.request()).unwrap_err();
    assert_eq!(error.kind, FailureKind::SourceRejected);
    assert!(error.findings.iter().any(|d| d.message.contains("family")));
}

#[test]
fn source_changes_and_new_candidates_keep_old_snapshots() {
    let f = Fixture::new();
    f.write("app/api.rill", "export type Thing = {};");
    f.write("app/main.rill", "import api::Thing; expect true;");
    let graph = load(f.request()).unwrap();
    validate_inputs(&graph, Limits::default()).unwrap();
    let old = graph.expectations()[0].span.clone();
    f.write("app/main.rill", "expect false;");
    assert_eq!(
        validate_inputs(&graph, Limits::default()).unwrap_err().kind,
        FailureKind::SourceChanged
    );
    assert_eq!(old.bytes(), b"expect true;");
    f.write("app/main.rill", "import api::Thing; expect true;");
    validate_inputs(&graph, Limits::default()).unwrap();
    f.write("app/api/Thing.rill", "");
    assert_eq!(
        validate_inputs(&graph, Limits::default()).unwrap_err().kind,
        FailureKind::SourceChanged
    );
    assert!(has_code(&load(f.request()).unwrap_err(), "R0402"));
    fs::remove_file(f.0.join("app/api/Thing.rill")).unwrap();
    let fresh = load(f.request()).unwrap();
    assert_ne!(graph.id(), fresh.id());
    assert_eq!(graph.modules()[0].key, fresh.modules()[0].key);
    assert_ne!(
        graph.modules()[0].declarations[0].id,
        fresh.modules()[0].declarations[0].id
    );
}

#[cfg(unix)]
#[test]
fn missing_child_after_valid_directory_link_is_a_missing_import() {
    let f = Fixture::new();
    f.write("app/real/anchor.rill", "");
    std::os::unix::fs::symlink("real", f.0.join("app/linked")).unwrap();
    f.write("app/main.rill", "import linked::Missing;");
    let error = load(f.request()).unwrap_err();
    assert_eq!(error.kind, FailureKind::SourceRejected);
    assert!(has_code(&error, "R0400"));
}

#[test]
fn m4_m5_metadata_retains_private_fields_and_written_generic_requirements() {
    use compiler::loader::DeclarationKind as D;
    use compiler::parser::NodeKind;
    let f = Fixture::new();
    f.write("app/resources.rill", "export type Ticket = { private code: i32; label: string }; function ready<T>(value: &T): bool; export function check<T>(value: &T): bool where ready(T) { return true; } export enum Packet<T> { Empty, Data(T) } function outer(): void { type Local = {}; function inner(): void {} }");
    f.write("app/main.rill", "import resources::{Ticket as T, check};");
    let graph = load(f.request()).unwrap();
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "resources.rill")
        .unwrap();
    let declarations = &module.declarations;
    let private = declarations
        .iter()
        .find(|d| d.id.kind == D::Field && d.name.as_deref() == Some("code"))
        .unwrap();
    assert!(!private.public);
    assert_eq!(private.id.module, module.key);
    assert!(private.scope.is_some());
    assert!(
        declarations
            .iter()
            .any(|d| d.id.kind == D::Family && !d.public && d.name.as_deref() == Some("ready"))
    );
    assert!(
        module
            .syntax
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Requirements && n.span.bytes() == b"where ready(T)")
    );
    assert!(
        declarations
            .iter()
            .any(|d| d.id.kind == D::Variant && d.public && d.name.as_deref() == Some("Data"))
    );
    assert!(declarations.iter().any(|d| d.id.kind == D::Record
        && d.scope.is_some()
        && d.name.as_deref() == Some("Local")));
    assert_eq!(
        graph.lookup_export(&module.key, Namespace::TypeModule, &["Local"]),
        Lookup::Missing
    );
    // These are metadata assertions, not M4's construction/privacy/requirement checks.
}

#[test]
fn m11_expectation_only_dependency_stays_loaded_in_both_modes() {
    let f = Fixture::new();
    f.write(
        "app/family.rill",
        "export function work<T>(value: &T): bool;",
    );
    f.write(
        "app/testonly.rill",
        "import family::{work}; implement work(value: &i32): bool { return true; } expect true;",
    );
    f.write("app/main.rill", "import testonly; expect true;");
    let ordinary = load(f.request()).unwrap();
    let mut request = f.request();
    request.test_mode = true;
    let testing = load(request).unwrap();
    assert_eq!(ordinary.expectations().len(), 2);
    assert_eq!(testing.expectations().len(), 2);
    assert_eq!(ordinary.implementations().len(), 1);
    assert_eq!(testing.implementations().len(), 1);
    assert_eq!(
        ordinary
            .modules()
            .iter()
            .map(|m| &m.key)
            .collect::<Vec<_>>(),
        testing.modules().iter().map(|m| &m.key).collect::<Vec<_>>()
    );
}

#[test]
fn m14_standard_unprefixed_has_no_application_fallback_and_roots_are_explicit() {
    let f = Fixture::new();
    f.write("std/core.rill", "import helper; export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;");
    f.write("app/helper.rill", "not valid Rill");
    f.write("app/main.rill", "import std;");
    f.write("std/std.rill", "export import helper as H;");
    assert!(has_code(&load(f.request()).unwrap_err(), "R0400"));
    f.write("std/helper.rill", "export type T = {};");
    f.write(
        "app/nested/leaf.rill",
        "import super::sibling; import self::local; import std::helper;",
    );
    f.write("app/nested/local.rill", "");
    f.write("app/sibling.rill", "");
    f.write("app/main.rill", "import nested::leaf; import std;");
    let graph = load(f.request()).unwrap();
    assert!(
        !graph
            .modules()
            .iter()
            .any(|m| m.key.origin == Origin::Application && m.key.path == "helper.rill")
    );
    assert!(
        graph
            .modules()
            .iter()
            .any(|m| m.key.origin == Origin::Standard && m.key.path == "std.rill")
    );
    f.write("app/main.rill", "import super::outside;");
    assert!(has_code(&load(f.request()).unwrap_err(), "R0404"));
}

#[test]
fn overlapping_roots_keep_application_and_standard_identities_separate() {
    let f = Fixture::new();
    f.write("app/core.rill", "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void; expect true;");
    f.write(
        "app/main.rill",
        "import self::core as appCore; import std::core as stdCore;",
    );
    let mut request = f.request();
    request.standard_root = f.0.join("app");
    let graph = load(request).unwrap();
    assert_eq!(
        graph
            .modules()
            .iter()
            .filter(|m| m.key.path == "core.rill")
            .count(),
        2
    );
    assert_eq!(graph.expectations().len(), 2);
    assert_ne!(
        graph
            .bindings()
            .iter()
            .find(|b| b.name == "appCore")
            .unwrap()
            .target,
        graph
            .bindings()
            .iter()
            .find(|b| b.name == "stdCore")
            .unwrap()
            .target
    );
}

#[test]
fn no_child_autoload_or_unrelated_scan_but_candidate_facades_join_graph() {
    let f = Fixture::new();
    f.write("app/api.rill", "function helper(): void {} expect false;");
    f.write("app/api/leaf.rill", "export type T = {};");
    f.write("app/unrelated.rill", "invalid syntax");
    f.write("app/api/child.rill", "invalid syntax");
    f.write("app/main.rill", "import api::leaf;");
    let graph = load(f.request()).unwrap();
    assert_eq!(graph.expectations().len(), 1);
    assert!(graph.modules().iter().any(|m| m.key.path == "api.rill"));
    let api = &graph
        .modules()
        .iter()
        .find(|m| m.key.path == "api.rill")
        .unwrap()
        .key;
    assert_eq!(
        graph.lookup_export(api, Namespace::TypeModule, &["child"]),
        Lookup::Missing
    );
    f.write("app/another-unrelated.rill", "also invalid");
    validate_inputs(&graph, Limits::default()).unwrap();
    f.write("app/api.rill", "invalid syntax");
    assert_eq!(
        load(f.request()).unwrap_err().kind,
        FailureKind::SourceRejected
    );
}

#[cfg(unix)]
#[test]
fn internal_links_share_identity_hard_links_do_not_and_relative_paths_are_canonical() {
    let f = Fixture::new();
    f.write(
        "app/real/source.rill",
        "import self::neighbor; import super::peer; export type T = {};",
    );
    f.write("app/real/neighbor.rill", "");
    f.write("app/peer.rill", "");
    f.write("app/nominal.rill", "export type T = {};");
    fs::hard_link(f.0.join("app/nominal.rill"), f.0.join("app/hard.rill")).unwrap();
    std::os::unix::fs::symlink("real/source.rill", f.0.join("app/alias.rill")).unwrap();
    f.write(
        "app/main.rill",
        "import alias as A; import real::source as B; import nominal; import hard;",
    );
    let graph = load(f.request()).unwrap();
    assert_eq!(
        graph
            .bindings()
            .iter()
            .find(|b| b.name == "A")
            .unwrap()
            .target,
        graph
            .bindings()
            .iter()
            .find(|b| b.name == "B")
            .unwrap()
            .target
    );
    assert!(
        graph
            .modules()
            .iter()
            .any(|m| m.key.path == "real/neighbor.rill")
    );
    let nominal = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "nominal.rill")
        .unwrap();
    let hard = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "hard.rill")
        .unwrap();
    assert_eq!(nominal.snapshot.bytes(), hard.snapshot.bytes());
    assert_ne!(nominal.key, hard.key);
    assert_ne!(nominal.declarations[0].id, hard.declarations[0].id);
}

#[cfg(unix)]
#[test]
fn file_export_facade_routes_deduplicate_by_canonical_identity() {
    let f = Fixture::new();
    f.write("app/a.rill", "export import b as c;");
    f.write("app/b.rill", "export type Thing = {};");
    fs::create_dir(f.0.join("app/a")).unwrap();
    std::os::unix::fs::symlink("../b.rill", f.0.join("app/a/c.rill")).unwrap();
    f.write("app/main.rill", "import a::c; import a::c::Thing;");
    let graph = load(f.request()).unwrap();
    assert_eq!(
        graph
            .modules()
            .iter()
            .filter(|m| m.key.path == "b.rill")
            .count(),
        1
    );
    assert!(
        graph
            .imports()
            .iter()
            .filter(|i| i.module.path == "main.rill")
            .all(|i| i.candidates.len() == 1)
    );
    let c = graph
        .imports()
        .iter()
        .find(|i| i.module.path == "main.rill" && i.name == "c")
        .unwrap();
    assert!(c.candidates[0].routes.len() >= 2);
    fs::remove_file(f.0.join("app/a/c.rill")).unwrap();
    f.write("app/a/c.rill", "export type Thing = {};");
    assert!(has_code(&load(f.request()).unwrap_err(), "R0402"));
}

#[test]
fn private_intermediate_modules_are_not_missing_or_public_routes() {
    let f = Fixture::new();
    f.write("app/real.rill", "export type T = {};");
    f.write("app/api.rill", "import real as inner;");
    f.write("app/main.rill", "import api::inner::T;");
    let error = load(f.request()).unwrap_err();
    assert!(has_code(&error, "R0401"));
    assert!(!has_code(&error, "R0400"));
    f.write("app/api/inner/T.rill", "");
    // The unique public filesystem route is not invalidated by a private route.
    load(f.request()).unwrap();
}

#[test]
fn exact_case_and_unicode_spelling_never_silently_match() {
    let f = Fixture::new();
    f.write("app/Case.rill", "");
    f.write("app/Cafe\u{301}.rill", "");
    f.write("app/main.rill", "import Case; import Cafe\u{301};");
    load(f.request()).unwrap();
    for wrong in ["case", "Caf\u{e9}"] {
        f.write("app/main.rill", &format!("import {wrong};"));
        let error = load(f.request()).unwrap_err();
        if error.kind == FailureKind::InputFailure {
            eprintln!(
                "host limitation: filesystem lookup cannot distinguish exact spelling for {wrong}; rejected instead of merging"
            );
            assert!(error.message.contains("exact"));
        } else {
            assert_eq!(error.kind, FailureKind::SourceRejected);
            assert!(has_code(&error, "R0400"));
        }
    }
}

#[test]
fn findings_follow_origin_order_not_absolute_root_order() {
    let f = Fixture::new();
    f.write("z-app/main.rill", "import absentApp;");
    f.write("a-std/core.rill", "import absentStd; export function copy<T>(v: &T): T; export function dispose<T>(v: T): void;");
    let mut request = f.request();
    request.entry = f.0.join("z-app/main.rill");
    request.standard_root = f.0.join("a-std");
    let error = load(request).unwrap_err();
    let locations: Vec<_> = error
        .findings
        .iter()
        .map(|d| d.span.as_ref().unwrap().snapshot().identity())
        .collect();
    assert!(locations[0].contains("/z-app/"));
    assert!(locations[1].contains("/a-std/"));
}

#[cfg(unix)]
#[test]
fn m9_escaping_file_directory_and_standard_links_reject() {
    let f = Fixture::new();
    f.write("outside.rill", "");
    f.write("outside/leaf.rill", "");
    f.write("app/main.rill", "import escape;");
    std::os::unix::fs::symlink("../outside.rill", f.0.join("app/escape.rill")).unwrap();
    assert!(has_code(&load(f.request()).unwrap_err(), "R0404"));
    std::os::unix::fs::symlink("../outside", f.0.join("app/escapeDir")).unwrap();
    f.write("app/main.rill", "import escapeDir::leaf;");
    assert!(has_code(&load(f.request()).unwrap_err(), "R0404"));
    std::os::unix::fs::symlink("../outside.rill", f.0.join("std/escape.rill")).unwrap();
    f.write("app/main.rill", "import std::escape;");
    assert!(has_code(&load(f.request()).unwrap_err(), "R0404"));
}

#[cfg(unix)]
#[test]
fn host_failures_are_not_missing_candidates() {
    use std::os::unix::net::UnixListener;
    let f = Fixture::new();
    f.write("app/main.rill", "import broken;");
    std::os::unix::fs::symlink("absent.rill", f.0.join("app/broken.rill")).unwrap();
    assert_eq!(
        load(f.request()).unwrap_err().kind,
        FailureKind::InputFailure
    );
    std::os::unix::fs::symlink("loop.rill", f.0.join("app/loop.rill")).unwrap();
    f.write("app/main.rill", "import loop;");
    assert_eq!(
        load(f.request()).unwrap_err().kind,
        FailureKind::InputFailure
    );
    let _socket = UnixListener::bind(f.0.join("app/special.rill")).unwrap();
    f.write("app/main.rill", "import special;");
    assert_eq!(
        load(f.request()).unwrap_err().kind,
        FailureKind::InputFailure
    );
    fs::remove_file(f.0.join("app/main.rill")).unwrap();
    assert_eq!(
        load(f.request()).unwrap_err().kind,
        FailureKind::InputFailure
    );
    f.write("app/main.rill", "");
    let mut request = f.request();
    request.standard_root = f.0.join("absent-root");
    assert_eq!(load(request).unwrap_err().kind, FailureKind::InputFailure);
}

#[cfg(unix)]
#[test]
fn unreadable_competing_facade_prevents_a_unique_result() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.write("app/api.rill", "");
    f.write("app/api/leaf.rill", "");
    f.write("app/main.rill", "import api::leaf;");
    let path = f.0.join("app/api.rill");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
    let denied = fs::read(&path).is_err();
    let result = load(f.request());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    if denied {
        assert_eq!(result.unwrap_err().kind, FailureKind::InputFailure);
    } else {
        eprintln!(
            "host limitation: privileged host bypasses permission denial; unreadable-file assertion is unsupported here"
        );
        result.unwrap();
    }
}

#[cfg(unix)]
#[test]
fn changed_links_roots_and_removed_dependencies_invalidate_capture() {
    let f = Fixture::new();
    f.write("app/a.rill", "");
    f.write("app/b.rill", "");
    f.write("app/main.rill", "import alias;");
    let link = f.0.join("app/alias.rill");
    std::os::unix::fs::symlink("a.rill", &link).unwrap();
    let graph = load(f.request()).unwrap();
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("b.rill", &link).unwrap();
    assert_eq!(
        validate_inputs(&graph, Limits::default()).unwrap_err().kind,
        FailureKind::SourceChanged
    );
    fs::remove_file(&link).unwrap();
    f.write("outside.rill", "");
    std::os::unix::fs::symlink("../outside.rill", &link).unwrap();
    assert_eq!(
        validate_inputs(&graph, Limits::default()).unwrap_err().kind,
        FailureKind::SourceChanged
    );
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("b.rill", &link).unwrap();
    let fresh = load(f.request()).unwrap();
    fs::remove_file(f.0.join("app/b.rill")).unwrap();
    assert!(matches!(
        validate_inputs(&fresh, Limits::default()).unwrap_err().kind,
        FailureKind::SourceChanged | FailureKind::InputFailure
    ));
    f.write("app/main.rill", "");
    let std_link = f.0.join("selected-std");
    std::os::unix::fs::symlink("std", &std_link).unwrap();
    let mut request = f.request();
    request.standard_root = std_link.clone();
    let graph = load(request).unwrap();
    f.write(
        "other-std/core.rill",
        "export function copy<T>(v: &T): T; export function dispose<T>(v: T): void;",
    );
    fs::remove_file(&std_link).unwrap();
    std::os::unix::fs::symlink("other-std", &std_link).unwrap();
    assert_eq!(
        validate_inputs(&graph, Limits::default()).unwrap_err().kind,
        FailureKind::SourceChanged
    );
}

#[test]
fn finite_limits_are_incomplete_not_source_errors_and_boundaries_are_inclusive() {
    let f = Fixture::new();
    f.write("app/main.rill", "expect true;");
    for field in 0..7 {
        let mut request = f.request();
        match field {
            0 => request.limits.max_modules = 0,
            1 => request.limits.max_source_bytes = 0,
            2 => request.limits.max_total_bytes = 0,
            3 => request.limits.max_work = 0,
            4 => request.limits.max_probes = 0,
            5 => request.limits.max_directory_entries = 0,
            _ => request.limits.parser.max_work = 0,
        }
        assert_eq!(load(request).unwrap_err().kind, FailureKind::ResourceLimit);
    }
    let main = fs::read(f.0.join("app/main.rill")).unwrap().len();
    let core = fs::read(f.0.join("std/core.rill")).unwrap().len();
    let mut request = f.request();
    request.limits.max_modules = 2;
    request.limits.max_source_bytes = main.max(core);
    request.limits.max_total_bytes = main + core;
    let graph = load(request.clone()).unwrap();
    request.limits.max_total_bytes -= 1;
    assert_eq!(load(request).unwrap_err().kind, FailureKind::ResourceLimit);
    let limits = Limits {
        max_work: 0,
        ..Limits::default()
    };
    assert_eq!(
        validate_inputs(&graph, limits).unwrap_err().kind,
        FailureKind::ResourceLimit
    );
}

#[test]
fn m1_m2_failures_propagate_without_successful_prefixes() {
    let f = Fixture::new();
    f.write("app/main.rill", "");
    fs::write(f.0.join("app/main.rill"), b"//\xff").unwrap();
    let error = load(f.request()).unwrap_err();
    assert_eq!(error.kind, FailureKind::SourceRejected);
    assert_eq!(error.findings[0].code, Some("R0001"));
    assert_eq!(error.findings[0].span.as_ref().unwrap().bytes(), b"\xff");
    f.write("app/main.rill", "import unused;");
    f.write("app/unused.rill", "function bad(): void { let broken = ; }");
    let error = load(f.request()).unwrap_err();
    assert_eq!(error.kind, FailureKind::SourceRejected);
    assert!(
        error.findings[0]
            .span
            .as_ref()
            .unwrap()
            .snapshot()
            .identity()
            .ends_with("unused.rill")
    );
    f.write(
        "app/unused.rill",
        "function deep(): void { (((((((((true))))))))); }",
    );
    let mut request = f.request();
    request.limits.parser.max_nesting = 2;
    assert_eq!(load(request).unwrap_err().kind, FailureKind::ResourceLimit);
}

#[test]
fn explicit_prelude_requires_public_bodyless_families() {
    let f = Fixture::new();
    f.write("app/main.rill", "");
    for core in [
        "",
        "export function copy<T>(v: &T): T { return v; } export function dispose<T>(v: T): void;",
        "function copy<T>(v: &T): T; export function dispose<T>(v: T): void;",
    ] {
        f.write("std/core.rill", core);
        assert_eq!(
            load(f.request()).unwrap_err().kind,
            FailureKind::InvalidRequest
        );
    }
    let mut request = f.request();
    request.prelude.copy.module = "../outside.rill".into();
    assert_eq!(load(request).unwrap_err().kind, FailureKind::InvalidRequest);
}

#[test]
fn m10_relocated_roots_and_import_order_keep_canonical_inventories() {
    let one = Fixture::new();
    let two = Fixture::new();
    for f in [&one, &two] {
        f.write("app/a.rill", "export type A = {}; expect true;");
        f.write(
            "app/z.rill",
            "type Last = {}; type First = {}; expect false;",
        );
        f.write("app/main.rill", "import z; import a;");
    }
    let first = load(one.request()).unwrap();
    two.write("app/main.rill", "import a; import z;");
    let second = load(two.request()).unwrap();
    let keys = |g: &compiler::loader::Graph| {
        g.modules()
            .iter()
            .map(|m| (m.key.origin, m.key.path.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&first), keys(&second));
    assert_eq!(
        keys(&first)
            .iter()
            .map(|(_, p)| p.as_str())
            .collect::<Vec<_>>(),
        ["a.rill", "main.rill", "z.rill", "core.rill"]
    );
    for graph in [&first, &second] {
        let module = graph
            .modules()
            .iter()
            .find(|m| m.key.path == "z.rill")
            .unwrap();
        let names: Vec<_> = module
            .declarations
            .iter()
            .filter_map(|d| d.name.as_deref())
            .collect();
        assert_eq!(names, ["Last", "First"]);
        assert!(graph.modules().iter().all(|m| {
            m.declarations
                .windows(2)
                .all(|pair| pair[0].id.start <= pair[1].id.start)
        }));
    }
}

#[test]
fn repeated_super_paths_stay_within_each_origin() {
    let f = Fixture::new();
    f.write(
        "app/main.rill",
        "import deep::nested::leaf; import std::deep::nested::leaf as standardLeaf;",
    );
    for origin in ["app", "std"] {
        f.write(
            &format!("{origin}/deep/nested/leaf.rill"),
            "import super::super::peer;",
        );
        f.write(&format!("{origin}/peer.rill"), "");
    }
    let graph = load(f.request()).unwrap();
    assert_eq!(
        graph
            .modules()
            .iter()
            .filter(|m| m.key.path == "peer.rill")
            .count(),
        2
    );
    f.write(
        "std/deep/nested/leaf.rill",
        "import super::super::super::peer;",
    );
    assert!(has_code(&load(f.request()).unwrap_err(), "R0404"));
}

#[test]
fn prelude_intrinsic_identities_and_canonical_operations_survive_shadowing() {
    let f = Fixture::new();
    f.write(
        "app/main.rill",
        "function copy(value: i32): i32 { return value; }",
    );
    let graph = load(f.request()).unwrap();
    let intrinsic_names: Vec<_> = graph
        .prelude()
        .intrinsics
        .iter()
        .map(|i| (i.name(), i.namespace()))
        .collect();
    assert_eq!(
        intrinsic_names
            .iter()
            .filter(|(_, ns)| *ns == Namespace::TypeModule)
            .count(),
        17
    );
    for name in [
        "noBorrow",
        "sendable",
        "shareable",
        "callable",
        "callableMut",
        "callableOnce",
    ] {
        assert!(intrinsic_names.contains(&(name, Namespace::ValueFunction)));
    }
    let shadow = graph
        .bindings()
        .iter()
        .find(|b| b.module.path == "main.rill" && b.name == "copy")
        .unwrap();
    assert_ne!(
        shadow.target,
        Target::Declaration(graph.prelude().copy.clone())
    );
    assert_eq!(graph.prelude().copy.module.origin, Origin::Standard);
}

#[test]
fn parser_work_is_reported_and_constrains_the_shared_loader_budget() {
    let f = Fixture::new();
    f.write("app/main.rill", "expect (true && false) || true;");
    let graph = load(f.request()).unwrap();
    for module in graph.modules() {
        assert!(module.syntax.work >= module.syntax.views.len() + module.syntax.nodes.len());
        assert!(module.syntax.work <= Limits::default().parser.max_work);
    }
    let parser_work: usize = graph.modules().iter().map(|m| m.syntax.work).sum();
    let mut request = f.request();
    request.limits.max_work = parser_work - 1;
    assert_eq!(load(request).unwrap_err().kind, FailureKind::ResourceLimit);
}

#[test]
fn long_alias_graphs_terminate_deterministically_and_respect_limits() {
    let f = Fixture::new();
    for index in 0..80 {
        let source = if index == 79 {
            "export type T = {}; expect true;".into()
        } else {
            format!("export import m{}::{{T}};", index + 1)
        };
        f.write(&format!("app/m{index}.rill"), &source);
    }
    f.write("app/main.rill", "import m0::{T};");
    let first = load(f.request()).unwrap();
    let second = load(f.request()).unwrap();
    assert_eq!(first.modules().len(), 82);
    assert_eq!(first.expectations().len(), 1);
    let inventories = |g: &compiler::loader::Graph| {
        g.modules()
            .iter()
            .map(|m| {
                (
                    m.key.clone(),
                    m.declarations
                        .iter()
                        .map(|d| (d.id.start, d.id.kind, d.name.clone(), d.public, d.scope))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(inventories(&first), inventories(&second));
    let mut request = f.request();
    request.limits.max_work = 1000;
    assert_eq!(load(request).unwrap_err().kind, FailureKind::ResourceLimit);
    f.write("app/m79.rill", "export import m0::{T};");
    let error = load(f.request()).unwrap_err();
    assert!(has_code(&error, "R0405"));
    assert!(!has_code(&error, "R0400"));
}

#[test]
fn m6_repeated_routes_collect_one_canonical_implementation_body() {
    let f = Fixture::new();
    f.write(
        "app/family.rill",
        "export function work<T>(value: &T): bool;",
    );
    f.write(
        "app/extension.rill",
        "import family::{work}; implement work(value: &i32): bool { return true; }",
    );
    f.write("app/api.rill", "export import extension as E;");
    f.write(
        "app/main.rill",
        "import api::E as A; import extension as B;",
    );
    let graph = load(f.request()).unwrap();
    assert_eq!(graph.implementations().len(), 1);
    assert_eq!(
        graph.implementations()[0].declaration.module.path,
        "extension.rill"
    );
    assert!(
        !graph
            .bindings()
            .iter()
            .any(|b| b.module.path == "extension.rill" && b.name == "work" && b.public)
    );
}

#[test]
fn module_startup_binders_and_group_aliases_participate_in_collisions() {
    let f = Fixture::new();
    f.write(
        "app/api.rill",
        "export function X(): void {} export type A = {}; export type B = {};",
    );
    f.write("app/main.rill", "let X = 1i32; import api::{X};");
    assert!(has_code(&load(f.request()).unwrap_err(), "R0403"));
    f.write("app/main.rill", "import api::{A as Same, B as Same};");
    assert!(has_code(&load(f.request()).unwrap_err(), "R0403"));
    f.write(
        "app/main.rill",
        "import api::{A as Same, A as Same}; function outer(): void { let Same = 1i32; }",
    );
    load(f.request()).unwrap();
}

#[test]
fn private_qualified_implementation_targets_are_not_missing() {
    let f = Fixture::new();
    f.write("app/family.rill", "function hidden<T>(value: &T): bool;");
    f.write(
        "app/main.rill",
        "import family as F; implement F::hidden(value: &i32): bool { return true; }",
    );
    let error = load(f.request()).unwrap_err();
    assert!(has_code(&error, "R0401"));
    assert!(!has_code(&error, "R0400"));
    let witness = &error
        .findings
        .iter()
        .find(|d| d.code == Some("R0401"))
        .unwrap()
        .witnesses[0];
    assert_eq!(witness.module.path, "family.rill");
    assert_eq!(witness.name, "hidden");
}

#[cfg(unix)]
#[test]
fn acyclic_link_work_ceiling_is_resource_incomplete_not_a_link_loop() {
    let f = Fixture::new();
    f.write("app/main.rill", "import link0;");
    f.write("app/target.rill", "");
    for index in 0..66 {
        let target = if index == 65 {
            "target.rill".into()
        } else {
            format!("link{}.rill", index + 1)
        };
        std::os::unix::fs::symlink(target, f.0.join(format!("app/link{index}.rill"))).unwrap();
    }
    assert_eq!(
        load(f.request()).unwrap_err().kind,
        FailureKind::ResourceLimit
    );
}

#[test]
fn canonical_order_uses_unsigned_utf8_paths_not_discovery_or_locale() {
    let f = Fixture::new();
    for name in ["Z", "a", "aa", "dir", "dir/a", "\u{3b1}"] {
        f.write(&format!("app/{name}.rill"), "expect true;");
    }
    f.write(
        "app/main.rill",
        "import \u{3b1}; import dir::a as child; import aa; import a; import Z;",
    );
    let graph = load(f.request()).unwrap();
    let paths: Vec<_> = graph
        .modules()
        .iter()
        .map(|m| m.key.path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "Z.rill",
            "a.rill",
            "aa.rill",
            "dir.rill",
            "dir/a.rill",
            "main.rill",
            "\u{3b1}.rill",
            "core.rill"
        ]
    );
    assert_eq!(
        graph
            .expectations()
            .iter()
            .map(|e| e.id.module.path.as_str())
            .collect::<Vec<_>>(),
        [
            "Z.rill",
            "a.rill",
            "aa.rill",
            "dir.rill",
            "dir/a.rill",
            "\u{3b1}.rill"
        ]
    );
}

#[cfg(unix)]
#[test]
fn link_target_parent_steps_follow_native_resolution_not_lexical_cleanup() {
    let f = Fixture::new();
    f.write("app/nested/target.rill", "export type Actual = {};");
    f.write("app/target.rill", "export type Wrong = {};");
    fs::create_dir(f.0.join("app/nested/deep")).unwrap();
    std::os::unix::fs::symlink("nested/deep", f.0.join("app/d")).unwrap();
    std::os::unix::fs::symlink("d/../target.rill", f.0.join("app/alias.rill")).unwrap();
    f.write("app/main.rill", "import alias;");
    let graph = load(f.request()).unwrap();
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "nested/target.rill")
        .expect("native target must load");
    assert_eq!(
        module.path,
        fs::canonicalize(f.0.join("app/alias.rill")).unwrap()
    );
    assert!(matches!(
        graph.lookup_export(&module.key, Namespace::TypeModule, &["Actual"]),
        Lookup::Found(_)
    ));
    assert!(!graph.modules().iter().any(|m| m.key.path == "target.rill"));
}

#[test]
fn missing_explicit_standard_facade_is_a_missing_import_not_host_failure() {
    let f = Fixture::new();
    f.write("app/main.rill", "import std;");
    let error = load(f.request()).unwrap_err();
    assert_eq!(error.kind, FailureKind::SourceRejected);
    assert!(has_code(&error, "R0400"));
}

#[test]
fn for_and_match_pattern_binders_have_distinct_containing_scope_records() {
    use compiler::loader::DeclarationKind;
    let f = Fixture::new();
    f.write("app/main.rill", "function inspect(input: i32): void { for (let [item, other] of items) { consume(item); } let answer = match (input) { branch => branch, Packet.Data(payload) => payload }; guard let success = operation() else (error) { return; } }");
    let graph = load(f.request()).unwrap();
    let main = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let binder = |name| {
        main.declarations
            .iter()
            .find(|d| d.id.kind == DeclarationKind::Binding && d.name.as_deref() == Some(name))
            .unwrap()
    };
    assert_eq!(binder("item").scope, binder("other").scope);
    assert_ne!(binder("item").scope, binder("branch").scope);
    assert_ne!(binder("branch").scope, binder("payload").scope);
    assert!(binder("payload").scope.is_some());
    assert_ne!(binder("success").scope, binder("error").scope);
    assert!(binder("error").scope.is_some());
    assert!(
        !main
            .declarations
            .iter()
            .any(|d| d.id.kind == DeclarationKind::Binding && d.name.as_deref() == Some("Packet"))
    );
}

#[test]
fn private_route_witness_retains_binding_owner_target_and_blocked_position() {
    let f = Fixture::new();
    f.write("app/real.rill", "export type Chosen = {};");
    f.write("app/api.rill", "import real as Hidden;");
    f.write("app/main.rill", "import api::Hidden::{Chosen};");
    let error = load(f.request()).unwrap_err();
    let witness = &error
        .findings
        .iter()
        .find(|d| d.code == Some("R0401"))
        .unwrap()
        .witnesses[0];
    assert_eq!(witness.module.path, "api.rill");
    assert_eq!(witness.name, "Hidden");
    assert_eq!(witness.namespace, Namespace::TypeModule);
    assert_eq!(witness.segment, 1);
    assert_eq!(witness.binding.bytes(), b"import real as Hidden;");
    let Target::Module(target) = &witness.target else {
        panic!("private module witness")
    };
    assert_eq!(target.path, "real.rill");
    assert!(error.findings.iter().all(|d| d.candidates.is_empty()));
}

#[test]
fn public_group_base_does_not_inherit_private_counterpart_failures() {
    let f = Fixture::new();
    f.write("app/real.rill", "export type Chosen = {};");
    f.write(
        "app/api.rill",
        "export import real as R; function R(): void {}",
    );
    f.write("app/main.rill", "import api::R::{Missing};");
    let error = load(f.request()).unwrap_err();
    assert!(has_code(&error, "R0400"));
    assert!(!has_code(&error, "R0401"));
}

#[test]
fn privacy_witnesses_use_actual_namespace_halves_after_closure() {
    let f = Fixture::new();
    f.write("app/real.rill", "export type Chosen = {};");
    f.write("app/values.rill", "export function publicValue(): void {}");
    f.write(
        "app/api.rill",
        "import real as Hidden; export import values::{publicValue as Hidden};",
    );
    f.write("app/main.rill", "import api::Hidden::{Chosen};");
    let error = load(f.request()).unwrap_err();
    assert!(has_code(&error, "R0401"));
    assert!(!has_code(&error, "R0400"));
    f.write("app/main.rill", "import api::{Hidden};");
    let graph = load(f.request()).unwrap();
    let bindings: Vec<_> = graph
        .bindings()
        .iter()
        .filter(|b| b.module.path == "main.rill")
        .collect();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].namespace, Namespace::ValueFunction);
    f.write("app/api.rill", "import values::{publicValue as Internal};");
    f.write("app/main.rill", "import api::Internal::Child;");
    let error = load(f.request()).unwrap_err();
    assert!(has_code(&error, "R0400"));
    assert!(!has_code(&error, "R0401"));
}

#[test]
fn grouped_selection_after_long_qualification_has_bounded_resolution_copies() {
    let f = Fixture::new();
    for index in 0..40 {
        let source = if index == 39 {
            (0..160)
                .map(|item| format!("export type T{item} = {{}}; "))
                .collect::<String>()
        } else {
            format!("export import m{} as Next;", index + 1)
        };
        f.write(&format!("app/m{index}.rill"), &source);
    }
    let path = format!("m0{}", "::Next".repeat(39));
    let members = (0..160)
        .map(|item| format!("T{item}"))
        .collect::<Vec<_>>()
        .join(",");
    f.write("app/main.rill", &format!("import {path}::{{{members}}};"));
    let graph = load(f.request()).unwrap();
    assert_eq!(
        graph
            .bindings()
            .iter()
            .filter(|b| b.module.path == "main.rill")
            .count(),
        160
    );
    let mut request = f.request();
    request.limits.max_work = 50_000;
    assert_eq!(load(request).unwrap_err().kind, FailureKind::ResourceLimit);
}

#[cfg(unix)]
#[test]
fn link_target_spelling_limitations_are_not_mislabeled_as_broken_links() {
    let f = Fixture::new();
    f.write("app/Case.rill", "");
    std::os::unix::fs::symlink("case.rill", f.0.join("app/alias.rill")).unwrap();
    f.write("app/main.rill", "import alias;");
    let error = load(f.request()).unwrap_err();
    assert_eq!(error.kind, FailureKind::InputFailure);
    if fs::metadata(f.0.join("app/alias.rill")).is_ok() {
        assert!(error.message.contains("exact"));
    } else {
        assert!(error.message.contains("broken"));
    }
}

#[test]
fn entry_and_explicit_prelude_reuse_m1_m2_snapshots() {
    let f = Fixture::new();
    f.write(
        "app/main.rill",
        "export type Point = { x: i32 }; expect true;",
    );
    let graph = load(f.request()).unwrap();
    let modules = graph.modules();
    assert_eq!(modules.len(), 2);
    assert_eq!(modules[0].key.origin, Origin::Application);
    assert_eq!(modules[0].key.path, "main.rill");
    assert_eq!(modules[1].key.origin, Origin::Standard);
    assert_eq!(modules[1].key.path, "core.rill");
    assert_eq!(modules[0].snapshot.id(), modules[0].syntax.snapshot.id());
    assert_eq!(
        modules[0].snapshot.bytes(),
        b"export type Point = { x: i32 }; expect true;"
    );
    assert_eq!(graph.expectations().len(), 1);
    assert_eq!(graph.prelude().copy.module.origin, Origin::Standard);
    assert_eq!(graph.prelude().dispose.module.path, "core.rill");
}
