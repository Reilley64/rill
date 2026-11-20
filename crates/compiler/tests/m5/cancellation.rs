use super::*;
use compiler::analysis::ownership::{
    CancellationExit, CancellationSuccessor, Cleanup as MirCleanup,
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

fn check_pair(test: &str, source: &str, checkpoint: &[u8]) {
    probe_with_typed(test, source, |request, module, receipt, typed| {
        let bodies = typed.bodies();
        let nodes = &module.syntax.nodes;
        let declaration = |name| {
            module
                .declarations
                .iter()
                .find(|d| d.scope.is_none() && d.name.as_deref() == Some(name))
                .unwrap()
        };
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
            let decl = declaration(name);
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
                    let outcomes = expr(condition).boolean.unwrap();
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
                    let callee = declaration("observe");
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
                assert_eq!(proof.entry_site.node, body.site.node);
                let entry_exit = &proof.on_entry_delivery;
                assert!(entry_exit.retained.as_ref().unwrap().last_store.is_none());
                assert!(
                    entry_exit
                        .cleanup
                        .iter()
                        .all(|action| matches!(action, MirCleanup::EndStorage(_)))
                );
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
                        let Terminator::Branch { no, .. } = next_test.last().unwrap().terminator
                        else {
                            panic!("ordinary backedge did not re-test the loop condition");
                        };
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
                assert_repaired_exit(exit, repair, module);
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

fn assert_repaired_exit(exit: &CancellationExit<'_>, repair: NodeId, module: &loader::Module) {
    let retained = exit.retained.as_ref().unwrap();
    assert_eq!(retained.parameter.root.span.bytes(), b"slot: &mut &bool");
    assert!(retained.parameter.projection.is_none());
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
    assert!(parameters.iter().all(|action| matches!(action, MirCleanup::EndStorage(place) if place.root.span.end() < text.root.span.start())));
}
