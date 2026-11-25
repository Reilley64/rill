use std::{
    fs, io,
    path::Path,
    process::{Command, Output, Stdio},
    sync::Mutex,
};

static LAUNCH: Mutex<()> = Mutex::new(());

pub(crate) fn copy_executable(from: &Path, to: &Path) -> io::Result<u64> {
    let _launch = LAUNCH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    fs::copy(from, to)
}

pub(crate) fn run(command: &mut Command) -> io::Result<Output> {
    let child = {
        let _launch = LAUNCH
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
    };
    child.wait_with_output()
}
