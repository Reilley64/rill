use super::roots::assert_limits;
use super::*;
use compiler::analysis::ownership::{
    GraphReport, Place, RootReport, analyze_ownership, lower_root,
};
use compiler::analysis::scalars::BodyOwner;

const HELLO_WORLD: &str = "import std::log;\n\nlog(\"hello world\");\n";

fn program(
    test: &str,
    source: &str,
    verify: impl Fn(&InstalledRequest, &loader::Graph, &TypedReceipt<'_, '_>),
) {
    installed(test, |request| {
        fs::write(&request.entry, source).unwrap();
        for test_mode in [false, true] {
            let request = InstalledRequest {
                test_mode,
                ..request.clone()
            };
            let graph = loader::load_installed(request.clone()).unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            let receipt = typed.receipt().expect("typed receipt");
            verify(&request, &graph, &receipt);
        }
    });
}

fn text(place: &Place<'_>) -> String {
    String::from_utf8_lossy(place.root.span.bytes()).into_owned()
}

fn declaration(report: &GraphReport<'_>, id: &loader::DeclarationId) -> String {
    let root = report
        .roots()
        .iter()
        .find(|root| root.owner() == BodyOwner::Declaration(id))
        .unwrap();
    let source = String::from_utf8_lossy(root.site().span.bytes()).into_owned();
    source[..source.find(" {").unwrap()].to_owned()
}

fn sequence(report: &GraphReport<'_>, mir: &Mir<'_>) -> Vec<String> {
    let blocks = straight_blocks(mir, mir.entry);
    let mut events = Vec::new();
    for block in blocks {
        match &block.operation {
            Operation::None => {}
            Operation::Initialize(place) => events.push(format!("init {}", text(place))),
            Operation::Borrow { source, holder, .. } => {
                events.push(format!("borrow {} in {}", text(source), text(holder)))
            }
            Operation::Move { from, into } => events.push(format!(
                "move {} into {}",
                text(from),
                text(into.as_ref().unwrap())
            )),
            Operation::Copy { from, into } => events.push(format!(
                "copy {} into {}",
                text(from),
                text(into.as_ref().unwrap())
            )),
            Operation::Call(id) => events.push(format!("call {}", declaration(report, id))),
            Operation::WriteLine { text: place } => {
                events.push(format!("write-line {}", text(place)))
            }
            Operation::Panic { message } => events.push(format!("panic {}", text(message))),
            Operation::Cleanup(actions) => {
                for action in actions {
                    events.push(match action {
                        Obligation::EndStorage(place) => format!("end {}", text(place)),
                        Obligation::Drop(place) => format!("drop {}", text(place)),
                        other => panic!("unexpected cleanup {other:?}"),
                    });
                }
            }
            other => panic!("unexpected operation {:.80}", format!("{other:?}")),
        }
    }
    events
}

fn startup<'a, 'g>(report: &'a GraphReport<'g>) -> &'a RootReport<'g> {
    report
        .roots()
        .iter()
        .find(|root| root.owner() == BodyOwner::Startup && root.site().module.path == "main.rill")
        .unwrap()
}

fn output_root<'a, 'g>(report: &'a GraphReport<'g>, prefix: &str) -> &'a RootReport<'g> {
    report
        .roots()
        .iter()
        .find(|root| {
            root.site().module.origin == Origin::Standard
                && root.site().module.path == "output.rill"
                && root.site().span.bytes().starts_with(prefix.as_bytes())
        })
        .unwrap()
}

#[test]
fn r01_r03_hello_world_moves_the_literal_into_log_and_startup_disposes_nothing() {
    program(
        "output::r01_r03_hello_world_moves_the_literal_into_log_and_startup_disposes_nothing",
        HELLO_WORLD,
        |_, _, receipt| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let root = startup(&report);
            assert_eq!(root.completion(), Completion::Checked);
            let mir = root.mir().unwrap();
            assert_eq!(
                sequence(&report, mir),
                [
                    "init \"hello world\"",
                    "call implement log(text: string): void",
                ]
            );
            assert_eq!(
                straight_blocks(mir, mir.entry).last().unwrap().terminator,
                Terminator::FinishStartup
            );
        },
    );
}

#[test]
fn r01_r03_a_borrowed_text_stays_owned_by_startup_and_is_dropped_by_it() {
    program(
        "output::r01_r03_a_borrowed_text_stays_owned_by_startup_and_is_dropped_by_it",
        "import std::log;\nlet s = \"x\";\nlog(&s);\n",
        |_, _, receipt| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let root = startup(&report);
            assert_eq!(root.completion(), Completion::Checked);
            assert_eq!(
                sequence(&report, root.mir().unwrap()),
                [
                    "init s",
                    "borrow s in &s",
                    "call implement log(text: &string): void",
                    "drop s",
                ]
            );
        },
    );
}

#[test]
fn r01_r03_a_moved_text_leaves_nothing_for_startup_to_drop() {
    program(
        "output::r01_r03_a_moved_text_leaves_nothing_for_startup_to_drop",
        "import std::log;\nlet s = \"x\";\nlog(s);\n",
        |_, _, receipt| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let root = startup(&report);
            assert_eq!(root.completion(), Completion::Checked);
            assert_eq!(
                sequence(&report, root.mir().unwrap()),
                [
                    "init s",
                    "move s into s",
                    "call implement log(text: string): void",
                ]
            );
        },
    );
}

#[test]
fn the_four_output_roots_lower_to_their_operations_in_order() {
    program(
        "output::the_four_output_roots_lower_to_their_operations_in_order",
        HELLO_WORLD,
        |_, _, receipt| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            for (prefix, expected) in [
                (
                    "implement log(text: string)",
                    vec![
                        "borrow text: string in &text",
                        "write-line &text",
                        "drop text: string",
                    ],
                ),
                (
                    "implement log(text: &string)",
                    vec![
                        "copy text: &string into text",
                        "write-line text",
                        "end text: &string",
                    ],
                ),
                (
                    "implement panic(message: string)",
                    vec![
                        "borrow message: string in &message",
                        "panic &message",
                        "drop message: string",
                    ],
                ),
                (
                    "implement panic(message: &string)",
                    vec![
                        "copy message: &string into message",
                        "panic message",
                        "end message: &string",
                    ],
                ),
            ] {
                let root = output_root(&report, prefix);
                assert_eq!(root.completion(), Completion::Checked, "{prefix}");
                assert!(root.dependencies().is_empty(), "{prefix}");
                assert_eq!(sequence(&report, root.mir().unwrap()), expected, "{prefix}");
            }
        },
    );
}
