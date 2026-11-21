use super::*;
use compiler::analysis::ownership::{
    CancellationSuccessor, Delivery, DependencyContract, GraphCompletion, GraphReport, Place,
    RootReport, analyze_ownership,
};
use compiler::analysis::scalars::{BodyOwner, Site};
use compiler::analysis::{Statistics, typed::ResultRole};
use compiler::loader::{DeclarationId, Module};

fn function<'g>(module: &'g Module, name: &str) -> &'g DeclarationId {
    &module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some(name))
        .unwrap()
        .id
}

fn root<'a, 'g>(report: &'a GraphReport<'g>, id: &DeclarationId) -> &'a RootReport<'g> {
    report
        .roots()
        .iter()
        .find(|r| matches!(r.owner(), BodyOwner::Declaration(found) if found == id))
        .unwrap()
}

fn caps(s: Statistics) -> (AnalysisLimits, [AnalysisLimits; 3]) {
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
}

#[test]
fn installed_fresh_result_residences() {
    use Completion::{Checked, Incomplete};
    probe_with_typed(
        "fresh_result_storage::installed_fresh_result_residences",
        r#"
        import std::result::Err as fail;
        function exportErr(): Result<bool, i32> {
            let value: Result<bool, i32> = fail<bool, i32>(7i32);
            let staged = value;
            return staged;
        }
        function exportOk(): Result<bool, i32> {
            let value: Result<bool, i32> = Result.Ok(true);
            let staged = value;
            return staged;
        }
        function retireErr(): void {
            let value: Result<bool, i32> = fail<bool, i32>(7i32);
            let staged = value;
            let _ = staged;
        }
        function retireOk(): void {
            let value: Result<bool, i32> = Result.Ok(true);
            let staged = value;
            let _ = staged;
        }
        function opaque(value: Result<bool, i32>): void {
            let staged = value;
            let _ = staged;
        }
        function otherSuccess(): Result<i32, i32> { return fail<i32, i32>(7i32); }
        function retained(): Result<bool, i32> {
            let input: Result<bool, i32> = fail<bool, i32>(7i32);
            let bit = input ? (8i32);
            return Result.Ok(bit);
        }
        function retireRetained(): void {
            let staged = retained();
            let _ = staged;
        }
    "#,
        |_, module, receipt, typed| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let (cap, shorts) = caps(report.statistics());
            let exact = analyze_ownership(receipt, cap);
            assert_eq!(exact.statistics(), report.statistics());
            assert_eq!(exact.completion(), GraphCompletion::Incomplete);
            for short in shorts {
                let exhausted = analyze_ownership(receipt, short);
                assert_eq!(exhausted.completion(), GraphCompletion::ResourceLimit);
                assert!(exhausted.roots().iter().all(|r| r.mir().is_none()));
            }
            let role = typed.result_role().unwrap();
            for (name, target_expected, root_expected) in [
                ("exportErr", Checked, Checked),
                ("exportOk", Checked, Checked),
                ("retireErr", Checked, Checked),
                ("retireOk", Checked, Checked),
                ("opaque", Incomplete, Incomplete),
                ("otherSuccess", Checked, Incomplete),
                ("retained", Checked, Incomplete),
                ("retireRetained", Incomplete, Incomplete),
            ] {
                let id = function(module, name);
                let lower = |cap| lower_body(receipt, module, id, cap).unwrap();
                let stats = lower(AnalysisLimits::default()).statistics();
                let (cap, shorts) = caps(stats);
                let target = lower(cap);
                assert_eq!(target.statistics(), stats);
                let body = root(&exact, id);
                assert_eq!(
                    (target.completion(), body.completion()),
                    (target_expected, root_expected),
                    "{name}"
                );
                assert_eq!(target.mir().is_some(), target_expected == Checked);
                assert_eq!(body.mir().is_some(), root_expected == Checked);
                for short in shorts {
                    let exhausted = lower(short);
                    assert_eq!(exhausted.completion(), Completion::ResourceLimit, "{name}");
                    assert!(exhausted.mir().is_none());
                }
                match name {
                    "opaque" | "retireRetained" => {
                        for site in [target.blocked_at(), body.blocked_at()] {
                            assert_eq!(site.unwrap().span.bytes(), b"let _ = staged;");
                        }
                    }
                    "otherSuccess" => assert_eq!(
                        body.dependencies()[0].contract,
                        DependencyContract::ResultConstructor
                    ),
                    "retained" => assert_eq!(
                        body.dependencies()[1].contract,
                        DependencyContract::RetainedErrorDisplay
                    ),
                    _ => {
                        let err = name.ends_with("Err");
                        let export = name.starts_with("export");
                        let origin = residence(target.mir().unwrap(), err, export, role);
                        assert_eq!(
                            residence(body.mir().unwrap(), err, export, role).node,
                            origin.node
                        );
                        assert_dependencies(body, err, export, origin, role);
                    }
                }
            }
        },
    );
}

fn assert_dependencies(
    body: &RootReport<'_>,
    err: bool,
    export: bool,
    origin: Site<'_>,
    role: &ResultRole<'_>,
) {
    let edges = body.dependencies();
    if !err {
        assert!(edges.is_empty());
        return;
    }
    let provider = role.providers.unwrap();
    assert_eq!(edges.len(), if export { 1 } else { 2 });
    assert_eq!(edges[0].site.node, origin.node);
    assert_eq!(edges[0].declaration, &provider.constructor.id);
    assert_eq!(
        edges[0].contract,
        DependencyContract::FreshResultConstructor {
            variant: &role.err.id,
            wrapper: &role.wrapper.id,
            storage_source: provider.storage_source,
        }
    );
    if !export {
        assert_eq!(edges[1].declaration, &role.wrapper.id);
        assert_eq!(
            edges[1].contract,
            DependencyContract::FreshResultErrorDisposal {
                origin: origin.node,
                variant: &role.err.id,
                storage_source: provider.storage_source,
            }
        );
    }
}

fn residence<'g>(mir: &Mir<'g>, err: bool, export: bool, role: &ResultRole<'_>) -> Site<'g> {
    let blocks = straight_blocks(mir, mir.entry);
    let constructor = blocks
        .iter()
        .find(|b| {
            matches!(
                b.operation,
                Operation::ConstructResultError { .. } | Operation::ConstructVariant { .. }
            )
        })
        .unwrap();
    let moves: Vec<_> = blocks
        .iter()
        .filter_map(|b| match b.operation {
            Operation::Move {
                from,
                into: Some(into),
            } => Some((from, into)),
            _ => None,
        })
        .collect();
    let [(value, staged), (moved, terminal)] = moves.as_slice() else {
        panic!("expected two whole Result moves");
    };
    assert_eq!(value.root.span.bytes(), b"value");
    assert_eq!(staged.root.span.bytes(), b"staged");
    assert_eq!(moved.root.node, staged.root.node);
    let origin = constructor.site;
    let into = match constructor.operation {
        Operation::ConstructResultError {
            into,
            variant,
            wrapper,
            ..
        } if err => {
            assert_eq!(origin.span.bytes(), b"fail<bool, i32>(7i32)");
            assert_eq!((variant, wrapper), (&role.err.id, &role.wrapper.id));
            into
        }
        Operation::ConstructVariant {
            into,
            variant,
            payload: Some(_),
        } if !err => {
            assert_eq!(origin.span.bytes(), b"Result.Ok(true)");
            assert_eq!(variant, &role.ok.id);
            into
        }
        _ => panic!("unexpected Result construction"),
    };
    assert_eq!(into.root.node, value.root.node);
    let cleanup: Vec<_> = blocks
        .iter()
        .flat_map(|b| match &b.operation {
            Operation::Cleanup(actions) => actions.as_slice(),
            Operation::Drop(_) | Operation::DropResultFailure { .. } => panic!("wrong cleanup"),
            _ => &[],
        })
        .copied()
        .collect();
    let exports: Vec<_> = blocks
        .iter()
        .filter_map(|b| match b.operation {
            Operation::Export(place) => Some(place.root.node),
            _ => None,
        })
        .collect();
    if export {
        assert_eq!(exports, [terminal.root.node]);
        assert!(cleanup.is_empty(), "export transfers all storage");
    } else {
        assert!(exports.is_empty());
        assert_eq!(
            retired(&cleanup, err, origin, role).root.node,
            terminal.root.node
        );
    }
    origin
}

fn retired<'g>(
    cleanup: &[Obligation<'g>],
    err: bool,
    origin: Site<'_>,
    role: &ResultRole<'_>,
) -> Place<'g> {
    let place = match cleanup {
        [
            Obligation::DropFreshResultError {
                operand,
                origin: built,
                variant,
                wrapper,
                storage_source,
            },
        ] if err => {
            assert_eq!((built.node, built.module), (origin.node, origin.module));
            assert_eq!(built.span.snapshot(), origin.span.snapshot());
            assert_eq!(built.span.start(), origin.span.start());
            assert_eq!((*variant, *wrapper), (&role.err.id, &role.wrapper.id));
            assert_eq!(*storage_source, role.representation.unwrap().storage_source);
            *operand
        }
        [Obligation::EndStorage(place)] if !err => *place,
        other => panic!("unexpected retirement: {other:?}"),
    };
    assert!(place.projection.is_none());
    place
}

#[test]
fn installed_fresh_result_is_retired_at_an_existing_checkpoint() {
    probe_with_typed(
        "fresh_result_storage::installed_fresh_result_is_retired_at_an_existing_checkpoint",
        r#"
        import std::result::Err as fail;
        function checkpoint(): void {}
        function deliverErr(): Result<bool, i32> {
            let value: Result<bool, i32> = fail<bool, i32>(7i32);
            let staged = value;
            checkpoint();
            return staged;
        }
        function deliverOk(): Result<bool, i32> {
            let value: Result<bool, i32> = Result.Ok(true);
            let staged = value;
            checkpoint();
            return staged;
        }
    "#,
        |_, module, receipt, typed| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let role = typed.result_role().unwrap();
            for (name, err) in [("deliverErr", true), ("deliverOk", false)] {
                let id = function(module, name);
                let target = lower_body(receipt, module, id, AnalysisLimits::default()).unwrap();
                let mir = target.mir().unwrap();
                let body = root(&graph, id);
                let Delivery::Proved(graph_delivery) = body.enabled_frame() else {
                    panic!("no enabled-frame proof for {name}");
                };
                for (mir, proof) in [
                    (mir, mir.cancellation().unwrap()),
                    (body.mir().unwrap(), graph_delivery),
                ] {
                    let origin = residence(mir, err, true, role);
                    assert!(proof.on_entry_delivery.cleanup.is_empty());
                    let checkpoints: Vec<_> = (0..mir.blocks.len())
                        .filter_map(|i| proof.successor(i))
                        .collect();
                    let [
                        CancellationSuccessor::CalleeEntry {
                            callee,
                            on_delivery,
                            ..
                        },
                    ] = checkpoints.as_slice()
                    else {
                        panic!("only the callee entry contributes delivery");
                    };
                    assert_eq!(*callee, function(module, "checkpoint"));
                    assert!(on_delivery.retained.is_none());
                    assert_eq!(
                        retired(&on_delivery.cleanup, err, origin, role)
                            .root
                            .span
                            .bytes(),
                        b"staged"
                    );
                }
            }
        },
    );
}

#[test]
fn installed_custom_disposer_cannot_use_fresh_result_storage() {
    use compiler::analysis::{scalars::ScalarUnsupported, typed::TypedCompletion};
    probe_report(
        "fresh_result_storage::installed_custom_disposer_cannot_use_fresh_result_storage",
        r#"
        import std::result::Err as fail;
        import std::core::dispose as release;
        implement release(value: Err<i32>): void {}
        function retire(): void {
            let stage: Result<bool, i32> = fail<bool, i32>(7i32);
            let _ = stage;
        }
    "#,
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            assert!(
                typed
                    .bodies()
                    .obligations()
                    .iter()
                    .any(|o| o.reason == ScalarUnsupported::FamilySelection)
            );
        },
    );
}

#[test]
fn installed_foreign_result_does_not_inherit_cleanup() {
    probe_report_with_modules(
        "fresh_result_storage::installed_foreign_result_does_not_inherit_cleanup",
        r#"
        import local::Result as Foreign;
        function retire(): void {
            let value: Foreign<bool, i32> = Foreign.Ok(true);
            let staged = value;
            let _ = staged;
        }
    "#,
        &[("local.rill", "export enum Result<T, E> { Ok(T), Err(E) }")],
        |_, module, typed| {
            let receipt = typed
                .receipt()
                .expect("foreign enum types still check in M4");
            let id = function(module, "retire");
            let target = lower_body(&receipt, module, id, AnalysisLimits::default()).unwrap();
            let graph = analyze_ownership(&receipt, AnalysisLimits::default());
            let body = root(&graph, id);
            assert_eq!(
                (target.completion(), body.completion()),
                (Completion::Incomplete, Completion::Incomplete)
            );
            assert!(
                target.mir().is_none() && body.mir().is_none() && body.dependencies().is_empty()
            );
        },
    );
}
