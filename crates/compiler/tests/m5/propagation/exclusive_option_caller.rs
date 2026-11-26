//! Ticket ri-vgz.3 (gap-analysis C1): `forward(input)?` over an exclusive
//! `Option<&mut bool>` moves the carrier and its descriptor across a checked
//! ordinary relay, with no shared copy.
use super::*;
use compiler::analysis::typed::TypedCompletion;
use compiler::analysis::{
    Statistics,
    ownership::{DependencyContract, GraphCompletion, Place, analyze_ownership},
    scalars::BodyOwner,
};

const SIBLINGS: &str = r#"
function forward(input: Option<&mut bool>): Option<&mut bool> {
    let view = input?;
    return Option.Some(view);
}
function choose(left: Option<&mut bool>, right: Option<&mut bool>): Option<&mut bool> {
    let view = left?;
    return Option.Some(view);
}
function generic<T>(input: Option<T>): Option<T> { return input; }
"#;

fn declaration<'g>(module: &'g loader::Module, name: &str) -> &'g loader::Declaration {
    module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some(name))
        .unwrap()
}

fn same(left: Place<'_>, right: Place<'_>) -> bool {
    left.projection.is_none() && right.projection.is_none() && left.root.node == right.root.node
}

#[derive(Clone, Copy)]
enum Expect {
    Checked,
    Incomplete(&'static str),
    /// Code, site and related site of the one finding.
    Rejected(&'static str, &'static str, &'static str),
}

impl Expect {
    fn completion(self) -> Completion {
        match self {
            Self::Checked => Completion::Checked,
            Self::Incomplete(_) => Completion::Incomplete,
            Self::Rejected(..) => Completion::SourceRejected,
        }
    }
}

/// `(work, entries, depth)` as the exact limits, its statistics, and the
/// limits one short on a single axis. An axis measured at 0 has no one-short cap.
fn pinned(
    (work, entries, max_depth): (usize, usize, usize),
) -> (Statistics, AnalysisLimits, Vec<AnalysisLimits>) {
    let exact = AnalysisLimits {
        max_work: work,
        max_entries: entries,
        max_depth,
    };
    let short = [
        work.checked_sub(1)
            .map(|max_work| AnalysisLimits { max_work, ..exact }),
        entries.checked_sub(1).map(|max_entries| AnalysisLimits {
            max_entries,
            ..exact
        }),
        max_depth
            .checked_sub(1)
            .map(|max_depth| AnalysisLimits { max_depth, ..exact }),
    ]
    .into_iter()
    .flatten()
    .collect();
    let stats = Statistics {
        work,
        entries,
        max_depth,
    };
    (stats, exact, short)
}

/// One caller per fixture beside `SIBLINGS`, so the whole-graph caps
/// attribute to that caller. Pins the exact targeted and graph caps,
/// `ResourceLimit` one short on each axis independently, the outcome of the
/// targeted query and of the caller's ordinary graph root, and the graph's
/// overall completion.
fn exclusive_relay_caller(
    test: &str,
    name: &str,
    caller: &str,
    expect: Expect,
    target: (usize, usize, usize),
    // The whole graph now, then before the i32 equality source root
    // (ri-785.9, metering rule 11), which must give ResourceLimit.
    graph: [(usize, usize, usize); 2],
    graph_completion: GraphCompletion,
) {
    let source = format!("{SIBLINGS}{caller}");
    let expected = expect.completion();
    let checked = expected == Completion::Checked;
    probe(test, &source, |_, module, receipt| {
        let function = declaration(module, name);
        let (stats, exact, short) = pinned(target);
        let report = lower_body(receipt, module, &function.id, exact).unwrap();
        assert_eq!(
            (report.completion(), report.statistics()),
            (expected, stats),
            "{name}: blocked at {:?}",
            report
                .blocked_at()
                .map(|s| s.span.bytes().escape_ascii().to_string())
        );
        assert_eq!(report.mir().is_some(), checked, "{name}");
        match expect {
            Expect::Checked => assert!(report.findings().is_empty()),
            Expect::Incomplete(site) => {
                assert!(report.findings().is_empty());
                assert_eq!(
                    report.blocked_at().map(|s| s.span.bytes()),
                    Some(site.as_bytes()),
                    "{name}"
                );
            }
            Expect::Rejected(code, site, related) => {
                let [finding] = report.findings() else {
                    panic!("{name}: one reached finding")
                };
                assert_eq!(
                    (
                        finding.code(),
                        finding.site.span.bytes(),
                        finding.related.span.bytes()
                    ),
                    (code, site.as_bytes(), related.as_bytes()),
                    "{name}"
                );
                assert_eq!(finding.site.module, &module.key);
                assert!(finding.related.span.start() < finding.site.span.start());
            }
        }
        for cap in short {
            let limited = lower_body(receipt, module, &function.id, cap).unwrap();
            crate::limit::assert_body_limit(&limited);
        }

        let [graph, pre_equality] = graph;
        crate::limit::assert_graph_limit(&analyze_ownership(receipt, pinned(pre_equality).1));
        let (stats, exact, short) = pinned(graph);
        let report = analyze_ownership(receipt, exact);
        assert_eq!(
            (report.completion(), report.statistics()),
            (graph_completion, stats),
            "{name}"
        );
        let root = report
            .roots()
            .iter()
            .find(|root| {
                matches!(root.owner(),
                BodyOwner::Declaration(id) if std::ptr::eq(id, &function.id))
            })
            .unwrap();
        assert_eq!(root.ordinary_completion(), expected, "{name}");
        assert_eq!(root.ordinary_mir().is_some(), checked, "{name}");
        match expect {
            Expect::Checked => {
                // `dead` also depends on its `dispose(flag)`.
                let forward = &declaration(module, "forward").id;
                let relayed: Vec<_> = root
                    .dependencies()
                    .iter()
                    .filter(|edge| std::ptr::eq(edge.declaration, forward))
                    .map(|edge| edge.contract)
                    .collect();
                assert_eq!(relayed, [DependencyContract::OrdinaryCall], "{name}");
            }
            Expect::Incomplete(site) => {
                assert_eq!(
                    root.blocked_at().map(|s| s.span.bytes()),
                    Some(site.as_bytes()),
                    "{name}"
                );
                assert!(root.dependencies().is_empty(), "{name}");
            }
            Expect::Rejected(code, site, related) => {
                let found: Vec<_> = report
                    .findings()
                    .iter()
                    .filter(|f| f.site.module == &module.key)
                    .map(|f| (f.code(), f.site.span.bytes(), f.related.span.bytes()))
                    .collect();
                assert_eq!(
                    found,
                    [(code, site.as_bytes(), related.as_bytes())],
                    "{name}"
                );
            }
        }
        for cap in short {
            let limited = analyze_ownership(receipt, cap);
            crate::limit::assert_graph_limit(&limited);
        }
    });
}

#[test]
fn exclusive_option_relay_moves_the_carrier_and_descriptor() {
    let source = format!(
        "{SIBLINGS}{}",
        r#"
function callerPostfix(input: Option<&mut bool>, tail: string): Option<&mut bool> {
    let view = forward(input)?;
    return Option.Some(view);
}
"#
    );
    probe_with_typed(
        "propagation::exclusive_option_caller::exclusive_option_relay_moves_the_carrier_and_descriptor",
        &source,
        |_, module, receipt, typed| {
            let caller = declaration(module, "callerPostfix");
            let forward = declaration(module, "forward");
            let option = typed.option_role().unwrap();
            let report =
                lower_body(receipt, module, &caller.id, AnalysisLimits::default()).unwrap();
            let mir = report.mir().unwrap_or_else(|| {
                panic!(
                    "blocked at {:?}",
                    report.blocked_at().map(|s| s.span.bytes())
                )
            });
            assert!(report.findings().is_empty());
            // No shared copy of the carrier, its descriptor or its payload.
            assert!(!mir.blocks.iter().any(|b| matches!(
                b.operation,
                Operation::Copy { .. } | Operation::CopyOptionPayload { .. }
            )));
            let prefix = straight_blocks(mir, mir.entry);
            let [moved, invoke, test] = prefix.as_slice() else {
                panic!("move, call and branch exactly once")
            };
            let Operation::Move {
                from: input,
                into: Some(argument),
            } = moved.operation
            else {
                panic!("move the named carrier into the call stage");
            };
            assert_eq!(input.root.span.bytes(), b"input: Option<&mut bool>");
            assert!(!same(input, argument));
            let Operation::CallIntoTemporary {
                callee,
                arguments,
                into,
            } = &invoke.operation
            else {
                panic!("publish the selected call's staged result");
            };
            assert!(std::ptr::eq(*callee, &forward.id));
            assert!(matches!(arguments.as_slice(), [passed] if same(*passed, argument)));
            assert!(!same(*into, argument) && !same(*into, input));
            assert!(
                matches!(test.operation, Operation::TestVariant { operand, .. } if same(operand, *into))
            );
            let (some, none) = variant_edges(mir, mir.entry, &option.some.id, b"forward(input)");
            let success = linear(mir, some);
            let [
                Operation::TakeOptionPayload {
                    operand,
                    into: view,
                },
                Operation::Move {
                    from,
                    into: Some(descriptor),
                },
                Operation::ConstructVariant {
                    variant,
                    payload: Some(payload),
                    into: out,
                },
                Operation::Export(exported),
                ..,
            ] = success.as_slice()
            else {
                panic!("take and move one descriptor into the exported Some")
            };
            assert!(same(*operand, *into));
            assert_eq!(view.root.span.bytes(), b"view");
            assert!(same(*from, *view) && same(*descriptor, *payload) && same(*out, *exported));
            assert_eq!(*variant, &option.some.id);
            assert_return(&success, &option.some.id, true, &[b"tail: string"]);
            let failure = linear(mir, none);
            assert!(matches!(failure.as_slice(),
                [Operation::EndEmptyOption(operand), Operation::ConstructVariant { payload: None, .. },
                 Operation::Export(_), ..] if same(*operand, *into)));
            assert_return(&failure, &option.none.id, false, &[b"tail: string"]);
            // The moved carrier, the stage and the descriptor are not ended again.
            for op in success.iter().chain(&failure) {
                if let Operation::Cleanup(cleanup) = op {
                    assert!(cleanup.iter().all(|entry| !matches!(entry,
                        Obligation::EndStorage(p) if same(*p, input) || same(*p, argument)
                            || same(*p, *into) || same(*p, *view))));
                }
            }
            assert!(mir.cancellation().is_none());
        },
    );
}

/// Positive: the call relay and its grouped form (rule 5: the same receipt,
/// deeper by the written groups). At the parent revision b5e361de both were
/// `Incomplete` at their postfix: `callerPostfix` at targeted 3,063/271/4 and
/// graph 193,052/5,277/4, `callerGroup` at 3,073/275/5 and 193,302/5,281/5.
#[test]
fn exclusive_option_relay_callers_check_at_pinned_caps() {
    let test =
        "propagation::exclusive_option_caller::exclusive_option_relay_callers_check_at_pinned_caps";
    for (name, call, target, graph) in [
        (
            "callerPostfix",
            "forward(input)",
            (8990, 1651, 4),
            [(195732, 6243, 5), (195872, 6109, 4)],
        ),
        (
            "callerGroup",
            "((forward(((input)))))",
            (9010, 1659, 8),
            [(195984, 6247, 8), (196124, 6113, 8)],
        ),
    ] {
        let caller = format!(
            "function {name}(input: Option<&mut bool>): Option<&mut bool> {{\n    \
             let view = {call}?;\n    return Option.Some(view);\n}}\n"
        );
        exclusive_relay_caller(
            test,
            name,
            &caller,
            Expect::Checked,
            target,
            graph,
            GraphCompletion::Incomplete,
        );
    }
}

/// Metering rule 2: the nearest unsupported siblings of the exclusive relay
/// keep the caps measured at the parent revision b5e361de, before the relay
/// was admitted, and stay `Incomplete` at the same postfix.
#[test]
fn unsupported_exclusive_relay_siblings_keep_parent_caps() {
    let test = "propagation::exclusive_option_caller::unsupported_exclusive_relay_siblings_keep_parent_caps";
    for (name, call, site, target, graph) in [
        (
            "callerComputed",
            "forward(((forward(input))))",
            "forward(((forward(input))))?",
            (3083, 275, 4),
            [(193628, 5415, 5), (193300, 5281, 4)],
        ),
        (
            "callerPipe",
            "input |> forward(_)",
            "input |> forward(_)?",
            (3072, 270, 4),
            [(193317, 5410, 5), (192990, 5276, 4)],
        ),
        (
            "callerGroupPipe",
            "(input |> forward(_))",
            "(input |> forward(_))?",
            (3078, 272, 4),
            [(193443, 5412, 5), (193115, 5278, 4)],
        ),
        (
            "callerGeneric",
            "generic(input)",
            "generic(input)?",
            (3075, 271, 4),
            [(193380, 5411, 5), (193052, 5277, 4)],
        ),
        (
            "callerMultiple",
            "choose(input, input)",
            "choose(input, input)?",
            (3077, 272, 4),
            [(193442, 5412, 5), (193114, 5278, 4)],
        ),
    ] {
        let caller = format!(
            "function {name}(input: Option<&mut bool>): Option<&mut bool> {{\n    \
             let view = {call}?;\n    return Option.Some(view);\n}}\n"
        );
        exclusive_relay_caller(
            test,
            name,
            &caller,
            Expect::Incomplete(site),
            target,
            graph,
            GraphCompletion::Incomplete,
        );
    }
}

/// The relayed descriptor keeps the carrier's exclusive loan: the moved
/// carrier cannot be read again, and while the descriptor is live its local
/// backing is neither accessed, escaped nor released. Once the descriptor is
/// dead, release checks.
#[test]
fn exclusive_option_relay_checks_reached_moves_and_loans() {
    let test = "propagation::exclusive_option_caller::exclusive_option_relay_checks_reached_moves_and_loans";
    for (name, caller, expect, target, graph, graph_completion) in [
        (
            "reuse",
            r#"
function reuse(input: Option<&mut bool>): Option<&mut bool> {
    let view = forward(input)?;
    return input;
}
"#,
            Expect::Rejected("R0200", "input", "input"),
            (5081, 960, 4),
            [(195328, 6101, 5), (195468, 5967, 4)],
            GraphCompletion::SourceRejected,
        ),
        (
            "conflict",
            r#"
function conflict(): Option<bool> {
    let mut flag = false;
    let input: Option<&mut bool> = Option.Some(&mut flag);
    let view = forward(input)?;
    let _ = &flag;
    return Option.Some(*view);
}
"#,
            Expect::Rejected("R0201", "&flag", "&mut flag"),
            (6515, 1336, 5),
            [(197303, 6477, 5), (197442, 6343, 5)],
            GraphCompletion::SourceRejected,
        ),
        (
            "escape",
            r#"
function escape(): Option<&mut bool> {
    let mut flag = false;
    let input: Option<&mut bool> = Option.Some(&mut flag);
    let view = forward(input)?;
    return Option.Some(view);
}
"#,
            Expect::Rejected("R0202", "return Option.Some(view);", "flag"),
            (5476, 1182, 5),
            [(196144, 6323, 5), (196283, 6189, 5)],
            GraphCompletion::SourceRejected,
        ),
        (
            "release",
            r#"
function release(): Option<bool> {
    let mut flag = false;
    let input: Option<&mut bool> = Option.Some(&mut flag);
    let view = forward(input)?;
    dispose(flag);
    return Option.Some(*view);
}
"#,
            Expect::Rejected("R0203", "dispose(flag)", "&mut flag"),
            (6631, 1345, 5),
            [(197539, 6486, 5), (197678, 6352, 5)],
            GraphCompletion::SourceRejected,
        ),
        (
            "dead",
            r#"
function dead(): Option<bool> {
    let mut flag = false;
    let input: Option<&mut bool> = Option.Some(&mut flag);
    let view = forward(input)?;
    let result = *view;
    dispose(flag);
    return Option.Some(result);
}
"#,
            Expect::Checked,
            (13398, 2461, 5),
            [(198373, 6789, 5), (198511, 6655, 5)],
            GraphCompletion::Incomplete,
        ),
    ] {
        exclusive_relay_caller(test, name, caller, expect, target, graph, graph_completion);
    }
}

/// Review ci-uaa1 (ri-vgz.3 #3): the relayed descriptor carries no exact
/// target, so a write through it is the open question. M4 issues no receipt
/// for the write, so ownership never runs: the typed query stops `Incomplete`
/// at the write with a pending `Storage` Result use (as RFC 0007 O1/O2, `ri-1up`).
#[test]
fn write_through_a_relayed_view_has_no_typed_receipt() {
    let source = format!(
        "{SIBLINGS}{}",
        r#"
function writeThrough(input: Option<&mut bool>): Option<bool> {
    let view = forward(input)?;
    *view = true;
    return Option.Some(true);
}
"#
    );
    probe_report(
        "propagation::exclusive_option_caller::write_through_a_relayed_view_has_no_typed_receipt",
        &source,
        |request, _, _| {
            let graph = loader::load_installed(request.clone()).unwrap();
            let (stats, exact, short) = pinned((436_053, 55_710, 12));
            let typed = analyze_typed(&graph, exact);
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert_eq!(typed.statistics(), stats);
            assert!(typed.receipt().is_none());
            let pending: Vec<_> = typed
                .result_use()
                .into_iter()
                .flat_map(|uses| uses.pending())
                .map(|p| {
                    (
                        format!("{:?}", p.blocker),
                        p.site.span.bytes().escape_ascii().to_string(),
                    )
                })
                .collect();
            assert_eq!(
                pending,
                [("Storage".to_string(), "*view = true;".to_string())]
            );
            for cap in short {
                let limited = analyze_typed(&graph, cap);
                crate::limit::assert_typed_limit(&limited);
            }
        },
    );
}

/// The relay is admitted only at a postfix over an exclusive Boolean carrier:
/// returning the call unpropagated and an `Option<&mut i32>` carrier stay
/// `Incomplete` at their sites, at the caps of the parent revision b5e361de.
#[test]
fn unpropagated_call_and_other_payload_stay_incomplete() {
    let test =
        "propagation::exclusive_option_caller::unpropagated_call_and_other_payload_stay_incomplete";
    for (name, caller, site, target, graph) in [
        (
            "caller",
            r#"
function caller(input: Option<&mut bool>): Option<&mut bool> {
    return forward(input);
}
"#,
            "forward(input)",
            (3036, 253, 4),
            [(193100, 5393, 5), (192773, 5259, 4)],
        ),
        (
            "otherPayload",
            r#"
function otherPayload(input: Option<&mut i32>): Option<&mut i32> {
    let view = input?;
    return Option.Some(view);
}
"#,
            "input: Option<&mut i32>",
            (1929, 51, 0),
            [(192114, 5191, 5), (191788, 5057, 4)],
        ),
    ] {
        exclusive_relay_caller(
            test,
            name,
            caller,
            Expect::Incomplete(site),
            target,
            graph,
            GraphCompletion::Incomplete,
        );
    }
}
