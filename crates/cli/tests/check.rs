use std::fs;
use std::process::{Command, Output};

mod support;

use support::Installation;

impl Installation {
    fn check(&self, source: &str) -> Output {
        let entry = self.root.join("app/main.rill");
        fs::write(&entry, source).unwrap();
        self.rill(&["check", entry.to_str().unwrap()])
    }

    fn rill(&self, args: &[&str]) -> Output {
        support::run(
            Command::new(self.root.join("bin/rill"))
                .args(args)
                .current_dir(self.root.join("app")),
        )
        .expect("run the staged rill executable")
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("UTF-8 report")
}

const READY: &str = "function ready(text: &string): void {}\n";
const REUSE: &str =
    "function reuse(value: string): bool { let taken = value; let again = value; return true; }\n";
const REUSE_AGAIN: &str =
    "function reuseAgain(item: string): bool { let taken = item; let again = item; return true; }\n";
const MINUS: &str = "function minus(a: i32, b: i32): i32 { return a - b; }\n";

#[test]
fn a_clean_program_exits_zero_with_no_report() {
    let installation = Installation::stage("clean");
    let output = installation.check(&format!("{READY}ready(&\"x\");\n"));
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{}", stderr(&output));
}

#[test]
fn a_program_that_is_compilable_while_other_code_is_unsupported_exits_zero() {
    let installation = Installation::stage("unreachable-unsupported");
    let output = installation.check(&format!("{READY}{MINUS}ready(&\"x\");\n"));
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(output.stderr.is_empty(), "{}", stderr(&output));
}

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

#[test]
fn a_missing_import_exits_one_with_its_code() {
    let installation = Installation::stage("missing-import");
    let output = installation.check("import nowhere::thing;\n");
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("R0400"), "{}", stderr(&output));
}

#[test]
fn a_lexical_error_exits_one_with_its_code() {
    let installation = Installation::stage("lexical-error");
    let output = installation.check("let s = \"abc;\n");
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("R0001"), "{}", stderr(&output));
}

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

#[test]
fn a_shape_typed_analysis_cannot_check_exits_one_with_r0605_and_its_span() {
    let installation = Installation::stage("typed-unsupported");
    let output = installation.check("let flag: bool = true;\nlet n: i32 = flag;\n");
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let report = stderr(&output);
    assert!(report.contains("R0605"), "{report}");
    assert!(report.contains("main.rill:2:1"), "{report}");
}

#[test]
fn an_unsupported_shape_exits_one_with_r0605_and_its_span() {
    let installation = Installation::stage("unsupported-shape");
    let output = installation.check(&format!(
        "{READY}{MINUS}ready(&\"x\");\nlet n = minus(3i32, 1i32);\n"
    ));
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "{output:?}");
    let report = stderr(&output);
    assert!(report.contains("R0605"), "{report}");
    assert!(report.contains("main.rill:2:"), "{report}");
    assert!(report.contains("a - b"), "{report}");
}

#[test]
fn a_shape_the_backend_does_not_lower_exits_one_with_r0605_and_its_exact_report() {
    let installation = Installation::stage("backend-shape");
    let output = installation.check("let flag = true;\n");
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "{output:?}");
    let entry = fs::canonicalize(installation.root.join("app/main.rill")).unwrap();
    assert_eq!(
        stderr(&output),
        format!(
            "error[R0605]: valid source whose shape the compiler cannot compile yet\n --> {}:1:12\n  |\n1 | let flag = true;\n  |            ^^^^\n",
            entry.display()
        )
    );
}

#[test]
fn every_ownership_finding_is_reported_in_source_order() {
    let installation = Installation::stage("every-finding");
    let output = installation.check(&format!("{READY}{REUSE}{REUSE_AGAIN}ready(&\"x\");\n"));
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "{output:?}");
    let entry = fs::canonicalize(installation.root.join("app/main.rill")).unwrap();
    let entry = entry.display();
    let report = |line: usize, source: &str, column: usize, name: &str| {
        format!(
            "error[R0200]: value used after it was moved\n --> {entry}:{line}:{column}\n  |\n{line} | {source}\n  | {}{}\n",
            " ".repeat(column - 1),
            "^".repeat(name.len())
        )
    };
    assert_eq!(
        stderr(&output),
        format!(
            "{}{}",
            report(2, REUSE.trim_end(), 70, "value"),
            report(3, REUSE_AGAIN.trim_end(), 73, "item")
        )
    );
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
    let output =
        support::run(Command::new(env!("CARGO_BIN_EXE_rill")).args(["check", manifest])).unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        stderr(&output).contains("installed bin directory"),
        "{}",
        stderr(&output)
    );
}
