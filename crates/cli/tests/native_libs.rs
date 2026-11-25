#![cfg(feature = "llvm")]

#[path = "../src/native_libs.rs"]
mod native_libs;

use std::path::Path;
use std::process::Command;

#[test]
fn the_link_line_names_what_the_runtime_archive_needs() {
    let target_directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-static-libs");
    let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args([
            "rustc",
            "--locked",
            "-p",
            "rill-runtime",
            "--lib",
            "--target-dir",
        ])
        .arg(&target_directory)
        .args(["--", "--print", "native-static-libs"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run cargo rustc");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    let reported: Vec<&str> = stderr
        .lines()
        .find_map(|line| line.trim().strip_prefix("note: native-static-libs:"))
        .unwrap_or_else(|| panic!("rustc printed no native-static-libs line:\n{stderr}"))
        .split_whitespace()
        .collect();
    assert_eq!(native_libs::NATIVE_LIBS, reported.as_slice());
}
