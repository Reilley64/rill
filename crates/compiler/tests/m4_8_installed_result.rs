//! Installed Result checks use production sources and the actual installed entry point.
use compiler::analysis::scalars::{BodyOwner, NodeFactKind, Site};
use compiler::analysis::typed::{TypedCompletion, analyze_typed};
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
            import std::collection::List;
            function make(): Outcome<bool> { return Outcome.Ok(true); }
            function discardTemporary(): void { make(); }
            function renamed(): void { let x = make(); let y = x; }
            function project(first: Outcome<bool>, second: Outcome<bool>): Outcome<bool> {
                let pair: [Outcome<bool>, Outcome<bool>] = [first, second];
                return pair[0];
            }
            function observe(value: Outcome<bool>): void { isOk(&value); }
            function inspectTemporary(): void { isOk(&make()); }
            function inspectAlias(value: Outcome<bool>, untouched: Outcome<bool>): void { let alias = &value; isOk(alias); }
            function dead(): void { if (false) { make(); } }
            function opaque(xs: List<Outcome<bool>>): void { }
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
                    7,
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
                        .any(|w| w.origin.span.bytes().starts_with(b"untouched:"))
                );
                assert!(
                    uses.pending()
                        .iter()
                        .any(|p| p.site.span.bytes().starts_with(b"xs: List<")
                            && p.blocker
                                == compiler::analysis::typed::ResultUseBlocker::Containment)
                );
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
fn installed_owned_iteration_retains_unyielded_result_on_break() {
    installed(
        "installed_owned_iteration_retains_unyielded_result_on_break",
        |request| {
            fs::write(
                &request.entry,
                "
            import std::result::Result;
            function first(): Result<bool> { return Result.Ok(true); }
            function second(): Result<bool> { return Result.Ok(false); }
            function stop(): void {
                for (let item of [first(), second()]) { let _ = item; break; }
            }
            function leave(): void {
                for (let item of [first(), second()]) { let _ = item; return; }
            }
            function drain(): void {
                for (let [item, _] of [[first(), second()], [second(), first()]]) {
                    let _ = item;
                    continue;
                }
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
                let app = graph
                    .modules()
                    .iter()
                    .find(|m| m.key.origin == Origin::Application)
                    .unwrap();
                let iteration = app
                    .syntax
                    .nodes
                    .iter()
                    .position(|n| n.kind == NodeKind::For)
                    .unwrap();
                assert!(matches!(
                    report
                        .bodies()
                        .node(app, iteration)
                        .unwrap()
                        .map(|n| &n.kind),
                    Some(NodeFactKind::Iteration(_))
                ));
                let uses = report.result_use().unwrap();
                let witnesses = uses
                    .discards()
                    .iter()
                    .filter(|witness| witness.origin.module.origin == Origin::Application)
                    .collect::<Vec<_>>();
                assert_eq!(
                    witnesses.len(),
                    2,
                    "expected one unyielded outcome per early exit"
                );
                for exit in [b"break;".as_slice(), b"return;"] {
                    assert!(
                        witnesses
                            .iter()
                            .any(|w| w.origin.span.bytes() == b"second()"
                                && w.discard.span.bytes() == exit)
                    );
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
                assert_eq!(report.completion(), TypedCompletion::Incomplete);
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
fn installed_invalid_core_operands_do_not_become_pending() {
    installed(
        "installed_invalid_core_operands_do_not_become_pending",
        |request| {
            fs::write(
                &request.entry,
                "
            import std::result::Result;
            import std::option::Option;
            type Plain = { payload: bool; };
            function invalid(value: Plain): Result<bool> {
                let _ = value?;
                return Result.Ok(true);
            }
            function refutable(value: Result<[bool, i32]>): i32 {
                guard let [true, n] = value else (err) { return 0i32; }
                return n;
            }
        ",
            )
            .unwrap();
            let graph = loader::load_installed(request).unwrap();
            let report = analyze_typed(&graph, AnalysisLimits::default());
            use compiler::analysis::scalars::ScalarFindingKind;
            assert!(
                report
                    .bodies()
                    .findings()
                    .iter()
                    .any(|f| f.kind == ScalarFindingKind::InvalidTarget
                        && f.site.span.bytes() == b"value?")
            );
            assert!(
                report
                    .bodies()
                    .findings()
                    .iter()
                    .any(|f| f.kind == ScalarFindingKind::RefutableBinding
                        && f.site.span.bytes() == b"[true, n]")
            );
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
fn installed_pipe_constructs_then_propagates_only_failure() {
    installed(
        "installed_pipe_constructs_then_propagates_only_failure",
        |request| {
            fs::write(&request.entry, "
            import std::result::{Result, Err as Make};
            function propagate(message: string, sibling: Result<bool, string>): Result<bool, string> {
                let _ = message |> Make<i32, string>?;
                let _ = sibling;
                return Result.Ok(true);
            }
        ").unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                let app = graph
                    .modules()
                    .iter()
                    .find(|m| m.key.origin == Origin::Application)
                    .unwrap();
                let pipe = app
                    .syntax
                    .nodes
                    .iter()
                    .position(|n| n.kind == NodeKind::Pipe)
                    .unwrap();
                let fact = report
                    .bodies()
                    .node(app, pipe)
                    .unwrap()
                    .expect("checked Err pipe");
                assert!(fact.flow.returns && !fact.flow.normal);
                assert!(
                    report.result_use().unwrap().discards().iter().any(|w| w
                        .origin
                        .span
                        .bytes()
                        .starts_with(b"sibling:")
                        && w.discard.node == pipe)
                );
            }
        },
    );
}

#[test]
fn installed_error_rendering_uses_the_checked_display_requirement() {
    installed(
        "installed_error_rendering_uses_the_checked_display_requirement",
        |request| {
            fs::write(&request.entry, "
            import std::result::Err as Wrapped;
            import std::error::{Cause, SourceLocation, display, formatError, formatCause, formatLocation, origin, cause};
            import std::option::Option;
            function causeOf(value: &Wrapped<i32>): Option<&Cause> { return cause(value); }
            function nested(value: &Cause): Option<&Cause> { return cause(value); }
            function originOf(value: &Wrapped<i32>): &SourceLocation { return origin(value); }
            function original(value: &Cause): &SourceLocation { return origin(value); }
            type Plain = { flag: bool; };
            function render<T>(value: &Wrapped<T>): string where display(T) {
                return formatError<T>(value);
            }
            function renderCause(value: &Cause): string { return formatCause(value); }
            function renderLocation(value: &SourceLocation): string { return formatLocation(value); }
            function interpolate(value: &Wrapped<i32>): string { return `${*value}`; }
            function invalid(value: &Wrapped<Plain>): string { return formatError<Plain>(value); }
        ").unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                let providers = report.error_providers().unwrap();
                let bindings = providers
                    .renderers
                    .map(|binding| binding.unwrap().operation.rust_binding());
                assert_eq!(
                    bindings,
                    [
                        "rill_error_storage::SourceLocation::format",
                        "rill_error_storage::Cause::format",
                        "rill_error_storage::OwnedErr::format"
                    ]
                );
                assert_eq!(
                    providers
                        .observers
                        .map(|binding| binding.unwrap().operation.rust_binding()),
                    [
                        "rill_error_storage::Cause::origin",
                        "rill_error_storage::Cause::cause"
                    ]
                );
                let role = report.result_role().unwrap();
                let observers = [role.origin.unwrap(), role.observe_cause.unwrap()];
                for observer in observers {
                    let implementations = graph
                        .implementations()
                        .iter()
                        .filter(|entry| entry.family == observer.id);
                    for implementation in implementations {
                        let owner = graph
                            .modules()
                            .iter()
                            .find(|module| module.key == implementation.declaration.module)
                            .unwrap();
                        assert!(
                            report
                                .bodies()
                                .body(
                                    owner,
                                    compiler::analysis::scalars::BodyOwner::Declaration(
                                        &implementation.declaration
                                    )
                                )
                                .unwrap()
                                .is_some(),
                            "{} in {}",
                            observer.name.as_deref().unwrap(),
                            owner.key.path
                        );
                    }
                }
                let app = graph
                    .modules()
                    .iter()
                    .find(|m| m.key.origin == Origin::Application)
                    .unwrap();
                for (name, checked) in [
                    ("causeOf", true),
                    ("nested", true),
                    ("originOf", true),
                    ("original", true),
                    ("render", true),
                    ("renderCause", true),
                    ("renderLocation", true),
                    ("interpolate", true),
                    ("invalid", false),
                ] {
                    let d = app
                        .declarations
                        .iter()
                        .find(|d| d.name.as_deref() == Some(name))
                        .unwrap();
                    assert_eq!(
                        report
                            .bodies()
                            .body(
                                app,
                                compiler::analysis::scalars::BodyOwner::Declaration(&d.id)
                            )
                            .unwrap()
                            .is_some(),
                        checked,
                        "{name}"
                    );
                }
            }
        },
    );
}

#[test]
fn installed_capture_replays_its_error_storage_input() {
    installed(
        "installed_capture_replays_its_error_storage_input",
        |request| {
            fs::write(
                &request.entry,
                r#"
                import std::result::{Result, Err};
                function fail(): Result<bool, string> { return Err("message"); }
            "#,
            )
            .unwrap();
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
            assert!(
                analyze_typed(&graph, AnalysisLimits::default())
                    .error_providers()
                    .is_some()
            );
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
            assert!(report.error_providers().is_none());
            assert!(
                report
                    .bodies()
                    .obligations()
                    .iter()
                    .any(|o| o.site.module.origin == Origin::Application
                        && o.site.span.bytes() == br#"Err("message")"#
                        && o.reason
                            == compiler::analysis::scalars::ScalarUnsupported::CoreAdmission)
            );
            assert!(
                report
                    .core_blockers()
                    .contains(&compiler::analysis::typed::CoreBlocker::Representation)
            );
            assert_eq!(report.completion(), TypedCompletion::Incomplete);
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
            function wrapNumeric(input: Result<bool, i32>): Result<bool, i32> {
                let _ = input ? 7;
                return Result.Ok(true);
            }
        ").unwrap();
            let graph = loader::load_installed(request).unwrap();
            let report = analyze_typed(&graph, AnalysisLimits::default());
            use compiler::analysis::scalars::ScalarFindingKind;
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
            assert!(pending.is_empty(), "{pending:?}");
        },
    );
}

#[test]
fn installed_aliased_closed_err_constructs_with_context() {
    installed(
        "installed_aliased_closed_err_constructs_with_context",
        |request| {
            fs::write(
                &request.entry,
                r#"
            import std::result::{Result as Outcome, Err as Make};
            import std::error::display;
            function fail(): Outcome<bool, string> { return Make("failed"); }
            function numeric(): Outcome<bool, i32> { return Make<bool, i32>(17); }
            function inferred(): void { let _ = Make<bool>("independent"); }
            function generic<E>(payload: E): Outcome<bool, E> where display(E) {
                return Make(payload);
            }
        "#,
            )
            .unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                let app = graph
                    .modules()
                    .iter()
                    .find(|m| m.key.origin == Origin::Application)
                    .unwrap();
                use compiler::analysis::scalars::{data::DataExpression, generics::GoalState};
                let exact = AnalysisLimits {
                    max_work: report.statistics().work,
                    ..AnalysisLimits::default()
                };
                assert_eq!(
                    analyze_typed(&graph, exact).statistics(),
                    report.statistics()
                );
                assert_eq!(
                    analyze_typed(
                        &graph,
                        AnalysisLimits {
                            max_work: exact.max_work - 1,
                            ..exact
                        }
                    )
                    .completion(),
                    TypedCompletion::ResourceLimit
                );
                let role = report.result_role().unwrap();
                for (node, syntax) in app
                    .syntax
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.kind == NodeKind::Call)
                {
                    let fact = report
                        .bodies()
                        .node(app, node)
                        .unwrap()
                        .expect("checked closed Err");
                    let NodeFactKind::Expression(value) = &fact.kind else {
                        panic!("typed expression");
                    };
                    let Some(DataExpression::ErrorConstruction(error)) = &value.data else {
                        panic!("closed Err fact");
                    };
                    assert_eq!(error.origin.start(), syntax.span.start());
                    assert_eq!(error.origin.snapshot(), &app.snapshot);
                    assert!(error.retained.is_none());
                    assert_eq!(
                        value.call.as_ref().unwrap().callee,
                        &role.constructor.unwrap().id
                    );
                    let display = report.bodies().goal(&error.display).unwrap();
                    assert_eq!(display.state, GoalState::Proved);
                    if syntax.span.bytes() == b"Make(payload)" {
                        assert!(display.selection.is_none());
                        assert_eq!(display.dependencies.len(), 1, "generic assumption");
                    } else {
                        assert_eq!(
                            display
                                .selection
                                .as_ref()
                                .unwrap()
                                .implementation
                                .module
                                .path,
                            "error.rill"
                        );
                    }
                }
            }
        },
    );
}

#[test]
fn installed_wrap_preserves_actual_cause_exact_e_and_lazy_control() {
    installed(
        "installed_wrap_preserves_actual_cause_exact_e_and_lazy_control",
        |request| {
            let source = r#"
            import std::result::{Result, Err as WrapErr};
            import std::error::{Error, display, error};
            type Inner = { code: u32; };
            implement display(problem: &Inner): string { return "inner"; }
            function wrap(input: Result<bool, Inner>): Result<i32> {
                let flag = input /* original operator */ ? error("outer");
                return Result.Ok(if (flag) 1i32 else 0i32);
            }
            function stop(): never { return stop(); }
            function neverOuter(input: Result<bool, Inner>): Result<i32> {
                let flag = input ? stop();
                return Result.Ok(if (flag) 1i32 else 0i32);
            }
            function two(inner: WrapErr<Inner>): Result<bool, string> {
                return WrapErr<bool, string, Inner>("outer", inner);
            }
            function inject(inner: WrapErr<Inner>): Result<bool, Inner> {
                return Result.Err(inner);
            }
        "#;
            fs::write(&request.entry, source).unwrap();
            for test_mode in [false, true] {
                use compiler::analysis::scalars::{data::DataExpression, generics::GoalState};
                use compiler::analysis::typed::ErrorOperation;
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                assert!(
                    report.bodies().findings().is_empty(),
                    "{:?}",
                    report
                        .bodies()
                        .findings()
                        .iter()
                        .map(|f| (f.kind, f.site.span.bytes()))
                        .collect::<Vec<_>>()
                );
                let role = report.result_role().unwrap();
                let representation = report.error_representation().unwrap();
                assert_eq!(representation.wrapper.id, role.wrapper.id);
                assert_eq!(representation.cause.id, role.cause.id);
                assert_eq!(
                    role.display_protection.unwrap().family.id,
                    role.display.unwrap().id
                );
                let app = graph
                    .modules()
                    .iter()
                    .find(|m| m.key.origin == Origin::Application)
                    .unwrap();
                let inner_display = graph
                    .implementations()
                    .iter()
                    .find(|i| i.declaration.module == app.key)
                    .unwrap();
                let mut wrapping = Vec::new();
                let mut explicit = None;
                let mut raw = None;
                for (node, syntax) in app.syntax.nodes.iter().enumerate() {
                    let Some(facts) = report.bodies().node(app, node).unwrap() else {
                        continue;
                    };
                    let NodeFactKind::Expression(value) = &facts.kind else {
                        continue;
                    };
                    match &value.data {
                        Some(DataExpression::Wrapping(w)) => wrapping.push((facts, value, w)),
                        Some(DataExpression::ErrorConstruction(c)) => explicit = Some(c),
                        Some(DataExpression::Construction(c))
                            if syntax.span.bytes() == b"Result.Err(inner)" =>
                        {
                            raw = Some(c)
                        }
                        _ => (),
                    }
                }
                assert_eq!(wrapping.len(), 2, "wrapping facts");
                for (facts, value, w) in wrapping {
                    assert!(
                        matches!(value.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
                    );
                    assert!(facts.flow.normal);
                    assert!(
                        !report
                            .result_use()
                            .unwrap()
                            .pending()
                            .iter()
                            .any(|p| p.site.module == &app.key && p.site.node == facts.site.node)
                    );
                    assert_eq!(w.branch.failure, &role.err.id);
                    assert_eq!(w.construction.operation, ErrorOperation::Wrap);
                    assert_eq!(w.construction.origin.bytes(), b"?");
                    assert_eq!(w.construction.origin.snapshot(), &app.snapshot);
                    assert!(
                        matches!(w.construction.payload.as_ref(), TypeTerm::Nominal { declaration, .. } if *declaration == &role.default_error.id)
                    );
                    let outer = report.bodies().goal(&w.construction.display).unwrap();
                    assert_eq!(outer.state, GoalState::Proved);
                    assert_ne!(
                        outer.selection.as_ref().unwrap().implementation,
                        &inner_display.declaration
                    );
                    let cause = w.construction.retained.as_ref().unwrap();
                    assert!(
                        matches!(cause.wrapper.as_ref(), TypeTerm::Nominal { declaration, arguments } if *declaration == &role.wrapper.id && matches!(&arguments[..], [c] if matches!(c.as_ref(), TypeTerm::Nominal { declaration, .. } if declaration.module == app.key)))
                    );
                    let selected = report.bodies().goal(&cause.display).unwrap();
                    assert_eq!(
                        selected.selection.as_ref().unwrap().implementation,
                        &inner_display.declaration
                    );
                    for goal in [&cause.no_borrow, &cause.sendable, &cause.shareable] {
                        assert_eq!(report.bodies().goal(goal).unwrap().state, GoalState::Proved);
                    }
                    if facts.site.span.bytes().ends_with(b"stop()") {
                        assert!(facts.flow.diverges && !facts.flow.returns);
                    } else {
                        assert!(facts.flow.returns && !facts.flow.diverges);
                    }
                }
                let explicit = explicit.expect("checked two-value Err");
                assert_eq!(explicit.operation, ErrorOperation::Retain);
                assert_eq!(
                    explicit.origin.start(),
                    source.find("WrapErr<bool, string, Inner>(").unwrap()
                );
                assert!(explicit.retained.is_some());
                assert!(
                    raw.is_some(),
                    "raw injection is an ordinary construction without a new origin"
                );
            }
        },
    );
}

#[test]
fn installed_closed_error_keeps_independent_rejections() {
    installed(
        "installed_closed_error_keeps_independent_rejections",
        |request| {
            fs::write(
                &request.entry,
                r#"
            import std::result::{Result, Err as Build};
            import std::option::Option;
            import std::error::display;
            type NoDisplay = {};
            type Borrowed = { text: &string; };
            implement display(e: &Borrowed): string { return "borrowed"; }
            function missingT(): void { let _ = Build("without T"); }
            function missingWidth(): void { let _ = Build<bool>(471); }
            function mismatchDefault(): Result<bool> { return Build("not Error"); }
            function wrongCenvelope(inner: Build<string>): Result<bool, string> {
                return Build<bool, string, i32>("new", inner);
            }
            function borrowedEnvelope(inner: &Build<string>): Result<bool, string> {
                return Build("new", inner);
            }
            function borrowedPayload(inner: Build<Borrowed>): Result<bool, string> {
                return Build("new", inner);
            }
            function noOuterDisplay(input: Result<bool, string>): Result<bool, NoDisplay> {
                let bit = input ? NoDisplay {};
                return Result.Ok(bit);
            }
            function wrongTarget(input: Result<bool, string>): Option<bool> {
                let _ = input ? "outer";
                return Option.None;
            }
            function borrowedLeft(input: &Result<bool, string>): Result<bool, string> {
                let bit = input ? "borrowed result";
                return Result.Ok(bit);
            }
            function optionLeft(input: Option<bool>): Result<bool, string> {
                let bit = input ? "not a Result";
                return Result.Ok(bit);
            }
        "#,
            )
            .unwrap();
            for test_mode in [false, true] {
                use compiler::analysis::scalars::ScalarFindingKind::*;
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                let findings = report
                    .bodies()
                    .findings()
                    .iter()
                    .filter(|f| f.site.module.origin == Origin::Application)
                    .map(|f| (f.kind, std::str::from_utf8(f.site.span.bytes()).unwrap()))
                    .collect::<Vec<_>>();
                for expected in [
                    (InsufficientContext, r#"Build("without T")"#),
                    (NumericContext, "471"),
                    (TypeMismatch, r#""not Error""#),
                    (TypeMismatch, "inner"),
                    (InvalidTarget, "inner"),
                    (MissingRequirement, "inner"),
                    (MissingRequirement, "input ? NoDisplay {}"),
                    (InvalidTarget, r#"input ? "outer""#),
                    (InvalidTarget, r#"input ? "borrowed result""#),
                    (InvalidTarget, r#"input ? "not a Result""#),
                ] {
                    assert!(
                        findings.contains(&expected),
                        "missing {expected:?}; got {findings:?}"
                    );
                }
                let app = graph
                    .modules()
                    .iter()
                    .find(|m| m.key.origin == Origin::Application)
                    .unwrap();
                for (node, syntax) in app.syntax.nodes.iter().enumerate() {
                    if syntax.kind == NodeKind::Wrap
                        || syntax.kind == NodeKind::Call
                            && syntax.span.bytes().starts_with(b"Build")
                    {
                        assert!(report.bodies().node(app, node).unwrap().is_none());
                    }
                }
            }
        },
    );
}

#[test]
fn installed_closed_err_does_not_dispatch_a_written_registration() {
    installed(
        "installed_closed_err_does_not_dispatch_a_written_registration",
        |request| {
            fs::write(
                &request.entry,
                r#"
            import std::result::{Result, Err as Build};
            import std::error::display;
            type Local = {};
            implement display(e: &Local): string { return "local"; }
            implement<T> Build<T, Local>(e: Local): Result<T, Local> {
                return Build(e);
            }
            function attempt(): Result<bool, Local> { return Build(Local {}); }
        "#,
            )
            .unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                let role = report.result_role().unwrap();
                assert!(report.error_representation().is_some());
                assert!(!role.constructor_protected);
                assert_eq!(
                    report.error_providers().unwrap().constructor.id,
                    role.constructor.unwrap().id
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
                    .position(|n| n.kind == NodeKind::Call && n.span.bytes() == b"Build(Local {})")
                    .unwrap();
                assert!(report.bodies().node(app, node).unwrap().is_none());
                assert!(
                    report
                        .bodies()
                        .obligations()
                        .iter()
                        .any(|o| o.site.module == &app.key
                            && o.site.node == node
                            && o.reason
                                == compiler::analysis::scalars::ScalarUnsupported::CoreProtection)
                );
            }
        },
    );
}

#[test]
fn installed_err_does_not_retry_display_after_failed_winner_requirement() {
    installed(
        "installed_err_does_not_retry_display_after_failed_winner_requirement",
        |request| {
            fs::write(
                &request.entry,
                r#"
            import std::result::{Result, Err as Build};
            import std::error::display;
            type Local<T> = { payload: T; };
            implement<T> display(e: &Local<T>): string { return "fallback"; }
            implement display(e: &Local<bool>): string where noBorrow(&string) { return "winner"; }
            function attempt(e: Local<bool>): Result<bool, Local<bool>> { return Build(e); }
        "#,
            )
            .unwrap();
            for test_mode in [false, true] {
                use compiler::analysis::scalars::{ScalarFindingKind, generics::GoalState};
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
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
                    .position(|n| n.kind == NodeKind::Call && n.span.bytes() == b"Build(e)")
                    .unwrap();
                let winner = graph
                    .implementations()
                    .iter()
                    .filter(|i| i.declaration.module == app.key)
                    .find(|i| {
                        app.syntax.nodes[i.signature_node]
                            .span
                            .bytes()
                            .windows(11)
                            .any(|part| part == b"Local<bool>")
                    })
                    .unwrap();
                let goal = report
                    .bodies()
                    .goals()
                    .iter()
                    .find(|g| {
                        g.site.module == &app.key && g.site.node == node && g.selection.is_some()
                    })
                    .expect("selected display");
                assert_eq!(
                    goal.selection.as_ref().unwrap().implementation,
                    &winner.declaration
                );
                assert_eq!(goal.state, GoalState::Disproved);
                assert!(goal.premise.is_none());
                assert!(report.bodies().node(app, node).unwrap().is_none());
                assert!(
                    report
                        .bodies()
                        .findings()
                        .iter()
                        .any(|f| f.site.module == &app.key
                            && f.site.node == node
                            && f.kind == ScalarFindingKind::MissingRequirement)
                );
            }
        },
    );
}

#[test]
fn installed_into_value_acknowledges_cause_but_not_the_returned_payload() {
    installed(
        "installed_into_value_acknowledges_cause_but_not_the_returned_payload",
        |request| {
            fs::write(
                &request.entry,
                "
                import std::error::intoValue;
                import std::result::{Err, Result};
                function extract(input: Err<Result<bool>>): void { intoValue(input); }
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
                let result = graph
                    .modules()
                    .iter()
                    .find(|module| module.key.path == "result.rill")
                    .unwrap();
                let extract = result
                    .declarations
                    .iter()
                    .find(|declaration| declaration.name.as_deref() == Some("intoValue"))
                    .unwrap();
                assert!(
                    report
                        .bodies()
                        .body(result, BodyOwner::Declaration(&extract.id))
                        .unwrap()
                        .is_some(),
                    "{:?}\n{:?}",
                    report.bodies().findings(),
                    report.bodies().obligations()
                );
                let uses = report.result_use().unwrap();
                let end = result.syntax.nodes[extract.node].span.end();
                let in_extract = |site: Site<'_>| {
                    site.module == &result.key
                        && (extract.id.start..end).contains(&site.span.start())
                };
                if let Some(pending) = uses.pending().iter().find(|p| in_extract(p.site)) {
                    panic!(
                        "unexpected {:?} in intoValue at {}",
                        pending.blocker,
                        String::from_utf8_lossy(pending.site.span.bytes())
                    );
                }
                assert!(!uses.discards().iter().any(|w| in_extract(w.discard)));
                assert!(uses.discards().iter().any(|witness| {
                    witness.origin.module.origin == Origin::Application
                        && witness.origin.span.bytes() == b"intoValue(input)"
                        && witness.code() == "R0103"
                }));
            }
        },
    );
}

#[test]
fn installed_combinator_roots_prove_conditional_result_use_without_callers() {
    installed(
        "installed_combinator_roots_prove_conditional_result_use_without_callers",
        |request| {
            fs::write(&request.entry, "import std::result::Result;").unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                let uses = report.result_use().unwrap();
                for (path, count) in [("option.rill", 2), ("result.rill", 3)] {
                    let module = graph.modules().iter().find(|m| m.key.path == path).unwrap();
                    let mut checked = 0;
                    for helper in module.declarations.iter().filter(|d| {
                        d.id.kind == loader::DeclarationKind::Function
                            && matches!(d.name.as_deref(), Some("map" | "mapErr" | "andThen"))
                    }) {
                        checked += 1;
                        assert!(
                            report
                                .bodies()
                                .body(module, BodyOwner::Declaration(&helper.id))
                                .unwrap()
                                .is_some(),
                            "{} was not checked",
                            helper.name.as_deref().unwrap()
                        );
                        let span = &module.syntax.nodes[helper.node].span;
                        let pending = uses.pending().iter().find(|p| {
                            p.site.module == &module.key
                                && (span.start()..span.end()).contains(&p.site.span.start())
                        });
                        if let Some(other) = pending {
                            panic!(
                                "unexpected {:?} at {}",
                                other.blocker,
                                String::from_utf8_lossy(other.site.span.bytes())
                            );
                        }
                    }
                    assert_eq!(checked, count);
                }
            }
        },
    );
}

#[test]
fn installed_combinator_callback_uses_check_concrete_source_and_skip_erasure() {
    use compiler::analysis::typed::ResultUseBlocker;

    installed(
        "installed_combinator_callback_uses_check_concrete_source_and_skip_erasure",
        |request| {
            fs::write(&request.entry, r#"
                import std::option::{Option, map as optionMap, andThen as optionThen};
                import std::result::{Result, Ok, Err, map as resultMap, andThen as resultThen, mapErr};

                function identity<T>(value: T): T { return value; }
                function some(value: i32): Option<i32> { return Option.Some(value); }
                function proven(): void {
                    let nested: Option<Result<i32, i32>> = Option.Some(Result.Ok(7i32));
                    let _ = optionMap(nested, identity<Result<i32, i32>>);
                    let _ = optionThen(Option.Some(7i32), some);
                    let _ = resultMap(Ok<i32, i32>(7i32), (x: i32) => x);
                    let _ = resultThen(Ok<i32, i32>(7i32), (x: i32) => Ok<i32, i32>(x));
                    let _ = mapErr(Err<i32, i32>(7i32), (x: i32) => x);
                }
                function erased(callback: once (i32) => i32): void {
                    let none: Option<i32> = Option.None;
                    let _ = optionMap(none, callback);
                    let ok: Result<i32, i32> = Result.Ok(7i32);
                    let _ = mapErr(ok, callback);
                    let _ = resultMap(Err<i32, i32>(7i32), callback);
                }
                function invoked(callback: once (i32) => i32): void {
                    let _ = optionMap(Option.Some(7i32), callback);
                }
                function maybe(value: Option<i32>, callback: once (i32) => i32): void {
                    let _ = optionMap(value, callback);
                }
            "#).unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(
                    &graph,
                    AnalysisLimits {
                        max_entries: 200_000,
                        ..AnalysisLimits::default()
                    },
                );
                assert_ne!(
                    report.completion(),
                    TypedCompletion::ResourceLimit,
                    "{:?}",
                    report.statistics()
                );
                assert!(
                    report.bodies().findings().is_empty(),
                    "{:?}",
                    report.bodies().findings()
                );
                assert!(
                    report.bodies().obligations().is_empty(),
                    "{:?}",
                    report.bodies().obligations()
                );
                let uses = report.result_use().unwrap();
                let pending: Vec<_> = uses
                    .pending()
                    .iter()
                    .map(|p| (p.site.module.path.as_str(), p.site.span.bytes(), p.blocker))
                    .collect();
                assert_eq!(
                    pending,
                    [
                        (
                            "main.rill",
                            &b"optionMap(Option.Some(7i32), callback)"[..],
                            ResultUseBlocker::CallBoundary
                        ),
                        (
                            "main.rill",
                            &b"optionMap(value, callback)"[..],
                            ResultUseBlocker::CallBoundary
                        ),
                    ]
                );
                assert!(uses.discards().is_empty());
            }
        },
    );
}

#[test]
fn installed_callback_captures_keep_their_witnesses_and_unchecked_uses() {
    use compiler::analysis::typed::ResultUseBlocker;

    installed(
        "installed_callback_captures_keep_their_witnesses_and_unchecked_uses",
        |request| {
            fs::write(
                &request.entry,
                r#"
                import std::option::{Option, map};
                import std::result::{Result, Ok};

                function forwarded(): void {
                    let kept = Ok<i32, i32>(11i32);
                    let callback = (value: i32) => kept;
                    let _ = map(Option.Some(1i32), callback);
                }
                function notForwarded(take: bool): void {
                    let lost: Result<i32, i32> = Result.Ok(22i32);
                    let callback = (value: i32) => {
                        if (take) { return lost; }
                        return Ok<i32, i32>(value);
                    };
                    let _ = map(Option.Some(2i32), callback);
                }
                function skipped(take: bool): void {
                    let skipped = Ok<i32, i32>(33i32);
                    let callback = (value: i32) => {
                        if (take) { return skipped; }
                        return Ok<i32, i32>(value);
                    };
                    let none: Option<i32> = Option.None;
                    let _ = map(none, callback);
                }
                function unchecked(opaque: once (Result<i32, i32>) => i32): void {
                    let unknown = Ok<i32, i32>(44i32);
                    let _ = map(Option.Some(4i32), (value: i32) => opaque(unknown));
                    let sibling: Result<i32, i32> = Result.Ok(55i32);
                }
            "#,
            )
            .unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                let uses = report.result_use().unwrap();
                let discards: Vec<_> = uses
                    .discards()
                    .iter()
                    .map(|w| {
                        (
                            w.origin.module.path.as_str(),
                            w.origin.span.bytes(),
                            w.storage.is_some(),
                        )
                    })
                    .collect();
                assert_eq!(
                    discards,
                    [
                        ("main.rill", &b"Result.Ok(22i32)"[..], true),
                        ("main.rill", &b"Result.Ok(55i32)"[..], true),
                    ]
                );
                let pending: Vec<_> = uses
                    .pending()
                    .iter()
                    .map(|p| (p.site.module.path.as_str(), p.site.span.bytes(), p.blocker))
                    .collect();
                assert_eq!(
                    pending,
                    [
                        (
                            "main.rill",
                            &b"opaque(unknown)"[..],
                            ResultUseBlocker::CallBoundary
                        ),
                        (
                            "main.rill",
                            &b"map(Option.Some(4i32), (value: i32) => opaque(unknown))"[..],
                            ResultUseBlocker::CallBoundary
                        ),
                    ]
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/error/src/lib.rs"),
        runtime.join("error.rs"),
    )
    .unwrap();
    let executable = root.join("bin/probe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
    for module in [
        "core.rill",
        "option.rill",
        "result.rill",
        "error.rill",
        "format.rill",
        "collection.rill",
        "iter.rill",
    ] {
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
