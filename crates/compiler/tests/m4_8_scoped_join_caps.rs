//! Fixed M4 caps measured at the published parent, 4ef298b.
// The installed M4 profile that can issue a receipt is macOS AArch64.
#![cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[path = "support/limit.rs"]
mod limit;
mod support;

use compiler::analysis::generics::{GoalBlocker, GoalState, OutputAgreement};
use compiler::analysis::scalars::{NodeFactKind, ScalarUnsupported};
use compiler::analysis::typed::{
    ResultDiscardKind, ResultUseBlocker, TypedCompletion, TypedReport, analyze_typed,
};
use compiler::analysis::{AnalysisLimits, TypeTerm};
use compiler::loader::{self, InstalledRequest, Origin};
use compiler::parser::NodeKind;
use std::{fs, path::PathBuf, process::Command};

const IMPORTS: &str = "\
import std::{List, Result, Err};
import std::channel::{Receiver, Sender};
import std::convert::{into, tryInto};
import std::error::formatError;
import std::iter::iterate;
import std::mutex::LockGuard;
import std::number::ParseError;
import std::option::{Option, map as mapOption};
import std::result::{isOk, map as mapResult};
import std::shared::Shared;

import std::routine::join;
";

#[test]
fn unsupported_result_joins_keep_parent_caps_and_blockers() {
    let Some(root) = std::env::var_os("RILL_JOIN_CAP_PROBE_ROOT") else {
        let root = std::env::temp_dir().join(format!("rill-join-cap-probe-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        for dir in [
            "bin",
            "lib/rill/std",
            "app",
            "lib/rill/runtime/aarch64-apple-darwin",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for module in include_str!("../../../tests/installed/typed/stdlib-sources.txt")
            .lines()
            .chain(include_str!("../../../tests/installed/typed/routine-sources.txt").lines())
        {
            fs::copy(
                repo.join("stdlib").join(module),
                root.join("lib/rill/std").join(module),
            )
            .unwrap();
        }
        let runtime = root.join("lib/rill/runtime/aarch64-apple-darwin");
        fs::copy(
            env!("RILL_OWNERSHIP_ARCHIVE"),
            runtime.join("librill_ownership.rlib"),
        )
        .unwrap();
        fs::copy(
            repo.join("runtime/error/src/lib.rs"),
            runtime.join("error.rs"),
        )
        .unwrap();
        let output = support::run_copied_probe(
            Command::new(root.join("bin/probe"))
                .args([
                    "--exact",
                    "unsupported_result_joins_keep_parent_caps_and_blockers",
                    "--nocapture",
                ])
                .env("RILL_JOIN_CAP_PROBE_ROOT", &root)
                .current_dir(root.join("app")),
        )
        .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };

    let entry = PathBuf::from(root).join("app/main.rill");
    let mut failures = Vec::new();
    const BASE: &str = include_str!("../../../tests/installed/typed/main.rill");
    fs::write(
        entry.with_file_name("checks.rill"),
        include_str!("../../../tests/installed/typed/checks.rill"),
    )
    .unwrap();
    let load = |test_mode| {
        let graph = loader::load_installed(InstalledRequest {
            entry: entry.clone(),
            target: "aarch64-apple-darwin".into(),
            options: Default::default(),
            test_mode,
            limits: Default::default(),
        })
        .unwrap();
        assert!(graph.modules().iter().any(
            |module| module.key.origin == Origin::Standard && module.key.path == "routine.rill"
        ));
        graph
    };
    for (base, source, work, entries, depth, result_slot, obligation, blocker, goal_state) in [
        (
            IMPORTS,
            "function joined(): void { join([() => Err<bool, i32>(7i32), () => !false]); }\n",
            456467,
            49568,
            12,
            0,
            None,
            ResultUseBlocker::CallBoundary,
            Some(GoalState::Proved),
        ),
        (
            IMPORTS,
            "function joined(value: Result<bool, i32>): [Result<bool, i32>, bool] {\n    return join([() => value, () => false]);\n}\n",
            457660,
            49708,
            12,
            0,
            None,
            ResultUseBlocker::CallBoundary,
            Some(GoalState::Proved),
        ),
        (
            IMPORTS,
            "function joined(job: () => Result<bool, i32>): [Result<bool, i32>, bool] { return join([job, () => false]); }\n",
            456311,
            49436,
            12,
            0,
            Some(ScalarUnsupported::FamilySelection),
            ResultUseBlocker::TypedInput,
            Some(GoalState::Unresolved(GoalBlocker::CallableEnvironment)),
        ),
        (
            IMPORTS,
            "function joined(flag: bool): [bool, Result<bool,i32>] { return join<[() => bool, () => Result<bool,i32>], [bool,Result<bool,i32>]>([() => !flag, () => Err<bool,i32>(7i32)]); }\n",
            460930,
            50337,
            14,
            1,
            None,
            ResultUseBlocker::CallBoundary,
            Some(GoalState::Proved),
        ),
        (
            IMPORTS,
            "function joined(a: () => bool, b: () => Result<bool,i32>): [bool, Result<bool,i32>] { return join([a,b]); }\n",
            456285,
            49396,
            12,
            1,
            Some(ScalarUnsupported::FamilySelection),
            ResultUseBlocker::TypedInput,
            Some(GoalState::Unresolved(GoalBlocker::CallableEnvironment)),
        ),
        (
            IMPORTS,
            "function joined(pair: [() => bool, () => Result<bool,i32>]): [bool, Result<bool,i32>] { return join(pair); }\n",
            455804,
            49282,
            12,
            1,
            Some(ScalarUnsupported::FamilySelection),
            ResultUseBlocker::TypedInput,
            Some(GoalState::Unresolved(GoalBlocker::CallableEnvironment)),
        ),
        (
            IMPORTS,
            "enum Fake<T,E> { Ok(T), Err(E) } function joined(): [Fake<bool,i32>, bool] { return join([(): Fake<bool,i32> => Fake.Ok(true), () => false]); }\n",
            457538,
            49784,
            12,
            0,
            None,
            ResultUseBlocker::CallBoundary,
            None,
        ),
        (
            BASE,
            "\nimport std::routine::join;\nfunction joined(): void {\n    join([() => Result.Ok(true), () => 29i32]);\n}\n",
            520229,
            56144,
            12,
            0,
            Some(ScalarUnsupported::FamilySelection),
            ResultUseBlocker::TypedInput,
            None,
        ),
    ] {
        fs::write(&entry, format!("{base}{source}")).unwrap();
        let parent = AnalysisLimits {
            max_work: work,
            max_entries: entries,
            max_depth: depth,
        };
        for test_mode in [false, true] {
            let graph = load(test_mode);
            for (limits, report) in [
                (parent, analyze_typed(&graph, parent)),
                exact_replay(&graph),
            ] {
                if report.completion() == TypedCompletion::Incomplete
                    && let Some(state) = goal_state
                {
                    let module = graph
                        .modules()
                        .iter()
                        .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
                        .unwrap();
                    let node = module
                        .syntax
                        .nodes
                        .iter()
                        .position(|n| {
                            n.kind == NodeKind::Call && n.span.bytes().starts_with(b"join")
                        })
                        .unwrap();
                    let call = report
                        .bodies()
                        .ordinary_call(module, node)
                        .unwrap()
                        .unwrap();
                    let goal = report.bodies().goal(call.goal.as_ref().unwrap()).unwrap();
                    let result = &goal.output.as_ref().unwrap().result;
                    assert_eq!(call.checked, state == GoalState::Proved);
                    assert_eq!(goal.state, state);
                    assert!(call.premise.is_none());
                    assert!(matches!(call.output_agreement, OutputAgreement::Agrees));
                    assert_eq!(call.selected_result.as_ref(), Some(result));
                    if call.checked {
                        let facts = report.bodies().node(module, node).unwrap().unwrap();
                        let NodeFactKind::Expression(value) = &facts.kind else {
                            panic!("expected checked call expression");
                        };
                        assert_eq!(&value.ty, result);
                    }
                    assert!(
                        matches!(result.as_ref(), TypeTerm::Tuple(fields) if fields.len() == 2
                            && matches!(fields[result_slot].as_ref(), TypeTerm::Nominal { declaration, arguments }
                                if *declaration == &report.result_role().unwrap().declaration.id
                                    && matches!(arguments.as_slice(), [ok, err] if primitive(ok, "bool") && primitive(err, "i32")))
                            && primitive(&fields[1 - result_slot], "bool"))
                    );
                }
                let uses = report.result_use();
                let first_use = uses.and_then(|uses| {
                    uses.pending()
                        .iter()
                        .find(|pending| pending.site.module.origin == Origin::Application)
                });
                let first_obligation = report
                    .bodies()
                    .obligations()
                    .iter()
                    .find(|obligation| obligation.site.module.origin == Origin::Application);
                let first_obligation = first_obligation.map(|o| o.reason);
                let first_use = first_use.map(|u| u.blocker);
                let findings =
                    report.bodies().findings().len() + report.public_interface_findings().len();
                let discards = uses.map(|uses| uses.discards().len());
                if report.completion() != TypedCompletion::Incomplete
                    || report.receipt().is_some()
                    || findings != 0
                    || discards != Some(0)
                    || first_obligation != obligation
                    || first_use != Some(blocker)
                {
                    failures.push(format!("{source} test_mode={test_mode}, {limits:?}: {:?} {:?}, first obligation: {first_obligation:?}, first use: {first_use:?}, findings: {findings}, discards: {discards:?}", report.completion(), report.statistics()));
                }
            }
        }
    }
    // An unavailable handoff cannot hide the exit of unrelated caller storage.
    for (source, exit) in [
        (
            "function joined(): void {\n    let independent = Err<bool,i32>(1i32);\n    join([() => Err<bool,i32>(7i32), () => !false]);\n}\n",
            "{\n    let independent = Err<bool,i32>(1i32);\n    join([() => Err<bool,i32>(7i32), () => !false]);\n}",
        ),
        (
            "function joined(value: Result<bool,i32>): [Result<bool,i32>, bool] {\n    let independent = Err<bool,i32>(1i32);\n    return join([() => value, () => false]);\n}\n",
            "return join([() => value, () => false]);",
        ),
    ] {
        fs::write(&entry, format!("{IMPORTS}{source}")).unwrap();
        for test_mode in [false, true] {
            let graph = load(test_mode);
            let (_, report) = exact_replay(&graph);
            let discards = report
                .result_use()
                .unwrap()
                .discards()
                .iter()
                .map(|finding| {
                    (
                        finding.code(),
                        finding.kind,
                        finding.origin.span.bytes(),
                        finding.discard.span.bytes(),
                    )
                })
                .collect::<Vec<_>>();
            if discards
                != [(
                    "R0103",
                    ResultDiscardKind::ScopeExit,
                    b"Err<bool,i32>(1i32)".as_slice(),
                    exit.as_bytes(),
                )]
            {
                failures.push(format!("{source} test_mode={test_mode}: {discards:?}"));
            }
        }
    }
    // Use the checked result after destructuring: both diagnostics need the
    // real bool type, even though Result-use could not transfer the join.
    let source = "function probe(): bool { let [r, b] = join([() => Err<bool,i32>(7i32), () => !false]); let _ = r; return b + 1i32; }";
    fs::write(&entry, format!("{IMPORTS}{source}")).unwrap();
    for test_mode in [false, true] {
        let graph = load(test_mode);
        let (_, report) = exact_replay(&graph);
        assert!(report.receipt().is_none());
        let [binary, operand] = report.bodies().findings() else {
            panic!(
                "expected the expression and operand findings: {:?}",
                report.bodies().findings()
            );
        };
        assert_eq!(binary.kind.code(), Some("R0100"));
        assert_eq!(binary.site.span.bytes(), b"b + 1i32");
        assert_eq!(operand.kind.code(), Some("R0100"));
        assert_eq!(operand.site.span.bytes(), b"b");
        assert!(primitive(operand.actual.as_ref().unwrap(), "bool"));
        assert!(primitive(operand.expected.as_ref().unwrap(), "i32"));
        assert!(report.result_use().unwrap().discards().is_empty());
    }
    // A receiving annotation cannot reorder the independent output.
    let source = "function probe(value: Result<bool,i32>): [bool, Result<bool,i32>] { return join([() => value, () => false]); }";
    fs::write(&entry, format!("{IMPORTS}{source}")).unwrap();
    for test_mode in [false, true] {
        let graph = load(test_mode);
        let (_, report) = exact_replay(&graph);
        assert_eq!(report.completion(), TypedCompletion::SourceRejected);
        assert!(report.receipt().is_none());
        let [finding] = report.bodies().findings() else {
            panic!(
                "expected the tuple order finding: {:?}",
                report.bodies().findings()
            );
        };
        assert_eq!(finding.kind.code(), Some("R0100"));
        assert_eq!(
            finding.site.span.bytes(),
            b"join([() => value, () => false])"
        );
        let (Some(TypeTerm::Tuple(actual)), Some(TypeTerm::Tuple(expected))) =
            (finding.actual.as_deref(), finding.expected.as_deref())
        else {
            panic!("expected tuple types: {finding:?}");
        };
        assert_eq!(actual[0], expected[1]);
        assert_eq!(actual[1], expected[0]);
        assert!(primitive(&actual[1], "bool"));
        assert!(report.result_use().unwrap().discards().is_empty());
    }
    // Check both independent comparisons: the written output filter and the
    // receiving annotation can disagree with the result fixed by the inputs.
    for (written, receiving) in [
        ("[bool, Result<bool,i32>]", "[Result<bool,i32>,bool]"),
        ("[Result<bool,i32>,bool]", "[bool, Result<bool,i32>]"),
    ] {
        let call = format!(
            "join<[() => bool, () => Result<bool,i32>], {written}>([() => flag, () => Err<bool,i32>(7i32)])"
        );
        fs::write(
            &entry,
            format!("{IMPORTS}function joined(flag: bool): {receiving} {{ return {call}; }}\n"),
        )
        .unwrap();
        for mode in [false, true] {
            let graph = load(mode);
            let (_, report) = exact_replay(&graph);
            assert_eq!(report.completion(), TypedCompletion::SourceRejected);
            let [finding] = report.bodies().findings() else {
                panic!(
                    "expected one explicit output mismatch: {:?}",
                    report.bodies().findings()
                );
            };
            assert_eq!(finding.kind.code(), Some("R0100"));
            assert_eq!(finding.site.span.bytes(), call.as_bytes());
            let (Some(TypeTerm::Tuple(actual)), Some(TypeTerm::Tuple(expected))) =
                (finding.actual.as_deref(), finding.expected.as_deref())
            else {
                panic!("expected tuple types: {finding:?}");
            };
            assert!(primitive(&actual[0], "bool"));
            assert_eq!(actual[0], expected[1]);
            assert_eq!(actual[1], expected[0]);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn exact_replay(graph: &loader::Graph) -> (AnalysisLimits, TypedReport<'_>) {
    let report = analyze_typed(graph, AnalysisLimits::default());
    let stats = report.statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    let replay = analyze_typed(graph, exact);
    assert_eq!(replay.completion(), report.completion());
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
        let limited = analyze_typed(graph, short);
        crate::limit::assert_typed_limit(&limited);
    }
    (exact, replay)
}

fn primitive(ty: &compiler::analysis::Type<'_>, name: &str) -> bool {
    matches!(ty.as_ref(), TypeTerm::Primitive(p) if p.name() == name)
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
