use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

const OPERATIONS: [&str; 4] = ["build", "check", "run", "test"];
#[cfg(feature = "llvm")]
const UNAVAILABLE: [&str; 1] = ["test"];
#[cfg(not(feature = "llvm"))]
const UNAVAILABLE: [&str; 3] = ["build", "run", "test"];
#[cfg(feature = "llvm")]
const SINGLE_ENTRY: [&str; 2] = ["check", "test"];
#[cfg(not(feature = "llvm"))]
const SINGLE_ENTRY: [&str; 4] = OPERATIONS;

fn rill(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rill"))
        .args(args)
        .output()
        .expect("run the rill executable")
}

fn assert_failure(output: &Output, message: &str) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = std::str::from_utf8(&output.stderr).expect("UTF-8 error message");
    assert!(stderr.starts_with("rill: "), "{stderr}");
    assert!(stderr.contains(message), "{stderr}");
}

fn assert_unavailable(output: &Output, command: &str) {
    assert_failure(
        output,
        &format!("'{command}' is not implemented in this scaffold"),
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("no compiler operation or LSP server was started")
    );
}

#[test]
fn help_is_real_and_describes_scaffold_status() {
    for spelling in ["help", "--help", "-h"] {
        let output = rill(&[spelling]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let help = std::str::from_utf8(&output.stdout).expect("UTF-8 help");
        assert!(help.contains("Usage: rill"));
        for command in ["build", "check", "run", "test", "lsp"] {
            assert!(help.contains(command), "{help}");
        }
        assert!(help.contains("unavailable\ncommands exit 2"));
        assert!(help.contains("limitation, not a compiler check/build/test outcome"));
        assert!(help.contains("do not open\nentry paths"));
        assert!(help.contains("check exits 0 for a compilable program, 1 for a"));
    }
}

#[test]
fn version_uses_the_package_version() {
    for spelling in ["version", "--version", "-V"] {
        let output = rill(&[spelling]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        assert_eq!(
            output.stdout,
            format!("rill {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
        );
    }
}

#[test]
fn every_unavailable_command_fails_without_fake_output() {
    for command in UNAVAILABLE.into_iter().chain(["lsp"]) {
        assert_unavailable(&rill(&[command]), command);
    }
}

#[test]
fn entry_paths_are_not_resolved_or_required_to_be_readable_sources() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let missing = crate_dir.join("tests/source-that-does-not-exist.rill");
    assert!(!missing.exists());

    // A missing path, a directory, and Rust test source all reach the same
    // unavailability error: none is loaded or validated as a Rill source file.
    let test_source = crate_dir.join("tests/cli.rs");
    for command in UNAVAILABLE {
        for entry in [missing.as_path(), crate_dir, test_source.as_path()] {
            let output = Command::new(env!("CARGO_BIN_EXE_rill"))
                .arg(command)
                .arg(entry)
                .output()
                .expect("run with an unopened entry path");
            assert_unavailable(&output, command);
        }
    }
}

#[cfg(unix)]
#[test]
fn source_and_lsp_input_are_not_read() {
    use std::process::Stdio;
    use std::thread;
    use std::time::{Duration, Instant};

    for command in UNAVAILABLE.into_iter().chain(["lsp"]) {
        let mut process = Command::new(env!("CARGO_BIN_EXE_rill"));
        process.arg(command);
        if command != "lsp" {
            process.arg("/dev/stdin");
        }
        let mut child = process
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run with an open, empty input pipe");
        let deadline = Instant::now() + Duration::from_secs(5);
        // Keep stdin open with no bytes: reading source or awaiting protocol
        // input would block instead of immediately reporting unavailability.
        while child.try_wait().expect("check child status").is_none() {
            if Instant::now() >= deadline {
                child.kill().expect("stop blocked child");
                child.wait().expect("reap blocked child");
                panic!("{command} waited for input instead of failing");
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_unavailable(&child.wait_with_output().expect("collect output"), command);
    }
}

#[test]
fn check_needs_an_entry_path() {
    assert_failure(&rill(&["check"]), "check expects an entry path");
}

#[test]
fn unknown_and_missing_commands_are_invocation_errors() {
    assert_failure(&rill(&[]), "expected a command");
    for command in ["unknown", "--unknown", "explain", "", "unknown\ncommand"] {
        assert_failure(&rill(&[command]), "unknown command");
    }
}

#[test]
fn extra_arguments_and_unsupported_options_are_rejected() {
    for command in ["help", "--help", "-h", "version", "--version", "-V"] {
        assert_failure(&rill(&[command, "extra"]), "additional arguments");
    }
    for command in SINGLE_ENTRY {
        assert_failure(
            &rill(&[command, "entry.rill", "extra"]),
            "at most one entry path",
        );
    }
    for command in OPERATIONS {
        for option in ["--unknown", "--help", "--report-format", "-"] {
            assert_failure(&rill(&[command, option]), "options are not supported");
        }
    }
    assert_failure(&rill(&["lsp", "extra"]), "lsp does not accept arguments");
}

fn assert_invalid_encoding(arg: &OsStr) {
    for prefix in [None, Some("check"), Some("--help")] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rill"));
        if let Some(prefix) = prefix {
            command.arg(prefix);
        }
        let output = command.arg(arg).output().expect("run with non-UTF-8 input");
        assert_failure(&output, "arguments must be valid UTF-8");
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_arguments_are_rejected_without_panicking() {
    use std::os::unix::ffi::OsStrExt;

    assert_invalid_encoding(OsStr::from_bytes(b"invalid-\xff"));
}

#[cfg(windows)]
#[test]
fn non_unicode_arguments_are_rejected_without_panicking() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    assert_invalid_encoding(&OsString::from_wide(&[0xd800]));
}
