//! Replays the published fixed work/entries/depth caps (gap-analysis row H1).
//!
//! `legacy-caps.tsv` is the only record of those caps: one row per published
//! cap triple. Each row's query runs at its exact cap, which must reproduce the
//! row's outcome and statistics, and again one short on each axis, which must
//! stop with `ResourceLimit` and publish nothing. Moving a cap is a one-line
//! change to that file. The fixtures are in `legacy-caps/<fixture>/`.
//!
//! This is LLVM-free host static analysis of ownership; no Rill program runs.
use super::*;
use compiler::analysis::ownership::{GraphCompletion, analyze_ownership, lower_body, lower_root};
use compiler::analysis::scalars::BodyOwner;
use compiler::analysis::{AnalysisLimits, Statistics};
use std::collections::BTreeMap;

const MANIFEST: &str = include_str!("legacy-caps.tsv");

#[test]
fn replays_every_published_cap_triple() {
    installed(
        "legacy_caps::replays_every_published_cap_triple",
        |request| {
            let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/m5/legacy-caps");
            let app = request.entry.parent().unwrap();
            let mut rows = BTreeMap::<&str, Vec<Row>>::new();
            for (number, line) in MANIFEST.lines().enumerate() {
                if !line.starts_with('#') {
                    let row = Row::parse(line, number + 1);
                    rows.entry(row.fixture).or_default().push(row);
                }
            }
            for entry in fs::read_dir(&fixtures).unwrap() {
                let name = entry.unwrap().file_name().into_string().unwrap();
                assert!(
                    rows.contains_key(name.as_str()),
                    "{name} has no manifest row"
                );
            }
            let mut failures = Vec::new();
            for (fixture, rows) in rows {
                for stale in fs::read_dir(app).unwrap() {
                    fs::remove_file(stale.unwrap().path()).unwrap();
                }
                for file in fs::read_dir(fixtures.join(fixture)).unwrap() {
                    let file = file.unwrap();
                    fs::copy(file.path(), app.join(file.file_name())).unwrap();
                }
                for test_mode in [false, true] {
                    let mode = if test_mode { "test" } else { "normal" };
                    let request = InstalledRequest {
                        test_mode,
                        ..request.clone()
                    };
                    let rows = rows
                        .iter()
                        .filter(|row| row.mode == mode)
                        .collect::<Vec<_>>();
                    assert!(!rows.is_empty(), "{fixture} has no {mode} rows");
                    replay_fixture(&format!("{fixture} {mode}"), request, &rows, &mut failures);
                }
            }
            assert!(
                failures.is_empty(),
                "{} published rows did not reproduce:\n{}",
                failures.len(),
                failures.join("\n")
            );
        },
    );
}

/// Metering rule 11, D2 (b) (ri-785.9): each graph cap the manifest published
/// before the i32 equality source root. Every one now gives `ResourceLimit`;
/// the manifest rows hold the re-pinned caps.
const PRE_EQUALITY_GRAPH: [(&str, (usize, usize, usize)); 17] = [
    ("shared", (243206, 25960, 10)),
    ("alias", (243206, 25960, 10)),
    ("competing", (214223, 5571, 10)),
    ("escape", (210417, 5194, 10)),
    ("slot-escape-good", (233659, 20423, 7)),
    ("grouped", (234037, 20429, 11)),
    ("release-live", (236457, 20915, 7)),
    ("release-after", (237290, 21197, 7)),
    ("disjoint-live", (213183, 5602, 10)),
    ("disjoint-dead", (249472, 32880, 10)),
    ("result-order", (241154, 25850, 11)),
    ("fixed-list", (191781, 4867, 6)),
    ("fixed-replacement", (203358, 8126, 3)),
    ("fwcap-literal", (190151, 4842, 6)),
    ("fwcap-grouped", (190283, 4844, 7)),
    ("fwcap-unrelated", (186093, 4126, 5)),
    ("fwcap-outer-guard", (190399, 4876, 7)),
];

#[test]
fn pre_equality_graph_caps_give_resource_limit() {
    installed(
        "legacy_caps::pre_equality_graph_caps_give_resource_limit",
        |request| {
            let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/m5/legacy-caps");
            let app = request.entry.parent().unwrap();
            let mut failures = Vec::new();
            for (fixture, (max_work, max_entries, max_depth)) in PRE_EQUALITY_GRAPH {
                for stale in fs::read_dir(app).unwrap() {
                    fs::remove_file(stale.unwrap().path()).unwrap();
                }
                for file in fs::read_dir(fixtures.join(fixture)).unwrap() {
                    let file = file.unwrap();
                    fs::copy(file.path(), app.join(file.file_name())).unwrap();
                }
                for test_mode in [false, true] {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    let receipt = typed.receipt().unwrap();
                    let old = AnalysisLimits {
                        max_work,
                        max_entries,
                        max_depth,
                    };
                    let report = analyze_ownership(&receipt, old);
                    if report.completion() != GraphCompletion::ResourceLimit {
                        failures.push(format!(
                            "{fixture} test_mode={test_mode}: {:?} at {old:?}",
                            report.completion()
                        ));
                    }
                }
            }
            assert!(failures.is_empty(), "{failures:#?}");
        },
    );
}

struct Row<'a> {
    fixture: &'a str,
    subject: &'a str,
    query: &'a str,
    mode: &'a str,
    outcome: &'a str,
    /// Absent for the rows that publish no cap.
    caps: Option<Statistics>,
    line: usize,
}

impl<'a> Row<'a> {
    fn parse(text: &'a str, line: usize) -> Self {
        let cells = text.split('\t').collect::<Vec<_>>();
        assert_eq!(cells.len(), 8, "legacy-caps.tsv:{line}: expected 8 columns");
        let number = |cell: &str| cell.parse::<usize>().unwrap();
        let caps = (cells[5] != "-").then(|| Statistics {
            work: number(cells[5]),
            entries: number(cells[6]),
            max_depth: number(cells[7]),
        });
        assert!(
            matches!(cells[3], "normal" | "test"),
            "legacy-caps.tsv:{line}: unknown mode {}",
            cells[3]
        );
        Row {
            fixture: cells[0],
            subject: cells[1],
            query: cells[2],
            mode: cells[3],
            outcome: cells[4],
            caps,
            line,
        }
    }
}

impl Row<'_> {
    fn limits(&self) -> AnalysisLimits {
        let caps = self.caps.expect("a replayed row publishes its caps");
        AnalysisLimits {
            max_work: caps.work,
            max_entries: caps.entries,
            max_depth: caps.max_depth,
        }
    }
}

/// What one run of a query reports.
struct Observed {
    outcome: String,
    statistics: Statistics,
    /// Whether the run published its receipt or MIR.
    published: bool,
}

/// Run `query` at the row's exact cap and one short on each axis.
fn replay(
    label: &str,
    row: &Row<'_>,
    failures: &mut Vec<String>,
    query: impl Fn(AnalysisLimits) -> Observed,
) {
    let caps = row.caps.expect("a replayed row publishes its caps");
    let exact = row.limits();
    let at = format!(
        "{label} {} {} (legacy-caps.tsv:{})",
        row.subject, row.query, row.line
    );
    let observed = query(exact);
    if (observed.outcome.as_str(), observed.statistics) != (row.outcome, caps) {
        let unlimited = query(AnalysisLimits::default());
        failures.push(format!(
            "{at}: published {} {caps:?}; at that cap {} {:?}; at default limits {} {:?}",
            row.outcome,
            observed.outcome,
            observed.statistics,
            unlimited.outcome,
            unlimited.statistics
        ));
        return;
    }
    // An axis the query never used (published as 0) has no one-short cap.
    for (axis, short) in [
        (
            "work",
            caps.work.checked_sub(1).map(|n| AnalysisLimits {
                max_work: n,
                ..exact
            }),
        ),
        (
            "entries",
            caps.entries.checked_sub(1).map(|n| AnalysisLimits {
                max_entries: n,
                ..exact
            }),
        ),
        (
            "depth",
            caps.max_depth.checked_sub(1).map(|n| AnalysisLimits {
                max_depth: n,
                ..exact
            }),
        ),
    ]
    .into_iter()
    .filter_map(|(axis, short)| Some((axis, short?)))
    {
        let limited = query(short);
        if limited.outcome != "ResourceLimit" || limited.published {
            failures.push(format!(
                "{at}: one short on {axis} gave {} (published: {})",
                limited.outcome, limited.published
            ));
        }
    }
}

fn replay_fixture(
    label: &str,
    request: InstalledRequest,
    rows: &[&Row<'_>],
    failures: &mut Vec<String>,
) {
    let graph = loader::load_installed(request).unwrap_or_else(|e| {
        panic!("{label}: {:?} {:?} {}", e.kind, e.path, e.message);
    });
    let only = |query: &str| {
        let mut found = rows
            .iter()
            .filter(|row| row.query == query && row.subject == "-");
        let row = found
            .next()
            .unwrap_or_else(|| panic!("{label}: no {query} row"));
        assert!(found.next().is_none(), "{label}: two {query} rows");
        *row
    };
    let standard = graph
        .modules()
        .iter()
        .filter(|m| m.key.origin == Origin::Standard)
        .count();
    let published = only("standard-modules");
    if standard.to_string() != published.outcome {
        failures.push(format!(
            "{label} standard-modules (legacy-caps.tsv:{}): published {}; loaded {standard}",
            published.line, published.outcome
        ));
    }

    let typed_row = only("typed");
    replay(label, typed_row, failures, |limits| {
        let typed = analyze_typed(&graph, limits);
        Observed {
            outcome: format!("{:?}", typed.completion()),
            statistics: typed.statistics(),
            published: typed.receipt().is_some(),
        }
    });
    // Later rows replay on the default-limit results, so one stale row does
    // not hide the rest.
    let typed = analyze_typed(&graph, AnalysisLimits::default());
    if rows
        .iter()
        .all(|row| matches!(row.query, "standard-modules" | "typed"))
    {
        // The capture was published without a whole-graph query.
        return;
    }
    let receipt = typed
        .receipt()
        .unwrap_or_else(|| panic!("{label}: rows past the typed query need a receipt"));
    for row in rows.iter().filter(|row| row.query == "graph") {
        replay(label, row, failures, |limits| {
            let report = analyze_ownership(&receipt, limits);
            Observed {
                outcome: format!("{:?}", report.completion()),
                statistics: report.statistics(),
                published: false,
            }
        });
    }
    let ownership = analyze_ownership(&receipt, AnalysisLimits::default());

    let mut roots = BTreeMap::new();
    for root in ownership.roots() {
        let module = root.site().module;
        if module.origin != Origin::Application {
            continue;
        }
        let module = graph.modules().iter().find(|m| &m.key == module).unwrap();
        let name = match root.owner() {
            BodyOwner::Startup => "Startup".to_string(),
            BodyOwner::Declaration(id) => {
                let declaration = module.declarations.iter().find(|d| &d.id == id).unwrap();
                match &declaration.name {
                    Some(name) if id.kind == loader::DeclarationKind::Function => {
                        format!("Function:{name}")
                    }
                    _ => format!("{:?}@{}", id.kind, id.start),
                }
            }
        };
        let subject = format!("{}#{name}", module.key.path);
        assert!(
            roots.insert(subject.clone(), (module, root)).is_none(),
            "{label}: two {subject} roots"
        );
    }
    for row in rows.iter().filter(|row| row.subject != "-") {
        let (module, root) = roots.get(row.subject).unwrap_or_else(|| {
            panic!(
                "{label}: no root {} (legacy-caps.tsv:{})",
                row.subject, row.line
            )
        });
        match row.query {
            "graph-root" => {
                let seen = format!("{:?}/{:?}", root.completion(), root.ordinary_completion());
                if seen != row.outcome {
                    failures.push(format!(
                        "{label} {} graph-root (legacy-caps.tsv:{}): published {}; now {seen}",
                        row.subject, row.line, row.outcome
                    ));
                }
            }
            "root" => replay(label, row, failures, |limits| {
                let report = lower_root(&receipt, module, root.owner(), limits).unwrap();
                Observed {
                    outcome: format!("{:?}", report.completion()),
                    statistics: report.statistics(),
                    published: report.mir().is_some(),
                }
            }),
            "body" => replay(label, row, failures, |limits| {
                let BodyOwner::Declaration(id) = root.owner() else {
                    panic!("{label}: {} is not a function", row.subject);
                };
                let report = lower_body(&receipt, module, id, limits).unwrap();
                Observed {
                    outcome: format!("{:?}", report.completion()),
                    statistics: report.statistics(),
                    published: report.mir().is_some(),
                }
            }),
            other => panic!(
                "{label}: unknown query {other} (legacy-caps.tsv:{})",
                row.line
            ),
        }
    }
}
