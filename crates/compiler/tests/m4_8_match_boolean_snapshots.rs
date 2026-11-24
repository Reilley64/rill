//! Boolean match snapshots and Result residences at the installed receipt boundary.
// The installed M4 profile that can issue a receipt is macOS AArch64.
#![cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[path = "support/limit.rs"]
mod limit;
mod support;

use compiler::analysis::AnalysisLimits;
use compiler::analysis::scalars::ScalarFindingKind;
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
        installed(
            &probe,
            name,
            &source,
            fixed.map(|(w, e)| (w, e, 12)),
            checked,
        );
    }

    for (name, statement, fixed) in [
        (
            "discarded literal result",
            "let _ = match (b@) { true => true, false => false };",
            (395301, 73732),
        ),
        (
            "discarded binder result",
            "let _ = match (b@) { v => v };",
            (326406, 48590),
        ),
        (
            "unused expression result",
            "match (b@) { v => v };",
            (326366, 48590),
        ),
    ] {
        let parameters = (0..10)
            .map(|i| format!("b{i}: bool"))
            .collect::<Vec<_>>()
            .join(", ");
        let statements = (0..10)
            .map(|i| statement.replace('@', &i.to_string()))
            .collect::<Vec<_>>()
            .join(" ");
        let source = format!("\nfunction probe({parameters}): void {{ {statements} }}\n");
        installed(&probe, name, &source, Some((fixed.0, fixed.1, 12)), checked);
    }
    for (n, pattern, helper, innermost, fixed) in [
        (11, "_", "", "", Some((326724, 48624, 46))),
        (
            10,
            "_",
            "function noop(): void {}\n",
            "noop();",
            Some((327586, 48557, 45)),
        ),
        (
            10,
            "(snapshot)",
            "function noop(): void {}\n",
            "noop();",
            None,
        ),
    ] {
        let parameters = (0..n)
            .map(|i| format!("b{i}: bool"))
            .collect::<Vec<_>>()
            .join(", ");
        let body = (0..n).rev().fold(innermost.to_owned(), |body, i| {
            format!("match (b{i}) {{ {pattern} => {{ {body} }} }};")
        });
        let source =
            format!("\n{helper}function probe({parameters}): bool {{ {body} return true; }}\n");
        installed(
            &probe,
            "nested unobserved snapshots",
            &source,
            fixed,
            checked,
        );
    }
    for (name, before, scrutinee, body, fixed) in [
        ("call14", "", "b@", "noop();", (331637, 49402, 12)),
        ("split14", "", "b@", "if (common) {}", (331989, 49850, 12)),
        (
            "known14",
            "let known = true;",
            "known",
            "if (common) {}",
            (333400, 50513, 12),
        ),
    ] {
        let parameters = (0..14).map(|i| format!(", b{i}: bool")).collect::<String>();
        let matches = (0..14)
            .map(|i| {
                format!(
                    "match ({}) {{ v => {{ {body} }} }}; ",
                    scrutinee.replace('@', &i.to_string())
                )
            })
            .collect::<String>();
        let source = format!(
            "\nfunction noop(): void {{}}
            function probe(common: bool{parameters}): bool {{ {before} {matches}return true; }}\n"
        );
        installed(&probe, name, &source, Some(fixed), checked);
    }

    // The two arms create the same equality classes with different labels.
    let parameters = (0..12).map(|i| format!(", b{i}: bool")).collect::<String>();
    let matches = (0..12)
        .map(|i| {
            format!(
                "match (b{i}) {{ true => {{ {arm} }}, false => {{ {arm} }} }};\n",
                arm = "invalidate(&mut current); output = match (current) { _ => current };"
            )
        })
        .collect::<String>();
    let source = format!(
        "\nfunction invalidate(flag: &mut bool): void {{}}
        function probe(flag: bool{parameters}): bool {{
            let mut current = flag; let mut output = false;
            {matches}
            return output;
        }}\n"
    );
    installed(
        &probe,
        "equivalent arm-local copy classes",
        &source,
        Some((AnalysisLimits::default().max_work, 54002, 12)),
        checked,
    );

    for (groups, depth) in [(0, 23), (15, 38)] {
        let source = format!(
            "\nfunction probe(flag: bool): bool {{ match (flag) {{ v => {{ {}let _ = {}v{};{} }} }}; return true; }}\n",
            "{".repeat(15),
            "(".repeat(groups),
            ")".repeat(groups),
            "}".repeat(15)
        );
        installed(
            &probe,
            "written discarded read depth",
            &source,
            None,
            |report| {
                checked(report);
                assert_eq!(report.statistics().max_depth, depth);
            },
        );
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
        "used match result remains correlated",
        r#"
        function probe(flag: bool): void {
            let debt = Err<bool,i32>(61i32);
            let copy = match (flag) { true => true, false => false };
            if (flag) { if (copy) { let _ = debt; } }
            else { if (!copy) { let _ = debt; } }
        }
        "#,
        None,
        checked,
    );
    installed(
        &probe,
        "wildcard arm result remains correlated",
        r#"
        function probe(flag: bool): void {
            let debt = Err<bool,i32>(61i32);
            let copy = match (flag) { _ => flag };
            if (flag) { if (copy) { let _ = debt; } }
            else { if (!copy) { let _ = debt; } }
        }
        "#,
        None,
        checked,
    );
    for (name, source) in [
        (
            "nested wildcard and guard observe the same copy",
            "
            function probe(flag: bool, outer: bool): void {
                let debt = Err<bool,i32>(61i32);
                match (outer) { _ => {
                    match (flag) { _ => {
                        let copy = flag;
                        match (copy) {
                            true if flag => { let _ = debt; },
                            false if !flag => { let _ = debt; },
                            _ => {}
                        };
                    } };
                } };
            }
            ",
        ),
        (
            "wildcard arm copies through a projected alias",
            "
            type Flags = { on: bool; };
            function probe(value: Flags): void {
                let view = &value;
                let debt = Err<bool,i32>(61i32);
                let copy = match ((*view).on) { (_) => value.on };
                if ((*view).on) { if (copy) { let _ = debt; } }
                else { if (!copy) { let _ = debt; } }
            }
            ",
        ),
        (
            "wildcard copy is detached on known writeback",
            "
            function probe(flag: bool): void {
                let mut current = flag;
                let mut copy = false;
                let debt = Err<bool,i32>(61i32);
                match (current) { _ => { copy = current; current = !current; } };
                if (current) { if (!copy) { let _ = debt; } }
                else { if (copy) { let _ = debt; } }
            }
            ",
        ),
        (
            "mutable call detaches the origin but preserves equal copies",
            "
            function invalidate(flag: &mut bool): void {}
            function probe(flag: bool): void {
                let mut current = flag;
                let mut first = false;
                let mut second = false;
                let debt = Err<bool,i32>(61i32);
                match (current) { _ => { first = current; second = current; invalidate(&mut current); } };
                if (first) { if (second) { let _ = debt; } }
                else { if (!second) { let _ = debt; } }
            }
            ",
        ),
        (
            "nested loop break and continue retain the feasible copy",
            "
            function probe(flag: bool): void {
                let mut current = flag;
                let mut copy = false;
                let debt = Err<bool,i32>(61i32);
                while (true) {
                    match (current) { _ => {
                        copy = current;
                        if (copy) { break; }
                        current = true;
                        continue;
                    } };
                }
                if (copy) { let _ = debt; }
            }
            ",
        ),
        (
            "loop equality ignores fresh labels but keeps distinct copied fields",
            "
            function unknown(flag: bool): bool { return flag; }
            function probe(flag: bool, other: bool, stop: bool): void {
                let mut saved: [bool,bool,bool,bool] = [false, false, false, false];
                let debt = Err<bool,i32>(61i32);
                while (true) {
                    let left = unknown(flag);
                    let right = unknown(other);
                    match (left) { a => {
                        match (right) { b => {
                            saved = [left, right, a, b];
                            if (stop) { break; }
                            continue;
                        } };
                    } };
                }
                if (saved[0]) { if (saved[2]) { let _ = debt; } }
                else { if (!saved[2]) { let _ = debt; } }
            }
            ",
        ),
        (
            "grouped binder snapshot survives a failed guard that changes the source",
            "
            function invalidate(flag: &mut bool): bool { return false; }
            function probe(flag: bool): void {
                let mut current = flag;
                let debt = Err<bool,i32>(61i32);
                match (current) { _ => {
                    let prior = current;
                    match (current) {
                        (snapshot) if invalidate(&mut current) => { let _ = debt; return; },
                        true => { let _ = debt; },
                        false => {}
                    };
                    if (!prior) { let _ = debt; }
                } };
            }
            ",
        ),
    ] {
        installed(&probe, name, source, None, checked);
    }
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
        unpaid,
    );
    installed(
        &probe,
        "replacement does not acknowledge the old false path",
        "
        function probe(flag: bool): void {
            let mut current = flag;
            let mut copy = false;
            let debt = Err<bool,i32>(61i32);
            match (current) { _ => { copy = current; current = true; } };
            if (current) { if (copy) { let _ = debt; } }
        }
        ",
        None,
        unpaid,
    );
    for (name, source) in [
        (
            "overlapping labels do not make distinct copy classes equal",
            "
            function probe(left: bool, right: bool, gate: bool): void {
                let debt = Err<bool,i32>(61i32);
                match (left) { _ => {
                    let first = left;
                    match (right) { _ => {
                        let second = right;
                        let selected = match (gate) { true => first, false => second };
                        if (first) { if (selected) { let _ = debt; } }
                        else { if (!selected) { let _ = debt; } }
                    } };
                } };
            }
            ",
        ),
        (
            "different alias targets keep separate alternatives",
            "
            function probe(left: bool, right: bool, gate: bool): void {
                let debt = Err<bool,i32>(61i32);
                let selected = match (gate) { true => &left, false => &right };
                if (left) { if (*selected) { let _ = debt; } }
                else { if (!*selected) { let _ = debt; } }
            }
            ",
        ),
        (
            "distinct debts at the same residence keep separate alternatives",
            "
            function probe(gate: bool): void {
                let debt: Result<bool,i32> = match (gate) {
                    true => Err<bool,i32>(60i32), false => Err<bool,i32>(61i32)
                };
                if (gate) { let _ = debt; }
            }
            ",
        ),
    ] {
        installed(&probe, name, source, None, unpaid);
    }
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

    // Reviews 35776287 and f67c0458 at db944e6: a single wildcard arm that copies its
    // source, and nested wildcard matches whose bodies copy and observe it. Verbatim,
    // including the reviewers' `std::routine::join` import.
    // Review e519e598 at 2ec4f36: two false R0103s that predate bd10617. `pre` copies
    // the Boolean before the match begins tracking it; `computed` matches through an
    // alias selected by an `if` expression, which has no typed source place.
    // Both check since ri-2s0.4 (C12).
    // Exact (work, entries, depth), the same in both load modes.
    for (name, source, exact, check) in [
        (
            "review 35776287: wildcard arm copies the source",
            "
import std::routine::join;
function probe(flag: bool): void {
  let debt = Err<bool,i32>(61i32);
  let copy = match (flag) { _ => flag };
  if (flag) { if (copy) { let _ = debt; } }
  else { if (!copy) { let _ = debt; } }
}
",
            (426696, 48863, 12),
            checked as fn(&TypedReport<'_>),
        ),
        (
            "review 35776287: nested wildcard copy observed by guards",
            "
import std::routine::join;
function probe(flag: bool, outer: bool): void {
  let debt = Err<bool,i32>(61i32);
  match (outer) { _ => {
    match (flag) { _ => {
      let copy = flag;
      match (copy) { true if flag => { let _ = debt; }, false if !flag => { let _ = debt; }, _ => {} };
    } };
  } };
}
",
            (427764, 49046, 16),
            checked,
        ),
        (
            "review f67c0458: nested guarded wildcard copies the source",
            "
import std::routine::join;
function probe(flag: bool, gate: bool): void {
    let debt = Err<bool,i32>(61i32);
    let mut copy = false; match (flag) { _ => { match (gate) { _ if gate => { copy = flag; }, _ => { copy = flag; } }; } };
    if (flag) { if (copy) { let _ = debt; } }
    else { if (!copy) { let _ = debt; } }
}
",
            (428401, 49120, 12),
            checked,
        ),
        (
            "review e519e598 pre: copy taken before the match",
            "
import std::routine::join;
function probe(flag: bool): void { let debt = Err<bool,i32>(61i32); let before = flag; let copy = match (flag) { _ => before }; if (flag) { if (copy) { let _ = debt; } } else { if (!copy) { let _ = debt; } } }
",
            (426895, 48892, 12),
            checked,
        ),
        (
            "review e519e598 computed: alias selected by an if expression",
            "
import std::routine::join;
function probe(flag: bool, gate: bool): void { let debt = Err<bool,i32>(61i32); match (*(if (gate) &flag else &flag)) { true if flag => { let _ = debt; }, false if !flag => { let _ = debt; }, _ => {} }; }
",
            (427988, 49104, 12),
            checked,
        ),
    ] {
        installed(&probe, name, source, None, |report| {
            check(report);
            let stats = report.statistics();
            assert_eq!((stats.work, stats.entries, stats.max_depth), exact, "{name}");
        });
    }

    // ri-2s0.4 (C12). Both rows cost less than the false R0103 they replace, so their
    // parent caps still hold: the old exact numbers, measured at 7002855e, now give Checked.
    for (name, source, old) in [
        ("pre", PRE, (426921, 48910, 12)),
        ("computed", COMPUTED, (428253, 49198, 12)),
    ] {
        installed(&probe, name, source, Some(old), checked);
    }

    // Grouped forms check like the bare control. The stdlib sets depth 12 for every row, so
    // grouping shows in work and entries; one short on depth still gives `ResourceLimit`.
    for (name, source, exact) in [
        (
            "pre: bare control",
            pre("let before = flag;", "before"),
            (324672, 48105, 12),
        ),
        (
            "pre: grouped initializer",
            pre("let before = (flag);", "before"),
            (324704, 48117, 12),
        ),
        (
            "pre: grouped binder",
            pre("let (before) = flag;", "before"),
            (324698, 48117, 12),
        ),
        (
            "pre: grouped arm body",
            pre("let before = flag;", "(before)"),
            (324742, 48122, 12),
        ),
        (
            "computed: grouped scrutinee",
            COMPUTED.replace(
                "match (*(if (gate) &flag else &flag))",
                "match (((*(if (gate) &flag else &flag))))",
            ),
            (325778, 48356, 12),
        ),
    ] {
        installed(&probe, name, &source, None, |report| {
            checked(report);
            let stats = report.statistics();
            assert_eq!(
                (stats.work, stats.entries, stats.max_depth),
                exact,
                "{name}"
            );
        });
    }

    // Review ci-0mdr Q4 (SP-7, ri-px1.11.2). A mutable local that is never written is as
    // good a copy as an immutable one, so the first two rows check with a receipt. Their
    // parent numbers are the rejections measured at da5bec50 (both paid before their
    // `R0103`, rule 1 exception ri-2s0.10). The third row is the same source with an
    // immutable local: it shares the screen, so it checks too. Measured at a4e56651 (C3's
    // base) it was the same `R0103` as the mutable source, 324529 / 48133 / 12, and it
    // costs less now, so that parent cap still gives `Checked`.
    for (name, source, parent, exact) in [
        (
            "pre: mutable binder",
            pre("let mut before = flag;", "before"),
            Some((324709, 48123, 12)),
            (324714, 48105, 12),
        ),
        (
            "pre: mutable source",
            MUTABLE_SOURCE.to_owned(),
            Some((324529, 48133, 12)),
            (324568, 48115, 12),
        ),
        (
            "pre: immutable local source",
            MUTABLE_SOURCE.replace("let mut current", "let current"),
            Some((324529, 48133, 12)),
            (324518, 48115, 12),
        ),
    ] {
        let holds = parent.is_some_and(|parent| exact.0 <= parent.0 && exact.1 <= parent.1);
        let over_parent = parent.filter(|_| !holds);
        installed_over_parent(
            &probe,
            name,
            &source,
            parent.filter(|_| holds),
            over_parent,
            |report| {
                checked(report);
                let stats = report.statistics();
                assert_eq!(
                    (stats.work, stats.entries, stats.max_depth),
                    exact,
                    "{name}"
                );
            },
        );
    }

    // Genuine rejections stay R0103: the written value is not the matched source. Exact
    // numbers at 7002855e, then here. `pre: copy of another parameter` pays a few steps to
    // read the written binding before it rejects, so its parent cap no longer holds. The
    // `computed` row costs no more. Rule 1 exception (chief of staff, 2026-11-23,
    // ri-2s0.10): `pre: copy of another parameter` (+14 work) pays before `R0103`, and its
    // parent cap (7002855e) gives `ResourceLimit`. The write twins pay nothing before
    // their rejection that the parents did not: their parent numbers are the ones measured
    // at 81d317b3, before the widened gate (review ci-b4b2 A-F1, ri-px1.11.9), and each
    // parent cap still holds. Named rule-1 exception (chief of staff, 2026-11-24 16:53
    // AEDT, ri-j6fm; `compiler-metering.md` rule 1): the two `ri-j6fm` rows write or lend a
    // different local after the source, so the free `last_written` screen no longer names
    // the source and it pays its scan before the same `R0103` (+86 and +90 work over the
    // parents measured at 81d317b3); each parent cap gives `ResourceLimit`.
    for (name, source, parent, exact) in [
        (
            "pre: copy of another parameter",
            pre("let before = other;", "before"),
            Some((324698, 48123, 12)),
            (324712, 48123, 12),
        ),
        (
            "pre: mutable binder reassigned before the match",
            pre("let mut before = flag; before = !flag;", "before"),
            Some((325493, 48441, 12)),
            (325493, 48441, 12),
        ),
        (
            "pre: mutable source reassigned after the copy",
            MUTABLE_SOURCE.replace(
                "let before = current;",
                "let before = current; current = !flag;",
            ),
            Some((325403, 48455, 12)),
            (325403, 48455, 12),
        ),
        (
            "pre: mutable source written, then another local written (ri-j6fm, X2)",
            MUTABLE_SOURCE.replace(
                "let before = current;",
                "let before = current; current = !flag; let mut z = flag; z = !flag;",
            ),
            Some((326097, 48707, 12)),
            (326183, 48707, 12),
        ),
        (
            "pre: mutable source written, then another local lent mutably (ri-j6fm, X3)",
            MUTABLE_SOURCE.replace(
                "let before = current;",
                "let before = current; current = !flag; let mut z = flag; let w = &mut z;",
            ),
            Some((326044, 48695, 12)),
            (326134, 48695, 12),
        ),
        (
            "computed: alias selects another place",
            COMPUTED.replace("&flag else &flag", "&flag else &other"),
            Some((325928, 48413, 12)),
            (325886, 48394, 12),
        ),
    ] {
        let holds = parent.is_some_and(|parent| exact.0 <= parent.0 && exact.1 <= parent.1);
        let over_parent = parent.filter(|_| !holds);
        installed_over_parent(
            &probe,
            name,
            &source,
            parent.filter(|_| holds),
            over_parent,
            |report| {
                unpaid(report);
                let stats = report.statistics();
                assert_eq!(
                    (stats.work, stats.entries, stats.max_depth),
                    exact,
                    "{name}"
                );
            },
        );
    }

    // Review ci-0mdr Q4 (SP-7), chief of staff 2026-11-24 14:50 AEDT, condition 3. The
    // immutable local `current` copies the parameter `flag`; neither can be written. Both
    // writes between the copy and the match are source errors (`ReadOnly`), so the screen
    // never sees a written source. `pre: mutable source reassigned after the copy` above
    // covers a source that can be written.
    for (name, edit, write, exact) in [
        (
            "pre: immutable local source reassigned after the copy",
            "let before = current; current = !flag;",
            b"current = !flag;".as_slice(),
            (324920, 48237, 12),
        ),
        (
            "pre: immutable local source, its parameter written after the copy",
            "let before = current; flag = !flag;",
            b"flag = !flag;".as_slice(),
            (324753, 48171, 12),
        ),
    ] {
        let source = MUTABLE_SOURCE
            .replace("let mut current", "let current")
            .replace("let before = current;", edit);
        installed_read_only(&probe, name, &source, write, exact);
    }

    // A mutable borrow of the copy or the source stays `Incomplete` (Storage) at the
    // `*value = !*value;` write inside `flip`, with the unpaid `R0103` still published,
    // whatever the match does, so neither can reach `Checked`. Parent numbers measured at
    // 81d317b3 (review ci-b4b2 A-F1, ri-px1.11.9): same outcome and blocker, and the
    // rejection costs no more than it did there.
    for (name, source, exact) in [
        (
            "pre: mutable binder lent mutably before the match",
            format!(
                "{FLIP}{}",
                pre("let mut before = flag; flip(&mut before);", "before")
            ),
            (325886, 48178, 12),
        ),
        (
            "pre: mutable source lent mutably after the copy",
            format!(
                "{FLIP}{}",
                MUTABLE_SOURCE.replace(
                    "let before = current;",
                    "let before = current; flip(&mut current);",
                )
            ),
            (325702, 48188, 12),
        ),
    ] {
        installed(&probe, name, &source, Some(exact), |report| {
            assert_eq!(report.completion(), TypedCompletion::Incomplete, "{name}");
            assert!(report.receipt().is_none());
            let uses = report.result_use().unwrap();
            let [pending] = uses.pending() else {
                panic!("{:?}", uses.pending())
            };
            assert_eq!(pending.blocker, ResultUseBlocker::Storage);
            assert_eq!(pending.site.span.bytes(), b"*value = !*value;", "{name}");
            let discards = report.result_use().unwrap().discards();
            assert_eq!(discards.len(), 1, "{name}");
            assert_eq!(discards[0].code(), "R0103", "{name}");
            let stats = report.statistics();
            assert_eq!(
                (stats.work, stats.entries, stats.max_depth),
                exact,
                "{name}"
            );
        });
    }
}

const MUTABLE_SOURCE: &str = "
function probe(flag: bool): void {
  let debt = Err<bool,i32>(61i32);
  let mut current = flag;
  let before = current;
  let copy = match (current) { _ => before };
  if (current) { if (copy) { let _ = debt; } } else { if (!copy) { let _ = debt; } }
}
";

const FLIP: &str = "
function flip(value: &mut bool): void { *value = !*value; }
";

const PRE: &str = "
function probe(flag: bool): void { let debt = Err<bool,i32>(61i32); let before = flag; let copy = match (flag) { _ => before }; if (flag) { if (copy) { let _ = debt; } } else { if (!copy) { let _ = debt; } } }
";

const COMPUTED: &str = "
function probe(flag: bool, other: bool, gate: bool): void { let debt = Err<bool,i32>(61i32); match (*(if (gate) &flag else &flag)) { true if flag => { let _ = debt; }, false if !flag => { let _ = debt; }, _ => {} }; }
";

fn pre(binding: &str, arm: &str) -> String {
    format!(
        "
function probe(flag: bool, other: bool): void {{ let debt = Err<bool,i32>(61i32); {binding} let copy = match (flag) {{ _ => {arm} }}; if (flag) {{ if (copy) {{ let _ = debt; }} }} else {{ if (!copy) {{ let _ = debt; }} }} }}
"
    )
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

fn unpaid(report: &TypedReport<'_>) {
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
}

fn installed(
    probe: &std::path::Path,
    name: &str,
    source: &str,
    fixed: Option<(usize, usize, usize)>,
    check: impl Fn(&TypedReport<'_>),
) {
    installed_over_parent(probe, name, source, fixed, None, check);
}

/// As `installed`, and when `over_parent` names a cap the candidate no longer
/// fits, that parent cap gives `ResourceLimit` with no receipt.
fn installed_over_parent(
    probe: &std::path::Path,
    name: &str,
    source: &str,
    fixed: Option<(usize, usize, usize)>,
    over_parent: Option<(usize, usize, usize)>,
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
                .any(|f| f.site.module.origin == Origin::Application),
            "{name}, test={test_mode}: {:?}",
            report.bodies().findings()
        );
        eprintln!("{name}, test={test_mode}: {:?}", report.statistics());
        check(&report);
        if let Some((max_work, max_entries, max_depth)) = fixed {
            let fixed = analyze_typed(
                &graph,
                AnalysisLimits {
                    max_work,
                    max_entries,
                    max_depth,
                },
            );
            check(&fixed);
        }
        if let Some((max_work, max_entries, max_depth)) = over_parent {
            let parent = analyze_typed(
                &graph,
                AnalysisLimits {
                    max_work,
                    max_entries,
                    max_depth,
                },
            );
            crate::limit::assert_typed_limit(&parent);
        }
        exact_and_one_short(&graph, &report, name, &check);
    }
}

fn exact_and_one_short(
    graph: &compiler::loader::Graph,
    report: &TypedReport<'_>,
    name: &str,
    check: &impl Fn(&TypedReport<'_>),
) {
    let exact = replay_exact(graph, report, name, check);
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
        let stopped = analyze_typed(graph, cap);
        crate::limit::assert_typed_limit(&stopped);
    }
}

fn replay_exact(
    graph: &compiler::loader::Graph,
    report: &TypedReport<'_>,
    name: &str,
    check: &impl Fn(&TypedReport<'_>),
) -> AnalysisLimits {
    let stats = report.statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    let replay = analyze_typed(graph, exact);
    assert_eq!(replay.statistics(), stats, "{name}");
    check(&replay);
    exact
}

/// A write the source language forbids is rejected before Result use: one `ReadOnly`
/// finding at the statement `write`, `Incomplete`, no receipt, the same numbers on replay at
/// the exact cap and `ResourceLimit` one short on each meter, in both load modes.
fn installed_read_only(
    probe: &std::path::Path,
    name: &str,
    source: &str,
    write: &[u8],
    exact: (usize, usize, usize),
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
        eprintln!("{name}, test={test_mode}: {:?}", report.statistics());
        let check = |report: &TypedReport<'_>| {
            let found: Vec<_> = report
                .bodies()
                .findings()
                .iter()
                .filter(|f| f.site.module.origin == Origin::Application)
                .map(|f| (f.kind, f.site.span.bytes()))
                .collect();
            assert_eq!(found, [(ScalarFindingKind::ReadOnly, write)], "{name}");
            assert_eq!(report.completion(), TypedCompletion::Incomplete, "{name}");
            assert!(report.receipt().is_none(), "{name}");
            let stats = report.statistics();
            assert_eq!(
                (stats.work, stats.entries, stats.max_depth),
                exact,
                "{name}"
            );
        };
        check(&report);
        exact_and_one_short(&graph, &report, name, &check);
    }
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
