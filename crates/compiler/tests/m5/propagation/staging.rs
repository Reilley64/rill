//! ri-vgz.6 (C4): a postfix failure inside a flat tuple initializer, and one
//! postfix in an ordinary `if` condition. Sources `tuple`, `checkTuple`,
//! `condition`, `conditionControl` and `conditionRead` are the audit's
//! `old-partial` and `old-entangled` captures (r76-audit, audit 4(c)).
use super::*;
use compiler::analysis::{
    Statistics,
    ownership::{Block, GraphCompletion, Place, Projection, analyze_ownership},
    scalars::BodyOwner,
    typed::TypedReport,
};

const TUPLE: &str = r#"
function stage(value: string): string { return value; }
function relay(flag: bool): bool { return flag; }
function tuple(first: string, second: string, gate: Option<bool>, suffix: string): Option<bool> {
    let value: [string, string, bool, string] = [stage(first), stage(second), gate?, stage(suffix)];
    return Option.Some(true);
}
function checkTuple(first: string, second: string, gate: bool, suffix: string): Option<bool> {
    let value: [string, string, bool, string] = [stage(first), stage(second), gate, stage(suffix)];
    return Option.Some(true);
}
function grouped(first: string, second: string, gate: Option<bool>, suffix: string): Option<bool> {
    let value: [string, string, bool, string] = [stage(first), stage(second), ((gate?)), stage(suffix)];
    return Option.Some(true);
}
function scoped(first: string, gate: Option<bool>, suffix: string): Option<bool> {
    { let value: [string, bool, string] = [stage(first), gate?, stage(suffix)]; }
    return Option.Some(true);
}
function looped(first: string, gate: Option<bool>, suffix: string): Option<bool> {
    while (true) { let value: [string, bool, string] = [stage(first), gate?, stage(suffix)]; break; }
    return Option.Some(true);
}
function certainNone(first: string, suffix: string): Option<bool> {
    let value: [string, bool, string] = [stage(first), Option<bool>.None?, stage(suffix)];
    return Option.Some(true);
}
function skippedSuffix(first: string, suffix: string): Option<bool> {
    let prior = suffix;
    let value: [string, bool, string] = [stage(first), Option<bool>.None?, stage(suffix)];
    return Option.Some(true);
}
function reachedSuffix(first: string, gate: Option<bool>, suffix: string): Option<bool> {
    let prior = suffix;
    let value: [string, bool, string] = [stage(first), gate?, stage(suffix)];
    return Option.Some(true);
}
function movedPrefix(first: string, gate: Option<bool>, suffix: string): Option<bool> {
    let prior = first;
    let value: [string, bool, string] = [stage(first), gate?, stage(suffix)];
    return Option.Some(true);
}
function embedded(first: string, gate: Option<bool>): Option<bool> {
    let value: [string, bool] = [stage(first), relay(gate?)];
    return Option.Some(true);
}
function twice(gate: Option<bool>, other: Option<bool>): Option<bool> {
    let value: [bool, bool] = [gate?, other?];
    return Option.Some(true);
}
function text(first: string, label: Option<string>): Option<bool> {
    let value: [string, string] = [stage(first), label?];
    return Option.Some(true);
}
function nested(first: string, gate: Option<bool>): Option<bool> {
    let value: [[string, bool], bool] = [[stage(first), gate?], true];
    return Option.Some(true);
}
"#;

const CONDITION: &str = r#"
function relay(flag: bool): bool { return flag; }
function condition(gate: Option<bool>, tail: string): Option<bool> {
    if (gate?) { return Option.Some(true); }
    return Option.Some(false);
}
function conditionControl(gate: bool): Option<bool> {
    if (gate) { return Option.Some(true); }
    return Option.Some(false);
}
function conditionRead(gate: Option<bool>, retired: string): Option<bool> {
    dispose(retired);
    if (gate?) { dispose(retired); }
    return Option.Some(true);
}
function grouped(gate: Option<bool>, tail: string): Option<bool> {
    if ((((((gate?)))))) { return Option.Some(true); }
    return Option.Some(false);
}
function scoped(gate: Option<bool>, tail: string): Option<bool> {
    { if (gate?) { return Option.Some(true); } }
    return Option.Some(false);
}
function looped(gate: Option<bool>, tail: string): Option<bool> {
    while (true) { if (gate?) { break; } }
    return Option.Some(false);
}
function branches(gate: Option<bool>, left: string, right: string): Option<bool> {
    if (gate?) { let taken = left; } else { let taken = right; }
    return Option.Some(true);
}
function certainNone(tail: string): Option<bool> {
    if (Option<bool>.None?) { let taken = tail; }
    return Option.Some(false);
}
function skippedRead(retired: string): Option<bool> {
    dispose(retired);
    if (Option<bool>.None?) { dispose(retired); }
    return Option.Some(true);
}
function whileCondition(gate: Option<bool>): Option<bool> {
    while (gate?) { break; }
    return Option.Some(true);
}
function negated(gate: Option<bool>): Option<bool> {
    if (!(gate?)) { return Option.Some(true); }
    return Option.Some(false);
}
function called(gate: Option<bool>): Option<bool> {
    if (relay(gate?)) { return Option.Some(true); }
    return Option.Some(false);
}
function resultCondition(gate: Result<bool, i32>): Result<bool, i32> {
    if (gate?) { return Result.Ok(true); }
    return Result.Ok(false);
}
"#;

fn stats(work: usize, entries: usize, max_depth: usize) -> Statistics {
    Statistics {
        work,
        entries,
        max_depth,
    }
}

fn exact(s: Statistics) -> AnalysisLimits {
    AnalysisLimits {
        max_work: s.work,
        max_entries: s.entries,
        max_depth: s.max_depth,
    }
}

fn one_short(s: Statistics) -> [AnalysisLimits; 3] {
    let exact = exact(s);
    [
        AnalysisLimits {
            max_work: s.work - 1,
            ..exact
        },
        AnalysisLimits {
            max_entries: s.entries - 1,
            ..exact
        },
        AnalysisLimits {
            max_depth: s.max_depth - 1,
            ..exact
        },
    ]
}

/// The written root and tuple position of a place, as source text.
fn place(p: &Place<'_>) -> String {
    let root = p.root.span.bytes().escape_ascii().to_string();
    match p.projection {
        None => root,
        Some(Projection::TupleElement(i)) => format!("{root}.{i}"),
        Some(other) => panic!("unexpected projection {other:?}"),
    }
}

/// One effect per block, in order, with the place each acts on. Blocks with
/// no effect (scalar ends and branch points) are skipped.
fn effect(block: &Block<'_>, typed: &TypedReport<'_>) -> Option<String> {
    let option = typed.option_role().unwrap();
    let variant = |id| {
        if id == &option.some.id {
            "Some"
        } else if id == &option.none.id {
            "None"
        } else {
            panic!("unexpected variant {id:?}")
        }
    };
    let cleanup = |obligations: &[Obligation<'_>]| {
        obligations
            .iter()
            .map(|o| match o {
                Obligation::Drop(p) => format!("drop {}", place(p)),
                Obligation::EndStorage(p) => format!("end {}", place(p)),
                other => panic!("unexpected cleanup {other:?}"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let site = block.site.span.bytes().escape_ascii().to_string();
    Some(match &block.operation {
        Operation::None => return None,
        Operation::Move { from, into } => match into {
            Some(into) => format!("move {} -> {}", place(from), place(into)),
            None => format!("move {}", place(from)),
        },
        Operation::Copy { from, into } => match into {
            Some(into) => format!("copy {} -> {}", place(from), place(into)),
            None => format!("copy {}", place(from)),
        },
        Operation::Call(_) => format!("call {site}"),
        Operation::Dispose { operand, .. } => format!("dispose {}", place(operand)),
        Operation::TestVariant { operand, success } => {
            format!("test {} for {}", place(operand), variant(*success))
        }
        Operation::CopyOptionPayload { operand, into } => {
            format!("payload {} -> {}", place(operand), place(into))
        }
        Operation::ConstructVariant {
            variant: made,
            payload,
            into,
        } => match payload {
            Some(payload) => format!("{}({}) -> {}", variant(*made), place(payload), place(into)),
            None => format!("{} -> {}", variant(*made), place(into)),
        },
        Operation::Initialize(p) => format!("init {}", place(p)),
        Operation::Export(p) => format!("export {}", place(p)),
        Operation::Drop(p) => format!("drop {}", place(p)),
        Operation::Cleanup(obligations) => format!("cleanup [{}]", cleanup(obligations)),
        other => panic!("unexpected operation {other:?}"),
    })
}

/// Effects from `pc` up to a branch (returning its targets) or the return.
fn path(
    mir: &Mir<'_>,
    typed: &TypedReport<'_>,
    mut pc: usize,
) -> (Vec<String>, Option<[usize; 2]>) {
    let mut effects = Vec::new();
    for _ in 0..mir.blocks.len() {
        let block = &mir.blocks[pc];
        effects.extend(effect(block, typed));
        match block.terminator {
            Terminator::Goto(next) => pc = next,
            Terminator::Branch { yes, no } => return (effects, Some([yes, no])),
            Terminator::Return => return (effects, None),
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("path did not reach a branch or return");
}

fn linear_path(mir: &Mir<'_>, typed: &TypedReport<'_>, pc: usize) -> Vec<String> {
    let (effects, branch) = path(mir, typed, pc);
    assert_eq!(branch, None, "expected a straight path: {effects:?}");
    effects
}

#[test]
fn interrupted_tuple_initializer_cleans_only_the_reached_prefix() {
    probe_with_typed(
        "propagation::staging::interrupted_tuple_initializer_cleans_only_the_reached_prefix",
        TUPLE,
        |_, module, receipt, typed| {
            let declaration = |name: &str| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let default = AnalysisLimits::default();
            let graph = analyze_ownership(receipt, default);
            let root = |name| {
                graph
                    .roots()
                    .iter()
                    .find(|r| r.owner() == BodyOwner::Declaration(&declaration(name).id))
                    .unwrap()
            };

            // Each new route at its exact caps and independently one short.
            for (name, completion, pinned) in [
                ("tuple", Completion::Checked, stats(8623, 2053, 7)),
                ("grouped", Completion::Checked, stats(8631, 2053, 9)),
                ("scoped", Completion::Checked, stats(8085, 1778, 8)),
                ("looped", Completion::Checked, stats(8261, 1883, 9)),
                ("certainNone", Completion::Checked, stats(7157, 1278, 10)),
                ("skippedSuffix", Completion::Checked, stats(7279, 1337, 10)),
                (
                    "reachedSuffix",
                    Completion::SourceRejected,
                    stats(4947, 1120, 7),
                ),
                (
                    "movedPrefix",
                    Completion::SourceRejected,
                    stats(4473, 836, 7),
                ),
            ] {
                let report = lower(name, default);
                assert_eq!(
                    (report.completion(), report.statistics()),
                    (completion, pinned),
                    "{name} at {:?}",
                    report
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                );
                assert_eq!(root(name).ordinary_completion(), completion, "{name}");
                assert_eq!(
                    root(name).ordinary_mir().is_some(),
                    completion == Completion::Checked,
                    "{name}"
                );
                let at_cap = lower(name, exact(pinned));
                assert_eq!(at_cap.completion(), completion, "{name}");
                for short in one_short(pinned) {
                    let limited = lower(name, short);
                    crate::limit::assert_body_limit(&limited);
                }
            }
            // Groups add written depth, not work beyond one step per layer.
            assert!(lower("grouped", default).statistics().max_depth > 7);

            // The written prefix runs before the postfix; the header stays
            // vacant on failure; both edges use one staged tuple.
            let mut prefix = vec![
                "move first: string -> first",
                "call stage(first)",
                "move second: string -> second",
                "call stage(second)",
                "copy gate: Option<bool> -> gate",
                "test gate for Some",
            ];
            // None: export, then end the reached members in reverse written
            // order. `suffix` is never evaluated, the empty local scope
            // commits nothing to `value`, and the formal is dropped unmoved.
            let failure = [
                "None -> gate?",
                "export gate?",
                "drop value.1",
                "drop value.0",
                "cleanup []",
                "cleanup [drop suffix: string, end gate: Option<bool>]",
            ];
            let success = [
                "payload gate -> value.2",
                "move suffix: string -> suffix",
                "call stage(suffix)",
                "init value",
                "init true",
                "Some(true) -> Option.Some(true)",
                "export Option.Some(true)",
                "cleanup [drop value.3, end value.2, drop value.1, drop value.0, end value]",
                "cleanup [end gate: Option<bool>]",
            ];
            for name in ["tuple", "grouped"] {
                for mir in [
                    lower(name, default).mir().unwrap(),
                    root(name).ordinary_mir().unwrap(),
                ] {
                    let (effects, branch) = path(mir, typed, mir.entry);
                    assert_eq!(effects, prefix, "{name}");
                    let [yes, no] = branch.unwrap();
                    assert_eq!(linear_path(mir, typed, no), failure, "{name}");
                    assert_eq!(linear_path(mir, typed, yes), success, "{name}");
                }
            }
            // The ordinary tuple control initializes in written order on one edge.
            prefix.truncate(4);
            let control = lower("checkTuple", default);
            assert_eq!(control.completion(), Completion::Checked);
            for mir in [
                control.mir().unwrap(),
                root("checkTuple").ordinary_mir().unwrap(),
            ] {
                let mut expected = prefix.clone();
                expected.extend(["copy gate: bool -> value.2"]);
                expected.extend(&success[1..7]);
                expected.extend([success[7], "cleanup [end gate: bool]"]);
                assert_eq!(linear_path(mir, typed, mir.entry), expected);
            }

            // Scoped and loop-wrapped forms end the same reached prefix.
            for name in ["scoped", "looped"] {
                for mir in [
                    lower(name, default).mir().unwrap(),
                    root(name).ordinary_mir().unwrap(),
                ] {
                    let (effects, branch) = path(mir, typed, mir.entry);
                    assert!(effects.ends_with(&[
                        "move first: string -> first".into(),
                        "call stage(first)".into(),
                        "copy gate: Option<bool> -> gate".into(),
                        "test gate for Some".into(),
                    ]));
                    let [_, no] = branch.unwrap();
                    assert_eq!(
                        linear_path(mir, typed, no),
                        [
                            "None -> gate?",
                            "export gate?",
                            "drop value.0",
                            "cleanup []",
                            "cleanup [drop suffix: string, end gate: Option<bool>]",
                        ],
                        "{name}"
                    );
                }
            }

            // A known None stops after its reached prefix; the suffix is not
            // evaluated, so an earlier move of its source is not reached.
            for (name, locals) in [
                ("certainNone", "cleanup []"),
                ("skippedSuffix", "cleanup [drop prior]"),
            ] {
                for mir in [
                    lower(name, default).mir().unwrap(),
                    root(name).ordinary_mir().unwrap(),
                ] {
                    let mut expected = vec![];
                    if name == "skippedSuffix" {
                        expected.push("move suffix: string -> prior");
                    }
                    expected.extend([
                        "move first: string -> first",
                        "call stage(first)",
                        "None -> Option<bool>.None",
                        "None -> Option<bool>.None?",
                        "export Option<bool>.None?",
                        "drop value.0",
                        locals,
                    ]);
                    expected.push(if name == "skippedSuffix" {
                        "cleanup []"
                    } else {
                        "cleanup [drop suffix: string]"
                    });
                    assert_eq!(linear_path(mir, typed, mir.entry), expected, "{name}");
                }
            }

            // Success reaches the suffix; the prefix is always reached.
            let body = |name: &str| &declaration(name).span;
            let offset = |name: &str, text: &[u8], within: usize| {
                let body = body(name);
                body.start()
                    + body
                        .bytes()
                        .windows(text.len())
                        .position(|w| w == text)
                        .unwrap()
                    + within
            };
            for (name, read, prior, used) in [
                (
                    "reachedSuffix",
                    b"suffix".as_slice(),
                    b"let prior = suffix;".as_slice(),
                    b"stage(suffix)".as_slice(),
                ),
                (
                    "movedPrefix",
                    b"first",
                    b"let prior = first;",
                    b"stage(first)",
                ),
            ] {
                let report = lower(name, default);
                assert!(report.mir().is_none() && root(name).mir().is_none());
                assert_eq!(root(name).ordinary_completion(), Completion::SourceRejected);
                let [finding] = report.findings() else {
                    panic!("{name}: expected only the moved source")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), read);
                assert_eq!(finding.related.span.bytes(), read);
                assert_eq!(
                    finding.related.span.start(),
                    offset(name, prior, b"let prior = ".len())
                );
                assert_eq!(
                    finding.site.span.start(),
                    offset(name, used, b"stage(".len())
                );
                assert!(graph.findings().iter().any(|g| g.code() == "R0200"
                    && g.site.span == finding.site.span
                    && g.related.span == finding.related.span));
            }

            // Nearest unsupported siblings keep the caps and blockers measured
            // at the parent revision 37a0a99e, before this route existed.
            for (name, blocked, parent) in [
                (
                    "embedded",
                    b"[stage(first), relay(gate?)]".as_slice(),
                    stats(3981, 611, 4),
                ),
                ("twice", b"[gate?, other?]", stats(3982, 611, 4)),
                ("text", b"[stage(first), label?]", stats(5064, 800, 4)),
                ("nested", b"value", stats(3958, 603, 3)),
            ] {
                let report = lower(name, exact(parent));
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert_eq!(report.statistics(), parent, "{name}");
                assert!(report.mir().is_none() && report.findings().is_empty());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), blocked, "{name}");
                assert_eq!(root(name).ordinary_completion(), Completion::Incomplete);
                for short in one_short(parent) {
                    crate::limit::assert_body_limit(&lower(name, short));
                }
            }

            // The whole graph at its exact caps and independently one short.
            assert_pre_equality_graph_cap(receipt, (240_330, 13_996, 10));
            let whole = stats(240_732, 14_130, 10);
            assert_eq!(graph.statistics(), whole);
            assert_eq!(graph.completion(), GraphCompletion::SourceRejected);
            assert_eq!(analyze_ownership(receipt, exact(whole)).statistics(), whole);
            for short in one_short(whole) {
                let limited = analyze_ownership(receipt, short);
                crate::limit::assert_graph_limit(&limited);
            }
        },
    );
}

#[test]
fn condition_postfix_continues_on_the_ordinary_branch() {
    probe_with_typed(
        "propagation::staging::condition_postfix_continues_on_the_ordinary_branch",
        CONDITION,
        |_, module, receipt, typed| {
            let declaration = |name: &str| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let default = AnalysisLimits::default();
            let graph = analyze_ownership(receipt, default);
            let root = |name| {
                graph
                    .roots()
                    .iter()
                    .find(|r| r.owner() == BodyOwner::Declaration(&declaration(name).id))
                    .unwrap()
            };

            for (name, completion, pinned) in [
                ("condition", Completion::Checked, stats(4074, 821, 6)),
                ("grouped", Completion::Checked, stats(4079, 821, 9)),
                ("scoped", Completion::Checked, stats(4076, 821, 7)),
                ("looped", Completion::Checked, stats(3981, 762, 6)),
                ("branches", Completion::Checked, stats(4553, 1037, 5)),
                ("certainNone", Completion::Checked, stats(3522, 509, 4)),
                ("skippedRead", Completion::Checked, stats(3632, 569, 4)),
                (
                    "conditionRead",
                    Completion::SourceRejected,
                    stats(4015, 790, 5),
                ),
            ] {
                let report = lower(name, default);
                assert_eq!(
                    (report.completion(), report.statistics()),
                    (completion, pinned),
                    "{name} at {:?}",
                    report
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                );
                assert_eq!(root(name).ordinary_completion(), completion, "{name}");
                assert_eq!(
                    root(name).ordinary_mir().is_some(),
                    completion == Completion::Checked,
                    "{name}"
                );
                assert_eq!(
                    lower(name, exact(pinned)).completion(),
                    completion,
                    "{name}"
                );
                for short in one_short(pinned) {
                    let limited = lower(name, short);
                    crate::limit::assert_body_limit(&limited);
                }
            }
            assert!(lower("grouped", default).statistics().max_depth > 6);

            // None exports before the lexical cleanup and reaches neither
            // branch. Some continues to the ordinary test of the payload.
            let failure = |params: &str| {
                vec![
                    "None -> gate?".to_string(),
                    "export gate?".into(),
                    format!("cleanup [{params}]"),
                ]
            };
            for name in ["condition", "grouped", "scoped"] {
                for mir in [
                    lower(name, default).mir().unwrap(),
                    root(name).ordinary_mir().unwrap(),
                ] {
                    let (effects, carrier) = path(mir, typed, mir.entry);
                    assert_eq!(
                        effects,
                        ["copy gate: Option<bool> -> gate", "test gate for Some"]
                    );
                    let [some, none] = carrier.unwrap();
                    assert_eq!(
                        linear_path(mir, typed, none),
                        failure("drop tail: string, end gate: Option<bool>"),
                        "{name}"
                    );
                    let (effects, condition) = path(mir, typed, some);
                    assert!(effects.is_empty(), "{name}: {effects:?}");
                    let [yes, no] = condition.unwrap();
                    for (edge, written) in [(yes, "true"), (no, "false")] {
                        assert_eq!(
                            linear_path(mir, typed, edge),
                            [
                                format!("init {written}"),
                                format!("Some({written}) -> Option.Some({written})"),
                                format!("export Option.Some({written})"),
                                "cleanup [drop tail: string, end gate: Option<bool>]".into(),
                            ],
                            "{name}"
                        );
                    }
                }
            }

            // Each branch moves only its own formal; failure moves neither.
            for mir in [
                lower("branches", default).mir().unwrap(),
                root("branches").ordinary_mir().unwrap(),
            ] {
                let (_, carrier) = path(mir, typed, mir.entry);
                let [some, none] = carrier.unwrap();
                assert_eq!(
                    linear_path(mir, typed, none),
                    failure("drop right: string, drop left: string, end gate: Option<bool>")
                );
                let (_, condition) = path(mir, typed, some);
                let [yes, no] = condition.unwrap();
                for (edge, moved, kept) in [(yes, "left", "right"), (no, "right", "left")] {
                    assert_eq!(
                        linear_path(mir, typed, edge),
                        [
                            format!("move {moved}: string -> taken"),
                            "cleanup [drop taken]".into(),
                            "init true".into(),
                            "Some(true) -> Option.Some(true)".into(),
                            "export Option.Some(true)".into(),
                            format!("cleanup [drop {kept}: string, end gate: Option<bool>]"),
                        ]
                    );
                }
            }

            // In a loop, failure leaves the loop and the function at once.
            for mir in [
                lower("looped", default).mir().unwrap(),
                root("looped").ordinary_mir().unwrap(),
            ] {
                let (effects, carrier) = path(mir, typed, mir.entry);
                assert_eq!(
                    effects,
                    ["copy gate: Option<bool> -> gate", "test gate for Some"]
                );
                let [_, none] = carrier.unwrap();
                assert_eq!(
                    linear_path(mir, typed, none),
                    failure("drop tail: string, end gate: Option<bool>")
                );
            }

            // A known None reaches neither branch: the guarded move and the
            // guarded second disposal are not reached.
            for (name, effects) in [
                (
                    "certainNone",
                    vec![
                        "None -> Option<bool>.None",
                        "None -> Option<bool>.None?",
                        "export Option<bool>.None?",
                        "cleanup [drop tail: string]",
                    ],
                ),
                (
                    "skippedRead",
                    vec![
                        "dispose retired: string",
                        "init dispose(retired)",
                        "None -> Option<bool>.None",
                        "None -> Option<bool>.None?",
                        "export Option<bool>.None?",
                        "cleanup []",
                    ],
                ),
            ] {
                for mir in [
                    lower(name, default).mir().unwrap(),
                    root(name).ordinary_mir().unwrap(),
                ] {
                    assert_eq!(linear_path(mir, typed, mir.entry), effects, "{name}");
                }
            }

            // The ordinary control and the reached second disposal.
            assert_eq!(
                lower("conditionControl", default).completion(),
                Completion::Checked
            );
            assert!(root("conditionControl").ordinary_mir().is_some());
            let read = lower("conditionRead", default);
            assert!(read.mir().is_none() && root("conditionRead").mir().is_none());
            let [finding] = read.findings() else {
                panic!("expected only the reached second disposal")
            };
            assert_eq!(finding.code(), "R0200");
            let body = &declaration("conditionRead").span;
            let disposals: Vec<_> = body
                .bytes()
                .windows(b"dispose(retired)".len())
                .enumerate()
                .filter(|(_, w)| *w == b"dispose(retired)")
                .map(|(at, _)| body.start() + at)
                .collect();
            assert_eq!(
                (finding.related.span.start(), finding.site.span.start()),
                (disposals[0], disposals[1])
            );
            assert!(graph.findings().iter().any(|g| g.code() == "R0200"
                && g.site.span == finding.site.span
                && g.related.span == finding.related.span));

            // Nearest unsupported siblings keep the caps and blockers measured
            // at the parent revision 37a0a99e, before this route existed.
            for (name, blocked, parent) in [
                ("whileCondition", b"gate?".as_slice(), stats(3389, 431, 4)),
                ("negated", b"!(gate?)", stats(3400, 435, 6)),
                ("called", b"relay(gate?)", stats(3400, 435, 6)),
                ("resultCondition", b"gate?", stats(3412, 441, 6)),
            ] {
                let report = lower(name, exact(parent));
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert_eq!(report.statistics(), parent, "{name}");
                assert!(report.mir().is_none() && report.findings().is_empty());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), blocked, "{name}");
                assert_eq!(root(name).ordinary_completion(), Completion::Incomplete);
                for short in one_short(parent) {
                    crate::limit::assert_body_limit(&lower(name, short));
                }
            }

            assert_pre_equality_graph_cap(receipt, (221_493, 9_847, 9));
            let whole = stats(221_865, 9_981, 9);
            assert_eq!(graph.statistics(), whole);
            assert_eq!(graph.completion(), GraphCompletion::SourceRejected);
            assert_eq!(analyze_ownership(receipt, exact(whole)).statistics(), whole);
            for short in one_short(whole) {
                let limited = analyze_ownership(receipt, short);
                crate::limit::assert_graph_limit(&limited);
            }
        },
    );
}

/// Metering rule 6 (review ci-uaa1, ri-vgz.6 #3): the same postfix tuple and
/// condition after a `return` grant nothing. No variant test, branch, stage or
/// cleanup is emitted for them, and the dead second use of an already disposed
/// string is not `R0200`. The feasible controls are `tuple` and `conditionRead`
/// above, the pruned ones `skippedSuffix` and `skippedRead`.
#[test]
fn dead_postfix_staging_grants_nothing() {
    probe_with_typed(
        "propagation::staging::dead_postfix_staging_grants_nothing",
        r#"
function stage(value: string): string { return value; }
function deadTuple(first: string, gate: Option<bool>, suffix: string): Option<bool> {
    dispose(first);
    return Option.Some(true);
    let value: [string, bool, string] = [stage(first), gate?, stage(suffix)];
}
function deadCondition(gate: Option<bool>, retired: string): Option<bool> {
    dispose(retired);
    return Option.Some(true);
    if (gate?) { dispose(retired); }
}
"#,
        |_, module, receipt, typed| {
            let graph_stats = stats(193_557, 4_955, 5);
            let graph = analyze_ownership(receipt, exact(graph_stats));
            assert_eq!(graph.statistics(), graph_stats);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert!(graph.findings().is_empty());
            for short in one_short(graph_stats) {
                let limited = analyze_ownership(receipt, short);
                crate::limit::assert_graph_limit(&limited);
            }
            for (name, pinned, expected) in [
                (
                    "deadTuple",
                    stats(3_361, 431, 4),
                    &[
                        "dispose first: string",
                        "init dispose(first)",
                        "init true",
                        "Some(true) -> Option.Some(true)",
                        "export Option.Some(true)",
                        "cleanup [drop suffix: string, end gate: Option<bool>]",
                    ][..],
                ),
                (
                    "deadCondition",
                    stats(3_315, 407, 4),
                    &[
                        "dispose retired: string",
                        "init dispose(retired)",
                        "init true",
                        "Some(true) -> Option.Some(true)",
                        "export Option.Some(true)",
                        "cleanup [end gate: Option<bool>]",
                    ][..],
                ),
            ] {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let report = lower_body(receipt, module, &d.id, exact(pinned)).unwrap();
                assert_eq!(
                    (report.completion(), report.statistics()),
                    (Completion::Checked, pinned),
                    "{name}"
                );
                assert!(report.findings().is_empty(), "{name}");
                let mir = report.mir().unwrap();
                assert_eq!(linear_path(mir, typed, mir.entry), expected, "{name}");
                for short in one_short(pinned) {
                    let limited = lower_body(receipt, module, &d.id, short).unwrap();
                    crate::limit::assert_body_limit(&limited);
                }
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.owner() == BodyOwner::Declaration(&d.id))
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Checked, "{name}");
                let mir = root.ordinary_mir().unwrap();
                assert_eq!(linear_path(mir, typed, mir.entry), expected, "{name}");
            }
        },
    );
}
