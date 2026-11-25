//! Ticket C2 (`ri-vgz.4`): postfix over an owning `Result<string, i32>`.
//! `Ok` moves the string to its binding, which drops once at lexical exit;
//! `Err` forwards the existing wrapper unchanged before lexical cleanup.
use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{GraphCompletion, analyze_ownership};

const PATHS: &str = r#"
    function bound(input: Result<string, i32>, tail: string): Result<bool, i32> {
        let value = input?;
        return Result.Ok(true);
    }
    function grouped(input: Result<string, i32>, tail: string): Result<bool, i32> {
        let value = ((input))?;
        return Result.Ok(true);
    }
    function discarded(input: Result<string, i32>, tail: string): Result<bool, i32> {
        let _ = input?;
        return Result.Ok(true);
    }
    function knownErr(input: Result<string, i32>.Err, tail: string): Result<bool, i32> {
        let value = input?;
        return Result.Ok(true);
    }
    function knownOk(input: Result<string, i32>.Ok, tail: string): Result<bool, i32> {
        let value = input?;
        return Result.Ok(true);
    }
"#;

/// One relevant MIR effect: its kind and the source text of its place.
type Effect = (&'static str, String);

fn text(place: &compiler::analysis::ownership::Place<'_>) -> String {
    String::from_utf8_lossy(place.root.span.bytes()).into_owned()
}

fn effects(ops: &[&Operation<'_>]) -> Vec<Effect> {
    let mut out = Vec::new();
    for op in ops {
        match op {
            Operation::Move {
                from,
                into: Some(into),
            } => {
                out.push(("move", text(from)));
                out.push(("into", text(into)));
            }
            Operation::TestVariant { operand, .. } => out.push(("test", text(operand))),
            Operation::TakeResultSuccess { operand, into } => {
                out.push(("take", text(operand)));
                out.push(("into", into.as_ref().map(text).unwrap_or_default()));
            }
            Operation::ForwardResultFailure { operand, .. } => out.push(("forward", text(operand))),
            Operation::ConstructVariant { .. } => out.push(("construct", String::new())),
            Operation::Export(_) => out.push(("export", String::new())),
            Operation::Drop(place) => out.push(("drop", text(place))),
            Operation::Cleanup(actions) => {
                for action in actions {
                    match action {
                        Obligation::Drop(place) => out.push(("drop", text(place))),
                        Obligation::EndStorage(_) => {}
                        other => panic!("unexpected cleanup {other:?}"),
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn expect(pairs: &[(&'static str, &str)]) -> Vec<Effect> {
    pairs.iter().map(|(k, p)| (*k, p.to_string())).collect()
}

#[test]
fn owning_text_result_postfix_moves_ok_and_forwards_err_before_cleanup() {
    probe_with_typed(
        "propagation::result_text::owning_text_result_postfix_moves_ok_and_forwards_err_before_cleanup",
        PATHS,
        |_, module, receipt, typed| {
            let role = typed.result_role().unwrap();
            let lower = |name: &str| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let report =
                    lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap();
                assert_eq!(
                    report.completion(),
                    Completion::Checked,
                    "{name} at {:?}",
                    report
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                );
                assert!(report.findings().is_empty());
                report
            };
            let tail = "tail: string";
            for (name, operand, param) in [
                ("bound", "input", "input: Result<string, i32>"),
                ("grouped", "((input))", "input: Result<string, i32>"),
            ] {
                let report = lower(name);
                let mir = report.mir().unwrap();
                let prefix: Vec<_> = straight_blocks(mir, mir.entry)
                    .into_iter()
                    .map(|b| &b.operation)
                    .collect();
                assert_eq!(
                    effects(&prefix),
                    expect(&[("move", param), ("into", operand), ("test", operand)]),
                    "{name}"
                );
                let (ok, err) = variant_edges(mir, mir.entry, &role.ok.id, operand.as_bytes());
                // Ok: the string moves to `value`, which drops once after the export.
                let success = linear(mir, ok);
                assert_eq!(
                    effects(&success),
                    expect(&[
                        ("take", operand),
                        ("into", "value"),
                        ("construct", ""),
                        ("export", ""),
                        ("drop", "value"),
                        ("drop", tail),
                    ]),
                    "{name}"
                );
                assert_return(&success, &role.ok.id, true, &[b"value", tail.as_bytes()]);
                // Err: the same wrapper is exported before lexical cleanup; no payload exists.
                let failure = linear(mir, err);
                assert_eq!(
                    effects(&failure),
                    expect(&[("forward", operand), ("export", ""), ("drop", tail)]),
                    "{name}"
                );
                assert_forward(&failure, role, operand.as_bytes(), &[tail.as_bytes()]);
            }

            // A discarded payload drops once, before the continuation.
            let report = lower("discarded");
            let mir = report.mir().unwrap();
            let (ok, err) = variant_edges(mir, mir.entry, &role.ok.id, b"input");
            assert_eq!(
                effects(&linear(mir, ok)),
                expect(&[
                    ("take", "input"),
                    ("into", "input?"),
                    ("drop", "input?"),
                    ("construct", ""),
                    ("export", ""),
                    ("drop", tail),
                ])
            );
            assert_eq!(
                effects(&linear(mir, err)),
                expect(&[("forward", "input"), ("export", ""), ("drop", tail)])
            );

            // Known variants: only the feasible edge is lowered.
            let report = lower("knownErr");
            let mir = report.mir().unwrap();
            assert_eq!(
                effects(&linear(mir, mir.entry)),
                expect(&[
                    ("move", "input: Result<string, i32>.Err"),
                    ("into", "input"),
                    ("forward", "input"),
                    ("export", ""),
                    ("drop", tail),
                ])
            );
            let report = lower("knownOk");
            let mir = report.mir().unwrap();
            assert_eq!(
                effects(&linear(mir, mir.entry)),
                expect(&[
                    ("move", "input: Result<string, i32>.Ok"),
                    ("into", "input"),
                    ("take", "input"),
                    ("into", "value"),
                    ("construct", ""),
                    ("export", ""),
                    ("drop", "value"),
                    ("drop", tail),
                ])
            );
        },
    );
}

#[derive(Clone, Copy, Debug)]
enum Outcome {
    Checked,
    /// One finding: its code, site and related use, each as `(context, needle)`.
    Rejected(
        &'static str,
        (&'static str, &'static str),
        (&'static str, &'static str),
    ),
    /// `Incomplete` at the blocker span.
    Blocked(&'static str),
}

const fn stats(work: usize, entries: usize, max_depth: usize) -> Statistics {
    Statistics {
        work,
        entries,
        max_depth,
    }
}

/// Each source declares `probe`, the queried root. `targeted` is
/// `lower_body(probe)`; `graph` is `analyze_ownership` for the module, then
/// its cap before the i32 equality source root (ri-785.9, metering rule 11).
#[allow(clippy::type_complexity)]
const CASES: [(&str, Outcome, Statistics, Statistics, (usize, usize, usize)); 19] = [
    // Positives: the bound, grouped-operand and grouped-postfix forms, an alias,
    // a discard, a statement, both known variants, a scope, a loop and an
    // ended loan.
    (
        "function probe(input: Result<string, i32>, tail: string): Result<bool, i32> { let value = input?; return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_618, 715, 4),
        stats(191_384, 4_810, 5),
        (188_540, 4_676, 4),
    ),
    (
        "function probe(input: Result<string, i32>, tail: string): Result<bool, i32> { let value = ((input))?; return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_626, 717, 5),
        stats(191_512, 4_812, 5),
        (188_668, 4_678, 5),
    ),
    (
        "function probe(input: Result<string, i32>, tail: string): Result<bool, i32> { let value = (input?); return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_621, 716, 4),
        stats(191_447, 4_811, 5),
        (188_603, 4_677, 4),
    ),
    (
        "function probe(input: Result<string, i32>, tail: string): Result<bool, i32> { let held = input; let value = held?; return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_748, 779, 4),
        stats(191_635, 4_874, 5),
        (188_790, 4_740, 4),
    ),
    (
        "function probe(input: Result<string, i32>, tail: string): Result<bool, i32> { let _ = input?; return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_566, 687, 4),
        stats(191_271, 4_782, 5),
        (188_428, 4_648, 4),
    ),
    (
        "function probe(input: Result<string, i32>, tail: string): Result<bool, i32> { input?; return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_568, 688, 4),
        stats(191_333, 4_783, 5),
        (188_490, 4_649, 4),
    ),
    (
        "function probe(input: Result<string, i32>.Err, tail: string): Result<bool, i32> { let value = input?; return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_312, 548, 3),
        stats(191_078, 4_643, 5),
        (188_234, 4_509, 3),
    ),
    (
        "function probe(input: Result<string, i32>.Ok, tail: string): Result<bool, i32> { let value = input?; return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_412, 602, 4),
        stats(191_178, 4_697, 5),
        (188_334, 4_563, 4),
    ),
    (
        "function probe(input: Result<string, i32>, flag: bool): Result<bool, i32> { if (flag) { let value = input?; } else { let other = input?; } return Result.Ok(true); }",
        Outcome::Checked,
        stats(5_485, 1_186, 5),
        stats(192_672, 5_281, 5),
        (189_827, 5_147, 5),
    ),
    (
        "function probe(input: Result<string, i32>, flag: bool): Result<bool, i32> { while (true) { let value = input?; break; } return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_745, 791, 5),
        stats(191_751, 4_886, 5),
        (188_907, 4_752, 5),
    ),
    (
        "function probe(input: Result<string, i32>): Result<bool, i32> { let _ = &input; let value = input?; return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_702, 758, 4),
        stats(191_527, 4_853, 5),
        (188_684, 4_719, 4),
    ),
    // Reads after a move, and a move under a live loan.
    (
        "function probe(input: Result<string, i32>): Result<bool, i32> { let value = input?; let moved = value; let again = value; return Result.Ok(true); }",
        Outcome::Rejected(
            "R0200",
            ("again = value", "value"),
            ("moved = value", "value"),
        ),
        stats(4_545, 682, 4),
        stats(191_494, 4_778, 5),
        (188_649, 4_644, 4),
    ),
    (
        "function probe(input: Result<string, i32>): Result<bool, i32> { let value = input?; let other = input?; return Result.Ok(true); }",
        Outcome::Rejected(
            "R0200",
            ("other = input", "input"),
            ("value = input", "input"),
        ),
        stats(4_534, 680, 4),
        stats(191_422, 4_776, 5),
        (188_578, 4_642, 4),
    ),
    (
        "function probe(input: Result<string, i32>): Result<bool, i32> { let view = &input; let value = input?; let _ = view; return Result.Ok(true); }",
        Outcome::Rejected(
            "R0203",
            ("value = input", "input"),
            ("view = &input", "&input"),
        ),
        stats(4_371, 572, 4),
        stats(191_319, 4_668, 5),
        (188_475, 4_534, 4),
    ),
    // Unsupported siblings. At 5882113b each blocked at its parameter
    // (1,849/16/0, 1,869/26/0 and 1,847/15/0); the parameter is now admitted.
    (
        "function probe(input: Result<string, i32>): Result<bool, i32> { let value = input ? 7i32; return Result.Ok(true); }",
        Outcome::Blocked("input ? 7i32"),
        stats(4_087, 426, 4),
        stats(190_852, 4_521, 5),
        (188_009, 4_387, 4),
    ),
    (
        "function take(value: string): bool { dispose(value); return true; }\nfunction probe(input: Result<string, i32>): Result<bool, i32> { let bit = take(input?); return Result.Ok(bit); }",
        Outcome::Blocked("input?"),
        stats(4_199, 442, 4),
        stats(192_835, 4_649, 5),
        (189_842, 4_515, 4),
    ),
    (
        "function probe(input: Result<string, i32>): Result<string, i32> { let value = input?; return Result.Ok(value); }",
        Outcome::Blocked("Result.Ok(value)"),
        stats(2_981, 227, 3),
        stats(189_686, 4_322, 5),
        (186_843, 4_188, 3),
    ),
    // Parent-cap regressions: the targeted caps are those measured at 5882113b,
    // so a screen that spends before rejecting fails here. The constructor's
    // graph is unchanged too; `make` now allocates its `Result<string, i32>`
    // return slot before its own blocker (graph 187,498/4,211/4 at 5882113b).
    (
        "function probe(text: string): Result<bool, i32> { let value = Result<string, i32>.Ok(text)?; return Result.Ok(true); }",
        Outcome::Blocked("Result<string, i32>.Ok(text)?"),
        stats(2_991, 231, 4),
        stats(189_756, 4_326, 5),
        (186_913, 4_192, 4),
    ),
    (
        "function make(text: string): Result<string, i32> { return Result.Ok(text); }\nfunction probe(text: string): Result<bool, i32> { let value = make(text)?; return Result.Ok(true); }",
        Outcome::Blocked("make(text)?"),
        stats(3_005, 238, 4),
        stats(191_444, 4_542, 5),
        (188_600, 4_408, 4),
    ),
];

fn locate(module: &loader::Module, (context, needle): (&str, &str)) -> (usize, usize) {
    let bytes = module.snapshot.bytes();
    let at = bytes
        .windows(context.len())
        .position(|w| w == context.as_bytes())
        .unwrap_or_else(|| panic!("missing {context}"));
    let offset = context.find(needle).unwrap();
    (at + offset, at + offset + needle.len())
}

fn limits(s: Statistics) -> (AnalysisLimits, Vec<AnalysisLimits>) {
    let exact = AnalysisLimits {
        max_work: s.work,
        max_entries: s.entries,
        max_depth: s.max_depth,
    };
    let mut short = vec![
        AnalysisLimits {
            max_work: s.work - 1,
            ..exact
        },
        AnalysisLimits {
            max_entries: s.entries - 1,
            ..exact
        },
    ];
    if let Some(max_depth) = s.max_depth.checked_sub(1) {
        short.push(AnalysisLimits { max_depth, ..exact });
    }
    (exact, short)
}

#[test]
fn owning_text_result_cases_keep_their_outcomes_at_exact_caps() {
    for (source, outcome, targeted, whole, pre_equality) in CASES {
        probe(
            "propagation::result_text::owning_text_result_cases_keep_their_outcomes_at_exact_caps",
            source,
            |_, module, receipt| {
                let f = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some("probe"))
                    .unwrap();
                let expected = match outcome {
                    Outcome::Checked => Completion::Checked,
                    Outcome::Rejected(..) => Completion::SourceRejected,
                    Outcome::Blocked(_) => Completion::Incomplete,
                };
                let run = |cap| lower_body(receipt, module, &f.id, cap).unwrap();
                let (exact, short) = limits(targeted);
                let target = run(exact);
                assert_eq!(target.completion(), expected, "{source}");
                assert_eq!(target.statistics(), targeted, "{source}");
                assert_eq!(target.mir().is_some(), expected == Completion::Checked);
                for cap in short {
                    let limited = run(cap);
                    crate::limit::assert_body_limit(&limited);
                }
                assert_pre_equality_graph_cap(receipt, pre_equality);
                let (exact, short) = limits(whole);
                let graph = analyze_ownership(receipt, exact);
                assert_eq!(graph.statistics(), whole, "{source}");
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == f.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), expected, "{source}");
                assert_eq!(
                    root.ordinary_mir().is_some(),
                    expected == Completion::Checked
                );
                for cap in short {
                    let limited = analyze_ownership(receipt, cap);
                    crate::limit::assert_graph_limit(&limited);
                }
                match outcome {
                    Outcome::Checked => {
                        assert!(target.findings().is_empty() && graph.findings().is_empty());
                    }
                    Outcome::Blocked(site) => {
                        assert!(target.findings().is_empty() && graph.findings().is_empty());
                        let blocker = target.blocked_at().unwrap();
                        assert_eq!(blocker.span.bytes(), site.as_bytes(), "{source}");
                        assert_eq!(root.blocked_at().unwrap().node, blocker.node);
                    }
                    Outcome::Rejected(code, site, related) => {
                        assert_eq!(graph.completion(), GraphCompletion::SourceRejected);
                        for findings in [target.findings(), graph.findings()] {
                            let [finding] = findings else {
                                panic!("expected one finding: {source}")
                            };
                            assert_eq!(finding.code(), code, "{source}");
                            for (place, expected) in
                                [(finding.site, site), (finding.related, related)]
                            {
                                assert_eq!(
                                    (place.span.start(), place.span.end()),
                                    locate(module, expected),
                                    "{source}"
                                );
                            }
                        }
                    }
                }
            },
        );
    }
}
