use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{
    Delivery, GraphCompletion, GraphObligation, GraphReport, Place, ReplacementCancellation,
    RootReport, analyze_ownership,
};
use loader::{DeclarationId, DeclarationKind};

const SOURCE: &str = r#"
import std::core::dispose as release;

function budgetLeaf(): void { }

type Bag = { head: string; transferred: string; manual: string; tail: string; }
implement release(value: Bag): void {
    let saved = value.transferred;
    budgetLeaf();
    let manual = value.manual;
}

function replaceInitialized(old: Bag, incoming: Bag): Bag {
    let mut current = old;
    current = incoming;
    budgetLeaf();
    return current;
}

function replaceVacant(old: Bag, incoming: Bag): Bag {
    let mut current = old;
    let prior = current;
    current = incoming;
    budgetLeaf();
    return current;
}
"#;

#[test]
fn installed_defers_delivery_through_replacement() {
    probe_with_typed(
        "cancellation::replacement::installed_defers_delivery_through_replacement",
        SOURCE,
        |request, module, receipt, typed| {
            let leaf = &declaration(module, "budgetLeaf").id;
            let initialized = &declaration(module, "replaceInitialized").id;
            let vacant = &declaration(module, "replaceVacant").id;
            let dispose = &typed.dispose_role().unwrap().id;
            let family = typed.inventory().family(dispose).unwrap().unwrap();
            let implementation = &family
                .implementations
                .iter()
                .find(|body| body.source.declaration.module == module.key)
                .unwrap()
                .source
                .declaration;
            assert_eq!(implementation.kind, DeclarationKind::Implementation);

            let run = |limits| lower_body(receipt, module, initialized, limits).unwrap();
            let report = run(AnalysisLimits::default());
            assert_eq!(report.completion(), Completion::Checked);
            let mir = report.mir().unwrap();
            let proof = mir
                .replacement_cancellation()
                .expect("checked ordinary MIR has no replacement delivery judgment");
            assert_eq!(proof.implementation, implementation);
            let body = mir.disposer(implementation).unwrap();
            assert!(mir.cancellation().is_none() && body.cancellation().is_none());
            let leaf_report = query(receipt, module, "budgetLeaf");
            let leaf_entry = leaf_report.mir().unwrap().cancellation().unwrap();
            assert_eq!(proof.entry.node, leaf_entry.entry_site.node);
            assert!(leaf_entry.on_entry_delivery.cleanup.is_empty());
            assert_initialized(mir, body, proof, leaf);
            let vacant_report = query(receipt, module, "replaceVacant");
            let vacant_mir = vacant_report.mir().unwrap();
            assert!(vacant_mir.replacement_cancellation().is_none());
            assert_vacant(vacant_mir, implementation);

            let limits = exact_limits(report.statistics());
            let exact = run(limits);
            assert_eq!(exact.statistics(), report.statistics());
            assert!(exact.mir().unwrap().replacement_cancellation().is_some());
            for short in one_short(limits) {
                let limited = run(short);
                assert_eq!(limited.completion(), Completion::ResourceLimit, "{short:?}");
                assert!(limited.mir().is_none() && limited.findings().is_empty());
            }

            let graph = |limits| analyze_ownership(receipt, limits);
            let report = graph(AnalysisLimits::default());
            assert_eq!(report.test_mode(), request.test_mode);
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
            assert!(report.findings().is_empty());
            assert!(
                report
                    .open_obligations()
                    .contains(&GraphObligation::MaskingAndJoin)
            );
            let root = graph_root(&report, initialized);
            assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
            assert!(matches!(root.selected_call(), Delivery::Unresolved));
            let Delivery::Proved(proof) = root.replacement_cancellation() else {
                panic!("checked graph has no replacement delivery judgment");
            };
            assert_eq!(proof.implementation, implementation);
            let active = report
                .active_disposers()
                .iter()
                .find(|root| root.owner() == BodyOwner::Declaration(implementation))
                .unwrap();
            assert!(matches!(active.enabled_frame(), Delivery::Unresolved));
            assert!(matches!(active.selected_call(), Delivery::Unresolved));
            assert!(matches!(
                active.replacement_cancellation(),
                Delivery::Unresolved
            ));
            assert!(matches!(
                graph_root(&report, leaf).enabled_frame(),
                Delivery::Proved(_)
            ));
            assert_initialized(root.mir().unwrap(), active.mir().unwrap(), proof, leaf);
            let root = graph_root(&report, vacant);
            assert!(matches!(
                root.replacement_cancellation(),
                Delivery::Unresolved
            ));
            assert_vacant(root.mir().unwrap(), implementation);

            let limits = exact_limits(report.statistics());
            let exact = graph(limits);
            assert_eq!(exact.statistics(), report.statistics());
            assert!(matches!(
                graph_root(&exact, initialized).replacement_cancellation(),
                Delivery::Proved(_)
            ));
            for short in one_short(limits) {
                let limited = graph(short);
                assert_eq!(
                    limited.completion(),
                    GraphCompletion::ResourceLimit,
                    "{short:?}"
                );
                let root = graph_root(&limited, initialized);
                assert!(
                    root.mir().is_none()
                        && matches!(root.replacement_cancellation(), Delivery::Unresolved)
                );
            }
        },
    );
}

#[test]
fn installed_replacement_without_the_proved_leaf_stays_unresolved() {
    // The initialized caller reaches a real empty entry, but not the
    // one reached by this disposer. The other caller has no admitted entry.
    let mut source = SOURCE.replacen("budgetLeaf();", "otherLeaf();", 1);
    source.push_str(
        r#"
        function otherLeaf(): void { }
        function nonempty(): void { let unrelated = "local"; }
        function replaceNoEntry(old: Bag, incoming: Bag): Bag {
            let mut current = old;
            current = incoming;
            nonempty();
            return current;
        }
    "#,
    );
    probe_with_typed(
        "cancellation::replacement::installed_replacement_without_the_proved_leaf_stays_unresolved",
        &source,
        |_, module, receipt, _| {
            // The full graph also checks the 14 scalar disposal source roots.
            // The targeted ordinary bounds below are unchanged.
            let graph = analyze_ownership(
                receipt,
                AnalysisLimits {
                    max_work: 203358,
                    max_entries: 8126,
                    max_depth: 3,
                },
            );
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            for (name, work, entries) in [
                ("replaceInitialized", 12804, 1835),
                ("replaceNoEntry", 12872, 1871),
            ] {
                let report = lower_body(
                    receipt,
                    module,
                    &declaration(module, name).id,
                    AnalysisLimits {
                        max_work: work,
                        max_entries: entries,
                        max_depth: 3,
                    },
                )
                .unwrap();
                assert_eq!(report.completion(), Completion::Checked);
                assert!(report.mir().unwrap().replacement_cancellation().is_none());
                let root = graph_root(&graph, &declaration(module, name).id);
                assert_eq!(root.completion(), Completion::Checked);
                assert!(matches!(
                    root.replacement_cancellation(),
                    Delivery::Unresolved
                ));
            }
        },
    );
}

fn assert_initialized(
    mir: &Mir<'_>,
    body: &Mir<'_>,
    proof: &ReplacementCancellation<'_>,
    leaf: &DeclarationId,
) {
    assert_eq!(proof.callee, leaf);
    assert_eq!(proof.entry.span.bytes(), b"{ }");
    let old = &mir.blocks[mir.entry];
    let Operation::Move {
        from,
        into: Some(current),
    } = old.operation
    else {
        panic!("old must move to current")
    };
    assert_eq!(from.root.span.bytes(), b"old: Bag");
    assert_eq!(current.root.span.bytes(), b"current");
    assert_same_place(current, proof.installed);
    let staged = &mir.blocks[goto(old.terminator)];
    let Operation::Move {
        from,
        into: Some(stage),
    } = staged.operation
    else {
        panic!("RHS must be staged separately")
    };
    assert_eq!(from.root.span.bytes(), b"incoming: Bag");
    assert_same_place(stage, proof.staged);
    assert_ne!(stage.root.node, current.root.node);
    assert_eq!(staged.terminator, Terminator::Goto(proof.displacement));
    let displace = &mir.blocks[proof.displacement];
    assert_dispose(&displace.operation, current, proof.implementation);
    assert_eq!(displace.terminator, Terminator::Goto(proof.installation));
    let install = &mir.blocks[proof.installation];
    let Operation::Install { target, incoming } = install.operation else {
        panic!("pending state ends only after installation")
    };
    assert_same_place(target, current);
    assert_same_place(incoming, stage);
    assert_eq!(
        install.terminator,
        Terminator::Goto(proof.post_install_call)
    );
    let post_install = &mir.blocks[proof.post_install_call];
    assert!(matches!(post_install.operation, Operation::Call(id) if id == leaf));

    let masked = &proof.deferred;
    let call = &body.blocks[masked.call];
    assert!(matches!(call.operation, Operation::Call(id) if id == leaf));
    assert_ne!(call.site.node, post_install.site.node);
    assert_eq!(call.terminator, Terminator::Goto(masked.resume));
    assert_eq!(masked.operand.root.span.bytes(), b"value: Bag");
    let saved = &body.blocks[body.entry];
    let saved_local = assert_field_move(&saved.operation, masked.operand, b"transferred", b"saved");
    assert_eq!(saved.terminator, Terminator::Goto(masked.call));
    let resume = &body.blocks[masked.resume];
    assert!(matches!(resume.operation, Operation::None));
    let manual = &body.blocks[goto(resume.terminator)];
    let manual_local = assert_field_move(&manual.operation, masked.operand, b"manual", b"manual");
    let locals = &body.blocks[goto(manual.terminator)];
    assert!(matches!(&locals.operation, Operation::Cleanup(actions)
        if matches!(actions.as_slice(), [MirCleanup::Drop(m), MirCleanup::Drop(s)]
            if m.root.node == manual_local && s.root.node == saved_local)));
    assert_eq!(locals.terminator, Terminator::Goto(masked.defer_fields));
    let defer = &body.blocks[masked.defer_fields];
    let Operation::Cleanup(actions) = &defer.operation else {
        panic!("deferred fields")
    };
    let [MirCleanup::DeferDisposerFields { from, into }] = actions.as_slice() else {
        panic!("active operand must not recursively dispose")
    };
    assert_same_place(*from, masked.operand);
    assert_same_place(*into, masked.remainder);
    let epilogue = &body.blocks[goto(defer.terminator)];
    let Operation::Cleanup(actions) = &epilogue.operation else {
        panic!("epilogue cleanup")
    };
    let [
        MirCleanup::Drop(tail),
        MirCleanup::Drop(head),
        MirCleanup::EndStorage(remainder),
    ] = actions.as_slice()
    else {
        panic!("only remaining fields, in reverse order")
    };
    for (place, field) in [(tail, b"tail"), (head, b"head")] {
        assert_eq!(place.root.node, remainder.root.node);
        assert_eq!(
            place.field().unwrap().name.as_deref().unwrap().as_bytes(),
            field
        );
    }
    assert_same_place(*remainder, masked.remainder);
    assert_eq!(
        body.blocks[goto(epilogue.terminator)].terminator,
        Terminator::Return
    );

    use compiler::analysis::ownership::{
        ReplacementRequest::{Clear, Pending},
        ReplacementSuccessor, ReplacementTrigger,
    };
    let resumed = masked.after_masked_yield(Pending);
    assert_eq!((resumed.block, resumed.request), (masked.resume, Pending));
    let check = |request, enabled| {
        let ReplacementSuccessor::Check(check) = proof.after_install(request, enabled) else {
            panic!("expected to reach the next check")
        };
        assert_eq!(check.block(), proof.post_install_call);
        assert_eq!(check.request(), request);
        check
    };
    let clear = masked.after_masked_yield(Clear).request;
    // At unmask a pending request requires no second yield. Fresh delivery
    // instead uses the later actual yield. Both exits dispose the new owner
    // under deferral, with no normal successor to export it or redeliver.
    for (step, cause) in [
        (
            proof.after_install(resumed.request, true),
            ReplacementTrigger::UnmaskAfterInstall,
        ),
        (
            check(clear, true).advance(true, true),
            ReplacementTrigger::PostInstallYield,
        ),
    ] {
        let ReplacementSuccessor::DisposeInstalledAndExit {
            trigger,
            cleanup,
            deferred,
        } = step
        else {
            panic!("expected terminal cleanup")
        };
        assert_eq!(trigger, cause);
        assert_dispose_actions(cleanup, target, proof.implementation);
        assert!(std::ptr::eq(deferred, masked));
    }
    let suffix = goto(post_install.terminator);
    for (next, expected) in [
        (check(resumed.request, false).advance(false, true), Pending),
        (check(resumed.request, false).advance(true, true), Pending),
        (check(clear, false).advance(true, true), Pending),
        (check(clear, true).advance(true, false), Pending),
        (check(clear, true).advance(false, true), Clear),
    ] {
        assert!(
            matches!(next, ReplacementSuccessor::Continue {block,request} if block==suffix && request==expected)
        );
    }
    assert!(path_drops(mir, proof.post_install_call).is_empty());
    assert_export(mir, target);
}

fn assert_vacant(mir: &Mir<'_>, implementation: &DeclarationId) {
    let install = mir
        .blocks
        .iter()
        .position(|b| matches!(b.operation, Operation::Install { .. }))
        .unwrap();
    let Operation::Install { target, .. } = mir.blocks[install].operation else {
        unreachable!()
    };
    let assignment = &mir.blocks[install - 1];
    assert_eq!(assignment.terminator, Terminator::Goto(install));
    assert!(matches!(&assignment.operation, Operation::Cleanup(actions) if actions.is_empty()));
    let drops = path_drops(mir, mir.entry);
    let [
        MirCleanup::InvokeDisposer {
            operand: prior,
            implementation: selected,
        },
    ] = drops.as_slice()
    else {
        panic!("dispose only the separate lexical owner")
    };
    assert_eq!(*selected, implementation);
    assert_eq!(prior.root.span.bytes(), b"prior");
    assert!(
        mir.blocks[..install]
            .iter()
            .any(|block| matches!(block.operation,
        Operation::Move { from, into: Some(into) }
        if from.root.node == target.root.node && into.root.node == prior.root.node))
    );
    assert_export(mir, target);
}

fn assert_export(mir: &Mir<'_>, current: Place<'_>) {
    let [export] = mir
        .blocks
        .iter()
        .filter_map(|b| match b.operation {
            Operation::Export(place) => Some(place),
            _ => None,
        })
        .collect::<Vec<_>>()[..]
    else {
        panic!("ordinary path exports exactly once")
    };
    assert!(mir.blocks.iter().any(
        |b| matches!(b.operation, Operation::Move { from, into: Some(stage) }
        if from.root.node == current.root.node && stage.root.node == export.root.node)
    ));
}

fn assert_field_move(
    op: &Operation<'_>,
    operand: Place<'_>,
    field: &[u8],
    binding: &[u8],
) -> NodeId {
    let Operation::Move {
        from,
        into: Some(into),
    } = op
    else {
        panic!("expected moved field")
    };
    assert_eq!(from.root.node, operand.root.node);
    assert_eq!(
        from.field().unwrap().name.as_deref().unwrap().as_bytes(),
        field
    );
    assert_eq!(into.root.span.bytes(), binding);
    into.root.node
}

fn assert_dispose(op: &Operation<'_>, owner: Place<'_>, implementation: &DeclarationId) {
    let Operation::Cleanup(actions) = op else {
        panic!("expected cleanup")
    };
    assert_dispose_actions(actions, owner, implementation);
}

fn assert_dispose_actions(
    actions: &[MirCleanup<'_>],
    owner: Place<'_>,
    implementation: &DeclarationId,
) {
    let [
        MirCleanup::InvokeDisposer {
            operand,
            implementation: selected,
        },
    ] = actions
    else {
        panic!("expected exactly one canonical disposer")
    };
    assert_eq!(*selected, implementation);
    assert_same_place(*operand, owner);
}

fn assert_same_place(actual: Place<'_>, expected: Place<'_>) {
    assert_eq!(actual.root.module, expected.root.module);
    assert_eq!(actual.root.node, expected.root.node);
    assert!(actual.projection.is_none() && expected.projection.is_none());
}

fn goto(terminator: Terminator) -> usize {
    let Terminator::Goto(next) = terminator else {
        panic!("expected continuation")
    };
    next
}

fn graph_root<'a, 'g>(report: &'a GraphReport<'g>, id: &DeclarationId) -> &'a RootReport<'g> {
    report
        .roots()
        .iter()
        .find(|root| root.owner() == BodyOwner::Declaration(id))
        .unwrap()
}

fn exact_limits(stats: Statistics) -> AnalysisLimits {
    AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    }
}

fn one_short(limits: AnalysisLimits) -> [AnalysisLimits; 3] {
    [
        AnalysisLimits {
            max_work: limits.max_work - 1,
            ..limits
        },
        AnalysisLimits {
            max_entries: limits.max_entries - 1,
            ..limits
        },
        AnalysisLimits {
            max_depth: limits.max_depth - 1,
            ..limits
        },
    ]
}
