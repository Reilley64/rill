//! Installed primitive and formatting facts, not Rill execution.
#[path = "support/limit.rs"]
mod limit;

use compiler::analysis::scalars::data::DataExpression;
use compiler::analysis::scalars::generics::{GoalJudgment, GoalState};
use compiler::analysis::scalars::{BodyOwner, NodeFactKind, ScalarFindingKind};
use compiler::analysis::typed::{
    Primitive, PrimitiveCoverage, PrimitiveOperation, PrimitiveRole, TypedCompletion, analyze_typed,
};
use compiler::analysis::{AnalysisLimits, TypeTerm};
use compiler::loader::{
    DeclarationKind, Graph, InstalledRequest, Limits, Prelude, Request, Selector, load,
    load_installed,
};
use compiler::parser::NodeKind;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn installed_primitive_providers() {
    if let Some(prefix) = std::env::var_os("RILL_PRIMITIVES_TEST_PREFIX") {
        for test_mode in [false, true] {
            general_show(Path::new(&prefix), test_mode);
            primitive_copy_and_dispose(Path::new(&prefix), test_mode);
            primitive_equality(Path::new(&prefix), test_mode);
            protected_primitive_replacement(Path::new(&prefix), test_mode);
            unavailable_formatter(Path::new(&prefix), test_mode);
            source_only_catalogue_is_not_authority(Path::new(&prefix), test_mode);
        }
        return;
    }
    let prefix = std::env::temp_dir().join(format!("rill-m48-primitives-{}", std::process::id()));
    fs::create_dir(&prefix).unwrap();
    let _cleanup = Cleanup(&prefix);
    for dir in ["bin", "lib/rill/std", "app"] {
        fs::create_dir_all(prefix.join(dir)).unwrap();
    }
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
    for file in ["core.rill", "compare.rill", "format.rill"] {
        fs::copy(source.join(file), prefix.join("lib/rill/std").join(file)).unwrap();
    }
    let executable = prefix.join("bin/primitive-probe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let output = Command::new(executable)
        .args(["--exact", "installed_primitive_providers", "--nocapture"])
        .env("RILL_PRIMITIVES_TEST_PREFIX", &prefix)
        .current_dir(prefix.join("app"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct Cleanup<'a>(&'a Path);
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
    }
}

fn installed(prefix: &Path, test_mode: bool, source: &str) -> Graph {
    let entry = prefix.join("app/main.rill");
    fs::write(&entry, source).unwrap();
    load_installed(InstalledRequest {
        entry,
        test_mode,
        target: "aarch64-apple-darwin".into(),
        options: Default::default(),
        limits: Limits::default(),
    })
    .unwrap()
}

fn primitive_copy_and_dispose(prefix: &Path, test_mode: bool) {
    let graph = installed(
        prefix,
        test_mode,
        r"
        import std::core::copy as clone;
        function text(value: &string): string { return clone(value); }
        function scalar(value: &i128): i128 { return copy(value); }
        function release(value: string): void { dispose(value); }
        function releaseFlag(value: bool): void { dispose(value); }
        function flagged<T>(value: T): T where copy(bool) { return value; }
        function pass(value: char): char { return flagged(value); }
    ",
    );
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert_eq!(report.completion(), TypedCompletion::Incomplete);
    let exact = AnalysisLimits {
        max_work: report.statistics().work,
        ..AnalysisLimits::default()
    };
    assert_eq!(
        analyze_typed(&graph, exact).statistics(),
        report.statistics()
    );
    let short = AnalysisLimits {
        max_work: exact.max_work - 1,
        ..exact
    };
    limit::assert_typed_limit(&analyze_typed(&graph, short));
    assert_eq!(report.primitive_protection().len(), 30);
    assert!(
        report
            .primitive_protection()
            .iter()
            .all(|scope| scope.coverage == PrimitiveCoverage::Clear
                && scope.required_winner.is_some()
                && scope.overlaps.is_empty())
    );
    let role = report.copy_role().unwrap();
    let providers = report.primitive_providers();
    assert_eq!(providers.len(), 30);
    let copy_text = providers
        .iter()
        .find(|p| p.primitive == Primitive::String && p.family == &role.id)
        .unwrap();
    assert_eq!(
        copy_text.binding.unwrap().operation,
        PrimitiveOperation::CopyText
    );
    let dispose_text = providers
        .iter()
        .find(|p| p.primitive == Primitive::String && p.role == PrimitiveRole::Dispose)
        .unwrap();
    assert_eq!(
        dispose_text.binding.unwrap().operation,
        PrimitiveOperation::DisposeText
    );
    for provider in providers {
        let module = graph
            .modules()
            .iter()
            .find(|m| m.key == provider.implementation.id.module)
            .unwrap();
        assert!(
            report
                .bodies()
                .body(module, BodyOwner::Declaration(&provider.implementation.id))
                .unwrap()
                .is_some(),
            "unchecked {:?} {:?}",
            provider.role,
            provider.primitive
        );
    }
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    for function in app
        .declarations
        .iter()
        .filter(|d| d.id.kind == DeclarationKind::Function)
    {
        assert!(
            report
                .bodies()
                .body(app, BodyOwner::Declaration(&function.id))
                .unwrap()
                .is_some(),
            "unchecked {:?}",
            function.name
        );
    }
}

fn source_only_catalogue_is_not_authority(prefix: &Path, test_mode: bool) {
    let installed = installed(
        prefix,
        test_mode,
        r"
        import std::format::show;
        import std::compare::eq;
        function text(value: &bool): string { return show(value); }
        function same(left: &char, right: &char): bool { return eq(left, right); }
    ",
    );
    assert_eq!(
        analyze_typed(&installed, AnalysisLimits::default())
            .primitive_providers()
            .len(),
        58
    );
    let selector = |name: &str| Selector {
        module: "core.rill".into(),
        name: name.into(),
    };
    let graph = load(Request {
        // Same entry, files and options as the installed capture above.
        entry: prefix.join("app/main.rill"),
        standard_root: prefix.join("lib/rill/std"),
        prelude: Prelude {
            copy: selector("copy"),
            dispose: selector("dispose"),
        },
        target: "aarch64-apple-darwin".into(),
        options: Default::default(),
        test_mode,
        limits: Limits::default(),
    })
    .unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert!(report.show_role().is_none());
    assert!(report.equality_role().is_none());
    assert!(report.copy_role().is_none());
    assert!(report.dispose_role().is_none());
    assert!(report.primitive_providers().is_empty());
    assert!(report.primitive_protection().is_empty());
}

fn primitive_equality(prefix: &Path, test_mode: bool) {
    let graph = installed(
        prefix,
        test_mode,
        r"
        import std::compare::eq as same;
        function text(left: &string, right: &string): bool { return same(left, right); }
        function refs(left: &&u128, right: &&u128): bool { return same(left, right); }
        function generic<T>(left: &T, right: &T): bool where same(T) { return same(left, right); }
    ",
    );
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let eq = report.equality_role().unwrap();
    let providers: Vec<_> = report
        .primitive_providers()
        .iter()
        .filter(|p| p.role == PrimitiveRole::Equality)
        .collect();
    assert_eq!(providers.len(), 13);
    assert!(
        providers
            .iter()
            .all(|p| p.family == &eq.id && p.binding.is_none())
    );
    let compare = graph
        .modules()
        .iter()
        .find(|m| m.key == eq.id.module)
        .unwrap();
    for body in compare
        .declarations
        .iter()
        .filter(|d| d.id.kind == DeclarationKind::Implementation)
    {
        assert!(
            report
                .bodies()
                .body(compare, BodyOwner::Declaration(&body.id))
                .unwrap()
                .is_some()
        );
    }
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    for function in app
        .declarations
        .iter()
        .filter(|d| d.id.kind == DeclarationKind::Function)
    {
        assert!(
            report
                .bodies()
                .body(app, BodyOwner::Declaration(&function.id))
                .unwrap()
                .is_some(),
            "unchecked equality {:?}",
            function.name
        );
    }
    let goals: Vec<_> = report.bodies().goals().iter().filter(|g|
        g.site.module == &app.key && matches!(&g.judgment, GoalJudgment::Determine { family, .. } if *family == &eq.id)).collect();
    assert!(
        goals
            .iter()
            .all(|g| g.state == GoalState::Proved && g.premise.is_none())
    );
    assert!(goals.iter().any(|g| {
        g.selection
            .as_ref()
            .is_some_and(|s| s.implementation.module == compare.key)
    }));
    assert!(
        goals
            .iter()
            .any(|g| g.selection.is_none() && !g.dependencies.is_empty())
    );
}

fn general_show(prefix: &Path, test_mode: bool) {
    let graph = installed(
        prefix,
        test_mode,
        r#"
        import std::format::show as render;
        type Label = { text: string; }
        implement render(value: &Label): string { return "label"; }
        function custom(value: Label): string { return `${value}`; }
        function explicit(value: &Label): string { return render(value); }
        function generic<T>(value: T): string where render(T) { return `${value}`; }
        function shared(value: &Label): string { return `${value}`; }
        function exclusive(value: &mut Label): string { return `${value}`; }
    "#,
    );
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    for name in ["custom", "explicit", "generic", "shared", "exclusive"] {
        let function = app
            .declarations
            .iter()
            .find(|d| d.name.as_deref() == Some(name))
            .unwrap();
        assert!(
            report
                .bodies()
                .body(app, BodyOwner::Declaration(&function.id))
                .unwrap()
                .is_some(),
            "{name}: roles {:?}; findings {:?}; obligations {:?}",
            report.show_role().map(|d| &d.name),
            report
                .bodies()
                .findings()
                .iter()
                .map(|f| (f.site.module.path.as_str(), f.kind))
                .collect::<Vec<_>>(),
            report
                .bodies()
                .obligations()
                .iter()
                .map(|o| (o.site.module.path.as_str(), o.syntax, o.reason))
                .collect::<Vec<_>>()
        );
    }
    let show = report.show_role().unwrap();
    let format = graph
        .modules()
        .iter()
        .find(|m| m.key == show.id.module)
        .unwrap();
    assert_eq!(
        report
            .primitive_providers()
            .iter()
            .filter(|p| p.role == PrimitiveRole::Show)
            .count(),
        15
    );
    for body in format
        .declarations
        .iter()
        .filter(|d| d.id.kind == DeclarationKind::Implementation)
    {
        assert!(
            report
                .bodies()
                .body(format, BodyOwner::Declaration(&body.id))
                .unwrap()
                .is_some()
        );
    }
    let mut selections = Vec::new();
    for (node, _) in app
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == NodeKind::Template)
    {
        let fact = report.bodies().node(app, node).unwrap().unwrap();
        let NodeFactKind::Expression(value) = &fact.kind else {
            panic!("not an expression")
        };
        assert!(matches!(value.ty.as_ref(), TypeTerm::Primitive(id) if id.name() == "string"));
        let Some(DataExpression::Interpolation(calls)) = &value.data else {
            panic!("no formatting facts")
        };
        let [call] = calls.as_ref() else {
            panic!("expected one hole")
        };
        assert_eq!(call.family, &show.id);
        let goal = report.bodies().goal(&call.goal).unwrap();
        assert_eq!(goal.state, GoalState::Proved);
        assert!(goal.premise.is_none());
        selections.push(
            goal.selection
                .as_ref()
                .map(|s| s.implementation.module.path.as_str()),
        );
    }
    assert_eq!(
        selections,
        [
            Some("main.rill"),
            None,
            Some("format.rill"),
            Some("format.rill")
        ]
    );
}

fn protected_primitive_replacement(prefix: &Path, test_mode: bool) {
    let graph = installed(
        prefix,
        test_mode,
        r"
        import std::core::copy as clone;
        implement clone(value: &i32): i32 { return *value; }
        function replacement(value: &i32): i32 { return clone(value); }
        function independent(value: &string): string { return clone(value); }
    ",
    );
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let scope = report
        .primitive_protection()
        .iter()
        .find(|p| p.role == PrimitiveRole::Copy && p.primitive == Primitive::I32)
        .unwrap();
    assert_eq!(scope.coverage, PrimitiveCoverage::Overlap);
    assert!(scope.required_winner.is_some());
    let [overlap] = scope.overlaps.as_slice() else {
        panic!("expected one overlap")
    };
    assert_eq!(overlap.code(), "R0500");
    assert_eq!(overlap.implementation.id.module.path, "main.rill");
    limit::assert_typed_limits_below(&graph, &report);
    // Ruling 19 (1): the `Overlap` verdict discloses an R0500 finding, so it is
    // `Overlap` at the exact cap and `Pending` one work short (as for ownership,
    // `m4_8_ownership.rs`).
    let exact = report.statistics();
    let short = analyze_typed(
        &graph,
        AnalysisLimits {
            max_work: exact.work - 1,
            max_entries: exact.entries,
            max_depth: exact.max_depth,
        },
    );
    limit::assert_typed_limit(&short);
    let pending = short
        .primitive_protection()
        .iter()
        .find(|p| p.role == PrimitiveRole::Copy && p.primitive == Primitive::I32)
        .expect("the i32 copy scope one work short");
    assert_eq!(pending.coverage, PrimitiveCoverage::Pending);
    assert!(pending.overlaps.is_empty());
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    for (name, accepted) in [("replacement", false), ("independent", true)] {
        let function = app
            .declarations
            .iter()
            .find(|d| d.name.as_deref() == Some(name))
            .unwrap();
        assert_eq!(
            report
                .bodies()
                .body(app, BodyOwner::Declaration(&function.id))
                .unwrap()
                .is_some(),
            accepted,
            "{name}"
        );
    }
}

fn unavailable_formatter(prefix: &Path, test_mode: bool) {
    let graph = installed(
        prefix,
        test_mode,
        r#"
        import std::format::show as render;
        import std::compare::eq;
        export type Marker = {};
        type Box<T> = { value: T; }
        function show(value: &Marker): string { return "not canonical"; }
        implement<T> render(value: &Box<T>): string { return "default"; }
        implement render(value: &Box<Marker>): string where eq(Marker) { return "specific"; }
        function namesake(value: Marker): string { return `${value}`; }
        function missing<T>(value: T): string { return `${value}`; }
        function noFallback(value: Box<Marker>): string { return `${value}`; }
    "#,
    );
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let findings: Vec<_> = report
        .bodies()
        .findings()
        .iter()
        .filter(|f| f.kind == ScalarFindingKind::MissingRequirement)
        .collect();
    assert_eq!(findings.len(), 3, "missing requirements: {findings:?}");
    let specific = report
        .bodies()
        .goals()
        .iter()
        .find(|g| {
            g.site.span.bytes() == b"${value}"
                && g.state == GoalState::Disproved
                && g.selection.is_some()
        })
        .unwrap();
    assert!(!specific.dependencies.is_empty());
    assert!(specific.premise.is_none());
}
