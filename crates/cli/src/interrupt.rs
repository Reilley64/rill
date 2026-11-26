use std::ffi::c_int;
use std::io;
use std::mem;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::ptr;
use std::sync::Mutex;
use std::thread;

struct State {
    scratch: Option<PathBuf>,
    child: Option<u32>,
}

static STATE: Mutex<State> = Mutex::new(State {
    scratch: None,
    child: None,
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Takes `SIGINT` and `SIGTERM` for the whole process: from now on either one stops the child
/// registered with [`set_child`], removes the directory registered with [`set_scratch`] and
/// exits with 128 plus the signal number (130 for `SIGINT`). Children the process starts
/// afterwards receive the signals normally. Call it before any other thread starts.
pub fn exit_on_signals() -> io::Result<()> {
    // SAFETY: `sigemptyset` and `sigaddset` initialise `set`, an all-zero `sigset_t` being a
    // valid place to build one, and `pthread_sigmask` reads it.
    let set = unsafe {
        let mut set: libc::sigset_t = mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::sigaddset(&mut set, libc::SIGTERM);
        let status = libc::pthread_sigmask(libc::SIG_BLOCK, &set, ptr::null_mut());
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status));
        }
        set
    };
    thread::Builder::new().spawn(move || {
        loop {
            let mut signal: c_int = 0;
            // SAFETY: `set` is an initialised signal set and `signal` is writable.
            if unsafe { libc::sigwait(&set, &mut signal) } != 0 {
                continue;
            }
            let state = state();
            if let Some(child) = state.child.and_then(|child| i32::try_from(child).ok()) {
                // SAFETY: `kill` takes a process id and a signal number and touches no memory.
                unsafe { libc::kill(child, signal) };
            }
            if let Some(scratch) = &state.scratch {
                let _ = std::fs::remove_dir_all(scratch);
            }
            std::process::exit(128 + signal);
        }
    })?;
    Ok(())
}

/// Makes `command` start with `SIGINT` and `SIGTERM` unblocked, which [`exit_on_signals`] blocks
/// in this process and which a child would otherwise inherit blocked.
pub fn unblock_signals(command: &mut Command) -> &mut Command {
    // SAFETY: the closure runs between `fork` and `exec` and calls only `sigemptyset` and
    // `pthread_sigmask`, which are async-signal-safe and touch no memory but their own.
    unsafe {
        command.pre_exec(|| {
            let mut none: libc::sigset_t = mem::zeroed();
            libc::sigemptyset(&mut none);
            match libc::pthread_sigmask(libc::SIG_SETMASK, &none, ptr::null_mut()) {
                0 => Ok(()),
                status => Err(io::Error::from_raw_os_error(status)),
            }
        })
    }
}

/// Names the directory an interrupt removes.
pub fn set_scratch(path: Option<PathBuf>) {
    state().scratch = path;
}

/// Names the process an interrupt stops, or none.
pub fn set_child(pid: Option<u32>) {
    state().child = pid;
}
