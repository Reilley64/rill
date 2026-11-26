#![cfg(feature = "llvm")]

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

mod support;

use support::{Installation, TARGET};

const PREFIX_VARIABLE: &str = "RILL_PROTOTYPES_PREFIX";
const HELLO: &str = include_str!("../../../tests/prototypes/hello-world.rill");
const HELLO_OUTPUT: &[u8] = b"hello world\n";

struct Workspace {
    rill: PathBuf,
    work: PathBuf,
    temporary: PathBuf,
    _installation: Option<Installation>,
}

impl Workspace {
    fn new(test: &str) -> Self {
        let (rill, work, installation) = match std::env::var_os(PREFIX_VARIABLE) {
            Some(prefix) => {
                let rill = Path::new(&prefix).join("bin/rill");
                assert!(
                    rill.is_file(),
                    "{PREFIX_VARIABLE} names no staged installation: run mise run build:prototypes"
                );
                let work = std::env::temp_dir()
                    .join(format!("rill-prototypes-{test}-{}", std::process::id()));
                let _ = fs::remove_dir_all(&work);
                (rill, work, None)
            }
            None => {
                let installation = Installation::stage(&format!("prototypes-{test}"));
                install_runtime_archive(&installation.root);
                let rill = installation.root.join("bin/rill");
                let work = installation.root.join("work");
                (rill, work, Some(installation))
            }
        };
        let temporary = work.join("tmp");
        fs::create_dir_all(work.join("app")).unwrap();
        fs::create_dir_all(&temporary).unwrap();
        Self {
            rill,
            work,
            temporary,
            _installation: installation,
        }
    }

    fn entry(&self, name: &str, source: &str) -> String {
        let relative = format!("app/{name}");
        fs::write(self.work.join(&relative), source).unwrap();
        relative
    }

    fn identity(&self, entry: &str) -> String {
        fs::canonicalize(self.work.join(entry))
            .unwrap()
            .display()
            .to_string()
    }

    fn rill(&self, args: &[&str]) -> Output {
        self.command(&self.rill, args, None)
    }

    fn command(&self, program: &Path, args: &[&str], path: Option<&str>) -> Output {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(&self.work)
            .env("TMPDIR", &self.temporary);
        if let Some(path) = path {
            command.env("PATH", path);
        }
        rill_test_support::run_staged(&mut command).expect("run a staged executable")
    }

    fn names(&self, directory: &str) -> BTreeSet<String> {
        fs::read_dir(self.work.join(directory))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect()
    }

    fn files(&self) -> BTreeSet<String> {
        self.names(".")
    }

    fn assert_no_scratch(&self) {
        assert!(self.names("tmp").is_empty(), "{:?}", self.names("tmp"));
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        if self._installation.is_none() {
            let _ = fs::remove_dir_all(&self.work);
        }
    }
}

fn install_runtime_archive(root: &Path) {
    static ARCHIVE: OnceLock<PathBuf> = OnceLock::new();
    let archive = ARCHIVE.get_or_init(|| {
        let executable = std::env::current_exe().expect("test executable path");
        let profile_directory = executable
            .parent()
            .and_then(Path::parent)
            .expect("profile directory above deps");
        let target_directory = profile_directory.parent().expect("target directory");
        let mut build = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        build
            .args(["build", "--locked", "-p", "rill-runtime", "--target-dir"])
            .arg(target_directory)
            .current_dir(env!("CARGO_MANIFEST_DIR"));
        match profile_directory.file_name().and_then(|name| name.to_str()) {
            Some("debug") => {}
            Some(name) => {
                build.args(["--profile", name]);
            }
            None => panic!("profile directory has no name"),
        }
        assert!(
            build
                .status()
                .expect("cargo build of the runtime")
                .success(),
            "cargo build of the runtime failed"
        );
        profile_directory.join("librill_runtime.a")
    });
    fs::copy(
        archive,
        root.join("lib/rill/runtime")
            .join(TARGET)
            .join("librill_runtime.a"),
    )
    .unwrap();
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_hello(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{}", stderr(output));
    assert_eq!(output.stdout, HELLO_OUTPUT);
    assert!(output.stderr.is_empty(), "{}", stderr(output));
}

fn assert_unsupported_shape(
    output: &Output,
    identity: &str,
    line: usize,
    column: usize,
    source_line: &str,
    marked: &str,
) {
    assert_eq!(output.status.code(), Some(1), "{}", stderr(output));
    assert!(output.stdout.is_empty(), "{output:?}");
    let gutter = " ".repeat(line.to_string().len());
    assert_eq!(
        stderr(output),
        format!(
            "error[R0605]: valid source whose shape the compiler cannot compile yet\n --> {identity}:{line}:{column}\n{gutter} |\n{line} | {source_line}\n{gutter} | {}{}\n",
            " ".repeat(column - 1),
            "^".repeat(marked.len())
        )
    );
}

fn assert_checked_hello(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{}", stderr(output));
    assert_eq!(output.stdout, HELLO_OUTPUT);
    assert_eq!(stderr(output), "live allocations: 0\n");
}

#[test]
fn run_with_check_allocations_reports_zero_live_allocations() {
    let workspace = Workspace::new("check-run");
    let entry = workspace.entry("hello-world.rill", HELLO);
    let before = workspace.files();
    assert_checked_hello(&workspace.rill(&[
        "run",
        "--check-allocations",
        &entry,
        "an",
        "argument",
    ]));
    assert_eq!(workspace.files(), before);
    workspace.assert_no_scratch();
}

#[test]
fn build_with_check_allocations_makes_a_program_that_reports_zero() {
    let workspace = Workspace::new("check-build");
    let entry = workspace.entry("hello-world.rill", HELLO);
    let output = workspace.rill(&["build", "--check-allocations", &entry, "-o", "app/checked"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
    assert_checked_hello(&workspace.command(&workspace.work.join("app/checked"), &[], None));
    let plain = workspace.rill(&["build", &entry, "-o", "app/plain"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    assert_hello(&workspace.command(&workspace.work.join("app/plain"), &[], None));
}

#[test]
fn check_allocations_is_in_no_help_text() {
    let workspace = Workspace::new("check-help");
    for args in [&["--help"][..], &["build", "--help"], &["run", "--help"]] {
        let output = workspace.rill(args);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            stderr(&output)
        );
        assert!(!text.contains("check-allocations"), "{args:?}: {text}");
    }
}

const UNSUPPORTED: &str = "function ready(flag: bool): bool { return flag; }\nfunction minus(a: i32, b: i32): i32 { return a - b; }\nlet ok = ready(true);\nlet n = minus(3i32, 1i32);\n";

#[test]
fn run_prints_hello_world_and_leaves_nothing_behind() {
    let workspace = Workspace::new("run");
    let entry = workspace.entry("hello-world.rill", HELLO);
    let before = workspace.files();
    let output = workspace.rill(&["run", &entry, "an", "--argument"]);
    assert_hello(&output);
    assert_eq!(workspace.files(), before);
    workspace.assert_no_scratch();
}

#[test]
fn build_writes_the_entry_stem_and_the_program_prints_hello_world() {
    let workspace = Workspace::new("build");
    let entry = workspace.entry("hello-world.rill", HELLO);
    let output = workspace.rill(&["build", &entry]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
    assert_eq!(
        workspace.files(),
        BTreeSet::from(["app".into(), "hello-world".into(), "tmp".into()])
    );
    workspace.assert_no_scratch();
    assert_hello(&workspace.command(&workspace.work.join("hello-world"), &[], None));
}

#[test]
fn build_o_writes_the_chosen_path_and_nowhere_else() {
    let workspace = Workspace::new("output");
    let entry = workspace.entry("hello-world.rill", HELLO);
    fs::create_dir(workspace.work.join("out")).unwrap();
    let before = workspace.files();
    let output = workspace.rill(&["build", &entry, "-o", "out/greeting"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
    assert_eq!(workspace.files(), before);
    assert_eq!(workspace.names("out"), BTreeSet::from(["greeting".into()]));
    workspace.assert_no_scratch();
    assert_hello(&workspace.command(&workspace.work.join("out/greeting"), &[], None));
}

#[test]
fn an_unsupported_shape_exits_one_with_r0605_and_no_executable() {
    let workspace = Workspace::new("unsupported");
    let entry = workspace.entry("main.rill", UNSUPPORTED);
    let before = workspace.files();
    let identity = workspace.identity(&entry);
    let minus = UNSUPPORTED.lines().nth(1).unwrap();
    let column = minus.find("a - b").unwrap() + 1;
    for command in ["check", "build", "run"] {
        assert_unsupported_shape(
            &workspace.rill(&[command, &entry]),
            &identity,
            2,
            column,
            minus,
            "a - b",
        );
        assert_eq!(workspace.files(), before);
        workspace.assert_no_scratch();
    }
    assert_unsupported_shape(
        &workspace.rill(&["build", &entry, "-o", "app/built"]),
        &identity,
        2,
        column,
        minus,
        "a - b",
    );
    assert!(!workspace.work.join("app/built").exists());
}

#[test]
fn a_shape_the_backend_does_not_lower_is_r0605_from_check_build_and_run_alike() {
    let workspace = Workspace::new("backend-shape");
    let entry = workspace.entry("main.rill", "let flag = true;\n");
    let identity = workspace.identity(&entry);
    let before = workspace.files();
    for command in ["check", "build", "run"] {
        assert_unsupported_shape(
            &workspace.rill(&[command, &entry]),
            &identity,
            1,
            12,
            "let flag = true;",
            "true",
        );
        assert_eq!(workspace.files(), before);
        workspace.assert_no_scratch();
    }
    assert_unsupported_shape(
        &workspace.rill(&["build", &entry, "-o", "app/built"]),
        &identity,
        1,
        12,
        "let flag = true;",
        "true",
    );
    assert!(!workspace.work.join("app/built").exists());
}

#[test]
fn a_missing_cc_exits_one_naming_cc_and_leaves_no_executable() {
    let workspace = Workspace::new("no-cc");
    let entry = workspace.entry("hello-world.rill", HELLO);
    let before = workspace.files();
    for command in ["build", "run"] {
        let output = workspace.command(&workspace.rill, &[command, &entry], Some(""));
        assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(stderr(&output).contains("cc"), "{}", stderr(&output));
        assert_eq!(workspace.files(), before);
        workspace.assert_no_scratch();
    }
}

#[test]
fn a_bad_invocation_exits_two_and_builds_nothing() {
    let workspace = Workspace::new("invocation");
    let entry = workspace.entry("hello-world.rill", HELLO);
    let before = workspace.files();
    let absent = workspace.work.join("absent.rill");
    let invocations: [&[&str]; 10] = [
        &["build"],
        &["run"],
        &["build", "--check-allocations"],
        &["run", "--check-allocations", "--bogus"],
        &["build", "--bogus"],
        &["run", "--bogus"],
        &["build", &entry, "--bogus"],
        &["build", &entry, "-o"],
        &["build", &entry, "extra"],
        &["build", absent.to_str().unwrap()],
    ];
    for args in invocations {
        let output = workspace.rill(args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?}: {}",
            stderr(&output)
        );
        assert!(output.stdout.is_empty(), "{output:?}");
        assert_eq!(workspace.files(), before, "{args:?}");
    }
    let absent = workspace.rill(&["run", absent.to_str().unwrap()]);
    assert_eq!(absent.status.code(), Some(2), "{}", stderr(&absent));
}

#[test]
fn build_refuses_to_overwrite_the_entry() {
    let workspace = Workspace::new("overwrite");
    let entry = workspace.entry("hello-world.rill", HELLO);
    let output = workspace.rill(&["build", &entry, "-o", &entry]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(
        fs::read_to_string(workspace.work.join(&entry)).unwrap(),
        HELLO
    );
}

fn signal_cc(workspace: &Workspace, signal: &str) -> String {
    let directory = workspace.work.join("fake-bin");
    fs::create_dir_all(&directory).unwrap();
    let cc = directory.join("cc");
    fs::write(
        &cc,
        format!(
            "#!/bin/sh\nprintf '#!/bin/sh\\nkill -{signal} $$\\n' > \"$2\"\n/bin/chmod +x \"$2\"\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&cc, fs::Permissions::from_mode(0o755)).unwrap();
    directory.into_os_string().into_string().unwrap()
}

#[test]
fn run_returns_128_plus_the_signal_and_prints_nothing_when_the_program_dies_by_one() {
    let workspace = Workspace::new("signal");
    let entry = workspace.entry("hello-world.rill", HELLO);
    let before = workspace.files();
    for (signal, number) in [("KILL", 9), ("TERM", 15), ("ABRT", 6)] {
        let path = signal_cc(&workspace, signal);
        let output = workspace.command(&workspace.rill, &["run", &entry], Some(&path));
        assert_eq!(
            output.status.code(),
            Some(128 + number),
            "{signal}: {}",
            stderr(&output)
        );
        assert!(output.stdout.is_empty(), "{signal}: {output:?}");
        assert!(output.stderr.is_empty(), "{signal}: {output:?}");
        workspace.assert_no_scratch();
    }
    fs::remove_dir_all(workspace.work.join("fake-bin")).unwrap();
    assert_eq!(workspace.files(), before);
}
