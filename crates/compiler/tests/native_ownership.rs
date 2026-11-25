//! Exercise the same native archive used by installed capture, without Rill lowering.

use std::path::Path;
use std::{fs, process::Command};

#[test]
fn delivered_native_ownership_actions() {
    let artifact = env!("RILL_OWNERSHIP_ARCHIVE");
    let directory =
        std::env::temp_dir().join(format!("rill-native-ownership-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let _cleanup = Cleanup(&directory);
    let executable = directory.join("ownership-tests");
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/ownership/tests.rs");
    let compile = Command::new(env!("RILL_OWNERSHIP_RUSTC"))
        .args(["--edition=2024", "--test", "--extern"])
        .arg(format!("rill_ownership={artifact}"))
        .arg(tests)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(executable).output().unwrap();
    assert!(
        run.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
}

struct Cleanup<'a>(&'a Path);
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
    }
}
