//! A private callable requirement does not check an actual environment by itself.
#[path = "support/limit.rs"]
mod limit;

use compiler::analysis::generics::{GoalState, callables::CaptureForm};
use compiler::analysis::scalars::{NodeFactKind, ScalarFindingKind, data::Access};
use compiler::analysis::typed::{
    ResultDiscardKind, ResultUseBlocker, TypedCompletion, analyze_typed,
};
use compiler::analysis::{AnalysisLimits, InvocationMode, Stage};
use compiler::loader::{InstalledRequest, Origin, load_installed};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const IMPORTS: &str = "import std::{List, Result, Err};
import std::channel::{Receiver, Sender};
import std::convert::{into, tryInto};
import std::error::formatError;
import std::iter::iterate;
import std::mutex::LockGuard;
import std::number::ParseError;
import std::option::{Option, map as mapOption};
import std::result::{isOk, map as mapResult};
import std::shared::Shared;

";
const WRAPPER: &str = "function invoke<F>(job: F): [&bool, bool] where callableOnce(F, [], [&bool, bool]) { return job(); }\n";
const CALLER: &str = "function caller(flag: &bool, seed: bool): [&bool, bool] { return invoke(() => [flag, seed]); }\n";
const SHARED_WRAPPER: &str =
    "function run<F>(job: &F): bool where callable(F, [], bool) { return job(); }\n";
const SHARED_CALLER: &str =
    "function caller(flag: bool): bool { let f: () => bool = () => flag; return run(&f); }\n";
const ARCHIVE: &str = concat!(
    "lib/rill/runtime/",
    env!("RILL_ANALYSIS_TARGET"),
    "/librill_ownership.rlib"
);

#[test]
fn private_once_checks_the_actual_shared_pair() {
    let Some(root) = std::env::var_os("RILL_M4_ONCE_TEST_PREFIX") else {
        let root = std::env::temp_dir().join(format!("rill-m4-once-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let std = root.join("lib/rill/std");
        let archive = root.join(ARCHIVE);
        for dir in [
            &root.join("bin"),
            &root.join("app"),
            &std,
            archive.parent().unwrap(),
        ] {
            fs::create_dir_all(dir).unwrap();
        }
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
        for module in include_str!("../../../tests/installed/typed/stdlib-sources.txt").lines() {
            fs::copy(source.join(module), std.join(module)).unwrap();
        }
        fs::copy(env!("RILL_OWNERSHIP_ARCHIVE"), &archive).unwrap();
        fs::copy(
            source.join("../runtime/error/src/lib.rs"),
            archive.with_file_name("error.rs"),
        )
        .unwrap();
        let probe = root.join("bin/probe");
        fs::copy(std::env::current_exe().unwrap(), &probe).unwrap();
        let output = Command::new(probe)
            .current_dir(root.join("app"))
            .env("RILL_M4_ONCE_TEST_PREFIX", &root)
            .args([
                "--nocapture",
                "--exact",
                "private_once_checks_the_actual_shared_pair",
            ])
            .output()
            .unwrap();
        print!("{}", String::from_utf8_lossy(&output.stdout));
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    check_sources(&PathBuf::from(root).join("app/main.rill"));
}

fn check_sources(entry: &Path) {
    use ResultUseBlocker::{CallBoundary, Containment};
    use TypedCompletion::{Checked, Incomplete, SourceRejected};
    let erased = WRAPPER
        .replace("<F>(job: F)", "(job: once () => [&bool, bool])")
        .replace(" where callableOnce(F, [], [&bool, bool])", "");
    let missing = WRAPPER.replace(" where callableOnce(F, [], [&bool, bool])", "");
    let sibling = CALLER.replace(
        "return invoke",
        "let unpaid: Result<bool, i32>.Ok = Result.Ok(seed); return invoke",
    );
    let second = CALLER
        .replace("caller(", "second(")
        .replace("flag", "otherFlag")
        .replace("seed", "otherSeed");
    let capture = "function caller(flag: &bool, seed: bool): [&bool, bool] { let res: Result<bool, i32>.Ok = Result.Ok(seed); let job: once () => [&bool, bool] = () => { let dropped = res; return [flag, seed]; }; return invoke(job); }\n";
    let borrowed_once = "function probe(left: string, right: string): [string, string] {\n    let mut f: once () => [string, string] = () => [left, right];\n    let descriptor = &mut f;\n    return descriptor();\n}\n";
    let local = "function probe(flag: &bool, seed: bool): [&bool, bool] {\n    let f: () => [&bool, bool] = () => [flag, seed];\n    let descriptor = &f;\n    return descriptor();\n}\n";
    let declared_only =
        "function run<F>(job: F): bool where callable(F, [], bool) { return job(); }\n";
    let output = "function invoke<F>(job: F): Result<bool, i32> where callableOnce(F, [], Result<bool, i32>) { return job(); }\nfunction caller(seed: bool): void { invoke(() => Result.Ok(seed)); }\n";
    let shared = SHARED_WRAPPER;
    let shared_actual = format!("{shared}{SHARED_CALLER}");
    let shared_second = SHARED_CALLER
        .replace("caller(", "second(")
        .replace("flag", "otherFlag");
    let shared_discard = "function run<F>(job: &F): Result<bool, i32> where callable(F, [], Result<bool, i32>) { return job(); }\nfunction caller(flag: bool): bool { let f: () => Result<bool, i32> = () => Result.Ok(flag); run(&f); return flag; }\n";
    for (name, source, completion, fixed) in [
        (
            "symbolic-invoke",
            format!("{WRAPPER}{CALLER}"),
            Checked,
            None,
        ),
        (
            "two-actuals",
            format!("{second}{WRAPPER}{CALLER}"),
            Checked,
            None,
        ),
        (
            "unused-once",
            WRAPPER.into(),
            Incomplete,
            Some((321414, 47470, 12)),
        ),
        (
            "declared-only",
            declared_only.into(),
            Incomplete,
            Some((320916, 47416, 12)),
        ),
        (
            "erased",
            format!("{erased}{CALLER}"),
            Incomplete,
            Some((323671, 47678, 12)),
        ),
        (
            "missing",
            format!("{missing}{CALLER}"),
            Incomplete,
            Some((323668, 47586, 12)),
        ),
        (
            "borrowed-once",
            borrowed_once.into(),
            SourceRejected,
            Some((323255, 47748, 12)),
        ),
        ("local", local.into(), Checked, Some((322989, 47803, 12))),
        (
            "unclosed-actual",
            format!(
                "{WRAPPER}{CALLER}function open(job: once () => [&bool, bool]): [&bool, bool] {{ return invoke(job); }}\n"
            ),
            Incomplete,
            None,
        ),
        (
            "dead-only",
            format!(
                "{WRAPPER}function caller(flag: &bool, seed: bool): [&bool, bool] {{ if (false) {{ return invoke(() => [flag, seed]); }} return [flag, seed]; }}\n"
            ),
            Incomplete,
            None,
        ),
        (
            "public",
            format!("export {WRAPPER}{CALLER}"),
            Incomplete,
            None,
        ),
        (
            "capture-unpaid",
            format!("{WRAPPER}{capture}"),
            Incomplete,
            Some((325994, 48079, 12)),
        ),
        (
            "sibling",
            format!("{WRAPPER}{sibling}"),
            SourceRejected,
            None,
        ),
        (
            "discarded-output",
            output.into(),
            Incomplete,
            Some((323924, 47810, 12)),
        ),
        (
            "declared-shared-actual",
            shared_actual,
            Checked,
            Some((324437, 47828, 12)),
        ),
        (
            "declared-shared-only",
            shared.into(),
            Incomplete,
            Some((320912, 47419, 12)),
        ),
        (
            "declared-shared-output-discard",
            shared_discard.into(),
            Incomplete,
            Some((324776, 47950, 12)),
        ),
        (
            "shared-two-actuals",
            format!("{shared_second}{shared}{SHARED_CALLER}"),
            Checked,
            Some((326815, 48076, 12)),
        ),
        (
            "shared-unclosed",
            format!(
                "{shared}{SHARED_CALLER}function open(job: &() => bool): bool {{ return run(job); }}\n"
            ),
            Incomplete,
            Some((325354, 47793, 12)),
        ),
        (
            "shared-dead-only",
            format!(
                "{shared}function caller(flag: bool): bool {{ let f: () => bool = () => flag; if (false) {{ return run(&f); }} return flag; }}\n"
            ),
            Incomplete,
            Some((323325, 47702, 12)),
        ),
        (
            "shared-public",
            format!("export {shared}{SHARED_CALLER}"),
            Incomplete,
            Some((323171, 47652, 12)),
        ),
        (
            "shared-grouped",
            format!("{shared}{}", SHARED_CALLER.replace("run(&f)", "run((&f))")),
            Incomplete,
            Some((323397, 47680, 12)),
        ),
        (
            "shared-scoped",
            format!(
                "{shared}function caller(flag: bool): bool {{ let f: () => bool = () => flag; {{ return run(&f); }} }}\n"
            ),
            Incomplete,
            Some((323176, 47656, 12)),
        ),
        (
            "shared-loop",
            format!(
                "{shared}function caller(flag: bool): bool {{ let f: () => bool = () => flag; while (flag) {{ return run(&f); }} return false; }}\n"
            ),
            Incomplete,
            Some((323355, 47720, 12)),
        ),
        (
            "shared-result-capture",
            format!(
                "{shared}function caller(flag: bool): bool {{ let res: Result<bool, i32> = Result.Ok(flag); let f: () => bool = () => isOk(&res); return run(&f); }}\n"
            ),
            Incomplete,
            Some((324513, 47868, 12)),
        ),
        (
            "shared-formal-actual",
            format!("{shared}function caller(f: () => bool): bool {{ return run(&f); }}\n"),
            Incomplete,
            Some((322800, 47577, 12)),
        ),
        // The 17-source M4 baseline: no wrapper route to discover.
        (
            "baseline",
            include_str!("../../../tests/installed/typed/main.rill").into(),
            Checked,
            Some((371288, 54059, 12)),
        ),
    ] {
        if name == "baseline" {
            fs::write(
                entry.with_file_name("checks.rill"),
                include_str!("../../../tests/installed/typed/checks.rill"),
            )
            .unwrap();
            fs::write(entry, source).unwrap();
        } else {
            fs::write(entry, format!("{IMPORTS}{source}")).unwrap();
        }
        for test_mode in [false, true] {
            let graph = load_installed(InstalledRequest {
                entry: entry.into(),
                target: env!("RILL_ANALYSIS_TARGET").into(),
                test_mode,
                options: Default::default(),
                limits: Default::default(),
            })
            .unwrap();
            let expected = include_str!("../../../tests/installed/typed/stdlib-sources.txt")
                .lines()
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                graph
                    .modules()
                    .iter()
                    .filter(|m| m.key.origin == Origin::Standard)
                    .map(|m| m.key.path.as_str())
                    .collect::<std::collections::BTreeSet<_>>(),
                expected
            );
            let report = analyze_typed(&graph, Default::default());
            let uses = report.result_use().unwrap();
            let pending = uses
                .pending()
                .iter()
                .map(|p| (p.site.span.start(), p.site.span.bytes(), p.blocker))
                .collect::<Vec<_>>();
            let stats = report.statistics();
            assert_eq!(
                report.completion(),
                completion,
                "{name} {test_mode}: {pending:?}"
            );
            assert!(report.routine_role().is_none());
            assert_eq!(report.receipt().is_some(), completion == Checked);
            if let Some(receipt) = report.receipt() {
                assert_eq!(receipt.context(), graph.id());
                assert_eq!(
                    receipt.not_performed(),
                    [Stage::Ownership, Stage::Execution]
                );
            }
            if let Some(fixed) = fixed {
                assert_eq!(
                    (stats.work, stats.entries, stats.max_depth),
                    fixed,
                    "{name} {test_mode}"
                );
            }
            let exact = AnalysisLimits {
                max_work: stats.work,
                max_entries: stats.entries,
                max_depth: stats.max_depth,
            };
            let replay = analyze_typed(&graph, exact);
            assert_eq!(replay.completion(), completion, "{name} {test_mode}");
            assert_eq!(replay.statistics(), stats);
            for short in [
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
            ] {
                let short_report = analyze_typed(&graph, short);
                crate::limit::assert_typed_limit(&short_report);
            }
            let one_debt = |at: &[u8], origin: &[u8], kind| {
                let [debt] = uses.discards() else {
                    panic!("{name}: expected one R0103")
                };
                assert_eq!(debt.code(), "R0103");
                assert_eq!(debt.discard.span.bytes(), at);
                assert_eq!(debt.origin.span.bytes(), origin);
                assert_eq!(debt.kind, kind);
                assert!(debt.component.is_empty() && debt.storage_component.is_empty());
            };
            match name {
                "symbolic-invoke" => {
                    assert!(pending.is_empty());
                    check_typed_origins(&graph, &report);
                }
                "declared-only" => assert_eq!(
                    pending.as_slice(),
                    [
                        (423, b"job()".as_slice(), CallBoundary),
                        (372, b"job: F".as_slice(), Containment)
                    ]
                ),
                "erased" => assert_eq!(
                    pending.as_slice(),
                    [(427, b"job()".as_slice(), CallBoundary)]
                ),
                "unused-once" | "unclosed-actual" | "dead-only" | "public" => {
                    assert!(
                        pending
                            .iter()
                            .any(|(_, span, kind)| *span == b"job()" && *kind == CallBoundary)
                    );
                    assert!(uses.discards().is_empty());
                }
                "capture-unpaid" => {
                    assert_eq!(
                        pending.as_slice(),
                        [(448, b"job()".as_slice(), CallBoundary)]
                    );
                    one_debt(
                        b"return invoke(job);",
                        b"Result.Ok(seed)",
                        ResultDiscardKind::ScopeExit,
                    );
                }
                "sibling" => {
                    assert!(pending.is_empty());
                    one_debt(
                        b"return invoke(() => [flag, seed]);",
                        b"Result.Ok(seed)",
                        ResultDiscardKind::ScopeExit,
                    );
                }
                "discarded-output" => one_debt(
                    b"invoke(() => Result.Ok(seed));",
                    b"invoke(() => Result.Ok(seed))",
                    ResultDiscardKind::Expression,
                ),
                "declared-shared-actual" | "shared-two-actuals" => {
                    assert!(pending.is_empty());
                    assert!(uses.discards().is_empty());
                    if name == "declared-shared-actual" {
                        check_lent_origins(&graph, &report);
                        lent_ownership_is_checked(&graph, &report);
                    }
                }
                "declared-shared-only"
                | "shared-unclosed"
                | "shared-dead-only"
                | "shared-grouped"
                | "shared-scoped"
                | "shared-loop" => {
                    assert_eq!(
                        pending.as_slice(),
                        [(424, b"job()".as_slice(), CallBoundary)]
                    );
                    assert!(uses.discards().is_empty());
                }
                "shared-public" => {
                    assert_eq!(
                        pending.as_slice(),
                        [(431, b"job()".as_slice(), CallBoundary)]
                    );
                    assert!(uses.discards().is_empty());
                }
                "shared-formal-actual" => {
                    assert_eq!(
                        pending.as_slice(),
                        [
                            (424, b"job()".as_slice(), CallBoundary),
                            (449, b"f: () => bool".as_slice(), Containment)
                        ]
                    );
                    assert!(uses.discards().is_empty());
                }
                // A checked lent call would not acknowledge its returned Result.
                "declared-shared-output-discard" => {
                    assert_eq!(
                        pending.as_slice(),
                        [
                            (450, b"job()".as_slice(), CallBoundary),
                            (551, b"run(&f)".as_slice(), Containment)
                        ]
                    );
                    one_debt(b"run(&f);", b"run(&f)", ResultDiscardKind::Expression);
                }
                // Lending the descriptor neither moves nor acknowledges its
                // caller's Result: the owner still discards it at scope exit.
                "shared-result-capture" => {
                    assert_eq!(
                        pending.as_slice(),
                        [
                            (424, b"job()".as_slice(), CallBoundary),
                            (535, b"() => isOk(&res)".as_slice(), Containment)
                        ]
                    );
                    one_debt(
                        b"return run(&f);",
                        b"Result.Ok(flag)",
                        ResultDiscardKind::ScopeExit,
                    );
                }
                "borrowed-once" => {
                    let [finding] = report.bodies().findings() else {
                        panic!("expected borrowed-once finding")
                    };
                    assert_eq!(finding.kind, ScalarFindingKind::ReadOnly);
                    assert_eq!(
                        (finding.site.span.start(), finding.site.span.bytes()),
                        (526, b"descriptor()".as_slice())
                    );
                }
                _ => {}
            }
            println!(
                "{name} {test_mode}: {completion:?} {stats:?}; pending {pending:?}; exact + all one-short"
            );
        }
    }
}

fn check_typed_origins(
    graph: &compiler::loader::Graph,
    report: &compiler::analysis::typed::TypedReport<'_>,
) {
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let facts = |text: &[u8]| {
        let node = app
            .syntax
            .nodes
            .iter()
            .position(|n| n.span.bytes() == text)
            .unwrap();
        report.bodies().node(app, node).unwrap().unwrap()
    };
    let invocation = facts(b"job()")
        .callable
        .as_ref()
        .unwrap()
        .invocation
        .as_ref()
        .unwrap();
    assert_eq!(
        (invocation.mode, invocation.access),
        (InvocationMode::Once, Access::Owned)
    );
    let path = invocation.path.as_ref().unwrap();
    let formal = app
        .declarations
        .iter()
        .find(|d| &d.id == path.root)
        .unwrap();
    assert_eq!(app.syntax.nodes[formal.node].span.bytes(), b"job: F");
    assert!(path.projections.is_empty());
    let assumption = report
        .bodies()
        .goal(invocation.assumption.as_ref().unwrap())
        .unwrap();
    assert_eq!(
        (assumption.source.span.start(), assumption.state),
        (404, GoalState::Assumption)
    );
    assert_eq!(
        assumption.source.span.bytes(),
        b"callableOnce(F, [], [&bool, bool])"
    );
    let actual = facts(b"() => [flag, seed]");
    let arrow = actual.callable.as_ref().unwrap().arrow.as_ref().unwrap();
    assert_eq!(
        (arrow.site.span.start(), arrow.mode),
        (529, Some(InvocationMode::ReadOnly))
    );
    assert_eq!(arrow.site.node, actual.site.node);
    let captures = arrow
        .captures
        .iter()
        .map(|c| (c.site.span.bytes(), c.form))
        .collect::<Vec<_>>();
    assert!(captures.contains(&(b"flag", CaptureForm::CarriedReference { mutable: false })));
    assert!(captures.contains(&(b"seed", CaptureForm::Value)));
    let call = facts(b"invoke(() => [flag, seed])");
    let NodeFactKind::Expression(value) = &call.kind else {
        panic!("missing named call")
    };
    let named = value.call.as_ref().unwrap();
    assert_eq!(named.arguments, [arrow.site.node]);
    assert_eq!(named.callee.start, 356);
    assert_eq!(value.ty, arrow.result);
    let goal = report
        .bodies()
        .goals()
        .iter()
        .find(|g| g.site.node == call.site.node && g.site.module == &app.key)
        .unwrap();
    assert_eq!(goal.source.node, assumption.source.node);
    assert_eq!(goal.state, GoalState::Proved);
    assert!(goal.premise.is_none() && goal.arrow_origin.is_none());
}

fn check_lent_origins(
    graph: &compiler::loader::Graph,
    report: &compiler::analysis::typed::TypedReport<'_>,
) {
    use compiler::analysis::scalars::data::PathProjection;
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let facts = |text: &[u8]| {
        let node = app
            .syntax
            .nodes
            .iter()
            .position(|n| n.span.bytes() == text)
            .unwrap();
        report.bodies().node(app, node).unwrap().unwrap()
    };
    let invocation = facts(b"job()")
        .callable
        .as_ref()
        .unwrap()
        .invocation
        .as_ref()
        .unwrap();
    // The wrapper reads through its borrow; it never owns or consumes `F`.
    assert_eq!(
        (invocation.mode, invocation.access),
        (InvocationMode::ReadOnly, Access::ReadOnly)
    );
    let path = invocation.path.as_ref().unwrap();
    assert!(matches!(
        path.projections.as_slice(),
        [PathProjection::Dereference]
    ));
    let assumption = report
        .bodies()
        .goal(invocation.assumption.as_ref().unwrap())
        .unwrap();
    assert_eq!(
        (assumption.source.span.start(), assumption.state),
        (393, GoalState::Assumption)
    );
    assert_eq!(assumption.source.span.bytes(), b"callable(F, [], bool)");
    let call = facts(b"run(&f)");
    let NodeFactKind::Expression(value) = &call.kind else {
        panic!("missing lent call")
    };
    let named = value.call.as_ref().unwrap();
    assert_eq!(
        app.syntax.nodes[named.arguments[0]].span.bytes(),
        b"&f".as_slice()
    );
    let goal = report
        .bodies()
        .goals()
        .iter()
        .find(|g| g.site.node == call.site.node && g.site.module == &app.key)
        .unwrap();
    assert_eq!(goal.source.node, assumption.source.node);
    assert_eq!(goal.state, GoalState::Proved);
}

/// The receipt reaches M5, whose declared shared frame checks the caller
/// (ri-785.7; frame, caps and contrasts in `m5/captures/declared_shared.rs`).
fn lent_ownership_is_checked(
    graph: &compiler::loader::Graph,
    report: &compiler::analysis::typed::TypedReport<'_>,
) {
    use compiler::analysis::ownership::{Completion, analyze_ownership, lower_body};
    let receipt = report.receipt().unwrap();
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let caller = app
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("caller"))
        .unwrap();
    let target = lower_body(&receipt, app, &caller.id, Default::default()).unwrap();
    let ownership = analyze_ownership(&receipt, Default::default());
    let root = ownership
        .roots()
        .iter()
        .find(|r| std::ptr::eq(r.site().module, &app.key) && r.site().node == caller.node)
        .unwrap();
    println!(
        "M5 caller: target {:?} {:?}; ordinary root {:?}",
        target.completion(),
        target.statistics(),
        root.ordinary_completion()
    );
    assert_eq!(target.completion(), Completion::Checked);
    assert_eq!(root.ordinary_completion(), Completion::Checked);
    assert!(target.mir().is_some() && target.findings().is_empty());
    assert!(root.ordinary_mir().is_some());
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
