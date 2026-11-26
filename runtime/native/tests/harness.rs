use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

const STARTUP: &str = r#"
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#ifdef CHECKING
void rill_rt_entry_stack_region(uintptr_t *low, uintptr_t *high);
#endif

void rill_rt_write_line(const uint8_t *ptr, size_t len);
_Noreturn void rill_rt_panic(const uint8_t *ptr, size_t len);
uint8_t *rill_rt_allocate(size_t size, size_t align);
uint8_t *rill_rt_grow(uint8_t *ptr, size_t old_size, size_t align, size_t new_size);
void rill_rt_release(uint8_t *ptr, size_t size, size_t align);
size_t rill_rt_arg_count(void);
const uint8_t *rill_rt_arg(size_t index, size_t *len);
size_t rill_rt_env_count(void);
const uint8_t *rill_rt_env(size_t index, size_t *len);

#define LINE(text) rill_rt_write_line((const uint8_t *)(text), strlen(text))

static volatile uint8_t sink;

#ifdef CHECKING
static void describe_stack(const char *label) {
    volatile uint8_t local = 0;
    uintptr_t low, high;
    rill_rt_entry_stack_region(&low, &high);
    uintptr_t at = (uintptr_t)&local;
    char text[128];
    snprintf(text, sizeof text, "%s %s %zu", label, at >= low && at < high ? "inside" : "outside", (size_t)(high - low));
    LINE(text);
}
#endif

static int mode_is(const char *name) {
    size_t len;
    const uint8_t *arg = rill_rt_arg(1, &len);
    return len == strlen(name) && memcmp(arg, name, len) == 0;
}

void rill_startup(void) {
    if (mode_is("write")) {
        LINE("hello world");
    } else if (mode_is("panic")) {
        LINE("before");
        const char *message = "something went wrong";
        rill_rt_panic((const uint8_t *)message, strlen(message));
    } else if (mode_is("args")) {
        for (size_t i = 0; i < rill_rt_arg_count(); i++) {
            size_t len;
            const uint8_t *arg = rill_rt_arg(i, &len);
            rill_rt_write_line(arg, len);
        }
    } else if (mode_is("env")) {
        for (size_t i = 0; i < rill_rt_env_count(); i++) {
            size_t len;
            const uint8_t *entry = rill_rt_env(i, &len);
            rill_rt_write_line(entry, len);
        }
#ifdef CHECKING
    } else if (mode_is("stack")) {
        describe_stack("startup");
        volatile uint8_t buffer[900 * 1024];
        for (size_t i = 0; i < sizeof buffer; i += 4096) {
            buffer[i] = 1;
        }
        uintptr_t low, high;
        rill_rt_entry_stack_region(&low, &high);
        uintptr_t first = (uintptr_t)&buffer[0];
        LINE(first >= low && first + sizeof buffer <= high ? "buffer inside" : "buffer outside");
        sink = buffer[0];
    } else if (mode_is("below")) {
        uintptr_t low, high;
        rill_rt_entry_stack_region(&low, &high);
        sink = *(volatile uint8_t *)low;
        LINE("lowest byte readable");
        sink = *(volatile uint8_t *)(low - 1);
        LINE("guard readable");
#endif
    } else if (mode_is("leak")) {
        uint8_t *block = rill_rt_allocate(16, 8);
        block = rill_rt_grow(block, 16, 8, 4096);
        uint8_t *other = rill_rt_allocate(32, 8);
        rill_rt_release(other, 32, 8);
        block[0] = 1;
    } else if (mode_is("leak-panic")) {
        uint8_t *block = rill_rt_allocate(16, 8);
        block[0] = 1;
        const char *message = "holding an allocation";
        rill_rt_panic((const uint8_t *)message, strlen(message));
    } else if (mode_is("alloc")) {
        uint8_t *block = rill_rt_allocate(16, 8);
        memcpy(block, "0123456789abcdef", 16);
        block = rill_rt_grow(block, 16, 8, 4096);
        memset(block + 16, 'x', 4080);
        LINE(memcmp(block, "0123456789abcdef", 16) == 0 ? "kept" : "lost");
        rill_rt_release(block, 4096, 8);
        uint8_t *aligned = rill_rt_allocate(64, 64);
        LINE(((uintptr_t)aligned % 64) == 0 ? "aligned" : "misaligned");
        rill_rt_release(aligned, 64, 64);
    }
}

void rill_startup_cleanup(void) {
#ifdef CHECKING
    if (mode_is("stack")) {
        describe_stack("cleanup");
    }
#endif
    LINE("cleanup");
}
"#;

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

fn static_library(checking: bool) -> PathBuf {
    let executable = std::env::current_exe().expect("test executable path");
    let profile_directory = executable
        .parent()
        .and_then(Path::parent)
        .expect("profile directory above deps");
    let target_directory = profile_directory.parent().expect("target directory");
    let profile = profile_directory.file_name().and_then(|name| name.to_str());
    let mut build = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    build.args(["build", "--locked", "-p", "rill-runtime", "--target-dir"]);
    let mut archive = if checking {
        build.arg(target_directory.join("checking-runtime"));
        build.args(["--features", "checking"]);
        target_directory.join("checking-runtime")
    } else {
        build.arg(target_directory);
        target_directory.to_path_buf()
    };
    build.current_dir(env!("CARGO_MANIFEST_DIR"));
    match profile {
        Some("debug") => {}
        Some(name) => {
            build.args(["--profile", name]);
        }
        None => panic!("profile directory has no name"),
    }
    let status = build.status().expect("cargo build of the runtime");
    assert!(status.success(), "cargo build of the runtime failed");
    archive.push(profile.expect("profile directory has a name"));
    archive.push("librill_runtime.a");
    assert!(archive.exists(), "{} was not built", archive.display());
    archive
}

fn build_executable(name: &str, checking: bool) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("harness");
    fs::create_dir_all(&directory).unwrap();
    let source = directory.join("startup.c");
    fs::write(&source, STARTUP).unwrap();
    let program = directory.join(name);
    let output = Command::new("cc")
        .args(checking.then_some("-DCHECKING"))
        .arg("-o")
        .arg(&program)
        .arg(&source)
        .arg(static_library(checking))
        .args(NATIVE_LIBS)
        .output()
        .expect("host cc");
    assert!(
        output.status.success(),
        "cc failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    program
}

fn executable() -> &'static Path {
    static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
    EXECUTABLE.get_or_init(|| build_executable("startup", false))
}

fn checking_executable() -> &'static Path {
    static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
    EXECUTABLE.get_or_init(|| build_executable("checking", true))
}

fn run(arguments: Vec<OsString>) -> Output {
    Command::new(executable())
        .args(arguments)
        .env_clear()
        .output()
        .unwrap()
}

fn run_checking(arguments: Vec<OsString>) -> Output {
    Command::new(checking_executable())
        .args(arguments)
        .env_clear()
        .output()
        .unwrap()
}

fn mode(name: &str) -> Vec<OsString> {
    vec![name.into()]
}

#[test]
fn a_line_is_written_and_the_status_is_zero() {
    let output = run(mode("write"));
    assert_eq!(output.stdout, b"hello world\ncleanup\n");
    assert_eq!(output.stderr, b"");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn a_panic_writes_its_message_and_exits_101_without_cleanup() {
    let output = run(mode("panic"));
    assert_eq!(output.stdout, b"before\n");
    assert_eq!(output.stderr, b"panic: something went wrong\n");
    assert_eq!(output.status.code(), Some(101));
}

#[test]
fn a_closed_standard_output_is_a_panic_with_status_101() {
    let output = Command::new("sh")
        .args(["-c", r#"exec "$0" "$@" >&-"#])
        .arg(executable())
        .arg("write")
        .env_clear()
        .output()
        .unwrap();
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"panic: standard output is not writable\n");
    assert_eq!(output.status.code(), Some(101));
}

#[test]
fn a_standard_output_pipe_with_no_reader_is_a_panic_with_status_101() {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let output = Command::new(executable())
        .arg("write")
        .env_clear()
        .stdout(writer)
        .output()
        .unwrap();
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"panic: standard output is not writable\n");
    assert_eq!(output.status.code(), Some(101));
}

#[test]
fn arguments_reach_startup_byte_for_byte() {
    let non_utf8 = OsString::from_vec(vec![b'a', 0xff, 0xfe, b'z']);
    let output = run(vec!["args".into(), non_utf8, "".into(), "two words".into()]);
    let mut expected = executable().as_os_str().as_encoded_bytes().to_vec();
    expected.extend_from_slice(b"\nargs\na\xff\xfez\n\ntwo words\ncleanup\n");
    assert_eq!(output.stdout, expected);
    assert_eq!(output.stderr, b"");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn environment_reaches_startup_byte_for_byte() {
    let output = Command::new(executable())
        .arg("env")
        .env_clear()
        .env("RILL_FIRST", "one")
        .env("RILL_SECOND", OsString::from_vec(vec![0xff, b'=', b'x']))
        .output()
        .unwrap();
    let mut lines: Vec<&[u8]> = output.stdout.split(|byte| *byte == b'\n').collect();
    lines.sort();
    assert_eq!(
        lines,
        [
            &b""[..],
            b"RILL_FIRST=one",
            b"RILL_SECOND=\xff=x",
            b"cleanup"
        ]
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn allocator_functions_allocate_grow_and_release() {
    let output = run(mode("alloc"));
    assert_eq!(output.stdout, b"kept\naligned\ncleanup\n");
    assert_eq!(output.stderr, b"");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn startup_and_cleanup_run_inside_a_one_mebibyte_region() {
    let output = run_checking(mode("stack"));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "startup inside 1048576\nbuffer inside\ncleanup inside 1048576\ncleanup\n"
    );
    assert_eq!(output.stderr, b"live allocations: 0\n");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn the_byte_below_the_region_cannot_be_read() {
    let output = run_checking(mode("below"));
    assert_eq!(output.stdout, b"lowest byte readable\n");
    let signal = output.status.signal();
    assert!(
        signal == Some(libc::SIGSEGV) || signal == Some(libc::SIGBUS),
        "{:?}",
        output.status
    );
}

#[test]
fn the_product_archive_reports_nothing_even_with_a_live_allocation() {
    let output = run(mode("leak"));
    assert_eq!(output.stdout, b"cleanup\n");
    assert_eq!(output.stderr, b"");
    assert_eq!(output.status.code(), Some(0));
    let output = run(mode("leak-panic"));
    assert_eq!(output.stderr, b"panic: holding an allocation\n");
    assert_eq!(output.status.code(), Some(101));
}

#[test]
fn the_checking_allocator_reports_zero_after_every_block_is_released() {
    for arguments in [mode("write"), mode("alloc"), mode("stack")] {
        let output = run_checking(arguments);
        assert_eq!(output.stderr, b"live allocations: 0\n");
        assert_eq!(output.status.code(), Some(0));
    }
}

#[test]
fn the_checking_allocator_counts_a_grown_block_once() {
    let output = run_checking(mode("leak"));
    assert_eq!(output.stdout, b"cleanup\n");
    assert_eq!(output.stderr, b"live allocations: 1\n");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn the_checking_allocator_reports_after_the_panic_line() {
    let output = run_checking(mode("leak-panic"));
    assert_eq!(
        output.stderr,
        b"panic: holding an allocation\nlive allocations: 1\n"
    );
    assert_eq!(output.stdout, b"");
    assert_eq!(output.status.code(), Some(101));
    let output = run_checking(mode("panic"));
    assert_eq!(
        output.stderr,
        b"panic: something went wrong\nlive allocations: 0\n"
    );
    assert_eq!(output.status.code(), Some(101));
}

fn defined_symbols(archive: &Path) -> String {
    let mut nm = Command::new("nm");
    nm.arg("-g");
    nm.arg(if cfg!(target_os = "macos") {
        "-U"
    } else {
        "--defined-only"
    });
    String::from_utf8_lossy(&nm.arg(archive).output().expect("nm").stdout).into_owned()
}

#[test]
fn the_product_archive_exports_no_counting_allocator_and_no_test_support_symbol() {
    let product = defined_symbols(&static_library(false));
    let checking = defined_symbols(&static_library(true));
    for name in ["rill_rt_entry_stack_region", "counting"] {
        assert!(
            checking.contains(name),
            "the checking archive lists no {name}"
        );
        assert!(
            !product.contains(name),
            "the product archive lists {name}:\n{product}"
        );
    }
    for name in ["rill_rt_allocate", "rill_rt_write_line", "main"] {
        assert!(
            product.contains(name),
            "the product archive lists no {name}"
        );
    }
}
