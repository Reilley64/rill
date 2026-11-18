//! Installed collection analysis through the public loader and shared checker.
use compiler::analysis::scalars::{
    BodyOwner, NodeFactKind, Place, ScalarFindingKind, ScalarUnsupported,
};
use compiler::analysis::{AnalysisLimits, TypeTerm, typed::analyze_typed};
use compiler::loader::{
    self, FailureKind, InstalledRequest, Limits, Origin, Prelude, Request, Selector,
};
use compiler::parser::NodeKind;
use std::{fs, path::PathBuf, process::Command};

#[test]
fn installed_list() {
    if let Some(root) = std::env::var_os("RILL_LIST_PROBE_ROOT") {
        let root = PathBuf::from(root);
        let request = InstalledRequest {
            entry: root.join("app/main.rill"),
            target: "aarch64-apple-darwin".into(),
            options: Default::default(),
            test_mode: false,
            limits: Limits::default(),
        };
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                test_mode,
                ..request.clone()
            })
            .unwrap();
            let collection = graph
                .modules()
                .iter()
                .find(|module| {
                    module.key.origin == Origin::Standard && module.key.path == "collection.rill"
                })
                .expect("brackets trigger the pinned collection module");
            let list = collection
                .declarations
                .iter()
                .find(|item| item.name.as_deref() == Some("List"))
                .unwrap();
            let app = graph
                .modules()
                .iter()
                .find(|module| module.key.origin == Origin::Application)
                .unwrap();
            assert!(
                !graph
                    .bindings()
                    .iter()
                    .any(|binding| binding.module == app.key && binding.name == "List")
            );
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert!(
                report.bodies().findings().is_empty(),
                "{:?}",
                report.bodies().findings()
            );
            let bracket = app
                .syntax
                .nodes
                .iter()
                .position(|node| node.kind == NodeKind::Bracket)
                .unwrap();
            let NodeFactKind::Expression(fact) = &report
                .bodies()
                .node(app, bracket)
                .unwrap()
                .expect("List bracket")
                .kind
            else {
                panic!("not a bracket expression")
            };
            assert!(
                matches!(fact.ty.as_ref(), TypeTerm::Nominal { declaration, arguments } if **declaration == list.id && matches!(arguments.as_slice(), [element] if matches!(element.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")))
            );
            let index = app
                .syntax
                .nodes
                .iter()
                .position(|node| node.kind == NodeKind::Index)
                .unwrap();
            let NodeFactKind::Expression(fact) = &report
                .bodies()
                .node(app, index)
                .unwrap()
                .expect("List projection")
                .kind
            else {
                panic!("not a projection")
            };
            assert!(matches!(fact.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"));
            assert_eq!(fact.place, Place::ReadOnly);
            let borrowed = app
                .syntax
                .nodes
                .iter()
                .position(|node| {
                    node.kind == NodeKind::Prefix("&") && node.span.bytes() == b"&view[at]"
                })
                .unwrap();
            let operand = app.syntax.nodes[borrowed].children[0];
            let NodeFactKind::Expression(projection) = &report
                .bodies()
                .node(app, operand)
                .unwrap()
                .expect("borrowed List projection")
                .kind
            else {
                panic!("not a projection")
            };
            let path = projection.path.as_ref().unwrap();
            use compiler::analysis::scalars::data::PathProjection;
            assert_eq!(
                path.projections,
                [PathProjection::Dereference, PathProjection::ListElement]
            );
            let read = app
                .declarations
                .iter()
                .find(|item| item.name.as_deref() == Some("read"))
                .unwrap();
            assert!(
                report
                    .bodies()
                    .body(app, BodyOwner::Declaration(&read.id))
                    .unwrap()
                    .is_some(),
                "{:?}",
                report.bodies().obligations()
            );
        }
        fs::write(&request.entry, "import std::collection::{List as Items, get}; import std::option::Option; function read(xs: &Items<i32>, at: u64): Option<&i32> { return get(xs, at); }").unwrap();
        let graph = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert!(
            report.bodies().findings().is_empty(),
            "{:?}",
            report.bodies().findings()
        );
        let app = graph
            .modules()
            .iter()
            .find(|module| module.key.origin == Origin::Application)
            .unwrap();
        let call = app
            .syntax
            .nodes
            .iter()
            .position(|node| node.kind == NodeKind::Call)
            .unwrap();
        let call = report
            .bodies()
            .ordinary_call(app, call)
            .unwrap()
            .expect("canonical get family");
        assert!(matches!(
            call.outcome,
            Some(compiler::analysis::ordinary::HypotheticalOutcome::Winner(_))
        ));
        let role = report.option_role().unwrap();
        let bridge = report.list_lookup_bridge().expect("admitted get bridge");
        let collection = graph
            .modules()
            .iter()
            .find(|module| module.key == bridge.id.module)
            .unwrap();
        let implementation = match call.outcome.unwrap() {
            compiler::analysis::ordinary::HypotheticalOutcome::Winner(id) => id,
            other => panic!("unexpected get selection: {other:?}"),
        };
        let body = graph
            .implementations()
            .iter()
            .find(|body| &body.declaration == implementation)
            .unwrap();
        let return_node = collection.syntax.nodes[body.body_node].children[0];
        let forwarded = collection.syntax.nodes[return_node].children[0];
        let NodeFactKind::Expression(forwarded) = &report
            .bodies()
            .node(collection, forwarded)
            .unwrap()
            .expect("Rill wrapper checks its provider invocation")
            .kind
        else {
            panic!("missing invocation")
        };
        assert_eq!(forwarded.call.as_ref().unwrap().callee, &bridge.id);
        assert!(
            matches!(call.selected_result.as_deref(), Some(TypeTerm::Nominal { declaration, arguments }) if **declaration == role.declaration.id && matches!(arguments.as_slice(), [element] if matches!(element.as_ref(), TypeTerm::Reference { mutable: false, referent } if matches!(referent.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"))))
        );
        fs::write(&request.entry, "
            import std::collection::List;
            function traverse(source: List<i32>): void { for (let item of source) { let _ = item; } }
            function readAll(source: &List<i32>): void { for (let item of source) { let _ = item; } }
            function editAll(source: &mut List<i32>): void { for (let item of source) { continue; } }
        ").unwrap();
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                test_mode,
                ..request.clone()
            })
            .unwrap();
            let iter = graph
                .modules()
                .iter()
                .find(|module| module.key.path == "iter.rill")
                .expect("for-of activates iterator metadata without an explicit import");
            let app = graph
                .modules()
                .iter()
                .find(|module| module.key.origin == Origin::Application)
                .unwrap();
            assert!(
                !graph
                    .bindings()
                    .iter()
                    .any(|b| b.module == app.key && matches!(b.name.as_str(), "iterate" | "next"))
            );
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert!(
                report.bodies().findings().is_empty(),
                "{:?}",
                report.bodies().findings()
            );
            let loops: Vec<_> = app
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == NodeKind::For)
                .collect();
            for ((node, syntax), (name, borrow)) in loops.into_iter().zip([
                ("OwnedListIter", None),
                ("ListIter", Some(false)),
                ("ListIterMut", Some(true)),
            ]) {
                let fact = report
                    .bodies()
                    .node(app, node)
                    .unwrap()
                    .expect("conditional iteration handoff");
                let NodeFactKind::Iteration(handoff) = &fact.kind else {
                    panic!("missing iteration")
                };
                assert!(fact.flow.pending);
                assert_eq!(handoff.source, syntax.children[1]);
                let iterator = iter
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap();
                assert!(
                    matches!(handoff.iterator.as_ref(), TypeTerm::Nominal { declaration, .. } if **declaration == iterator.id)
                );
                let element = match borrow {
                    None => &handoff.yielded,
                    Some(mutable) => match handoff.yielded.as_ref() {
                        TypeTerm::Reference {
                            mutable: actual,
                            referent,
                        } if *actual == mutable => referent,
                        other => panic!("unexpected yield access {other:?}"),
                    },
                };
                assert!(matches!(element.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"));
                for goal in [&handoff.iterate, &handoff.next] {
                    let facts = report.bodies().goal(goal).unwrap();
                    let selection = facts.selection.as_ref().unwrap();
                    assert_eq!(selection.implementation.module, iter.key);
                    assert_eq!(selection.substitution.len(), 1);
                    assert!(facts.premise.is_some());
                }
            }
        }
        fs::write(
            &request.entry,
            "
            type List<T> = { value: T };
            function invalid(): i32 { let xs = [7i32]; return xs[0i32]; }
            function notCanonical(xs: List<i32>): i32 { return xs[0]; }
            function notTuple(): void { let _ = [true, 7i32]; }
            function missingElementType(): void { let _ = []; }
            ",
        )
        .unwrap();
        let graph = loader::load_installed(request).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert!(
            report
                .bodies()
                .findings()
                .iter()
                .any(|finding| finding.kind == ScalarFindingKind::TypeMismatch
                    && finding.site.span.bytes() == b"0i32")
        );
        for (kind, bytes) in [
            (ScalarFindingKind::TypeMismatch, b"xs[0]".as_slice()),
            (ScalarFindingKind::TypeMismatch, b"7i32".as_slice()),
            (ScalarFindingKind::InsufficientContext, b"[]".as_slice()),
        ] {
            assert!(
                report
                    .bodies()
                    .findings()
                    .iter()
                    .any(|finding| finding.kind == kind && finding.site.span.bytes() == bytes),
                "{kind:?} at {bytes:?}"
            );
        }
        check_authority(&root);
        return;
    }
    let root = std::env::temp_dir().join(format!("rill-m48-list-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _cleanup = Cleanup(root.clone());
    for path in ["bin", "lib/rill/std", "app"] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    let executable = root.join("bin/list-probe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
    for module in ["core.rill", "collection.rill", "option.rill", "iter.rill"] {
        fs::copy(source.join(module), root.join("lib/rill/std").join(module)).unwrap();
    }
    fs::write(
        root.join("app/main.rill"),
        "
        function read(): i32 { let xs = [7i32, 8]; return xs[0]; }
        function borrow(at: u64): void { let xs = [7i32]; let view = &xs; let _ = &view[at]; }
        function assign(at: u64): void { let mut xs = [7i32]; xs[at] = 8; }
        expect [true, false][0];
        ",
    )
    .unwrap();
    let output = Command::new(executable)
        .args(["--exact", "installed_list", "--nocapture"])
        .env("RILL_LIST_PROBE_ROOT", &root)
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

fn check_authority(root: &std::path::Path) {
    let entry = root.join("app/authority.rill");
    let request = InstalledRequest {
        entry,
        target: "aarch64-apple-darwin".into(),
        options: Default::default(),
        test_mode: false,
        limits: Limits::default(),
    };
    let standard = root.join("lib/rill/std");
    fs::write(
        &request.entry,
        "import std::collection::List; function build(): List<bool> { return [true]; }",
    )
    .unwrap();
    let selector = |name: &str| Selector {
        module: "core.rill".into(),
        name: name.into(),
    };
    let graph = loader::load(Request {
        entry: request.entry.clone(),
        standard_root: standard.clone(),
        prelude: Prelude {
            copy: selector("copy"),
            dispose: selector("dispose"),
        },
        target: request.target.clone(),
        options: request.options.clone(),
        test_mode: false,
        limits: Limits::default(),
    })
    .unwrap();
    let untrusted = analyze_typed(&graph, AnalysisLimits::default());
    assert!(untrusted.list_role().is_none() && untrusted.list_lookup_bridge().is_none());
    assert!(
        untrusted
            .bodies()
            .obligations()
            .iter()
            .any(|obligation| obligation.reason == ScalarUnsupported::CoreAdmission)
    );

    fs::write(&request.entry, "for (let _ of true) { break; }").unwrap();
    let retained = loader::load_installed(request.clone()).unwrap();
    assert!(
        retained
            .modules()
            .iter()
            .any(|module| module.key.path == "iter.rill")
    );
    assert!(
        analyze_typed(&retained, AnalysisLimits::default())
            .list_role()
            .is_some(),
        "transitive metadata dependency"
    );
    let iter = standard.join("iter.rill");
    let temporary = iter.with_extension("saved");
    fs::rename(&iter, &temporary).unwrap();
    let absent = loader::load_installed(request.clone()).unwrap();
    let missing = analyze_typed(&absent, AnalysisLimits::default());
    assert!(missing.list_role().is_some());
    assert!(
        missing
            .bodies()
            .obligations()
            .iter()
            .any(|obligation| obligation.reason == ScalarUnsupported::CoreAdmission)
    );
    fs::rename(&temporary, &iter).unwrap();
    fs::write(iter, "// not the pinned bytes\n").unwrap();
    assert_eq!(
        loader::load_installed(request).unwrap_err().kind,
        FailureKind::InvalidRequest
    );
    assert_eq!(
        loader::validate_inputs(&retained, Limits::default())
            .unwrap_err()
            .kind,
        FailureKind::SourceChanged
    );
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
