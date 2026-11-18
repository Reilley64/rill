//! Installed capture and typed analysis, not execution of Rill programs.
use compiler::analysis::conversions::RegistrationIssueKind;
use compiler::analysis::scalars::data::DataExpression;
use compiler::analysis::scalars::{BodyOwner, NodeFactKind, ScalarUnsupported};
use compiler::analysis::typed::{Primitive, PrimitiveValue, analyze_typed};
use compiler::analysis::{AnalysisLimits, TypeTerm};
use compiler::loader::{
    self, Graph, InstalledRequest, Limits, Origin, Prelude, Request, Selector, load_installed,
};
use compiler::parser::NodeKind;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn installed_formatting_and_conversions() {
    if let Some(root) = std::env::var_os("RILL_PROVIDER_TEST_ROOT") {
        let root = PathBuf::from(root);
        for test_mode in [false, true] {
            let graph = load_installed(InstalledRequest {
                entry: root.join("app/main.rill"),
                target: "aarch64-apple-darwin".into(),
                options: BTreeMap::new(),
                test_mode,
                limits: Limits::default(),
            })
            .unwrap();
            formatting(&graph);
            numeric_conversions(&root, test_mode);
            direct_formatting(&root, test_mode);
            protected_replacements(&root, test_mode);
        }
        authority_boundaries(&root);
        return;
    }
    let root = std::env::temp_dir().join(format!("rill-m48-providers-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _cleanup = Cleanup(root.clone());
    for dir in ["bin", "lib/rill/std", "app"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
    for file in ["core.rill", "format.rill", "convert.rill"] {
        fs::copy(source.join(file), root.join("lib/rill/std").join(file)).unwrap();
    }
    fs::write(
        root.join("app/main.rill"),
        r#"function show(flag: &bool): string { return "not canonical"; }
            function render(flag: bool): string { return `flag=${flag} count=${42i32}`; }"#,
    )
    .unwrap();
    let executable = root.join("bin/provider-probe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let output = Command::new(executable)
        .args([
            "--exact",
            "installed_formatting_and_conversions",
            "--nocapture",
        ])
        .env("RILL_PROVIDER_TEST_ROOT", &root)
        .current_dir(root.join("app"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn formatting(graph: &Graph) {
    let report = analyze_typed(graph, AnalysisLimits::default());
    let role = report.show_role().expect("canonical show");
    assert_eq!(role.id.module.origin, Origin::Standard);
    assert!(
        !graph
            .bindings()
            .iter()
            .any(|b| b.module.origin == Origin::Application
                && b.import.is_some()
                && b.name == "show")
    );
    let main = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let render = main
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("render"))
        .unwrap();
    assert!(
        report
            .bodies()
            .body(main, BodyOwner::Declaration(&render.id))
            .unwrap()
            .is_some(),
        "{:?}; {:?}",
        report.bodies().findings(),
        report.bodies().obligations()
    );
    let template = main
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Template)
        .unwrap();
    let facts = report
        .bodies()
        .node(main, template)
        .unwrap()
        .expect("typed interpolation");
    let NodeFactKind::Expression(value) = &facts.kind else {
        panic!("expected expression")
    };
    assert!(matches!(value.ty.as_ref(), TypeTerm::Primitive(id) if id.name() == "string"));
    let Some(DataExpression::Interpolation(calls)) = &value.data else {
        panic!("missing formatting calls")
    };
    assert_eq!(
        calls.iter().map(|c| c.primitive).collect::<Vec<_>>(),
        [Some(Primitive::Bool), Some(Primitive::I32)]
    );
    assert!(calls.iter().all(|c| *c.family == role.id));
    assert!(calls[0].expression < calls[1].expression);
}

fn direct_formatting(root: &std::path::Path, test_mode: bool) {
    let entry = root.join("app/show.rill");
    fs::write(
        &entry,
        r"
        import std::format::show as render;
        function explicit(value: &char): string { return render(value); }
    ",
    )
    .unwrap();
    let graph = load_installed(InstalledRequest {
        entry,
        target: "aarch64-apple-darwin".into(),
        options: BTreeMap::new(),
        test_mode,
        limits: Limits::default(),
    })
    .unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let node = app
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Call)
        .unwrap();
    let facts = report
        .bodies()
        .node(app, node)
        .unwrap()
        .expect("checked explicit primitive show");
    let NodeFactKind::Expression(value) = &facts.kind else {
        panic!("expected expression")
    };
    let call = value.call.as_ref().unwrap();
    assert_eq!(*call.callee, report.show_role().unwrap().id);
    assert!(matches!(
        value.data,
        Some(DataExpression::PrimitiveShow(Primitive::Char))
    ));
    // The aliased call must use the same primitive operation as an interpolation hole.
    assert!(matches!(value.ty.as_ref(), TypeTerm::Primitive(id) if id.name() == "string"));
}

fn numeric_conversions(root: &std::path::Path, test_mode: bool) {
    let entry = root.join("app/numeric.rill");
    fs::write(
        &entry,
        r"
        import std::convert::{into, from as cast};
        function widened(value: i8): i64 { return cast(value); }
        function explicit(value: u16): i32 { return into<i32>(value); }
        function explicitSource(): i32 { return into<i32, u8>(1 + 2); }
        function rounded(value: u128): string { return `${into<f32>(value)}`; }
        function gap(value: i64): u8 { return into<u8>(value); }
        function untyped(): i32 { return into<i32>(1); }
        function filtered(value: i8): i32 { return into<i32, u8>(value); }
        function filteredLiteral(): i32 { return into<i32, u8>(1i8); }
    ",
    )
    .unwrap();
    let graph = load_installed(InstalledRequest {
        entry,
        target: "aarch64-apple-darwin".into(),
        options: BTreeMap::new(),
        test_mode,
        limits: Limits::default(),
    })
    .unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    for name in ["widened", "explicit", "explicitSource", "rounded"] {
        let d = app
            .declarations
            .iter()
            .find(|d| d.name.as_deref() == Some(name))
            .unwrap();
        assert!(
            report
                .bodies()
                .body(app, BodyOwner::Declaration(&d.id))
                .unwrap()
                .is_some(),
            "{name}: {:?}; {:?}",
            report.bodies().findings(),
            report.bodies().obligations()
        );
    }
    let role = report.into_role().expect("admitted into");
    assert!(report.into_registration_issues().unwrap().is_empty());
    let call = app
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Call)
        .unwrap();
    let fact = report
        .bodies()
        .node(app, call)
        .unwrap()
        .expect("checked aliased conversion");
    let NodeFactKind::Expression(fact) = &fact.kind else {
        panic!("expected expression")
    };
    let call = fact.call.as_ref().unwrap();
    assert_eq!(*call.callee, role.id);
    let Some(DataExpression::NumericInto(operation)) = fact.data else {
        panic!("missing conversion operation")
    };
    assert_eq!(
        operation.convert(PrimitiveValue::I8(-9)),
        Some(Ok(PrimitiveValue::I64(-9)))
    );
    let codes: Vec<_> = report
        .bodies()
        .findings()
        .iter()
        .filter_map(|f| f.kind.code())
        .collect();
    assert_eq!(codes, ["R0314", "R0003", "R0314", "R0314"]);
}

fn protected_replacements(root: &std::path::Path, test_mode: bool) {
    let entry = root.join("app/replacement.rill");
    fs::write(&entry, "import std::convert::from as cast; implement cast<u8>(v: u16): u8 { return 0u8; } implement cast<i32>(v: i8): i32 { return 0i32; }").unwrap();
    let graph = load_installed(InstalledRequest {
        entry,
        ..request(root, test_mode)
    })
    .unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let issues = report.into_registration_issues().unwrap();
    assert_eq!(
        issues
            .iter()
            .filter(|i| i.kind == RegistrationIssueKind::ProtectedNumericPair)
            .count(),
        2
    );
}

fn authority_boundaries(root: &std::path::Path) {
    let standard_root = root.join("lib/rill/std");
    let selector = |name: &str| Selector {
        module: "core.rill".into(),
        name: name.into(),
    };
    let graph = loader::load(Request {
        entry: root.join("app/numeric.rill"),
        standard_root: standard_root.clone(),
        prelude: Prelude {
            copy: selector("copy"),
            dispose: selector("dispose"),
        },
        target: "aarch64-apple-darwin".into(),
        options: BTreeMap::new(),
        test_mode: false,
        limits: Limits::default(),
    })
    .unwrap();
    assert!(
        analyze_typed(&graph, AnalysisLimits::default())
            .into_role()
            .is_none()
    );

    let path = standard_root.join("format.rill");
    fs::remove_file(&path).unwrap();
    let graph = load_installed(request(root, false)).unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert!(report.show_role().is_none());
    assert!(
        report
            .bodies()
            .obligations()
            .iter()
            .any(|o| o.reason == ScalarUnsupported::Formatting)
    );

    fs::write(path, "export function show<T>(x: &T): string;").unwrap();
    assert_eq!(
        load_installed(request(root, false)).unwrap_err().kind,
        loader::FailureKind::InvalidRequest
    );
}

fn request(root: &std::path::Path, test_mode: bool) -> InstalledRequest {
    InstalledRequest {
        entry: root.join("app/main.rill"),
        target: "aarch64-apple-darwin".into(),
        options: BTreeMap::new(),
        test_mode,
        limits: Limits::default(),
    }
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
