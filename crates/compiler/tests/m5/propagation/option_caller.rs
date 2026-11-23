use super::*;
use compiler::analysis::{
    Statistics,
    ownership::{DependencyContract, GraphCompletion, Place, analyze_ownership},
    scalars::BodyOwner,
};

const RELAY: &str = r#"
    function relay(input: Option<&string>): Option<&string> {
        let view = input?;
        return Option.Some(view);
    }
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

#[test]
fn selected_shared_option_call_preserves_the_caller_backing() {
    let source = format!(
        "{RELAY}{}",
        r#"
        function caller(input: Option<&string>, tail: string): Option<&string> {
            let view = relay(input)?;
            let _ = input?;
            return Option.Some(view);
        }
    "#
    );
    probe_with_typed(
        "propagation::option_caller::selected_shared_option_call_preserves_the_caller_backing",
        &source,
        |_, module, receipt, typed| {
            let caller = declaration(module, "caller");
            let relay = declaration(module, "relay");
            let option = typed.option_role().unwrap();
            let defaults = AnalysisLimits::default();
            let report = lower_body(receipt, module, &caller.id, defaults).unwrap();
            let mir = report.mir().unwrap_or_else(|| {
                panic!(
                    "blocked at {:?}",
                    report.blocked_at().map(|s| s.span.bytes())
                )
            });
            let prefix = straight_blocks(mir, mir.entry);
            let [copy, invoke, test] = prefix.as_slice() else {
                panic!("copy, call and branch exactly once")
            };
            let Operation::Copy {
                from: input,
                into: Some(argument),
            } = copy.operation
            else {
                panic!("copy the named carrier into the call stage");
            };
            assert_eq!(input.root.span.bytes(), b"input: Option<&string>");
            assert!(!same(input, argument));
            let Operation::CallIntoTemporary {
                callee,
                arguments,
                into,
            } = &invoke.operation
            else {
                panic!("publish the selected call's staged result");
            };
            assert!(std::ptr::eq(*callee, &relay.id));
            assert!(matches!(arguments.as_slice(), [passed] if same(*passed, argument)));
            assert!(!same(*into, argument));
            assert!(
                matches!(test.operation, Operation::TestVariant { operand, .. } if same(operand, *into))
            );
            let (some, none) = variant_edges(mir, mir.entry, &option.some.id, b"relay(input)");
            let success = straight_blocks(mir, some);
            assert!(
                matches!(success[0].operation, Operation::TakeOptionPayload { operand, into: view }
                if same(operand, *into) && view.root.span.bytes() == b"view")
            );
            assert!(
                matches!(success[1].operation, Operation::Copy { from, .. } if same(from, input))
            );
            let (reused, _) = variant_edges(mir, some, &option.some.id, b"input");
            assert_return(
                &linear(mir, reused),
                &option.some.id,
                true,
                &[b"tail: string"],
            );
            let failure = linear(mir, none);
            assert!(
                matches!(failure.as_slice(), [Operation::EndEmptyOption(operand),
                Operation::ConstructVariant { payload: None, .. }, Operation::Export(_), ..]
                if same(*operand, *into))
            );
            assert_return(&failure, &option.none.id, false, &[b"tail: string"]);
            assert!(mir.cancellation().is_none());

            let graph = analyze_ownership(receipt, defaults);
            let root = graph
                .roots()
                .iter()
                .find(|root| {
                    matches!(root.owner(),
                BodyOwner::Declaration(id) if std::ptr::eq(id, &caller.id))
                })
                .unwrap();
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            assert!(root.ordinary_mir().is_some());
            let [edge] = root.dependencies() else {
                panic!("one ordinary call")
            };
            assert_eq!(edge.contract, DependencyContract::OrdinaryCall);
            assert!(std::ptr::eq(edge.declaration, &relay.id));
            assert_eq!(edge.site.span.bytes(), b"relay(input)");
        },
    );
}

#[test]
fn call_result_keeps_local_backing_until_the_descriptor_dies() {
    let source = format!(
        "{RELAY}{}",
        r#"
        function escape(local: string): Option<&string> {
            let input: Option<&string> = Option.Some(&local);
            let view = relay(input)?;
            return Option.Some(view);
        }
        function live(local: string): Option<&string> {
            let input: Option<&string> = Option.Some(&local);
            let view = relay(input)?;
            dispose(local);
            return Option.Some(view);
        }
        function ended(local: string): Option<bool> {
            let input: Option<&string> = Option.Some(&local);
            let _ = relay(input)?;
            dispose(local);
            return Option.Some(true);
        }
        function alwaysNone(input: Option<&string>): Option<&string> {
            return Option<&string>.None;
        }
        function conservative(local: string): Option<&string> {
            let input: Option<&string> = Option.Some(&local);
            let view = alwaysNone(input)?;
            dispose(local);
            return Option.Some(view);
        }
        function select(left: &string, right: &string): &string { return left; }
        function liveLeft(left: string, right: string): Option<&string> {
            let input: Option<&string> = Option.Some(select(&left, &right));
            let view = relay(input)?;
            dispose(left);
            return Option.Some(view);
        }
        function liveRight(left: string, right: string): Option<&string> {
            let input: Option<&string> = Option.Some(select(&left, &right));
            let view = relay(input)?;
            dispose(right);
            return Option.Some(view);
        }
    "#
    );
    probe_with_typed(
        "propagation::option_caller::call_result_keeps_local_backing_until_the_descriptor_dies",
        &source,
        |_, module, receipt, typed| {
            let check = |name| {
                lower_body(
                    receipt,
                    module,
                    &declaration(module, name).id,
                    AnalysisLimits::default(),
                )
                .unwrap()
            };
            for (name, code, site, related) in [
                (
                    "escape",
                    "R0202",
                    b"return Option.Some(view);".as_slice(),
                    b"local: string".as_slice(),
                ),
                (
                    "live",
                    "R0203",
                    b"dispose(local)".as_slice(),
                    b"&local".as_slice(),
                ),
                (
                    "conservative",
                    "R0203",
                    b"dispose(local)".as_slice(),
                    b"&local".as_slice(),
                ),
                (
                    "liveLeft",
                    "R0203",
                    b"dispose(left)".as_slice(),
                    b"&left".as_slice(),
                ),
                (
                    "liveRight",
                    "R0203",
                    b"dispose(right)".as_slice(),
                    b"&right".as_slice(),
                ),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("one finding for {name}")
                };
                assert_eq!(
                    (
                        finding.code(),
                        finding.site.span.bytes(),
                        finding.related.span.bytes()
                    ),
                    (code, site, related)
                );
                let body = &module.syntax.nodes[declaration(module, name).node].span;
                assert!((body.start()..body.end()).contains(&finding.related.span.start()));
            }
            let ended = check("ended");
            let mir = ended.mir().expect("last use ended the borrowed dependency");
            let option = typed.option_role().unwrap();
            let (some, none) = variant_edges(mir, mir.entry, &option.some.id, b"relay(input)");
            let success = linear(mir, some);
            assert!(
                matches!(success.as_slice(), [Operation::TakeOptionPayload { into, .. },
                Operation::Cleanup(actions), Operation::Dispose { operand, .. }, ..]
                if operand.root.span.bytes() == b"local: string" && matches!(actions.as_slice(),
                    [Obligation::EndStorage(view)] if same(*view, *into)))
            );
            assert_return(&success, &option.some.id, true, &[]);
            let failure = linear(mir, none);
            assert!(
                !failure
                    .iter()
                    .any(|op| matches!(op, Operation::Dispose { .. }))
            );
            assert_return(&failure, &option.none.id, false, &[b"local: string"]);
        },
    );
}

#[test]
fn only_a_reached_selected_callee_needs_an_ordinary_body_proof() {
    probe(
        "propagation::option_caller::only_a_reached_selected_callee_needs_an_ordinary_body_proof",
        r#"
        function wrong(input: Option<&string>): Option<&string> {
            let owned = "text";
            return Option.Some(&owned);
        }
        function reached(input: Option<&string>): Option<bool> {
            let _ = wrong(input)?;
            return Option.Some(true);
        }
        function skipped(input: Option<&string>): Option<bool> {
            if (false) { let _ = wrong(input)?; }
            return Option.Some(true);
        }
        "#,
        |_, module, receipt| {
            let run = |name| {
                lower_body(
                    receipt,
                    module,
                    &declaration(module, name).id,
                    AnalysisLimits::default(),
                )
                .unwrap()
            };
            let reached = run("reached");
            assert_eq!(reached.completion(), Completion::SourceRejected);
            assert!(reached.mir().is_none());
            let [finding] = reached.findings() else {
                panic!("the callee's rejection")
            };
            assert_eq!(finding.code(), "R0202");
            assert_eq!(finding.site.span.bytes(), b"return Option.Some(&owned);");
            assert_eq!(finding.related.span.bytes(), b"owned");
            let skipped = run("skipped");
            assert_eq!(skipped.completion(), Completion::Checked);
            assert!(
                !skipped
                    .mir()
                    .unwrap()
                    .blocks
                    .iter()
                    .any(|b| matches!(b.operation, Operation::CallIntoTemporary { .. }))
            );
        },
    );
}

#[test]
fn shared_option_postfix_call_does_not_open_other_call_contracts() {
    let test =
        "propagation::option_caller::shared_option_postfix_call_does_not_open_other_call_contracts";
    probe(
        test,
        r#"
        function choose(left: Option<&string>, right: Option<&string>): Option<&string> {
            let view = left?;
            return Option.Some(view);
        }
        function ambiguous(left: Option<&string>, right: Option<&string>): Option<&string> {
            let view = choose(left, right)?;
            return Option.Some(view);
        }
        "#,
        |_, module, receipt| {
            let d = declaration(module, "ambiguous");
            let report = lower_body(receipt, module, &d.id, AnalysisLimits::default()).unwrap();
            assert_eq!(report.completion(), Completion::Incomplete);
            assert!(report.mir().is_none() && report.findings().is_empty());
            assert_eq!(
                report.blocked_at().unwrap().span.bytes(),
                b"choose(left, right)?"
            );
        },
    );
    probe_report(
        test,
        r#"
        function erased(input: Option<&string>, callback: (Option<&string>) => Option<&string>): Option<&string> {
            let view = callback(input)?;
            return Option.Some(view);
        }
        "#,
        |_, module, typed| {
            use compiler::analysis::typed::{ResultUseBlocker, TypedCompletion};
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none() && typed.bodies().findings().is_empty());
            assert!(
                typed
                    .result_use()
                    .unwrap()
                    .pending()
                    .iter()
                    .any(|pending| pending.blocker == ResultUseBlocker::CallBoundary
                        && pending.site.module == &module.key
                        && pending.site.span.bytes() == b"callback(input)")
            );
        },
    );
    probe_report(
        test,
        r#"
        function store(slot: &mut Option<&string>, input: Option<&string>): Option<bool> {
            let view = input?;
            *slot = Option.Some(view);
            return Option.Some(true);
        }
        "#,
        |_, module, typed| {
            use compiler::analysis::typed::{ResultUseBlocker, TypedCompletion};
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none() && typed.bodies().findings().is_empty());
            let [pending] = typed.result_use().unwrap().pending() else {
                panic!("one unsupported slot")
            };
            assert_eq!(pending.blocker, ResultUseBlocker::Storage);
            assert_eq!(pending.site.module, &module.key);
            assert_eq!(pending.site.span.bytes(), b"*slot = Option.Some(view);");
        },
    );
}

const RELAY_SIBLINGS: &str = r#"
function relay(input: Option<&string>): Option<&string> {
    let view = input?;
    return Option.Some(view);
}
function relayExclusive(input: Option<&mut bool>): Option<&mut bool> {
    let view = input?;
    return Option.Some(view);
}
function choose(left: Option<&string>, right: Option<&string>): Option<&string> {
    let view = left?;
    return Option.Some(view);
}
function generic<T>(input: Option<T>): Option<T> { return input; }
"#;

/// One caller per fixture, so the whole-graph caps attribute to that caller.
/// Checks exact `(work, entries, depth)` for the targeted query and the graph,
/// and `ResourceLimit` one short on each axis independently.
fn relay_caller_keeps_caps(
    test: &str,
    name: &str,
    call: &str,
    expected: Completion,
    target: (usize, usize, usize),
    // The whole graph now, then before the i32 equality source root
    // (ri-785.9, metering rule 11), which must give ResourceLimit.
    graph: [(usize, usize, usize); 2],
) {
    let payload = if name == "callerExclusive" {
        "&mut bool"
    } else {
        "&string"
    };
    let source = format!(
        "{RELAY_SIBLINGS}function {name}(input: Option<{payload}>): Option<{payload}> {{\n    \
         let view = {call}?;\n    return Option.Some(view);\n}}\n"
    );
    let site = format!("{call}?");
    let checked = expected == Completion::Checked;
    let pinned = |(work, entries, max_depth)| {
        let exact = AnalysisLimits {
            max_work: work,
            max_entries: entries,
            max_depth,
        };
        let short = [
            AnalysisLimits {
                max_work: work - 1,
                ..exact
            },
            AnalysisLimits {
                max_entries: entries - 1,
                ..exact
            },
            AnalysisLimits {
                max_depth: max_depth - 1,
                ..exact
            },
        ];
        let stats = Statistics {
            work,
            entries,
            max_depth,
        };
        (stats, exact, short)
    };
    probe_with_typed(test, &source, |_, module, receipt, typed| {
        let caller = declaration(module, name);
        let (stats, exact, short) = pinned(target);
        let report = lower_body(receipt, module, &caller.id, exact).unwrap();
        assert_eq!(report.completion(), expected, "{name}");
        assert_eq!(report.statistics(), stats, "{name}");
        assert!(report.findings().is_empty());
        assert_eq!(report.mir().is_some(), checked, "{name}");
        if name == "callerExclusive" {
            exclusive_relay_moves(report.mir().unwrap(), module, typed);
        }
        assert_eq!(
            report.blocked_at().map(|site| site.span.bytes()),
            (!checked).then_some(site.as_bytes()),
            "{name}"
        );
        for cap in short {
            let limited = lower_body(receipt, module, &caller.id, cap).unwrap();
            assert_eq!(limited.completion(), Completion::ResourceLimit, "{name}");
            assert!(limited.mir().is_none() && limited.findings().is_empty());
        }

        let [graph, pre_equality] = graph;
        assert_eq!(
            analyze_ownership(receipt, pinned(pre_equality).1).completion(),
            GraphCompletion::ResourceLimit,
            "{name}"
        );
        let (stats, exact, short) = pinned(graph);
        let report = analyze_ownership(receipt, exact);
        assert_eq!(report.completion(), GraphCompletion::Incomplete, "{name}");
        assert_eq!(report.statistics(), stats, "{name}");
        let root = report
            .roots()
            .iter()
            .find(|root| {
                matches!(root.owner(),
                BodyOwner::Declaration(id) if std::ptr::eq(id, &caller.id))
            })
            .unwrap();
        assert_eq!(root.ordinary_completion(), expected, "{name}");
        assert_eq!(root.ordinary_mir().is_some(), checked, "{name}");
        if name == "callerExclusive" {
            exclusive_relay_moves(root.ordinary_mir().unwrap(), module, typed);
        }
        assert_eq!(
            root.blocked_at().map(|site| site.span.bytes()),
            (!checked).then_some(site.as_bytes()),
            "{name}"
        );
        if checked {
            let callee = call.trim_start_matches('(').split('(').next().unwrap();
            let relay = declaration(module, callee);
            let [edge] = root.dependencies() else {
                panic!("{name}: one ordinary call")
            };
            assert_eq!(edge.contract, DependencyContract::OrdinaryCall);
            assert!(std::ptr::eq(edge.declaration, &relay.id));
        } else {
            assert!(root.dependencies().is_empty(), "{name}");
        }
        for cap in short {
            assert_eq!(
                analyze_ownership(receipt, cap).completion(),
                GraphCompletion::ResourceLimit,
                "{name}"
            );
        }
    });
}

/// Review ci-uaa1 R-6 (rule 9): the admitted `callerExclusive` moves the
/// exclusive carrier into the relay and takes the descriptor out of its result,
/// with no shared copy: `Move` -> `CallIntoTemporary` -> `TestVariant`, then
/// `TakeOptionPayload` on `Some`, `EndEmptyOption` on `None`.
fn exclusive_relay_moves<'g>(
    mir: &Mir<'g>,
    module: &'g loader::Module,
    typed: &compiler::analysis::typed::TypedReport<'g>,
) {
    let relay = declaration(module, "relayExclusive");
    let option = typed.option_role().unwrap();
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
    let Operation::CallIntoTemporary {
        callee,
        arguments,
        into,
    } = &invoke.operation
    else {
        panic!("publish the relay's staged result");
    };
    assert!(std::ptr::eq(*callee, &relay.id));
    assert!(matches!(arguments.as_slice(), [passed] if same(*passed, argument)));
    let Terminator::Branch { yes, no } = test.terminator else {
        panic!("branch on the relayed variant")
    };
    assert!(matches!(test.operation,
        Operation::TestVariant { operand, success } if same(operand, *into) && success == &option.some.id));
    let success = linear(mir, yes);
    assert!(matches!(success.as_slice(),
        [Operation::TakeOptionPayload { operand, into: view },
         Operation::Move { from, into: Some(_) }, ..]
        if same(*operand, *into) && view.root.span.bytes() == b"view" && same(*from, *view)));
    let failure = linear(mir, no);
    assert!(matches!(failure.as_slice(),
        [Operation::EndEmptyOption(operand), ..] if same(*operand, *into)));
}

/// Standards review 3651c5b4: unsupported siblings of the shared relay keep the
/// parent revision's caps (measured at cb94538f, before the relay was admitted).
#[test]
fn unsupported_option_relay_siblings_keep_parent_caps() {
    let test = "propagation::option_caller::unsupported_option_relay_siblings_keep_parent_caps";
    for (name, call, target, graph) in [
        (
            "callerNonmonomorphic",
            "generic(input)",
            (3089, 284, 4),
            [(196007, 5926, 5), (195691, 5792, 4)],
        ),
        (
            "callerComputed",
            "relay(((relay(input))))",
            (3097, 288, 4),
            [(196255, 5930, 5), (195939, 5796, 4)],
        ),
        (
            "callerPipe",
            "input |> relay(_)",
            (3087, 283, 4),
            [(195945, 5925, 5), (195629, 5791, 4)],
        ),
        (
            "callerGroupPipe",
            "(input |> relay(_))",
            (3092, 285, 4),
            [(196070, 5927, 5), (195754, 5793, 4)],
        ),
        (
            "callerMultiple",
            "choose(input, input)",
            (3091, 285, 4),
            [(196069, 5927, 5), (195753, 5793, 4)],
        ),
    ] {
        relay_caller_keeps_caps(test, name, call, Completion::Incomplete, target, graph);
    }
}

/// The screen admits a grouped whole carrier without walking it: direct and
/// grouped relays keep the caps the relay was admitted at (68e4fa7).
/// `callerExclusive` is admitted by ri-vgz.3 (gap-analysis C1), which moves the
/// exclusive carrier into the relay; metering rule 11 re-pin, see
/// `exclusive_relay_caller_exceeds_its_parent_caps`.
#[test]
fn admitted_option_relay_callers_keep_their_caps() {
    let test = "propagation::option_caller::admitted_option_relay_callers_keep_their_caps";
    for (name, call, target, graph) in [
        (
            "callerDirect",
            "relay(input)",
            (9516, 1677, 4),
            [(198870, 6758, 5), (198554, 6624, 4)],
        ),
        (
            "callerGroup",
            "((relay(((input)))))",
            (9536, 1685, 8),
            [(199122, 6762, 8), (198806, 6628, 8)],
        ),
        (
            "callerExclusive",
            "relayExclusive(input)",
            (9466, 1677, 4),
            [(198837, 6758, 5), (198521, 6624, 4)],
        ),
    ] {
        relay_caller_keeps_caps(test, name, call, Completion::Checked, target, graph);
    }
}

/// Metering rule 11: ri-vgz.3 (gap-analysis C1) admits `relayExclusive(input)?`,
/// so `callerExclusive` moves from `Incomplete` at targeted 3,089/284/4 and
/// graph 195,691/5,792/4 (A3, measured at cb94538f) to `Checked` at 9,466/1,677/4
/// and 198,521/6,624/4. The old caps now give `ResourceLimit`.
#[test]
fn exclusive_relay_caller_exceeds_its_parent_caps() {
    let source = format!(
        "{RELAY_SIBLINGS}function callerExclusive(input: Option<&mut bool>): Option<&mut bool> {{\n    \
         let view = relayExclusive(input)?;\n    return Option.Some(view);\n}}\n"
    );
    probe(
        "propagation::option_caller::exclusive_relay_caller_exceeds_its_parent_caps",
        &source,
        |_, module, receipt| {
            let caller = declaration(module, "callerExclusive");
            let old = AnalysisLimits {
                max_work: 3089,
                max_entries: 284,
                max_depth: 4,
            };
            let report = lower_body(receipt, module, &caller.id, old).unwrap();
            assert_eq!(report.completion(), Completion::ResourceLimit);
            assert!(report.mir().is_none() && report.findings().is_empty());
            let old = AnalysisLimits {
                max_work: 195691,
                max_entries: 5792,
                max_depth: 4,
            };
            let report = analyze_ownership(receipt, old);
            assert_eq!(report.completion(), GraphCompletion::ResourceLimit);
            assert!(report.findings().is_empty());
            // Rule 8 (review ci-uaa1 R-7): and no root publishes MIR.
            assert!(
                report
                    .roots()
                    .iter()
                    .all(|root| root.mir().is_none() && root.ordinary_mir().is_none())
            );
        },
    );
}
