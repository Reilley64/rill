use crate::check::{
    self, ANALYSIS_INCOMPLETE, INVOCATION_ERROR, SOURCE_FAILURE, escape, report_failure,
};
use crate::native_libs::NATIVE_LIBS;
use compiler::analysis::compilable::CompilableProgram;
use compiler::loader::Graph;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const HOST_TARGET: &str = env!("RILL_HOST_TARGET");
const SIGNAL_STATUS: u8 = 128;

struct Failure {
    status: u8,
    message: String,
    tool_output: Vec<u8>,
}

impl Failure {
    fn new(status: u8, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            tool_output: Vec::new(),
        }
    }

    fn report(self) -> ExitCode {
        let status = self.status;
        let code = report_failure(&self.message, status);
        if !self.tool_output.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&self.tool_output));
        }
        code
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> io::Result<Self> {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.subsec_nanos());
        for _ in 0..16 {
            let path = std::env::temp_dir().join(format!(
                "rill-{}-{nanos}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::ErrorKind::AlreadyExists.into())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn build(entry: &str, output: Option<&str>) -> ExitCode {
    let output = output.map_or_else(|| default_output(entry), PathBuf::from);
    check::with_program(entry, |graph, program| {
        match build_to(entry, graph, program, &output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(failure) => failure.report(),
        }
    })
}

pub fn run(entry: &str, arguments: &[OsString]) -> ExitCode {
    check::with_program(entry, |graph, program| {
        match run_built(entry, graph, program, arguments) {
            Ok(code) => code,
            Err(failure) => failure.report(),
        }
    })
}

fn default_output(entry: &str) -> PathBuf {
    let stem = Path::new(entry).file_stem().unwrap_or(entry.as_ref());
    Path::new(".").join(stem)
}

fn build_to(
    entry: &str,
    graph: &Graph,
    program: &CompilableProgram<'_, '_>,
    output: &Path,
) -> Result<(), Failure> {
    if same_file(Path::new(entry), output) {
        return Err(Failure::new(
            INVOCATION_ERROR,
            format!("the output '{}' is the entry file", output.display()),
        ));
    }
    let scratch = Scratch::create().map_err(scratch_failure)?;
    let object = emit(graph, program, &scratch.0)?;
    let directory = match output.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let name = output.file_name().map_or_else(
        || "program".into(),
        |name| name.to_string_lossy().into_owned(),
    );
    let temporary = directory.join(format!(".{name}.rill-{}", std::process::id()));
    let linked = link(&object, &temporary).and_then(|()| {
        fs::rename(&temporary, output).map_err(|error| {
            Failure::new(
                SOURCE_FAILURE,
                format!("cannot write '{}': {error}", output.display()),
            )
        })
    });
    if linked.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    linked
}

fn run_built(
    entry: &str,
    graph: &Graph,
    program: &CompilableProgram<'_, '_>,
    arguments: &[OsString],
) -> Result<ExitCode, Failure> {
    let scratch = Scratch::create().map_err(scratch_failure)?;
    let object = emit(graph, program, &scratch.0)?;
    let executable = scratch.0.join("program");
    link(&object, &executable)?;
    let mut child = Command::new(&executable)
        .arg0(entry)
        .args(arguments)
        .spawn()
        .map_err(|error| {
            Failure::new(
                SOURCE_FAILURE,
                format!("cannot run the built program: {error}"),
            )
        })?;
    let status = child.wait().map_err(|error| {
        Failure::new(
            SOURCE_FAILURE,
            format!("cannot wait for the built program: {error}"),
        )
    })?;
    drop(scratch);
    let code = match (status.code(), status.signal()) {
        (Some(code), _) => code,
        (None, Some(signal)) => i32::from(SIGNAL_STATUS) + signal,
        (None, None) => i32::from(SOURCE_FAILURE),
    };
    Ok(ExitCode::from(u8::try_from(code).unwrap_or(u8::MAX)))
}

fn emit(
    graph: &Graph,
    program: &CompilableProgram<'_, '_>,
    scratch: &Path,
) -> Result<PathBuf, Failure> {
    let object = scratch.join("program.o");
    codegen_llvm::emit_object(graph, program, &object).map_err(|error| {
        Failure::new(
            ANALYSIS_INCOMPLETE,
            format!("the backend failed: {error}; this is a compiler failure, not a source error"),
        )
    })?;
    Ok(object)
}

fn link(object: &Path, executable: &Path) -> Result<(), Failure> {
    let archive = runtime_archive()?;
    let output = Command::new("cc")
        .arg("-o")
        .arg(executable)
        .arg(object)
        .arg(&archive)
        .args(NATIVE_LIBS)
        .output()
        .map_err(|error| Failure::new(SOURCE_FAILURE, format!("cannot run cc: {error}")))?;
    if output.status.success() {
        return Ok(());
    }
    let mut failure = Failure::new(
        SOURCE_FAILURE,
        format!("cc failed to link the program ({})", output.status),
    );
    failure.tool_output = output.stderr;
    Err(failure)
}

fn runtime_archive() -> Result<PathBuf, Failure> {
    let archive = std::env::current_exe()
        .and_then(fs::canonicalize)
        .ok()
        .and_then(|executable| Some(executable.parent()?.parent()?.to_path_buf()))
        .map(|prefix| {
            prefix
                .join("lib/rill/runtime")
                .join(HOST_TARGET)
                .join("librill_runtime.a")
        })
        .filter(|archive| archive.is_file());
    archive.ok_or_else(|| {
        Failure::new(
            SOURCE_FAILURE,
            format!(
                "the installation has no runtime archive at lib/rill/runtime/{}/librill_runtime.a",
                escape(HOST_TARGET)
            ),
        )
    })
}

fn scratch_failure(error: io::Error) -> Failure {
    Failure::new(
        SOURCE_FAILURE,
        format!("cannot create a temporary directory: {error}"),
    )
}

fn same_file(entry: &Path, output: &Path) -> bool {
    match (fs::canonicalize(entry), fs::canonicalize(output)) {
        (Ok(entry), Ok(output)) => entry == output,
        _ => false,
    }
}
