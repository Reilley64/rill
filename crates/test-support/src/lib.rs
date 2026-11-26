use std::{
    fs,
    io::{self, ErrorKind},
    path::Path,
    process::{Child, Command, Output, Stdio},
    sync::{Mutex, MutexGuard},
    thread,
    time::Duration,
};

const BUSY_ATTEMPTS: u32 = 5;

static LAUNCH: Mutex<()> = Mutex::new(());

fn launch() -> MutexGuard<'static, ()> {
    LAUNCH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Place the executable `from` at `to` so that `to` can be spawned at once.
///
/// The destination is a hard link, so no descriptor open for writing exists
/// and Linux cannot refuse the spawn with `ETXTBSY`. When the link fails (for
/// example across file systems) the executable is copied under the lock that
/// [`spawn_staged`] takes, which keeps every spawn made through this crate
/// out of the copy.
pub fn stage_executable(from: &Path, to: &Path) -> io::Result<()> {
    if fs::hard_link(from, to).is_ok() {
        return Ok(());
    }
    let _launch = launch();
    fs::copy(from, to).map(drop)
}

/// Stage this test binary at `to`. See [`stage_executable`].
pub fn stage_current_exe(to: &Path) -> io::Result<()> {
    stage_executable(&std::env::current_exe()?, to)
}

/// Spawn `command` with a null standard input and piped standard output and
/// standard error.
///
/// The spawn holds the lock that a fallback copy in [`stage_executable`]
/// holds, and is tried again a bounded number of times when the executable is
/// still busy.
pub fn spawn_staged(command: &mut Command) -> io::Result<Child> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut attempt = 1;
    loop {
        let result = {
            let _launch = launch();
            command.spawn()
        };
        match result {
            Err(error)
                if error.kind() == ErrorKind::ExecutableFileBusy && attempt < BUSY_ATTEMPTS =>
            {
                thread::sleep(Duration::from_millis(20 * u64::from(attempt)));
                attempt += 1;
            }
            result => return result,
        }
    }
}

/// Run `command` to completion and collect its output. See [`spawn_staged`].
pub fn run_staged(command: &mut Command) -> io::Result<Output> {
    spawn_staged(command)?.wait_with_output()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("rill-test-support-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn a_staged_current_exe_runs_at_once() {
        let root = scratch("run");
        let staged = root.join("probe");
        stage_current_exe(&staged).unwrap();
        let output = run_staged(Command::new(&staged).args(["--list"])).unwrap();
        assert!(output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("a_staged_current_exe_runs_at_once")
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn staging_shares_the_inode_with_the_source() {
        use std::os::unix::fs::MetadataExt;
        let root = scratch("inode");
        let staged = root.join("probe");
        stage_current_exe(&staged).unwrap();
        let source = fs::metadata(std::env::current_exe().unwrap()).unwrap();
        assert_eq!(fs::metadata(&staged).unwrap().ino(), source.ino());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn concurrent_staging_and_spawning_never_sees_a_busy_executable() {
        let root = scratch("race");
        let threads: Vec<_> = (0..16)
            .map(|index| {
                let staged = root.join(format!("probe-{index}"));
                thread::spawn(move || {
                    for _ in 0..4 {
                        let _ = fs::remove_file(&staged);
                        stage_current_exe(&staged).unwrap();
                        let output = run_staged(Command::new(&staged).args(["--list"])).unwrap();
                        assert!(output.status.success());
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let _ = fs::remove_dir_all(&root);
    }
}
