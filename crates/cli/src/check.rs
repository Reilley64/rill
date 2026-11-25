use compiler::analysis::compilable::{
    Blocker, Compilability, CompilableProgram, ProgramAnalysis, UNSUPPORTED_SHAPE,
};
use compiler::analysis::typed::analyze_typed;
use compiler::analysis::{AnalysisLimits, Statistics};
use compiler::loader::{self, FailureKind, Graph, InstalledRequest, Limits};
use compiler::source::Span;
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

const HOST_TARGET: &str = env!("RILL_HOST_TARGET");
const PRIMARY_REPORT_LIMIT: usize = 20;
pub const SOURCE_FAILURE: u8 = 1;
pub const INVOCATION_ERROR: u8 = 2;
pub const ANALYSIS_INCOMPLETE: u8 = 3;

struct Diagnostic {
    code: Option<&'static str>,
    message: String,
    span: Option<Span>,
}

pub fn run(entry: &str) -> ExitCode {
    with_program(entry, |_, _| ExitCode::SUCCESS)
}

/// Run the front end on `entry` and call `compilable` with the graph and the
/// program when it is compilable. Every other outcome is reported on standard
/// error and returned as its driver status; `check`, `build` and `run` all
/// decide this here and nowhere else.
pub fn with_program(
    entry: &str,
    compilable: impl FnOnce(&Graph, &CompilableProgram<'_, '_>) -> ExitCode,
) -> ExitCode {
    if !Path::new(entry).is_file() {
        return invocation(&format!("entry '{}' is not a readable file", escape(entry)));
    }
    let request = InstalledRequest {
        entry: entry.into(),
        target: HOST_TARGET.into(),
        options: Default::default(),
        test_mode: false,
        limits: Limits::default(),
    };
    let graph = match loader::load_installed(request) {
        Ok(graph) => graph,
        Err(failure) => return load_failure(failure),
    };
    let limits = AnalysisLimits::default();
    let typed = analyze_typed(&graph, limits);
    let analysis = ProgramAnalysis::new(&graph, &typed, limits);
    match analysis.compilability() {
        Compilability::Compilable(program) => compilable(&graph, &program),
        Compilability::Blocked(Blocker::Incomplete { site }) => reject(vec![Diagnostic {
            code: Some(UNSUPPORTED_SHAPE),
            message: "valid source whose shape the compiler cannot compile yet".into(),
            span: Some(site.span.clone()),
        }]),
        Compilability::Blocked(Blocker::SourceRejected { finding }) => reject(vec![Diagnostic {
            code: Some(finding.code()),
            message: ownership_message(finding.code()).into(),
            span: Some(finding.site.span.clone()),
        }]),
        Compilability::Blocked(Blocker::TypedRejected { findings }) => {
            let mut diagnostics: Vec<_> = findings
                .into_iter()
                .map(|finding| Diagnostic {
                    code: finding.code,
                    message: finding.description,
                    span: Some(finding.site.span.clone()),
                })
                .collect();
            if diagnostics.is_empty() {
                diagnostics.push(Diagnostic {
                    code: None,
                    message: "the program is rejected for a finding this report does not locate"
                        .into(),
                    span: None,
                });
            }
            reject(diagnostics)
        }
        Compilability::Blocked(Blocker::TypedUnlocated { core_blockers }) => incomplete(&format!(
            "typed analysis could not finish and names no source site (blocked on {core_blockers:?})"
        )),
        Compilability::Blocked(Blocker::ResourceLimit { statistics }) => {
            limit_reached(&exhausted(statistics, limits))
        }
    }
}

fn exhausted(statistics: Statistics, limits: AnalysisLimits) -> String {
    let reached: Vec<String> = [
        ("work", statistics.work, limits.max_work),
        ("entries", statistics.entries, limits.max_entries),
        ("depth", statistics.max_depth, limits.max_depth),
    ]
    .into_iter()
    .filter(|&(_, used, cap)| used >= cap)
    .map(|(name, used, cap)| format!("{name} {used} of {cap}"))
    .collect();
    if reached.is_empty() {
        "an analysis limit".into()
    } else {
        reached.join(", ")
    }
}

fn load_failure(failure: loader::Failure) -> ExitCode {
    match failure.kind {
        FailureKind::SourceRejected if !failure.findings.is_empty() => reject(
            failure
                .findings
                .into_iter()
                .map(|finding| Diagnostic {
                    code: finding.code,
                    message: finding.message,
                    span: finding.span,
                })
                .collect(),
        ),
        FailureKind::SourceRejected | FailureKind::InputFailure | FailureKind::SourceChanged => {
            let mut message = failure.message;
            if let Some(path) = failure.path {
                let _ = write!(message, ": {}", path.display());
            }
            report_failure(&message, SOURCE_FAILURE)
        }
        FailureKind::InvalidRequest => report_failure(&failure.message, INVOCATION_ERROR),
        FailureKind::ResourceLimit => {
            limit_reached(&format!("the loader limit ({})", failure.message))
        }
    }
}

fn ownership_message(code: &str) -> &'static str {
    match code {
        "R0200" => "value used after it was moved",
        "R0201" => "conflicting access",
        "R0202" => "reference escapes the value it borrows",
        "R0203" => "value released while it is borrowed",
        _ => "ownership rule violated",
    }
}

fn reject(mut diagnostics: Vec<Diagnostic>) -> ExitCode {
    diagnostics.sort_by_key(|diagnostic| {
        diagnostic.span.as_ref().map(|span| {
            (
                span.snapshot().identity().to_owned(),
                span.start(),
                span.end(),
            )
        })
    });
    let mut report = String::new();
    for diagnostic in diagnostics.iter().take(PRIMARY_REPORT_LIMIT) {
        render(&mut report, diagnostic);
    }
    if let Some(omitted) = diagnostics.len().checked_sub(PRIMARY_REPORT_LIMIT) {
        let _ = writeln!(
            report,
            "rill: {omitted} more reports omitted; the limit is {PRIMARY_REPORT_LIMIT}"
        );
    }
    eprint!("{report}");
    ExitCode::from(SOURCE_FAILURE)
}

fn render(report: &mut String, diagnostic: &Diagnostic) {
    let message = escape(&diagnostic.message);
    match diagnostic.code {
        Some(code) => writeln!(report, "error[{code}]: {message}"),
        None => writeln!(report, "error: {message}"),
    }
    .unwrap();
    let Some(span) = &diagnostic.span else {
        return;
    };
    let identity = escape(span.snapshot().identity());
    let Some((start, end)) = span.coordinates() else {
        let _ = writeln!(report, " --> {identity}:byte {}", span.start());
        return;
    };
    let _ = writeln!(
        report,
        " --> {identity}:{}:{}",
        start.line + 1,
        start.scalar + 1
    );
    let bytes = span.snapshot().bytes();
    let line_start = span.start() - start.utf8;
    let line_end = bytes[span.start()..]
        .iter()
        .position(|byte| matches!(byte, b'\n' | b'\r'))
        .map_or(bytes.len(), |offset| span.start() + offset);
    let line = String::from_utf8_lossy(&bytes[line_start..line_end]);
    let number = (start.line + 1).to_string();
    let gutter = " ".repeat(number.len());
    let skipped = escape(&String::from_utf8_lossy(&bytes[line_start..span.start()]));
    let marked_end = span.end().min(line_end);
    let marked = escape(&String::from_utf8_lossy(&bytes[span.start()..marked_end]));
    let width = marked.chars().count().max(1);
    let _ = end;
    let _ = writeln!(report, "{gutter} |");
    let _ = writeln!(report, "{number} | {}", escape(&line));
    let _ = writeln!(
        report,
        "{gutter} | {}{}",
        " ".repeat(skipped.chars().count()),
        "^".repeat(width)
    );
}

pub fn escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn invocation(message: &str) -> ExitCode {
    report_failure(message, INVOCATION_ERROR)
}

fn limit_reached(limit: &str) -> ExitCode {
    incomplete(&format!("reached {limit}"))
}

fn incomplete(reason: &str) -> ExitCode {
    report_failure(
        &format!("analysis is incomplete: {reason}; this is not a passed check"),
        ANALYSIS_INCOMPLETE,
    )
}

pub fn report_failure(message: &str, status: u8) -> ExitCode {
    eprintln!("rill: {}", escape(message));
    ExitCode::from(status)
}
