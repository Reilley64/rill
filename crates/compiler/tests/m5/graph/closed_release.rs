use super::*;
use compiler::analysis::ownership::lower_root;

const TEXT: &str = r#"export function ready(): bool { return true; }
function consumes(value: string): bool { dispose(value); return true; }
expect consumes("expectation");
let startup = "startup";
dispose(startup);
"#;

fn imported_roots<'a, 'g>(report: &'a GraphReport<'g>) -> [&'a RootReport<'g>; 3] {
    let startup = report
        .roots()
        .iter()
        .find(|root| {
            root.site().module.origin == Origin::Application
                && root.site().module.path == "helper.rill"
                && root.owner() == BodyOwner::Startup
        })
        .unwrap();
    [
        startup,
        root(report, b"function consumes"),
        root(report, b"expect consumes"),
    ]
}

#[test]
fn duplicate_disposal_keeps_the_first_release_as_related_evidence() {
    let startup = format!("{TEXT}dispose(startup);\n");
    let function = TEXT.replace("dispose(value);", "dispose(value); dispose(value);");
    let sources = [startup.as_str(), function.as_str()];
    fixture(
        "graph::closed_release::duplicate_disposal_keeps_the_first_release_as_related_evidence",
        &sources,
        |case, _, graph, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.completion(), GraphCompletion::SourceRejected);
            let [finding] = report.findings() else {
                panic!("{:?}", report.findings())
            };
            assert_eq!(finding.code(), "R0200");
            let written = ["dispose(startup)", "dispose(value)"][case];
            let first = sources[case].find(written).unwrap();
            let second = sources[case].rfind(written).unwrap();
            for (site, start) in [(finding.site, second), (finding.related, first)] {
                assert_eq!(
                    (site.module.origin, site.module.path.as_str()),
                    (Origin::Application, "helper.rill")
                );
                assert_eq!(
                    site.span.start()..site.span.end(),
                    start..start + written.len()
                );
            }
            let callers = imported_roots(&report);
            for (index, caller) in callers.into_iter().enumerate() {
                let rejected = (index == 0) == (case == 0);
                assert_eq!(
                    caller.ordinary_completion(),
                    if rejected {
                        Completion::SourceRejected
                    } else {
                        Completion::Checked
                    }
                );
                assert_eq!(caller.ordinary_mir().is_none(), rejected);
            }
            let helper = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "helper.rill")
                .unwrap();
            let targeted = lower_root(
                receipt,
                helper,
                callers[case].owner(),
                AnalysisLimits::default(),
            )
            .unwrap();
            assert_eq!(targeted.completion(), Completion::SourceRejected);
            assert!(targeted.mir().is_none());
            let [direct] = targeted.findings() else {
                panic!("expected targeted duplicate disposal")
            };
            assert_eq!(direct.code(), finding.code());
            for (direct, graph) in [
                (direct.site, finding.site),
                (direct.related, finding.related),
            ] {
                assert_eq!((direct.module, direct.node), (graph.module, graph.node));
            }
            assert!(
                root(&report, b"function entryControl")
                    .ordinary_mir()
                    .is_some()
            );
        },
    );
}

#[test]
fn wrong_text_wrapper_or_missing_guard_archive_cannot_publish_mir() {
    use compiler::analysis::typed::{NativeOwnershipAction, OwnershipRule};

    installed(
        "graph::closed_release::wrong_text_wrapper_or_missing_guard_archive_cannot_publish_mir",
        |request| {
            let prefix = request.entry.parent().unwrap().parent().unwrap();
            let core = prefix.join("lib/rill/std/core.rill");
            let archive =
                prefix.join("lib/rill/runtime/aarch64-apple-darwin/librill_ownership.rlib");
            let saved = archive.with_extension("saved");
            let original = fs::read_to_string(&core).unwrap();
            let wrong = original.replace("{ disposeText(value); }", "{}");
            assert_ne!(original, wrong);
            fs::write(
                &request.entry,
                format!(
                    "{MAIN}\nfunction releases(lock: LockGuard<i32>): void {{ dispose(lock); }}\n"
                ),
            )
            .unwrap();
            fs::write(request.entry.with_file_name("helper.rill"), TEXT).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                fs::write(&core, &wrong).unwrap();
                let failure = loader::load_installed(request.clone()).unwrap_err();
                assert_eq!(failure.kind, loader::FailureKind::InvalidRequest);
                assert_eq!(
                    failure.message,
                    "installed source differs from toolchain pin"
                );
                fs::write(&core, &original).unwrap();

                fs::rename(&archive, &saved).unwrap();
                let graph = loader::load_installed(request.clone()).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                assert!(typed.receipt().is_none());
                let guard = typed
                    .ownership_protection()
                    .iter()
                    .find(|scope| {
                        scope.rule == OwnershipRule::Required(NativeOwnershipAction::GuardRelease)
                    })
                    .unwrap();
                assert!(guard.provider.is_none());
                fs::rename(&saved, &archive).unwrap();
            }
        },
    );
}

fn standard_root<'a, 'g>(
    report: &'a GraphReport<'g>,
    module: &str,
    prefix: &[u8],
) -> &'a RootReport<'g> {
    report
        .roots()
        .iter()
        .find(|root| {
            root.site().module.origin == Origin::Standard
                && root.site().module.path == module
                && root.site().span.bytes().starts_with(prefix)
                && matches!(root.owner(), BodyOwner::Declaration(_))
        })
        .unwrap()
}

#[test]
fn imported_text_disposal_closes_only_the_ordinary_caller_edges() {
    fixture(
        "graph::closed_release::imported_text_disposal_closes_only_the_ordinary_caller_edges",
        &[TEXT],
        |_, _, graph, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let helper = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "helper.rill")
                .unwrap();
            let [startup, function, expectation] = imported_roots(&report);
            for caller in [startup, function, expectation] {
                let targeted =
                    lower_root(receipt, helper, caller.owner(), AnalysisLimits::default()).unwrap();
                assert_eq!(targeted.completion(), Completion::Checked);
                assert_eq!(
                    caller.ordinary_completion(),
                    Completion::Checked,
                    "{:?}",
                    caller.blocked_at()
                );
                assert!(targeted.mir().is_some());
                assert!(caller.ordinary_mir().is_some());
                assert!(matches!(caller.enabled_frame(), Delivery::Unresolved));
            }
            for (caller, name) in [(startup, b"startup".as_slice()), (function, b"value")] {
                assert!(caller.ordinary_mir().unwrap().blocks.iter().any(|block| matches!(
                block.operation, Operation::Dispose { operand, .. }
                    if operand.root.span.bytes().starts_with(name) && operand.projection.is_none()
            )));
                let [edge] = caller.dependencies() else {
                    panic!("{:?}", caller.dependencies())
                };
                assert_eq!(edge.contract, DependencyContract::BuiltinDispose);
                assert_eq!(edge.declaration.module.origin, Origin::Standard);
                assert_eq!(edge.declaration.module.path, "core.rill");
            }
            let provider = standard_root(&report, "core.rill", b"implement dispose(value: string)");
            assert_eq!(provider.ordinary_completion(), Completion::Incomplete);
            assert!(provider.ordinary_mir().is_none());
            assert!(report.active_disposers().is_empty());
            assert!(report.findings().is_empty());
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
        },
    );
}

#[test]
fn implicit_guard_does_not_pay_for_written_call_verification() {
    probe(
        "graph::closed_release::implicit_guard_does_not_pay_for_written_call_verification",
        "function implicit(lock: LockGuard<i32>): void {}",
        |_, _, receipt| {
            // Exact graph cap before the written release verifier, in both installed modes.
            let limits = AnalysisLimits {
                max_work: 185222,
                max_entries: 3993,
                max_depth: 3,
            };
            let report = analyze_ownership(receipt, limits);
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
            let implicit = root(&report, b"function implicit");
            assert_eq!(implicit.ordinary_completion(), Completion::Incomplete);
            assert!(implicit.ordinary_mir().is_none());
            assert!(
                matches!(implicit.dependencies(), [edge] if matches!(edge.contract, DependencyContract::ProtectedRelease { .. }))
            );
        },
    );
}

#[test]
fn written_guard_release_closes_only_the_ordinary_caller_edge() {
    let source = format!(
        "{MAIN}
function releases(lock: LockGuard<i32>): void {{ dispose(lock); }}
function implicit(lock: LockGuard<i32>): void {{}}
function unsupported(lock: LockGuard<i64>): void {{ dispose(lock); }}
type Parcel = {{ text: string; }}
function projected(parcel: Parcel): void {{ dispose(parcel.text); }}
"
    );
    fixture_with_main(
        "graph::closed_release::written_guard_release_closes_only_the_ordinary_caller_edge",
        &source,
        &[HELPER],
        |_, _, graph, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let main = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let caller = root(&report, b"function releases");
            let targeted =
                lower_root(receipt, main, caller.owner(), AnalysisLimits::default()).unwrap();
            assert_eq!(targeted.completion(), Completion::Checked);
            assert!(targeted.mir().is_some());
            assert_eq!(caller.ordinary_completion(), Completion::Checked);
            assert!(caller.ordinary_mir().is_some());
            assert!(matches!(caller.enabled_frame(), Delivery::Unresolved));
            let [edge] = caller.dependencies() else {
                panic!("{:?}", caller.dependencies())
            };
            let provider = standard_root(&report, "mutex.rill", b"implement<T> dispose");
            assert_eq!(BodyOwner::Declaration(edge.declaration), provider.owner());
            let DependencyContract::ProtectedRelease { entry } = edge.contract else {
                panic!("{edge:?}")
            };
            assert_eq!(entry.module, edge.declaration.module);
            assert_eq!(entry.kind, loader::DeclarationKind::Family);
            assert!(caller.ordinary_mir().unwrap().blocks.iter().any(|block| matches!(
            &block.operation, Operation::Cleanup(actions)
                if matches!(actions.as_slice(), [Obligation::ReleaseGuard { operand, implementation, entry: selected }]
                    if operand.root.span.bytes() == b"lock: LockGuard<i32>"
                        && *implementation == edge.declaration && *selected == entry)
        )));
            let projected = root(&report, b"function projected");
            let targeted =
                lower_root(receipt, main, projected.owner(), AnalysisLimits::default()).unwrap();
            assert!(targeted.mir().is_some());
            assert!(
                projected
                    .dependencies()
                    .iter()
                    .any(|edge| edge.contract == DependencyContract::BuiltinDispose)
            );
            for pending in [
                provider,
                root(&report, b"function implicit"),
                root(&report, b"function unsupported"),
                projected,
            ] {
                assert_eq!(pending.ordinary_completion(), Completion::Incomplete);
                assert!(pending.ordinary_mir().is_none());
            }
            assert!(report.active_disposers().is_empty());
            assert!(report.findings().is_empty());
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
        },
    );
}
