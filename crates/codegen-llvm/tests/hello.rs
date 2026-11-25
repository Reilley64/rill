mod support;

use std::path::Path;
use std::process::Command;

const LOG: &str = "import std::log;\n";

fn run(test: &str, source: &str) -> Option<std::process::Output> {
    let built = support::emit(test, source)?;
    assert_eq!(built.error, None);
    Some(built.run())
}

#[test]
fn log_hello_world_prints_the_line_and_exits_zero() {
    let Some(output) = run(
        "log_hello_world_prints_the_line_and_exits_zero",
        "import std::log;\n\nlog(\"hello world\");\n",
    ) else {
        return;
    };
    assert_eq!(output.stdout, b"hello world\n");
    assert_eq!(output.stderr, b"");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn a_borrowed_local_is_logged_and_released_by_startup() {
    let Some(output) = run(
        "a_borrowed_local_is_logged_and_released_by_startup",
        &format!("{LOG}let s = \"borrowed\";\nlog(&s);\nlog(&s);\n"),
    ) else {
        return;
    };
    assert_eq!(output.stdout, b"borrowed\nborrowed\n");
    assert_eq!(output.stderr, b"");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn a_moved_local_is_logged_once() {
    let Some(output) = run(
        "a_moved_local_is_logged_once",
        &format!("{LOG}let s = \"moved\";\nlog(s);\n"),
    ) else {
        return;
    };
    assert_eq!(output.stdout, b"moved\n");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn a_view_of_a_literal_crosses_a_user_function_into_log() {
    let Some(output) = run(
        "a_view_of_a_literal_crosses_a_user_function_into_log",
        &format!("{LOG}function hi(s: &string): void {{ log(s); }}\nhi(&\"through\");\n"),
    ) else {
        return;
    };
    assert_eq!(output.stdout, b"through\n");
    assert_eq!(output.stderr, b"");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn a_void_value_bound_to_a_name_lowers_to_nothing() {
    let Some(output) = run(
        "a_void_value_bound_to_a_name_lowers_to_nothing",
        &format!("{LOG}let r = log(\"bound\");\n"),
    ) else {
        return;
    };
    assert_eq!(output.stdout, b"bound\n");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn literals_keep_their_decoded_bytes_in_order() {
    let Some(output) = run(
        "literals_keep_their_decoded_bytes_in_order",
        &format!("{LOG}log(\"a\");\nlog(\"\\u{{e9}}\\t\\\\\");\nlog((\"\"));\nlog(\"a\");\n"),
    ) else {
        return;
    };
    assert_eq!(output.stdout, "a\n\u{e9}\t\\\n\na\n".as_bytes());
    assert_eq!(output.stderr, b"");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn the_object_exports_the_startup_symbols_and_the_allocation_flag_and_nothing_else() {
    let Some(built) = support::emit(
        "the_object_exports_the_startup_symbols_and_the_allocation_flag_and_nothing_else",
        "import std::log;\nlog(\"hello world\");\n",
    ) else {
        return;
    };
    assert_eq!(built.error, None);
    let mut defined = defined_symbols(&built.object());
    defined.sort();
    assert_eq!(
        defined,
        [
            "rill_check_allocations",
            "rill_startup",
            "rill_startup_cleanup"
        ]
    );
}

fn defined_symbols(object: &Path) -> Vec<String> {
    let mut nm = Command::new("nm");
    nm.arg("-g");
    if cfg!(target_os = "macos") {
        nm.arg("-U");
    } else {
        nm.arg("--defined-only");
    }
    let output = nm.arg(object).output().expect("nm");
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .map(|name| {
            name.strip_prefix('_')
                .filter(|_| cfg!(target_os = "macos"))
                .unwrap_or(name)
                .to_owned()
        })
        .collect()
}
