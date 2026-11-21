use super::*;
use compiler::analysis::ownership::{CancellationSuccessor, lower_root};

// The positive #69 cases also run alone so resource exhaustion cannot be
// mistaken for withholding MIR because of a known rejection.
const SAFE: &str = r#"
function nestedGoodWithOwner(enter: bool): void {
    let outerText = "outer";
    goodWithOwner(enter);
    let outerSuffix = "only on normal return";
}
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
implement release(value: Owner): void { nestedCaller(value.enter); }
function maskedCaller(value: Owner): void { dispose(value); }
function constantOuter(enter: bool): void { bad(false); }
function guardedOuter(enter: bool): void { if (enter) {} else { bad(enter); } }
function alwaysOuter(enter: bool): void { if (true) { bad(enter); } }
function skippedOuter(enter: bool): void { if (false) { bad(enter); } }
function unknownOuter(enter: bool): void { let gate = enter; bad(gate); }
function recurse(enter: bool): void { recurse(enter); }
function deeper(enter: bool): void { nestedCaller(enter); }
function infeasibleLeaf(slot: &mut &bool, input: &bool, enter: bool): void {
    let flag = true;
    *slot = &flag;
    if (enter) {} else { while (enter) { continue; } }
    *slot = input;
}
function infeasibleMiddle(enter: bool): void {
    let prior = false;
    let input = true;
    let mut slot = &prior;
    infeasibleLeaf(&mut slot, &input, enter);
}
function infeasibleOuter(enter: bool): void { infeasibleMiddle(enter); }
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
function guardedMiddleOuter(enter: bool): void { guardedCaller(enter); }
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
            let Delivery::Rejected {
                finding_id: 0,
                via_call: Some(retaining),
            } = bad.selected_call()
            else {
                panic!("missing validated retaining rejection");
            };
            assert_eq!(
                retaining.site.span.bytes(),
                b"unsafeBody(&mut slot, &input, enter)"
            );
            assert_eq!(
                report.roots()[retaining.callee_root].owner(),
                named("unsafeBody").owner()
            );
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert_eq!(bad.ordinary_completion(), Completion::Checked);
            assert!(bad.mir().is_none() && bad.ordinary_mir().is_some());
            let nested = named("nestedCaller");
            assert_eq!(nested.completion(), Completion::SourceRejected);
            assert_eq!(nested.ordinary_completion(), Completion::Checked);
            assert!(nested.ordinary_mir().is_some() && nested.mir().is_none());
            assert!(matches!(nested.enabled_frame(), Delivery::Unresolved));
            assert!(matches!(nested.selected_call(), Delivery::Unresolved));
            let Delivery::Rejected {
                finding_id: 0,
                via_call: Some(forwarding),
            } = nested.forwarded_call()
            else {
                panic!("missing forwarding rejection");
            };
            let block = &nested.ordinary_mir().unwrap().blocks[forwarding.block];
            assert_eq!(forwarding.site.node, block.site.node);
            assert_eq!(forwarding.site.span.bytes(), b"bad(enter)");
            assert_eq!(report.roots()[forwarding.callee_root].owner(), bad.owner());
            assert!(
                matches!(block.operation, Operation::Call(callee) if bad.owner() == BodyOwner::Declaration(callee))
            );
            for name in [
                "maskedCaller",
                "constantOuter",
                "guardedOuter",
                "guardedMiddleOuter",
                "alwaysOuter",
                "unknownOuter",
                "recurse",
                "deeper",
                "infeasibleOuter",
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
                assert!(
                    matches!(caller.forwarded_call(), Delivery::Unresolved),
                    "{name}"
                );
            }
            let skipped = named("skippedOuter");
            assert!(skipped.dependencies().is_empty());
            assert!(matches!(skipped.forwarded_call(), Delivery::Unresolved));
            assert!(matches!(
                named("infeasibleMiddle").selected_call(),
                Delivery::Proved(_)
            ));
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
                assert!(matches!(nonfun.forwarded_call(), Delivery::Unresolved));
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
            assert_forwarded_proof(&report, module);
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
            assert_forwarded_proof(&report, module);
            check_limits(request.test_mode, receipt, &report);
        },
    );
}

#[test]
fn a_dead_binding_after_return_keeps_the_reached_forwarding_call() {
    let middle_and_leaf = OTHER.split("function nestedCaller").next().unwrap();
    let source = format!(
        "{middle_and_leaf}\nfunction outer(enter: bool): void {{ bad(enter); return; let _ = false; }}"
    );
    probe(
        "graph::delivery::a_dead_binding_after_return_keeps_the_reached_forwarding_call",
        &source,
        |_, module, receipt| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let outer = root(&report, b"function outer");
            let Delivery::Rejected {
                finding_id,
                via_call: Some(via),
            } = outer.forwarded_call()
            else {
                panic!("missing outer call witness")
            };
            let target =
                lower_root(receipt, module, outer.owner(), AnalysisLimits::default()).unwrap();
            assert!(outer.ordinary_mir().is_some() && outer.mir().is_none());
            assert_eq!(target.completion(), Completion::SourceRejected);
            assert_eq!(via.site.span.bytes(), b"bad(enter)");
            for found in [&report.findings()[*finding_id], &target.findings()[0]] {
                assert_eq!(found.code(), "R0202");
                assert_eq!(found.site.span.bytes(), b"*slot = &flag;");
                assert_eq!(found.related.span.bytes(), b"&flag");
            }
        },
    );
}

#[test]
fn unsupported_forwarders_keep_the_original_frame_budgets() {
    let retaining = r#"function outer(enter: bool): void { middle(ARG); }
function middle(enter: bool): void {
    let prior = false; let input = true; let mut slot = &prior;
    unsafeBody(&mut slot, &input, enter);
}
function unsafeBody(slot: &mut &bool, input: &bool, enter: bool): void {
    let flag = true; *slot = &flag; while (enter) { continue; } *slot = input;
}"#;
    for (source, target_cap, graph_cap, expected_graph) in [
        (retaining.replace("ARG", "false"), (7419,935,6), (190151,4842,6), GraphCompletion::SourceRejected),
        (retaining.replace("ARG", "((false))"), (7439,941,7), (190283,4844,7), GraphCompletion::SourceRejected),
        (retaining.replace("middle(ARG);", "if (true) { middle(enter); }"), (7499,975,7), (190399,4876,7), GraphCompletion::SourceRejected),
        ("function outer(enter: bool): void { middle(enter); }\nfunction middle(enter: bool): void { let _ = enter; }".to_owned(), (3954,162,5), (186093,4126,5), GraphCompletion::Incomplete),
    ] {
        probe("graph::delivery::unsupported_forwarders_keep_the_original_frame_budgets", &source, |_, module, receipt| {
            let cap = |(max_work,max_entries,max_depth)| AnalysisLimits {max_work,max_entries,max_depth};
            let graph = analyze_ownership(receipt, cap(graph_cap));
            assert_eq!(graph.completion(), expected_graph);
            let outer = root(&graph, b"function outer");
            assert!(matches!(outer.forwarded_call(), Delivery::Unresolved));
            let target = lower_root(receipt, module, outer.owner(), cap(target_cap)).unwrap();
            assert_eq!(target.completion(), Completion::Checked);
            assert!(target.mir().unwrap().cancellation().is_none());
            assert!(target.findings().is_empty());
        });
    }
}

#[test]
fn forwarded_call_uses_the_selected_module_not_the_function_spelling() {
    let unsafe_middle = format!("{}\n", OTHER.split("function nestedCaller").next().unwrap())
        .replace("function bad(", "export function bad(");
    probe_report_with_modules(
        "graph::delivery::forwarded_call_uses_the_selected_module_not_the_function_spelling",
        r#"
import left::bad as unsafeMiddle;
import right::{bad as otherMiddle, unavailable};
function rejected(enter: bool): void { unsafeMiddle(enter); }
function other(enter: bool): void { otherMiddle(enter); }
function blocked(enter: bool): void { unavailable(enter); }
"#,
        &[
            ("left.rill", &unsafe_middle),
            (
                "right.rill",
                r#"
export function bad(enter: bool): void { let text = "ordinary"; }
export function unavailable(enter: bool): void { let values = [13i32, 29i32]; }
"#,
            ),
        ],
        |_, module, typed| {
            let receipt = typed.receipt().unwrap();
            let report = analyze_ownership(&receipt, AnalysisLimits::default());
            let [finding] = report.findings() else {
                panic!("expected one canonical leaf finding");
            };
            assert_eq!(finding.site.module.path, "left.rill");
            let rejected = root(&report, b"function rejected(");
            let Delivery::Rejected {
                finding_id: 0,
                via_call: Some(call),
            } = rejected.forwarded_call()
            else {
                panic!("missing qualified rejection");
            };
            assert_eq!(call.site.span.bytes(), b"unsafeMiddle(enter)");
            assert_eq!(
                report.roots()[call.callee_root].site().module.path,
                "left.rill"
            );
            let target = lower_root(
                &receipt,
                module,
                rejected.owner(),
                AnalysisLimits::default(),
            )
            .unwrap();
            assert_eq!(target.completion(), Completion::SourceRejected);
            assert_eq!(target.findings()[0].site.module.path, "left.rill");
            let chain = target.rejected_forwarding_calls().unwrap();
            assert_eq!(
                chain.map(|site| site.module.path.as_str()),
                ["main.rill", "left.rill"]
            );

            let other = root(&report, b"function other(");
            assert!(matches!(other.forwarded_call(), Delivery::Unresolved));
            let [dependency] = other.dependencies() else {
                panic!("missing other ordinary edge")
            };
            assert_eq!(dependency.declaration.module.path, "right.rill");
            let target =
                lower_root(&receipt, module, other.owner(), AnalysisLimits::default()).unwrap();
            assert!(target.mir().unwrap().cancellation().is_none());
            assert!(target.rejected_forwarding_calls().is_none());

            let blocked = root(&report, b"function blocked(");
            assert_eq!(blocked.ordinary_completion(), Completion::Incomplete);
            assert!(matches!(blocked.forwarded_call(), Delivery::Unresolved));
            let target =
                lower_root(&receipt, module, blocked.owner(), AnalysisLimits::default()).unwrap();
            assert_eq!(target.completion(), Completion::Incomplete);
            assert!(target.rejected_forwarding_calls().is_none());
        },
    );
}

fn assert_forwarded_proof(report: &GraphReport<'_>, module: &loader::Module) {
    let outer = root(report, b"function nestedGoodWithOwner(");
    let Delivery::Proved(forwarding) = outer.forwarded_call() else {
        panic!("missing forwarding proof");
    };
    assert!(matches!(outer.enabled_frame(), Delivery::Unresolved));
    assert!(matches!(outer.selected_call(), Delivery::Unresolved));
    let mir = outer.mir().unwrap();
    let block = &mir.blocks[forwarding.call.block];
    let middle = &report.roots()[forwarding.call.callee_root];
    assert!(
        matches!(block.operation, Operation::Call(callee) if middle.owner() == BodyOwner::Declaration(callee))
    );
    assert_eq!(forwarding.call.site.node, block.site.node);
    assert_eq!(forwarding.call.site.span.bytes(), b"goodWithOwner(enter)");
    let Delivery::Proved(retaining) = middle.selected_call() else {
        panic!("missing middle retaining context");
    };
    let leaf = &report.roots()[retaining.call.callee_root];
    let Delivery::Proved(enabled) = leaf.enabled_frame() else {
        panic!("missing leaf proof");
    };
    let leaf_mir = leaf.ordinary_mir().unwrap();
    let checkpoint = (0..leaf_mir.blocks.len())
        .find(|&index| {
            matches!(
                enabled.successor(index),
                Some(CancellationSuccessor::LoopBackedge { .. })
            )
        })
        .unwrap();
    assert_eq!(
        forwarding.leaf_checkpoint.node,
        leaf_mir.blocks[checkpoint].site.node
    );
    assert_eq!(
        forwarding.leaf_checkpoint.span.bytes(),
        b"while (enter) { continue; }"
    );
    assert_eq!(
        forwarding.suspended.argument.staged.root.span.bytes(),
        b"enter"
    );
    assert_eq!(
        forwarding.suspended.argument.parameter.root.span.bytes(),
        b"enter: bool"
    );
    for (body, at_entry) in [
        (outer, forwarding.caller_entry_site),
        (middle, retaining.caller_entry_site),
        (leaf, enabled.entry_site),
    ] {
        assert_eq!(
            (at_entry.module, at_entry.node),
            (
                &module.key,
                module.syntax.nodes[body.site().node].children[1]
            )
        );
    }
    assert_eq!(
        cleanup_names(&forwarding.on_caller_entry_delivery.cleanup),
        [(false, b"enter: bool".as_slice())]
    );
    assert_eq!(
        cleanup_names(&forwarding.suspended.caller_cleanup),
        [(true, b"outerText".as_slice()), (false, b"enter: bool")]
    );
    let Terminator::Goto(next) = block.terminator else {
        panic!("missing normal suffix")
    };
    assert_eq!(
        cleanup_names(&path_drops(mir, next)),
        [(true, b"outerSuffix".as_slice()), (true, b"outerText")]
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
    assert!(matches!(
        root(&at, b"function nestedGoodWithOwner(").forwarded_call(),
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
                        && !matches!(root.forwarded_call(), Delivery::Proved(_))
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
                    root(&exhausted, b"function nestedCaller(").forwarded_call(),
                    Delivery::Rejected {
                        finding_id: 0,
                        via_call: Some(_)
                    }
                ));
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
