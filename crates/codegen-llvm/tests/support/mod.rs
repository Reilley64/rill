use compiler::analysis::AnalysisLimits;
use compiler::analysis::compilable::{Compilability, ProgramAnalysis};
use compiler::analysis::typed::analyze_typed;
use compiler::loader::{self, InstalledRequest};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};

pub const TARGET: &str = env!("RILL_HOST_TARGET");
const PROBE_ROOT: &str = "RILL_PROBE_ROOT";

#[cfg(target_os = "macos")]
const NATIVE_LIBS: &[&str] = &["-liconv", "-lSystem", "-lc", "-lm"];
#[cfg(target_os = "linux")]
const NATIVE_LIBS: &[&str] = &[
    "-lgcc_s",
    "-lutil",
    "-lrt",
    "-lpthread",
    "-lm",
    "-ldl",
    "-lc",
];

/// The installed layout of a toolchain staged for one test, removed on drop.
pub struct Installation {
    root: PathBuf,
}

impl Installation {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn object(&self) -> PathBuf {
        self.root.join("app/program.o")
    }

    fn error_file(&self) -> PathBuf {
        self.root.join("app/error.txt")
    }

    fn shapes_file(&self) -> PathBuf {
        self.root.join("app/shapes.txt")
    }
}

impl Drop for Installation {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn child_request() -> Option<InstalledRequest> {
    let root = PathBuf::from(std::env::var_os(PROBE_ROOT)?);
    Some(InstalledRequest {
        entry: root.join("app/main.rill"),
        target: TARGET.into(),
        options: Default::default(),
        test_mode: false,
        limits: Default::default(),
    })
}

/// What the backend did with a staged program.
#[allow(dead_code)]
pub struct Built {
    installation: Installation,
    /// The backend's error message, when it refused to emit an object.
    pub error: Option<String>,
    /// The names of the shapes the compilable-program rule found in the
    /// program, one per line of the child's report.
    pub shapes: Vec<String>,
}

impl Built {
    pub fn object(&self) -> PathBuf {
        self.installation.object()
    }

    /// Link the object with the runtime through `cc` and run it with an empty
    /// environment.
    pub fn run(&self) -> Output {
        let executable = self.installation.root().join("app/program");
        let link = link(&self.object(), &executable).expect("host cc");
        assert!(
            link.status.success(),
            "cc failed: {}",
            String::from_utf8_lossy(&link.stderr)
        );
        Command::new(&executable)
            .env_clear()
            .stdin(Stdio::null())
            .output()
            .expect("run the executable")
    }
}

/// Take the staged program through the front end and the backend. The work
/// happens in a child copy of this test binary run from the staged `bin/`,
/// which is the only place the loader finds the installation; in the child
/// this returns `None` once the object (or the error) is written.
pub fn emit(test: &str, source: &str) -> Option<Built> {
    let Some(request) = child_request() else {
        let (installation, output) = run_in_installation(test, source);
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let error = fs::read_to_string(installation.error_file()).ok();
        let shapes = fs::read_to_string(installation.shapes_file())
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect();
        return Some(Built {
            installation,
            error,
            shapes,
        });
    };
    let root = PathBuf::from(std::env::var_os(PROBE_ROOT).expect("a probe child"));
    let graph = loader::load_installed(request).expect("the staged program loads");
    let limits = AnalysisLimits::default();
    let typed = analyze_typed(&graph, limits);
    let analysis = ProgramAnalysis::new(&graph, &typed, limits);
    let program = match analysis.compilability() {
        Compilability::Compilable(program) => program,
        Compilability::Blocked(blocker) => {
            panic!("the staged program is not compilable: {blocker:?}")
        }
    };
    let shapes: Vec<String> = program
        .shapes()
        .iter()
        .map(|shape| format!("{shape:?}"))
        .collect();
    fs::write(root.join("app/shapes.txt"), shapes.join("\n")).unwrap();
    if let Err(error) =
        codegen_llvm::emit_object(&graph, &program, &root.join("app/program.o"), false)
    {
        fs::write(root.join("app/error.txt"), error.to_string()).unwrap();
    }
    None
}

fn run_in_installation(test: &str, source: &str) -> (Installation, Output) {
    static LAUNCH: Mutex<()> = Mutex::new(());
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(test.replace("::", "-"));
    let _ = fs::remove_dir_all(&root);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let std = root.join("lib/rill/std");
    let runtime = root.join("lib/rill/runtime").join(TARGET);
    for directory in [&root.join("bin"), &root.join("app"), &std, &runtime] {
        fs::create_dir_all(directory).unwrap();
    }
    for name in include_str!("../../../../tests/installed/typed/stdlib-sources.txt")
        .lines()
        .chain(include_str!("../../../../tests/installed/typed/routine-sources.txt").lines())
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
    fs::write(root.join("app/main.rill"), source).unwrap();
    let installation = Installation { root };

    let child = {
        let _launch = LAUNCH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let probe = installation.root.join("bin/probe");
        fs::copy(std::env::current_exe().unwrap(), &probe).unwrap();
        Command::new(&probe)
            .args(["--exact", test, "--include-ignored", "--nocapture"])
            .env(PROBE_ROOT, &installation.root)
            .current_dir(installation.root.join("app"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let output = child.wait_with_output().unwrap();
    (installation, output)
}

fn runtime_archive() -> &'static Path {
    static ARCHIVE: OnceLock<PathBuf> = OnceLock::new();
    ARCHIVE.get_or_init(|| {
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
        let status = build.status().expect("cargo build of the runtime");
        assert!(status.success(), "cargo build of the runtime failed");
        let archive = profile_directory.join("librill_runtime.a");
        assert!(archive.exists(), "{} was not built", archive.display());
        archive
    })
}

fn link(object: &Path, executable: &Path) -> io::Result<Output> {
    Command::new("cc")
        .arg("-o")
        .arg(executable)
        .arg(object)
        .arg(runtime_archive())
        .args(NATIVE_LIBS)
        .output()
}
