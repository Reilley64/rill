use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TARGET: &str = env!("RILL_HOST_TARGET");

struct Installation {
    root: PathBuf,
}

impl Installation {
    fn stage(test: &str) -> Self {
        let root = std::env::temp_dir().join(format!("rill-cli-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let std = root.join("lib/rill/std");
        let runtime = root.join("lib/rill/runtime").join(TARGET);
        for directory in [&root.join("bin"), &root.join("app"), &std, &runtime] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::copy(env!("CARGO_BIN_EXE_rill"), root.join("bin/rill")).unwrap();
        for name in include_str!("../../../tests/installed/typed/stdlib-sources.txt")
            .lines()
            .chain(include_str!("../../../tests/installed/typed/routine-sources.txt").lines())
        {
            fs::copy(repo.join("stdlib").join(name), std.join(name)).unwrap();
        }
        fs::copy(
            repo.join("runtime/error/src/lib.rs"),
            runtime.join("error.rs"),
        )
        .unwrap();
        fs::write(
            runtime.join("librill_ownership.rlib"),
            compiler::loader::pinned_ownership_archive(),
        )
        .unwrap();
        Self { root }
    }

    fn check(&self, source: &str) -> Output {
        let entry = self.root.join("app/main.rill");
        fs::write(&entry, source).unwrap();
        self.rill(&["check", entry.to_str().unwrap()])
    }

    fn rill(&self, args: &[&str]) -> Output {
        Command::new(self.root.join("bin/rill"))
            .args(args)
            .current_dir(self.root.join("app"))
            .output()
            .expect("run the staged rill executable")
    }
}

impl Drop for Installation {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("UTF-8 report")
}

const READY: &str = "function ready(flag: bool): bool { return flag; }\n";
const MINUS: &str = "function minus(a: i32, b: i32): i32 { return a - b; }\n";

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn a_clean_program_exits_zero_with_no_report() {
    let installation = Installation::stage("clean");
    let output = installation.check(&format!("{READY}let ok = ready(true);\n"));
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{}", stderr(&output));
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn a_program_that_is_compilable_while_other_code_is_unsupported_exits_zero() {
    let installation = Installation::stage("unreachable-unsupported");
    let output = installation.check(&format!("{READY}{MINUS}let ok = ready(true);\n"));
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(output.stderr.is_empty(), "{}", stderr(&output));
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn a_type_error_exits_one_with_its_code_and_span() {
    let installation = Installation::stage("type-error");
    let output = installation.check("function f(): bool { return 1i32; }\n");
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "{output:?}");
    let report = stderr(&output);
    assert!(report.contains("R0100"), "{report}");
    assert!(report.contains("main.rill:1:29"), "{report}");
    assert!(report.contains("^^^^"), "{report}");
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn a_missing_import_exits_one_with_its_code() {
    let installation = Installation::stage("missing-import");
    let output = installation.check("import nowhere::thing;\n");
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("R0400"), "{}", stderr(&output));
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn a_lexical_error_exits_one_with_its_code() {
    let installation = Installation::stage("lexical-error");
    let output = installation.check("let s = \"abc;\n");
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("R0001"), "{}", stderr(&output));
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn a_syntax_error_exits_one() {
    let installation = Installation::stage("syntax-error");
    let output = installation.check("let = ;\n");
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("main.rill:1:5"),
        "{}",
        stderr(&output)
    );
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn a_shape_typed_analysis_cannot_check_exits_one_with_r0605_and_its_span() {
    let installation = Installation::stage("typed-unsupported");
    let output = installation.check("let flag: bool = true;\nlet n: i32 = flag;\n");
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let report = stderr(&output);
    assert!(report.contains("R0605"), "{report}");
    assert!(report.contains("main.rill:2:1"), "{report}");
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn an_unsupported_shape_exits_one_with_r0605_and_its_span() {
    let installation = Installation::stage("unsupported-shape");
    let output = installation.check(&format!(
        "{READY}{MINUS}let ok = ready(true);\nlet n = minus(3i32, 1i32);\n"
    ));
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "{output:?}");
    let report = stderr(&output);
    assert!(report.contains("R0605"), "{report}");
    assert!(report.contains("main.rill:2:"), "{report}");
    assert!(report.contains("a - b"), "{report}");
}

#[test]
fn a_missing_entry_file_exits_two() {
    let installation = Installation::stage("missing-file");
    let missing = installation.root.join("app/absent.rill");
    let output = installation.rill(&["check", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(stderr(&output).starts_with("rill: "), "{}", stderr(&output));
}

#[test]
fn a_bad_option_exits_two_before_anything_is_read() {
    let installation = Installation::stage("bad-option");
    let output = installation.rill(&["check", "--bogus"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("options are not supported"));
}

#[test]
fn a_binary_outside_an_installation_fails_without_a_pass() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let output = Command::new(env!("CARGO_BIN_EXE_rill"))
        .args(["check", manifest])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        stderr(&output).contains("installed bin directory"),
        "{}",
        stderr(&output)
    );
}
