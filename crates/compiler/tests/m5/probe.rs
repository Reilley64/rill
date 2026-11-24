use crate::pipeline::{self, ModeSummary, Outcome};
use compiler::loader::InstalledRequest;
use std::{fmt::Write, fs, path::Path};

const FIXTURE: &str = include_str!("probe-fixture.txt");

fn sections(sources: &str) -> Vec<(&str, &str)> {
    let mut found: Vec<(&str, &str)> = Vec::new();
    let mut rest = sources;
    let mut start = None;
    while !rest.is_empty() {
        let (line, next) = rest.split_once('\n').unwrap_or((rest, ""));
        if let Some(name) = line.strip_prefix("=== ") {
            if let Some((name, from)) = start.take() {
                found.push((name, &sources[from..sources.len() - rest.len()]));
            }
            start = Some((name.trim(), sources.len() - next.len()));
        } else {
            assert!(
                start.is_some() || line.trim().is_empty(),
                "text before the first `=== name` line: {line:?}"
            );
        }
        rest = next;
    }
    if let Some((name, from)) = start {
        found.push((name, &sources[from..]));
    }
    assert!(
        !found.is_empty(),
        "the sources file has no `=== name` section"
    );
    found
}

fn describe(summary: &ModeSummary, out: &mut String) {
    let mode = if summary.test_mode { "test" } else { "run" };
    match &summary.outcome {
        Outcome::LoadFailed {
            kind,
            code,
            message,
            text,
        } => {
            writeln!(
                out,
                "{mode}: load failed {kind} {} {message:?} at {text:?}",
                code.as_deref().unwrap_or("-")
            )
            .unwrap();
        }
        Outcome::Panicked(message) => writeln!(out, "{mode}: panicked {message:?}").unwrap(),
        Outcome::Analysed { typed, graph: None } => {
            writeln!(out, "{mode}: typed {typed}, no receipt").unwrap();
        }
        Outcome::Analysed {
            typed,
            graph: Some(graph),
        } => {
            let stats = graph.statistics;
            writeln!(
                out,
                "{mode}: typed {typed}, graph {}, work {} entries {} depth {}",
                graph.completion, stats.work, stats.entries, stats.max_depth
            )
            .unwrap();
            for root in &graph.roots {
                writeln!(
                    out,
                    "  root {} {} {:?} mir {} blocked {}",
                    root.module,
                    root.owner,
                    root.completion,
                    if root.has_mir { "yes" } else { "no" },
                    root.blocked_at.as_deref().unwrap_or("-")
                )
                .unwrap();
                for usage in &root.whole_value_uses {
                    writeln!(out, "    use {usage}").unwrap();
                }
            }
            for finding in &graph.findings {
                writeln!(out, "  finding {finding}").unwrap();
            }
        }
    }
}

fn report(request: &InstalledRequest, sources: &str) -> String {
    let app = request.entry.parent().unwrap();
    let mut out = String::new();
    for (name, source) in sections(sources) {
        writeln!(out, "=== {name}").unwrap();
        pipeline::replace_with_source(app, "main.rill", &format!("{}\n{source}", crate::IMPORTS));
        for summary in pipeline::summarize(request, app, "main.rill") {
            describe(&summary, &mut out);
        }
    }
    out
}

#[test]
#[ignore = "run through `mise run probe:m5 -- <sources file>`"]
fn probe_sources() {
    let path = std::env::var_os("RILL_M5_PROBE_SOURCES")
        .expect("RILL_M5_PROBE_SOURCES names the sources file (an absolute path)");
    let sources = fs::read_to_string(Path::new(&path)).unwrap();
    crate::installed("probe::probe_sources", |request| {
        println!("{}", report(&request, &sources));
    });
}

#[test]
fn probe_reports_the_fixture_in_both_modes() {
    crate::installed(
        "probe::probe_reports_the_fixture_in_both_modes",
        |request| {
            let report = report(&request, FIXTURE);
            let section = |name: &str| {
                report
                    .split("=== ")
                    .find(|part| part.starts_with(&format!("{name}\n")))
                    .unwrap_or_else(|| panic!("no section {name}:\n{report}"))
            };
            let modes = |name: &str| {
                let (run, test) = section(name)
                    .split_once("\ntest: ")
                    .unwrap_or_else(|| panic!("no test mode in {name}:\n{report}"));
                let run = run.split_once("run: ").expect("a run mode").1;
                [run, test]
            };
            assert_eq!(report.matches("=== ").count(), 3, "{report}");
            for mode in modes("moved-and-copied") {
                assert!(
                    mode.starts_with("typed Checked, graph ")
                        && mode.contains(", work ")
                        && mode.contains(" entries ")
                        && mode.contains(" depth "),
                    "{report}"
                );
                assert!(
                    mode.contains("  root main.rill Function:moved Checked mir yes blocked -\n"),
                    "{report}"
                );
                assert!(mode.contains("    use Move text@"), "{report}");
                assert!(mode.contains("    use Copy count@"), "{report}");
            }
            for mode in modes("reused-after-move") {
                assert!(
                    mode.contains("Function:reused SourceRejected mir no"),
                    "{report}"
                );
                assert!(
                    mode.contains("  finding R0200 main.rill@")
                        && mode.contains("\"text\" related \"text\""),
                    "{report}"
                );
            }
            for mode in modes("does-not-load") {
                assert!(mode.starts_with("load failed SourceRejected "), "{report}");
            }
        },
    );
}
