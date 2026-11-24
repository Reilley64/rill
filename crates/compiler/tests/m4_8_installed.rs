//! Exercises installed capture with production core sources and no trust override.
#[path = "support/limit.rs"]
mod limit;

use compiler::analysis::scalars::data::DataExpression;
use compiler::analysis::scalars::{
    BodyOwner, NodeFactKind, PropagationTarget, ScalarFindingKind, ScalarUnsupported,
    generics::BodyCompletion,
};
use compiler::analysis::typed::{CoreBlocker, TypedCompletion, analyze_typed};
use compiler::analysis::{AnalysisLimits, TypeTerm};
use compiler::loader::{
    self, FailureKind, InstalledRequest, Limits, Origin, Prelude, Request, Selector,
};
use compiler::parser::NodeKind;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn installed_option_uses_pinned_sources_in_both_modes() {
    if let Some(root) = std::env::var_os("RILL_INSTALLED_PROBE_ROOT") {
        let root = PathBuf::from(root);
        let request = InstalledRequest {
            entry: root.join("app/main.rill"),
            target: "aarch64-apple-darwin".into(),
            options: BTreeMap::new(),
            test_mode: false,
            limits: Limits::default(),
        };
        installed_option(&request);
        option_guards(&root, &request);
        option_pipe(&root, &request);
        reject_propagation(&root, &request);
        propagation_prerequisites(&root, &request);
        untrusted_profiles(&root, &request);
        metadata_trigger(&root, &request);
        pin_mismatch(&root, request);
        return;
    }

    let root = std::env::temp_dir().join(format!("rill-m48-installed-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _cleanup = Cleanup(root.clone());
    for path in ["bin", "lib/rill/std", "app"] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    let executable = root.join("bin/m48-installed-probe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
    for module in ["core.rill", "option.rill"] {
        fs::copy(source.join(module), root.join("lib/rill/std").join(module)).unwrap();
    }
    fs::write(
        root.join("app/main.rill"),
        "
        import std::option::{Option as Maybe, isSome};
        function propagate(x: Maybe<bool>): Maybe<i32> {
            let b = x?;
            return if (b) Maybe.Some(1i32) else Maybe.None;
        }
        function absent(): Maybe<i32> { let _ = Maybe<bool>.None?; }
        function present(): Maybe<i32> { let _ = Maybe<bool>.Some(true)?; return Maybe.None; }
        expect isSome(&Maybe<bool>.Some(true));
    ",
    )
    .unwrap();
    let output = Command::new(executable)
        .args([
            "--exact",
            "installed_option_uses_pinned_sources_in_both_modes",
            "--nocapture",
        ])
        .env("RILL_INSTALLED_PROBE_ROOT", &root)
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

fn installed_option(profile: &InstalledRequest) {
    for test_mode in [false, true] {
        let graph = loader::load_installed(InstalledRequest {
            test_mode,
            ..profile.clone()
        })
        .unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        let role = report.option_role().expect("admitted Option schema");
        assert_eq!(role.declaration.name.as_deref(), Some("Option"));
        assert_eq!(role.some.name.as_deref(), Some("Some"));
        assert_eq!(role.none.name.as_deref(), Some("None"));
        assert_eq!(role.declaration.id.context, graph.id());
        let provider = report.option_inspection().expect("checked provider body");
        assert_eq!(provider.name.as_deref(), Some("isSome"));
        assert_eq!(provider.id.context, graph.id());
        let option = graph
            .modules()
            .iter()
            .find(|m| m.key == role.declaration.id.module)
            .unwrap();
        for name in ["asRef", "asMut", "map", "andThen"] {
            let helper = option
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some(name))
                .expect(name);
            assert!(
                report
                    .bodies()
                    .body(option, BodyOwner::Declaration(&helper.id))
                    .unwrap()
                    .is_some(),
                "{name}: {:?}; {:?}",
                report.bodies().findings(),
                report.bodies().obligations()
            );
        }
        assert_eq!(report.completion(), TypedCompletion::Incomplete);
        assert_eq!(
            report.core_blockers().len(),
            4,
            "other contracts are still missing"
        );
        assert!(
            report.bodies().findings().is_empty(),
            "{:?}",
            report.bodies().findings()
        );
        let item = graph
            .modules()
            .iter()
            .find(|m| m.key.origin == Origin::Application)
            .unwrap();
        for (name, normal, returns) in [
            ("propagate", true, true),
            ("absent", false, true),
            ("present", true, false),
        ] {
            let declaration = item
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some(name))
                .unwrap();
            let step = item
                .syntax
                .nodes
                .iter()
                .position(|n| {
                    n.kind == NodeKind::Propagate
                        && declaration.span.start() <= n.span.start()
                        && n.span.end() <= declaration.span.end()
                })
                .unwrap();
            let facts = report
                .bodies()
                .node(item, step)
                .unwrap()
                .expect("typed propagation");
            let NodeFactKind::Expression(value) = &facts.kind else {
                panic!("not an expression")
            };
            let Some(DataExpression::Propagation(branch)) = &value.data else {
                panic!("missing propagation branch")
            };
            assert!(
                matches!(branch.target, PropagationTarget::Callable(id) if *id == declaration.id)
            );
            assert_eq!(branch.success, &role.some.id);
            assert_eq!(branch.failure, &role.none.id);
            assert!(matches!(value.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"));
            assert_eq!(
                (facts.flow.normal, facts.flow.returns),
                (normal, returns),
                "{name}"
            );
            assert!(
                report
                    .bodies()
                    .body(item, BodyOwner::Declaration(&declaration.id))
                    .unwrap()
                    .is_some()
            );
        }
        let module_scan = graph
            .modules()
            .iter()
            .position(|m| m.key == option.key)
            .unwrap()
            + 1;
        let binding_scan = graph
            .bindings()
            .iter()
            .position(|b| b.module == option.key && b.name == "Option")
            .unwrap()
            + 1;
        let declaration_scan = option
            .declarations
            .iter()
            .position(|d| d.id == role.declaration.id)
            .unwrap()
            + 1;
        let role_work = report.inventory().statistics().work + module_scan + binding_scan + declaration_scan
            + 3 // declaration, validity and header lookups
            + 1; // admitted role entry
        let at_role = AnalysisLimits {
            max_work: role_work,
            ..AnalysisLimits::default()
        };
        let before_role = AnalysisLimits {
            max_work: role_work - 1,
            ..at_role
        };
        assert!(analyze_typed(&graph, at_role).option_role().is_some());
        let exhausted = analyze_typed(&graph, before_role);
        limit::assert_typed_limit(&exhausted);
        assert!(exhausted.option_role().is_none());

        let short = AnalysisLimits {
            max_work: report.statistics().work - 1,
            ..AnalysisLimits::default()
        };
        limit::assert_typed_limit(&analyze_typed(&graph, short));
    }
}

fn option_guards(root: &Path, profile: &InstalledRequest) {
    let entry = root.join("app/option-guards.rill");
    fs::write(&entry, "
        import std::option::Option as Maybe;
        function direct(value: &Maybe<bool>): bool { guard value else { return false; } return true; }
        function payload(value: Maybe<bool>): bool { guard let actual: bool = value else { return false; } return actual; }
        function borrowed(value: &Maybe<string>, fallback: &string): &string { guard let text = value else { return fallback; } return text; }
        function refutable(value: Maybe<[bool, i32]>): i32 { guard let [true, n] = value else { return 0i32; } return n; }
        function onlyFailure(): bool { guard Maybe<bool>.None else { return false; } }
    ").unwrap();
    for test_mode in [false, true] {
        let graph = loader::load_installed(InstalledRequest {
            entry: entry.clone(),
            test_mode,
            ..profile.clone()
        })
        .unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert!(report.bodies().findings().is_empty());
        let app = graph
            .modules()
            .iter()
            .find(|module| module.key.origin == Origin::Application)
            .unwrap();
        for declaration in app
            .declarations
            .iter()
            .filter(|d| d.id.kind == loader::DeclarationKind::Function)
        {
            assert_eq!(
                report
                    .bodies()
                    .body_status(app, BodyOwner::Declaration(&declaration.id))
                    .unwrap()
                    .completion,
                BodyCompletion::Checked,
                "{:?}",
                declaration.name
            );
        }
    }
}

fn option_pipe(root: &Path, profile: &InstalledRequest) {
    let entry = root.join("app/option-pipe.rill");
    fs::write(&entry, "
        import std::option::Option as Maybe;
        function receive(flag: bool, offset: i32): Maybe<i32> { return if (flag) Maybe.Some(offset) else Maybe.None; }
        function piped(value: bool): Maybe<bool> {
            let n: i32 = value |> receive(3)?;
            return Maybe.Some(n > 0);
        }
        function callable(value: bool, receiver: (bool) => Maybe<i32>): Maybe<bool> {
            let n = value |> receiver?;
            return Maybe.Some(n > 0);
        }
    ").unwrap();
    let graph = loader::load_installed(InstalledRequest {
        entry,
        ..profile.clone()
    })
    .unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert!(report.bodies().findings().is_empty());
    let app = graph
        .modules()
        .iter()
        .find(|module| module.key.origin == Origin::Application)
        .unwrap();
    let mut count = 0;
    for (node, syntax) in app.syntax.nodes.iter().enumerate() {
        if syntax.kind != NodeKind::Pipe {
            continue;
        }
        let fact = report
            .bodies()
            .node(app, node)
            .unwrap()
            .expect("checked pipe");
        let NodeFactKind::Expression(expression) = &fact.kind else {
            panic!("expression")
        };
        assert!(matches!(expression.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"));
        assert!(matches!(
            expression.data,
            Some(DataExpression::Propagation(_))
        ));
        assert!(fact.flow.normal && fact.flow.returns);
        if let Some(call) = &expression.call {
            assert_eq!(call.pipe_input, Some(syntax.children[0]));
            assert_eq!(call.argument_evaluation_order().count(), 2);
        }
        count += 1;
    }
    assert_eq!(count, 2);
}

fn reject_propagation(root: &Path, profile: &InstalledRequest) {
    let entry = root.join("app/invalid.rill");
    fs::write(&entry, "import std::option::Option; function borrowed(x: &Option<bool>): Option<bool> { let b = x?; return Option.Some(b); } function wrong(x: Option<bool>): bool { return x?; } enum Local { Item } function wrongWithoutRole(x: Local): bool { return x?; }").unwrap();
    let graph = loader::load_installed(InstalledRequest {
        entry,
        ..profile.clone()
    })
    .unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert_eq!(
        report
            .bodies()
            .findings()
            .iter()
            .filter(|f| f.kind == ScalarFindingKind::InvalidTarget)
            .count(),
        3
    );
}

fn propagation_prerequisites(root: &Path, profile: &InstalledRequest) {
    let entry = root.join("app/prerequisites.rill");
    fs::write(&entry, "
        import std::option::Option;
        enum Local { Item }
        function invalidOperand(): Option<bool> { let _ = missing?; return Option.None; }
        function pendingArrow(x: Local): void { let cb: () => Local = () => { let _ = x?; }; }
        function fallsThrough(x: Option<bool>): void { let cb: () => Option<bool> = () => { let _ = x?; }; }
    ").unwrap();
    let graph = loader::load_installed(InstalledRequest {
        entry,
        ..profile.clone()
    })
    .unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let body = |name| {
        app.declarations
            .iter()
            .find(|d| d.name.as_deref() == Some(name))
            .unwrap()
    };
    let invalid = body("invalidOperand");
    assert_eq!(
        report
            .bodies()
            .body_status(app, BodyOwner::Declaration(&invalid.id))
            .unwrap()
            .completion,
        BodyCompletion::SourceRejected
    );
    assert!(
        report
            .bodies()
            .findings()
            .iter()
            .any(|f| f.kind == ScalarFindingKind::MissingValue)
    );
    let pending = body("pendingArrow");
    let core_blockers: Vec<_> = report
        .bodies()
        .obligations()
        .iter()
        .filter(|o| o.reason == ScalarUnsupported::CoreAdmission)
        .collect();
    assert_eq!(core_blockers.len(), 1);
    assert!(core_blockers[0].site.span.start() >= pending.span.start());
    assert!(core_blockers[0].site.span.end() <= pending.span.end());
    let coverage: Vec<_> = report
        .bodies()
        .findings()
        .iter()
        .filter(|f| f.kind == ScalarFindingKind::ReturnCoverage)
        .collect();
    assert_eq!(coverage.len(), 1);
    assert!(coverage[0].site.span.start() >= body("fallsThrough").span.start());
}

fn untrusted_profiles(root: &Path, profile: &InstalledRequest) {
    let select = |name: &str| Selector {
        module: "core.rill".into(),
        name: name.into(),
    };
    let source = loader::load(Request {
        entry: profile.entry.clone(),
        standard_root: root.join("lib/rill/std"),
        prelude: Prelude {
            copy: select("copy"),
            dispose: select("dispose"),
        },
        target: profile.target.clone(),
        options: profile.options.clone(),
        test_mode: false,
        limits: Limits::default(),
    })
    .unwrap();
    let unknown_target = loader::load_installed(InstalledRequest {
        target: "x86_64-apple-darwin".into(),
        ..profile.clone()
    })
    .unwrap();
    let unknown_options = loader::load_installed(InstalledRequest {
        options: [("unsupported".into(), "1".into())].into(),
        ..profile.clone()
    })
    .unwrap();
    for graph in [&source, &unknown_target, &unknown_options] {
        let report = analyze_typed(graph, AnalysisLimits::default());
        assert!(report.option_role().is_none() && report.option_inspection().is_none());
        assert!(report.core_blockers().contains(&CoreBlocker::Roles));
        assert!(
            report
                .bodies()
                .obligations()
                .iter()
                .any(|o| o.reason == ScalarUnsupported::CoreAdmission)
        );
    }
}

fn metadata_trigger(root: &Path, profile: &InstalledRequest) {
    let entry = root.join("app/metadata.rill");
    fs::write(&entry, "expect true;").unwrap();
    let request = InstalledRequest {
        entry,
        ..profile.clone()
    };
    let idle = loader::load_installed(request.clone()).unwrap();
    assert_eq!(idle.modules().len(), 3, "metadata loads without a trigger");
    fs::write(&request.entry, "enum Local<T> { Some(T), None } function pending(x: Local<bool>): Local<bool> { let _ = x?; return x; }").unwrap();
    let graph = loader::load_installed(request.clone()).unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert!(report.option_inspection().is_some());
    assert!(
        report
            .bodies()
            .obligations()
            .iter()
            .any(|o| o.reason == ScalarUnsupported::CoreAdmission)
    );
    assert!(
        !graph
            .bindings()
            .iter()
            .any(|b| b.module.origin == Origin::Application && b.name == "Option")
    );
    let mut short = request.clone();
    short.limits.max_modules = 2;
    assert_eq!(
        loader::load_installed(short).unwrap_err().kind,
        FailureKind::ResourceLimit
    );

    let installed = root.join("lib/rill/std/option.rill");
    let temporary = installed.with_extension("saved");
    fs::rename(&installed, &temporary).unwrap();
    let absent = loader::load_installed(request).unwrap();
    fs::rename(&temporary, installed).unwrap();
    let report = analyze_typed(&absent, AnalysisLimits::default());
    assert!(report.option_role().is_none());
    assert!(
        report
            .bodies()
            .obligations()
            .iter()
            .any(|o| o.reason == ScalarUnsupported::CoreAdmission)
    );
}

fn pin_mismatch(root: &Path, request: InstalledRequest) {
    let retained = loader::load_installed(request.clone()).unwrap();
    fs::write(
        root.join("lib/rill/std/option.rill"),
        b"!unparseable pinned source\n",
    )
    .unwrap();
    let rejected = loader::load_installed(request).unwrap_err();
    assert_eq!(rejected.kind, FailureKind::InvalidRequest);
    assert_eq!(
        loader::validate_inputs(&retained, Limits::default())
            .unwrap_err()
            .kind,
        FailureKind::SourceChanged
    );
    assert!(
        analyze_typed(&retained, AnalysisLimits::default())
            .option_inspection()
            .is_some()
    );
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
