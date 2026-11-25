use super::roots::assert_limits;
use super::*;
use compiler::analysis::ownership::{
    GraphReport, Place, RootReport, analyze_ownership, lower_root,
};
use compiler::analysis::scalars::BodyOwner;

const HELLO_WORLD: &str = "import std::log;\n\nlog(\"hello world\");\n";

fn program(test: &str, source: &str, verify: impl Fn(&loader::Graph, &TypedReceipt<'_, '_>)) {
    programs(test, &[source], |_, graph, receipt| verify(graph, receipt));
}

fn programs(
    test: &str,
    sources: &[&str],
    verify: impl Fn(usize, &loader::Graph, &TypedReceipt<'_, '_>),
) {
    installed(test, |request| {
        for (index, source) in sources.iter().enumerate() {
            fs::write(&request.entry, source).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let receipt = typed.receipt().expect("typed receipt");
                verify(index, &graph, &receipt);
            }
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
        |_, receipt| {
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
        |_, receipt| {
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
        |_, receipt| {
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
        |_, receipt| {
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

fn module_of<'g>(graph: &'g loader::Graph, key: &loader::ModuleKey) -> &'g loader::Module {
    graph.modules().iter().find(|m| m.key == *key).unwrap()
}

#[test]
fn the_log_string_root_sets_the_graph_depth() {
    program(
        "output::the_log_string_root_sets_the_graph_depth",
        HELLO_WORLD,
        |graph, receipt| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let mut depths = Vec::new();
            for (prefix, triple) in [
                ("implement log(text: string)", (2367, 126, 6)),
                ("implement log(text: &string)", (2364, 124, 5)),
                ("implement panic(message: string)", (2330, 103, 5)),
                ("implement panic(message: &string)", (2327, 101, 4)),
            ] {
                let root = output_root(&report, prefix);
                let module = module_of(graph, root.site().module);
                let targeted =
                    lower_root(receipt, module, root.owner(), AnalysisLimits::default()).unwrap();
                assert_eq!(targeted.completion(), Completion::Checked, "{prefix}");
                assert_limits(receipt, module, root.owner(), &targeted, triple);
                depths.push(triple.2);
            }
            assert_eq!(depths, [6, 5, 5, 4]);
            assert_eq!(report.statistics().max_depth, 6);
        },
    );
}

fn startup_report<'g>(
    graph: &'g loader::Graph,
    receipt: &TypedReceipt<'_, 'g>,
) -> (
    &'g loader::Module,
    compiler::analysis::ownership::Report<'g>,
) {
    let module = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
        .unwrap();
    let report = lower_root(
        receipt,
        module,
        BodyOwner::Startup,
        AnalysisLimits::default(),
    )
    .unwrap();
    (module, report)
}

type Triple = (usize, usize, usize);

const SIBLINGS: [(&str, &str, &str, Triple, Triple); 7] = [
    (
        "piped",
        "import std::log;\n\"x\" |> log();\n",
        "\"x\" |> log()",
        (1898, 7, 3),
        (1898, 7, 3),
    ),
    (
        "piped group",
        "import std::log;\n(\"x\") |> log();\n",
        "(\"x\") |> log()",
        (1901, 8, 3),
        (1901, 8, 3),
    ),
    (
        "piped scope",
        "import std::log;\n{ \"x\" |> log(); }\n",
        "\"x\" |> log()",
        (1902, 8, 4),
        (1902, 8, 4),
    ),
    (
        "panic",
        "import std::panic;\npanic(\"boom\");\n",
        "panic(\"boom\")",
        (1892, 5, 3),
        (1896, 7, 3),
    ),
    (
        "panic group",
        "import std::panic;\npanic((\"boom\"));\n",
        "panic((\"boom\"))",
        (1895, 6, 3),
        (1899, 8, 3),
    ),
    (
        "panic scope",
        "import std::panic;\n{ panic(\"boom\"); }\n",
        "panic(\"boom\")",
        (1896, 6, 4),
        (1900, 8, 4),
    ),
    (
        "panic borrow",
        "import std::panic;\nlet s = \"x\";\npanic(&s);\n",
        "panic(&s)",
        (1907, 15, 3),
        (1911, 17, 3),
    ),
];

#[test]
fn unsupported_output_siblings_stay_incomplete_at_their_call_and_pay_nothing_more() {
    let sources: Vec<String> = SIBLINGS
        .iter()
        .map(|(_, body, ..)| format!("{}\n{body}", super::IMPORTS))
        .collect();
    let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
    programs(
        "output::unsupported_output_siblings_stay_incomplete_at_their_call_and_pay_nothing_more",
        &sources,
        |index, graph, receipt| {
            let (name, _, blocker, triple, parent) = SIBLINGS[index];
            let (module, report) = startup_report(graph, receipt);
            assert_eq!(report.completion(), Completion::Incomplete, "{name}");
            assert_eq!(
                report.blocked_at().unwrap().span.bytes(),
                blocker.as_bytes(),
                "{name}"
            );
            assert_limits(receipt, module, BodyOwner::Startup, &report, triple);
            let at_parent = AnalysisLimits {
                max_work: parent.0,
                max_entries: parent.1,
                max_depth: parent.2,
            };
            let parent_report = lower_root(receipt, module, BodyOwner::Startup, at_parent).unwrap();
            assert_eq!(parent_report.completion(), Completion::Incomplete, "{name}");
            assert_eq!(
                parent_report.blocked_at().unwrap().span.bytes(),
                blocker.as_bytes(),
                "{name}"
            );
            assert!(parent_report.mir().is_none(), "{name}");
        },
    );
}

const GROUPED: [(&str, &str, (usize, usize, usize)); 6] = [
    ("literal", "import std::log;\nlog(\"x\");\n", (4136, 104, 5)),
    (
        "literal group",
        "import std::log;\nlog((\"x\"));\n",
        (4142, 105, 6),
    ),
    (
        "literal scope",
        "import std::log;\n{ log(\"x\"); }\n",
        (4141, 105, 6),
    ),
    (
        "borrow",
        "import std::log;\nlet s = \"x\";\nlog(&s);\n",
        (4246, 155, 6),
    ),
    (
        "borrow groups",
        "import std::log;\nlet s = \"x\";\nlog(((&s)));\n",
        (4258, 157, 8),
    ),
    (
        "move",
        "import std::log;\nlet s = \"x\";\nlog(s);\n",
        (4236, 152, 5),
    ),
];

#[test]
fn grouped_and_scoped_log_calls_add_depth_once_per_layer_and_keep_their_receipt() {
    let sources: Vec<String> = GROUPED
        .iter()
        .map(|(_, body, _)| format!("{}\n{body}", super::IMPORTS))
        .collect();
    let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
    programs(
        "output::grouped_and_scoped_log_calls_add_depth_once_per_layer_and_keep_their_receipt",
        &sources,
        |index, graph, receipt| {
            let (name, _, triple) = GROUPED[index];
            let (module, targeted) = startup_report(graph, receipt);
            assert_eq!(targeted.completion(), Completion::Incomplete, "{name}");
            assert_eq!(
                targeted.blocked_at().unwrap().module.path,
                "output.rill",
                "{name}"
            );
            assert_limits(receipt, module, BodyOwner::Startup, &targeted, triple);
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(startup(&report).completion(), Completion::Checked, "{name}");
            let depth = |label: &str| GROUPED.iter().find(|row| row.0 == label).unwrap().2.2;
            assert_eq!(depth("literal group"), depth("literal") + 1);
            assert_eq!(depth("literal scope"), depth("literal") + 1);
            assert_eq!(depth("borrow groups"), depth("borrow") + 2);
        },
    );
}

fn loads_output(graph: &loader::Graph) -> bool {
    graph
        .modules()
        .iter()
        .any(|m| m.key.origin == Origin::Standard && m.key.path == "output.rill")
}

#[test]
fn output_rill_loads_only_when_a_program_imports_log_or_panic() {
    const CASES: [(&str, bool); 9] = [
        ("", false),
        ("import std::{Result};\n", false),
        ("import std::{Option, Result};\n", false),
        ("import std::option;\n", false),
        ("import std::log;\n", true),
        ("import std::panic;\n", true),
        ("import std::{log, panic};\n", true),
        ("import std::{Result, log};\n", true),
        ("import std;\n", true),
    ];
    installed(
        "output::output_rill_loads_only_when_a_program_imports_log_or_panic",
        |request| {
            for (source, loads) in CASES {
                fs::write(&request.entry, source).unwrap();
                for test_mode in [false, true] {
                    let request = InstalledRequest {
                        test_mode,
                        ..request.clone()
                    };
                    let graph = loader::load_installed(request).unwrap();
                    assert_eq!(
                        loads_output(&graph),
                        loads,
                        "{source:?} test_mode {test_mode}"
                    );
                    let standard = graph
                        .modules()
                        .iter()
                        .filter(|m| m.key.origin == Origin::Standard)
                        .count();
                    assert_eq!(standard, 17 + usize::from(loads), "{source:?}");
                }
            }
        },
    );
}

#[test]
fn importing_log_adds_exactly_the_declarations_of_output_rill() {
    installed(
        "output::importing_log_adds_exactly_the_declarations_of_output_rill",
        |request| {
            fs::write(&request.entry, "").unwrap();
            let graph = loader::load_installed(request.clone()).unwrap();
            let without = graph.declaration_count();
            fs::write(&request.entry, "import std::log;\n").unwrap();
            let graph = loader::load_installed(request).unwrap();
            assert_eq!(graph.declaration_count() - without, 18);
        },
    );
}
