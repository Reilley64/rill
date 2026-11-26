mod support;

use compiler::analysis::compilable::capability::Shape;
use std::collections::BTreeSet;

const LOG: &str = "import std::log;\n";
const PANIC: &str = "import std::panic;\n";
const APPEND: &str = "import std::string::append::append;\n";

struct Program {
    test: &'static str,
    source: String,
    status: i32,
    stderr: &'static str,
    covers: &'static [Shape],
}

fn programs() -> Vec<Program> {
    vec![
        Program {
            test: "a_string_literal_is_initialized_and_logged",
            source: format!("{LOG}log(\"hello\");\n"),
            status: 0,
            stderr: "",
            covers: &[
                Shape::Goto,
                Shape::Return,
                Shape::FinishStartup,
                Shape::Nothing,
                Shape::InitializeText,
                Shape::BorrowShared,
                Shape::Call,
                Shape::WriteLine,
                Shape::Cleanup,
                Shape::DropText,
                Shape::TextParameter,
                Shape::VoidResult,
            ],
        },
        Program {
            test: "a_view_crosses_a_function_into_log",
            source: format!(
                "{LOG}function hi(s: &string): void {{ log(s); }}\nhi(&\"through\");\n"
            ),
            status: 0,
            stderr: "",
            covers: &[
                Shape::BorrowStaticText,
                Shape::CopyInto,
                Shape::EndStorage,
                Shape::TextViewParameter,
            ],
        },
        Program {
            test: "a_named_string_is_moved_into_log",
            source: format!("{LOG}let s = \"moved\";\nlog(s);\n"),
            status: 0,
            stderr: "",
            covers: &[Shape::MoveInto],
        },
        Program {
            test: "an_exclusive_string_grows_through_append",
            source: format!("{LOG}{APPEND}let mut s = \"a\";\nappend(&mut s, &\"b\");\nlog(&s);\n"),
            status: 0,
            stderr: "",
            covers: &[
                Shape::BorrowExclusive,
                Shape::ReborrowExclusive,
                Shape::AppendText,
                Shape::ExclusiveTextParameter,
            ],
        },
        Program {
            test: "a_panic_ends_the_program_with_status_101",
            source: format!("{PANIC}panic(\"x\");\n"),
            status: 101,
            stderr: "panic: x\n",
            covers: &[Shape::Panic, Shape::NeverResult],
        },
    ]
}

fn lowers(test: &str) {
    let program = programs()
        .into_iter()
        .find(|program| program.test == test)
        .expect("a program for this test");
    let Some(built) = support::emit(test, &program.source) else {
        return;
    };
    assert_eq!(built.error, None);
    assert!(built.object().exists());
    let output = built.run();
    assert_eq!(output.status.code(), Some(program.status), "{output:?}");
    assert_eq!(output.stderr, program.stderr.as_bytes(), "{output:?}");
    let found: BTreeSet<_> = built.shapes.iter().map(String::as_str).collect();
    for shape in program.covers {
        assert!(
            found.contains(format!("{shape:?}").as_str()),
            "{shape:?} is not in {found:?}"
        );
    }
}

#[test]
fn a_string_literal_is_initialized_and_logged() {
    lowers("a_string_literal_is_initialized_and_logged");
}

#[test]
fn a_view_crosses_a_function_into_log() {
    lowers("a_view_crosses_a_function_into_log");
}

#[test]
fn a_named_string_is_moved_into_log() {
    lowers("a_named_string_is_moved_into_log");
}

#[test]
fn an_exclusive_string_grows_through_append() {
    lowers("an_exclusive_string_grows_through_append");
}

#[test]
fn a_panic_ends_the_program_with_status_101() {
    lowers("a_panic_ends_the_program_with_status_101");
}

#[test]
fn every_shape_the_rule_admits_has_a_program() {
    let covered: BTreeSet<Shape> = programs()
        .iter()
        .flat_map(|program| program.covers.iter().copied())
        .collect();
    let admitted: BTreeSet<Shape> = Shape::ALL.iter().copied().collect();
    assert_eq!(covered, admitted);
    assert_eq!(Shape::ALL.len(), admitted.len(), "a shape is listed twice");
}
