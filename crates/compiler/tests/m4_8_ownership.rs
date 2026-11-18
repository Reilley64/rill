//! Installed ownership evidence is capture-bound and does not depend on application aliases.
#![cfg(all(target_arch = "aarch64", target_os = "macos"))]

use compiler::analysis::AnalysisLimits;
use compiler::analysis::typed::{
    CoreBlocker, NativeOwnershipAction, OwnershipCoverage, OwnershipRule, analyze_typed,
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
            let [overlap] = report.ownership_overlaps() else {
                panic!(
                    "expected only the nested-payload replacement: {:?}",
                    report.ownership_overlaps()
                );
            };
            assert_eq!(overlap.code(), "R0500");
            assert_eq!(overlap.rule, copy.rule);
            assert_eq!(overlap.implementation.id.module.origin, Origin::Application);
            let release = report
                .ownership_protection()
                .iter()
                .find(|p| p.rule == OwnershipRule::Required(NativeOwnershipAction::SharedRelease))
                .unwrap();
            assert_eq!(release.coverage, OwnershipCoverage::Clear);
        }

        guard_and_endpoint_regions(&request);
        missing_evidence(&root, &request);
        partial_collection(&request);
        combined_installed_capture(&request);

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
    for module in [
        "core.rill",
        "shared.rill",
        "mutex.rill",
        "channel.rill",
        "option.rill",
        "result.rill",
        "error.rill",
        "collection.rill",
        "iter.rill",
        "format.rill",
        "convert.rill",
    ] {
        fs::copy(source.join(module), root.join("lib/rill/std").join(module)).unwrap();
    }
    fs::copy(env!("RILL_OWNERSHIP_ARCHIVE"), root.join(ARCHIVE)).unwrap();
    fs::copy(
        source.join("../runtime/error.rs"),
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

fn combined_installed_capture(request: &InstalledRequest) {
    use compiler::analysis::scalars::{BodyOwner, ScalarFailureKind};
    fs::write(
        &request.entry,
        r#"
        import std::shared::Shared as Counted;
        import std::result::{Result as Outcome, isOk};
        import std::convert::into;
        import std::collection::List;
        function convertAndShow(value: u8): Outcome<string> {
            let xs: List<i32> = [into<i32>(value)];
            return Outcome.Ok(`${xs[0u64]}`);
        }
        function iterateAll(xs: List<i32>): void { for (let _ of xs) { continue; } }
        function retainType(value: &Counted<i32>): void { }
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
        assert!(report.result_use().is_some());
        assert!(report.show_role().is_some());
        assert!(report.into_role().is_some());
        assert!(report.list_role().is_some());
        assert!(
            report
                .ownership_protection()
                .iter()
                .any(|scope| scope.provider.is_some())
        );
        assert_eq!(report.completion(), ScalarFailureKind::Incomplete);
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
    use compiler::analysis::scalars::ScalarFailureKind::{Incomplete, ResourceLimit};
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
