use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{
    DependencyContract, GraphCompletion, JoinChildState, JoinEvent, JoinNext, RootReport,
    analyze_ownership,
};
use compiler::analysis::scalars::Site;
use compiler::analysis::typed::ScopedJoinCall;

pub(super) fn assert_fresh_result_lifecycle<'g>(
    mir: &Mir<'g>,
    typed: &TypedReport<'g>,
    transfer: &ScopedJoinCall<'g>,
    position: usize,
) {
    let [left, right, operation, ..] = linear(mir, mir.entry)[..] else {
        panic!("both environments must form in source order before join")
    };
    let Operation::ScopedJoin(join) = operation else {
        panic!("actual scoped operation")
    };
    assert_eq!(join.family, &transfer.role.family.id);
    assert_eq!(join.implementation, &transfer.role.implementation.id);
    assert_eq!(join.bridge, &transfer.role.bridge.id);
    let role = typed.result_role().unwrap();
    let origin = transfer.slots[position].result_origin.unwrap();
    let err = origin.span.bytes().starts_with(b"Err<");
    let captured = !transfer.slots[1 - position].arrow().captures.is_empty();
    let stages = std::array::from_fn::<_, 2, _>(|i| {
        let frame = &join.frames[i];
        let arrow = transfer.slots[i].arrow();
        assert_eq!(frame.origin.node, arrow.site.node);
        assert!(frame.backing.is_none());
        let Operation::FormScopedValueArrow {
            environment,
            capture,
        } = [left, right][i]
        else {
            panic!("actual scoped environment formation");
        };
        assert_eq!(environment.root.node, frame.origin.node);
        let parts = linear(&frame.body, frame.body.entry);
        let (value, rest) = parts.split_first().unwrap();
        if i == position {
            assert!(capture.is_none() && frame.capture.is_none());
            let Operation::Initialize(payload) = value else {
                panic!("actual payload before construction");
            };
            assert_eq!(
                payload.root.span.bytes(),
                if err { b"7i32" } else { b"true" }
            );
            match rest[0] {
                Operation::ConstructResultError {
                    constructor,
                    variant,
                    wrapper,
                    payload: from,
                    ..
                } if err => {
                    assert_eq!(*constructor, &role.providers.unwrap().constructor.id);
                    assert_eq!(*variant, &role.err.id);
                    assert_eq!(*wrapper, &role.wrapper.id);
                    assert_eq!(from.root.node, payload.root.node);
                }
                Operation::ConstructVariant {
                    variant,
                    payload: Some(from),
                    ..
                } if !err => {
                    assert_eq!(*variant, &role.ok.id);
                    assert_eq!(from.root.node, payload.root.node);
                }
                other => panic!("actual construction at {origin:?}: {other:?}"),
            }
            let construct = frame
                .body
                .blocks
                .iter()
                .find(|block| std::ptr::eq(&block.operation, rest[0]))
                .unwrap();
            assert_eq!(construct.site.node, origin.node);
        } else if captured {
            assert_eq!(capture.unwrap().root.span.bytes(), b"flag: bool");
            assert!(matches!(value, Operation::Copy { from, .. }
                if from.root.node == frame.capture.unwrap().root.node));
        } else {
            assert!(capture.is_none() && frame.capture.is_none());
            assert!(
                matches!(value, Operation::Initialize(place) if place.root.span.bytes() == b"false")
            );
        }
        let [
            Operation::StageScopedValue { from, retirement },
            Operation::Cleanup(cleanup),
            Operation::None,
        ] = &rest[usize::from(i == position)..]
        else {
            panic!("actual child export, then environment cleanup, then return");
        };
        assert_eq!(ended(cleanup[0]).root.node, frame.origin.node);
        assert_eq!(cleanup.len(), 1);
        assert_stage_cleanup(
            *retirement,
            from.root.node,
            (err && i == position).then_some(origin),
            role,
        );
        from.root.node
    });
    for (i, cleanup) in join.before_commit.iter().rev().enumerate() {
        assert_eq!(ended(*cleanup).root.node, join.frames[i].origin.node);
    }
    assert_eq!(join.before_commit.len(), 2);
    assert_eq!(join.entries.len(), 8);
    for entry in &join.entries {
        let delivered = entry.yielded && entry.delivery.enabled && entry.delivery.pending;
        assert_eq!(
            matches!(entry.next, JoinNext::CancelBeforeCommit),
            delivered
        );
        if !delivered {
            let JoinNext::Waiting(i) = entry.next else {
                panic!("committed wait")
            };
            assert_eq!(join.waiting[i].initialized_results, [false, false]);
        }
    }
    assert_eq!(join.parent_exit.cleanup.len(), usize::from(captured));
    if captured {
        assert_eq!(
            ended(join.parent_exit.cleanup[0]).root.span.bytes(),
            b"flag: bool"
        );
    }

    let mut finish_last = [false; 2];
    let mut stopped = [false; 4];
    let mut masked_publish = false;
    for wait in &join.waiting {
        assert!(wait.retained_backings.iter().all(Option::is_none));
        assert!(wait.environment_backings.iter().all(Option::is_none));
        for (event, next) in &wait.successors {
            if let JoinEvent::StageResult(i) = *event {
                let JoinNext::Waiting(next) = *next else {
                    panic!("stage is not publication")
                };
                assert_eq!(wait.children[i], JoinChildState::Running);
                assert!(!wait.initialized_results[i]);
                let next = &join.waiting[next];
                assert_eq!(next.children[i], JoinChildState::Cleaning);
                assert!(next.initialized_results[i]);
                assert_eq!(
                    next.initialized_results[1 - i],
                    wait.initialized_results[1 - i]
                );
            }
            if let JoinEvent::CleanChildEnvironment(i) = *event {
                let JoinNext::Waiting(next) = *next else {
                    panic!("child cleanup precedes child finish")
                };
                assert_eq!(wait.children[i], JoinChildState::Cleaning);
                assert_eq!(
                    join.waiting[next].children[i],
                    if wait.initialized_results[i] {
                        JoinChildState::AtNormalReturn
                    } else {
                        JoinChildState::AtDeliveredExit
                    }
                );
            }
            match *next {
                JoinNext::Publish {
                    delivery,
                    transfers,
                    retained_results,
                } => {
                    let JoinEvent::FinishChild(last) = *event else {
                        panic!("publish only after both cleanups and finishes")
                    };
                    assert!(!wait.deferred);
                    assert_eq!(wait.initialized_results, [true, true]);
                    assert_eq!(wait.children[last], JoinChildState::AtNormalReturn);
                    assert_eq!(wait.children[1 - last], JoinChildState::Complete);
                    assert_eq!(transfers.map(|(from, _)| from.root.node), stages);
                    assert_eq!(
                        transfers.map(|(_, into)| into.tuple_element()),
                        [Some(0), Some(1)]
                    );
                    assert_eq!(
                        transfers.map(|(_, into)| into.root.node),
                        join.frames.each_ref().map(|f| f.result.root.node)
                    );
                    assert!(retained_results.iter().all(Option::is_none));
                    assert_eq!(delivery, wait.delivery);
                    finish_last[last] = true;
                    masked_publish |= !delivery.enabled && delivery.pending;
                }
                JoinNext::Cancel { retired } => {
                    let JoinEvent::FinishChild(last) = *event else {
                        panic!("cancel only after both cleanups and finishes")
                    };
                    assert!(wait.deferred);
                    assert!(matches!(
                        wait.children[last],
                        JoinChildState::AtNormalReturn | JoinChildState::AtDeliveredExit
                    ));
                    assert_eq!(wait.children[1 - last], JoinChildState::Complete);
                    assert_eq!(
                        retired.map(|action| action.is_some()),
                        wait.initialized_results
                    );
                    for (i, action) in retired.into_iter().enumerate() {
                        if let Some(cleanup) = action {
                            assert_stage_cleanup(
                                cleanup,
                                stages[i],
                                (err && i == position).then_some(origin),
                                role,
                            );
                        }
                    }
                    stopped[usize::from(wait.initialized_results[0])
                        + 2 * usize::from(wait.initialized_results[1])] = true;
                }
                _ => {}
            }
        }
    }
    assert_eq!(finish_last, [true; 2]);
    assert_eq!(stopped, [true; 4]);
    assert!(masked_publish);
    let block = mir
        .blocks
        .iter()
        .find(|b| std::ptr::eq(&b.operation, operation))
        .unwrap();
    let Terminator::Goto(next) = block.terminator else {
        panic!("ordinary join continuation")
    };
    let destination = join.frames[0].result.root;
    let (next, exported) = if destination.span.bytes() == b"pair" {
        let Operation::Move {
            from,
            into: Some(into),
        } = mir.blocks[next].operation
        else {
            panic!("named tuple moves after publication");
        };
        assert_eq!(from.root.node, destination.node);
        let Terminator::Goto(next) = mir.blocks[next].terminator else {
            panic!("continue to export")
        };
        (next, into.root)
    } else {
        (next, destination)
    };
    assert!(
        matches!(mir.blocks[next].operation, Operation::Export(place) if place.root.node == exported.node)
    );
}

fn assert_stage_cleanup(
    cleanup: Obligation<'_>,
    source: usize,
    err: Option<Site<'_>>,
    role: &compiler::analysis::typed::ResultRole<'_>,
) {
    if let Some(expected) = err {
        let Obligation::DropFreshResultError {
            operand,
            origin,
            variant,
            wrapper,
            storage_source,
        } = cleanup
        else {
            panic!("expected real sealed Err disposal, got {cleanup:?}");
        };
        assert_eq!(operand.root.node, source);
        assert_eq!(origin.node, expected.node);
        assert_eq!(origin.span, expected.span);
        assert_eq!(variant, &role.err.id);
        assert_eq!(wrapper, &role.wrapper.id);
        assert_eq!(storage_source, role.representation.unwrap().storage_source);
    } else {
        assert_eq!(ended(cleanup).root.node, source);
    }
}

pub(super) fn assert_dependencies<'g>(
    root: &RootReport<'g>,
    typed: &TypedReport<'g>,
    join: &ScopedJoinCall<'g>,
    origin: Site<'g>,
) {
    let [join_edge, rest @ ..] = root.dependencies() else {
        panic!("selected join dependency")
    };
    assert_eq!(join_edge.declaration, &join.role.implementation.id);
    assert!(
        matches!(join_edge.contract, DependencyContract::ScopedJoin { bridge, .. } if bridge == &join.role.bridge.id)
    );
    if origin.span.bytes() == b"Result.Ok(true)" {
        assert!(rest.is_empty());
        return;
    }
    let role = typed.result_role().unwrap();
    let repr = role.representation.unwrap().storage_source;
    let [construct, retire] = rest else {
        panic!("child constructor and cancellation-only retirement: {rest:?}")
    };
    assert_eq!(construct.site.node, origin.node);
    assert_eq!(
        construct.declaration,
        &role.providers.unwrap().constructor.id
    );
    assert!(
        matches!(construct.contract, DependencyContract::FreshResultConstructor { variant, wrapper, storage_source } if variant == &role.err.id && wrapper == &role.wrapper.id && storage_source == repr)
    );
    assert_eq!(retire.site.node, origin.node);
    assert_eq!(retire.declaration, &role.wrapper.id);
    assert!(
        matches!(retire.contract, DependencyContract::FreshResultErrorDisposal { variant, origin: original, storage_source } if variant == &role.err.id && original == origin.node && storage_source == repr)
    );
}

#[test]
fn fresh_join_checks_capture_conflicts_and_parent_cleanup() {
    probe_with_typed(
        "scoped_join::fresh_result::fresh_join_checks_capture_conflicts_and_parent_cleanup",
        r#"
            import std::routine::join as scoped;
            type Bag = {}
            implement dispose(bag: Bag): void { let values: List<i32> = [1i32]; dispose(values); }
            function unproved(bag: Bag): [Result<bool, i32>, bool] {
                return scoped([(): Result<bool, i32> => Result.Ok(true), () => false]);
            }
            function conflict(seed: bool, bag: Bag): [Result<bool, i32>, bool] {
                let mut flag = seed;
                let held = &mut flag;
                let pair = scoped([() => Err<bool, i32>(7i32), () => flag]);
                let keep = *held;
                return pair;
            }
            function liveParentError(): [Result<bool, i32>, bool] {
                let error = Err<bool, i32>(11i32);
                let pair = scoped([(): Result<bool, i32> => Result.Ok(true), () => false]);
                let _ = error;
                return pair;
            }
            function retiredParentError(): [Result<bool, i32>, bool] {
                let error = Err<bool, i32>(11i32);
                let _ = error;
                return scoped([(): Result<bool, i32> => Result.Ok(true), () => false]);
            }
        "#,
        |_, module, receipt, typed| {
            assert_pre_equality_graph_cap(receipt, (228_462, 18_860, 8));
            let graph_cost = Statistics {
                work: 228_778,
                entries: 18_994,
                max_depth: 8,
            };
            let (limit, short) = budget_caps(graph_cost);
            let graph = analyze_ownership(receipt, limit);
            assert_eq!(graph.statistics(), graph_cost);
            assert_eq!(graph.completion(), GraphCompletion::SourceRejected);
            let [conflict] = graph.findings() else {
                panic!("second capture conflicts")
            };
            assert_eq!(conflict.code(), "R0201");
            assert_eq!(conflict.site.span.bytes(), b"flag");
            assert_eq!(conflict.related.span.bytes(), b"&mut flag");
            for cap in short {
                let limited = analyze_ownership(receipt, cap);
                assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                assert!(
                    limited
                        .roots()
                        .iter()
                        .all(|root| root.ordinary_mir().is_none())
                );
            }

            for (name, expected, (work, entries, max_depth)) in [
                ("unproved", Completion::Incomplete, (4_246, 561, 7)),
                ("conflict", Completion::SourceRejected, (4_520, 693, 7)),
                ("liveParentError", Completion::Incomplete, (4_182, 638, 7)),
                (
                    "retiredParentError",
                    Completion::Checked,
                    (25_938, 13_247, 8),
                ),
            ] {
                let d = &module.declarations[named(module, name)];
                let body = &module.syntax.nodes[d.node].span;
                let call = module
                    .syntax
                    .nodes
                    .iter()
                    .position(|node| {
                        node.kind == NodeKind::Call
                            && node.span.start() >= body.start()
                            && node.span.end() <= body.end()
                            && node.span.bytes().starts_with(b"scoped([")
                    })
                    .unwrap();
                let transfer = receipt.scoped_join(module, call).unwrap().unwrap();
                assert_eq!(
                    transfer.slots[0].result_origin.unwrap().span.bytes(),
                    if name == "conflict" {
                        b"Err<bool, i32>(7i32)".as_slice()
                    } else {
                        b"Result.Ok(true)"
                    }
                );
                assert!(transfer.slots[1].result_origin.is_none());
                let stats = Statistics {
                    work,
                    entries,
                    max_depth,
                };
                let (limit, short) = budget_caps(stats);
                let target = lower_body(receipt, module, &d.id, limit).unwrap();
                assert_eq!(target.statistics(), stats, "{name}");
                for cap in short {
                    let limited = lower_body(receipt, module, &d.id, cap).unwrap();
                    assert_eq!(
                        limited.completion(),
                        Completion::ResourceLimit,
                        "{name}: {cap:?}"
                    );
                    assert!(limited.mir().is_none());
                }
                assert_eq!(
                    target.completion(),
                    expected,
                    "{name}: {:?}",
                    target.blocked_at()
                );
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == d.node)
                    .unwrap();
                assert_eq!(root.completion(), expected);
                assert_eq!(root.ordinary_completion(), expected);
                assert_eq!(
                    root.blocked_at().map(|site| site.node),
                    target.blocked_at().map(|site| site.node)
                );
                if name == "retiredParentError" {
                    let role = typed.result_role().unwrap();
                    for mir in [target.mir().unwrap(), root.mir().unwrap()] {
                        let ops = linear(mir, mir.entry);
                        let drop = ops
                            .iter()
                            .position(|op| {
                                matches!(op,
                                    Operation::Cleanup(actions) if matches!(actions.as_slice(),
                                        [Obligation::DropFreshResultError { origin, variant, wrapper, storage_source, .. }]
                                            if origin.span.bytes() == b"Err<bool, i32>(11i32)"
                                                && *variant == &role.err.id
                                                && *wrapper == &role.wrapper.id
                                                && *storage_source == role.representation.unwrap().storage_source
                                    )
                                )
                            })
                            .unwrap();
                        let joined = ops
                            .iter()
                            .position(|op| matches!(op, Operation::ScopedJoin(_)))
                            .unwrap();
                        assert!(drop < joined);
                        let [
                            Operation::ScopedJoin(proof),
                            Operation::Export(exported),
                            ..,
                        ] = &ops[joined..]
                        else {
                            panic!("normal parent exports the published pair")
                        };
                        assert_eq!(exported.root.node, proof.frames[0].result.root.node);
                        assert!(
                            proof.parent_exit.cleanup.is_empty(),
                            "stop cannot retire the consumed parent Err"
                        );
                    }
                    continue;
                }
                assert!(target.mir().is_none() && root.ordinary_mir().is_none());
                assert!(root.dependencies().is_empty());
                if name == "conflict" {
                    let [finding] = target.findings() else {
                        panic!("capture conflict")
                    };
                    assert_eq!(finding.code(), conflict.code());
                    assert_eq!(finding.site.span, conflict.site.span);
                    assert_eq!(finding.related.span, conflict.related.span);
                } else {
                    assert_eq!(target.blocked_at().unwrap().node, call);
                    assert!(target.findings().is_empty());
                }
            }
        },
    );
}

#[test]
fn live_parent_error_reaches_the_join_at_its_repinned_cap() {
    // Metering rule 11, D3 (b) (ri-785.4, review 0c3e2b4f): at d95ae9e this
    // route stopped at the unlowered `pair` binding at 3,449/320/3. Since
    // 003c414 it checks both captures before the parent exit, so it stops at
    // the join call. The fixture is the legacy-caps row that pins the new cap.
    installed(
        "scoped_join::fresh_result::live_parent_error_reaches_the_join_at_its_repinned_cap",
        |request| {
            fs::write(
                &request.entry,
                include_str!("../legacy-caps/live-parent-err/main.rill"),
            )
            .unwrap();
            for test_mode in [false, true] {
                let loaded = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let module = app(&loaded);
                let typed = analyze_typed(&loaded, AnalysisLimits::default());
                assert_eq!(typed.completion(), TypedCompletion::Checked);
                let receipt = typed.receipt().unwrap();

                assert_pre_equality_graph_cap(&receipt, (219_596, 17_727, 8));
                let graph_cost = Statistics {
                    work: 219_912,
                    entries: 17_861,
                    max_depth: 8,
                };
                let (limit, short) = budget_caps(graph_cost);
                let graph = analyze_ownership(&receipt, limit);
                assert_eq!(graph.statistics(), graph_cost);
                assert_eq!(graph.completion(), GraphCompletion::Incomplete);
                assert!(graph.findings().is_empty());
                for cap in short {
                    let limited = analyze_ownership(&receipt, cap);
                    assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                    assert!(limited.findings().is_empty());
                    assert!(limited.roots().iter().all(|r| r.mir().is_none()));
                }

                for (name, expected, (work, entries, max_depth)) in [
                    ("liveParentError", Completion::Incomplete, (4_024, 571, 7)),
                    (
                        "retiredParentError",
                        Completion::Checked,
                        (25_740, 13_172, 8),
                    ),
                ] {
                    let d = &module.declarations[named(module, name)];
                    let body = &module.syntax.nodes[d.node].span;
                    let call = module
                        .syntax
                        .nodes
                        .iter()
                        .position(|node| {
                            node.kind == NodeKind::Call
                                && node.span.start() >= body.start()
                                && node.span.end() <= body.end()
                                && node.span.bytes().starts_with(b"join([")
                        })
                        .unwrap();
                    let stats = Statistics {
                        work,
                        entries,
                        max_depth,
                    };
                    let (limit, short) = budget_caps(stats);
                    let target = lower_body(&receipt, module, &d.id, limit).unwrap();
                    assert_eq!(target.statistics(), stats, "{name}");
                    assert_eq!(target.completion(), expected, "{name}");
                    assert!(target.findings().is_empty());
                    for cap in short {
                        let limited = lower_body(&receipt, module, &d.id, cap).unwrap();
                        assert_eq!(
                            limited.completion(),
                            Completion::ResourceLimit,
                            "{name}: {cap:?}"
                        );
                        assert!(limited.mir().is_none() && limited.findings().is_empty());
                    }
                    let root = graph
                        .roots()
                        .iter()
                        .find(|r| r.site().module == &module.key && r.site().node == d.node)
                        .unwrap();
                    assert_eq!(root.completion(), expected);
                    assert_eq!(root.ordinary_completion(), expected);
                    if expected == Completion::Checked {
                        assert!(target.mir().is_some() && root.ordinary_mir().is_some());
                        assert!(target.blocked_at().is_none() && root.blocked_at().is_none());
                        continue;
                    }
                    assert!(target.mir().is_none() && root.ordinary_mir().is_none());
                    assert_eq!(target.blocked_at().unwrap().node, call);
                    assert_eq!(root.blocked_at().unwrap().node, call);
                    let old = AnalysisLimits {
                        max_work: 3_449,
                        max_entries: 320,
                        max_depth: 3,
                    };
                    let stopped = lower_body(&receipt, module, &d.id, old).unwrap();
                    assert_eq!(stopped.completion(), Completion::ResourceLimit);
                    assert!(stopped.mir().is_none() && stopped.findings().is_empty());
                }
            }
        },
    );
}

#[test]
fn shared_join_rejects_a_live_fresh_error_without_wrapper_selection() {
    installed(
        "scoped_join::fresh_result::shared_join_rejects_a_live_fresh_error_without_wrapper_selection",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            // Review 0bfd8df4: a live parent Err cannot close the ordinary
            // scoped exit, so the rejection must not select its wrapper.
            let source = SHARED
                .replace(
                    "import std::routine::join;",
                    "import std::routine::join;\nimport std::result::Err as fail;",
                )
                .replace(
                    "    let [first, second] = join(",
                    "    let staged: Result<bool, i32> = fail<bool, i32>(7i32);\n    let [first, second] = join(",
                )
                .replace(
                    "    dispose(numbers);",
                    "    let _ = staged;\n    dispose(numbers);",
                );
            assert_eq!(source.matches("staged").count(), 2);
            fs::write(&request.entry, format!("{BASE}\n{source}")).unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let module = app(&graph);
                let caller = &module.declarations[named(module, "joined")];
                let node = call_node(module, "join([");
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                assert_eq!(typed.completion(), TypedCompletion::Checked);
                let receipt = typed.receipt().unwrap();
                // Equal to the same fixture with the scoped parent's disposer
                // lookup removed; the pre-preflight route paid 6,299/1,398/10
                // targeted and 217,320/6,276/10 for the graph.
                let target_cost = Statistics {
                    work: 5_825,
                    entries: 1_301,
                    max_depth: 10,
                };
                let (limit, short) = budget_caps(target_cost);
                let target = lower_body(&receipt, module, &caller.id, limit).unwrap();
                assert_eq!(target.statistics(), target_cost);
                assert_eq!(target.completion(), Completion::Incomplete);
                assert!(target.mir().is_none() && target.findings().is_empty());
                assert_eq!(target.blocked_at().unwrap().node, node);
                for cap in short {
                    let limited = lower_body(&receipt, module, &caller.id, cap).unwrap();
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{cap:?}");
                    assert!(limited.mir().is_none());
                }
                assert_pre_equality_graph_cap(&receipt, (216_846, 6_179, 10));
                let graph_cost = Statistics {
                    work: 217_162,
                    entries: 6_313,
                    max_depth: 10,
                };
                let (limit, short) = budget_caps(graph_cost);
                let owned = analyze_ownership(&receipt, limit);
                assert_eq!(owned.statistics(), graph_cost);
                assert_eq!(owned.completion(), GraphCompletion::Incomplete);
                let root = owned
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == caller.node)
                    .unwrap();
                assert_eq!(root.completion(), Completion::Incomplete);
                assert!(root.mir().is_none());
                assert_eq!(root.blocked_at().unwrap().node, node);
                assert!(!root.dependencies().iter().any(|edge| matches!(
                    edge.contract,
                    DependencyContract::FreshResultErrorDisposal { .. }
                )));
                for cap in short {
                    let limited = analyze_ownership(&receipt, cap);
                    assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                    assert!(limited.roots().iter().all(|r| r.mir().is_none()));
                }
            }
        },
    );
}
