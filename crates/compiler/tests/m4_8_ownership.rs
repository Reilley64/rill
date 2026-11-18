//! Installed ownership evidence is capture-bound and does not depend on application aliases.
#![cfg(all(target_arch = "aarch64", target_os = "macos"))]

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

const ARCHIVE: &str = "lib/rill/runtime/aarch64-apple-darwin/librill_ownership.rlib";

#[test]
fn installed_ownership_requires_the_built_archive() {
    if let Some(root) = std::env::var_os("RILL_OWNERSHIP_PROBE_ROOT") {
        let root = PathBuf::from(root);
        let request = InstalledRequest {
            entry: root.join("app/main.rill"),
            target: "aarch64-apple-darwin".into(),
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
            assert!(
                report
                    .core_blockers()
                    .contains(&CoreBlocker::Representation)
            );
            assert!(report.core_blockers().contains(&CoreBlocker::Roles));
            let [overlap] = report.ownership_overlaps() else {
                panic!(
                    "expected only the nested-payload replacement: {:?}",
                    report.ownership_overlaps()
                );
            };
            assert_eq!(overlap.code(), "R0500");
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
            let [storage] = report.native_representations() else {
                panic!("expected one Shared representation");
            };
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

        let facade = root.join("lib/rill/std/std.rill");
        let bytes = fs::read(&facade).unwrap();
        fs::write(&facade, [&bytes[..], b"\n// changed pin"].concat()).unwrap();
        assert_eq!(
            loader::load_installed(request.clone()).unwrap_err().kind,
            FailureKind::InvalidRequest
        );
        fs::write(facade, bytes).unwrap();
        let graph = loader::load_installed(request.clone()).unwrap();
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
        let graph = loader::load_installed(request).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert!(
            report.ownership_protection().is_empty(),
            "an unrelated same-spelled source must not request the archive"
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
        "lib/rill/runtime/aarch64-apple-darwin",
    ] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    let executable = root.join("bin/m48-ownership-probe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
    for module in include_str!("../../../tests/installed/typed/stdlib-sources.txt").lines() {
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
        assert_eq!(protected.len(), 2);
        for scope in protected {
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
        assert_eq!(
            short.completion(),
            compiler::analysis::typed::TypedCompletion::ResourceLimit
        );
        assert!(short.core_blockers().contains(&CoreBlocker::Roles));

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
        assert_eq!(exhausted.completion(), TypedCompletion::ResourceLimit);
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
        assert_eq!(short.completion(), TypedCompletion::ResourceLimit);
        assert!(short.receipt().is_none());

        fs::write(
            &request.entry,
            format!("{MAIN}\nfunction unused(input: Result<bool>.Ok): void {{}}\n"),
        )
        .unwrap();
        let rejected = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&rejected, AnalysisLimits::default());
        assert_eq!(report.completion(), TypedCompletion::SourceRejected);
        assert!(report.result_use().unwrap().discards().iter().any(|w| {
            w.code() == "R0103" && w.origin.span.bytes() == b"input: Result<bool>.Ok"
        }));
        assert!(report.receipt().is_none());

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
    use compiler::analysis::typed::TypedCompletion::{Incomplete, ResourceLimit};
    assert_eq!(analyze_typed(&graph, exact).completion(), Incomplete);
    assert_eq!(
        analyze_typed(&graph, first_short).completion(),
        ResourceLimit
    );
}

struct Cleanup<'a>(&'a Path);
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
    }
}
