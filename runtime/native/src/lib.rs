//! Rill's native runtime library, linked into every built program.
//!
//! The C symbols below are the contract with generated code. `main` captures the arguments and
//! environment, then calls `rill_startup` and `rill_startup_cleanup`, which generated code
//! provides, on a managed 1 MiB entry stack with a guard page, and returns 0. `main` also sets `SIGPIPE` to ignored, so a write to a closed pipe
//! fails with `EPIPE` instead of ending the process.
#[cfg(feature = "checking")]
mod counting;
mod entry_stack;
#[cfg(feature = "checking")]
mod stack_region;

use std::alloc::{self, Layout};
use std::ffi::{CStr, c_char, c_int};
use std::fs::File;
use std::io::Write;
use std::mem::ManuallyDrop;
use std::os::fd::FromRawFd;
use std::sync::OnceLock;

const PANIC_STATUS: c_int = 101;
const SIGPIPE: c_int = 13;
const SIG_IGN: usize = 1;

unsafe extern "C" {
    fn rill_startup();
    fn rill_startup_cleanup();
    fn _exit(status: c_int) -> !;
    fn signal(signal: c_int, handler: usize) -> usize;
}

struct Captured {
    arguments: Vec<Vec<u8>>,
    environment: Vec<Vec<u8>>,
}

static CAPTURED: OnceLock<Captured> = OnceLock::new();

/// Ignores `SIGPIPE`, captures the arguments and environment, runs startup, then its cleanup,
/// on the entry stack, and returns 0. In the `checking` build, counting of live allocations
/// starts after the capture and the count is reported on standard error after cleanup, or after
/// the panic line.
///
/// # Safety
///
/// Called once by the C runtime with its `argc` and `argv`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    // SAFETY: `signal` with `SIG_IGN` installs no Rust code as a handler.
    unsafe { signal(SIGPIPE, SIG_IGN) };
    let arguments = usize::try_from(argc).unwrap_or(0);
    // SAFETY: the C runtime passes `argc` valid NUL-terminated strings in `argv`, and the
    // environment block it owns is a null-terminated array of the same kind.
    let captured = unsafe {
        Captured {
            arguments: capture(argv, Some(arguments)),
            environment: capture(environment(), None),
        }
    };
    let _ = CAPTURED.set(captured);
    #[cfg(feature = "checking")]
    counting::start();
    if let Err(message) = entry_stack::run_on_entry_stack(run_startup) {
        panic_exit(message);
    }
    report_live_allocations();
    0
}

extern "C" fn run_startup() {
    // SAFETY: generated code defines both symbols with these signatures.
    unsafe {
        rill_startup();
        rill_startup_cleanup();
    }
}

#[cfg(not(feature = "checking"))]
fn report_live_allocations() {}

#[cfg(feature = "checking")]
fn report_live_allocations() {
    if let Some(live) = counting::stop() {
        let mut line = [0u8; 48];
        let mut free = &mut line[..];
        let _ = writeln!(free, "live allocations: {live}");
        let length = 48 - free.len();
        let _ = descriptor(2).write_all(&line[..length]);
    }
}

unsafe fn capture(strings: *const *const c_char, count: Option<usize>) -> Vec<Vec<u8>> {
    let mut captured = Vec::new();
    if strings.is_null() {
        return captured;
    }
    for index in 0.. {
        // SAFETY: the caller guarantees `count` entries or a terminating null entry.
        let entry = unsafe { *strings.add(index) };
        if entry.is_null() || count.is_some_and(|count| index >= count) {
            break;
        }
        // SAFETY: a non-null entry is a valid NUL-terminated string.
        captured.push(unsafe { CStr::from_ptr(entry) }.to_bytes().to_vec());
    }
    captured
}

#[cfg(target_os = "macos")]
unsafe fn environment() -> *const *const c_char {
    unsafe extern "C" {
        fn _NSGetEnviron() -> *const *const *const c_char;
    }
    // SAFETY: `_NSGetEnviron` returns the address of the process's `environ` variable.
    unsafe { *_NSGetEnviron() }
}

#[cfg(not(target_os = "macos"))]
unsafe fn environment() -> *const *const c_char {
    unsafe extern "C" {
        static environ: *const *const c_char;
    }
    // SAFETY: `environ` is defined by the C library.
    unsafe { environ }
}

fn captured_entry(entries: &'static [Vec<u8>], index: usize, len: *mut usize) -> *const u8 {
    let Some(entry) = entries.get(index) else {
        panic_exit(b"index out of range");
    };
    // SAFETY: the caller of the public accessor passes a writable `len`.
    unsafe { *len = entry.len() };
    entry.as_ptr()
}

fn captured() -> &'static Captured {
    static EMPTY: Captured = Captured {
        arguments: Vec::new(),
        environment: Vec::new(),
    };
    CAPTURED.get().unwrap_or(&EMPTY)
}

/// The number of arguments `main` received, including argument zero.
#[unsafe(no_mangle)]
pub extern "C" fn rill_rt_arg_count() -> usize {
    captured().arguments.len()
}

/// The bytes of argument `index`, unvalidated and not NUL-terminated; their length goes to `len`.
/// The bytes live for the whole program. An index past the last argument is a panic.
///
/// # Safety
///
/// `len` is valid for one write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_rt_arg(index: usize, len: *mut usize) -> *const u8 {
    captured_entry(&captured().arguments, index, len)
}

/// The number of `NAME=value` entries in the environment `main` saw.
#[unsafe(no_mangle)]
pub extern "C" fn rill_rt_env_count() -> usize {
    captured().environment.len()
}

/// The bytes of environment entry `index`, unvalidated and not NUL-terminated; their length goes
/// to `len`. The bytes live for the whole program. An index past the last entry is a panic.
///
/// # Safety
///
/// `len` is valid for one write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_rt_env(index: usize, len: *mut usize) -> *const u8 {
    captured_entry(&captured().environment, index, len)
}

fn layout(size: usize, align: usize) -> Layout {
    Layout::from_size_align(size, align)
        .unwrap_or_else(|_| panic_exit(b"invalid allocation layout"))
}

fn dangling(align: usize) -> *mut u8 {
    std::ptr::without_provenance_mut(align)
}

fn checked(pointer: *mut u8) -> *mut u8 {
    if pointer.is_null() {
        panic_exit(b"out of memory");
    }
    pointer
}

/// Allocates `size` bytes aligned to `align` from the system allocator. Never returns null:
/// failure, or an `align` that is not a power of two, is a panic. A zero `size` returns a
/// dangling pointer equal to `align` and allocates nothing; callers should avoid it.
#[unsafe(no_mangle)]
pub extern "C" fn rill_rt_allocate(size: usize, align: usize) -> *mut u8 {
    let layout = layout(size, align);
    if size == 0 {
        return dangling(align);
    }
    // SAFETY: the layout has a nonzero size.
    checked(unsafe { alloc::alloc(layout) })
}

/// Resizes a block from `rill_rt_allocate` or `rill_rt_grow` to `new_size` bytes, keeping the
/// bytes up to the smaller size and the same `align`. Never returns null: failure is a panic.
///
/// # Safety
///
/// `ptr` is a live block of `old_size` bytes aligned to `align` from this library. It is not
/// used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_rt_grow(
    ptr: *mut u8,
    old_size: usize,
    align: usize,
    new_size: usize,
) -> *mut u8 {
    let old = layout(old_size, align);
    let new = layout(new_size, align);
    match (old_size, new_size) {
        (0, _) => rill_rt_allocate(new_size, align),
        (_, 0) => {
            // SAFETY: the caller guarantees `ptr` is a live block with this layout.
            unsafe { alloc::dealloc(ptr, old) };
            dangling(align)
        }
        // SAFETY: the caller guarantees `ptr` is a live block with layout `old`, and
        // `new_size` is nonzero and fits `isize` once rounded up to `align`.
        _ => checked(unsafe { alloc::realloc(ptr, old, new.size()) }),
    }
}

/// Returns a block from `rill_rt_allocate` or `rill_rt_grow` to the system allocator.
///
/// # Safety
///
/// `ptr` is a live block of `size` bytes aligned to `align` from this library. It is not used
/// again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_rt_release(ptr: *mut u8, size: usize, align: usize) {
    let layout = layout(size, align);
    if size != 0 {
        // SAFETY: the caller guarantees `ptr` is a live block with this layout.
        unsafe { alloc::dealloc(ptr, layout) };
    }
}

fn descriptor(fd: c_int) -> ManuallyDrop<File> {
    // SAFETY: the descriptor is only written to, and `ManuallyDrop` never closes it.
    ManuallyDrop::new(unsafe { File::from_raw_fd(fd) })
}

fn bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        return &[];
    }
    // SAFETY: the caller of the public function passes `len` readable bytes.
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

fn panic_exit(message: &[u8]) -> ! {
    let mut standard_error = descriptor(2);
    let _ = standard_error.write_all(b"panic: ");
    let _ = standard_error.write_all(message);
    let _ = standard_error.write_all(b"\n");
    report_live_allocations();
    // SAFETY: `_exit` ends the process at once and runs no cleanup.
    unsafe { _exit(PANIC_STATUS) }
}

/// Writes the bytes and one `\n` straight to file descriptor 1, unbuffered. A write that fails
/// is a panic with the message `standard output is not writable`.
///
/// # Safety
///
/// `ptr` is valid for `len` bytes of reads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_rt_write_line(ptr: *const u8, len: usize) {
    let mut standard_output = descriptor(1);
    let written = standard_output
        .write_all(bytes(ptr, len))
        .and_then(|()| standard_output.write_all(b"\n"));
    if written.is_err() {
        panic_exit(b"standard output is not writable");
    }
}

/// Writes exactly `panic: <message>\n` to file descriptor 2 and ends the process with status
/// 101. No startup cleanup runs and nothing is buffered.
///
/// # Safety
///
/// `ptr` is valid for `len` bytes of reads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_rt_panic(ptr: *const u8, len: usize) -> ! {
    panic_exit(bytes(ptr, len))
}
