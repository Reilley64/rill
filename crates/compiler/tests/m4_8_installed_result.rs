//! Installed Result checks use production sources and the actual installed entry point.
use compiler::analysis::scalars::{NodeFactKind, ScalarFailureKind};
use compiler::analysis::typed::analyze_typed;
use compiler::analysis::{AnalysisLimits, TypeTerm};
use compiler::loader::{self, InstalledRequest, Limits, Origin};
use compiler::parser::NodeKind;
use std::{fs, path::PathBuf, process::Command};

#[test]
fn installed_result_discards_keep_original_storage_and_early_exit() {
    installed(
        "installed_result_discards_keep_original_storage_and_early_exit",
        |request| {
            fs::write(
                &request.entry,
                "
            import std::result::{Result as Outcome, isOk};
            function make(): Outcome<bool> { return Outcome.Ok(true); }
            function discardTemporary(): void { make(); }
            function renamed(): void { let x = make(); let y = x; }
            function project(first: Outcome<bool>, second: Outcome<bool>): Outcome<bool> {
                let pair: [Outcome<bool>, Outcome<bool>] = [first, second];
                return pair[0];
            }
            function observe(value: Outcome<bool>): void { isOk(&value); }
            function inspectTemporary(): void { isOk(&make()); }
            function inspectAlias(value: Outcome<bool>): void { let alias = &value; isOk(alias); }
            function propagate(outer: Outcome<bool>, sibling: Outcome<bool>): Outcome<bool> {
                let b = outer?;
                let _ = sibling;
                return Outcome.Ok(b);
            }
            function relay(input: Outcome<bool>): Outcome<bool> { return input; }
            function propagatePipe(outer: Outcome<bool>, sibling: Outcome<bool>): Outcome<bool> {
                let b = outer |> relay?;
                let _ = sibling;
                return Outcome.Ok(b);
            }
        ",
            )
            .unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                let result = report.result_role().expect("installed Result identity");
                assert_eq!(result.declaration.id.context, graph.id());
                let uses = report.result_use().expect("source-level Result-use pass");
                let witnesses: Vec<_> = uses
                    .discards()
                    .iter()
                    .filter(|f| f.origin.module.origin == Origin::Application)
                    .collect();
                assert_eq!(
                    witnesses.len(),
                    5,
                    "{:?}",
                    witnesses
                        .iter()
                        .map(|w| (
                            String::from_utf8_lossy(w.origin.span.bytes()),
                            String::from_utf8_lossy(w.discard.span.bytes())
                        ))
                        .collect::<Vec<_>>()
                );
                assert!(witnesses.iter().all(|f| f.code() == "R0103"));
                assert!(
                    witnesses
                        .iter()
                        .any(|w| w.discard.span.bytes() == b"make();")
                );
                assert!(witnesses.iter().any(|w| w.storage_component
                    == [compiler::analysis::typed::ResultComponent::TupleElement(1)]));
                for span in [b"outer?".as_slice(), b"outer |> relay?"] {
                    assert!(witnesses.iter().any(|w| w.discard.span.bytes() == span
                        && w.origin.span.bytes().starts_with(b"sibling:")));
                }
            }
        },
    );
}

#[test]
fn installed_result_default_is_canonical_without_an_error_binding() {
    installed(
        "installed_result_default_is_canonical_without_an_error_binding",
        |request| {
            fs::write(
                &request.entry,
                "
            import std::result::{Result as Outcome, Ok};
            function success(): Outcome<bool> { return Ok(true); }
        ",
            )
            .unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                assert_eq!(report.completion(), ScalarFailureKind::Incomplete);
                assert!(
                    report.bodies().findings().is_empty(),
                    "{:?}",
                    report.bodies().findings()
                );
                assert!(
                    !graph
                        .bindings()
                        .iter()
                        .any(|b| b.module.origin == Origin::Application && b.name == "Error")
                );
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
                    .expect("typed Ok call");
                let NodeFactKind::Expression(value) = &facts.kind else {
                    panic!("not an expression")
                };
                let TypeTerm::Nominal {
                    declaration,
                    arguments,
                } = value.ty.as_ref()
                else {
                    panic!("not a Result")
                };
                let role = report.result_role().expect("admitted Result schema");
                assert_eq!(declaration, &&role.declaration.id);
                assert_eq!(role.declaration.id.context, graph.id());
                let inspection = report.result_inspection().expect("checked isOk provider");
                assert_eq!(inspection.name.as_deref(), Some("isOk"));
                assert_eq!(inspection.id.context, graph.id());
                assert_eq!(role.ok.name.as_deref(), Some("Ok"));
                assert_eq!(role.err.name.as_deref(), Some("Err"));
                assert_eq!(role.wrapper.name.as_deref(), Some("Err"));
                for (id, names) in [
                    (
                        &role.declaration.id,
                        &[
                            "Ok",
                            "isOk",
                            "ok",
                            "map",
                            "andThen",
                            "mapErr",
                            "value",
                            "intoValue",
                        ][..],
                    ),
                    (&role.default_error.id, &["error"][..]),
                ] {
                    let module = graph.modules().iter().find(|m| m.key == id.module).unwrap();
                    for name in names {
                        use compiler::analysis::scalars::BodyOwner;
                        let body = module
                            .declarations
                            .iter()
                            .find(|d| {
                                d.id.kind == loader::DeclarationKind::Function
                                    && d.name.as_deref() == Some(name)
                            })
                            .expect(name);
                        assert!(
                            report
                                .bodies()
                                .body(module, BodyOwner::Declaration(&body.id))
                                .unwrap()
                                .is_some(),
                            "{name}: {:?}",
                            report.bodies().obligations()
                        );
                    }
                }
                assert!(
                    matches!(arguments[1].as_ref(), TypeTerm::Nominal { declaration, arguments } if *declaration == &role.default_error.id && arguments.is_empty())
                );
            }
            let selectors = |name: &str| loader::Selector {
                module: "core.rill".into(),
                name: name.into(),
            };
            let untrusted = loader::load(loader::Request {
                entry: request.entry.clone(),
                standard_root: request.entry.parent().unwrap().join("../lib/rill/std"),
                prelude: loader::Prelude {
                    copy: selectors("copy"),
                    dispose: selectors("dispose"),
                },
                target: request.target.clone(),
                options: request.options.clone(),
                test_mode: false,
                limits: request.limits,
            })
            .unwrap();
            let report = analyze_typed(&untrusted, AnalysisLimits::default());
            assert!(report.result_role().is_none());
            assert!(report.result_inspection().is_none());
            assert!(report.error_storage_source().is_none());
            let source = request
                .entry
                .parent()
                .unwrap()
                .join("../lib/rill/std/error.rill");
            let original = fs::read_to_string(&source).unwrap();
            fs::write(source, format!("{original}\n// not the installed pin\n")).unwrap();
            assert_eq!(
                loader::load_installed(request).unwrap_err().kind,
                loader::FailureKind::InvalidRequest
            );
        },
    );
}

#[test]
fn installed_result_guards_check_canonical_payload_and_wrapper_access() {
    installed(
        "installed_result_guards_check_canonical_payload_and_wrapper_access",
        |request| {
            fs::write(
                &request.entry,
                "
            import std::result::{Result as Outcome, Err as Wrapped, value, intoValue};
            function direct(value: &Outcome<bool, string>): bool {
                guard value else { return false; }
                return true;
            }
            function owned(input: Outcome<bool, string>): bool {
                guard let bit = input else (failure) {
                    let wrapper: Wrapped<string> = failure;
                    let message: string = intoValue(wrapper);
                    return false;
                }
                return bit;
            }
            function borrowed(input: &Outcome<string, string>): &string {
                guard let message = input else (failure) {
                    let wrapper: &Wrapped<string> = failure;
                    return value(wrapper);
                }
                return message;
            }
        ",
            )
            .unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                assert!(
                    report.bodies().findings().is_empty(),
                    "{:?}",
                    report.bodies().findings()
                );
                assert!(
                    !report
                        .bodies()
                        .obligations()
                        .iter()
                        .any(|o| o.site.module.origin == Origin::Application),
                    "{:?}",
                    report.bodies().obligations()
                );
            }
        },
    );
}

#[test]
fn installed_postfix_forwards_only_exact_e() {
    installed("installed_postfix_forwards_only_exact_e", |request| {
        fs::write(
            &request.entry,
            "
            import std::result::Result as Outcome;
            import std::option::Option;
            function exact<E>(input: Outcome<bool, E>): Outcome<i32, E> {
                let condition = input?;
                return Outcome.Ok(if (condition) 1i32 else 0i32);
            }
            function unequal(input: Outcome<bool, i32>): Outcome<i32, u32> {
                let _ = input?;
                return Outcome.Ok(0i32);
            }
            function borrowed(input: &Outcome<bool, i32>): Outcome<i32, i32> {
                let _ = input?;
                return Outcome.Ok(0i32);
            }
            function wrongTarget(input: Outcome<bool, i32>): Option<i32> {
                let _ = input?;
                return Option.None;
            }
        ",
        )
        .unwrap();
        let graph = loader::load_installed(request).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        let app = graph
            .modules()
            .iter()
            .find(|m| m.key.origin == Origin::Application)
            .unwrap();
        let mut steps = app
            .syntax
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == NodeKind::Propagate);
        let (first, _) = steps.next().unwrap();
        let fact = report
            .bodies()
            .node(app, first)
            .unwrap()
            .expect("typed exact-E propagation");
        let NodeFactKind::Expression(value) = &fact.kind else {
            panic!("not an expression")
        };
        let Some(compiler::analysis::scalars::data::DataExpression::Propagation(edge)) =
            &value.data
        else {
            panic!("no checked propagation")
        };
        let role = report.result_role().unwrap();
        let failure = edge.error.as_ref().unwrap();
        assert_eq!(edge.success, &role.ok.id);
        assert_eq!(edge.failure, &role.err.id);
        assert_eq!(failure.wrapper, &role.wrapper.id);
        assert!(
            matches!(failure.payload.as_ref(), TypeTerm::Parameter { declaration, .. } if declaration.module == app.key)
        );
        assert!(matches!(value.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool"));
        assert!(fact.flow.normal && fact.flow.returns);
        for ((node, _), kind) in steps.zip([
            compiler::analysis::scalars::ScalarFindingKind::TypeMismatch,
            compiler::analysis::scalars::ScalarFindingKind::InvalidTarget,
            compiler::analysis::scalars::ScalarFindingKind::InvalidTarget,
        ]) {
            assert!(
                report
                    .bodies()
                    .findings()
                    .iter()
                    .any(|f| f.site.node == node && f.site.module == &app.key && f.kind == kind),
                "{:?}",
                report.bodies().findings()
            );
        }
    });
}

#[test]
fn installed_capture_replays_its_error_storage_input() {
    installed(
        "installed_capture_replays_its_error_storage_input",
        |request| {
            fs::write(&request.entry, "import std::result::Result;").unwrap();
            let graph = loader::load_installed(request.clone()).unwrap();
            let evidence = analyze_typed(&graph, AnalysisLimits::default());
            assert!(
                evidence
                    .error_storage_source()
                    .unwrap()
                    .ends_with("runtime/aarch64-apple-darwin/error.rs")
            );
            let artifact = request
                .entry
                .parent()
                .unwrap()
                .join("../lib/rill/runtime/aarch64-apple-darwin/error.rs");
            fs::write(&artifact, "// not the pinned implementation\n").unwrap();
            assert_eq!(
                loader::validate_inputs(&graph, Limits::default())
                    .unwrap_err()
                    .kind,
                loader::FailureKind::SourceChanged
            );
            assert_eq!(
                loader::load_installed(request.clone()).unwrap_err().kind,
                loader::FailureKind::InvalidRequest
            );
            fs::remove_file(artifact).unwrap();
            let absent = loader::load_installed(request).unwrap();
            let report = analyze_typed(&absent, AnalysisLimits::default());
            assert!(report.result_role().is_some());
            assert!(report.error_storage_source().is_none());
            assert!(
                report
                    .core_blockers()
                    .contains(&compiler::analysis::typed::CoreBlocker::Representation)
            );
            assert_eq!(report.completion(), ScalarFailureKind::Incomplete);
        },
    );
}

#[test]
fn installed_closed_error_checks_do_not_invent_wrappers() {
    installed(
        "installed_closed_error_checks_do_not_invent_wrappers",
        |request| {
            fs::write(&request.entry, "
            import std::result::{Result, Err as Make};
            function noValue(): Result<bool, i32> { return Make<bool, i32>(); }
            function bare(): void { let _ = Make; }
            function cause(inner: Make<i32>): Result<bool, i32> { return Make<bool, i32, i32>(1i32, inner); }
            function wrongOuter(input: Result<bool, i32>): Result<bool, i32> {
                let _ = input ? true;
                return Result.Ok(true);
            }
            function pendingOuter(input: Result<bool, i32>): Result<bool, i32> {
                let _ = input ? 7;
                return Result.Ok(true);
            }
        ").unwrap();
            let graph = loader::load_installed(request).unwrap();
            let report = analyze_typed(&graph, AnalysisLimits::default());
            use compiler::analysis::scalars::{ScalarFindingKind, ScalarUnsupported};
            let role = report.result_role().unwrap();
            assert!(
                role.constructor.is_some(),
                "no closed role; issues={:?}",
                report.inventory().declaration_issues()
            );
            let app = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application)
                .unwrap();
            let findings: Vec<_> = report
                .bodies()
                .findings()
                .iter()
                .filter(|f| f.site.module == &app.key)
                .map(|f| f.kind)
                .collect();
            assert_eq!(
                findings,
                [
                    ScalarFindingKind::Arity,
                    ScalarFindingKind::InvalidTarget,
                    ScalarFindingKind::TypeMismatch
                ]
            );
            let pending: Vec<_> = report
                .bodies()
                .obligations()
                .iter()
                .filter(|o| o.site.module == &app.key)
                .collect();
            assert_eq!(pending.len(), 2, "{pending:?}");
            for obligation in pending {
                assert_eq!(obligation.reason, ScalarUnsupported::CoreAdmission);
                assert!(
                    report
                        .bodies()
                        .node(app, obligation.site.node)
                        .unwrap()
                        .is_none()
                );
            }
        },
    );
}

fn installed(test: &str, check: impl FnOnce(InstalledRequest)) {
    if let Some(root) = std::env::var_os("RILL_RESULT_PROBE_ROOT") {
        check(InstalledRequest {
            entry: PathBuf::from(root).join("app/main.rill"),
            target: "aarch64-apple-darwin".into(),
            options: Default::default(),
            test_mode: false,
            limits: Limits::default(),
        });
        return;
    }
    let root = std::env::temp_dir().join(format!("rill-{test}-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _cleanup = Cleanup(root.clone());
    for dir in ["bin", "lib/rill/std", "app"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    let runtime = root.join("lib/rill/runtime/aarch64-apple-darwin");
    fs::create_dir_all(&runtime).unwrap();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/error.rs"),
        runtime.join("error.rs"),
    )
    .unwrap();
    let executable = root.join("bin/probe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
    for module in ["core.rill", "option.rill", "result.rill", "error.rill"] {
        fs::copy(source.join(module), root.join("lib/rill/std").join(module)).unwrap();
    }
    let output = Command::new(executable)
        .args(["--exact", test, "--nocapture"])
        .env("RILL_RESULT_PROBE_ROOT", &root)
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

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
