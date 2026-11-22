use super::*;
use compiler::analysis::ownership::CancellationSuccessor;

// The positive ri-33m cases also run alone so resource exhaustion cannot be
// mistaken for withholding MIR because of a known rejection.
const SAFE: &str = r#"
function safeBody(slot: &mut &bool, input: &bool, enter: bool): void {
    let flag = true;
    *slot = &flag;
    *slot = input;
    while ((((((((enter)))))))) { continue; }
}
function good(enter: bool): void {
    let prior = false;
    let input = true;
    let mut slot = &prior;
    safeBody(&mut slot, &input, enter);
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
"#;

const OTHER: &str = r#"
function unsafeBody(slot: &mut &bool, input: &bool, enter: bool): void {
    let flag = true;
    *slot = &flag;
    while (enter) { continue; }
    *slot = input;
}
function bad(enter: bool): void {
    let prior = false;
    let input = true;
    let mut slot = &prior;
    unsafeBody(&mut slot, &input, enter);
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
function nonempty(): void { leaf(); }
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
function repairInElse(slot: &mut &bool, input: &bool, enter: bool): void {
    let flag = true;
    *slot = &flag;
    if (enter) {} else { *slot = input; }
    while (enter) { continue; }
    *slot = input;
}
function breakOnly(slot: &mut &bool, input: &bool, enter: bool): void {
    let flag = true;
    *slot = &flag;
    while (enter) { break; }
    if (false) { leaf(); }
    *slot = input;
}
"#;

#[test]
fn installed_selected_delivery_and_unresolved_contexts() {
    probe_with_typed(
        "graph::delivery::installed_selected_delivery_and_unresolved_contexts",
        &format!("{SAFE}{OTHER}"),
        |request, module, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.completion(), GraphCompletion::SourceRejected);
            assert_eq!(report.findings().len(), 5);
            let named = |name| root(&report, format!("function {name}(").as_bytes());
            for (id, (name, delivery)) in [
                ("unsafeBody", "while (enter) { continue; }"),
                ("unsafeWithNested", "while (enter) { continue; }"),
                ("badCall", "leaf()"),
                ("badLoop", "while (enter) { continue; }"),
                ("repairInElse", "while (enter) { continue; }"),
            ]
            .into_iter()
            .enumerate()
            {
                let body = named(name);
                assert_eq!(body.ordinary_completion(), Completion::Checked);
                assert!(body.ordinary_mir().is_some() && body.mir().is_none());
                assert!(
                    matches!(body.enabled_frame(), Delivery::Rejected { finding_id, via_call: None } if *finding_id == id)
                );
                let finding = &report.findings()[id];
                let actual = finding.cancellation_delivery.unwrap();
                assert_eq!(finding.code(), "R0202");
                assert_eq!(finding.site.span.bytes(), b"*slot = &flag;");
                assert_eq!(finding.related.span.bytes(), b"&flag");
                assert_eq!(actual.span.bytes(), delivery.as_bytes());
                let declaration = body.site().span;
                assert!(
                    declaration.start() <= finding.site.span.start()
                        && actual.span.end() <= declaration.end()
                );
            }
            let bad = named("bad");
            assert!(matches!(
                bad.selected_call(),
                Delivery::Rejected {
                    finding_id: 0,
                    via_call: Some(_)
                }
            ));
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert_eq!(bad.ordinary_completion(), Completion::Checked);
            assert!(bad.mir().is_none() && bad.ordinary_mir().is_some());
            for name in [
                "nestedCaller",
                "maskedCaller",
                "constantCaller",
                "guardedCaller",
                "callerOfNestedBody",
            ] {
                let caller = named(name);
                assert_eq!(caller.completion(), Completion::Checked, "{name}");
                assert!(caller.mir().is_some());
                assert!(
                    matches!(caller.enabled_frame(), Delivery::Unresolved),
                    "{name}"
                );
                assert!(
                    matches!(caller.selected_call(), Delivery::Unresolved),
                    "{name}"
                );
            }
            let implementation = root(&report, b"implement release(");
            assert_eq!(implementation.completion(), Completion::Incomplete);
            let [active] = report.active_disposers() else {
                panic!("expected one active disposer")
            };
            assert_eq!(active.owner(), implementation.owner());
            assert_eq!(active.completion(), Completion::Checked);
            for nonfun in [implementation, active, &report.roots()[0]] {
                assert!(matches!(nonfun.enabled_frame(), Delivery::Unresolved));
                assert!(matches!(nonfun.selected_call(), Delivery::Unresolved));
            }
            let breaks = named("breakOnly");
            let Delivery::Proved(proof) = breaks.enabled_frame() else {
                panic!("expected entry-only proof")
            };
            assert!(
                (0..breaks.mir().unwrap().blocks.len())
                    .all(|block| proof.successor(block).is_none())
            );
            assert_good_proofs(&report, module);
            check_limits(request.test_mode, receipt, &report);
        },
    );
}

#[test]
fn independent_delivery_rejection_survives_an_unresolved_ordinary_edge() {
    probe_with_typed(
        "graph::delivery::independent_delivery_rejection_survives_an_unresolved_ordinary_edge",
        r#"
function leaf(): void {}
type PendingText = { text: string; }
function pending(): void { let value = PendingText { text: "owned" }; dispose(value.text); }
function closed(): void { let text = "owned"; dispose(text); }
function before(slot: &mut &bool, input: &bool): void {
    let flag = true;
    *slot = &flag;
    leaf();
    *slot = input;
    pending();
}
function beyond(slot: &mut &bool, input: &bool): void {
    let flag = true;
    *slot = &flag;
    pending();
    leaf();
    *slot = input;
}
"#,
        |_, module, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.completion(), GraphCompletion::SourceRejected);
            let [finding] = report.findings() else {
                panic!("expected only the independent exit")
            };
            let early = root(&report, b"function before(");
            let late = root(&report, b"function beyond(");
            assert!(root(&report, b"function closed(").ordinary_mir().is_some());
            for body in [early, late] {
                assert_eq!(body.ordinary_completion(), Completion::Incomplete);
                assert_eq!(
                    body.blocked_at().unwrap().span.bytes(),
                    b"dispose(value.text)"
                );
                assert!(body.mir().is_none() && body.ordinary_mir().is_none());
                assert!(matches!(body.selected_call(), Delivery::Unresolved));
            }
            assert_eq!(early.completion(), Completion::SourceRejected);
            assert!(matches!(
                early.enabled_frame(),
                Delivery::Rejected {
                    finding_id: 0,
                    via_call: None
                }
            ));
            assert_eq!(late.completion(), Completion::Incomplete);
            assert!(matches!(late.enabled_frame(), Delivery::Unresolved));
            assert_eq!(finding.code(), "R0202");
            let edge = finding.cancellation_delivery.unwrap();
            for (site, spelling) in [
                (finding.site, b"*slot = &flag;".as_slice()),
                (finding.related, b"&flag"),
                (edge, b"leaf()"),
            ] {
                assert_eq!(site.module, &module.key);
                assert_eq!(site.span.bytes(), spelling);
                assert!(
                    early.site().span.start() <= site.span.start()
                        && site.span.end() <= early.site().span.end()
                );
            }
        },
    );
}

#[test]
fn good_only_graph_needs_the_complete_work_entry_and_depth_budget() {
    probe_with_typed(
        "graph::delivery::good_only_graph_needs_the_complete_work_entry_and_depth_budget",
        SAFE,
        |request, module, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
            assert!(report.findings().is_empty());
            assert_good_proofs(&report, module);
            check_limits(request.test_mode, receipt, &report);
        },
    );
}

fn assert_good_proofs(report: &GraphReport<'_>, module: &loader::Module) {
    let nodes = &module.syntax.nodes;
    for (name, callee_name, with_owner) in [
        ("good", "safeBody", false),
        ("goodWithOwner", "safeWithOwner", true),
    ] {
        let caller = root(report, format!("function {name}(").as_bytes());
        assert_eq!(caller.completion(), Completion::Checked);
        assert!(matches!(caller.enabled_frame(), Delivery::Unresolved));
        let Delivery::Proved(proof) = caller.selected_call() else {
            panic!("missing selected proof for {name}")
        };
        let ordinary = caller.mir().unwrap();
        assert!(ordinary.cancellation().is_none());
        assert_eq!(
            proof.caller_entry_site.node,
            nodes[caller.site().node].children[1]
        );
        let block = &ordinary.blocks[proof.call.block];
        let Operation::CallRetainingWithBool {
            callee,
            arguments,
            destination,
            ..
        } = &block.operation
        else {
            panic!("proof did not select the ordinary retaining instruction");
        };
        assert_eq!(proof.call.site.node, block.site.node);
        assert!(
            matches!(destination, compiler::analysis::ownership::RetainedPlace::Local(slot) if slot.root.node == proof.suspended.destination.root.node)
        );
        let target = &report.roots()[proof.call.callee_root];
        assert_eq!(target.owner(), BodyOwner::Declaration(callee));
        let declaration = module
            .declarations
            .iter()
            .find(|d| d.name.as_deref() == Some(callee_name))
            .unwrap();
        assert_eq!(*callee, &declaration.id);
        assert_eq!(
            cleanup_names(&proof.on_caller_entry_delivery.cleanup),
            [(false, b"enter: bool".as_slice())]
        );

        let signature = nodes[declaration.node].children[0];
        let formals = nodes[signature]
            .children
            .iter()
            .find(|&&node| nodes[node].kind == compiler::parser::NodeKind::ValueParameters)
            .unwrap();
        for ((transfer, actual), &formal) in proof
            .suspended
            .arguments
            .iter()
            .zip(arguments)
            .zip(&nodes[*formals].children)
        {
            assert_eq!(
                (transfer.staged.root.module, transfer.staged.root.node),
                (&module.key, actual.root.node)
            );
            assert_eq!(
                (transfer.parameter.root.module, transfer.parameter.root.node),
                (&module.key, formal)
            );
        }
        let Delivery::Proved(enabled) = target.enabled_frame() else {
            panic!("missing callee's direct proof")
        };
        assert_eq!(enabled.entry_site.node, nodes[declaration.node].children[1]);
        let params = [
            (false, b"enter: bool".as_slice()),
            (false, b"input: &bool"),
            (false, b"slot: &mut &bool"),
        ];
        assert_eq!(cleanup_names(&enabled.on_entry_delivery.cleanup), params);
        let retained = enabled.on_entry_delivery.retained.as_ref().unwrap();
        assert_eq!(retained.parameter.root.node, nodes[*formals].children[0]);
        assert!(retained.last_store.is_none());
        let body = target.ordinary_mir().unwrap();
        let [(backedge, CancellationSuccessor::LoopBackedge { on_delivery })] =
            (0..body.blocks.len())
                .filter_map(|block| {
                    enabled
                        .successor(block)
                        .map(|proof| (&body.blocks[block], proof))
                })
                .collect::<Vec<_>>()[..]
        else {
            panic!("expected one feasible callee backedge");
        };
        let retained = on_delivery.retained.as_ref().unwrap();
        assert_eq!(retained.parameter.root.node, nodes[*formals].children[0]);
        let repair = retained.last_store.unwrap();
        assert_eq!(repair.span.bytes(), b"*slot = input;");
        assert!(repair.span.start() < backedge.site.span.start());
        let mut callee_cleanup = vec![(false, b"flag".as_slice())];
        let mut caller_cleanup = vec![
            (false, b"slot".as_slice()),
            (false, b"input"),
            (false, b"prior"),
        ];
        if with_owner {
            callee_cleanup.push((true, b"calleeText"));
            caller_cleanup.extend([(false, b"initial".as_slice()), (true, b"callerText")]);
        }
        callee_cleanup.extend(params);
        caller_cleanup.push((false, b"enter: bool"));
        assert_eq!(cleanup_names(&on_delivery.cleanup), callee_cleanup);
        assert_eq!(
            cleanup_names(&proof.suspended.caller_cleanup),
            caller_cleanup
        );
        assert_eq!(
            proof
                .suspended
                .possible_backing
                .iter()
                .map(|place| place.root.span.bytes())
                .collect::<std::collections::BTreeSet<_>>(),
            [b"prior".as_slice(), b"input"].into()
        );
        let Terminator::Goto(next) = block.terminator else {
            panic!("missing normal continuation")
        };
        let normal = straight_blocks(ordinary, next);
        assert_eq!(normal.last().unwrap().terminator, Terminator::Return);
        if with_owner {
            assert!(
                normal
                    .iter()
                    .any(|block| matches!(block.operation, Operation::Export(_)))
            );
            assert_eq!(
                cleanup_names(&path_drops(ordinary, next)),
                [(true, b"callerSuffix".as_slice()), (true, b"callerText")]
            );
        }
    }
}

fn cleanup_names<'g>(cleanup: &[Obligation<'g>]) -> Vec<(bool, &'g [u8])> {
    cleanup
        .iter()
        .map(|action| match action {
            Obligation::EndStorage(place) => (false, place.root.span.bytes()),
            Obligation::Drop(place) => (true, place.root.span.bytes()),
            other => panic!("unexpected bounded cleanup: {other:?}"),
        })
        .collect()
}

fn check_limits(test_mode: bool, receipt: &TypedReceipt<'_, '_>, report: &GraphReport<'_>) {
    let stats = report.statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    eprintln!(
        "graph {:?}, test_mode={test_mode}: {stats:?}",
        report.completion()
    );
    let at = analyze_ownership(receipt, exact);
    assert_eq!(at.completion(), report.completion());
    assert_eq!(at.statistics(), stats);
    assert_eq!(at.findings().len(), report.findings().len());
    assert!(matches!(
        root(&at, b"function goodWithOwner(").selected_call(),
        Delivery::Proved(_)
    ));
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
        let exhausted = analyze_ownership(receipt, short);
        assert_eq!(
            exhausted.completion(),
            GraphCompletion::ResourceLimit,
            "{short:?}"
        );
        assert!(exhausted.inventory_complete());
        assert!(
            exhausted
                .roots()
                .iter()
                .chain(exhausted.active_disposers())
                .all(|root| {
                    root.mir().is_none()
                        && root.ordinary_mir().is_none()
                        && !matches!(root.enabled_frame(), Delivery::Proved(_))
                        && !matches!(root.selected_call(), Delivery::Proved(_))
                })
        );
        if short.max_depth == exact.max_depth {
            assert_eq!(
                exhausted.findings().len(),
                report.findings().len(),
                "late exhaustion lost a finding"
            );
            if !report.findings().is_empty() {
                assert!(matches!(
                    root(&exhausted, b"function bad(").selected_call(),
                    Delivery::Rejected {
                        finding_id: 0,
                        via_call: Some(_)
                    }
                ));
            }
        } else {
            assert!(exhausted.findings().is_empty());
        }
    }
}
