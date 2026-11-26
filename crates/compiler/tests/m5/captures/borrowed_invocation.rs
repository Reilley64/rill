use super::*;
use compiler::analysis::{InvocationMode, Statistics, typed::TypedCompletion};

fn caps(work: usize, entries: usize, max_depth: usize) -> AnalysisLimits {
    AnalysisLimits {
        max_work: work,
        max_entries: entries,
        max_depth,
    }
}

fn exact(used: Statistics) -> AnalysisLimits {
    caps(used.work, used.entries, used.max_depth)
}

fn assert_invocation<'g>(mir: &Mir<'g>, borrowed_capture: bool) -> (Place<'g>, Place<'g>) {
    let path = straight_blocks(mir, mir.entry);
    let mut ops = path.iter().copied();
    let formation = ops
        .find(|block| matches!(block.operation, Operation::FormClosurePair { .. }))
        .unwrap();
    let Operation::FormClosurePair {
        closure,
        captures: [first, second],
        mode: InvocationMode::ReadOnly,
    } = formation.operation
    else {
        panic!("expected a read-only formation")
    };
    assert_eq!(closure.span.bytes(), b"f");
    assert_eq!(second.span.bytes(), b"seed");
    let acquisition = path
        .iter()
        .take_while(|block| block.site.node != formation.site.node);
    if borrowed_capture {
        assert_eq!(first.span.bytes(), b"&flag");
        assert!(acquisition.clone().any(|block| matches!(block.operation,
            Operation::Borrow { source, holder, exclusive: false }
            if source.root.span.bytes() == b"flag" && holder.root.node == first.node)));
    } else {
        assert_eq!(first.span.bytes(), b"flag");
        assert!(acquisition.clone().any(|block| matches!(block.operation,
            Operation::Copy { from, into: Some(holder) }
            if from.root.span.bytes() == b"flag: &bool" && holder.root.node == first.node)));
    }
    assert!(acquisition.clone().any(|block| matches!(block.operation,
        Operation::Copy { from, into: Some(holder) }
        if from.root.span.bytes() == b"seed: bool" && holder.root.node == second.node)));
    assert!(ops.any(|block| matches!(block.operation,
        Operation::Borrow { source, holder, exclusive: false }
        if source.root.node == closure.node && holder.root.span.bytes() == b"descriptor")));
    let call = ops
        .find(|block| block.site.span.bytes() == b"descriptor()")
        .unwrap();
    let Operation::InvokeReadOnlyClosurePair {
        receiver,
        origin,
        into,
    } = call.operation
    else {
        panic!("expected to invoke the concrete closure body")
    };
    assert_eq!(origin.node, formation.site.node);
    assert_eq!(origin.module, formation.site.module);
    assert_eq!(receiver.root.node, closure.node);
    assert!(path_drops(mir, mir.entry).is_empty());
    (receiver, into)
}

#[test]
fn shared_descriptor_invokes_the_reaching_local_pair() {
    probe_with_typed(
        "captures::borrowed_invocation::shared_descriptor_invokes_the_reaching_local_pair",
        r#"
            function probe(flag: &bool, seed: bool): [&bool, bool] {
                let f: () => [&bool, bool] = () => [flag, seed];
                let descriptor = &f;
                return descriptor();
            }
        "#,
        |request, module, receipt, typed| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let target = check_local_capture_root(module, receipt, &graph, "probe", None);
            let decl = capture_declaration(module, "probe");
            let verify = |mir: &Mir<'_>| {
                let (receiver, result) = assert_invocation(mir, false);
                let call = straight_blocks(mir, mir.entry)
                    .into_iter()
                    .find(|block| {
                        matches!(block.operation, Operation::InvokeReadOnlyClosurePair { .. })
                    })
                    .unwrap();
                let fact = typed
                    .bodies()
                    .node(module, call.site.node)
                    .unwrap()
                    .unwrap();
                let invocation = fact.callable.as_ref().unwrap().invocation.as_ref().unwrap();
                assert_eq!(invocation.access, Access::ReadOnly);
                assert_eq!(invocation.mode, InvocationMode::ReadOnly);
                assert!(invocation.assumption.is_none());
                assert!(
                    typed
                        .bodies()
                        .arrow(module, invocation.receiver)
                        .unwrap()
                        .is_none()
                );
                let ops = linear(mir, mir.entry);
                assert!(ops.iter().any(|op| matches!(op, Operation::Export(place) if place.root.node == result.root.node)));
                assert!(ops.iter().all(|op| !matches!(op, Operation::Move { from, .. } if from.root.node == receiver.root.node)));
            };
            verify(target.mir().unwrap());
            verify(
                capture_graph_root(module, &graph, decl)
                    .ordinary_mir()
                    .unwrap(),
            );

            let run = |limit| lower_body(receipt, module, &decl.id, limit).unwrap();
            verify(run(exact(target.statistics())).mir().unwrap());
            for short in short_capture_limits(exact(target.statistics())) {
                let limited = run(short);
                crate::limit::assert_body_limit(&limited);
            }
            let bounded = analyze_ownership(receipt, exact(graph.statistics()));
            verify(
                capture_graph_root(module, &bounded, decl)
                    .ordinary_mir()
                    .unwrap(),
            );
            for short in short_capture_limits(exact(graph.statistics())) {
                let limited = analyze_ownership(receipt, short);
                crate::limit::assert_graph_limit(&limited);
            }
            let foreign = loader::load_installed(request.clone()).unwrap();
            let duplicate = foreign
                .modules()
                .iter()
                .find(|m| m.key == module.key)
                .unwrap();
            assert!(matches!(
                lower_body(receipt, duplicate, &decl.id, AnalysisLimits::default()),
                Err(compiler::analysis::QueryError::InvalidRequest)
            ));
        },
    );
}

#[test]
fn descriptor_access_ends_before_its_results_backing() {
    for (tail, rejection) in [
        ("let _ = *view; flag = true;", None),
        (
            "flag = true; let _ = *view;",
            Some(("R0203", "flag = true;", "&flag")),
        ),
        (
            "let _ = &f; let _ = *view; flag = true;",
            Some(("R0200", "&f", "f")),
        ),
    ] {
        probe(
            "captures::borrowed_invocation::descriptor_access_ends_before_its_results_backing",
            &format!(
                r#"
                function probe(input: bool, seed: bool): bool {{
                    let mut flag = input;
                    let f: () => [&bool, bool] = () => [&flag, seed];
                    let descriptor = &f;
                    let [view, bit] = descriptor();
                    let _ = f;
                    {tail}
                    return bit;
                }}
            "#
            ),
            |_, module, receipt| {
                let graph = analyze_ownership(receipt, AnalysisLimits::default());
                let target = check_local_capture_root(module, receipt, &graph, "probe", rejection);
                let decl = capture_declaration(module, "probe");
                let root = capture_graph_root(module, &graph, decl);
                if rejection.is_some() {
                    assert!(root.ordinary_mir().is_none());
                    return;
                }
                for mir in [target.mir().unwrap(), root.ordinary_mir().unwrap()] {
                    let (receiver, result) = assert_invocation(mir, true);
                    let ops = linear(mir, mir.entry);
                    for (index, binding) in [(0, b"view".as_slice()), (1, b"bit")] {
                        assert!(ops.iter().any(|op| matches!(op,
                            Operation::Copy { from, into: Some(into) }
                            if from.root.node == result.root.node && from.tuple_element() == Some(index)
                                && into.root.span.bytes() == binding)));
                    }
                    let release = ops.iter().position(|op| matches!(op, Operation::Move { from, .. } if from.root.node == receiver.root.node)).unwrap();
                    let read = ops.iter().position(|op| matches!(op, Operation::Dereference(from) if from.root.span.bytes() == b"view")).unwrap();
                    let write = ops.iter().position(|op| matches!(op, Operation::Install { target, .. } if target.root.span.bytes() == b"flag")).unwrap();
                    let returned = ops.iter().position(|op| matches!(op, Operation::Copy { from, .. } if from.root.span.bytes() == b"bit")).unwrap();
                    assert!(release < read && read < write && write < returned);
                }
            },
        );
    }
}

#[test]
fn descriptor_cannot_invoke_a_replaced_owner_or_use_exclusive_access() {
    probe(
        "captures::borrowed_invocation::descriptor_cannot_invoke_a_replaced_owner_or_use_exclusive_access",
        r#"
            function replaced(flag: &bool, seed: bool): [&bool, bool] {
                let mut f: () => [&bool, bool] = () => [flag, seed];
                let descriptor = &f;
                f = () => [flag, seed];
                return descriptor();
            }
            function exclusive(flag: &bool, seed: bool): [&bool, bool] {
                let mut f: () => [&bool, bool] = () => [flag, seed];
                let descriptor = &mut f;
                return descriptor();
            }
        "#,
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            check_local_capture_root(
                module,
                receipt,
                &graph,
                "replaced",
                Some(("R0203", "f = () => [flag, seed];", "&f")),
            );
            check_capture_blocker(module, receipt, &graph, "exclusive", "descriptor()");
        },
    );
}

#[test]
fn borrowed_once_and_unproved_bodies_do_not_issue_receipts() {
    use compiler::analysis::scalars::ScalarFindingKind;

    for (source, once, limit) in [
        (
            r#"
            function probe(left: string, right: string): [string, string] {
                let f: once () => [string, string] = () => [left, right];
                let descriptor = &f;
                return descriptor();
            }
        "#,
            true,
            caps(429_076, 53_075, 12),
        ),
        (
            r#"
            function probe(f: () => [&bool, bool]): bool {
                let descriptor = &f;
                let [view, bit] = descriptor();
                let _ = *view;
                return bit;
            }
        "#,
            false,
            caps(427_259, 52_882, 12),
        ),
        (
            r#"
            function forward(job: () => [&bool, bool]): () => [&bool, bool] { return job; }
            function probe(input: bool, seed: bool): bool {
                let mut flag = input;
                let f: () => [&bool, bool] = () => [&flag, seed];
                let moved: () => [&bool, bool] = forward(f);
                let descriptor = &moved;
                let [view, bit] = descriptor();
                let _ = *view;
                return bit;
            }
        "#,
            false,
            caps(431_615, 53_375, 12),
        ),
    ] {
        probe_report(
            "captures::borrowed_invocation::borrowed_once_and_unproved_bodies_do_not_issue_receipts",
            source,
            |request, module, typed| {
                let expected = if once {
                    TypedCompletion::SourceRejected
                } else {
                    TypedCompletion::Incomplete
                };
                assert_eq!(typed.completion(), expected);
                assert!(typed.receipt().is_none());
                if once {
                    let [finding] = typed.bodies().findings() else {
                        panic!("expected an access finding")
                    };
                    assert_eq!(finding.kind, ScalarFindingKind::ReadOnly);
                    assert_eq!(finding.site.module, &module.key);
                    assert_eq!(finding.site.span.bytes(), b"descriptor()");
                } else {
                    assert!(typed.bodies().findings().is_empty());
                    assert!(
                        typed
                            .result_use()
                            .unwrap()
                            .pending()
                            .iter()
                            .any(|p| p.blocker == ResultUseBlocker::CallBoundary
                                && p.site.module == &module.key
                                && p.site.span.bytes() == b"descriptor()")
                    );
                }
                let graph = loader::load_installed(request.clone()).unwrap();
                let at_limit = crate::limit::measured(
                    "analyze_typed",
                    "limit",
                    limit,
                    |limits| analyze_typed(&graph, limits),
                    |report| report.statistics(),
                );
                assert_eq!(at_limit.completion(), expected);
                for short in short_capture_limits(exact(typed.statistics())) {
                    let limited = analyze_typed(&graph, short);
                    crate::limit::assert_typed_limit(&limited);
                }
            },
        );
    }
}
