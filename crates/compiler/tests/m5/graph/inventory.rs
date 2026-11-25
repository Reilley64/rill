//! The graph root inventory of the 17- and 18-source installed captures.
//!
//! The ledger section "Graph root inventory at the integration tip" quotes these counts. The
//! 17-source capture is `tests/installed/typed`; the 18-source capture adds `std::routine::join`.
//! Each root's outcome is asserted by name, so a ticket that flips one root changes one row.
use super::*;
use std::collections::BTreeMap;

const BASELINE_MAIN: &str = include_str!("../../../../../tests/installed/typed/main.rill");
const BASELINE_CHECKS: &str = include_str!("../../../../../tests/installed/typed/checks.rill");
const JOIN: &str = r#"
import std::routine::join;
function joined(input: List<i32>): [i32, i32] {
    let numbers = input;
    let [first, second] = join([() => (&numbers)[0u64], () => (&numbers)[1u64]]);
    dispose(numbers);
    return [first, second];
}
function joinedOrder(): bool {
    let input: List<i32> = [13i32, 29i32];
    let pair = joined(input);
    return pair[0] == 13i32 && pair[1] == 29i32;
}
expect joinedOrder();
"#;

/// Blocker families of the incomplete standard roots (gap-analysis §1.5), by owning module.
const FAMILIES: [(&str, &[&str]); 6] = [
    (
        "core text and comparison",
        &["char.rill", "compare.rill", "core.rill", "number.rill"],
    ),
    (
        "bytes, conversions and string",
        &["bytes.rill", "convert.rill", "string.rill"],
    ),
    ("List and iterator", &["collection.rill", "iter.rill"]),
    (
        "native and protected wrappers",
        &["channel.rill", "mutex.rill", "shared.rill"],
    ),
    ("generic Option/Result", &["option.rill", "result.rill"]),
    (
        "diagnostics text and formatting",
        &["error.rill", "format.rill"],
    ),
];

struct Capture {
    name: &'static str,
    main: String,
    standard_modules: usize,
    /// M4 typed work, entries and depth: the fixed caps.
    typed: [usize; 3],
    graph: [usize; 3],
    /// The graph meter before the i32 equality source root (ri-785.9); it
    /// now gives `ResourceLimit` (metering rule 11).
    pre_equality_graph: [usize; 3],
    /// The graph meter before `joinedOrder` and its expectation became checked
    /// roots (ri-e9t8, metering rule 11); it now gives `ResourceLimit`. Only the
    /// capture with the O16 source has one.
    before_o16_graph: Option<[usize; 3]>,
    app_roots: &'static [(&'static str, &'static str, &'static str, Completion)],
    /// Standard module, checked roots, incomplete roots.
    standard: &'static [(&'static str, usize, usize)],
}

use Completion::{Checked, Incomplete};

const APP_BASELINE: &[(&str, &str, &str, Completion)] = &[
    ("checks.rill", "Startup", "-", Checked),
    ("checks.rill", "Function", "ready", Checked),
    ("checks.rill", "Expectation", "expect ready();", Checked),
    ("main.rill", "Startup", "-", Incomplete),
    ("main.rill", "Implementation", "-", Incomplete),
    ("main.rill", "Function", "usesOrdinary", Incomplete),
    ("main.rill", "Function", "convertAndShow", Incomplete),
    ("main.rill", "Function", "visit", Incomplete),
    ("main.rill", "Function", "listLifecycle", Incomplete),
    ("main.rill", "Function", "retain", Incomplete),
    ("main.rill", "Function", "releaseNative", Incomplete),
    ("main.rill", "Function", "inspectMapped", Incomplete),
    ("main.rill", "Function", "mapSuccess", Incomplete),
    ("main.rill", "Function", "parse", Incomplete),
    ("main.rill", "Function", "wrap", Incomplete),
    ("main.rill", "Function", "render", Incomplete),
    ("main.rill", "Expectation", "expect ready();", Checked),
    (
        "main.rill",
        "Expectation",
        "expect usesOrdinary(1i32);",
        Incomplete,
    ),
    (
        "main.rill",
        "Expectation",
        "expect isOk(&Result.Ok(true));",
        Incomplete,
    ),
];

const APP_SHARED: &[(&str, &str, &str, Completion)] = &[
    ("checks.rill", "Startup", "-", Checked),
    ("checks.rill", "Function", "ready", Checked),
    ("checks.rill", "Expectation", "expect ready();", Checked),
    ("main.rill", "Startup", "-", Incomplete),
    ("main.rill", "Implementation", "-", Incomplete),
    ("main.rill", "Function", "usesOrdinary", Incomplete),
    ("main.rill", "Function", "convertAndShow", Incomplete),
    ("main.rill", "Function", "visit", Incomplete),
    ("main.rill", "Function", "listLifecycle", Incomplete),
    ("main.rill", "Function", "retain", Incomplete),
    ("main.rill", "Function", "releaseNative", Incomplete),
    ("main.rill", "Function", "inspectMapped", Incomplete),
    ("main.rill", "Function", "mapSuccess", Incomplete),
    ("main.rill", "Function", "parse", Incomplete),
    ("main.rill", "Function", "wrap", Incomplete),
    ("main.rill", "Function", "render", Incomplete),
    ("main.rill", "Expectation", "expect ready();", Checked),
    (
        "main.rill",
        "Expectation",
        "expect usesOrdinary(1i32);",
        Incomplete,
    ),
    (
        "main.rill",
        "Expectation",
        "expect isOk(&Result.Ok(true));",
        Incomplete,
    ),
    ("main.rill", "Function", "joined", Checked),
    ("main.rill", "Function", "joinedOrder", Checked),
    ("main.rill", "Expectation", "expect joinedOrder();", Checked),
];

const STANDARD_BASELINE: &[(&str, usize, usize)] = &[
    ("bytes.rill", 1, 7),
    ("channel.rill", 1, 3),
    ("char.rill", 1, 2),
    ("collection.rill", 1, 4),
    ("compare.rill", 2, 13),
    ("convert.rill", 1, 14),
    ("core.rill", 29, 2),
    ("error.rill", 1, 22),
    ("format.rill", 1, 17),
    ("iter.rill", 1, 9),
    ("mutex.rill", 1, 1),
    ("number.rill", 1, 4),
    ("option.rill", 1, 5),
    ("result.rill", 1, 13),
    ("shared.rill", 1, 2),
    ("std.rill", 1, 0),
    ("string.rill", 1, 1),
];

/// The baseline plus `routine.rill`: its startup checks; the generic `join` wrapper does not.
const STANDARD_SHARED: &[(&str, usize, usize)] = &[
    ("bytes.rill", 1, 7),
    ("channel.rill", 1, 3),
    ("char.rill", 1, 2),
    ("collection.rill", 1, 4),
    ("compare.rill", 2, 13),
    ("convert.rill", 1, 14),
    ("core.rill", 29, 2),
    ("error.rill", 1, 22),
    ("format.rill", 1, 17),
    ("iter.rill", 1, 9),
    ("mutex.rill", 1, 1),
    ("number.rill", 1, 4),
    ("option.rill", 1, 5),
    ("result.rill", 1, 13),
    ("routine.rill", 1, 1),
    ("shared.rill", 1, 2),
    ("std.rill", 1, 0),
    ("string.rill", 1, 1),
];

fn capture_cases() -> [Capture; 2] {
    [
        Capture {
            name: "baseline",
            main: BASELINE_MAIN.into(),
            standard_modules: 17,
            typed: [371_869, 54_059, 12],
            graph: [203_014, 5_005, 5],
            pre_equality_graph: [202_698, 4_871, 3],
            before_o16_graph: None,
            app_roots: APP_BASELINE,
            standard: STANDARD_BASELINE,
        },
        Capture {
            name: "shared",
            main: format!("{BASELINE_MAIN}{JOIN}"),
            standard_modules: 18,
            typed: [495_450, 56_822, 12],
            graph: [244_835, 26_772, 10],
            pre_equality_graph: [243_206, 25_960, 10],
            before_o16_graph: Some([243_522, 26_094, 10]),
            app_roots: APP_SHARED,
            standard: STANDARD_SHARED,
        },
    ]
}

fn name_of(module: &loader::Module, root: &RootReport<'_>) -> String {
    match root.owner() {
        BodyOwner::Startup => "-".into(),
        BodyOwner::Declaration(id) => {
            let declaration = module.declarations.iter().find(|d| &d.id == id).unwrap();
            declaration.name.clone().unwrap_or_else(|| {
                if id.kind == Expectation {
                    String::from_utf8_lossy(root.site().span.bytes()).into()
                } else {
                    "-".into()
                }
            })
        }
    }
}

#[test]
fn installed_captures_state_the_graph_root_inventory() {
    installed(
        "graph::inventory::installed_captures_state_the_graph_root_inventory",
        |request| {
            for case in capture_cases() {
                fs::write(&request.entry, &case.main).unwrap();
                fs::write(request.entry.with_file_name("checks.rill"), BASELINE_CHECKS).unwrap();
                let mut modes = Vec::new();
                for test_mode in [false, true] {
                    let request = InstalledRequest {
                        test_mode,
                        ..request.clone()
                    };
                    let graph = loader::load_installed(request).unwrap();
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    let receipt = typed.receipt().expect("installed M4 receipt");
                    let used = typed.statistics();
                    assert_eq!(
                        [used.work, used.entries, used.max_depth],
                        case.typed,
                        "{}: M4 caps",
                        case.name
                    );
                    assert_eq!(
                        graph
                            .modules()
                            .iter()
                            .filter(|m| m.key.origin == Origin::Standard)
                            .count(),
                        case.standard_modules
                    );
                    let report = analyze_ownership(&receipt, AnalysisLimits::default());
                    assert_eq!(report.test_mode(), test_mode);
                    assert_eq!(report.completion(), GraphCompletion::Incomplete);
                    assert!(report.inventory_complete());
                    assert!(report.findings().is_empty());
                    assert!(report.active_disposers().is_empty());
                    assert_eq!(
                        report.open_obligations(),
                        &[
                            GraphObligation::IntrinsicCancellation,
                            GraphObligation::MaskingAndJoin,
                            GraphObligation::EmbeddedAndUnreachedSource,
                        ]
                    );
                    // An incomplete root has a blocker and no MIR; a checked root has MIR.
                    for root in report.roots() {
                        let checked = root.completion() == Checked;
                        assert_eq!(root.ordinary_completion(), root.completion());
                        assert!(matches!(root.completion(), Checked | Incomplete));
                        assert_eq!(root.mir().is_some(), checked);
                        assert_eq!(root.blocked_at().is_some(), !checked);
                    }

                    let mut app = Vec::new();
                    let mut standard: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
                    for root in report.roots() {
                        let key = root.site().module;
                        let module = graph.modules().iter().find(|m| &m.key == key).unwrap();
                        let checked = root.completion() == Checked;
                        if key.origin == Origin::Application {
                            let kind = match root.owner() {
                                BodyOwner::Startup => "Startup".to_string(),
                                BodyOwner::Declaration(id) => format!("{:?}", id.kind),
                            };
                            app.push((
                                key.path.clone(),
                                kind,
                                name_of(module, root),
                                root.completion(),
                            ));
                        } else {
                            let counts = standard.entry(key.path.as_str()).or_default();
                            if checked {
                                counts.0 += 1;
                            } else {
                                counts.1 += 1;
                            }
                        }
                    }
                    let app: Vec<_> = app
                        .iter()
                        .map(|(p, k, n, c)| (p.as_str(), k.as_str(), n.as_str(), *c))
                        .collect();
                    assert_eq!(app, case.app_roots, "{}: application roots", case.name);
                    let standard: Vec<_> =
                        standard.into_iter().map(|(p, (c, i))| (p, c, i)).collect();
                    assert_eq!(standard, case.standard, "{}: standard roots", case.name);

                    let families: Vec<_> = FAMILIES
                        .iter()
                        .map(|(name, modules)| {
                            (
                                *name,
                                standard
                                    .iter()
                                    .filter(|(path, ..)| modules.contains(path))
                                    .map(|(.., incomplete)| incomplete)
                                    .sum::<usize>(),
                            )
                        })
                        .collect();
                    assert_eq!(
                        families,
                        [
                            ("core text and comparison", 21),
                            ("bytes, conversions and string", 22),
                            ("List and iterator", 13),
                            ("native and protected wrappers", 6),
                            ("generic Option/Result", 18),
                            ("diagnostics text and formatting", 39),
                        ],
                        "{}: incomplete standard roots by family",
                        case.name
                    );

                    let used = report.statistics();
                    assert_eq!(
                        [used.work, used.entries, used.max_depth],
                        case.graph,
                        "{}: graph meter",
                        case.name
                    );
                    let exact = AnalysisLimits {
                        max_work: used.work,
                        max_entries: used.entries,
                        max_depth: used.max_depth,
                    };
                    let again = analyze_ownership(&receipt, exact);
                    assert_eq!(again.completion(), GraphCompletion::Incomplete);
                    assert_eq!(again.statistics(), used);
                    let [max_work, max_entries, max_depth] = case.pre_equality_graph;
                    let before_o16 =
                        case.before_o16_graph
                            .map(|[max_work, max_entries, max_depth]| AnalysisLimits {
                                max_work,
                                max_entries,
                                max_depth,
                            });
                    for short in before_o16.into_iter().chain([
                        AnalysisLimits {
                            max_work,
                            max_entries,
                            max_depth,
                        },
                        AnalysisLimits {
                            max_work: exact.max_work - 1,
                            ..exact
                        },
                        AnalysisLimits {
                            max_entries: exact.max_entries - 1,
                            ..exact
                        },
                        AnalysisLimits {
                            max_depth: exact.max_depth - 1,
                            ..exact
                        },
                    ]) {
                        let limited = analyze_ownership(&receipt, short);
                        crate::limit::assert_graph_limit(&limited);
                        assert!(limited.inventory_complete());
                    }
                    modes.push((app.len(), standard.len()));
                }
                assert_eq!(modes[0], modes[1]);
            }
        },
    );
}
