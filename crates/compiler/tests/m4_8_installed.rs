//! Exercises installed capture with production core sources and no trust override.
use compiler::analysis::AnalysisLimits;
use compiler::analysis::typed::{CoreBlocker, analyze_typed};
use compiler::loader::{self, FailureKind, InstalledRequest, Limits, Prelude, Request, Selector};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn installed_option_uses_pinned_sources_in_both_modes() {
    if let Some(root) = std::env::var_os("RILL_INSTALLED_PROBE_ROOT") {
        let root = PathBuf::from(root);
        let entry = root.join("app/main.rill");
        let mut options = BTreeMap::new();
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                entry: entry.clone(),
                target: "aarch64-apple-darwin".into(),
                options: options.clone(),
                test_mode,
                limits: Limits::default(),
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
            assert_eq!(
                report.core_blockers().len(),
                4,
                "other contracts are still missing"
            );
            assert!(report.bodies().findings().is_empty());
        }
        let select = |name| Selector {
            module: "core.rill".into(),
            name,
        };
        let untrusted = loader::load(Request {
            entry: entry.clone(),
            standard_root: root.join("lib/rill/std"),
            prelude: Prelude {
                copy: select("copy".into()),
                dispose: select("dispose".into()),
            },
            target: "aarch64-apple-darwin".into(),
            options: options.clone(),
            test_mode: false,
            limits: Limits::default(),
        })
        .unwrap();
        let report = analyze_typed(&untrusted, AnalysisLimits::default());
        assert!(report.option_role().is_none() && report.option_inspection().is_none());
        assert!(report.core_blockers().contains(&CoreBlocker::Roles));
        let mut request = InstalledRequest {
            entry,
            target: "aarch64-apple-darwin".into(),
            options: BTreeMap::new(),
            test_mode: false,
            limits: Limits::default(),
        };
        options.insert("unsupported".into(), "1".into());
        request.options = options;
        let unknown = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&unknown, AnalysisLimits::default());
        assert!(report.option_role().is_none() && report.option_inspection().is_none());
        request.options.clear();
        fs::write(
            root.join("lib/rill/std/option.rill"),
            b"!unparseable pinned source\n",
        )
        .unwrap();
        let rejected = loader::load_installed(request).unwrap_err();
        assert_eq!(rejected.kind, FailureKind::InvalidRequest);
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
        "import std::option::{Option, isSome}; expect isSome(&Option<bool>.Some(true));",
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

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
