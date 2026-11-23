//! Ticket C3 (`ri-vgz.5`): one exact owning error type, `string`, through
//! propagation. A postfix forwards the existing `Result<_, string>` wrapper
//! unchanged before lexical cleanup. When the right-hand side of a wrap
//! always fails, its wrapper is exported unchanged and the held left
//! `OwnedErr<string>` is retired once, after that export: no new wrapper and
//! no second origin.
use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{GraphCompletion, analyze_ownership};

const PATHS: &str = r#"
    function forwards(input: Result<bool, string>, tail: string): Result<bool, string> {
        let flag = input?;
        return Result.Ok(flag);
    }
    function knownErr(input: Result<bool, string>.Err, tail: string): Result<bool, string> {
        let flag = input?;
        return Result.Ok(flag);
    }
    function whole(input: Result<bool, string>, tail: string): Result<bool, string> {
        return input;
    }
    function retires(input: Result<bool, string>.Err, source: Result<string, string>.Err, tail: string): Result<bool, string> {
        let _ = input ? (source?);
        return Result.Ok(true);
    }
    function retiresBound(input: Result<bool, string>.Err, source: Result<string, string>.Err, tail: string): Result<bool, string> {
        let flag = input ? (source?);
        return Result.Ok(flag);
    }
"#;

/// One relevant MIR effect: its kind and the source text of its place.
type Effect = (&'static str, String);

fn text(place: &compiler::analysis::ownership::Place<'_>) -> String {
    String::from_utf8_lossy(place.root.span.bytes()).into_owned()
}

/// Every operation on the path, in order. An unexpected kind fails, so a new
/// wrapper, constructor or second disposal cannot hide in the sequence.
fn effects(ops: &[&Operation<'_>]) -> Vec<Effect> {
    let mut out = Vec::new();
    for op in ops {
        match op {
            Operation::Move { from, into } => {
                out.push(("move", text(from)));
                out.push(("into", into.as_ref().map(text).unwrap_or_default()));
            }
            Operation::Copy { from, into } => {
                out.push(("copy", text(from)));
                out.push(("into", into.as_ref().map(text).unwrap_or_default()));
            }
            Operation::TestVariant { operand, .. } => out.push(("test", text(operand))),
            Operation::TakeResultSuccess { operand, into } => {
                out.push(("take", text(operand)));
                out.push(("into", into.as_ref().map(text).unwrap_or_default()));
            }
            Operation::ForwardResultFailure { operand, into, .. } => {
                out.push(("forward", text(operand)));
                out.push(("into", text(into)));
            }
            Operation::DropResultFailure { operand, .. } => out.push(("retire", text(operand))),
            Operation::ConstructVariant { into, .. } => out.push(("construct", text(into))),
            Operation::Export(place) => out.push(("export", text(place))),
            Operation::Cleanup(actions) => {
                for action in actions {
                    match action {
                        Obligation::Drop(place) => out.push(("drop", text(place))),
                        Obligation::EndStorage(place) => out.push(("end", text(place))),
                        other => panic!("unexpected cleanup {other:?}"),
                    }
                }
            }
            Operation::None => {}
            other => panic!("unexpected operation {other:?}"),
        }
    }
    out
}

fn expect(pairs: &[(&'static str, &str)]) -> Vec<Effect> {
    pairs.iter().map(|(k, p)| (*k, p.to_string())).collect()
}

#[test]
fn string_error_is_forwarded_unchanged_and_the_held_error_is_retired_once() {
    probe_with_typed(
        "propagation::text_err::string_error_is_forwarded_unchanged_and_the_held_error_is_retired_once",
        PATHS,
        |_, module, receipt, typed| {
            let role = typed.result_role().unwrap();
            let pin = role.representation.unwrap().storage_source;
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            assert!(graph.findings().is_empty());
            // The targeted MIR and the ordinary graph root MIR of `name`.
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
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == decl.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Checked, "{name}");
                (report, root.ordinary_mir().unwrap())
            };
            let tail = "tail: string";

            // Ok: the bool moves to `flag`; Err: the same wrapper is exported
            // before lexical cleanup.
            let (report, root) = lower("forwards");
            for mir in [report.mir().unwrap(), root] {
                let (ok, err) = variant_edges(mir, mir.entry, &role.ok.id, b"input");
                let prefix: Vec<_> = straight_blocks(mir, mir.entry)
                    .into_iter()
                    .map(|b| &b.operation)
                    .collect();
                assert_eq!(
                    effects(&prefix),
                    expect(&[
                        ("move", "input: Result<bool, string>"),
                        ("into", "input"),
                        ("test", "input"),
                    ])
                );
                assert_eq!(
                    effects(&linear(mir, ok)),
                    expect(&[
                        ("take", "input"),
                        ("into", "flag"),
                        ("copy", "flag"),
                        ("into", "flag"),
                        ("construct", "Result.Ok(flag)"),
                        ("export", "Result.Ok(flag)"),
                        ("end", "flag"),
                        ("drop", tail),
                    ])
                );
                let failure = linear(mir, err);
                assert_eq!(
                    effects(&failure),
                    expect(&[
                        ("forward", "input"),
                        ("into", "input?"),
                        ("export", "input?"),
                        ("drop", tail),
                    ])
                );
                assert_forward(&failure, role, b"input", &[tail.as_bytes()]);
            }

            // A known Err has only the forwarding edge.
            let (report, root) = lower("knownErr");
            for mir in [report.mir().unwrap(), root] {
                assert_eq!(
                    effects(&linear(mir, mir.entry)),
                    expect(&[
                        ("move", "input: Result<bool, string>.Err"),
                        ("into", "input"),
                        ("forward", "input"),
                        ("into", "input?"),
                        ("export", "input?"),
                        ("drop", tail),
                    ])
                );
            }

            // The whole carrier is exported as it is.
            let (report, root) = lower("whole");
            for mir in [report.mir().unwrap(), root] {
                assert_eq!(
                    effects(&linear(mir, mir.entry)),
                    expect(&[
                        ("move", "input: Result<bool, string>"),
                        ("into", "input"),
                        ("export", "input"),
                        ("drop", tail),
                    ])
                );
            }

            // The RHS always fails: export its wrapper unchanged, then retire
            // the held left error once. No wrapper is built, so no origin is
            // added; the only exported origin is the one `source` carries.
            for name in ["retires", "retiresBound"] {
                let (report, root) = lower(name);
                for mir in [report.mir().unwrap(), root] {
                    let path = linear(mir, mir.entry);
                    assert_eq!(
                        effects(&path),
                        expect(&[
                            ("move", "input: Result<bool, string>.Err"),
                            ("into", "input"),
                            ("move", "source: Result<string, string>.Err"),
                            ("into", "source"),
                            ("forward", "source"),
                            ("into", "source?"),
                            ("export", "source?"),
                            ("retire", "input"),
                            ("drop", tail),
                        ]),
                        "{name}"
                    );
                    let [retire] = path
                        .iter()
                        .filter_map(|op| match op {
                            Operation::DropResultFailure {
                                operand,
                                variant,
                                wrapper,
                                storage_source,
                            } => Some((operand, *variant, *wrapper, *storage_source)),
                            _ => None,
                        })
                        .collect::<Vec<_>>()[..]
                    else {
                        panic!("{name}: one retirement");
                    };
                    assert_eq!(text(retire.0), "input");
                    assert_eq!(
                        (retire.1, retire.2, retire.3),
                        (&role.err.id, &role.wrapper.id, pin)
                    );
                    assert!(
                        matches!(path[2], Operation::ForwardResultFailure { variant, wrapper, .. }
                        if *variant == &role.err.id && *wrapper == &role.wrapper.id)
                    );
                }
            }
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
/// `lower_body(probe)`; `graph` is `analyze_ownership` for the module. At the
/// parent a95e04e8 every source below stopped at its first
/// `Result<_, string>` parameter (e.g. `forwards` 1,852/17/0, graph
/// 185,787/3,978/3), except the two parent-cap rows at the end.
/// The last column is the whole-graph cap before the i32 equality source root
/// (ri-785.9, metering rule 11).
#[allow(clippy::type_complexity)]
const CASES: [(&str, Outcome, Statistics, Statistics, (usize, usize, usize)); 24] = [
    // Forwarding: bound, grouped operand, grouped postfix, alias, statement,
    // both known variants, the whole carrier, a grouped return, a scope and
    // a loop.
    (
        "function probe(input: Result<bool, string>, tail: string): Result<bool, string> { let flag = input?; return Result.Ok(flag); }",
        Outcome::Checked,
        stats(3_521, 522, 4),
        stats(187_772, 4_617, 5),
        (187_456, 4_483, 4),
    ),
    (
        "function probe(input: Result<bool, string>, tail: string): Result<bool, string> { let flag = ((input))?; return Result.Ok(flag); }",
        Outcome::Checked,
        stats(3_529, 524, 5),
        stats(187_900, 4_619, 5),
        (187_584, 4_485, 5),
    ),
    (
        "function probe(input: Result<bool, string>, tail: string): Result<bool, string> { let flag = (input?); return Result.Ok(flag); }",
        Outcome::Checked,
        stats(3_524, 523, 4),
        stats(187_835, 4_618, 5),
        (187_519, 4_484, 4),
    ),
    (
        "function probe(input: Result<bool, string>, tail: string): Result<bool, string> { let held = input; let flag = held?; return Result.Ok(flag); }",
        Outcome::Checked,
        stats(3_650, 586, 4),
        stats(188_022, 4_681, 5),
        (187_706, 4_547, 4),
    ),
    (
        "function probe(input: Result<bool, string>, tail: string): Result<bool, string> { input?; return Result.Ok(true); }",
        Outcome::Checked,
        stats(3_372, 444, 4),
        stats(187_622, 4_539, 5),
        (187_306, 4_405, 4),
    ),
    (
        "function probe(input: Result<bool, string>.Err, tail: string): Result<bool, string> { let flag = input?; return Result.Ok(flag); }",
        Outcome::Checked,
        stats(3_204, 353, 3),
        stats(187_455, 4_448, 5),
        (187_139, 4_314, 3),
    ),
    (
        "function probe(input: Result<bool, string>.Ok, tail: string): Result<bool, string> { let flag = input?; return Result.Ok(flag); }",
        Outcome::Checked,
        stats(3_316, 409, 4),
        stats(187_567, 4_504, 5),
        (187_251, 4_370, 4),
    ),
    (
        "function probe(input: Result<bool, string>, tail: string): Result<bool, string> { return input; }",
        Outcome::Checked,
        stats(3_077, 279, 3),
        stats(187_087, 4_374, 5),
        (186_771, 4_240, 3),
    ),
    (
        "function probe(input: Result<bool, string>, tail: string): Result<bool, string> { let flag = input?; return (Result.Ok(flag)); }",
        Outcome::Checked,
        stats(3_526, 523, 5),
        stats(187_837, 4_618, 5),
        (187_521, 4_484, 5),
    ),
    (
        "function probe(input: Result<bool, string>, flag: bool): Result<bool, string> { if (flag) { let value = input?; } else { let other = input?; } return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_374, 991, 5),
        stats(189_046, 5_086, 5),
        (188_730, 4_952, 5),
    ),
    (
        "function probe(input: Result<bool, string>, flag: bool): Result<bool, string> { while (true) { let value = input?; break; } return Result.Ok(true); }",
        Outcome::Checked,
        stats(3_636, 596, 5),
        stats(188_127, 4_691, 5),
        (187_811, 4_557, 5),
    ),
    // Retiring the held error: discarded, bound, grouped operands, grouped
    // RHS, and in each branch of a scope.
    (
        "function probe(input: Result<bool, string>.Err, source: Result<string, string>.Err, tail: string): Result<bool, string> { let _ = input ? (source?); return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_794, 668, 4),
        stats(189_345, 4_763, 5),
        (189_029, 4_629, 4),
    ),
    (
        "function probe(input: Result<bool, string>.Err, source: Result<string, string>.Err, tail: string): Result<bool, string> { let flag = input ? (source?); return Result.Ok(flag); }",
        Outcome::Checked,
        stats(4_885, 722, 4),
        stats(189_497, 4_817, 5),
        (189_181, 4_683, 4),
    ),
    (
        "function probe(input: Result<bool, string>.Err, source: Result<string, string>.Err, tail: string): Result<bool, string> { let _ = ((input)) ? ((source)?); return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_806, 671, 5),
        stats(189_537, 4_766, 5),
        (189_221, 4_632, 5),
    ),
    (
        "function probe(input: Result<bool, string>.Err, source: Result<string, string>.Err, tail: string): Result<bool, string> { let _ = input ? ((source?)); return Result.Ok(true); }",
        Outcome::Checked,
        stats(4_797, 669, 5),
        stats(189_408, 4_764, 5),
        (189_092, 4_630, 5),
    ),
    (
        "function probe(input: Result<bool, string>.Err, source: Result<string, string>.Err, flag: bool): Result<bool, string> { if (flag) { let _ = input ? (source?); } else { let _ = input ? (source?); } return Result.Ok(true); }",
        Outcome::Checked,
        stats(5_828, 1_075, 6),
        stats(190_919, 5_170, 6),
        (190_603, 5_036, 6),
    ),
    // A second postfix, a wrap whose RHS reads its own moved operand, and a
    // move under a live loan.
    (
        "function probe(input: Result<bool, string>, tail: string): Result<bool, string> { let flag = input?; let again = input?; return Result.Ok(flag); }",
        Outcome::Rejected(
            "R0200",
            ("again = input", "input"),
            ("flag = input", "input"),
        ),
        stats(3_473, 509, 4),
        stats(187_907, 4_605, 5),
        (187_591, 4_471, 4),
    ),
    (
        "function probe(source: Result<string, string>.Err, tail: string): Result<bool, string> { let _ = source ? (source?); return Result.Ok(true); }",
        Outcome::Rejected("R0200", ("(source?)", "source"), ("= source ?", "source")),
        stats(4_602, 567, 4),
        stats(188_974, 4_663, 5),
        (188_658, 4_529, 4),
    ),
    (
        "function probe(input: Result<bool, string>): Result<bool, string> { let view = &input; let flag = input?; let _ = view; return Result.Ok(flag); }",
        Outcome::Rejected(
            "R0203",
            ("flag = input", "input"),
            ("view = &input", "&input"),
        ),
        stats(3_279, 382, 4),
        stats(187_712, 4_478, 5),
        (187_396, 4_344, 4),
    ),
    // Unsupported siblings. A `string` payload for a new wrapper, a call
    // RHS and an owning success payload that can continue stay Incomplete.
    // At a95e04e8 each stopped at its parameter (1,854/18/0, 1,886/34/0,
    // 1,852/17/0); the parameter is now admitted.
    (
        "function probe(input: Result<bool, string>, tail: string): Result<bool, string> { let flag = input ? tail; return Result.Ok(flag); }",
        Outcome::Blocked("input ? tail"),
        stats(2_996, 240, 4),
        stats(187_307, 4_335, 5),
        (186_991, 4_201, 4),
    ),
    (
        "function carryFailure(source: Result<string, string>.Err): Result<string, string>.Err { return source; }\nfunction probe(input: Result<bool, string>.Err, source: Result<string, string>.Err, tail: string): Result<bool, string> { let _ = input ? (carryFailure(source)?); return Result.Ok(true); }",
        Outcome::Blocked("carryFailure(source)"),
        stats(4_481, 504, 4),
        stats(191_037, 4_856, 5),
        (190_721, 4_722, 4),
    ),
    // Review ci-uaa1 (ri-vgz.5 #2): a non-postfix `string` RHS that always
    // returns. Rule 1: it is rejected after its RHS fact and grouping, before
    // the retained goals, display and `?` token are charged (before that fix,
    // 4,107/435/4 and graph 189,351/4,600/5). At a95e04e8 it stopped at its
    // parameter (1,878/30/0).
    (
        "function label(value: string): string { return value; }\nfunction probe(input: Result<bool, string>.Err, source: Result<string, string>.Err, tail: string): Result<bool, string> { let _ = input ? (label(source?)); return Result.Ok(true); }",
        Outcome::Blocked("(label(source?))"),
        stats(4_095, 435, 4),
        stats(189_339, 4_600, 5),
        (189_023, 4_466, 4),
    ),
    (
        "function probe(input: Result<string, string>, tail: string): Result<bool, string> { let flag = input?; return Result.Ok(true); }",
        Outcome::Blocked("input?"),
        stats(4_081, 430, 4),
        stats(188_332, 4_525, 5),
        (188_016, 4_391, 4),
    ),
    // Parent-cap regression, measured at a95e04e8 and not re-measured: a
    // `Result<_, string>` returned by a producer that can itself return is
    // screened before its return storage is proved (the installed `wrap`).
    (
        "function probe(input: Result<bool, i32>): Result<bool, string> { return Result.Ok(input ? \"outer\"); }",
        Outcome::Blocked("Result.Ok(input ? \"outer\")"),
        stats(2_949, 215, 3),
        stats(187_078, 4_310, 5),
        (186_762, 4_176, 3),
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
fn string_error_cases_keep_their_outcomes_at_exact_caps() {
    for (source, outcome, targeted, whole, pre_equality) in CASES {
        probe(
            "propagation::text_err::string_error_cases_keep_their_outcomes_at_exact_caps",
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
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{source}");
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
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
                    assert_eq!(
                        limited.completion(),
                        GraphCompletion::ResourceLimit,
                        "{source}"
                    );
                    assert!(limited.roots().iter().all(|r| r.ordinary_mir().is_none()));
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
