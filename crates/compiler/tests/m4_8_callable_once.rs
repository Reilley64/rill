//! A private callable requirement does not check an actual environment by itself.
#![cfg(all(target_arch = "aarch64", target_os = "macos"))]

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
const ARCHIVE: &str = "lib/rill/runtime/aarch64-apple-darwin/librill_ownership.rlib";

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
            Some((322208, 47470, 12)),
        ),
        (
            "declared-only",
            declared_only.into(),
            Incomplete,
            Some((321709, 47416, 12)),
        ),
        (
            "erased",
            format!("{erased}{CALLER}"),
            Incomplete,
            Some((324473, 47678, 12)),
        ),
        (
            "missing",
            format!("{missing}{CALLER}"),
            Incomplete,
            Some((324476, 47586, 12)),
        ),
        (
            "borrowed-once",
            borrowed_once.into(),
            SourceRejected,
            Some((324058, 47748, 12)),
        ),
        ("local", local.into(), Checked, Some((323795, 47803, 12))),
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
            Some((326803, 48079, 12)),
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
            Some((324723, 47810, 12)),
        ),
    ] {
        fs::write(entry, format!("{IMPORTS}{source}")).unwrap();
        for test_mode in [false, true] {
            let graph = load_installed(InstalledRequest {
                entry: entry.into(),
                target: "aarch64-apple-darwin".into(),
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
                assert_eq!(
                    short_report.completion(),
                    TypedCompletion::ResourceLimit,
                    "{name} {test_mode} {short:?}"
                );
                assert!(short_report.receipt().is_none());
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

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
