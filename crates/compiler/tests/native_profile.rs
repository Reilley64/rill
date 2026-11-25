mod support;

use compiler::analysis::AnalysisLimits;
use compiler::analysis::typed::{TypedCompletion, analyze_typed};
use compiler::loader::{self, InstalledRequest};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const HOST: &str = env!("RILL_ANALYSIS_TARGET");
const NATIVE_TARGETS: [&str; 4] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
];

#[test]
fn installed_program_gets_a_typed_receipt_on_the_host_target() {
    installed(
        "installed_program_gets_a_typed_receipt_on_the_host_target",
        |app, root| {
            for test_mode in [false, true] {
                let request = request(app, HOST, test_mode);
                let graph = loader::load_installed(request).unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                assert_eq!(report.completion(), TypedCompletion::Checked, "{root:?}");
                assert!(report.receipt().is_some(), "test_mode {test_mode}");
            }
        },
    );
}

#[test]
fn installed_profile_is_the_host_only() {
    installed("installed_profile_is_the_host_only", |app, _| {
        let elsewhere = NATIVE_TARGETS
            .into_iter()
            .filter(|target| *target != HOST)
            .chain(["x86_64-pc-windows-msvc"]);
        for target in elsewhere {
            let graph = loader::load_installed(request(app, target, false)).unwrap();
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert!(report.receipt().is_none(), "{target}");
        }
    });
}

#[test]
fn host_is_one_of_the_four_native_targets() {
    assert!(NATIVE_TARGETS.contains(&HOST), "{HOST}");
}

fn request(app: &Path, target: &str, test_mode: bool) -> InstalledRequest {
    InstalledRequest {
        entry: app.join("main.rill"),
        target: target.into(),
        options: Default::default(),
        test_mode,
        limits: Default::default(),
    }
}

fn installed(test: &str, check: impl FnOnce(&Path, &Path)) {
    if let Some(root) = std::env::var_os("RILL_NATIVE_PROFILE_ROOT") {
        let root = PathBuf::from(root);
        check(&root.join("app"), &root);
        return;
    }
    let root =
        std::env::temp_dir().join(format!("rill-native-profile-{test}-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _cleanup = Cleanup(&root);
    let runtime = root.join("lib/rill/runtime").join(HOST);
    let stdlib = root.join("lib/rill/std");
    for path in [&root.join("bin"), &root.join("app"), &stdlib, &runtime] {
        fs::create_dir_all(path).unwrap();
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in include_str!("../../../tests/installed/typed/stdlib-sources.txt")
        .lines()
        .chain(include_str!("../../../tests/installed/typed/routine-sources.txt").lines())
    {
        fs::copy(repo.join("stdlib").join(name), stdlib.join(name)).unwrap();
    }
    for name in ["main.rill", "checks.rill"] {
        fs::copy(
            repo.join("tests/installed/typed").join(name),
            root.join("app").join(name),
        )
        .unwrap();
    }
    fs::copy(
        env!("RILL_OWNERSHIP_ARCHIVE"),
        runtime.join("librill_ownership.rlib"),
    )
    .unwrap();
    fs::copy(
        repo.join("runtime/error/src/lib.rs"),
        runtime.join("error.rs"),
    )
    .unwrap();
    let output = support::run_copied_probe(
        Command::new(root.join("bin/native-profile-probe"))
            .args(["--exact", test, "--nocapture"])
            .env("RILL_NATIVE_PROFILE_ROOT", &root)
            .current_dir(root.join("app")),
    )
    .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct Cleanup<'a>(&'a Path);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
    }
}
