use super::*;
use compiler::analysis::ownership::{
    CancellationExit, CancellationSuccessor, Cleanup as MirCleanup, RetainedPlace,
};
use compiler::analysis::scalars::{BodyOwner, NodeFactKind};
use compiler::parser::{NodeId, NodeKind};

#[test]
fn ordinary_only_list_loops_keep_the_pre_delivery_caps() {
    use compiler::analysis::ownership::{Delivery, analyze_ownership};
    probe(
        "cancellation::ordinary_only_list_loops_keep_the_pre_delivery_caps",
        r#"
        function entryOnly(xs: List<i32>): i32 { return (&xs)[0u64]; }
        function noop(): void {}
        function across(xs: List<i32>, again: bool): void { noop(); while (again) { continue; } dispose(xs); }
        function veto(xs: List<i32>, again: bool): void { let gate = again; while (gate) { continue; } dispose(xs); }
        function impossible(xs: List<i32>, again: bool): void { while (again) { if (again) { break; } } }
        function nested(xs: List<i32>, again: bool): void { { let view = &xs; while (again) { continue; } let last = view; } }
    "#,
        |_, module, receipt| {
            // These are afd1afd's exact caps on this entire capture, not caps
            // derived by first running the new optional proof.
            let limits = |max_work, max_entries, max_depth| AnalysisLimits {
                max_work,
                max_entries,
                max_depth,
            };
            // Metering rule 11 (ri-785.9): the whole-graph cap was
            // 191,781/4,867/6 until the i32 equality source root (+316/+134).
            crate::limit::assert_graph_limit(&analyze_ownership(receipt, limits(191781, 4867, 6)));
            let graph = analyze_ownership(receipt, limits(192114, 5001, 6));
            assert_eq!(
                graph.statistics(),
                compiler::analysis::Statistics {
                    work: 192114,
                    entries: 5001,
                    max_depth: 6
                }
            );
            for (name, work, entries, depth) in [
                ("entryOnly", 2208, 175, 6),
                ("across", 4435, 344, 5),
                ("veto", 2413, 276, 5),
                ("impossible", 2207, 180, 6),
                ("nested", 2378, 254, 5),
            ] {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let local =
                    lower_body(receipt, module, &d.id, limits(work, entries, depth)).unwrap();
                assert_eq!(local.completion(), Completion::Checked, "{name}");
                assert!(local.mir().unwrap().cancellation().is_none());
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == d.node)
                    .unwrap();
                assert!(root.mir().is_some(), "{name}");
                assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
                let unconstrained = lower_body(receipt, module, &d.id, Default::default()).unwrap();
                assert!(
                    unconstrained.mir().unwrap().cancellation().is_none(),
                    "{name} must not publish an entry-only proof"
                );
            }
        },
    );
}

#[test]
fn installed_list_backing_on_budget_delivery() {
    use compiler::analysis::Statistics;
    use compiler::analysis::ownership::{
        Delivery, GraphObligation, GraphReport, analyze_ownership,
    };
    use compiler::analysis::typed::CollectionOperation;

    probe_with_typed(
        "cancellation::installed_list_backing_on_budget_delivery",
        r#"
        function read(input: List<i32>, again: bool): i32 {
            let numbers = input;
            let first = &numbers;
            let second = &numbers;
            while (again) { continue; }
            let result = first[0u64];
            let other = second[1u64];
            dispose(numbers);
            return result;
        }
        function released(input: List<i32>, again: bool): void {
            dispose(input);
            while (again) { continue; }
        }
        function observe(): void {}
        function acrossCall(input: List<i32>, again: bool): void {
            observe();
            while (again) { continue; }
            dispose(input);
        }
        "#,
        |request, module, receipt, typed| {
            let limits = AnalysisLimits::default();
            let decl = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let declaration = decl("read");
            let local = lower_body(receipt, module, &declaration.id, limits).unwrap();
            assert_eq!(local.completion(), Completion::Checked, "{local:?}");
            let graph = analyze_ownership(receipt, limits);
            let root_at = |graph: &GraphReport<'_>, name| {
                graph
                    .roots()
                    .iter()
                    .position(|r| {
                        r.site().module == &module.key && r.site().node == decl(name).node
                    })
                    .unwrap()
            };
            let root = &graph.roots()[root_at(&graph, "read")];
            assert_eq!(root.completion(), Completion::Checked);
            let local_mir = local.mir().unwrap();
            let local_proof = local_mir.cancellation().unwrap_or_else(|| {
                panic!(
                    "no enabled List-frame proof in test_mode={}",
                    request.test_mode
                )
            });
            let Delivery::Proved(graph_proof) = root.enabled_frame() else {
                panic!("no enabled List-frame proof in graph");
            };
            let provider = typed
                .collection_protection()
                .find(|p| p.operation == CollectionOperation::DisposeList)
                .unwrap()
                .provider
                .unwrap();
            let assert_release = |items: &[Obligation<'_>], name: &[u8]| {
                let [
                    Obligation::DisposeListI32 {
                        operand,
                        implementation,
                        bridge,
                    },
                ] = items
                else {
                    panic!("expected one List release, got {items:?}");
                };
                assert_eq!(operand.root.span.bytes(), name);
                assert!(std::ptr::eq(*implementation, &provider.implementation.id));
                assert!(std::ptr::eq(*bridge, &provider.bridge.id));
            };
            for (mir, proof) in [(local_mir, local_proof), (root.mir().unwrap(), graph_proof)] {
                let entry = &proof.on_entry_delivery;
                assert!(entry.retained.is_none());
                let [
                    Obligation::EndStorage(flag),
                    release @ Obligation::DisposeListI32 { .. },
                ] = entry.cleanup.as_slice()
                else {
                    panic!("entry: {:?}", entry.cleanup)
                };
                assert_eq!(flag.root.span.bytes(), b"again: bool");
                assert_release(std::slice::from_ref(release), b"input: List<i32>");

                let successors = (0..mir.blocks.len())
                    .filter_map(|block| proof.successor(block).map(|edge| (block, edge)))
                    .collect::<Vec<_>>();
                let [(block, CancellationSuccessor::LoopBackedge { on_delivery })] =
                    successors.as_slice()
                else {
                    panic!("expected only budget backedge: {successors:?}")
                };
                assert_eq!(
                    mir.blocks[*block].site.span.bytes(),
                    b"while (again) { continue; }"
                );
                assert!(on_delivery.retained.is_none());
                let [
                    Obligation::EndStorage(second),
                    Obligation::EndStorage(first),
                    release @ Obligation::DisposeListI32 { .. },
                    Obligation::EndStorage(flag),
                ] = on_delivery.cleanup.as_slice()
                else {
                    panic!("backedge: {:?}", on_delivery.cleanup)
                };
                assert_eq!(second.root.span.bytes(), b"second");
                assert_eq!(first.root.span.bytes(), b"first");
                assert_eq!(flag.root.span.bytes(), b"again: bool");
                assert_release(std::slice::from_ref(release), b"numbers");
                let (_, ordinary_exit) = branch_targets(mir);
                assert_release(&path_drops(mir, ordinary_exit), b"numbers");
            }
            let released = lower_body(receipt, module, &decl("released").id, limits).unwrap();
            let released_mir = released.mir().unwrap();
            let released_proof = released_mir.cancellation().unwrap();
            let [CancellationSuccessor::LoopBackedge { on_delivery }] =
                (0..released_mir.blocks.len())
                    .filter_map(|i| released_proof.successor(i))
                    .collect::<Vec<_>>()[..]
            else {
                panic!("actual backedge after release")
            };
            assert!(
                matches!(on_delivery.cleanup.as_slice(), [Obligation::EndStorage(flag)]
                if flag.root.span.bytes() == b"again: bool")
            );
            let across = lower_body(receipt, module, &decl("acrossCall").id, limits).unwrap();
            assert!(across.mir().unwrap().cancellation().is_none());
            assert!(matches!(
                graph.roots()[root_at(&graph, "acrossCall")].enabled_frame(),
                Delivery::Unresolved
            ));
            assert!(
                graph
                    .open_obligations()
                    .contains(&GraphObligation::MaskingAndJoin)
            );

            let caps = |s: Statistics| {
                let exact = AnalysisLimits {
                    max_work: s.work,
                    max_entries: s.entries,
                    max_depth: s.max_depth,
                };
                (
                    exact,
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
                    ],
                )
            };
            let (exact, shorts) = caps(local.statistics());
            let limited = |limits| lower_body(receipt, module, &declaration.id, limits).unwrap();
            assert!(limited(exact).mir().unwrap().cancellation().is_some());
            for cap in shorts {
                let short = limited(cap);
                crate::limit::assert_body_limit(&short);
            }
            let (exact, shorts) = caps(graph.statistics());
            let exact_graph = analyze_ownership(receipt, exact);
            assert!(matches!(
                exact_graph.roots()[root_at(&exact_graph, "read")].enabled_frame(),
                Delivery::Proved(_)
            ));
            for cap in shorts {
                let short = analyze_ownership(receipt, cap);
                crate::limit::assert_graph_limit(&short);
                assert!(short.roots().iter().all(|root| root.mir().is_none()
                    && matches!(root.enabled_frame(), Delivery::Unresolved)));
            }
        },
    );
}

// ri-px1.9 (D4): a frame that owns a LockGuard<i32> across the proved
// feasible backedge. Delivery releases the guard once, through its protected
// provider, before the frame's remaining parameter storage ends; it exports
// nothing. Without delivery the ordinary suffix exports, then releases.
#[test]
fn installed_guard_release_on_budget_delivery() {
    use compiler::analysis::Statistics;
    use compiler::analysis::ownership::{Delivery, GraphReport, analyze_ownership};
    use compiler::analysis::typed::{NativeOwnershipAction, OwnershipCoverage, OwnershipRule};

    probe_with_typed(
        "cancellation::installed_guard_release_on_budget_delivery",
        r#"
        function hold(again: bool, lock: LockGuard<i32>): i32 {
            while (again) { continue; }
            return 7;
        }
        function released(again: bool, lock: LockGuard<i32>): void {
            dispose(lock);
            while (again) { continue; }
        }
        function moved(again: bool, lock: LockGuard<i32>): void {
            let held = lock;
            while (again) { continue; }
        }
        "#,
        |request, module, receipt, typed| {
            let limits = |max_work, max_entries, max_depth| AnalysisLimits {
                max_work,
                max_entries,
                max_depth,
            };
            let decl = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower = |name, limits| lower_body(receipt, module, &decl(name).id, limits).unwrap();
            let root_at = |graph: &GraphReport<'_>, name| {
                graph
                    .roots()
                    .iter()
                    .position(|r| {
                        r.site().module == &module.key && r.site().node == decl(name).node
                    })
                    .unwrap()
            };
            let provider = typed
                .ownership_protection()
                .iter()
                .find(|scope| {
                    scope.rule == OwnershipRule::Required(NativeOwnershipAction::GuardRelease)
                        && scope.coverage == OwnershipCoverage::Clear
                })
                .unwrap()
                .provider
                .as_ref()
                .unwrap();
            // One protected release of `name`, then the bool parameter ends.
            let assert_delivery = |cleanup: &[Obligation<'_>], name: &[u8]| {
                let [
                    Obligation::ReleaseGuard {
                        operand,
                        implementation,
                        entry,
                    },
                    Obligation::EndStorage(flag),
                ] = cleanup
                else {
                    panic!("expected one guard release before the flag ends: {cleanup:?}");
                };
                assert_eq!(operand.root.span.bytes(), name);
                assert!(operand.projection.is_none());
                assert!(std::ptr::eq(*implementation, &provider.implementation.id));
                assert!(std::ptr::eq(*entry, &provider.entry.id));
                assert_eq!(flag.root.span.bytes(), b"again: bool");
            };
            // Targeted query and ordinary graph root of the witness.
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let local = lower("hold", AnalysisLimits::default());
            assert_eq!(local.completion(), Completion::Checked, "{local:?}");
            let root = &graph.roots()[root_at(&graph, "hold")];
            assert_eq!(root.completion(), Completion::Checked);
            let local_mir = local.mir().unwrap();
            let local_proof = local_mir.cancellation().unwrap_or_else(|| {
                panic!(
                    "no enabled guard-frame proof in test_mode={}",
                    request.test_mode
                )
            });
            let Delivery::Proved(graph_proof) = root.enabled_frame() else {
                panic!("no enabled guard-frame proof in graph");
            };
            for (mir, proof) in [(local_mir, local_proof), (root.mir().unwrap(), graph_proof)] {
                assert!(proof.on_entry_delivery.retained.is_none());
                assert_delivery(&proof.on_entry_delivery.cleanup, b"lock: LockGuard<i32>");
                assert_delivery(&loop_delivery_cleanup(mir, proof), b"lock: LockGuard<i32>");
                // Without delivery the loop exits to the ordinary suffix:
                // export the result, then release the guard once.
                let (_, ordinary_exit) = branch_targets(mir);
                let suffix = render_path(mir, ordinary_exit);
                assert_eq!(
                    suffix,
                    [
                        "init 7",
                        "export 7",
                        "cleanup [release lock: LockGuard<i32>; end again: bool]",
                        "none",
                    ]
                );
                let drops = path_drops(mir, ordinary_exit);
                let [Obligation::ReleaseGuard { operand, .. }] = drops.as_slice() else {
                    panic!("ordinary suffix: {drops:?}");
                };
                assert_eq!(operand.root.span.bytes(), b"lock: LockGuard<i32>");
            }

            // A guard released before the loop is not released again on delivery.
            let released = lower("released", AnalysisLimits::default());
            let released_mir = released.mir().unwrap();
            let released_root = &graph.roots()[root_at(&graph, "released")];
            assert_eq!(released_root.completion(), Completion::Checked);
            let Delivery::Proved(released_graph) = released_root.enabled_frame() else {
                panic!("no enabled proof for the released guard in graph");
            };
            for (mir, proof) in [
                (released_mir, released_mir.cancellation().unwrap()),
                (released_root.mir().unwrap(), released_graph),
            ] {
                assert_delivery(&proof.on_entry_delivery.cleanup, b"lock: LockGuard<i32>");
                let cleanup = loop_delivery_cleanup(mir, proof);
                assert!(
                    matches!(cleanup.as_slice(), [Obligation::EndStorage(flag)]
                        if flag.root.span.bytes() == b"again: bool"),
                    "{cleanup:?}"
                );
            }

            // A guard moved into a local is released before the flag ends in
            // the targeted proof. Its graph root stays open at the body, as
            // D3 left it, and publishes no delivery proof.
            let moved = lower("moved", AnalysisLimits::default());
            let moved_mir = moved.mir().unwrap();
            let moved_proof = moved_mir.cancellation().unwrap();
            assert_delivery(
                &moved_proof.on_entry_delivery.cleanup,
                b"lock: LockGuard<i32>",
            );
            assert_delivery(&loop_delivery_cleanup(moved_mir, moved_proof), b"held");
            let moved_root = &graph.roots()[root_at(&graph, "moved")];
            assert_eq!(moved_root.completion(), Completion::Incomplete);
            assert!(moved_root.mir().is_none());
            assert!(matches!(moved_root.enabled_frame(), Delivery::Unresolved));

            // Exact caps, each independently one short.
            let shorts = |exact: AnalysisLimits| {
                [
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
                ]
            };
            let stats = |s: Statistics| (s.work, s.entries, s.max_depth);
            for (name, (work, entries, depth)) in [
                ("hold", (2266, 203, 4)),
                ("released", (2308, 223, 4)),
                ("moved", (2326, 223, 4)),
            ] {
                let measured = lower(name, AnalysisLimits::default()).statistics();
                assert_eq!(stats(measured), (work, entries, depth), "{name}");
                let exact = limits(work, entries, depth);
                assert!(
                    lower(name, exact).mir().unwrap().cancellation().is_some(),
                    "{name}"
                );
                for cap in shorts(exact) {
                    let short = lower(name, cap);
                    crate::limit::assert_body_limit(&short);
                }
            }
            let (work, entries, depth) = (188823, 4635, 5);
            assert_eq!(stats(graph.statistics()), (work, entries, depth));
            let exact = limits(work, entries, depth);
            let exact_graph = analyze_ownership(receipt, exact);
            for name in ["hold", "released"] {
                assert!(matches!(
                    exact_graph.roots()[root_at(&exact_graph, name)].enabled_frame(),
                    Delivery::Proved(_)
                ));
            }
            for cap in shorts(exact) {
                let short = analyze_ownership(receipt, cap);
                crate::limit::assert_graph_limit(&short);
                assert!(short.roots().iter().all(|root| root.mir().is_none()
                    && matches!(root.enabled_frame(), Delivery::Unresolved)));
            }
        },
    );
}

#[test]
fn guard_loops_without_a_proof_keep_their_parent_caps() {
    use compiler::analysis::ownership::{Delivery, GraphCompletion, analyze_ownership};
    probe(
        "cancellation::guard_loops_without_a_proof_keep_their_parent_caps",
        r#"
        function entryOnly(lock: LockGuard<i32>): void {}
        function noop(): void {}
        function across(again: bool, lock: LockGuard<i32>): void { noop(); while (again) { continue; } }
        function veto(again: bool, lock: LockGuard<i32>): void { let gate = again; while (gate) { continue; } }
        function impossible(again: bool, lock: LockGuard<i32>): void { while (again) { if (again) { break; } } }
        function nested(again: bool, lock: LockGuard<i32>): void { { let view = &lock; while (again) { continue; } let last = view; } }
    "#,
        |_, module, receipt| {
            // Measured at the parent before guard frames could carry a
            // delivery proof: the graph at 3da1d5a3 (with B6's i32 equality
            // root), the targeted caps at both 0f595438 and 3da1d5a3. None of
            // these shapes gains a proof.
            let limits = |max_work, max_entries, max_depth| AnalysisLimits {
                max_work,
                max_entries,
                max_depth,
            };
            let shorts = |exact: AnalysisLimits| {
                [
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
                ]
            };
            let graph_exact = limits(190512, 4613, 6);
            let graph = analyze_ownership(receipt, graph_exact);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            for cap in shorts(graph_exact) {
                crate::limit::assert_graph_limit(&analyze_ownership(receipt, cap));
            }
            for (name, work, entries, depth, completion) in [
                ("entryOnly", 2003, 83, 1, Completion::Checked),
                ("across", 4213, 245, 4, Completion::Checked),
                ("veto", 2222, 192, 4, Completion::Checked),
                ("impossible", 2174, 166, 6, Completion::Checked),
                ("nested", 2026, 86, 5, Completion::Incomplete),
            ] {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let exact = limits(work, entries, depth);
                let local = lower_body(receipt, module, &d.id, exact).unwrap();
                assert_eq!(local.completion(), completion, "{name}");
                assert!(
                    local.mir().is_none_or(|mir| mir.cancellation().is_none()),
                    "{name}"
                );
                for cap in shorts(exact) {
                    let short = lower_body(receipt, module, &d.id, cap).unwrap();
                    crate::limit::assert_body_limit(&short);
                }
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == d.node)
                    .unwrap();
                assert_eq!(root.completion(), completion, "{name}");
                assert!(
                    matches!(root.enabled_frame(), Delivery::Unresolved),
                    "{name}"
                );
                let unconstrained = lower_body(receipt, module, &d.id, Default::default()).unwrap();
                assert!(
                    unconstrained
                        .mir()
                        .is_none_or(|mir| mir.cancellation().is_none()),
                    "{name} must not publish a guard delivery proof"
                );
            }
        },
    );
}

/// Review ci-uaa1 (ri-px1.9 #3): a guard-owning loop frame whose ordinary path
/// returns the guard has no release of it to verify. It stops at the guard
/// parameter with no delivery proof, at the caps measured at the parent
/// 3da1d5a3 (targeted 1,888/25/2, graph 186,018/4,120/5).
#[test]
fn guard_returned_after_a_loop_keeps_its_parent_caps() {
    use compiler::analysis::ownership::{Delivery, GraphCompletion, analyze_ownership};
    probe(
        "cancellation::guard_returned_after_a_loop_keeps_its_parent_caps",
        "function pass(again: bool, lock: LockGuard<i32>): LockGuard<i32> { while (again) { continue; } return lock; }",
        |_, module, receipt| {
            let shorts = |exact: AnalysisLimits| {
                [
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
                ]
            };
            let graph_exact = AnalysisLimits {
                max_work: 186_025,
                max_entries: 4_120,
                max_depth: 5,
            };
            let graph = analyze_ownership(receipt, graph_exact);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert!(graph.findings().is_empty());
            for cap in shorts(graph_exact) {
                let short = analyze_ownership(receipt, cap);
                crate::limit::assert_graph_limit(&short);
            }
            let d = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("pass"))
                .unwrap();
            let exact = AnalysisLimits {
                max_work: 1_900,
                max_entries: 25,
                max_depth: 2,
            };
            let local = lower_body(receipt, module, &d.id, exact).unwrap();
            assert_eq!(local.completion(), Completion::Incomplete);
            assert_eq!(local.blocked_at().unwrap().span.bytes(), b"lock");
            assert!(local.mir().is_none() && local.findings().is_empty());
            for cap in shorts(exact) {
                let short = lower_body(receipt, module, &d.id, cap).unwrap();
                crate::limit::assert_body_limit(&short);
            }
            let root = graph
                .roots()
                .iter()
                .find(|r| r.site().module == &module.key && r.site().node == d.node)
                .unwrap();
            assert_eq!(root.ordinary_completion(), Completion::Incomplete);
            assert!(root.ordinary_mir().is_none());
            assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
        },
    );
}

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
            let nested = query(receipt, module, "nestedCaller");
            assert_eq!(nested.completion(), Completion::SourceRejected);
            let [nested_finding] = nested.findings() else {
                panic!("missing nested cancellation escape: {nested:?}");
            };
            assert_eq!(nested_finding.code(), finding.code());
            assert_eq!(nested_finding.site.node, finding.site.node);
            assert_eq!(nested_finding.related.node, finding.related.node);
            assert_eq!(
                nested_finding.cancellation_delivery.unwrap().node,
                finding.cancellation_delivery.unwrap().node
            );
            let [outer, retaining] = nested.rejected_forwarding_calls().unwrap();
            for (name, site, spelling) in [
                ("nestedCaller", outer, b"bad(enter)".as_slice()),
                ("bad", retaining, b"unsafeBody(&mut slot, &input, enter)"),
            ] {
                let function = &nodes[declaration(module, name).node];
                assert_eq!(site.module, &module.key);
                assert_eq!(site.span.bytes(), spelling);
                assert!(
                    function.span.start() <= site.span.start()
                        && site.span.end() <= function.span.end()
                );
            }
            assert_forwarded_cleanup(receipt, module, request.test_mode);
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
                let report = query(receipt, module, name);
                assert_eq!(
                    report.completion(),
                    Completion::Checked,
                    "{name}: {report:?}"
                );
                assert!(report.mir().unwrap().cancellation().is_none(), "{name}");
                assert!(report.rejected_forwarding_calls().is_none(), "{name}");
            }
            let skipped = query(receipt, module, "skippedOuter");
            let skipped_mir = skipped.mir().unwrap();
            let entry_only = skipped_mir.cancellation().unwrap();
            assert!(
                (0..skipped_mir.blocks.len()).all(|block| entry_only.successor(block).is_none())
            );
            assert!(skipped.rejected_forwarding_calls().is_none());
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
                        crate::limit::assert_body_limit(&report);
                    }
                }
            }
        },
    );
}

fn assert_forwarded_cleanup(
    receipt: &TypedReceipt<'_, '_>,
    module: &loader::Module,
    test_mode: bool,
) {
    let outer = declaration(module, "nestedGoodWithOwner");
    let run = |limits| lower_body(receipt, module, &outer.id, limits).unwrap();
    let report = run(AnalysisLimits::default());
    let mir = report.mir().unwrap();
    let proof = mir.cancellation().unwrap();
    let [(_, CancellationSuccessor::ForwardedRetainingCall(call))] = (0..mir.blocks.len())
        .filter_map(|i| proof.successor(i).map(|edge| (i, edge)))
        .collect::<Vec<_>>()[..]
    else {
        panic!("expected one link, not an extra call-site checkpoint");
    };
    let middle = declaration(module, "goodWithOwner");
    assert_eq!(call.callee, &middle.id);
    let middle_mir = &call.body;
    let middle_proof = middle_mir.cancellation().unwrap();
    let [CancellationSuccessor::RetainingCall(call_leaf)] = (0..middle_mir.blocks.len())
        .filter_map(|i| middle_proof.successor(i))
        .collect::<Vec<_>>()[..]
    else {
        panic!("missing middle retaining context");
    };
    let leaf = declaration(module, "safeWithOwner");
    assert_eq!(call_leaf.callee, &leaf.id);
    let leaf_proof = call_leaf.body.cancellation().unwrap();
    let [(check, CancellationSuccessor::LoopBackedge { on_delivery })] =
        (0..call_leaf.body.blocks.len())
            .filter_map(|i| {
                leaf_proof
                    .successor(i)
                    .map(|edge| (&call_leaf.body.blocks[i], edge))
            })
            .collect::<Vec<_>>()[..]
    else {
        panic!("missing actual leaf delivery");
    };
    assert_eq!(
        (call.leaf_checkpoint.module, call.leaf_checkpoint.node),
        (&module.key, check.site.node)
    );
    let nodes = &module.syntax.nodes;
    for (decl, entry) in [
        (outer, proof.entry_site),
        (middle, middle_proof.entry_site),
        (leaf, leaf_proof.entry_site),
    ] {
        assert_eq!(
            (entry.module, entry.node),
            (&module.key, nodes[decl.node].children[1])
        );
    }
    let parameter = call.suspended.argument.parameter;
    assert_eq!(parameter.root.span.bytes(), b"enter: bool");
    assert_eq!(call.suspended.argument.staged.root.span.bytes(), b"enter");
    assert!(matches!(
        middle_proof.on_entry_delivery.cleanup.as_slice(),
        [MirCleanup::EndStorage(place)] if place.root.node == parameter.root.node
    ));
    let unwind = on_delivery
        .cleanup
        .iter()
        .chain(&call_leaf.caller_cleanup)
        .chain(&call.suspended.caller_cleanup)
        .map(|action| match action {
            MirCleanup::EndStorage(place) => (false, place.root.span.bytes()),
            MirCleanup::Drop(place) => (true, place.root.span.bytes()),
            _ => panic!("unexpected delivered cleanup: {action:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        unwind,
        [
            (false, b"flag".as_slice()),
            (true, b"calleeText"),
            (false, b"enter: bool"),
            (false, b"input: &bool"),
            (false, b"slot: &mut &bool"),
            (false, b"slot"),
            (false, b"input"),
            (false, b"prior"),
            (false, b"initial"),
            (true, b"callerText"),
            (false, b"enter: bool"),
            (true, b"outerText"),
            (false, b"enter: bool"),
        ]
    );

    for (decl, completion) in [
        (outer, Completion::Checked),
        (
            declaration(module, "nestedCaller"),
            Completion::SourceRejected,
        ),
    ] {
        let run = |limits| lower_body(receipt, module, &decl.id, limits).unwrap();
        let stats = run(AnalysisLimits::default()).statistics();
        eprintln!("targeted forwarding {completion:?}, test_mode={test_mode}: {stats:?}");
        let exact = AnalysisLimits {
            max_work: stats.work,
            max_entries: stats.entries,
            max_depth: stats.max_depth,
        };
        let at = run(exact);
        assert_eq!(at.completion(), completion);
        assert_eq!(at.statistics(), stats);
        if completion == Completion::SourceRejected {
            assert!(at.rejected_forwarding_calls().is_some());
        } else {
            assert!(at.mir().and_then(|mir| mir.cancellation()).is_some());
        }
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
            let exhausted = run(short);
            crate::limit::assert_body_limit(&exhausted);
            assert!(exhausted.rejected_forwarding_calls().is_none());
        }
    }
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
                        max_work: 4074,
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
                        crate::limit::assert_body_limit(&short);
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
                max_work: 2284,
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
            crate::limit::assert_body_limit(&short);
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

/// The cleanup of the one loop-backedge delivery successor in `proof`.
fn loop_delivery_cleanup<'g>(
    mir: &Mir<'g>,
    proof: &compiler::analysis::ownership::Cancellation<'g>,
) -> Vec<MirCleanup<'g>> {
    let successors = (0..mir.blocks.len())
        .filter_map(|block| proof.successor(block).map(|edge| (block, edge)))
        .collect::<Vec<_>>();
    let [(block, CancellationSuccessor::LoopBackedge { on_delivery })] = successors.as_slice()
    else {
        panic!("expected only the budget backedge: {successors:?}")
    };
    assert_eq!(
        mir.blocks[*block].site.span.bytes(),
        b"while (again) { continue; }"
    );
    assert!(on_delivery.retained.is_none());
    on_delivery.cleanup.clone()
}

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
                    crate::limit::assert_body_limit(&short);
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
