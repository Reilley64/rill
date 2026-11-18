//! Installed typed-analysis acceptance runner. Not a Rill product command.
use compiler::analysis::{AnalysisLimits, typed::analyze_typed};
use compiler::loader::{self, InstalledRequest, Limits};
use std::{env, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let Some(entry) = args
        .next()
        .map(PathBuf::from)
        .filter(|_| args.next().is_none())
    else {
        eprintln!("usage: m4_8_installed <entry.rill>");
        return ExitCode::FAILURE;
    };
    for test_mode in [false, true] {
        let mode = if test_mode { "test" } else { "normal" };
        let graph =
            match loader::load_installed(InstalledRequest {
                entry: entry.clone(),
                target: "aarch64-apple-darwin".into(),
                options: Default::default(),
                limits: Limits::default(),
                test_mode,
            }) {
                Ok(graph) => graph,
                Err(failure) => {
                    eprintln!(
                        "{mode}: installed capture failed: {:?}: {}",
                        failure.kind, failure.message
                    );
                    for finding in failure.findings {
                        eprintln!(
                            "  {}: {} ({})",
                            finding.code.unwrap_or("source"),
                            finding.message,
                            finding.span.as_ref().map_or_else(String::new, |span| {
                                String::from_utf8_lossy(span.bytes()).into_owned()
                            }),
                        );
                    }
                    continue;
                }
            };
        let report = analyze_typed(&graph, AnalysisLimits::default());
        eprintln!(
            "{mode}: {:?}; core blockers: {:?}",
            report.completion(),
            report.core_blockers()
        );
        for finding in report.bodies().findings() {
            eprintln!(
                "  {}:{}: {:?}",
                finding.site.module.path,
                finding.site.span.start(),
                finding.kind
            );
        }
        for obligation in report.bodies().obligations() {
            eprintln!(
                "  {}:{}: {:?}",
                obligation.site.module.path,
                obligation.site.span.start(),
                obligation.reason
            );
        }
        if let Some(uses) = report.result_use() {
            for witness in uses.discards() {
                eprintln!(
                    "  {}:{}: {}",
                    witness.discard.module.path,
                    witness.discard.span.start(),
                    witness.code()
                );
            }
            for pending in uses.pending() {
                eprintln!(
                    "  {}:{}: {:?}",
                    pending.site.module.path,
                    pending.site.span.start(),
                    pending.blocker
                );
            }
        }
    }
    // A partial report cannot grant a positive acceptance result.
    ExitCode::FAILURE
}
