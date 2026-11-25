//! The backend lowers every shape the compilable-program rule admits.
//!
//! `Shape::ALL` is the compiler's statement of what the backend can lower.
//! Each program below is compiled through the rule and the backend, and the
//! shapes the rule finds in it are checked against the ones the program is
//! here to cover; the last test fails when a shape in `Shape::ALL` has no
//! program. If the backend cannot lower a shape, remove it from `Shape::ALL`
//! (and from the classifiers in `compiler/src/analysis/compilable/capability.rs`)
//! rather than from this table.
mod support;

use compiler::analysis::compilable::capability::Shape;
use std::collections::BTreeSet;

const LOG: &str = "import std::log;\n";

struct Program {
    test: &'static str,
    source: String,
    covers: &'static [Shape],
}

/// One program per row; each row's `test` is the name of the test that
/// compiles it, because the front end runs in a child copy of that test.
fn programs() -> Vec<Program> {
    vec![
        Program {
            test: "a_string_literal_is_initialized_and_logged",
            source: format!("{LOG}log(\"hello\");\n"),
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
            source: format!("{LOG}function hi(s: &string): void {{ log(s); }}\nhi(&\"through\");\n"),
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
            covers: &[Shape::MoveInto],
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
    assert_eq!(output.status.code(), Some(0), "{output:?}");
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
fn every_shape_the_rule_admits_has_a_program() {
    let covered: BTreeSet<Shape> = programs()
        .iter()
        .flat_map(|program| program.covers.iter().copied())
        .collect();
    let admitted: BTreeSet<Shape> = Shape::ALL.iter().copied().collect();
    assert_eq!(covered, admitted);
    assert_eq!(Shape::ALL.len(), admitted.len(), "a shape is listed twice");
}
