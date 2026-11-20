use super::*;
use compiler::analysis::ownership::{
    CancellationExit, CancellationSuccessor, Cleanup as MirCleanup, RetainedPlace,
};
use compiler::analysis::scalars::{BodyOwner, NodeFactKind};
use compiler::parser::{NodeId, NodeKind};

#[test]
fn installed_loop_backedge_cancellation_rejects_unrepaired_store() {
    check_pair(
        "cancellation::installed_loop_backedge_cancellation_rejects_unrepaired_store",
        r#"
function repairBefore(slot: &mut &bool, input: &bool, enter: bool): bool {
    let text = "owned";
    let mut holder = input;
    let flag = true;
    holder = &flag;
    *slot = &flag;
    *slot = input;
    while (enter) { continue; }
    return flag;
}

function repairAfter(slot: &mut &bool, input: &bool, enter: bool): bool {
    let text = "owned";
    let mut holder = input;
    let flag = true;
    holder = &flag;
    *slot = &flag;
    while (enter) { continue; }
    *slot = input;
    return flag;
}
        "#,
        b"while (enter) { continue; }",
    );
}

#[test]
fn installed_correlated_repair_is_not_a_cancellation_escape() {
    probe_with_typed(
        "cancellation::installed_correlated_repair_is_not_a_cancellation_escape",
        r#"
function repairs(slot: &mut &bool, input: &bool, enter: bool): bool {
    let flag = true;
    *slot = &flag;
    if (enter) { *slot = input; }
    while (enter) { continue; }
    *slot = input;
    return flag;
}

function alias(slot: &mut &bool, input: &bool, enter: bool): bool {
    let again = enter;
    let flag = true;
    *slot = &flag;
    if (again) { *slot = input; }
    while (enter) { continue; }
    *slot = input;
    return flag;
}

function constant(slot: &mut &bool, input: &bool): bool {
    let never = false;
    let flag = true;
    *slot = &flag;
    while (never) { continue; }
    *slot = input;
    return flag;
}
        "#,
        |request, module, receipt, _| {
            for name in ["repairs", "alias", "constant"] {
                let report = query(receipt, module, name);
                assert_eq!(
                    report.completion(),
                    Completion::Checked,
                    "{name}, test_mode={}: {:?}",
                    request.test_mode,
                    report
                        .findings()
                        .iter()
                        .map(|finding| finding.code())
                        .collect::<Vec<_>>()
                );
                let mir = report.mir().unwrap();
                match name {
                    "repairs" => {
                        let proof = mir.cancellation().expect("direct immutable parameter");
                        let [CancellationSuccessor::LoopBackedge { on_delivery }] =
                            (0..mir.blocks.len())
                                .filter_map(|index| proof.successor(index))
                                .collect::<Vec<_>>()[..]
                        else {
                            panic!("only the repaired branch can reach the backedge");
                        };
                        assert_eq!(
                            on_delivery
                                .retained
                                .as_ref()
                                .unwrap()
                                .last_store
                                .unwrap()
                                .span
                                .bytes(),
                            b"*slot = input;"
                        );
                        // The other identical store is after the loop, not its witness.
                        let nodes = &module.syntax.nodes;
                        let decl = declaration(module, name);
                        let body = nodes[decl.node].children[1];
                        let conditional = nodes[body].children[2];
                        let repair = nodes[nodes[conditional].children[1]].children[0];
                        assert_eq!(
                            on_delivery
                                .retained
                                .as_ref()
                                .unwrap()
                                .last_store
                                .unwrap()
                                .node,
                            repair
                        );
                    }
                    "alias" => assert!(mir.cancellation().is_none()),
                    "constant" => {
                        if let Some(proof) = mir.cancellation() {
                            assert!(
                                (0..mir.blocks.len()).all(|index| proof.successor(index).is_none())
                            );
                        }
                    }
                    _ => unreachable!(),
                }
            }
        },
    );
}

#[test]
fn installed_callee_entry_cancellation_rejects_unrepaired_store() {
    check_pair(
        "cancellation::installed_callee_entry_cancellation_rejects_unrepaired_store",
        r#"
function observe(): void {}

function repairBefore(slot: &mut &bool, input: &bool): bool {
    let text = "owned";
    let mut holder = input;
    let flag = true;
    holder = &flag;
    *slot = &flag;
    *slot = input;
    observe();
    return flag;
}

function repairAfter(slot: &mut &bool, input: &bool): bool {
    let text = "owned";
    let mut holder = input;
    let flag = true;
    holder = &flag;
    *slot = &flag;
    observe();
    *slot = input;
    return flag;
}
        "#,
        b"observe();",
    );
}

#[test]
fn installed_retaining_call_cancellation_checks_the_reached_body() {
    probe_with_typed(
        "cancellation::installed_retaining_call_cancellation_checks_the_reached_body",
        r#"
function safeBody(slot: &mut &bool, input: &bool, enter: bool): void {
    let flag = true;
    *slot = &flag;
    *slot = input;
    while ((((((((enter)))))))) { continue; }
}
function unsafeBody(slot: &mut &bool, input: &bool, enter: bool): void {
    let flag = true;
    *slot = &flag;
    while (enter) { continue; }
    *slot = input;
}
function good(enter: bool): void {
    let prior = false;
    let input = true;
    let mut slot = &prior;
    safeBody(&mut slot, &input, enter);
}
function bad(enter: bool): void {
    let prior = false;
    let input = true;
    let mut slot = &prior;
    unsafeBody(&mut slot, &input, enter);
}
function safeWithOwner(slot: &mut &bool, input: &bool, enter: bool): void {
    let calleeText = "callee";
    let flag = true;
    *slot = &flag;
    if (enter) { *slot = input; }
    while (enter) { continue; }
    *slot = input;
    let calleeSuffix = "only on normal return";
}
function goodWithOwner(enter: bool): bool {
    let callerText = "caller";
    let initial = false;
    let mut slot = &initial;
    let prior = false;
    let input = true;
    slot = &prior;
    safeWithOwner(&mut slot, &input, enter);
    let callerSuffix = "only on normal return";
    return enter;
}
function nestedCaller(enter: bool): void { bad(enter); }
import std::core::dispose as release;
type Owner = { enter: bool; text: string; }
implement release(value: Owner): void { bad(value.enter); }
function maskedCaller(value: Owner): void { dispose(value); }
function constantCaller(): void {
    let prior = false;
    let input = true;
    let mut slot = &prior;
    unsafeBody(&mut slot, &input, false);
}
function guardedCaller(enter: bool): void {
    let prior = false;
    let input = true;
    let mut slot = &prior;
    if (enter) {} else { unsafeBody(&mut slot, &input, enter); }
}
function leaf(): void {}
function unsafeWithNested(slot: &mut &bool, input: &bool, enter: bool): void {
    let flag = true;
    *slot = &flag;
    while (enter) { continue; }
    *slot = input;
    leaf();
}
function callerOfNestedBody(enter: bool): void {
    let prior = false;
    let input = true;
    let mut slot = &prior;
    unsafeWithNested(&mut slot, &input, enter);
}
        "#,
        |request, module, receipt, _| {
            let bad = query(receipt, module, "bad");
            assert_eq!(
                bad.completion(),
                Completion::SourceRejected,
                "test_mode={}: {bad:?}",
                request.test_mode
            );
            assert!(bad.mir().is_none() && bad.blocked_at().is_none());
            let [finding] = bad.findings() else {
                panic!("expected the reached callee's escape: {bad:?}");
            };
            let nodes = &module.syntax.nodes;
            let unsafe_body = nodes[declaration(module, "unsafeBody").node].children[1];
            let [_, store, backedge, _] = nodes[unsafe_body].children[..] else {
                panic!("expected paired stores around the backedge");
            };
            assert_eq!(finding.code(), "R0202");
            for (actual, node, bytes) in [
                (finding.site, store, b"*slot = &flag;".as_slice()),
                (finding.related, nodes[store].children[1], b"&flag"),
                (
                    finding.cancellation_delivery.unwrap(),
                    backedge,
                    b"while (enter) { continue; }",
                ),
            ] {
                assert_eq!((actual.module, actual.node), (&module.key, node));
                assert_eq!(actual.span.bytes(), bytes);
            }
            for name in [
                "nestedCaller",
                "maskedCaller",
                "constantCaller",
                "guardedCaller",
                "callerOfNestedBody",
            ] {
                let report = query(receipt, module, name);
                assert_eq!(
                    report.completion(),
                    Completion::Checked,
                    "{name}: {report:?}"
                );
                assert!(report.mir().unwrap().cancellation().is_none(), "{name}");
            }
            for (name, callee) in [("good", "safeBody"), ("goodWithOwner", "safeWithOwner")] {
                let good = query(receipt, module, name);
                assert_eq!(good.completion(), Completion::Checked, "{name}: {good:?}");
                let mir = good.mir().unwrap();
                let proof = mir
                    .cancellation()
                    .expect("bounded caller cancellation proof");
                assert!(
                    matches!(proof.on_entry_delivery.cleanup.as_slice(), [MirCleanup::EndStorage(place)]
                    if place.root.span.bytes() == b"enter: bool")
                );
                let [(block, CancellationSuccessor::RetainingCall(call))] = (0..mir.blocks.len())
                    .filter_map(|i| proof.successor(i).map(|edge| (&mir.blocks[i], edge)))
                    .collect::<Vec<_>>()[..]
                else {
                    panic!("expected only the reached retaining call");
                };
                let callee = declaration(module, callee);
                let Operation::CallRetainingWithBool {
                    callee: selected,
                    destination: RetainedPlace::Local(slot),
                    arguments,
                    ..
                } = &block.operation
                else {
                    panic!("expected the exact ordinary retaining call");
                };
                assert_eq!(call.callee, &callee.id);
                assert_eq!(*selected, call.callee);
                assert_place(call.destination, slot.root.node, module);
                let signature = nodes[callee.node].children[0];
                let parameters = nodes[signature]
                    .children
                    .iter()
                    .find(|&&node| nodes[node].kind == NodeKind::ValueParameters)
                    .map(|&node| nodes[node].children.as_slice())
                    .unwrap();
                for ((transfer, actual), &formal) in
                    call.arguments.iter().zip(arguments).zip(parameters)
                {
                    assert_place(transfer.staged, actual.root.node, module);
                    assert_place(transfer.parameter, formal, module);
                }

                let body = &call.body;
                let callee_proof = body.cancellation().expect("enabled callee proof");
                assert_eq!(callee_proof.entry_site.node, nodes[callee.node].children[1]);
                let at_entry = &callee_proof.on_entry_delivery;
                assert_parameter_ends(&at_entry.cleanup, parameters, module);
                let retained = at_entry.retained.as_ref().unwrap();
                assert_place(retained.parameter, parameters[0], module);
                assert!(
                    retained.last_store.is_none(),
                    "entry preserves old contents"
                );
                let [(backedge, CancellationSuccessor::LoopBackedge { on_delivery })] =
                    (0..body.blocks.len())
                        .filter_map(|i| {
                            callee_proof
                                .successor(i)
                                .map(|edge| (&body.blocks[i], edge))
                        })
                        .collect::<Vec<_>>()[..]
                else {
                    panic!("expected the feasible callee backedge");
                };
                let loop_site: &[u8] = if name == "good" {
                    b"while ((((((((enter)))))))) { continue; }"
                } else {
                    b"while (enter) { continue; }"
                };
                assert_eq!(backedge.site.span.bytes(), loop_site);
                let retained = on_delivery.retained.as_ref().unwrap();
                assert_place(retained.parameter, parameters[0], module);
                let repair = retained.last_store.unwrap();
                assert_eq!(repair.span.bytes(), b"*slot = input;");
                assert!(repair.span.start() < backedge.site.span.start());
                let [
                    MirCleanup::EndStorage(flag),
                    locals @ ..,
                    param_2,
                    param_1,
                    param_0,
                ] = on_delivery.cleanup.as_slice()
                else {
                    panic!("expected callee owners and parameters");
                };
                assert_eq!(flag.root.span.bytes(), b"flag");
                assert_parameter_ends(&[*param_2, *param_1, *param_0], parameters, module);

                let backing = call
                    .possible_backing
                    .iter()
                    .map(|place| place.root.span.bytes())
                    .collect::<std::collections::BTreeSet<_>>();
                assert_eq!(backing, [b"prior".as_slice(), b"input"].into());
                let cleanup: Vec<_> = call
                    .caller_cleanup
                    .iter()
                    .map(|action| match action {
                        MirCleanup::EndStorage(place) => ("end", place.root.span.bytes()),
                        MirCleanup::Drop(place) => ("drop", place.root.span.bytes()),
                        other => panic!("unexpected caller delivery cleanup: {other:?}"),
                    })
                    .collect();
                let Terminator::Goto(next) = block.terminator else {
                    panic!("missing ordinary call continuation");
                };
                let normal = straight_blocks(mir, next);
                assert!(matches!(
                    normal.last().unwrap().terminator,
                    Terminator::Return
                ));
                if name == "goodWithOwner" {
                    assert!(
                        matches!(locals, [MirCleanup::Drop(place)] if place.root.span.bytes() == b"calleeText")
                    );
                    // Both possible backing owners were declared after slot; without
                    // the retained dependencies they would retire before the slot.
                    assert_eq!(
                        cleanup,
                        [
                            ("end", b"slot".as_slice()),
                            ("end", b"input"),
                            ("end", b"prior"),
                            ("end", b"initial"),
                            ("drop", b"callerText"),
                            ("end", b"enter: bool")
                        ]
                    );
                    assert!(
                        normal
                            .iter()
                            .any(|block| matches!(block.operation, Operation::Export(_)))
                    );
                    assert!(
                        matches!(path_drops(mir, next).as_slice(), [Obligation::Drop(suffix), Obligation::Drop(text)]
                        if suffix.root.span.bytes() == b"callerSuffix" && text.root.span.bytes() == b"callerText")
                    );
                } else {
                    assert!(locals.is_empty());
                    assert_eq!(
                        cleanup,
                        [
                            ("end", b"slot".as_slice()),
                            ("end", b"input"),
                            ("end", b"prior"),
                            ("end", b"enter: bool")
                        ]
                    );
                    let start = straight_blocks(body, body.entry);
                    let Terminator::Branch { no, .. } = start.last().unwrap().terminator else {
                        panic!("enter=false must be able to skip the loop");
                    };
                    assert!(matches!(
                        straight_blocks(body, no).last().unwrap().terminator,
                        Terminator::Return
                    ));
                    let used = good.statistics();
                    let exact = AnalysisLimits {
                        max_work: used.work,
                        max_entries: used.entries,
                        max_depth: used.max_depth,
                    };
                    let at = |limits| {
                        lower_body(receipt, module, &declaration(module, name).id, limits).unwrap()
                    };
                    assert!(at(exact).mir().unwrap().cancellation().is_some());
                    for short in [
                        AnalysisLimits {
                            max_work: used.work - 1,
                            ..exact
                        },
                        AnalysisLimits {
                            max_entries: used.entries - 1,
                            ..exact
                        },
                        AnalysisLimits {
                            max_depth: used.max_depth - 1,
                            ..exact
                        },
                    ] {
                        let report = at(short);
                        assert_eq!(report.completion(), Completion::ResourceLimit);
                        assert!(report.mir().is_none());
                    }
                }
            }
        },
    );
}

#[test]
fn installed_only_reached_budget_checks_contribute_delivery_edges() {
    probe_with_typed(
        "cancellation::installed_only_reached_budget_checks_contribute_delivery_edges",
        r#"
function leaf(): void {}
function receives(value: bool): void {}

function repairs(slot: &mut &bool, input: &bool, enter: bool): bool {
    let flag = true;
    *slot = &flag;
    while (enter) { break; }
    if (false) { leaf(); }
    *slot = input;
    return flag;
}

function outsideSubset(value: bool): void {
    receives(value);
}
        "#,
        |request, module, receipt, _| {
            for (name, covered) in [("repairs", true), ("outsideSubset", false)] {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|decl| decl.scope.is_none() && decl.name.as_deref() == Some(name))
                    .unwrap();
                let report =
                    lower_body(receipt, module, &declaration.id, AnalysisLimits::default())
                        .unwrap();
                assert_eq!(
                    report.completion(),
                    Completion::Checked,
                    "{name}, test_mode={}: {report:?}",
                    request.test_mode
                );
                if !covered {
                    // Exact ordinary stats from 0fbf765 for this unchanged capture.
                    let exact = AnalysisLimits {
                        max_work: 4022,
                        max_entries: 197,
                        ..AnalysisLimits::default()
                    };
                    let at_limit = lower_body(receipt, module, &declaration.id, exact).unwrap();
                    assert_eq!(
                        at_limit.completion(),
                        Completion::Checked,
                        "ordinary budget consumed by optional proof: {:?}",
                        at_limit.statistics()
                    );
                    assert!(at_limit.mir().unwrap().cancellation().is_none());
                    for limit in [
                        AnalysisLimits {
                            max_work: 4021,
                            ..exact
                        },
                        AnalysisLimits {
                            max_entries: 196,
                            ..exact
                        },
                    ] {
                        let short = lower_body(receipt, module, &declaration.id, limit).unwrap();
                        assert_eq!(short.completion(), Completion::ResourceLimit);
                        assert!(short.mir().is_none());
                    }
                }
                let mir = report.mir().unwrap();
                assert_eq!(mir.cancellation().is_some(), covered);
                if let Some(proof) = mir.cancellation() {
                    assert!((0..mir.blocks.len()).all(|index| proof.successor(index).is_none()));
                    assert!(
                        !mir.blocks
                            .iter()
                            .any(|block| matches!(block.operation, Operation::Call(_)))
                    );
                }
            }
        },
    );
}

#[test]
fn installed_unknown_paths_withhold_receipts_but_preserve_independent_witnesses() {
    probe_with_typed(
        "cancellation::installed_unknown_paths_withhold_receipts_but_preserve_independent_witnesses",
        r#"
function leaf(): void {}
function nonempty(): void { leaf(); }

function mixedState(enter: bool, moveIt: bool): void {
    leaf();
    if (enter) {
        let nested = "owned";
        if (moveIt) { let moved = nested; }
        leaf();
    }
}

function badCall(slot: &mut &bool, input: &bool): bool {
    let flag = true;
    *slot = &flag;
    leaf();
    *slot = input;
    nonempty();
    return flag;
}

function badLoop(slot: &mut &bool, input: &bool, takeLoop: bool, enter: bool): bool {
    let flag = true;
    *slot = &flag;
    if (takeLoop) { while (enter) { continue; } }
    else { nonempty(); }
    *slot = input;
    return flag;
}

function unsafeBody(slot: &mut &bool, input: &bool, enter: bool): void {
    let flag = true;
    *slot = &flag;
    while (enter) { continue; }
    *slot = input;
}

function caller(enter: bool): void {
    let flag = true;
    let input = false;
    let mut slot = &flag;
    leaf();
    unsafeBody(&mut slot, &input, enter);
}
        "#,
        |request, module, receipt, _| {
            for name in ["mixedState", "caller"] {
                let report = query(receipt, module, name);
                assert_eq!(
                    report.completion(),
                    Completion::Checked,
                    "{name}, mode={}",
                    request.test_mode
                );
                assert!(
                    report.mir().unwrap().cancellation().is_none(),
                    "partial proof for {name}"
                );
            }
            for (name, checkpoint) in [
                ("badCall", b"leaf()".as_slice()),
                ("badLoop", b"while (enter) { continue; }".as_slice()),
                ("unsafeBody", b"while (enter) { continue; }".as_slice()),
            ] {
                let report = query(receipt, module, name);
                assert_eq!(
                    report.completion(),
                    Completion::SourceRejected,
                    "{name}, mode={}",
                    request.test_mode
                );
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("expected one cancellation escape for {name}");
                };
                assert_eq!(finding.code(), "R0202");
                assert_eq!(
                    finding.cancellation_delivery.unwrap().span.bytes(),
                    checkpoint
                );
            }
        },
    );
}

#[test]
fn aggregate_outside_cancellation_keeps_ordinary_budget() {
    probe_with_typed(
        "cancellation::aggregate_outside_cancellation_keeps_ordinary_budget",
        "function tupleOnly(pair: [bool, bool]): void {}\n",
        |_, module, receipt, _| {
            // Both are the measured ordinary limits for this capture at 0fbf765.
            let limits = AnalysisLimits {
                max_work: 2273,
                max_entries: 108,
                ..Default::default()
            };
            let report = lower_body(
                receipt,
                module,
                &declaration(module, "tupleOnly").id,
                limits,
            )
            .unwrap();
            assert_eq!(report.completion(), Completion::Checked);
            assert!(report.mir().unwrap().cancellation().is_none());
        },
    );
}

#[test]
fn empty_entry_proof_costs_an_entry() {
    probe_with_typed(
        "cancellation::empty_entry_proof_costs_an_entry",
        "function leaf(): void {}\n",
        |_, module, receipt, _| {
            let report = query(receipt, module, "leaf");
            assert!(report.mir().unwrap().cancellation().is_some());
            // Without the entry proof the ordinary result occupied nine entries.
            let limits = AnalysisLimits {
                max_entries: 9,
                ..Default::default()
            };
            let short =
                lower_body(receipt, module, &declaration(module, "leaf").id, limits).unwrap();
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(short.mir().is_none());
        },
    );
}

#[test]
fn independent_conditions_reconverge_before_callee_entry() {
    probe_with_typed(
        "cancellation::independent_conditions_reconverge_before_callee_entry",
        r#"function leaf(): void {}
function diamonds8(b0: bool, b1: bool, b2: bool, b3: bool, b4: bool, b5: bool, b6: bool, b7: bool): void {
    if (b0) {}
    if (b1) {}
    if (b2) {}
    if (b3) {}
    if (b4) {}
    if (b5) {}
    if (b6) {}
    if (b7) {}
    leaf();
}
"#,
        |_, module, receipt, _| {
            let limits = AnalysisLimits {
                max_entries: 2000,
                ..Default::default()
            };
            let report = lower_body(
                receipt,
                module,
                &declaration(module, "diamonds8").id,
                limits,
            )
            .unwrap();
            assert_eq!(report.completion(), Completion::Checked);
            let mir = report.mir().unwrap();
            let proof = mir.cancellation().unwrap();
            let entries: Vec<_> = (0..mir.blocks.len())
                .filter_map(|i| proof.successor(i))
                .collect();
            assert!(
                matches!(entries.as_slice(), [CancellationSuccessor::CalleeEntry { callee, .. }] if **callee == declaration(module, "leaf").id)
            );
        },
    );
}

#[path = "cancellation/replacement.rs"]
mod replacement;

fn declaration<'g>(module: &'g loader::Module, name: &str) -> &'g loader::Declaration {
    module
        .declarations
        .iter()
        .find(|decl| decl.scope.is_none() && decl.name.as_deref() == Some(name))
        .unwrap()
}

fn query<'g>(
    receipt: &TypedReceipt<'_, 'g>,
    module: &'g loader::Module,
    name: &str,
) -> compiler::analysis::ownership::Report<'g> {
    lower_body(
        receipt,
        module,
        &declaration(module, name).id,
        AnalysisLimits::default(),
    )
    .unwrap()
}

fn check_pair(test: &str, source: &str, checkpoint: &[u8]) {
    probe_with_typed(test, source, |request, module, receipt, typed| {
        let bodies = typed.bodies();
        let nodes = &module.syntax.nodes;
        let fact = |node| bodies.node(module, node).unwrap().unwrap();
        let expr = |node| {
            let NodeFactKind::Expression(facts) = &fact(node).kind else {
                panic!(
                    "expected typed expression at {:?}",
                    nodes[node].span.bytes()
                );
            };
            facts
        };
        for name in ["repairBefore", "repairAfter"] {
            let decl = declaration(module, name);
            let signature = nodes[decl.node].children[0];
            let parameter_list = nodes[signature]
                .children
                .iter()
                .find(|&&node| nodes[node].kind == NodeKind::ValueParameters)
                .unwrap();
            let parameters = &nodes[*parameter_list].children;
            let body = bodies
                .body(module, BodyOwner::Declaration(&decl.id))
                .unwrap()
                .unwrap();
            assert!(body.flow.returns && !body.flow.pending);
            let &[_, _, _, _, store, first, second, suffix] =
                nodes[body.site.node].children.as_slice()
            else {
                panic!("expected stores, checkpoint and return in {name}");
            };
            let (repair, site) = if name == "repairBefore" {
                (first, second)
            } else {
                (second, first)
            };
            assert_eq!(nodes[site].span.bytes(), checkpoint);
            assert_eq!(nodes[store].span.bytes(), b"*slot = &flag;");
            let borrow = nodes[store].children[1];
            assert_eq!(nodes[borrow].span.bytes(), b"&flag");
            let call_target = match nodes[site].kind {
                NodeKind::While => {
                    let [condition, loop_body] = nodes[site].children[..] else {
                        panic!("expected loop condition and body");
                    };
                    // A concrete bad input is enter=true: the direct immutable
                    // parameter read enters the loop, and the continue is unconditional.
                    // The normal zero-iteration path instead uses enter=false.
                    let condition_facts = expr(condition);
                    assert_eq!(
                        condition_facts.place,
                        compiler::analysis::scalars::Place::ReadOnly
                    );
                    let path = condition_facts.path.as_ref().unwrap();
                    let enter = *parameters.last().unwrap();
                    assert_eq!(nodes[enter].span.bytes(), b"enter: bool");
                    assert_eq!(path.root.start, nodes[enter].span.start());
                    assert!(path.projections.is_empty());
                    assert_eq!(condition_facts.binding, Some(path.root));
                    let outcomes = condition_facts.boolean.unwrap();
                    assert!(outcomes.can_be_true() && outcomes.can_be_false());
                    let [backedge] = nodes[loop_body].children[..] else {
                        panic!("expected one continue");
                    };
                    assert_eq!(nodes[backedge].span.bytes(), b"continue;");
                    assert!(
                        matches!(fact(backedge).kind, NodeFactKind::Continue { target } if target == site)
                    );
                    None
                }
                NodeKind::ExpressionStatement => {
                    let [call] = nodes[site].children[..] else {
                        panic!("expected one call");
                    };
                    assert_eq!(nodes[call].span.bytes(), b"observe()");
                    let callee = declaration(module, "observe");
                    assert_eq!(expr(call).call.as_ref().unwrap().callee, &callee.id);
                    let entry = bodies
                        .body(module, BodyOwner::Declaration(&callee.id))
                        .unwrap()
                        .unwrap();
                    assert_eq!(entry.site.span.bytes(), b"{}");
                    Some((call, callee, entry.site))
                }
                other => panic!("unexpected checkpoint source: {other:?}"),
            };

            let report = lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap();
            let mode = request.test_mode;
            if name == "repairBefore" {
                assert_eq!(
                    report.completion(),
                    Completion::Checked,
                    "test_mode={mode}: {report:?}"
                );
                assert!(report.blocked_at().is_none() && report.findings().is_empty());
                let mir = report.mir().unwrap();
                let proof = mir.cancellation().expect("known flat cancellation proof");
                let used = report.statistics();
                let exact = AnalysisLimits {
                    max_work: used.work,
                    max_entries: used.entries,
                    max_depth: used.max_depth,
                };
                let at_limit = lower_body(receipt, module, &decl.id, exact).unwrap();
                assert!(at_limit.mir().unwrap().cancellation().is_some());
                for limit in [
                    AnalysisLimits {
                        max_work: used.work - 1,
                        ..exact
                    },
                    AnalysisLimits {
                        max_entries: used.entries - 1,
                        ..exact
                    },
                ] {
                    let short = lower_body(receipt, module, &decl.id, limit).unwrap();
                    assert_eq!(short.completion(), Completion::ResourceLimit);
                    assert!(short.mir().is_none());
                }
                assert_eq!(proof.entry_site.node, body.site.node);
                let entry_exit = &proof.on_entry_delivery;
                let retained = entry_exit.retained.as_ref().unwrap();
                assert!(retained.last_store.is_none());
                assert_place(retained.parameter, parameters[0], module);
                assert_parameter_ends(&entry_exit.cleanup, parameters, module);
                let [(block, edge)] = mir
                    .blocks
                    .iter()
                    .enumerate()
                    .filter_map(|(index, block)| proof.successor(index).map(|edge| (block, edge)))
                    .collect::<Vec<_>>()[..]
                else {
                    panic!("expected one reached delivery edge")
                };
                let Terminator::Goto(next) = block.terminator else {
                    panic!("ordinary continuation lost")
                };
                let (exit, normal_suffix) = match (edge, call_target) {
                    (CancellationSuccessor::LoopBackedge { on_delivery }, None) => {
                        assert_eq!(block.site.node, site);
                        assert_eq!(mir.blocks[next].site.node, nodes[site].children[0]);
                        let next_test = straight_blocks(mir, next);
                        let retest = next_test.last().unwrap();
                        assert_eq!(retest.site.node, nodes[site].children[0]);
                        assert!(matches!(retest.terminator, Terminator::Branch { .. }));

                        let initial_path = straight_blocks(mir, mir.entry);
                        let first_test = initial_path.last().unwrap();
                        assert_eq!(first_test.site.node, retest.site.node);
                        let Terminator::Branch { yes, no } = first_test.terminator else {
                            panic!("the initial test must permit zero iterations");
                        };
                        assert!(
                            straight_blocks(mir, yes)
                                .iter()
                                .any(|item| std::ptr::eq(*item, block))
                        );
                        (on_delivery, no)
                    }
                    (
                        CancellationSuccessor::CalleeEntry {
                            callee,
                            entry,
                            on_delivery,
                        },
                        Some((call, target, expected)),
                    ) => {
                        assert_eq!(block.site.node, call);
                        assert!(
                            matches!(&block.operation, Operation::Call(actual) if actual == callee)
                        );
                        assert_eq!(*callee, &target.id);
                        assert_eq!(
                            (entry.module, entry.node, entry.span),
                            (expected.module, expected.node, expected.span)
                        );
                        let body =
                            lower_body(receipt, module, callee, AnalysisLimits::default()).unwrap();
                        let proof = body.mir().unwrap().cancellation().unwrap();
                        assert_eq!(proof.entry_site.node, entry.node);
                        assert!(
                            proof.on_entry_delivery.cleanup.is_empty()
                                && proof.on_entry_delivery.retained.is_none()
                        );
                        (on_delivery, next)
                    }
                    _ => panic!("wrong delivery origin"),
                };
                assert_repaired_exit(exit, repair, parameters, module);
                assert!(straight_blocks(mir, mir.entry).iter().any(|block| {
                    block.site.node == repair
                        && matches!(block.operation, Operation::InstallThrough { .. })
                }));
                assert!(straight_blocks(mir, normal_suffix).iter().any(|block| {
                    block.site.node == suffix && matches!(block.operation, Operation::Export(_))
                }));
                assert!(
                    matches!(path_drops(mir, normal_suffix).as_slice(), [Obligation::Drop(place)] if place.root.span.bytes() == b"text")
                );
                continue;
            }

            assert!(
                report.mir().is_none(),
                "unrepaired store published MIR, test_mode={mode}"
            );
            assert_eq!(
                report.completion(),
                Completion::SourceRejected,
                "test_mode={mode}: {report:?}"
            );
            assert!(report.blocked_at().is_none());
            let [finding] = report.findings() else {
                panic!("test_mode={mode}: expected one retained-storage finding, got {report:?}");
            };
            assert_eq!(finding.code(), "R0202");
            let delivery = finding.cancellation_delivery.unwrap();
            let delivery_node = call_target.map_or(site, |(call, _, _)| call);
            assert_eq!(delivery.module, &module.key);
            assert_eq!(delivery.node, delivery_node);
            assert_eq!(delivery.span, &nodes[delivery_node].span);
            for (actual, expected) in [(&finding.site, store), (&finding.related, borrow)] {
                assert_eq!(actual.module, &module.key);
                assert_eq!(actual.node, expected);
                assert_eq!(actual.span, &nodes[expected].span);
            }
        }
    });
}

fn assert_repaired_exit(
    exit: &CancellationExit<'_>,
    repair: NodeId,
    params: &[NodeId],
    module: &loader::Module,
) {
    let retained = exit.retained.as_ref().unwrap();
    assert_place(retained.parameter, params[0], module);
    let written = retained.last_store.unwrap();
    assert_eq!(written.module, &module.key);
    assert_eq!(written.node, repair);
    assert_eq!(written.span.bytes(), b"*slot = input;");
    let [
        MirCleanup::EndStorage(holder),
        MirCleanup::EndStorage(flag),
        MirCleanup::Drop(text),
        parameters @ ..,
    ] = exit.cleanup.as_slice()
    else {
        panic!("expected dependency-first flat cleanup")
    };
    assert_eq!(holder.root.span.bytes(), b"holder");
    assert_eq!(flag.root.span.bytes(), b"flag");
    assert_eq!(text.root.span.bytes(), b"text");
    assert!(holder.root.span.start() < flag.root.span.start());
    assert_parameter_ends(parameters, params, module);
}

fn assert_parameter_ends(cleanup: &[MirCleanup<'_>], params: &[NodeId], module: &loader::Module) {
    // Each frame parameter ends once. The outward contents are not a
    // parameter-owned cleanup operand, even when they were overwritten.
    assert_eq!(cleanup.len(), params.len());
    for (action, &param) in cleanup.iter().zip(params.iter().rev()) {
        let MirCleanup::EndStorage(place) = action else {
            panic!("expected parameter descriptor storage, got {action:?}");
        };
        assert_place(*place, param, module);
    }
}

fn assert_place(
    place: compiler::analysis::ownership::Place<'_>,
    node: NodeId,
    module: &loader::Module,
) {
    assert_eq!(place.root.module, &module.key);
    assert_eq!(place.root.node, node);
    assert_eq!(place.root.span, &module.syntax.nodes[node].span);
    assert!(place.projection.is_none());
}
