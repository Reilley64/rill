//! Boolean match snapshots and Result residences at the installed receipt boundary.
mod support;

use compiler::analysis::AnalysisLimits;
use compiler::analysis::typed::{ResultUseBlocker, TypedCompletion, TypedReport, analyze_typed};
use compiler::loader::{InstalledRequest, Origin, load_installed};
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
";

#[test]
fn installed_boolean_matches() {
    let probe = std::env::current_exe().unwrap();
    if probe.file_name().unwrap() != "boolean-match-probe" {
        let root = std::env::temp_dir().join(format!("rill-boolean-match-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let runtime = root.join("lib/rill/runtime/aarch64-apple-darwin");
        for directory in ["bin", "app", "lib/rill/std"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::create_dir_all(&runtime).unwrap();
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
            Command::new(root.join("bin/boolean-match-probe")).current_dir(root.join("app")),
        )
        .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    // Fixed work/entry caps measured with the same installed inventory at 78df2be.
    let cases = [
        ("wildcard", "_ => {}", Some((325927, 48411))),
        ("unused binder", "v => {}", Some((326277, 48521))),
        (
            "acknowledged binder",
            "v => { let _ = v; }",
            Some((326739, 48681)),
        ),
        (
            "literal arms",
            "true => {}, false => {}",
            Some((381828, 66402)),
        ),
        ("local copy", "v => { let copy = v; let _ = copy; }", None),
    ];
    for (name, arms, fixed) in cases {
        let parameters = (0..10)
            .map(|i| format!("b{i}: bool"))
            .collect::<Vec<_>>()
            .join(", ");
        let matches = (0..10)
            .map(|i| format!("match (b{i}) {{ {arms} }}; "))
            .collect::<String>();
        let source = format!("\nfunction probe({parameters}): bool {{ {matches}return true; }}\n");
        installed(&probe, name, &source, fixed, checked);
    }

    installed(
        &probe,
        "dependent debt",
        "
        function probe(flag: bool): void {
            let debt = Err<bool,i32>(61i32);
            match (flag) { true => { let _ = debt; }, false => {} };
            if (!flag) { let _ = debt; }
        }
    ",
        None,
        checked,
    );
    installed(
        &probe,
        "unpaid debt",
        "
        function probe(flag: bool): void {
            let debt = Err<bool,i32>(61i32);
            match (flag) { true => { let _ = debt; }, false => {} };
        }
    ",
        None,
        |report| {
            assert_eq!(report.completion(), TypedCompletion::SourceRejected);
            assert!(report.receipt().is_none());
            let uses = report.result_use().unwrap();
            let [discard] = uses.discards() else {
                panic!("{:?}", uses.discards())
            };
            assert_eq!(
                (discard.code(), discard.origin.span.bytes()),
                ("R0103", b"Err<bool,i32>(61i32)".as_slice())
            );
            assert!(uses.pending().is_empty());
        },
    );
    installed(
        &probe,
        "snapshot copy and updated source",
        "
        function probe(flag: bool): void {
            let mut current = flag;
            let debt = Err<bool,i32>(61i32);
            match (current) { snapshot => {
                current = !current;
                if (snapshot) { let _ = debt; }
            } };
            if (current) { let _ = debt; }
        }
    ",
        None,
        checked,
    );

    installed(
        &probe,
        "guard failure falls through with the same snapshot",
        "
        function probe(flag: bool): void {
            let debt = Err<bool,i32>(61i32);
            match (flag) { snapshot if snapshot => { let _ = debt; }, false => {}, _ => {} };
            if (!flag) { let _ = debt; }
        }
    ",
        None,
        checked,
    );

    for (name, field, target, ack, incomplete) in [
        ("Boolean projection", "", "x", "", false),
        (
            "whole record with Result",
            "owed: Result<bool,i32>;",
            "x",
            "let _ = x;",
            true,
        ),
        (
            "only Boolean field",
            "owed: Result<bool,i32>;",
            "x.on",
            "let _ = x;",
            false,
        ),
    ] {
        let target_type = if target == "x" { "Flags" } else { "bool" };
        let source = format!(
            "
            import std::routine::join;
            type Flags = {{ on: bool; {field} }};
            function mutate(value: &mut {target_type}): void {{}}
            function probe(value: Flags): void {{
                let mut x = value;
                match (x.on) {{ _ => {{ mutate(&mut {target}); }} }};
                {ack}
            }}
        "
        );
        installed(&probe, name, &source, None, |report| {
            if incomplete {
                assert_eq!(report.completion(), TypedCompletion::Incomplete);
                assert!(report.receipt().is_none());
                let uses = report.result_use().unwrap();
                let [pending] = uses.pending() else {
                    panic!("{:?}", uses.pending())
                };
                assert_eq!(pending.blocker, ResultUseBlocker::Storage);
                assert_eq!(pending.site.span.bytes(), b"&mut x");
                assert!(uses.discards().is_empty());
            } else {
                checked(report);
            }
        });
    }
}

fn checked(report: &TypedReport<'_>) {
    assert_eq!(
        report.completion(),
        TypedCompletion::Checked,
        "{:?}",
        report.result_use()
    );
    assert!(report.receipt().is_some());
}

fn installed(
    probe: &std::path::Path,
    name: &str,
    source: &str,
    fixed: Option<(usize, usize)>,
    check: impl Fn(&TypedReport<'_>),
) {
    let entry = probe
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/main.rill");
    fs::write(&entry, format!("{IMPORTS}{source}")).unwrap();
    for test_mode in [false, true] {
        let graph = load_installed(InstalledRequest {
            entry: entry.clone(),
            target: "aarch64-apple-darwin".into(),
            test_mode,
            limits: Default::default(),
            options: Default::default(),
        })
        .unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert!(
            !report
                .bodies()
                .findings()
                .iter()
                .any(|f| f.site.module.origin == Origin::Application)
        );
        eprintln!("{name}, test={test_mode}: {:?}", report.statistics());
        check(&report);
        if let Some((max_work, max_entries)) = fixed {
            let fixed = analyze_typed(
                &graph,
                AnalysisLimits {
                    max_work,
                    max_entries,
                    max_depth: 12,
                },
            );
            check(&fixed);
        }
        let stats = report.statistics();
        let exact = AnalysisLimits {
            max_work: stats.work,
            max_entries: stats.entries,
            max_depth: stats.max_depth,
        };
        let replay = analyze_typed(&graph, exact);
        assert_eq!(replay.statistics(), stats);
        check(&replay);
        for cap in [
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
            let stopped = analyze_typed(&graph, cap);
            assert_eq!(
                stopped.completion(),
                TypedCompletion::ResourceLimit,
                "{name}, test={test_mode}: {cap:?}"
            );
            assert!(stopped.receipt().is_none());
        }
    }
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
