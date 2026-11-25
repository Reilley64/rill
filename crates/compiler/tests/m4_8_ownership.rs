//! Installed ownership evidence is capture-bound and does not depend on application aliases.

#[path = "support/limit.rs"]
mod limit;

use compiler::analysis::AnalysisLimits;
use compiler::analysis::typed::{
    CoreBlocker, NativeOwnershipAction, OwnershipCoverage, OwnershipRule, TypedCompletion,
    analyze_typed,
};
use compiler::loader::{
    self, FailureKind, InstalledRequest, Limits, Origin, Prelude, Request, Selector,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ARCHIVE: &str = concat!(
    "lib/rill/runtime/",
    env!("RILL_ANALYSIS_TARGET"),
    "/librill_ownership.rlib"
);

#[test]
fn installed_ownership_requires_the_built_archive() {
    if let Some(root) = std::env::var_os("RILL_OWNERSHIP_PROBE_ROOT") {
        let root = PathBuf::from(root);
        let request = InstalledRequest {
            entry: root.join("app/main.rill"),
            target: env!("RILL_ANALYSIS_TARGET").into(),
            options: BTreeMap::new(),
            test_mode: false,
            limits: Limits::default(),
        };
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                test_mode,
                ..request.clone()
            })
            .unwrap();
            let report = analyze_typed(&graph, AnalysisLimits::default());
            let copy = report
                .ownership_protection()
                .iter()
                .find(|p| p.rule == OwnershipRule::Required(NativeOwnershipAction::SharedRetain))
                .expect("protected Shared copy");
            assert_eq!(copy.coverage, OwnershipCoverage::Overlap);
            let provider = copy
                .provider
                .as_ref()
                .expect("actual counted-owner provider");
            assert_eq!(provider.implementation.id.context, graph.id());
            assert_eq!(provider.implementation.id.module.origin, Origin::Standard);
            assert_eq!(provider.entry.name.as_deref(), Some("retainShared"));
            assert_eq!(
                provider.artifact,
                root.join(ARCHIVE).canonicalize().unwrap()
            );
            assert_eq!(
                NativeOwnershipAction::SharedRetain.rust_item(),
                "rill_ownership::shared::retain"
            );
            assert_eq!(report.core_blockers(), []);
            let [overlap] = report.ownership_overlaps() else {
                panic!(
                    "expected only the nested-payload replacement: {:?}",
                    report.ownership_overlaps()
                );
            };
            assert_eq!(overlap.code(), "R0500");
            limit::assert_typed_limits_below(&graph, &report);
            // Ruling 19 (1): the overlap verdict discloses an R0500 finding, so it
            // is `Overlap` at the exact cap and `Pending` one work short.
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
                .ownership_protection()
                .iter()
                .find(|p| p.rule == copy.rule)
                .expect("the Shared copy's protection at one work short");
            assert_eq!(pending.coverage, OwnershipCoverage::Pending);
            assert_eq!(overlap.rule, copy.rule);
            assert_eq!(overlap.implementation.id.module.origin, Origin::Application);
            let app = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application)
                .unwrap();
            let invocation = app
                .syntax
                .nodes
                .iter()
                .position(|n| n.span.bytes() == b"duplicate(value)")
                .unwrap();
            assert!(
                report.bodies().node(app, invocation).unwrap().is_none(),
                "overlap cannot produce a checked call"
            );
            let release = report
                .ownership_protection()
                .iter()
                .find(|p| p.rule == OwnershipRule::Required(NativeOwnershipAction::SharedRelease))
                .unwrap();
            assert_eq!(release.coverage, OwnershipCoverage::Clear);
            // A protected overlap must not erase independent representation evidence.
            let (shared, others): (Vec<_>, Vec<_>) = report
                .native_representations()
                .iter()
                .partition(|r| r.owner.id == copy.owner.id);
            let [storage] = shared[..] else {
                panic!("expected one Shared representation");
            };
            assert!(
                others
                    .iter()
                    .all(|r| r.owner.id.module.origin == Origin::Standard
                        && r.owner.id.module != copy.owner.id.module),
                "the extra representations belong to standard modules the program does not import"
            );
            assert_eq!(storage.owner.id, copy.owner.id);
            assert_eq!(storage.argument.name.as_deref(), Some("T"));
            assert_eq!(storage.artifact, provider.artifact);
            assert_eq!(
                storage.kind,
                compiler::analysis::typed::NativeRepresentationKind::CountedPayload
            );
        }

        guard_and_endpoint_regions(&request);
        missing_evidence(&root, &request);
        partial_collection(&request);
        combined_installed_capture(&request);
        aggregate_classification(&root, &request);
        closed_roles_without_native_providers(&root, &request);
        complete_provider_and_protection_scope(&request);
        installed_receipts(&request);
        installed_ordinary_family(&request);
        installed_pair_evidence(&request);
        installed_pair_invocation(&request);

        let facade = root.join("lib/rill/std/std.rill");
        let bytes = fs::read(&facade).unwrap();
        fs::write(&facade, [&bytes[..], b"\n// changed pin"].concat()).unwrap();
        assert_eq!(
            loader::load_installed(request.clone()).unwrap_err().kind,
            FailureKind::InvalidRequest
        );
        fs::write(facade, bytes).unwrap();
        let graph = loader::load_installed(request.clone()).unwrap();
        let archive = fs::read(root.join(ARCHIVE)).unwrap();
        fs::write(root.join(ARCHIVE), b"substituted archive").unwrap();
        // Static evidence stays with the original captured inputs.
        assert!(
            analyze_typed(&graph, AnalysisLimits::default())
                .ownership_protection()
                .iter()
                .any(|p| p.provider.is_some())
        );
        assert_eq!(
            loader::validate_inputs(&graph, Limits::default())
                .unwrap_err()
                .kind,
            FailureKind::SourceChanged
        );
        assert_eq!(
            loader::load_installed(request.clone()).unwrap_err().kind,
            FailureKind::InvalidRequest
        );
        fs::write(
            &request.entry,
            "
            type Shared<T> = { payload: T; }
            implement<T> dispose(value: Shared<T>): void { }
        ",
        )
        .unwrap();
        assert_eq!(
            loader::load_installed(request.clone()).unwrap_err().kind,
            FailureKind::InvalidRequest
        );
        fs::write(root.join(ARCHIVE), archive).unwrap();
        let graph = loader::load_installed(request).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert!(
            report
                .ownership_protection()
                .iter()
                .all(|p| p.owner.id.module.origin == Origin::Standard),
            "an unrelated same-spelled source gets no ownership protection"
        );
        return;
    }

    let root = std::env::temp_dir().join(format!("rill-m48-ownership-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _cleanup = Cleanup(&root);
    for path in [
        "bin",
        "lib/rill/std",
        "app",
        concat!("lib/rill/runtime/", env!("RILL_ANALYSIS_TARGET")),
    ] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    let executable = root.join("bin/m48-ownership-probe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
    for module in include_str!("../../../tests/installed/typed/stdlib-sources.txt")
        .lines()
        .chain(include_str!("../../../tests/installed/typed/routine-sources.txt").lines())
    {
        fs::copy(source.join(module), root.join("lib/rill/std").join(module)).unwrap();
    }
    fs::copy(env!("RILL_OWNERSHIP_ARCHIVE"), root.join(ARCHIVE)).unwrap();
    fs::copy(
        source.join("../runtime/error/src/lib.rs"),
        root.join(ARCHIVE).with_file_name("error.rs"),
    )
    .unwrap();
    fs::write(
        root.join("app/main.rill"),
        "
        import std::shared::Shared as Counted;
        import std::core::copy as duplicate;
        type Payload<T> = { part: T; }
        implement<T> duplicate(value: &Counted<Payload<T>>): Counted<Payload<T>> { }
        implement dispose(value: Payload<bool>): void { }
        function blocked(value: &Counted<Payload<bool>>): Counted<Payload<bool>> { return duplicate(value); }
    ",
    )
    .unwrap();
    let output = Command::new(executable)
        .args([
            "--exact",
            "installed_ownership_requires_the_built_archive",
            "--nocapture",
        ])
        .env("RILL_OWNERSHIP_PROBE_ROOT", &root)
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

fn aggregate_classification(root: &Path, request: &InstalledRequest) {
    use compiler::analysis::scalars::generics::{GoalJudgment, GoalState};
    use compiler::analysis::scalars::{NodeFactKind, ScalarUnsupported};
    use compiler::analysis::{InvocationMode, TypeTerm};

    let mut source = String::from(
        "
        import std::core::{dispose as release, copy as duplicate};
        import std::mutex::LockGuard as Guard;
        import std::bytes::Bytes;
        import std::error::Cause;
        import std::collection::List;
        type Plain = { member: bool; }
        type Text = { member: string; }
        implement duplicate(value: &Text): Text { while (true) {} }
        type Special<T> = { member: bool; }
        implement release(value: Special<bool>): void {}
        type Wrap<T> = { member: T; }
        type Conditional<T> = { member: bool; }
        implement<T> release(value: Conditional<T>): void where noBorrow(T) {}
        type Fallback<T> = { member: bool; }
        implement<T> release(value: Fallback<T>): void {}
        implement release(value: Fallback<&bool>): void where noBorrow(&bool) {}
        type Pair<T, U> = { member: bool; }
        implement<T> release(value: Pair<bool, T>): void {}
        implement<T> release(value: Pair<T, bool>): void {}
        enum Choice<T> { Empty, Value(T), Released }
        implement release(value: Choice<bool>.Released): void {}
        implement release(value: [Plain, bool]): void {}
        // This overlap blocks the required release. The independent copy
        // prohibition must still classify Guard.
        implement release(value: Guard<Plain>): void {}
        ",
    );
    // Some(true) copies; Some(false) borrows / consumes; None stays pending.
    let cases = [
        ("Plain", "Plain", "", Some(true), None),
        ("Tuple", "[bool, Plain]", "", Some(true), None),
        ("TupleDispose", "[Plain, bool]", "", Some(false), None),
        ("Nested", "Wrap<Special<i32>>", "", Some(true), None),
        (
            "NestedDispose",
            "Wrap<Special<bool>>",
            "",
            Some(false),
            None,
        ),
        ("Nonuniform", "Special<T>", "<T>", None, None),
        ("Where", "Conditional<T>", "<T>", Some(false), None),
        ("MissingWhere", "Conditional<T>", "<T>", None, None),
        ("FailedWinner", "Fallback<&bool>", "", None, None),
        ("Ambiguous", "Pair<bool, bool>", "", None, None),
        ("Variant", "Choice<string>.Empty", "", Some(true), None),
        (
            "VariantDispose",
            "Choice<bool>.Released",
            "",
            Some(false),
            None,
        ),
        ("Enum", "Choice<string>", "", Some(false), Some(false)),
        ("Descriptor", "Wrap<&Guard<Plain>>", "", Some(true), None),
        ("Exclusive", "Wrap<&mut bool>", "", Some(false), Some(false)),
        ("Protected", "Guard<Plain>", "", Some(false), None),
        ("Bytes", "Bytes", "", Some(false), None),
        ("Cause", "Cause", "", Some(false), None),
        ("List", "List<bool>", "", Some(false), None),
        ("ExplicitCopy", "Text", "", Some(false), Some(false)),
        ("Rigid", "T", "<T>", Some(false), Some(false)),
    ];
    for (name, ty, generics, _, _) in cases {
        let requirements = if name == "Where" {
            " where noBorrow(T)"
        } else {
            ""
        };
        source.push_str(&format!(
            "\nfunction bind{name}{generics}(input: &[{ty}]): void{requirements} {{ let [bound{name}] = input; }}
            function capture{name}{generics}(captured{name}: {ty}): void{requirements} {{ let arrow = () => captured{name}; }}\n"
        ));
    }
    fs::write(&request.entry, source).unwrap();
    for test_mode in [false, true] {
        let installed = loader::load_installed(InstalledRequest {
            test_mode,
            ..request.clone()
        })
        .unwrap();
        let source_only = loader::load(installed.request().clone()).unwrap();
        let archive = root.join(ARCHIVE);
        let saved = archive.with_extension("saved");
        fs::rename(&archive, &saved).unwrap();
        let storage = archive.with_file_name("error.rs");
        let saved_storage = storage.with_extension("saved");
        fs::rename(&storage, &saved_storage).unwrap();
        let no_artifacts = loader::load_installed(InstalledRequest {
            test_mode,
            ..request.clone()
        });
        fs::rename(saved, archive).unwrap();
        fs::rename(saved_storage, storage).unwrap();
        for (graph, admitted, artifacts) in [
            (&installed, true, true),
            (&source_only, false, false),
            (&no_artifacts.unwrap(), true, false),
        ] {
            let report = analyze_typed(graph, AnalysisLimits::default());
            let dispose = report.dispose_role();
            assert_eq!(dispose.is_some(), admitted);
            let app = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application)
                .unwrap();
            if let Some(dispose) = dispose {
                assert_eq!(dispose.id, graph.prelude().dispose);
                assert_eq!(dispose.id.context, graph.id());
                assert_eq!(report.error_representation().is_some(), artifacts);
                let guard: Vec<_> = report
                    .ownership_protection()
                    .iter()
                    .filter(|scope| scope.owner.name.as_deref() == Some("LockGuard"))
                    .collect();
                let [release, copy] = guard.as_slice() else {
                    panic!("missing Guard scopes: {guard:?}")
                };
                assert_eq!(copy.owner.id.context, graph.id());
                assert_eq!(copy.owner.id.module.path, "mutex.rill");
                assert_eq!(release.owner.id, copy.owner.id);
                assert_eq!(release.family, &dispose.id);
                assert_eq!(copy.family, &report.copy_role().unwrap().id);
                assert_eq!(copy.family, &graph.prelude().copy);
                assert_eq!(copy.rule, OwnershipRule::ProhibitedCopy);
                assert!(copy.provider.is_none());
                assert_eq!(
                    release.rule,
                    OwnershipRule::Required(NativeOwnershipAction::GuardRelease)
                );
                assert_eq!(release.provider.is_some(), artifacts);
                let coverage = if artifacts {
                    let [overlap] = report.ownership_overlaps() else {
                        panic!("expected only the Guard release overlap")
                    };
                    assert_eq!(overlap.owner.id, copy.owner.id);
                    assert_eq!(overlap.rule, release.rule);
                    assert_eq!(overlap.implementation.id.module, app.key);
                    limit::assert_typed_limits_below(graph, &report);
                    (OwnershipCoverage::Overlap, OwnershipCoverage::Clear)
                } else {
                    (OwnershipCoverage::Pending, OwnershipCoverage::Pending)
                };
                assert_eq!((release.coverage, copy.coverage), coverage);
            } else {
                assert!(report.ownership_protection().is_empty());
                assert!(report.error_representation().is_none());
            }
            let bodies = report.bodies();
            for (name, _, generics, with_authority, without_authority) in cases {
                let expected = if !admitted {
                    without_authority
                } else if matches!(name, "Protected" | "Cause") && !artifacts {
                    None
                } else {
                    with_authority
                };
                let original = |source: &str| {
                    app.syntax
                        .nodes
                        .iter()
                        .position(|n| n.span.bytes() == source.as_bytes())
                        .unwrap()
                };
                let binder = format!("bound{name}");
                let binding = app
                    .syntax
                    .nodes
                    .iter()
                    .enumerate()
                    .find_map(|(node, n)| {
                        if n.span.bytes() == binder.as_bytes()
                            && let Some(fact) = bodies.node(app, node).unwrap()
                            && let NodeFactKind::Binding(binding) = &fact.kind
                        {
                            Some(binding)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                let bound = binding
                    .ty
                    .as_ref()
                    .map(|ty| !matches!(ty.as_ref(), TypeTerm::Reference { .. }));
                assert_eq!(
                    bound, expected,
                    "binder {name}, admitted={admitted} artifacts={artifacts}"
                );
                let arrow = format!("() => captured{name}");
                let arrow = bodies.arrow(app, original(&arrow)).unwrap().unwrap();
                let mode = expected.map(|copy| {
                    if copy {
                        InvocationMode::ReadOnly
                    } else {
                        InvocationMode::Once
                    }
                });
                assert_eq!(
                    arrow.mode, mode,
                    "arrow {name}, admitted={admitted} artifacts={artifacts}"
                );
                let [capture] = arrow.captures.as_slice() else {
                    panic!("{:?}", arrow.captures)
                };
                assert_eq!(capture.demand, mode);
                assert_eq!(capture.storage.as_ref(), Some(&capture.ty));
                if generics.is_empty()
                    && let Some(ty) = &binding.ty
                {
                    match expected {
                        Some(true) => assert_eq!(ty, &capture.ty),
                        Some(false) => assert!(matches!(ty.as_ref(),
                            TypeTerm::Reference { mutable: false, referent } if referent == &capture.ty)),
                        None => unreachable!(),
                    }
                }
                if admitted && name == "Where" {
                    for span in [&binder, &format!("captured{name}")] {
                        let goal = bodies
                            .goals()
                            .iter()
                            .find(|g| {
                                g.site.module == &app.key
                                    && g.site.span.bytes() == span.as_bytes()
                                    && g.source.span.bytes() == b"noBorrow(T)"
                                    && g.state == GoalState::Proved
                            })
                            .expect("disposer requirement proof");
                        let [dependency] = goal.dependencies.as_slice() else {
                            panic!("{goal:?}")
                        };
                        let premise = bodies.goal(dependency).unwrap();
                        let GoalJudgment::Requirement { arguments, .. } = &goal.judgment else {
                            panic!("{goal:?}")
                        };
                        let GoalJudgment::Requirement {
                            arguments: assumed, ..
                        } = &premise.judgment
                        else {
                            panic!("{premise:?}")
                        };
                        assert_eq!(arguments, assumed);
                        assert_eq!(premise.state, GoalState::Assumption);
                        assert!(goal.premise.is_none());
                    }
                }
                if expected.is_none() {
                    assert!(
                        bodies
                            .obligations()
                            .iter()
                            .any(|o| o.site.module == &app.key
                                && o.site.span.bytes() == binder.as_bytes()
                                && o.reason == ScalarUnsupported::ImplicitCopy)
                    );
                    assert!(
                        bodies
                            .obligations()
                            .iter()
                            .any(|o| o.site.node == arrow.site.node
                                && o.site.module == &app.key
                                && o.reason == ScalarUnsupported::CaptureMode)
                    );
                }
            }
        }
    }
}

fn combined_installed_capture(request: &InstalledRequest) {
    use compiler::analysis::scalars::{BodyOwner, NodeFactKind, data::DataExpression};
    use compiler::parser::NodeKind;
    fs::write(
        &request.entry,
        r#"
        import std::shared::Shared as Counted;
        import std::{List, Result as Outcome};
        import std::result::isOk;
        import std::convert::into;
        function convertAndShow(value: u8): Outcome<string> {
            let xs: List<i32> = [into<i32>(value)];
            return Outcome.Ok(`${xs[0u64]}`);
        }
        function iterateAll(xs: List<i32>): void { for (let _ of xs) { continue; } }
        function retainType(value: &Counted<i32>): Counted<i32> { return copy(value); }
        function releaseType(value: Counted<i32>): void { dispose(value); }
        function borrowType(input: &[Counted<i32>]): &Counted<i32> { let [item] = input; return item; }
        function dropsCounted(scalar: Counted<i32>, retained: Counted<Outcome<bool>>): void {}
        let startup = convertAndShow(5u8);
        let _ = isOk(&startup);
        expect isOk(&convertAndShow(6u8));
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
        assert!(report.option_role().is_some());
        assert!(report.result_role().is_some());
        assert!(report.error_storage_source().is_some());
        let uses = report.result_use().unwrap();
        let holds_payload = |span: &[u8]| {
            uses.pending().iter().any(|pending| {
                pending.site.span.bytes() == span
                    && pending.blocker == compiler::analysis::typed::ResultUseBlocker::Containment
            })
        };
        assert!(holds_payload(b"retained: Counted<Outcome<bool>>"));
        assert!(!holds_payload(b"scalar: Counted<i32>"));
        assert!(report.show_role().is_some());
        assert!(report.into_role().is_some());
        let list = report.list_role().expect("original List role");
        assert_eq!(list.declaration.id.module.path, "collection.rill");
        assert_eq!(
            report.result_role().unwrap().declaration.id.module.path,
            "result.rill"
        );
        let shared = graph
            .modules()
            .iter()
            .find(|module| module.key.path == "shared.rill")
            .unwrap();
        let protected = report.ownership_protection();
        let (protected, others): (Vec<_>, Vec<_>) = protected
            .iter()
            .partition(|scope| scope.owner.id.module.path == "shared.rill");
        assert_eq!(protected.len(), 2);
        assert!(
            others.iter().all(|scope| {
                scope.owner.id.module.origin == Origin::Standard
                    && matches!(
                        scope.owner.id.module.path.as_str(),
                        "channel.rill" | "mutex.rill"
                    )
            }),
            "the extra protection belongs to standard modules the program does not import"
        );
        for scope in &protected {
            let provider = scope.provider.as_ref().expect("delivered Shared provider");
            assert!(
                report
                    .bodies()
                    .body(shared, BodyOwner::Declaration(&provider.implementation.id))
                    .unwrap()
                    .is_some(),
                "native wrapper body was not checked: {:?}",
                report.bodies().obligations()
            );
        }
        assert_eq!(report.completion(), TypedCompletion::Incomplete);
        let app = graph
            .modules()
            .iter()
            .find(|module| module.key.origin == Origin::Application)
            .unwrap();
        let convert = app
            .declarations
            .iter()
            .find(|declaration| declaration.name.as_deref() == Some("convertAndShow"))
            .unwrap();
        assert!(
            report
                .bodies()
                .body(app, BodyOwner::Declaration(&convert.id))
                .unwrap()
                .is_some(),
            "{:?}\n{:?}",
            report.bodies().findings(),
            report.bodies().obligations()
        );
        assert!(
            graph
                .modules()
                .iter()
                .any(|module| module.key.path == "iter.rill")
        );
        for name in ["retainType", "releaseType", "borrowType"] {
            let declaration = app
                .declarations
                .iter()
                .find(|item| item.name.as_deref() == Some(name))
                .unwrap();
            assert!(
                report
                    .bodies()
                    .body(app, BodyOwner::Declaration(&declaration.id))
                    .unwrap()
                    .is_some(),
                "{name}"
            );
        }
        for (span, action) in [
            (
                b"copy(value)".as_slice(),
                NativeOwnershipAction::SharedRetain,
            ),
            (
                b"dispose(value)".as_slice(),
                NativeOwnershipAction::SharedRelease,
            ),
        ] {
            let id = app
                .syntax
                .nodes
                .iter()
                .position(|n| n.kind == NodeKind::Call && n.span.bytes() == span)
                .unwrap();
            let NodeFactKind::Expression(value) =
                &report.bodies().node(app, id).unwrap().unwrap().kind
            else {
                panic!("not a call")
            };
            let Some(DataExpression::NativeOwnership(operation)) = &value.data else {
                panic!("no admitted operation")
            };
            assert!(value.ordinary_context.is_none());
            assert_eq!(operation.action, action);
            let evidence = protected
                .iter()
                .find(|scope| scope.rule == OwnershipRule::Required(action))
                .unwrap()
                .provider
                .as_ref()
                .unwrap();
            assert_eq!(*operation.implementation, evidence.implementation.id);
            assert_eq!(*operation.entry, evidence.entry.id);
            assert_eq!(operation.artifact, evidence.artifact);
        }
        let expectation = app
            .declarations
            .iter()
            .find(|declaration| declaration.id.kind == loader::DeclarationKind::Expectation)
            .unwrap();
        for owner in [BodyOwner::Startup, BodyOwner::Declaration(&expectation.id)] {
            assert!(report.bodies().body(app, owner).unwrap().is_some());
        }
        assert!(
            report.bodies().findings().is_empty(),
            "{:?}",
            report.bodies().findings()
        );
    }
}

fn guard_and_endpoint_regions(request: &InstalledRequest) {
    fs::write(
        &request.entry,
        "
        import std::shared::Shared;
        import std::mutex::LockGuard as Guard;
        import std::channel::{Sender, Receiver};
        type Message<T> = { part: T; }
        implement dispose(value: Guard<Message<bool>>): void { }
        implement<T> copy(value: &Guard<Message<T>>): Guard<Message<T>> { }
        implement<T> copy(value: &Receiver<Message<T>>): Receiver<Message<T>> { }
        implement<T> dispose(value: Sender<T>): void { }
    ",
    )
    .unwrap();
    for test_mode in [false, true] {
        let graph = loader::load_installed(InstalledRequest {
            test_mode,
            ..request.clone()
        })
        .unwrap();
        assert_guard_and_endpoint_regions(&graph);
    }
}

fn assert_guard_and_endpoint_regions(graph: &loader::Graph) {
    let report = analyze_typed(graph, AnalysisLimits::default());
    assert_eq!(report.ownership_protection().len(), 8);
    let mut observed: Vec<_> = report
        .ownership_overlaps()
        .iter()
        .map(|finding| {
            (
                finding.owner.name.as_deref().unwrap(),
                finding.family == &graph.prelude().copy,
            )
        })
        .collect();
    observed.sort_unstable();
    assert_eq!(
        observed,
        [
            ("LockGuard", false),
            ("LockGuard", true),
            ("Receiver", true),
            ("Sender", false)
        ]
    );
    for scope in report.ownership_protection() {
        let copy = scope.family == &graph.prelude().copy;
        let owner = scope.owner.name.as_deref().unwrap();
        if copy && matches!(owner, "LockGuard" | "Receiver") {
            assert_eq!(scope.rule, OwnershipRule::ProhibitedCopy);
            assert_eq!(scope.coverage, OwnershipCoverage::Overlap);
            assert!(
                scope.provider.is_none(),
                "prohibitions do not have a provider"
            );
        } else {
            assert!(scope.provider.is_some(), "{scope:?}");
        }
    }
}

fn missing_evidence(root: &Path, request: &InstalledRequest) {
    let archive = root.join(ARCHIVE);
    let withheld = root.join("withheld.rlib");
    fs::rename(&archive, &withheld).unwrap();
    let graph = loader::load_installed(request.clone()).unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert_eq!(report.ownership_protection().len(), 8);
    assert!(
        report
            .ownership_protection()
            .iter()
            .all(|p| p.provider.is_none() && p.coverage == OwnershipCoverage::Pending)
    );
    assert!(report.ownership_overlaps().is_empty());
    assert!(report.native_representations().is_empty());
    fs::rename(&withheld, &archive).unwrap();
    assert_eq!(
        loader::validate_inputs(&graph, Limits::default())
            .unwrap_err()
            .kind,
        FailureKind::SourceChanged
    );

    let archive_size = usize::try_from(fs::metadata(&archive).unwrap().len()).unwrap();
    let limited = InstalledRequest {
        limits: Limits {
            max_source_bytes: archive_size - 1,
            ..request.limits
        },
        ..request.clone()
    };
    assert_eq!(
        loader::load_installed(limited).unwrap_err().kind,
        FailureKind::ResourceLimit
    );

    let select = |name: &str| Selector {
        module: "core.rill".into(),
        name: name.into(),
    };
    let graph = loader::load(Request {
        entry: request.entry.clone(),
        target: request.target.clone(),
        standard_root: root.join("lib/rill/std"),
        prelude: Prelude {
            copy: select("copy"),
            dispose: select("dispose"),
        },
        options: request.options.clone(),
        test_mode: request.test_mode,
        limits: request.limits,
    })
    .unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert!(
        report.ownership_protection().is_empty(),
        "even identical source bytes have no authority"
    );
    assert!(report.ownership_overlaps().is_empty());
}

fn closed_roles_without_native_providers(root: &Path, request: &InstalledRequest) {
    fs::write(
        &request.entry,
        "
        import std::{List, Result};
        import std::bytes::Bytes;
        import std::shared::Shared;
        import std::mutex::LockGuard;
        import std::channel::{Sender, Receiver};
        import std::iter::{iterate, OwnedListIter};
        import std::convert::into;
        function unresolvedGuard(lock: LockGuard<Result<bool>.Ok>): void { dispose(lock); }
        function unresolvedCounted(counted: Shared<Result<bool>.Ok>): void { dispose(counted); }
        function unresolvedSender(tx: Sender<Result<bool>.Ok>): void { dispose(tx); }
        function unresolvedReceiver(rx: Receiver<Result<bool>.Ok>): void { dispose(rx); }
        function unresolvedList(items: List<Result<bool>.Ok>): void { dispose(items); }
        function unresolvedIter(items: OwnedListIter<Result<bool>.Ok>): void { dispose(items); }
        ",
    )
    .unwrap();
    let archive = root.join(ARCHIVE);
    let saved = archive.with_extension("saved");
    fs::rename(&archive, &saved).unwrap();
    for test_mode in [false, true] {
        let graph = loader::load_installed(InstalledRequest {
            test_mode,
            ..request.clone()
        })
        .unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert_eq!(
            report.core_blockers(),
            &[
                CoreBlocker::Providers,
                CoreBlocker::Protection,
                CoreBlocker::Representation
            ]
        );
        assert!(report.native_representations().is_empty());
        assert!(
            report
                .ownership_protection()
                .iter()
                .all(|p| p.provider.is_none())
        );
        let exact = AnalysisLimits {
            max_work: report.statistics().work,
            ..AnalysisLimits::default()
        };
        assert_eq!(
            analyze_typed(&graph, exact).core_blockers(),
            report.core_blockers()
        );
        let short = analyze_typed(
            &graph,
            AnalysisLimits {
                max_work: exact.max_work - 1,
                ..exact
            },
        );
        limit::assert_typed_limit(&short);
        assert!(!short.core_blockers().contains(&CoreBlocker::Roles));

        let source_only = loader::load(graph.request().clone()).unwrap();
        assert!(
            analyze_typed(&source_only, AnalysisLimits::default())
                .core_blockers()
                .contains(&CoreBlocker::Roles)
        );
    }
    fs::rename(&saved, &archive).unwrap();
    for test_mode in [false, true] {
        let graph = loader::load_installed(InstalledRequest {
            test_mode,
            ..request.clone()
        })
        .unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert!(
            !report
                .core_blockers()
                .contains(&CoreBlocker::Representation)
        );
        assert_eq!(report.native_representations().len(), 4);
        assert_eq!(report.collection_representations().count(), 4);
        assert!(report.error_representation().is_some());
        assert!(report.core_blockers().is_empty());
        assert_eq!(report.completion(), TypedCompletion::Incomplete);
        assert!(report.receipt().is_none());
        let uses = report.result_use().unwrap();
        let modules = [
            "channel.rill",
            "mutex.rill",
            "shared.rill",
            "collection.rill",
            "iter.rill",
        ];
        for pending in uses.pending() {
            assert!(
                pending.site.module.origin != Origin::Standard
                    || !modules.contains(&pending.site.module.path.as_str()),
                "unexpected symbolic release blocker {:?} at {}: {}",
                pending.blocker,
                pending.site.module.path,
                String::from_utf8_lossy(pending.site.span.bytes()),
            );
        }
        assert_eq!(
            uses.pending()
                .iter()
                .filter(|p| p.site.module.origin == Origin::Application)
                .count(),
            6
        );
        assert!(
            uses.discards()
                .iter()
                .all(|w| w.origin.module.origin != Origin::Application)
        );
    }
}

fn complete_provider_and_protection_scope(request: &InstalledRequest) {
    const IMPORTS: &str = "
        import std::{List, Result};
        import std::bytes::Bytes;
        import std::shared::Shared as Counted;
        import std::mutex::LockGuard;
        import std::channel::{Sender, Receiver};
        import std::iter::iterate;
        import std::convert::into;
    ";
    for test_mode in [false, true] {
        let request = InstalledRequest {
            test_mode,
            ..request.clone()
        };
        fs::write(&request.entry, IMPORTS).unwrap();
        let graph = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        for blocker in [CoreBlocker::Providers, CoreBlocker::Protection] {
            assert!(!report.core_blockers().contains(&blocker), "{blocker:?}");
        }
        let exact = AnalysisLimits {
            max_work: report.statistics().work,
            ..AnalysisLimits::default()
        };
        assert_eq!(
            analyze_typed(&graph, exact).core_blockers(),
            report.core_blockers()
        );
        let exhausted = analyze_typed(
            &graph,
            AnalysisLimits {
                max_work: report.bodies().statistics().work,
                ..AnalysisLimits::default()
            },
        );
        limit::assert_typed_limit(&exhausted);
        assert!(exhausted.core_blockers().contains(&CoreBlocker::Providers));
        assert!(exhausted.core_blockers().contains(&CoreBlocker::Protection));

        fs::write(
            &request.entry,
            format!("{IMPORTS}\nfunction ignores(value: Result<bool>): void {{}}"),
        )
        .unwrap();
        let discards = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&discards, AnalysisLimits::default());
        assert!(!report.core_blockers().contains(&CoreBlocker::Providers));
        assert!(!report.core_blockers().contains(&CoreBlocker::Protection));
        assert!(
            report
                .result_use()
                .unwrap()
                .discards()
                .iter()
                .any(|discard| {
                    discard.origin.module.origin == Origin::Application && discard.code() == "R0103"
                })
        );

        fs::write(
            &request.entry,
            format!("{IMPORTS}\nimplement copy(value: &Missing): Missing {{}}"),
        )
        .unwrap();
        let unknown = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&unknown, AnalysisLimits::default());
        assert!(!report.core_blockers().contains(&CoreBlocker::Providers));
        assert!(report.core_blockers().contains(&CoreBlocker::Protection));
        assert!(report.ownership_overlaps().is_empty());

        fs::write(
            &request.entry,
            format!(
                "{IMPORTS}\n            type Payload<T> = {{ item: T; }}
            implement<T> copy(value: &Counted<Payload<T>>): Counted<Payload<T>> {{}}
        "
            ),
        )
        .unwrap();
        let overlap = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&overlap, AnalysisLimits::default());
        assert!(!report.core_blockers().contains(&CoreBlocker::Providers));
        assert!(report.receipt().is_none());
        assert_eq!(report.ownership_overlaps()[0].code(), "R0500");
        limit::assert_typed_limits_below(&overlap, &report);

        fs::write(
            &request.entry,
            format!("{IMPORTS}\nimplement into<u8>(value: i32): u8 {{ return 0u8; }}"),
        )
        .unwrap();
        let gap = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&gap, AnalysisLimits::default());
        assert!(!report.core_blockers().contains(&CoreBlocker::Providers));
        assert!(report.receipt().is_none());
        assert!(
            report
                .into_registration_issues()
                .unwrap()
                .iter()
                .any(|issue| issue.kind.code() == Some("R0311"))
        );
        limit::assert_typed_limits_below(&gap, &report);
    }
}

fn installed_receipts(request: &InstalledRequest) {
    use compiler::analysis::Stage;
    const MAIN: &str = include_str!("../../../tests/installed/typed/main.rill");
    const SUPPORT: &str = include_str!("../../../tests/installed/typed/checks.rill");

    let checks = request.entry.with_file_name("checks.rill");
    for test_mode in [false, true] {
        let request = InstalledRequest {
            test_mode,
            ..request.clone()
        };
        fs::write(&request.entry, MAIN).unwrap();
        fs::write(&checks, SUPPORT).unwrap();
        let graph = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        let receipt = report.receipt().unwrap_or_else(|| {
            let pending: Vec<_> = report
                .result_use()
                .unwrap()
                .pending()
                .iter()
                .map(|p| {
                    (
                        p.site.module.path.as_str(),
                        p.blocker,
                        String::from_utf8_lossy(p.site.span.bytes()),
                    )
                })
                .collect();
            panic!(
                "no typed receipt: {:?} {:?}; Result use: {pending:?}",
                report.completion(),
                report.core_blockers()
            );
        });
        assert_eq!(receipt.context(), graph.id());
        let entry = graph
            .modules()
            .iter()
            .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
            .unwrap();
        let call = entry
            .syntax
            .nodes
            .iter()
            .position(|node| {
                node.kind == compiler::parser::NodeKind::Call
                    && node.span.bytes() == b"chooseOne(&value)"
            })
            .unwrap();
        let call = report.bodies().ordinary_call(entry, call).unwrap().unwrap();
        assert!(call.checked);
        let goal = report.bodies().goal(call.goal.as_ref().unwrap()).unwrap();
        assert_eq!(goal.state, compiler::analysis::generics::GoalState::Proved);
        assert!(goal.premise.is_none());
        let source = graph
            .implementations()
            .iter()
            .find(|source| &source.family == call.family)
            .unwrap();
        assert_eq!(
            goal.selection.as_ref().unwrap().implementation,
            &source.declaration
        );
        assert_eq!(
            receipt.not_performed(),
            &[Stage::Ownership, Stage::Execution]
        );
        let statistics = receipt.statistics();
        let exact = AnalysisLimits {
            max_work: statistics.work,
            max_entries: statistics.entries,
            max_depth: statistics.max_depth,
        };
        assert!(analyze_typed(&graph, exact).receipt().is_some());
        let short = analyze_typed(
            &graph,
            AnalysisLimits {
                max_work: exact.max_work - 1,
                ..exact
            },
        );
        limit::assert_typed_limit(&short);

        for (source, origin) in [
            (
                "function unused(input: Result<bool>.Ok): void {}",
                b"input: Result<bool>.Ok".as_slice(),
            ),
            (
                "function pick(ignored: &Result<bool>.Ok, answer: &bool): &bool { return answer; }
                 function dereference(answer: &bool): bool { return *pick(&Result.Ok(true), answer); }",
                b"Result.Ok(true)".as_slice(),
            ),
            (
                "function missedDiscard(): never { *(&Result.Ok(true)); while (true) {} }",
                b"Result.Ok(true)".as_slice(),
            ),
        ] {
            fs::write(&request.entry, format!("{MAIN}\n{source}\n")).unwrap();
            let rejected = loader::load_installed(request.clone()).unwrap();
            let report = analyze_typed(&rejected, AnalysisLimits::default());
            assert_eq!(report.completion(), TypedCompletion::SourceRejected);
            assert!(report.result_use().unwrap().discards().iter().any(|w| {
                w.code() == "R0103" && w.origin.span.bytes() == origin
            }));
            assert!(report.receipt().is_none());
        }

        let rejected_impl = "function dormant<T>(value: &T): bool;
                             implement<T> dormant(value: &T): bool { return 7i32; }";
        fs::write(&request.entry, format!("{MAIN}\n{rejected_impl}\n")).unwrap();
        let rejected = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&rejected, AnalysisLimits::default());
        assert_eq!(report.completion(), TypedCompletion::SourceRejected);
        assert!(report.receipt().is_none());
        assert!(report.bodies().findings().iter().any(|f| {
            f.site.module.path == "main.rill"
                && f.kind.code() == Some("R0100")
                && f.site.span.bytes() == b"7i32"
        }));
        fs::write(
            &request.entry,
            format!("{MAIN}\n{rejected_impl}\nfunction opaque<T>(input: T): void {{}}\n"),
        )
        .unwrap();
        let mixed = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&mixed, AnalysisLimits::default());
        assert_eq!(report.completion(), TypedCompletion::Incomplete);
        assert!(report.receipt().is_none());
        assert!(
            report
                .result_use()
                .unwrap()
                .pending()
                .iter()
                .any(|p| p.site.span.bytes() == b"input: T")
        );

        fs::write(&request.entry, MAIN).unwrap();
        fs::write(&checks, format!("{SUPPORT}\nexpect 1i32;\n")).unwrap();
        let rejected = loader::load_installed(request).unwrap();
        let report = analyze_typed(&rejected, AnalysisLimits::default());
        assert_eq!(report.completion(), TypedCompletion::SourceRejected);
        assert!(report.receipt().is_none());
        assert!(
            report
                .bodies()
                .findings()
                .iter()
                .any(|f| { f.site.module.path == "checks.rill" && f.kind.code() == Some("R0101") })
        );
    }
    fs::write(checks, SUPPORT).unwrap();
}

fn installed_ordinary_family(request: &InstalledRequest) {
    use compiler::analysis::TypeTerm;
    use compiler::analysis::scalars::generics::GoalState;
    use compiler::analysis::scalars::{BodyOwner, NodeFactKind, ScalarUnsupported};
    use compiler::loader::DeclarationKind;
    const MAIN: &str = include_str!("../../../tests/installed/typed/main.rill");
    const FAMILY: &str = "
        export function chooseOne<T>(value: &T): bool;
        implement<U> chooseOne(value: &U): bool { return true; }
        function usesOrdinary(value: i32): bool { return chooseOne(&value); }
        export function relay<T>(value: T): T;
        implement<V> relay(value: V): V { return value; }
    ";
    let entry = request.entry.with_file_name("ordinary_main.rill");
    let ordinary = request.entry.with_file_name("ordinary.rill");
    fs::write(&ordinary, FAMILY).unwrap();
    for test_mode in [false, true] {
        let request = InstalledRequest {
            entry: entry.clone(),
            test_mode,
            ..request.clone()
        };
        fs::write(&entry, format!("
            import ordinary::{{chooseOne as pick, relay}};
            {MAIN}
            function probeOrdinary(value: i32): bool {{ return pick(&value); }}
            function produce<T, U = bool>(value: &T): U;
            implement<A, B> produce(value: &A): B where noBorrow(B) {{ while (true) {{}} }}
            function probeDefault(value: i32): bool {{ let produced = produce(&value); return produced; }}
            function directRelay(value: Result<bool>.Ok): Result<bool>.Ok {{ return relay(value); }}
            function valueRelay(value: Result<bool>.Ok): Result<bool>.Ok {{
                let held: (Result<bool>.Ok) => Result<bool>.Ok = relay;
                return held(value);
            }}
        ")).unwrap();
        let graph = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        let obligations: Vec<_> = report
            .bodies()
            .obligations()
            .iter()
            .map(|o| (o.reason, o.site.span.bytes()))
            .collect();
        let pending: Vec<_> = report
            .result_use()
            .unwrap()
            .pending()
            .iter()
            .map(|p| (p.blocker, p.site.span.bytes()))
            .collect();
        let receipt = report.receipt().unwrap_or_else(|| {
            panic!(
                "mode={test_mode}: {:?}; {:?}; {obligations:?}; {pending:?}",
                report.completion(),
                report.core_blockers()
            )
        });
        let app = graph
            .modules()
            .iter()
            .find(|m| m.key.path == "ordinary_main.rill")
            .unwrap();
        let node = app
            .syntax
            .nodes
            .iter()
            .position(|n| n.span.bytes() == b"pick(&value)")
            .unwrap();
        let call = report.bodies().ordinary_call(app, node).unwrap().unwrap();
        assert!(call.checked && call.premise.is_none());
        let fact = report.bodies().node(app, node).unwrap().unwrap();
        let NodeFactKind::Expression(value) = &fact.kind else {
            panic!("expected call")
        };
        let call = value.call.as_ref().unwrap();
        let selection = call.selection.as_ref().expect("actual ordinary winner");
        assert_eq!(call.callee.module.path, "ordinary.rill");
        assert_eq!(call.bindings[0].name, "pick");
        assert_eq!(selection.implementation.module, call.callee.module);
        assert_eq!(
            selection.implementation.kind,
            DeclarationKind::Implementation
        );
        let [family_arg] = call.substitution.as_slice() else {
            panic!("family substitution")
        };
        let [body_arg] = selection.substitution.as_slice() else {
            panic!("body substitution")
        };
        assert_ne!(family_arg.binder, body_arg.binder);
        assert!(matches!(body_arg.argument.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"));
        let owner = graph
            .modules()
            .iter()
            .find(|m| m.key == selection.implementation.module)
            .unwrap();
        assert!(
            report
                .bodies()
                .body(owner, BodyOwner::Declaration(selection.implementation))
                .unwrap()
                .is_some()
        );
        let produce = app
            .syntax
            .nodes
            .iter()
            .position(|n| n.span.bytes() == b"produce(&value)")
            .unwrap();
        let NodeFactKind::Expression(value) =
            &report.bodies().node(app, produce).unwrap().unwrap().kind
        else {
            panic!("expected call")
        };
        let call = value.call.as_ref().unwrap();
        assert_eq!(call.defaults.len(), 1);
        assert_eq!(call.selection.as_ref().unwrap().substitution.len(), 2);
        let value = app
            .syntax
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.span.bytes() == b"relay")
            .find_map(|(id, _)| {
                report
                    .bodies()
                    .node(app, id)
                    .unwrap()
                    .and_then(|n| n.callable.as_ref()?.function_value.as_ref())
            })
            .expect("formed family value");
        assert_eq!(value.declaration.kind, DeclarationKind::Family);
        let target = value.selection.as_ref().unwrap();
        assert_eq!(target.implementation.kind, DeclarationKind::Implementation);
        assert_ne!(target.substitution[0].binder, value.substitution[0].binder);

        let exact = AnalysisLimits {
            max_work: receipt.statistics().work,
            max_entries: receipt.statistics().entries,
            max_depth: receipt.statistics().max_depth,
        };
        assert!(analyze_typed(&graph, exact).receipt().is_some());
        for short in [
            AnalysisLimits {
                max_work: exact.max_work - 1,
                ..exact
            },
            AnalysisLimits {
                max_entries: exact.max_entries - 1,
                ..exact
            },
        ] {
            limit::assert_typed_limit(&analyze_typed(&graph, short));
        }

        fs::write(
            &entry,
            "import ordinary::chooseOne as pick; expect pick(&true);",
        )
        .unwrap();
        let partial = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&partial, AnalysisLimits::default());
        assert_eq!(report.core_blockers(), []);
        assert!(
            !report
                .bodies()
                .obligations()
                .iter()
                .any(|o| o.reason == ScalarUnsupported::FamilySelection
                    && o.site.span.bytes() == b"pick(&true)")
        );
        assert!(report.receipt().is_some());

        fs::write(
            &entry,
            format!(
                "
            {MAIN}
            function checkPattern<T>(value: &T): bool;
            implement<T> checkPattern(value: &T): bool {{ return true; }}
            implement checkPattern(value: &i32): bool where noBorrow(&i32) {{ return true; }}
            expect checkPattern(&1i32);
        "
            ),
        )
        .unwrap();
        let graph = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        let app = graph
            .modules()
            .iter()
            .find(|m| m.key.path == "ordinary_main.rill")
            .unwrap();
        let node = app
            .syntax
            .nodes
            .iter()
            .position(|n| n.span.bytes() == b"checkPattern(&1i32)")
            .unwrap();
        let call = report.bodies().ordinary_call(app, node).unwrap().unwrap();
        let proof = report.bodies().goal(call.goal.as_ref().unwrap()).unwrap();
        assert_eq!(proof.state, GoalState::Disproved);
        let winner = proof.selection.as_ref().unwrap().implementation;
        assert!(
            app.declarations
                .iter()
                .any(|d| d.id == *winner && d.span.bytes().starts_with(b"implement checkPattern"))
        );
        assert!(report.bodies().node(app, node).unwrap().is_none());
        assert!(
            report
                .bodies()
                .findings()
                .iter()
                .any(|f| f.site.node == node && f.kind.code() == Some("R0302"))
        );
        assert_eq!(report.completion(), TypedCompletion::SourceRejected);

        fs::write(
            &entry,
            format!(
                "
            {MAIN}
            function repeated<T>(value: &T): bool;
            implement<A> repeated(value: &A): bool {{ return true; }}
            implement<B> repeated(value: &B): bool {{ return false; }}
        "
            ),
        )
        .unwrap();
        let graph = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert_eq!(report.completion(), TypedCompletion::SourceRejected);
        assert!(
            report
                .bodies()
                .findings()
                .iter()
                .any(|f| f.kind.code() == Some("R0304"))
        );

        fs::write(
            &entry,
            format!(
                "
            import std::core::copy as protectedCopy;
            {MAIN}
            function blocked(value: &Shared<i32>): Shared<i32> {{
                let held: (&Shared<i32>) => Shared<i32> = protectedCopy;
                return held(value);
            }}
        "
            ),
        )
        .unwrap();
        let graph = loader::load_installed(request).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert!(!report.core_blockers().contains(&CoreBlocker::Roles));
        assert!(
            report
                .bodies()
                .obligations()
                .iter()
                .any(|o| o.reason == ScalarUnsupported::FamilySelection
                    && o.site.span.bytes() == b"protectedCopy")
        );
        assert!(report.receipt().is_none());
    }
}

fn installed_pair_evidence(request: &InstalledRequest) {
    use compiler::analysis::generics::{GoalBlocker, GoalState};
    use compiler::analysis::scalars::ScalarFindingKind;
    use compiler::parser::NodeKind;

    const MAIN: &str = include_str!("../../../tests/installed/typed/main.rill");
    const PAIR: &str = "
        function keepPair<W, R>(jobs: W): R;
        implement<F, G, A, B> keepPair<[F, G], [F, G]>(jobs: [F, G]): [F, G]
        where callableOnce(F, [], A), callableOnce(G, [], B),
              sendable(F), sendable(G), sendable(A), sendable(B) { return jobs; }
    ";
    const STANDALONE: &str = "
        function keepFirst<F, G>(jobs: [F, G]): [F, G]
        where callableOnce(F, [], i32), callableOnce(G, [], i32), noBorrow(F) { return jobs; }
        function forward<F, G>(first: F, second: G): [F, G]
        where callableOnce(F, [], i32), callableOnce(G, [], i32), noBorrow(F) {
            return keepFirst<F, G>([first, second]);
        }
    ";
    const COMPETITION: &str = "
        function choosePair<W, R>(jobs: W): R;
        implement<W> choosePair<W, i32>(jobs: W): i32 { let _ = jobs; return 7i32; }
        implement<F, G> choosePair<[F, G], bool>(jobs: [F, G]): bool
        where callableOnce(F, [], i32), callableOnce(G, [], i32), noBorrow(F) {
            let _ = jobs;
            return true;
        }
    ";
    for test_mode in [false, true] {
        let request = InstalledRequest {
            test_mode,
            ..request.clone()
        };
        let capture = |header: &str, caller: &str| {
            fs::write(&request.entry, format!("{MAIN}\n{header}\n{caller}")).unwrap();
            loader::load_installed(request.clone()).unwrap()
        };
        for (different_signatures, argument) in [
            (false, "() => { let _ = (&right)[0u64]; return 2i32; }"),
            (true, "second"),
        ] {
            let binding = if different_signatures {
                "let second = () => (&right)[0u64];"
            } else {
                ""
            };
            let graph = capture(
                PAIR,
                &format!(
                    "
                function checkJobs(left: List<i32>, right: List<bool>): void {{
                    {binding}
                    let _ = keepPair([() => (&left)[0u64], {argument}]);
                }}
            "
                ),
            );
            assert_pair_evidence(&graph, different_signatures);
        }

        for (pair, expected, state) in [
            (
                "() => (&numbers)[0u64], () => 1i32",
                TypedCompletion::SourceRejected,
                GoalState::Disproved,
            ),
            (
                "() => 1i32, () => (&numbers)[0u64]",
                TypedCompletion::Checked,
                GoalState::Proved,
            ),
        ] {
            let graph = capture(
                STANDALONE,
                &format!(
                    "
                function checkJobs(numbers: List<i32>): void {{ let _ = keepFirst([{pair}]); }}
            "
                ),
            );
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert_eq!(report.completion(), expected);
            assert_eq!(
                report.receipt().is_some(),
                expected == TypedCompletion::Checked
            );
            let goal = report
                .bodies()
                .goals()
                .iter()
                .find(|g| {
                    g.site.span.bytes().starts_with(b"keepFirst([")
                        && g.source.span.bytes() == b"noBorrow(F)"
                })
                .unwrap();
            assert_eq!(goal.state, state);
            assert_eq!(
                goal.arrow_origin.as_ref().unwrap().site.span.bytes(),
                pair.split(", ").next().unwrap().as_bytes()
            );
            let symbolic = report
                .bodies()
                .goals()
                .iter()
                .find(|g| {
                    g.site.span.bytes() == b"keepFirst<F, G>([first, second])"
                        && g.source.span.bytes() == b"noBorrow(F)"
                })
                .unwrap();
            assert!(symbolic.arrow_origin.is_none());
            assert_eq!(symbolic.state, GoalState::Proved);
            let [assumption] = symbolic.dependencies.as_slice() else {
                panic!("written assumption")
            };
            assert_eq!(
                report.bodies().goal(assumption).unwrap().state,
                GoalState::Assumption
            );
        }

        // Same input values; only a requirement tied to a written input occurrence
        // may use its origin. In particular R can equal W without denoting W.
        let caller = "function checkJobs(left: List<i32>, right: List<i32>): void { let _ = keepPair(([() => (&left)[0u64], () => (&right)[0u64]])); }";
        for (header, property, expected, origins, state) in [
            (PAIR.to_owned(), "sendable(F)", TypedCompletion::Checked, vec!["() => (&left)[0u64]"], GoalState::Proved),
            ("function keepPair<W,R>(jobs: W): R; implement<F> keepPair<[F,() => i32],[F,() => i32]>(jobs: [F,() => i32]): [F,() => i32] where callableOnce(F, [], i32), sendable(F) { return jobs; }".to_owned(), "sendable(F)", TypedCompletion::Checked, vec!["() => (&left)[0u64]"], GoalState::Proved),
            (PAIR.replace("(jobs: W): R;", "(jobs: W): R where sendable(W);"), "sendable(W)", TypedCompletion::Checked, vec!["() => (&left)[0u64]", "() => (&right)[0u64]"], GoalState::Proved),
            (PAIR.replace("(jobs: W): R;", "(jobs: W): R where sendable(R);"), "sendable(R)", TypedCompletion::Incomplete, vec![], GoalState::Unresolved(GoalBlocker::CallableEnvironment)),
            (PAIR.replace("(jobs: W): R;", "(jobs: W): R where noBorrow(W);"), "noBorrow(W)", TypedCompletion::SourceRejected, vec!["() => (&left)[0u64]", "() => (&right)[0u64]"], GoalState::Disproved),
            ("function keepPair<W,R>(jobs: W): R; implement<F> keepPair<[F,F],[F,F]>(jobs: [F,F]): [F,F] where callableOnce(F, [], i32), sendable(F) { return jobs; }".to_owned(), "sendable(F)", TypedCompletion::Incomplete, vec![], GoalState::Unresolved(GoalBlocker::CallableEnvironment)),
        ] {
            let graph = capture(&header, caller);
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert_eq!(report.completion(), expected, "{header}");
            assert_eq!(report.receipt().is_some(), expected == TypedCompletion::Checked);
            let bodies = report.bodies();
            let goal = bodies.goals().iter().find(|g| g.site.span.bytes().starts_with(b"keepPair(([") && g.source.span.bytes() == property.as_bytes()).unwrap();
            let observed: Vec<_> = if let Some(arrow) = &goal.arrow_origin {
                vec![arrow.site.span.bytes()]
            } else {
                goal.dependencies.iter().filter_map(|id| bodies.goal(id).unwrap().arrow_origin.as_ref().map(|a|a.site.span.bytes())).collect()
            };
            assert_eq!(observed, origins.iter().map(|s| s.as_bytes()).collect::<Vec<_>>());
            // Review ci-uaa1 S2-F7: the exact state, not "not Proved".
            assert_eq!(goal.state, state, "{header}");
        }

        // Both erased parameters have exactly the other slot's signature.
        for (erased, pair) in ["erased, () => 1i32", "() => 1i32, erased"]
            .into_iter()
            .enumerate()
        {
            let graph = capture(
                PAIR,
                &format!(
                    "
                function checkJobs(erased: () => i32): void {{ let _ = keepPair([{pair}]); }}
            "
                ),
            );
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert_eq!(report.completion(), TypedCompletion::Incomplete);
            assert!(report.receipt().is_none());
            let bodies = report.bodies();
            let goal = bodies
                .goals()
                .iter()
                .find(|g| g.site.span.bytes().starts_with(b"keepPair([") && g.selection.is_some())
                .unwrap();
            assert_eq!(
                goal.state,
                GoalState::Unresolved(GoalBlocker::CallableEnvironment)
            );
            let missing = bodies.goal(&goal.dependencies[2 + erased]).unwrap();
            let known = bodies.goal(&goal.dependencies[3 - erased]).unwrap();
            assert!(missing.arrow_origin.is_none());
            assert_eq!(missing.state, goal.state);
            assert_eq!(known.state, GoalState::Proved);
            assert_eq!(
                known.arrow_origin.as_ref().unwrap().site.span.bytes(),
                b"() => 1i32"
            );
        }

        for (pair, result, finding, state) in [
            (
                "() => (&numbers)[0u64], () => 1i32",
                "bool",
                ScalarFindingKind::MissingRequirement,
                GoalState::Disproved,
            ),
            (
                "() => 1i32, () => (&numbers)[0u64]",
                "i32",
                ScalarFindingKind::TypeMismatch,
                GoalState::Proved,
            ),
        ] {
            // Explicit input context fixes bracket category even with the catchall.
            let graph = capture(
                COMPETITION,
                &format!(
                    "
                function checkJobs(numbers: List<i32>): {result} {{
                    return choosePair<[() => i32, () => i32]>([{pair}]);
                }}
            "
                ),
            );
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert_eq!(report.completion(), TypedCompletion::SourceRejected);
            assert!(report.receipt().is_none());
            let bodies = report.bodies();
            let app = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let node = app
                .syntax
                .nodes
                .iter()
                .position(|n| {
                    n.kind == NodeKind::Call && n.span.bytes().starts_with(b"choosePair<")
                })
                .unwrap();
            let call = bodies.ordinary_call(app, node).unwrap().unwrap();
            assert_eq!(call.candidates.len(), 2);
            assert!(!call.checked);
            let goal = bodies.goal(call.goal.as_ref().unwrap()).unwrap();
            assert_eq!(goal.state, state);
            let winner = goal.selection.as_ref().unwrap().implementation;
            assert!(
                app.declarations.iter().any(|d| &d.id == winner
                    && d.span.bytes().starts_with(b"implement<F, G> choosePair"))
            );
            assert!(
                bodies
                    .findings()
                    .iter()
                    .any(|f| f.site.node == node && f.kind == finding)
            );
        }

        let graph = capture(
            PAIR,
            "function checkJobs(): void {
            let _ = keepPair([() => { Result.Ok(true); return 1i32; }, () => 2i32]);
        }",
        );
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert_eq!(report.completion(), TypedCompletion::SourceRejected);
        assert!(report.receipt().is_none());
        assert!(report.result_use().unwrap().discards().iter().any(|w| {
            w.code() == "R0103"
                && w.origin.span.bytes() == b"Result.Ok(true)"
                && w.origin.module.origin == Origin::Application
        }));
    }
}

const INVOKE_PAIR: &str = r#"
    function runTwo<W, R>(jobs: W): R;
    implement<F, G, A, B> runTwo<[F, G], [A, B]>(jobs: [F, G]): [A, B]
    where callableOnce(F, [], A), callableOnce(G, [], B),
          sendable(F), sendable(G), sendable(A), sendable(B) {
        return [jobs[0](), jobs[1]()];
    }
"#;

fn installed_pair_invocation(request: &InstalledRequest) {
    use compiler::analysis::Stage;
    use compiler::analysis::generics::{GoalState, callables::CaptureForm};
    use compiler::analysis::ownership::{Completion, lower_body};
    use compiler::analysis::typed::ResultUseBlocker;

    let baseline = include_str!("../../../tests/installed/typed/main.rill");
    let checks = include_str!("../../../tests/installed/typed/checks.rill");
    fs::write(&request.entry, baseline).unwrap();
    fs::write(request.entry.with_file_name("checks.rill"), checks).unwrap();
    for test_mode in [false, true] {
        let graph = loader::load_installed(InstalledRequest {
            test_mode,
            ..request.clone()
        })
        .unwrap();
        let mut captured: Vec<_> = graph
            .modules()
            .iter()
            .filter(|module| module.key.origin == Origin::Standard)
            .map(|module| module.key.path.as_str())
            .collect();
        let mut pins: Vec<_> = include_str!("../../../tests/installed/typed/stdlib-sources.txt")
            .lines()
            .collect();
        captured.sort_unstable();
        pins.sort_unstable();
        assert_eq!(captured, pins);
        // Baseline before two-child Result-use: no pair candidate to discover.
        let baseline = crate::limit::measured(
            "analyze_typed",
            "baseline",
            AnalysisLimits {
                max_work: 507593,
                max_entries: 54774,
                max_depth: 12,
            },
            |limits| analyze_typed(&graph, limits),
            |report| report.statistics(),
        );
        assert!(baseline.receipt().is_some());
    }
    fs::write(
        request.entry.with_file_name("checks.rill"),
        format!(
            "{}\n{}",
            checks,
            INVOKE_PAIR
                .replace("function runTwo", "export function runTwo")
                .replace("[jobs[0](), jobs[1]()]", "([(jobs[0]()), jobs[1]()])")
        ),
    )
    .unwrap();
    let source = format!(
        "{baseline}\n{}",
        r#"
        import checks::runTwo as join;
        function mixed(left: List<bool>, right: List<i32>): [[bool, i32], i32] {
            return join([(): [bool, i32] => [(&left)[(0u64)], 17i32], () => { return (&right)[0u64]; }]);
        }
        function shared(input: List<i32>): [i32, i32] {
            let numbers = input;
            let [first, second] = join([() => (&numbers)[0u64], () => (&numbers)[1u64]]);
            dispose(numbers);
            return [first, second];
        }
        function nestedParts(): [[[bool, i32], bool], bool] {
            return join([(): [[bool, i32], bool] => [[true, 1i32], false], () => true]);
        }
        function residences(): void {
            let owed: Result<bool> = Result.Ok(true);
            let [parts, no] = join([(): [bool, i32] => [true, 17i32], () => false]);
            if (parts[0] && !no) { let _ = owed; }
        }
    "#
    );
    fs::write(&request.entry, source).unwrap();
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
            .find(|m| m.key.path == "main.rill")
            .unwrap();
        let node = app
            .syntax
            .nodes
            .iter()
            .position(|n| {
                n.span
                    .bytes()
                    .starts_with(b"join([(): [bool, i32] => [(&left)")
            })
            .unwrap();
        let call = report.bodies().ordinary_call(app, node).unwrap().unwrap();
        assert!(call.checked && call.premise.is_none());
        let goal = report.bodies().goal(call.goal.as_ref().unwrap()).unwrap();
        assert_eq!(goal.state, GoalState::Proved);
        assert_eq!(
            goal.selection.as_ref().unwrap().implementation.module.path,
            "checks.rill"
        );
        assert_eq!(goal.dependencies.len(), 6);
        let mut origins = Vec::new();
        for id in &goal.dependencies {
            let goal = report.bodies().goal(id).unwrap();
            assert_eq!(goal.state, GoalState::Proved);
            assert!(goal.premise.is_none());
            if let Some(arrow) = &goal.arrow_origin {
                let [capture] = arrow.captures.as_slice() else {
                    panic!("one actual capture")
                };
                assert_eq!(capture.form, CaptureForm::ExplicitBorrow { mutable: false });
                origins.push((arrow.site.node, capture.site.span.bytes()));
            }
        }
        assert_eq!(origins.len(), 2);
        assert_ne!(origins[0].0, origins[1].0);
        assert_eq!(
            [origins[0].1, origins[1].1],
            [b"&left".as_slice(), b"&right"]
        );
        let uses = report.result_use().unwrap();
        let pending: Vec<_> = uses
            .pending()
            .iter()
            .map(|p| (p.blocker, p.site.span.bytes().escape_ascii().to_string()))
            .collect();
        assert_eq!(report.completion(), TypedCompletion::Checked, "{pending:?}");
        assert!(uses.discards().is_empty());
        let receipt = report.receipt().unwrap();
        assert_eq!(
            receipt.not_performed(),
            &[Stage::Ownership, Stage::Execution]
        );
        for name in ["mixed", "shared"] {
            let user = app
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some(name))
                .unwrap();
            let (limits, expected) = if name == "shared" {
                (AnalysisLimits::default(), Completion::Checked)
            } else {
                // The mixed output remains unsupported at the old pending cap.
                (
                    AnalysisLimits {
                        max_work: 2472,
                        max_entries: 300,
                        max_depth: 0,
                    },
                    Completion::Incomplete,
                )
            };
            let local = lower_body(&receipt, app, &user.id, limits).unwrap();
            assert_eq!(local.completion(), expected);
            assert_eq!(local.mir().is_some(), expected == Completion::Checked);
        }
        let s = receipt.statistics();
        let exact = AnalysisLimits {
            max_work: s.work,
            max_entries: s.entries,
            max_depth: s.max_depth,
        };
        assert!(analyze_typed(&graph, exact).receipt().is_some());
        for limit in [
            AnalysisLimits {
                max_work: s.work - 1,
                ..exact
            },
            AnalysisLimits {
                max_entries: s.entries - 1,
                ..exact
            },
            AnalysisLimits {
                max_depth: s.max_depth - 1,
                ..exact
            },
        ] {
            limit::assert_typed_limit(&analyze_typed(&graph, limit));
        }
    }

    for (source, completion, boundary, discard) in [
        ("function bodyWithoutCaller(): bool { return true; }", TypedCompletion::Checked, false, None),
        ("function exclusive(input: List<i32>): [i32, i32] {
            let mut numbers = input;
            return join([() => (&numbers)[0u64], () => (&mut numbers)[1u64]]);
        }", TypedCompletion::Incomplete, true, None),
        ("function erased(left: List<i32>, right: () => i32): [i32, i32] {
            return join([() => (&left)[0u64], right]);
        }", TypedCompletion::Incomplete, false, None),
        ("function escaping(input: List<i32>): [&i32, i32] {
            return join([() => { let child = 1i32; return &child; }, () => (&input)[0u64]]);
        }", TypedCompletion::Incomplete, true, None),
        // Transfer properties now prove, but the unsupported serial boundary
        // still reports only its provisional output, not the child occurrence.
        ("function nested(): void {
            let outputs = join([(): [Result<bool, i32>, i32] => [Result.Ok(true), 1i32], () => false]);
        }", TypedCompletion::Incomplete, true,
            Some(b"join([(): [Result<bool, i32>, i32] => [Result.Ok(true), 1i32], () => false])".as_slice())),
        ("function childOwes(): void {
            let _ = join([() => {
                let nested: [Result<bool>, i32] = [Result.Ok(true), 1i32];
                return 2i32;
            }, () => 3i32]);
        }", TypedCompletion::Incomplete, true, Some(b"Result.Ok(true)".as_slice())),
        ("function callerOwes(numbers: List<i32>): void {
            let owed: Result<bool> = Result.Ok(false);
            let _ = join([() => (&numbers)[0u64], () => (&numbers)[1u64]]);
        }", TypedCompletion::SourceRejected, false, Some(b"Result.Ok(false)".as_slice())),
    ] {
        fs::write(&request.entry, format!("{baseline}\nimport checks::runTwo as join;\n{source}")).unwrap();
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest { test_mode, ..request.clone() }).unwrap();
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert_eq!(report.completion(), completion, "{source}");
            assert_eq!(report.receipt().is_some(), completion == TypedCompletion::Checked);
            let uses = report.result_use().unwrap();
            let blocked_call = uses.pending().iter().any(|p| {
                p.blocker == ResultUseBlocker::CallBoundary && p.site.span.bytes().starts_with(b"join([")
            });
            assert_eq!(blocked_call, boundary, "{source}");
            let discards: Vec<_> = uses.discards().iter().map(|w| {
                assert_eq!(w.code(), "R0103");
                w.origin.span.bytes()
            }).collect();
            assert_eq!(discards, discard.as_slice(), "{source}");
        }
    }

    // A checked selection with the same written premises is not enough. The
    // selected implementation must actually route both invocation results.
    let reordered = INVOKE_PAIR
        .replace("[A, B]", "[B, A]")
        .replace("[jobs[0](), jobs[1]()]", "[jobs[1](), jobs[0]()]");
    fs::write(
        &request.entry,
        format!(
            "{baseline}\n{reordered}\n{}",
            "function flipped(): [i32, bool] { return runTwo([() => true, () => 1i32]); }",
        ),
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
            .find(|m| m.key.path == "main.rill")
            .unwrap();
        let node = app
            .syntax
            .nodes
            .iter()
            .position(|node| node.span.bytes() == b"runTwo([() => true, () => 1i32])")
            .unwrap();
        assert!(
            report
                .bodies()
                .ordinary_call(app, node)
                .unwrap()
                .unwrap()
                .checked
        );
        assert_eq!(report.completion(), TypedCompletion::Incomplete);
        assert!(report.receipt().is_none());
        let pending = report.result_use().unwrap().pending();
        for call in [b"jobs[1]()", b"jobs[0]()"] {
            assert!(pending.iter().any(
                |p| p.blocker == ResultUseBlocker::CallBoundary && p.site.span.bytes() == call
            ));
        }
    }
}

fn assert_pair_evidence(graph: &loader::Graph, different_signatures: bool) {
    use compiler::analysis::generics::{
        GoalFacts, GoalJudgment, GoalState, callables::CaptureForm,
    };
    use compiler::analysis::ordinary::RequirementTarget;
    use compiler::analysis::scalars::NodeFactKind;
    use compiler::analysis::{QueryError, Stage, TypeTerm};
    use compiler::loader::DeclarationKind;
    use compiler::parser::NodeKind;

    let report = analyze_typed(graph, AnalysisLimits::default());
    assert_eq!(report.completion(), TypedCompletion::Checked);
    let receipt = report.receipt().unwrap();
    assert_eq!(receipt.context(), graph.id());
    assert_eq!(
        receipt.not_performed(),
        &[Stage::Ownership, Stage::Execution]
    );
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.path == "main.rill")
        .unwrap();
    let family = app
        .declarations
        .iter()
        .find(|d| d.id.kind == DeclarationKind::Family && d.name.as_deref() == Some("keepPair"))
        .unwrap();
    let mut implementations = graph
        .implementations()
        .iter()
        .filter(|i| i.family == family.id);
    let implementation = implementations.next().unwrap();
    assert!(implementations.next().is_none());
    let node = app
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Call && n.span.bytes().starts_with(b"keepPair(["))
        .unwrap();
    let bodies = report.bodies();
    let ordinary = bodies.ordinary_call(app, node).unwrap().unwrap();
    assert!(ordinary.checked && ordinary.premise.is_none());
    assert_eq!(ordinary.family, &family.id);
    let goal = bodies.goal(ordinary.goal.as_ref().unwrap()).unwrap();
    assert_eq!(goal.state, GoalState::Proved);
    assert!(goal.premise.is_none() && ordinary.instantiation.is_none());
    let NodeFactKind::Expression(expr) = &bodies.node(app, node).unwrap().unwrap().kind else {
        panic!("expected checked call")
    };
    let call = expr.call.as_ref().unwrap();
    assert_eq!(call.callee, &family.id);
    let selection = call.selection.as_ref().unwrap();
    assert_eq!(selection.implementation, &implementation.declaration);
    let selected = goal.selection.as_ref().unwrap();
    assert_eq!(selected.implementation, selection.implementation);
    assert_eq!(selected.alternative, selection.alternative);
    let signature = &report
        .inventory()
        .family(&family.id)
        .unwrap()
        .unwrap()
        .implementations[0]
        .header;
    let [tuple] = call.arguments.as_slice() else {
        panic!("one tuple argument")
    };
    let [first, second] = app.syntax.nodes[*tuple].children.as_slice() else {
        panic!("two slots")
    };
    let check = |proof: &GoalFacts<'_>, predicate: &str| {
        assert_eq!(proof.state, GoalState::Proved);
        assert!(proof.premise.is_none());
        let expected = graph
            .prelude()
            .intrinsics
            .iter()
            .find(|id| id.name() == predicate)
            .unwrap();
        assert!(matches!(&proof.judgment, GoalJudgment::Requirement {
            target: RequirementTarget::Predicate(id), ..
        } if *id == expected));
    };
    assert_eq!(goal.dependencies.len(), 6);
    assert_ne!(goal.dependencies[2], goal.dependencies[3]);
    let TypeTerm::Tuple(inputs) = ordinary.inputs[0].as_deref().unwrap() else {
        panic!("owned tuple")
    };
    for (i, element) in [first, second].into_iter().enumerate() {
        let result = if i == 1 && different_signatures {
            "bool"
        } else {
            "i32"
        };
        let element_type = ["i32", "bool"][i];
        let origin = if i == 1 && different_signatures {
            assert_eq!(app.syntax.nodes[*element].span.bytes(), b"second");
            app.syntax
                .nodes
                .iter()
                .position(|n| n.span.bytes() == b"() => (&right)[0u64]")
                .unwrap()
        } else {
            *element
        };
        let arrow = bodies.arrow(app, origin).unwrap().unwrap();
        assert!(arrow.premise.is_none() && arrow.parameters.is_empty());
        assert!(matches!(arrow.result.as_ref(), TypeTerm::Primitive(id) if id.name() == result));
        let [capture] = arrow.captures.as_slice() else {
            panic!("one capture per arrow")
        };
        assert_eq!(capture.form, CaptureForm::ExplicitBorrow { mutable: false });
        assert_eq!(
            capture.site.span.bytes(),
            [b"&left".as_slice(), b"&right"][i]
        );
        assert_eq!(capture.path.root.context, graph.id());
        let storage = capture.storage.as_ref().unwrap();
        let TypeTerm::Reference {
            mutable: false,
            referent,
        } = storage.as_ref()
        else {
            panic!("shared capture storage")
        };
        let TypeTerm::Nominal {
            declaration,
            arguments,
        } = referent.as_ref()
        else {
            panic!("List referent")
        };
        assert_eq!(declaration.module.origin, Origin::Standard);
        assert_eq!(declaration.module.path, "collection.rill");
        assert!(
            matches!(arguments.as_slice(), [ty] if matches!(ty.as_ref(), TypeTerm::Primitive(id) if id.name() == element_type))
        );

        let invoke = bodies.goal(&goal.dependencies[i]).unwrap();
        let send_job = bodies.goal(&goal.dependencies[2 + i]).unwrap();
        let send_result = bodies.goal(&goal.dependencies[4 + i]).unwrap();
        check(invoke, "callableOnce");
        check(send_job, "sendable");
        check(send_result, "sendable");
        assert!(send_result.arrow_origin.is_none());
        assert!(std::ptr::eq(
            send_job.arrow_origin.as_deref().unwrap(),
            arrow
        ));
        let job = &selection.substitution[i];
        let output = &selection.substitution[2 + i];
        assert_eq!(job.binder, &signature.binders[i].declaration.id);
        assert_eq!(output.binder, &signature.binders[2 + i].declaration.id);
        assert_eq!(inputs[i], job.argument);
        assert_eq!(
            send_job.source.span.bytes(),
            [b"sendable(F)", b"sendable(G)"][i]
        );
        assert!(
            matches!(&invoke.judgment, GoalJudgment::Requirement { arguments, .. }
            if matches!(arguments.as_slice(), [receiver, _, result] if receiver == &job.argument && result == &arrow.result))
        );
        assert!(
            matches!(&send_job.judgment, GoalJudgment::Requirement { arguments, .. }
            if arguments == std::slice::from_ref(&job.argument))
        );
        assert!(
            matches!(&send_result.judgment, GoalJudgment::Requirement { arguments, .. }
            if output.argument == arrow.result && arguments == std::slice::from_ref(&arrow.result))
        );

        let mut parent = send_job;
        for (predicate, ty) in [
            ("sendable", storage),
            ("shareable", referent),
            ("shareable", &arguments[0]),
        ] {
            let [child] = parent.dependencies.as_slice() else {
                panic!("one property dependency")
            };
            parent = bodies.goal(child).unwrap();
            check(parent, predicate);
            assert!(
                matches!(&parent.judgment, GoalJudgment::Requirement { arguments, .. }
                if arguments == std::slice::from_ref(ty))
            );
            if predicate == "sendable" {
                assert_eq!(
                    (parent.source.module, parent.source.node),
                    (capture.site.module, capture.site.node)
                );
            }
        }
    }

    let statistics = receipt.statistics();
    let exact = AnalysisLimits {
        max_work: statistics.work,
        max_entries: statistics.entries,
        max_depth: statistics.max_depth,
    };
    let replay = analyze_typed(graph, exact);
    assert!(replay.receipt().is_some());
    assert!(matches!(
        replay.bodies().goal(&goal.id),
        Err(QueryError::InvalidRequest)
    ));
    for short in [
        AnalysisLimits {
            max_work: exact.max_work - 1,
            ..exact
        },
        AnalysisLimits {
            max_entries: exact.max_entries - 1,
            ..exact
        },
        AnalysisLimits {
            max_depth: exact.max_depth - 1,
            ..exact
        },
    ] {
        limit::assert_typed_limit(&analyze_typed(graph, short));
    }
}

fn partial_collection(request: &InstalledRequest) {
    fs::write(
        &request.entry,
        "
        import std::shared::Shared;
        implement copy(value: &Missing): Missing { }
    ",
    )
    .unwrap();
    let graph = loader::load_installed(request.clone()).unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let scope = |action| {
        report
            .ownership_protection()
            .iter()
            .find(|p| p.rule == OwnershipRule::Required(action))
            .unwrap()
    };
    let copy = scope(NativeOwnershipAction::SharedRetain);
    assert!(copy.provider.is_some());
    assert_eq!(
        copy.coverage,
        OwnershipCoverage::Pending,
        "unresolved candidate headers cannot be omitted"
    );
    assert_eq!(
        scope(NativeOwnershipAction::SharedRelease).coverage,
        OwnershipCoverage::Clear
    );
    assert!(report.ownership_overlaps().is_empty());

    fs::write(
        &request.entry,
        "
        import std::shared::Shared;
        implement<T> dispose(value: T): void { }
        type SharedAliasNamesake<T> = { value: T; }
        implement<T> dispose(value: SharedAliasNamesake<T>): void { }
    ",
    )
    .unwrap();
    let graph = loader::load_installed(request.clone()).unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert!(
        report.ownership_overlaps().is_empty(),
        "a less-specific input cannot displace the provider"
    );
    assert!(
        report
            .ownership_protection()
            .iter()
            .all(|p| p.coverage == OwnershipCoverage::Clear)
    );
    let exact = AnalysisLimits {
        max_work: report.statistics().work,
        ..AnalysisLimits::default()
    };
    let first_short = AnalysisLimits {
        max_work: exact.max_work - 1,
        ..exact
    };
    assert_eq!(
        analyze_typed(&graph, exact).completion(),
        TypedCompletion::Incomplete
    );
    limit::assert_typed_limit(&analyze_typed(&graph, first_short));
}

struct Cleanup<'a>(&'a Path);
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
    }
}
