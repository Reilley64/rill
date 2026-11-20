use super::*;
use compiler::analysis::{
    ownership::{
        CallableField, CallableLayout, Cleanup as Obligation, GraphCompletion, GraphReport, Place,
        RootReport, analyze_ownership,
    },
    scalars::{data::Access, generics::callables::CaptureForm},
    typed::ResultUseBlocker,
};

#[test]
fn local_exclusive_capture_transfers_the_loan_until_discard() {
    probe_with_typed(
        "captures::local_exclusive_capture_transfers_the_loan_until_discard",
        r#"
            function clear(seed: bool): bool {
                let mut flag = seed;
                let job: mut () => &mut bool = () => &mut flag;
                let held: once () => &mut bool = job;
                let other = held;
                let _ = other;
                flag = true;
                return flag;
            }
            function writeLive(seed: bool): void {
                let mut flag = seed;
                let job: mut () => &mut bool = () => &mut flag;
                let held: once () => &mut bool = job;
                let other = held;
                flag = true;
                let _ = other;
            }
            function readLive(seed: bool): bool {
                let mut flag = seed;
                let job: mut () => &mut bool = () => &mut flag;
                let held: once () => &mut bool = job;
                let other = held;
                let observed = flag;
                let _ = other;
                return observed;
            }
            function reuse(seed: bool): void {
                let mut flag = seed;
                let job: mut () => &mut bool = () => &mut flag;
                let held: once () => &mut bool = job;
                let other = held;
                let _ = held;
                let _ = other;
            }
        "#,
        |request, module, receipt, typed| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(graph.test_mode(), request.test_mode);
            let clear = check_local_capture_root(module, receipt, &graph, "clear", None);
            let (origin, capture) =
                assert_local_exclusive_route(module, clear.mir().unwrap(), false);
            let root = capture_graph_root(module, &graph, capture_declaration(module, "clear"));
            assert_eq!(
                assert_local_exclusive_route(module, root.mir().unwrap(), false),
                (origin, capture)
            );
            let arrow = typed.bodies().arrow(module, origin).unwrap().unwrap();
            let [fact] = arrow.captures.as_slice() else {
                panic!("one explicit capture")
            };
            assert_eq!(fact.site.node, capture);
            assert_eq!(fact.form, CaptureForm::ExplicitBorrow { mutable: true });
            assert_eq!(fact.access, Access::Writable);
            assert!(fact.path.projections.is_empty());

            for (name, code, site, related) in [
                ("writeLive", "R0203", "flag = true;", "&mut flag"),
                ("readLive", "R0201", "flag", "&mut flag"),
                ("reuse", "R0200", "held", "held"),
            ] {
                check_local_capture_root(
                    module,
                    receipt,
                    &graph,
                    name,
                    Some((code, site, related)),
                );
            }

            let declaration = capture_declaration(module, "clear");
            let run = |limits| lower_body(receipt, module, &declaration.id, limits).unwrap();
            let used = clear.statistics();
            let exact = AnalysisLimits {
                max_work: used.work,
                max_entries: used.entries,
                max_depth: used.max_depth,
            };
            assert!(run(exact).mir().is_some());
            for short in short_capture_limits(exact) {
                let exhausted = run(short);
                assert_eq!(exhausted.completion(), Completion::ResourceLimit);
                assert!(exhausted.mir().is_none() && exhausted.findings().is_empty());
            }

            let used = graph.statistics();
            let exact = AnalysisLimits {
                max_work: used.work,
                max_entries: used.entries,
                max_depth: used.max_depth,
            };
            let exact_graph = analyze_ownership(receipt, exact);
            assert_eq!(exact_graph.statistics(), used);
            assert_eq!(exact_graph.completion(), GraphCompletion::SourceRejected);
            for name in ["clear", "writeLive", "readLive", "reuse"] {
                let decl = capture_declaration(module, name);
                let expected = capture_graph_root(module, &graph, decl);
                let actual = capture_graph_root(module, &exact_graph, decl);
                assert_eq!(actual.completion(), expected.completion());
                assert_eq!(actual.mir().is_some(), expected.mir().is_some());
            }
            for short in short_capture_limits(exact) {
                let exhausted = analyze_ownership(receipt, short);
                assert_eq!(exhausted.completion(), GraphCompletion::ResourceLimit);
                assert!(exhausted.roots().iter().all(|root| root.mir().is_none()));
            }
        },
    );
}

#[test]
fn local_exclusive_capture_checks_conflicts_in_written_order() {
    probe(
        "captures::local_exclusive_capture_checks_conflicts_in_written_order",
        r#"
            function exclusiveThenExclusive(seed: bool): void {
                let mut flag = seed;
                let first: once () => &mut bool = () => &mut flag;
                let second: once () => &mut bool = () => &mut flag;
                let _ = first;
                let _ = second;
            }
            function exclusiveThenShared(seed: bool): void {
                let mut flag = seed;
                let first: once () => &mut bool = () => &mut flag;
                let second: once () => &bool = () => &flag;
                let _ = first;
                let _ = second;
            }
            function sharedThenExclusive(seed: bool): void {
                let mut flag = seed;
                let first: once () => &bool = () => &flag;
                let second: once () => &mut bool = () => &mut flag;
                let _ = first;
                let _ = second;
            }
            function independentExclusive(seed: bool): bool {
                let mut flag = seed;
                let mut separate = seed;
                let first: once () => &mut bool = () => &mut flag;
                let second: once () => &mut bool = () => &mut separate;
                let _ = first;
                flag = true;
                let _ = second;
                return flag;
            }
            function independentShared(seed: bool): void {
                let mut flag = seed;
                let separate = seed;
                let first: once () => &mut bool = () => &mut flag;
                let second: once () => &bool = () => &separate;
                let _ = first;
                let _ = second;
            }
            function reacquire(seed: bool): void {
                let mut flag = seed;
                let first: once () => &mut bool = () => &mut flag;
                let _ = first;
                let second: once () => &mut bool = () => &mut flag;
                let _ = second;
            }
            function pairOverlap(seed: bool): void {
                let mut flag = seed;
                let both: once () => [&mut bool, &bool] = () => [&mut flag, &flag];
                let _ = both;
            }
            function pairExclusiveOverlap(seed: bool): void {
                let mut flag = seed;
                let both: once () => [&mut bool, &mut bool] = () => [&mut flag, &mut flag];
                let _ = both;
            }
            function pairIndependent(seed: bool): void {
                let mut flag = seed;
                let separate = seed;
                let both: once () => [&mut bool, &bool] = () => [&mut flag, &separate];
                let _ = both;
            }
            function sharedShared(seed: bool): void {
                let flag = seed;
                let both: once () => [&bool, &bool] = () => [&flag, &flag];
                let _ = both;
            }
            function pairSharedThenExclusive(seed: bool): void {
                let mut flag = seed;
                let both: once () => [&bool, &mut bool] = () => [&flag, &mut flag];
                let _ = both;
            }
            function repeatedMove(seed: bool): void {
                let mut flag = seed;
                let input = &mut flag;
                let both: once () => [&mut bool, &mut bool] = () => [input, input];
                let _ = both;
            }
        "#,
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            for (name, attempted, original) in [
                ("exclusiveThenExclusive", "&mut flag", "&mut flag"),
                ("exclusiveThenShared", "&flag", "&mut flag"),
                ("sharedThenExclusive", "&mut flag", "&flag"),
                ("pairOverlap", "&flag", "&mut flag"),
                ("pairExclusiveOverlap", "&mut flag", "&mut flag"),
                ("pairSharedThenExclusive", "&mut flag", "&flag"),
            ] {
                check_local_capture_root(
                    module,
                    receipt,
                    &graph,
                    name,
                    Some(("R0201", attempted, original)),
                );
            }
            for name in [
                "independentExclusive",
                "independentShared",
                "reacquire",
                "sharedShared",
            ] {
                check_local_capture_root(module, receipt, &graph, name, None);
            }
            let pair = check_local_capture_root(module, receipt, &graph, "pairIndependent", None);
            let mir = pair.mir().unwrap();
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Borrow { .. }
                            | Operation::Reborrow { .. }
                            | Operation::FormClosurePair { .. }
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Borrow { source: flag, holder: left, exclusive: true },
                Operation::Borrow { source: separate, holder: right, exclusive: false },
                Operation::FormClosurePair { captures, .. },
            ] if flag.root.span.bytes() == b"flag" && separate.root.span.bytes() == b"separate"
                && captures[0].node == left.root.node && captures[1].node == right.root.node
                && captures[0].span.bytes() == b"&mut flag" && captures[1].span.bytes() == b"&separate"));
            assert!(path_drops(mir, mir.entry).is_empty());
            check_capture_blocker(
                module,
                receipt,
                &graph,
                "repeatedMove",
                "() => [input, input]",
            );
        },
    );
}

#[test]
fn carried_exclusive_capture_moves_the_descriptor_not_the_referent() {
    probe_with_typed(
        "captures::carried_exclusive_capture_moves_the_descriptor_not_the_referent",
        r#"
            function carriedClear(seed: bool): bool {
                let mut flag = seed;
                let input = &mut flag;
                let job: once () => &mut bool = () => input;
                let held = job;
                let other = held;
                let _ = other;
                flag = true;
                return flag;
            }
            function carriedWriteLive(seed: bool): void {
                let mut flag = seed;
                let input = &mut flag;
                let job: once () => &mut bool = () => input;
                let held = job;
                let other = held;
                flag = true;
                let _ = other;
            }
            function carriedUseOld(seed: bool): void {
                let mut flag = seed;
                let input = &mut flag;
                let job: once () => &mut bool = () => input;
                let _ = input;
                let _ = job;
            }
        "#,
        |_, module, receipt, typed| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let clear = check_local_capture_root(module, receipt, &graph, "carriedClear", None);
            let (origin, capture) =
                assert_local_exclusive_route(module, clear.mir().unwrap(), true);
            let root =
                capture_graph_root(module, &graph, capture_declaration(module, "carriedClear"));
            assert_eq!(
                assert_local_exclusive_route(module, root.mir().unwrap(), true),
                (origin, capture)
            );
            let arrow = typed.bodies().arrow(module, origin).unwrap().unwrap();
            let [fact] = arrow.captures.as_slice() else {
                panic!("one carried capture")
            };
            assert_eq!(fact.site.node, capture);
            assert_eq!(fact.form, CaptureForm::CarriedReference { mutable: true });
            assert_eq!(fact.access, Access::Owned);
            assert_eq!(fact.demand, Some(compiler::analysis::InvocationMode::Once));
            for (name, code, site, related) in [
                ("carriedWriteLive", "R0203", "flag = true;", "&mut flag"),
                ("carriedUseOld", "R0200", "input", "input"),
            ] {
                check_local_capture_root(
                    module,
                    receipt,
                    &graph,
                    name,
                    Some((code, site, related)),
                );
            }
        },
    );
}

fn capture_declaration<'g>(module: &'g loader::Module, name: &str) -> &'g loader::Declaration {
    module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some(name))
        .unwrap()
}

fn capture_graph_root<'a, 'g>(
    module: &'g loader::Module,
    graph: &'a GraphReport<'g>,
    decl: &loader::Declaration,
) -> &'a RootReport<'g> {
    graph
        .roots()
        .iter()
        .find(|r| std::ptr::eq(r.site().module, &module.key) && r.site().node == decl.node)
        .unwrap()
}

fn check_local_capture_root<'g>(
    module: &'g loader::Module,
    receipt: &TypedReceipt<'_, 'g>,
    graph: &GraphReport<'g>,
    name: &str,
    rejection: Option<(&str, &str, &str)>,
) -> compiler::analysis::ownership::Report<'g> {
    let decl = capture_declaration(module, name);
    let report = lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap();
    let root = capture_graph_root(module, graph, decl);
    let expected = if rejection.is_some() {
        Completion::SourceRejected
    } else {
        Completion::Checked
    };
    assert_eq!(report.completion(), expected, "{name}");
    assert_eq!(root.ordinary_completion(), expected, "{name}");
    assert_eq!(root.mir().is_some(), rejection.is_none(), "{name}");
    assert_eq!(report.mir().is_some(), rejection.is_none(), "{name}");
    if let Some((code, site, related)) = rejection {
        let [finding] = report.findings() else {
            panic!("{name}: expected one finding")
        };
        assert_eq!(finding.code(), code);
        let body = &module.syntax.nodes[decl.node].span;
        for (site, expected) in [(&finding.site, site), (&finding.related, related)] {
            assert!(std::ptr::eq(site.module, &module.key));
            assert_eq!(site.span.bytes(), expected.as_bytes());
            assert_eq!(site.span, &module.syntax.nodes[site.node].span);
            assert!(body.start() <= site.span.start() && site.span.end() <= body.end());
        }
        assert!(
            finding.related.span.end() < finding.site.span.start(),
            "{name}"
        );
        let same = |a: compiler::analysis::scalars::Site<'_>,
                    b: compiler::analysis::scalars::Site<'_>| {
            std::ptr::eq(a.module, b.module) && a.node == b.node && a.span == b.span
        };
        assert_eq!(
            graph
                .findings()
                .iter()
                .filter(|f| f.code() == code
                    && same(f.site, finding.site)
                    && same(f.related, finding.related))
                .count(),
            1,
            "{name}"
        );
    } else {
        assert!(report.findings().is_empty());
    }
    report
}

fn check_capture_blocker<'g>(
    module: &'g loader::Module,
    receipt: &TypedReceipt<'_, 'g>,
    graph: &GraphReport<'g>,
    name: &str,
    at: &str,
) {
    let decl = capture_declaration(module, name);
    let report = lower_capture_root(module, receipt, name);
    let root = capture_graph_root(module, graph, decl);
    assert_eq!(report.completion(), Completion::Incomplete, "{name}");
    assert!(report.findings().is_empty() && report.mir().is_none());
    assert_eq!(root.ordinary_completion(), Completion::Incomplete);
    assert!(root.mir().is_none());
    let target_site = report.blocked_at().unwrap();
    let graph_site = root.blocked_at().unwrap();
    assert_eq!(target_site.node, graph_site.node);
    assert!(std::ptr::eq(target_site.module, graph_site.module));
    assert_eq!(target_site.span.bytes(), at.as_bytes());
    assert_eq!(graph_site.span, target_site.span);
}

fn short_capture_limits(exact: AnalysisLimits) -> [AnalysisLimits; 3] {
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
}

fn assert_local_exclusive_route(
    module: &loader::Module,
    mir: &Mir<'_>,
    carried: bool,
) -> (usize, usize) {
    use compiler::analysis::InvocationMode;

    let ops = linear(mir, mir.entry);
    let route: Vec<_> = ops
        .iter()
        .copied()
        .filter(|op| {
            matches!(
                op,
                Operation::Borrow { .. }
                    | Operation::Reborrow { .. }
                    | Operation::Move { .. }
                    | Operation::FormClosure { .. }
            )
        })
        .collect();
    let [
        Operation::Borrow {
            source,
            holder,
            exclusive: true,
        },
        route @ ..,
    ] = route.as_slice()
    else {
        panic!("one exclusive acquisition: {route:?}")
    };
    let (stage, route) = if carried {
        let [
            Operation::Move {
                from,
                into: Some(stage),
            },
            route @ ..,
        ] = route
        else {
            panic!("move the existing descriptor: {route:?}")
        };
        assert_eq!(from.root.node, holder.root.node);
        assert_eq!(from.root.span.bytes(), b"input");
        assert_eq!(stage.root.span.bytes(), b"input");
        (stage, route)
    } else {
        assert_eq!(holder.root.span.bytes(), b"&mut flag");
        (holder, route)
    };
    let [
        Operation::FormClosure {
            closure,
            capture,
            mode,
        },
        Operation::Move {
            from: job,
            into: Some(held),
        },
        Operation::Move {
            from: held_use,
            into: Some(other),
        },
        Operation::Move {
            from: other_use,
            into: Some(discard),
        },
    ] = route
    else {
        panic!("form once and transfer the only owner: {route:?}")
    };
    assert_eq!(
        *mode,
        if carried {
            InvocationMode::Once
        } else {
            InvocationMode::Mutable
        }
    );
    assert_eq!(capture.node, stage.root.node);
    assert_eq!(closure.node, job.root.node);
    assert_eq!(held.root.node, held_use.root.node);
    assert_eq!(other.root.node, other_use.root.node);
    for (place, written) in [
        (source, "flag"),
        (job, "job"),
        (held, "held"),
        (other, "other"),
        (discard, "other"),
    ] {
        assert!(std::ptr::eq(place.root.module, &module.key));
        assert_eq!(place.root.span.bytes(), written.as_bytes());
    }
    let owners = [holder, stage, job, held, other, discard].map(|p| p.root.node);
    let mut ended = Vec::new();
    let mut write = None;
    for (i, op) in ops.iter().enumerate() {
        match op {
            Operation::Cleanup(actions) => ended.extend(actions.iter().filter_map(|a| match a {
                Obligation::EndStorage(place)
                    if owners.contains(&place.root.node) || place.root.node == source.root.node =>
                {
                    Some((i, place.root.node))
                }
                _ => None,
            })),
            Operation::Install { target, .. } if target.root.node == source.root.node => {
                write = Some(i)
            }
            Operation::Copy { from, .. } => assert!(!owners.contains(&from.root.node)),
            _ => {}
        }
    }
    let write = write.expect("backing is writable after discard");
    assert!(
        matches!(ended.as_slice(), [(before, owner), (after, backing)]
        if *owner == discard.root.node && *backing == source.root.node && *before < write && write < *after)
    );
    assert!(path_drops(mir, mir.entry).is_empty());
    let origin = straight_blocks(mir, mir.entry)
        .into_iter()
        .find(|block| matches!(block.operation, Operation::FormClosure { .. }))
        .unwrap()
        .site;
    assert!(std::ptr::eq(origin.module, &module.key));
    assert_eq!(
        origin.span.bytes(),
        if carried {
            b"() => input".as_slice()
        } else {
            b"() => &mut flag"
        }
    );
    (origin.node, capture.node)
}

#[test]
fn exclusive_capture_evidence_distinguishes_disproved_from_unresolved() {
    use compiler::analysis::{scalars::ScalarUnsupported, typed::TypedCompletion};

    let scalar = r#"
        function need<T>(value: T): bool where noBorrow(T) { let _ = value; return true; }
        function probe(seed: bool): bool {
            let mut flag = seed;
            let job: mut () => bool = () => { return seed; };
            let held: once () => bool = job;
            return need(held);
        }
    "#;
    // Same signature, weakening, and goal; only the captured access changes.
    let exclusive = scalar.replace("return seed;", "let _ = &mut flag; return seed;");
    let transfer = |result, packing, call| {
        format!(
            r#"
            type Holder<F> = {{ task: F }};
            function need<T>(value: T): bool where noBorrow(T) {{ let _ = value; return true; }}
            function probe(seed: bool): {result} {{
                let mut flag = seed;
                let job: mut () => &mut bool = () => &mut flag;
                let held: once () => &mut bool = job;
                let other = held;
                {packing}
                return {call};
            }}
        "#
        )
    };
    let wrapped = "let packed: Holder<once () => &mut bool> = Holder { task: other };";
    for (source, completion, call, reason) in [
        (scalar, TypedCompletion::Checked, "need(held)", None),
        (
            &exclusive,
            TypedCompletion::SourceRejected,
            "need(held)",
            None,
        ),
        (
            &transfer("bool", wrapped, "need(packed)"),
            TypedCompletion::Incomplete,
            "need(packed)",
            Some(ScalarUnsupported::Requirements),
        ),
        (
            &transfer("once () => &mut bool", "", "copy(&other)"),
            TypedCompletion::Incomplete,
            "copy(&other)",
            Some(ScalarUnsupported::FamilySelection),
        ),
        (
            &transfer("Holder<once () => &mut bool>", wrapped, "copy(&packed)"),
            TypedCompletion::Incomplete,
            "copy(&packed)",
            Some(ScalarUnsupported::FamilySelection),
        ),
    ] {
        probe_report(
            "captures::exclusive_capture_evidence_distinguishes_disproved_from_unresolved",
            source,
            |_, module, typed| {
                assert_eq!(typed.completion(), completion, "{call}");
                assert_eq!(
                    typed.receipt().is_some(),
                    completion == TypedCompletion::Checked
                );
                assert!(typed.core_blockers().is_empty());
                if completion == TypedCompletion::SourceRejected {
                    let [finding] = typed.bodies().findings() else {
                        panic!("expected only the noBorrow failure")
                    };
                    assert_eq!(finding.kind.code(), Some("R0302"));
                    assert!(std::ptr::eq(finding.site.module, &module.key));
                    assert_eq!(finding.site.span.bytes(), call.as_bytes());
                    assert_eq!(
                        finding.site.span,
                        &module.syntax.nodes[finding.site.node].span
                    );
                } else {
                    assert!(typed.bodies().findings().is_empty());
                }
                if let Some(reason) = reason {
                    let [pending] = typed.bodies().obligations() else {
                        panic!("expected an unresolved goal: {call}")
                    };
                    assert_eq!(pending.reason, reason);
                    assert!(std::ptr::eq(pending.site.module, &module.key));
                    assert_eq!(pending.site.span.bytes(), call.as_bytes());
                    assert_eq!(
                        pending.site.span,
                        &module.syntax.nodes[pending.site.node].span
                    );
                }
            },
        );
    }
}

#[test]
fn exclusive_capture_formation_does_not_grant_invocation_or_erasure() {
    use compiler::analysis::typed::TypedCompletion;

    const TEST: &str = "captures::exclusive_capture_formation_does_not_grant_invocation_or_erasure";
    probe_report(
        TEST,
        r#"
            function callOnce(seed: bool): [&mut bool, bool] {
                let mut flag = seed;
                let job: mut () => [&mut bool, bool] = () => [&mut flag, seed];
                let held: once () => [&mut bool, bool] = job;
                return held();
            }
            function callMutable(seed: bool): &mut bool {
                let mut flag = seed;
                let mut job: mut () => &mut bool = () => &mut flag;
                return job();
            }
        "#,
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none() && typed.bodies().findings().is_empty());
            let calls: Vec<_> = typed
                .result_use()
                .unwrap()
                .pending()
                .iter()
                .filter(|p| {
                    std::ptr::eq(p.site.module, &module.key)
                        && p.blocker == ResultUseBlocker::CallBoundary
                })
                .map(|p| p.site.span.bytes())
                .collect();
            assert_eq!(calls, [b"held()".as_slice(), b"job()"]);
        },
    );
    probe(
        TEST,
        r#"
            function localCaller(input: &mut bool): void {
                let job: once () => &mut bool = () => input;
                let _ = job;
            }
            function eraseCaller(input: &mut bool): once () => &mut bool {
                let job: once () => &mut bool = () => input;
                return job;
            }
            type Carrier<F> = { task: F };
            function eraseWrapped(input: &mut bool): Carrier<once () => &mut bool> {
                let job: once () => &mut bool = () => input;
                let packed: Carrier<once () => &mut bool> = Carrier { task: job };
                return packed;
            }
        "#,
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            check_local_capture_root(module, receipt, &graph, "localCaller", None);
            check_capture_blocker(module, receipt, &graph, "eraseCaller", "return job;");
            check_capture_blocker(module, receipt, &graph, "eraseWrapped", "packed");
        },
    );
}

#[test]
fn private_callable_transfer_keeps_backing_and_owns_string_cleanup() {
    let mut modes = Vec::new();
    probe(
        "captures::private_callable_transfer_keeps_backing_and_owns_string_cleanup",
        r#"
            function forward(job: once () => [&bool, string]): once () => [&bool, string] {
                let note = "epilogue";
                return job;
            }
            function after(seed: bool, text: string): void {
                let mut flag = seed;
                let held = forward(() => [&flag, text]);
                let _ = held;
                flag = true;
            }
            function overlap(seed: bool, text: string): void {
                let mut flag = seed;
                let held = forward(() => [&flag, text]);
                flag = true;
                let _ = held;
            }
        "#,
        |request, module, receipt| {
            let lower = |name| {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &declaration.id, AnalysisLimits::default()).unwrap()
            };
            let after = lower("after");
            modes.push((request.test_mode, after.completion()));
            if request.test_mode {
                assert_eq!(
                    modes,
                    [(false, Completion::Checked), (true, Completion::Checked)]
                );
            }
            let Some(mir) = after.mir() else { return };
            assert!(mir.cancellation().is_none());
            let ops = linear(mir, mir.entry);
            let layout = CallableLayout([
                CallableField::SharedPrimitive("bool"),
                CallableField::OwnedString,
            ]);
            let (call_pos, argument, result, body) = ops
                .iter()
                .enumerate()
                .find_map(|(i, op)| match op {
                    Operation::CallCallableTransfer {
                        callee,
                        argument,
                        into,
                        layout: bound,
                        body,
                    } => {
                        assert_eq!(*bound, layout);
                        assert_eq!(
                            *callee,
                            &module
                                .declarations
                                .iter()
                                .find(|d| d.name.as_deref() == Some("forward"))
                                .unwrap()
                                .id
                        );
                        Some((i, *argument, *into, body))
                    }
                    _ => None,
                })
                .expect("a real named transfer");
            let same = |a: Place<'_>, b: Place<'_>| {
                a.root.node == b.root.node && a.projection.is_none() && b.projection.is_none()
            };
            assert!(!same(argument, result));
            assert_eq!(
                ops[..call_pos]
                    .iter()
                    .filter(|op| matches!(op,
                Operation::Move { from, .. } if from.root.span.bytes() == b"text: string"))
                    .count(),
                1
            );
            let callee = linear(body, body.entry);
            let effects: Vec<_> = callee
                .iter()
                .copied()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Move { .. }
                            | Operation::Copy { .. }
                            | Operation::Cleanup(_)
                            | Operation::Drop(_)
                            | Operation::DropCapture { .. }
                            | Operation::Export(_)
                    ) && !matches!(op, Operation::Cleanup(actions) if actions.is_empty())
                })
                .collect();
            let [
                Operation::Move {
                    from,
                    into: Some(stage),
                },
                Operation::Cleanup(epilogue),
                Operation::Export(output),
            ] = effects.as_slice()
            else {
                panic!("expected only formal move, epilogue, and export")
            };
            assert_eq!(from.root.span.bytes(), b"job: once () => [&bool, string]");
            assert!(same(*stage, *output));
            assert!(
                matches!(epilogue.as_slice(), [Obligation::Drop(note)] if note.root.span.bytes() == b"note")
            );
            let mut consumed = Vec::new();
            for (i, op) in ops.iter().enumerate() {
                if let Operation::Cleanup(actions) = op {
                    for action in actions {
                        match action {
                            Obligation::ConsumeCallableTransfer {
                                operand,
                                layout: bound,
                            } => {
                                assert_eq!(*bound, layout);
                                consumed.push((i, *operand));
                            }
                            Obligation::Drop(_) | Obligation::DropCapture { .. } => {
                                panic!("duplicate caller drop")
                            }
                            _ => {}
                        }
                    }
                }
                assert!(!matches!(
                    op,
                    Operation::Drop(_) | Operation::DropCapture { .. }
                ));
            }
            let [(cleanup_pos, discarded)] = consumed.as_slice() else {
                panic!("consume returned owner once")
            };
            assert!(ops[call_pos + 1..*cleanup_pos].iter().any(|op| matches!(op,
                Operation::Move { from, into: Some(stage) } if same(*from, result) && same(*stage, *discarded)
            )));
            assert!(
                ops[*cleanup_pos + 1..]
                    .iter()
                    .any(|op| matches!(op, Operation::Install { target, .. }
                if target.root.span.bytes() == b"flag"))
            );
            assert!(
                !ops[..*cleanup_pos]
                    .iter()
                    .any(|op| matches!(op, Operation::Install { .. }))
            );
            let caller = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("after"))
                .unwrap();
            let used = after.statistics();
            let exact = AnalysisLimits {
                max_work: used.work,
                max_entries: used.entries,
                max_depth: used.max_depth,
            };
            let run = |limits| lower_body(receipt, module, &caller.id, limits).unwrap();
            assert!(run(exact).mir().is_some());
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
                let exhausted = run(short);
                assert_eq!(exhausted.completion(), Completion::ResourceLimit);
                assert!(exhausted.mir().is_none() && exhausted.findings().is_empty());
            }
            let overlap = lower("overlap");
            assert_eq!(overlap.completion(), Completion::SourceRejected);
            let [finding] = overlap.findings() else {
                panic!("expected an overlap finding")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.site.span.bytes(), b"flag = true;");
            assert_eq!(finding.related.span.bytes(), b"&flag");
            assert!(overlap.mir().is_none());
            assert_eq!(lower("forward").completion(), Completion::Incomplete);
        },
    );
}

#[test]
fn callable_transfer_withholds_unproved_environments_and_checks_escape() {
    probe(
        "captures::callable_transfer_withholds_unproved_environments_and_checks_escape",
        r#"
            function forward(job: once () => [&bool, bool]): once () => [&bool, bool] { return job; }
            function alternate(job: once () => [&bool, bool]): once () => [&bool, bool] {
                let note = "callee local"; let _ = &note; return job;
            }
            function diverge(): never { while (true) { continue; } }
            function different(job: once () => [&bool, bool]): once () => [&bool, bool] {
                let _ = job; return () => diverge();
            }
            function differentSameLayout(job: once () => [bool, bool]): once () => [bool, bool] {
                let _ = job;
                let flag = true;
                let other = false;
                return () => [flag, other];
            }
            function escaping(job: once () => [&bool, bool]): once () => [&bool, bool] {
                let local = true; let _ = job; return () => [&local, local];
            }
            function after(seed: bool): void {
                let mut flag = seed;
                let other = true;
                let job: once () => [&bool, bool] = () => [&flag, other];
                let held = forward(job);
                let _ = held;
                flag = true;
            }
            function alternateAfter(seed: bool): void {
                let mut flag = seed;
                let other = true;
                let held = alternate(() => [&flag, other]);
                let _ = held;
                flag = true;
            }
            function side(): void {}
            function independent(job: once () => [&bool, bool]): once () => [&bool, bool] {
                let mut gate = false; if (gate) { side(); return job; }
                let local = true; let _ = job; return () => [&local, local];
            }
            function beyond(job: once () => [&bool, bool]): once () => [&bool, bool] {
                side(); let local = true; let _ = job; return () => [&local, local];
            }
            function independentEscape(seed: bool): void {
                let bit = false; let _ = independent(() => [&seed, bit]);
            }
            function beyondCall(seed: bool): void {
                let bit = false; let _ = beyond(() => [&seed, bit]);
            }
            function escapesCaller(seed: bool): once () => [&bool, bool] {
                let flag = seed; let other = true; return forward(() => [&flag, other]);
            }
            function escapesCallee(seed: bool): void {
                let flag = seed; let other = true; let _ = escaping(() => [&flag, other]);
            }
            function unknown(job: once () => [&bool, bool]): void { let _ = forward(job); }
            function unknownCleanup(seed: bool): void {
                let flag = seed; let owned = "hidden";
                let _ = forward(() => { let _ = owned; return [&flag, flag]; });
            }
            function unprovedReturn(seed: bool): void {
                let mut flag = seed; let other = true; let held = different(() => [&flag, other]);
                flag = true; let _ = held;
            }
            function unprovedPair(seed: bool): void {
                let other = true; let _ = differentSameLayout(() => [seed, other]);
            }
        "#,
        |_, module, receipt| {
            let lower = |name| {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &declaration.id, AnalysisLimits::default()).unwrap()
            };
            for name in ["after", "alternateAfter"] {
                assert_eq!(lower(name).completion(), Completion::Checked, "{name}");
            }
            for name in [
                "unknown",
                "unknownCleanup",
                "unprovedReturn",
                "unprovedPair",
                "beyondCall",
            ] {
                let checked = lower(name);
                assert_eq!(checked.completion(), Completion::Incomplete, "{name}");
                assert!(checked.mir().is_none() && checked.findings().is_empty());
            }
            for (name, related) in [
                ("escapesCallee", &b"&local"[..]),
                ("escapesCaller", &b"&flag"[..]),
                ("independentEscape", &b"&local"[..]),
            ] {
                let checked = lower(name);
                assert_eq!(checked.completion(), Completion::SourceRejected, "{name}");
                let [finding] = checked.findings() else {
                    panic!("expected one escape finding")
                };
                assert_eq!(finding.code(), "R0202");
                assert_eq!(finding.related.span.bytes(), related);
                assert!(checked.mir().is_none());
            }
        },
    );
}

#[test]
fn callable_transfer_preflight_respects_argument_and_field_depth() {
    probe(
        "captures::callable_transfer_preflight_respects_argument_and_field_depth",
        r#"
function forward(job: once () => [&bool, bool]): once () => [&bool, bool] { return job; }
export function openForward(job: once () => [&bool, bool]): once () => [&bool, bool] { return job; }
function bare(flag: bool, bit: bool): void { let held = forward(() => [&flag, bit]); let _ = held; }
function grouped(flag: bool, bit: bool): void { let held = forward(((((((((((() => [&flag, bit]))))))))))); let _ = held; }
function unproved(flag: bool, bit: bool): void { let held = openForward(((((((((((() => [&flag, bit]))))))))))); let _ = held; }
function computed(flag: bool, bit: bool): void { let held = forward(() => [&flag, ((((((((((!bit))))))))))]); let _ = held; }
"#,
        |_, module, receipt| {
            let query = |name: &str, limits| {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &d.id, limits).unwrap()
            };
            let default = AnalysisLimits::default();
            let bare = query("bare", default);
            assert!(bare.mir().is_some());
            let limit = AnalysisLimits {
                max_depth: bare.statistics().max_depth,
                ..default
            };
            assert!(query("bare", limit).mir().is_some());
            for (name, expected) in [
                ("grouped", Completion::Checked),
                ("unproved", Completion::Incomplete),
                ("computed", Completion::Incomplete),
            ] {
                assert_eq!(query(name, default).completion(), expected, "{name}");
                let result = query(name, limit);
                assert_eq!(result.completion(), Completion::ResourceLimit, "{name}");
                assert!(result.mir().is_none());
            }
        },
    );
}

#[test]
fn transfer_does_not_prove_erased_invocation() {
    probe_report(
        "captures::transfer_does_not_prove_erased_invocation",
        r#"
            function invoke(job: once () => [&bool, bool]): [&bool, bool] { return job(); }
        "#,
        |_, module, typed| {
            assert!(typed.receipt().is_none());
            let pending = typed.result_use().unwrap().pending();
            assert!(
                pending
                    .iter()
                    .any(|p| p.blocker == ResultUseBlocker::CallBoundary
                        && p.site.module == &module.key
                        && p.site.span.bytes() == b"job()")
            );
        },
    );
}

#[test]
fn stored_pair_captures_transfer_backing_and_cleanup() {
    probe_with_typed(
        "captures::stored_pair_captures_transfer_backing_and_cleanup",
        r#"
            function owned(left: string, right: string, owner: string): void {
                let job: once () => [string, string] = () => [right, left];
                let moved = job;
            }
            function copied(text: string, flag: bool): bool {
                let job: once () => [string, bool] = () => [text, flag];
                return flag;
            }
            function movedInput(left: string, right: string): string {
                let job: once () => [string, string] = () => [left, right];
                return right;
            }
            function escaping(text: string, right: string): once () => [&string, string] {
                return () => [&text, right];
            }
            function escapingSecond(text: string, right: string): once () => [string, &string] {
                return () => [right, &text];
            }
            function replace(first: string, second: string, third: string, fourth: string): void {
                let mut job: once () => [string, string] = () => [first, second];
                job = () => [third, fourth];
            }
            function replaceMoved(first: string, second: string, third: string, fourth: string): void {
                let mut job: once () => [string, string] = () => [first, second];
                let old = job;
                job = () => [third, fourth];
            }
            function retireFirst(first: string, second: string): void {
                let job: () => [&string, &string] = () => [&first, &second];
                dispose(first);
                let _ = job;
            }
            function retireSecond(first: string, second: string): void {
                let job: () => [&string, &string] = () => [&first, &second];
                dispose(second);
                let _ = job;
            }
            function fromParameter(view: &string, text: string): once () => [&string, string] {
                return () => [view, text];
            }
            function sameScalar(flag: bool): bool {
                let job: () => [bool, bool] = () => [flag, flag];
                return flag;
            }
            function sameBorrow(text: string): void {
                let _: () => [&string, &string] = () => [&text, &text];
            }
            function grouped(flag: bool, text: string): bool {
                let job: once () => [bool, string] = () => [(flag), text];
                return flag;
            }
            function singleDiscard(text: string): void { let _ = () => text; }
            function singleReplace(first: string, second: string): void {
                let mut job = () => first;
                job = () => second;
            }
            function computed(text: string, flag: bool): void {
                let job: once () => [i32, string] = () => [payload(flag), text];
            }
            function payload(flag: bool): i32 { return 7i32; }
        "#,
        |_, module, receipt, _| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let check =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let lower = |name| check(name, AnalysisLimits::default());
            let owned = lower("owned");
            let mir = owned
                .mir()
                .unwrap_or_else(|| panic!("owned: {:?}", owned.completion()));
            let ops = linear(mir, mir.entry);
            let route: Vec<_> = ops
                .iter()
                .copied()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Move { .. }
                            | Operation::FormClosurePair { .. }
                            | Operation::Call(_)
                    )
                })
                .collect();
            let [
                Operation::Move {
                    from: right,
                    into: Some(first),
                },
                Operation::Move {
                    from: left,
                    into: Some(second),
                },
                Operation::FormClosurePair {
                    closure,
                    captures,
                    mode,
                },
                Operation::Move {
                    from: job,
                    into: Some(moved),
                },
            ] = route.as_slice()
            else {
                panic!("expected two capture moves, formation, and environment move")
            };
            assert_eq!(*mode, compiler::analysis::InvocationMode::Once);
            assert_eq!(right.root.span.bytes(), b"right: string");
            assert_eq!(left.root.span.bytes(), b"left: string");
            assert_eq!(captures[0].node, first.root.node);
            assert_eq!(captures[1].node, second.root.node);
            assert_eq!(closure.node, job.root.node);
            assert_eq!(moved.root.span.bytes(), b"moved");
            let drops = path_drops(mir, mir.entry);
            assert!(matches!(drops.as_slice(), [
                Obligation::DropCapture { closure: holder1, field: last },
                Obligation::DropCapture { closure: holder2, field: first },
                Obligation::Drop(owner),
            ] if holder1.node == moved.root.node && holder2.node == moved.root.node
                && last.node == captures[1].node && first.node == captures[0].node
                && owner.root.span.bytes() == b"owner: string"));

            let copied = lower("copied");
            let mir = copied
                .mir()
                .expect("copying the scalar does not consume the original");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::FormClosurePair { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: flag, into: Some(staged) },
                Operation::FormClosurePair { captures, .. },
                Operation::Copy { from: original, into: Some(result) },
                Operation::Export(exported),
            ] if flag.root.span.bytes() == b"flag: bool" && flag.root.node == original.root.node
                && captures[1].node == staged.root.node && result.root.node == exported.root.node));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::DropCapture { field, .. }]
                if field.span.bytes() == b"text")
            );

            let forwarded = lower("fromParameter");
            let mir = forwarded
                .mir()
                .expect("the reference's backing belongs to the caller");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::Move { .. }
                            | Operation::FormClosurePair { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: view, into: Some(staged) },
                Operation::Move { into: Some(text), .. },
                Operation::FormClosurePair { closure, captures, .. },
                Operation::Export(exported),
            ] if view.root.span.bytes() == b"view: &string" && captures[0].node == staged.root.node
                && captures[1].node == text.root.node && closure.node == exported.root.node));
            assert!(path_drops(mir, mir.entry).is_empty());

            let replace = lower("replace");
            let mir = replace
                .mir()
                .expect("replacement follows the environment owner");
            let mut events = Vec::new();
            let mut formed = None;
            for op in linear(mir, mir.entry) {
                match op {
                    Operation::FormClosurePair { closure, .. } => {
                        formed = Some(closure.node);
                        events.push(b"form".as_slice());
                    }
                    Operation::Install { incoming, target } => {
                        assert_eq!(Some(incoming.root.node), formed);
                        assert_eq!(target.root.span.bytes(), b"job");
                        events.push(b"install".as_slice());
                    }
                    Operation::Cleanup(actions) => {
                        events.extend(actions.iter().filter_map(|action| match action {
                            Obligation::DropCapture { closure, field } => {
                                assert_eq!(closure.span.bytes(), b"job");
                                Some(field.span.bytes())
                            }
                            _ => None,
                        }));
                    }
                    _ => {}
                }
            }
            assert_eq!(
                events,
                [
                    b"form".as_slice(),
                    b"form",
                    b"second",
                    b"first",
                    b"install",
                    b"fourth",
                    b"third"
                ]
            );

            let moved = lower("replaceMoved");
            let mir = moved
                .mir()
                .expect("moving the old environment prevents displacement cleanup");
            let drops: Vec<_> = path_drops(mir, mir.entry)
                .into_iter()
                .map(|action| {
                    let Obligation::DropCapture { closure, field } = action else {
                        panic!("expected only capture cleanup")
                    };
                    (closure.span.bytes(), field.span.bytes())
                })
                .collect();
            assert_eq!(
                drops,
                [
                    (b"old".as_slice(), b"second".as_slice()),
                    (b"old", b"first"),
                    (b"job", b"fourth"),
                    (b"job", b"third"),
                ]
            );

            let limits = AnalysisLimits {
                max_work: owned.statistics().work,
                max_entries: owned.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert_eq!(check("owned", limits).completion(), Completion::Checked);
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = check("owned", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }

            for (name, code, at, related) in [
                ("movedInput", "R0200", "right", "right"),
                (
                    "escaping",
                    "R0202",
                    "return () => [&text, right];",
                    "text: string",
                ),
                (
                    "escapingSecond",
                    "R0202",
                    "return () => [right, &text];",
                    "text: string",
                ),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one ownership error")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(
                    std::str::from_utf8(finding.site.span.bytes()),
                    Ok(at),
                    "{name} primary"
                );
                assert_eq!(
                    std::str::from_utf8(finding.related.span.bytes()),
                    Ok(related),
                    "{name} related"
                );
            }
            for (name, site, backing) in [
                (
                    "retireFirst",
                    b"dispose(first)".as_slice(),
                    b"&first".as_slice(),
                ),
                (
                    "retireSecond",
                    b"dispose(second)".as_slice(),
                    b"&second".as_slice(),
                ),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one live backing conflict")
                };
                assert_eq!(finding.code(), "R0203");
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), backing);
            }
            let repeated = lower("sameScalar");
            let mir = repeated
                .mir()
                .expect("the same scalar can supply both copyable positions");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. } | Operation::FormClosurePair { .. }
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: first, into: Some(staged1) },
                Operation::Copy { from: second, into: Some(staged2) },
                Operation::FormClosurePair { captures, .. },
                Operation::Copy { from: after, .. },
            ] if first.root.node == second.root.node && second.root.node == after.root.node
                && staged1.root.node == captures[0].node && staged2.root.node == captures[1].node));
            let loan = lower("sameBorrow");
            let mir = loan
                .mir()
                .expect("two shared loans of a stable place are compatible");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Borrow { .. } | Operation::FormClosurePair { .. }
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Borrow { source: first, holder: first_holder, exclusive: false },
                Operation::Borrow { source: second, holder: second_holder, exclusive: false },
                Operation::FormClosurePair { captures, .. },
            ] if first.root.node == second.root.node && first_holder.root.node == captures[0].node
                && second_holder.root.node == captures[1].node));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(text)]
                if text.root.span.bytes() == b"text: string")
            );
            let grouped = lower("grouped");
            let mir = grouped
                .mir()
                .expect("grouping does not change the capture acquisition");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::Move { .. }
                            | Operation::FormClosurePair { .. }
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: flag, into: Some(staged) },
                Operation::Move { into: Some(text), .. },
                Operation::FormClosurePair { captures, .. },
                Operation::Copy { from: after, .. },
            ] if flag.root.span.bytes() == b"flag: bool" && flag.root.node == after.root.node
                && staged.root.node == captures[0].node && text.root.node == captures[1].node));
            for (name, capture) in [
                ("singleDiscard", b"text".as_slice()),
                ("singleReplace", b"first".as_slice()),
            ] {
                let report = lower(name);
                let mir = report.mir().unwrap();
                let standalone: Vec<_> = linear(mir, mir.entry)
                    .into_iter()
                    .filter_map(|op| match op {
                        Operation::DropCapture { field, .. } => Some(field.span.bytes()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(standalone, [capture], "{name}");
            }
            let computed = lower("computed");
            assert_eq!(computed.completion(), Completion::Incomplete);
            assert!(computed.mir().is_none());
            assert_eq!(
                computed.blocked_at().unwrap().span.bytes(),
                b"() => [payload(flag), text]"
            );
        },
    );
}

#[test]
fn invokes_a_known_once_pair_and_transfers_its_contents() {
    probe_with_typed(
        "captures::invokes_a_known_once_pair_and_transfers_its_contents",
        r#"
            function direct(left: string, right: string): string {
                let job: once () => [string, string] = () => [right, left];
                let [first, _] = job();
                return first;
            }
            function moved(left: string, right: string): [string, string] {
                let job: once () => [string, string] = () => [left, right];
                let now = job;
                return now();
            }
            function selected(flag: bool, first: string, second: string, third: string, fourth: string): [string, string] {
                let mut job: once () => [string, string] = () => [first, second];
                if (flag) { job = () => [third, fourth]; }
                return job();
            }
            function selectedScalars(flag: bool, a: bool, b: bool, c: bool, d: bool): [bool, bool] {
                let mut job: once () => [bool, bool] = () => [a, b];
                if (flag) { job = () => [c, d]; }
                return job();
            }
            function consumed(left: string, right: string): [string, string] {
                let job: once () => [string, string] = () => [left, right];
                let result = job();
                let old = job;
                return result;
            }
            function repeated(left: string, right: string): [string, string] {
                let job: once () => [string, string] = () => [left, right];
                let _ = job();
                return job();
            }
            function groupedRepeat(left: &bool, right: string): [&bool, string] {
                let job: once () => [&bool, string] = () => [(left), (right)];
                let _ = (job)();
                return (job)();
            }
            function conditionalRepeat(flag: bool, first: string, second: string): [string, string] {
                let job: once () => [string, string] = () => [first, second];
                if (flag) { let _ = job(); }
                return job();
            }
            function restored(first: string, second: string, third: string, fourth: string): [string, string] {
                let mut job: once () => [string, string] = () => [first, second];
                let _ = job();
                job = () => [fourth, third];
                return job();
            }
            function liveReceiver(left: string, right: string): [string, string] {
                let job: once () => [string, string] = () => [left, right];
                let view = &job;
                let result = job();
                let _ = view;
                return result;
            }
            function deadReceiver(left: string, right: string): [string, string] {
                let job: once () => [string, string] = () => [left, right];
                let view = &job;
                let _ = view;
                return job();
            }
        "#,
        |request, module, receipt, typed| {
            use compiler::analysis::InvocationMode;
            use compiler::analysis::scalars::data::Access;

            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let check =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let lower = |name| check(name, AnalysisLimits::default());
            let direct = lower("direct");
            let mir = direct
                .mir()
                .unwrap_or_else(|| panic!("direct: {:?}", direct.completion()));
            let blocks = straight_blocks(mir, mir.entry);
            assert!(matches!(
                blocks.last().unwrap().terminator,
                Terminator::Return
            ));
            let route: Vec<_> = blocks
                .iter()
                .copied()
                .filter(|block| match &block.operation {
                    Operation::FormClosurePair { .. } | Operation::InvokeClosurePair { .. } => true,
                    Operation::Move { from, .. } => from.tuple_element().is_some(),
                    _ => false,
                })
                .collect();
            let [form, call, transfer] = route.as_slice() else {
                panic!("expected formation, invocation, and tuple transfer");
            };
            let (
                Operation::FormClosurePair {
                    closure,
                    captures,
                    mode,
                },
                Operation::InvokeClosurePair {
                    receiver,
                    origin,
                    into,
                },
                Operation::Move {
                    from,
                    into: Some(first),
                },
            ) = (&form.operation, &call.operation, &transfer.operation)
            else {
                panic!("expected the direct once invocation route");
            };
            assert_eq!(*mode, InvocationMode::Once);
            assert_eq!(closure.node, receiver.root.node);
            assert_eq!(origin.node, form.site.node);
            assert_eq!(origin.span.bytes(), b"() => [right, left]");
            assert_eq!(captures[0].span.bytes(), b"right");
            assert_eq!(captures[1].span.bytes(), b"left");
            assert_eq!(from.root.node, into.root.node);
            assert_eq!(from.tuple_element(), Some(0));
            assert_eq!(first.root.span.bytes(), b"first");
            assert_eq!(call.site.span.bytes(), b"job()");
            let typed_call = typed
                .bodies()
                .node(module, call.site.node)
                .unwrap()
                .unwrap();
            let invocation = typed_call
                .callable
                .as_ref()
                .unwrap()
                .invocation
                .as_ref()
                .unwrap();
            assert_eq!(invocation.mode, InvocationMode::Once);
            assert_eq!(invocation.access, Access::Owned);
            assert!(invocation.assumption.is_none());
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(unbound)]
                if unbound.root.node == into.root.node && unbound.tuple_element() == Some(1))
            );

            let moved = lower("moved");
            let mir = moved
                .mir()
                .expect("formation identity moves with its environment");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Move { .. }
                            | Operation::FormClosurePair { .. }
                            | Operation::InvokeClosurePair { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [_, _,
                Operation::FormClosurePair { closure, .. },
                Operation::Move { from, into: Some(moved) },
                Operation::InvokeClosurePair { receiver, into, origin },
                Operation::Export(output),
            ] if from.root.node == closure.node && moved.root.span.bytes() == b"now"
                && moved.root.node == receiver.root.node && into.root.node == output.root.node
                && origin.span.bytes() == b"() => [left, right]"));
            assert!(path_drops(mir, mir.entry).is_empty());

            let selected = lower("selected");
            let mir = selected
                .mir()
                .expect("each continuing path invokes the origin it stored");
            let (yes, no) = branch_targets(mir);
            for (edge, expected) in [
                (yes, b"() => [third, fourth]".as_slice()),
                (no, b"() => [first, second]".as_slice()),
            ] {
                let ops = linear(mir, edge);
                let route: Vec<_> = ops
                    .iter()
                    .copied()
                    .filter(|op| {
                        matches!(
                            op,
                            Operation::InvokeClosurePair { .. } | Operation::Export(_)
                        )
                    })
                    .collect();
                assert!(matches!(route.as_slice(), [
                    Operation::InvokeClosurePair { receiver, origin, into }, Operation::Export(output),
                ] if receiver.root.span.bytes() == b"job" && origin.span.bytes() == expected
                    && into.root.node == output.root.node));
            }
            assert!(matches!(path_drops(mir, yes).as_slice(), [
                Obligation::DropCapture { field: second, .. }, Obligation::DropCapture { field: first, .. },
            ] if second.span.bytes() == b"second" && first.span.bytes() == b"first"));
            assert!(
                matches!(path_drops(mir, no).as_slice(), [Obligation::Drop(fourth), Obligation::Drop(third)]
                if fourth.root.span.bytes() == b"fourth: string" && third.root.span.bytes() == b"third: string")
            );

            let scalar = lower("selectedScalars");
            let mir = scalar
                .mir()
                .expect("origin survives joins even without loans or drops");
            let (yes, no) = branch_targets(mir);
            for (edge, expected) in [
                (yes, b"() => [c, d]".as_slice()),
                (no, b"() => [a, b]".as_slice()),
            ] {
                let calls: Vec<_> = linear(mir, edge)
                    .into_iter()
                    .filter(|op| matches!(op, Operation::InvokeClosurePair { .. }))
                    .collect();
                assert!(
                    matches!(calls.as_slice(), [Operation::InvokeClosurePair { origin, .. }]
                    if origin.span.bytes() == expected)
                );
                assert!(path_drops(mir, edge).is_empty());
            }
            let limits = AnalysisLimits {
                max_work: selected.statistics().work,
                max_entries: selected.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert_eq!(check("selected", limits).completion(), Completion::Checked);
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let report = check("selected", short);
                assert_eq!(report.completion(), Completion::ResourceLimit);
                assert!(report.mir().is_none());
            }
            let consumed = lower("consumed");
            assert_eq!(consumed.completion(), Completion::SourceRejected);
            assert!(consumed.mir().is_none());
            let [finding] = consumed.findings() else {
                panic!("expected use after invocation")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"job");
            assert_eq!(finding.related.span.bytes(), b"job()");
            for (name, site) in [
                ("repeated", b"job()".as_slice()),
                ("conditionalRepeat", b"job()".as_slice()),
                ("groupedRepeat", b"(job)()".as_slice()),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected a consumed receiver")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), site);
                assert!(finding.related.span.start() < finding.site.span.start());
            }
            let restored = lower("restored");
            let mir = restored
                .mir()
                .expect("replacing a consumed binding installs a new environment");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::FormClosurePair { .. }
                            | Operation::InvokeClosurePair { .. }
                            | Operation::Install { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::FormClosurePair { closure: old, .. },
                Operation::InvokeClosurePair { receiver: first, origin: first_origin, into: discarded },
                Operation::FormClosurePair { closure: new, .. },
                Operation::Install { target, incoming },
                Operation::InvokeClosurePair { receiver: second, origin: second_origin, into },
                Operation::Export(output),
            ] if first.root.node == old.node && second.root.node == target.root.node && target.root.node == old.node
                && incoming.root.node == new.node && first_origin.span.bytes() == b"() => [first, second]"
                && second_origin.span.bytes() == b"() => [fourth, third]" && into.root.node == output.root.node
                && discarded.root.node != into.root.node));
            let [
                _,
                Operation::InvokeClosurePair {
                    into: discarded, ..
                },
                ..,
            ] = route.as_slice()
            else {
                unreachable!()
            };
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(last), Obligation::Drop(first)]
                if last.root.node == discarded.root.node && first.root.node == discarded.root.node
                    && last.tuple_element() == Some(1) && first.tuple_element() == Some(0))
            );
            let borrowed = lower("liveReceiver");
            assert_eq!(borrowed.completion(), Completion::SourceRejected);
            assert!(borrowed.mir().is_none());
            let [finding] = borrowed.findings() else {
                panic!("expected a live receiver loan conflict")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.site.span.bytes(), b"job()");
            assert_eq!(finding.related.span.bytes(), b"&job");
            let released = lower("deadReceiver");
            let mir = released
                .mir()
                .expect("a dead loan does not prevent consuming invocation");
            assert!(linear(mir, mir.entry).iter().any(
                |op| matches!(op, Operation::InvokeClosurePair { receiver, .. }
                if receiver.root.span.bytes() == b"job")
            ));
            assert!(path_drops(mir, mir.entry).is_empty());
            let type_limits = AnalysisLimits {
                max_work: typed.statistics().work,
                max_entries: typed.statistics().entries,
                ..AnalysisLimits::default()
            };
            let graph = loader::load_installed(request.clone()).unwrap();
            let type_check = |limits| analyze_typed(&graph, limits);
            assert!(type_check(type_limits).receipt().is_some());
            for short in [
                AnalysisLimits {
                    max_work: type_limits.max_work - 1,
                    ..type_limits
                },
                AnalysisLimits {
                    max_entries: type_limits.max_entries - 1,
                    ..type_limits
                },
            ] {
                let report = type_check(short);
                assert_eq!(
                    report.completion(),
                    compiler::analysis::typed::TypedCompletion::ResourceLimit
                );
                assert!(report.receipt().is_none());
            }
        },
    );
}

fn read_only_pair_results<'g>(
    module: &'g loader::Module,
    typed: &TypedReport<'g>,
    mir: &Mir<'g>,
    entry: usize,
    written: [&str; 2],
    forms: [CaptureForm; 2],
) -> [Place<'g>; 2] {
    use compiler::analysis::InvocationMode;

    let path = straight_blocks(mir, entry);
    assert!(matches!(
        path.last().unwrap().terminator,
        Terminator::Return
    ));
    let route: Vec<_> = path
        .iter()
        .copied()
        .filter(|block| {
            matches!(
                block.operation,
                Operation::FormClosurePair { .. }
                    | Operation::InvokeReadOnlyClosurePair { .. }
                    | Operation::InvokeClosurePair { .. }
            )
        })
        .collect();
    let [formation, first, second] = route.as_slice() else {
        panic!("expected one environment and two invocations")
    };
    let Operation::FormClosurePair {
        closure,
        captures,
        mode: InvocationMode::ReadOnly,
    } = &formation.operation
    else {
        panic!("expected formation of a read-only pair")
    };
    let fact = |node| typed.bodies().node(module, node).unwrap().unwrap();
    let declaration_at = |start, name| {
        module
            .declarations
            .iter()
            .find(|d| d.id.start == start && d.name.as_deref() == Some(name))
            .unwrap()
    };
    let binding = declaration_at(closure.span.start(), "job");
    let results = [first, second].map(|block| {
        let Operation::InvokeReadOnlyClosurePair {
            receiver,
            origin,
            into,
        } = block.operation
        else {
            panic!("expected a nonconsuming invocation")
        };
        assert_eq!(receiver.root.node, closure.node);
        assert_eq!(origin.node, formation.site.node);
        assert_eq!(origin.module, &module.key);
        assert_eq!(block.site.span.bytes(), b"job()");
        let invocation = fact(block.site.node)
            .callable
            .as_ref()
            .unwrap()
            .invocation
            .as_ref()
            .unwrap();
        assert_eq!(invocation.mode, InvocationMode::ReadOnly);
        assert_eq!(invocation.access, Access::Owned);
        assert!(invocation.assumption.is_none());
        let receiver_path = invocation.path.as_ref().unwrap();
        assert_eq!(receiver_path.root, &binding.id);
        assert!(receiver_path.projections.is_empty());
        let arrow = fact(invocation.receiver)
            .callable
            .as_ref()
            .unwrap()
            .arrow
            .as_ref()
            .unwrap();
        assert_eq!(arrow.site.node, origin.node);
        assert_eq!(arrow.site.module, origin.module);
        assert_eq!(
            arrow.site.span.bytes(),
            format!("() => [{}, {}]", written[0], written[1]).as_bytes()
        );
        assert_eq!(arrow.captures.len(), 2);
        for ((name, form), capture) in written.into_iter().zip(forms).zip(captures) {
            assert_eq!(capture.span.bytes(), name.as_bytes());
            let fact = arrow
                .captures
                .iter()
                .find(|fact| fact.site.node == capture.node)
                .unwrap();
            assert_eq!(fact.form, form);
            assert!(fact.reachable);
            assert!(fact.path.projections.is_empty());
            let source = path
                .iter()
                .find_map(|step| match step.operation {
                    Operation::Copy {
                        from,
                        into: Some(stage),
                    } if stage.root.node == capture.node
                        && !matches!(form, CaptureForm::ExplicitBorrow { .. }) =>
                    {
                        Some(from.root.span.start())
                    }
                    Operation::Borrow {
                        source,
                        holder,
                        exclusive: false,
                    } if holder.root.node == capture.node
                        && form == (CaptureForm::ExplicitBorrow { mutable: false }) =>
                    {
                        Some(source.root.span.start())
                    }
                    _ => None,
                })
                .expect("each written tuple position has its own capture stage");
            assert_eq!(
                fact.path.root,
                &declaration_at(source, name.trim_start_matches('&')).id
            );
        }
        into
    });
    assert_ne!(results[0].root.node, results[1].root.node);
    assert!(path.iter().all(|step| !matches!(step.operation,
        Operation::Move { from, .. } if from.root.node == closure.node
    )));
    assert!(path_drops(mir, entry).is_empty());
    let ends = path.iter().flat_map(|step| match &step.operation {
        Operation::Cleanup(actions) => actions.as_slice(),
        _ => &[],
    }).filter(|action| matches!(action, Obligation::EndStorage(place) if place.root.node == closure.node));
    assert_eq!(ends.count(), 1);
    results
}

#[test]
fn invokes_a_known_read_only_scalar_pair_twice() {
    probe_with_typed(
        "captures::invokes_a_known_read_only_scalar_pair_twice",
        r#"
            function twice(left: bool, right: bool): [bool, bool] {
                let job: () => [bool, bool] = () => [left, right];
                let first = job();
                return job();
            }
            function onceTwin(left: bool, right: bool): [bool, bool] {
                let job: once () => [bool, bool] = () => [left, right];
                let first = job();
                return job();
            }
        "#,
        |_, module, receipt, typed| {
            let lower = |name| {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &declaration.id, AnalysisLimits::default()).unwrap()
            };
            let report = lower("twice");
            let mir = report.mir().unwrap_or_else(|| {
                panic!(
                    "twice: {:?} at {:?}",
                    report.completion(),
                    report
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                )
            });
            let [first, second] = read_only_pair_results(
                module,
                typed,
                mir,
                mir.entry,
                ["left", "right"],
                [CaptureForm::Value; 2],
            );
            let ops = linear(mir, mir.entry);
            assert_eq!(first.root.span.bytes(), b"first");
            assert!(ops.iter().any(
                |op| matches!(op, Operation::Export(result) if result.root.node == second.root.node)
            ));

            let once = lower("onceTwin");
            assert_eq!(once.completion(), Completion::SourceRejected);
            assert!(once.mir().is_none());
            let [finding] = once.findings() else {
                panic!("expected exactly one moved receiver finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"job()");
            assert_eq!(finding.related.span.bytes(), b"job()");
            assert!(finding.related.span.start() < finding.site.span.start());
        },
    );
}

#[test]
fn invokes_a_known_mutable_scalar_cell_twice() {
    use compiler::analysis::InvocationMode;

    probe_with_typed(
        "captures::invokes_a_known_mutable_scalar_cell_twice",
        r#"
            function counter(): [i32, i32, i32] {
                let mut initial = 0i32;
                let mut next: mut () => i32 = () => {
                    initial += 1i32;
                    return initial;
                };
                let first = next();
                let second = next();
                return [first, second, initial];
            }
        "#,
        |request, module, receipt, typed| {
            let report = lower_capture_root(module, receipt, "counter");
            let stats = report.statistics();
            let limits = AnalysisLimits {
                max_work: stats.work,
                max_entries: stats.entries,
                max_depth: stats.max_depth,
            };
            let counter = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("counter"))
                .unwrap();
            let query = |limits| lower_body(receipt, module, &counter.id, limits).unwrap();
            assert!(query(limits).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: stats.work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: stats.entries - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_depth: stats.max_depth - 1,
                    ..limits
                },
            ] {
                let result = query(short);
                assert_eq!(result.completion(), Completion::ResourceLimit);
                assert!(result.mir().is_none() && result.findings().is_empty());
            }
            let mir = report.mir().unwrap_or_else(|| {
                panic!(
                    "mode {}: {:?} at {:?}",
                    request.test_mode,
                    report.completion(),
                    report
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                )
            });
            assert_eq!(report.completion(), Completion::Checked);
            let path = straight_blocks(mir, mir.entry);
            assert!(matches!(
                path.last().unwrap().terminator,
                Terminator::Return
            ));
            let formations: Vec<_> = path
                .iter()
                .enumerate()
                .filter_map(|(index, block)| match block.operation {
                    Operation::FormClosure {
                        closure,
                        capture,
                        mode: InvocationMode::Mutable,
                    } => Some((index, block.site, closure, capture)),
                    _ => None,
                })
                .collect();
            let &[(formed_at, origin, closure, capture)] = formations.as_slice() else {
                panic!("expected one mutable environment");
            };
            let Operation::Copy {
                from: initial,
                into: Some(stage),
            } = path[formed_at - 1].operation
            else {
                panic!("expected formation to copy the outer scalar");
            };
            assert_eq!(initial.root.span.bytes(), b"initial");
            assert_eq!(stage.root.node, capture.node);
            assert_ne!(initial.root.node, capture.node);
            let fact = |node| typed.bodies().node(module, node).unwrap().unwrap();
            let calls: Vec<_> = path[formed_at + 1..]
                .iter()
                .filter_map(|block| {
                    let Operation::InvokeMutableI32Cell {
                        receiver,
                        origin: actual,
                        field,
                        addend,
                        into,
                    } = block.operation
                    else {
                        return None;
                    };
                    assert_eq!(receiver.root.node, closure.node);
                    assert_eq!((actual.module, actual.node), (origin.module, origin.node));
                    assert_eq!((field.module, field.node), (capture.module, capture.node));
                    assert_eq!(addend, 1);
                    let call = fact(block.site.node);
                    let invocation = call.callable.as_ref().unwrap().invocation.as_ref().unwrap();
                    assert_eq!(invocation.mode, InvocationMode::Mutable);
                    assert_eq!(invocation.access, Access::Owned);
                    assert!(
                        fact(invocation.receiver)
                            .callable
                            .as_ref()
                            .and_then(|f| f.arrow.as_ref())
                            .is_none()
                    );
                    Some(into)
                })
                .collect();
            let [first, second] = calls.as_slice() else {
                panic!("expected two exclusive updates")
            };
            assert_ne!(first.root.node, second.root.node);
            assert_eq!(first.root.span.bytes(), b"first");
            assert_eq!(second.root.span.bytes(), b"second");
            let output = path
                .iter()
                .find_map(|block| match block.operation {
                    Operation::Export(output) => Some(output),
                    _ => None,
                })
                .unwrap();
            let members: Vec<_> = path
                .iter()
                .filter_map(|block| match block.operation {
                    Operation::Copy {
                        from,
                        into: Some(into),
                    } if into.root.node == output.root.node => {
                        Some((from.root.node, into.tuple_element().unwrap()))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                members,
                [
                    (first.root.node, 0),
                    (second.root.node, 1),
                    (initial.root.node, 2)
                ]
            );
            assert!(path.iter().all(|block| !matches!(block.operation,
                Operation::Move { from, .. } if [closure.node, initial.root.node].contains(&from.root.node)
            ) && !matches!(block.operation,
                Operation::Install { target, .. } if target.root.node == initial.root.node
            )));
            let ends = path.iter().flat_map(|block| match &block.operation {
                Operation::Cleanup(actions) => actions.as_slice(),
                _ => &[],
            }).filter(|action| matches!(action, Obligation::EndStorage(place) if place.root.node == closure.node));
            assert_eq!(ends.count(), 1);
            assert!(path_drops(mir, mir.entry).is_empty());
        },
    );
}

fn lower_capture_root<'g>(
    module: &'g loader::Module,
    receipt: &TypedReceipt<'_, 'g>,
    name: &str,
) -> compiler::analysis::ownership::Report<'g> {
    let declaration = module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some(name))
        .unwrap();
    lower_body(receipt, module, &declaration.id, AnalysisLimits::default()).unwrap()
}

#[test]
fn mutable_cell_receiver_checks_and_arithmetic_limit() {
    probe(
        "captures::mutable_cell_receiver_checks_and_arithmetic_limit",
        r#"
            function lastUse(): i32 {
                let mut seed = 4i32;
                let mut job: mut () => i32 = () => { seed += 3i32; return seed; };
                let unused = &job;
                let first = job();
                let view = &job;
                let used = view;
                return job();
            }
            function overlap(): i32 {
                let mut seed = 4i32;
                let mut job: mut () => i32 = () => { seed += 3i32; return seed; };
                let view = &job;
                let value = job();
                let used = view;
                return value;
            }
            function callsOverlap(): i32 { return overlap(); }
            function joinedCounter(choose: bool): [i32, i32] {
                let mut a = 2i32;
                let mut b = 7i32;
                let mut job: mut () => i32 = () => { a += 3i32; return a; };
                if (choose) { job = () => { b += 4i32; return b; }; }
                let first = job(); let second = job(); return [first, second];
            }
            function retarget(): [i32, i32, i32] {
                let mut seed = 4i32;
                let mut job: mut () => i32 = () => { seed += 3i32; return seed; };
                let a = job();
                let mut other = job;
                let b = other();
                other = () => { seed += 5i32; return seed; };
                return [a, b, other()];
            }
            function lostOrigin(): i32 {
                let mut seed = 4i32;
                let mut job: mut () => i32 = () => { seed += 3i32; return seed; };
                job = () => seed;
                return job();
            }
            function onceInterface(): i32 {
                let mut seed = 4i32;
                let mut job: mut () => i32 = () => { seed += 3i32; return seed; };
                let other: once () => i32 = job;
                return other();
            }
            function originalCounter(): i32 {
                let mut initial = 0i32;
                let mut next: mut () => i32 = () => { initial += 1i32; return initial; };
                let first = next();
                let second = next();
                return first * 10i32 + second;
            }
            expect originalCounter() == 12i32;
        "#,
        |_, module, receipt| {
            let lower = |name| lower_capture_root(module, receipt, name);
            let last_use = lower("lastUse");
            assert_eq!(last_use.completion(), Completion::Checked);
            let mir = last_use.mir().unwrap();
            assert_eq!(
                linear(mir, mir.entry)
                    .iter()
                    .filter(|op| matches!(op, Operation::InvokeMutableI32Cell { .. }))
                    .count(),
                2
            );

            let overlap = lower("overlap");
            assert_eq!(overlap.completion(), Completion::SourceRejected);
            assert!(overlap.mir().is_none());
            let [finding] = overlap.findings() else {
                panic!("expected the live receiver borrow to conflict")
            };
            assert_eq!(finding.code(), "R0201");
            assert_eq!(finding.site.span.bytes(), b"job()");
            assert_eq!(finding.related.span.bytes(), b"&job");

            let caller = lower("callsOverlap");
            assert_eq!(caller.completion(), Completion::SourceRejected);
            assert!(caller.mir().is_none());
            let [reached] = caller.findings() else {
                panic!("expected callee receiver conflict")
            };
            assert_eq!(
                (reached.code(), reached.site.node, reached.related.node),
                (finding.code(), finding.site.node, finding.related.node)
            );
            let graph = compiler::analysis::ownership::analyze_ownership(
                receipt,
                AnalysisLimits::default(),
            );
            for name in ["overlap", "callsOverlap"] {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == decl.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::SourceRejected);
                assert!(root.mir().is_none());
            }
            assert_eq!(
                graph
                    .findings()
                    .iter()
                    .filter(|f| f.code() == "R0201"
                        && f.site.module == &module.key
                        && f.site.node == finding.site.node
                        && f.related.node == finding.related.node)
                    .count(),
                1
            );

            let joined = lower("joinedCounter");
            let mir = joined
                .mir()
                .expect("the receiver retains its path-specific cell");
            let prefix = straight_blocks(mir, mir.entry);
            let formation = |blocks: &[&compiler::analysis::ownership::Block<'_>]| {
                blocks.iter().find_map(|b| match b.operation {
                    Operation::FormClosure { capture, .. } => Some((b.site.node, capture.node)),
                    _ => None,
                })
            };
            let first = formation(&prefix).unwrap();
            let (yes, no) = branch_targets(mir);
            for (arm, replaced) in [(yes, true), (no, false)] {
                let path = straight_blocks(mir, arm);
                let new = formation(&path);
                assert_eq!(new.is_some(), replaced);
                let expected = new.unwrap_or(first);
                let updates: Vec<_> = path
                    .iter()
                    .filter_map(|b| match b.operation {
                        Operation::InvokeMutableI32Cell {
                            receiver,
                            origin,
                            field,
                            addend,
                            ..
                        } => {
                            assert_eq!(receiver.root.span.bytes(), b"job");
                            Some(((origin.node, field.node), addend))
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(updates, [(expected, if replaced { 4 } else { 3 }); 2]);
            }

            let retarget = lower("retarget");
            let mir = retarget
                .mir()
                .expect("the cell follows the move and replacement");
            let path = straight_blocks(mir, mir.entry);
            let origins: Vec<_> = path
                .iter()
                .filter_map(|block| match block.operation {
                    Operation::FormClosure {
                        closure, capture, ..
                    } => Some((closure.node, block.site.node, capture.node)),
                    _ => None,
                })
                .collect();
            let [
                (source, original, original_field),
                (_, replacement, replacement_field),
            ] = origins.as_slice()
            else {
                panic!("expected the original and replacement environments")
            };
            assert_ne!(original, replacement);
            let calls: Vec<_> = path
                .iter()
                .filter_map(|block| match block.operation {
                    Operation::InvokeMutableI32Cell {
                        receiver,
                        origin,
                        field,
                        addend,
                        ..
                    } => Some((receiver.root.node, origin.node, field.node, addend)),
                    _ => None,
                })
                .collect();
            let [first, second, third] = calls.as_slice() else {
                panic!("expected three cell calls")
            };
            let destination = second.0;
            assert_ne!(*source, destination);
            assert_eq!(*first, (*source, *original, *original_field, 3));
            assert_eq!(*second, (destination, *original, *original_field, 3));
            assert_eq!(*third, (destination, *replacement, *replacement_field, 5));
            assert!(path.iter().any(|block| matches!(block.operation,
                Operation::Move { from, into: Some(into) } if from.root.node == *source && into.root.node == destination
            )));

            // M4 admits all three roots. M5 cannot invoke a ReadOnly formation
            // as a mutable cell, lower its once interface, or lower ordinary arithmetic.
            for (name, site) in [
                ("lostOrigin", b"job()".as_slice()),
                ("onceInterface", b"other()"),
                ("originalCounter", b"first * 10i32 + second"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), site);
            }
        },
    );
}

#[test]
fn mutable_cell_preflight_checks_depth_before_inspecting_the_update() {
    probe(
        "captures::mutable_cell_preflight_checks_depth_before_inspecting_the_update",
        r#"
function f(): i32 {
    let mut seed = 0i32;
    let job: mut () => i32 = () => { seed -= 1i32; return seed; };
    return seed;
}
"#,
        |_, module, receipt| {
            let decl = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("f"))
                .unwrap();
            let query = |limits| lower_body(receipt, module, &decl.id, limits).unwrap();
            let unsupported = query(AnalysisLimits::default());
            assert_eq!(unsupported.completion(), Completion::Incomplete);
            assert!(unsupported.mir().is_none());
            let short = query(AnalysisLimits {
                max_depth: 4,
                ..Default::default()
            });
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(short.mir().is_none() && short.blocked_at().is_none());
        },
    );
}

#[test]
fn moved_mutable_cell_has_no_typed_receipt() {
    use compiler::analysis::typed::{ResultUseBlocker, TypedCompletion};

    probe_report(
        "captures::moved_mutable_cell_has_no_typed_receipt",
        r#"
            function moved(): i32 {
                let mut seed = 4i32;
                let mut job: mut () => i32 = () => { seed += 3i32; return seed; };
                let other = job;
                return job();
            }
        "#,
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            let [pending] = typed.result_use().unwrap().pending() else {
                panic!("expected M4 to block invocation of the moved receiver")
            };
            assert_eq!(pending.blocker, ResultUseBlocker::CallBoundary);
            assert_eq!(pending.site.span.bytes(), b"job()");
        },
    );
}

#[test]
fn repeated_descriptor_pairs_keep_backing_through_last_use() {
    probe_with_typed(
        "captures::repeated_descriptor_pairs_keep_backing_through_last_use",
        r#"
            function views(left: &bool, right: &bool): [&bool, &bool] {
                let job: () => [&bool, &bool] = () => [left, right];
                let first = job();
                return job();
            }
            function overlap(seed: &bool): bool {
                let mut flag = true;
                let left = &flag;
                let job: () => [&bool, &bool] = () => [left, seed];
                let first = job(); flag = false; let again = job(); return *again[0];
            }
            function after(seed: &bool): bool {
                let mut flag = true;
                let left = &flag;
                let job: () => [&bool, &bool] = () => [left, seed];
                let first = job(); let again = job(); let scalar = *again[0]; flag = false; return scalar;
            }
            function mixed(bit: bool, view: &bool): [bool, &bool] {
                let job: () => [&bool, bool] = () => [view, bit];
                let [_, saved] = job();
                let [carried, _] = job();
                return [saved, carried];
            }
            function liveFirst(seed: &bool): bool {
                let mut flag = true;
                let left = &flag;
                let job: () => [&bool, &bool] = () => [seed, left];
                let first = job(); let _ = job(); flag = false; return *first[1];
            }
            function selected(choose: bool, a: &bool, b: &bool): bool {
                let mut flag = true;
                let view = &flag;
                if (choose) {
                    let job: () => [&bool, &bool] = () => [view, b];
                    let observing = &job;
                    let first = job(); let again = job();
                    let _ = observing;
                    let answer = *again[0]; flag = false; return answer;
                }
                flag = false;
                let job: () => [&bool, &bool] = () => [b, a];
                let first = job(); let again = job(); return *again[1];
            }
        "#,
        |_, module, receipt, typed| {
            let check = |name, limits| {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &declaration.id, limits).unwrap()
            };
            let lower = |name| check(name, AnalysisLimits::default());
            let shared = CaptureForm::CarriedReference { mutable: false };
            let views = lower("views");
            let mir = views
                .mir()
                .expect("descriptor results carry caller backing, not a loan of job");
            let [_, returned] = read_only_pair_results(
                module,
                typed,
                mir,
                mir.entry,
                ["left", "right"],
                [shared; 2],
            );
            assert!(linear(mir, mir.entry).iter().any(|op| matches!(op, Operation::Export(result) if result.root.node == returned.root.node)));

            let mixed = lower("mixed");
            let mir = mixed
                .mir()
                .expect("independent scalar and descriptor stages can be copied");
            let [first, second] = read_only_pair_results(
                module,
                typed,
                mir,
                mir.entry,
                ["view", "bit"],
                [shared, CaptureForm::Value],
            );
            let ops = linear(mir, mir.entry);
            for (result, position, name) in
                [(first, 1, b"saved".as_slice()), (second, 0, b"carried")]
            {
                assert!(ops.iter().any(
                    |op| matches!(op, Operation::Copy { from, into: Some(binding) }
                    if from.root.node == result.root.node && from.tuple_element() == Some(position)
                    && binding.root.span.bytes() == name)
                ));
            }

            for name in ["overlap", "liveFirst"] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none());
                let [finding] = rejected.findings() else {
                    panic!("expected one backing conflict in {name}")
                };
                assert_eq!(finding.code(), "R0203", "{name}");
                assert_eq!(finding.site.span.bytes(), b"flag = false;", "{name}");
                assert_eq!(finding.related.span.bytes(), b"&flag", "{name}");
            }
            let after = lower("after");
            let mir = after
                .mir()
                .expect("the carried loan ends after the last descriptor use");
            let [_, again] = read_only_pair_results(
                module,
                typed,
                mir,
                mir.entry,
                ["left", "seed"],
                [shared; 2],
            );
            let last_use = |mir: &Mir<'_>, entry, result: Place<'_>, position| {
                let path = straight_blocks(mir, entry);
                let read = path.iter().position(|step| matches!(step.operation,
                    Operation::Dereference(from) if from.root.node == result.root.node && from.tuple_element() == Some(position)
                )).expect("dereference the written result position");
                let write = path
                    .iter()
                    .position(|step| {
                        matches!(step.operation,
                            Operation::Install { target, .. } if target.root.span.bytes() == b"flag"
                        )
                    })
                    .expect("install the replacement of flag");
                (read, write)
            };
            let (read, write) = last_use(mir, mir.entry, again, 0);
            assert!(read < write);

            let selected = lower("selected");
            let mir = selected.mir().unwrap_or_else(|| {
                panic!(
                    "selected: {:?} at {:?}",
                    selected.completion(),
                    selected
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                )
            });
            let (yes, no) = branch_targets(mir);
            let [_, yes_result] =
                read_only_pair_results(module, typed, mir, yes, ["view", "b"], [shared; 2]);
            let [_, no_result] =
                read_only_pair_results(module, typed, mir, no, ["b", "a"], [shared; 2]);
            let (read, write) = last_use(mir, yes, yes_result, 0);
            assert!(read < write);
            let (read, write) = last_use(mir, no, no_result, 1);
            assert!(write < read);

            let limits = AnalysisLimits {
                max_work: after.statistics().work,
                max_entries: after.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(check("after", limits).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let report = check("after", short);
                assert_eq!(report.completion(), Completion::ResourceLimit);
                assert!(report.mir().is_none());
            }
        },
    );
}

#[test]
fn repeatable_calls_use_the_reaching_environment_after_a_join() {
    probe_with_typed(
        "captures::repeatable_calls_use_the_reaching_environment_after_a_join",
        r#"
function joined(choose: bool, a: bool, b: bool): [bool, bool] {
    let mut job: () => [bool, bool] = () => [a, b];
    if (choose) { job = () => [b, a]; }
    let first = job(); return job();
}
function retainsAfterJoin(choose: bool, seed: &bool, bit: bool): bool {
    let mut flag = true;
    let mut job: () => [&bool, bool] = () => [seed, bit];
    if (choose) { job = () => [&flag, bit]; }
    let first = job(); flag = false; let second = job(); return *second[0];
}
"#,
        |_, module, receipt, _| {
            let find = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower = |name| {
                lower_body(receipt, module, &find(name).id, AnalysisLimits::default()).unwrap()
            };
            let joined = lower("joined");
            let mir = joined
                .mir()
                .expect("both incoming environments have checked formations");
            let prefix = straight_blocks(mir, mir.entry);
            let (original, slot) = prefix
                .iter()
                .find_map(|block| match block.operation {
                    Operation::FormClosurePair {
                        closure, captures, ..
                    } => {
                        assert_eq!(captures.map(|s| s.span.bytes()), [b"a", b"b"]);
                        Some((block.site.node, closure.node))
                    }
                    _ => None,
                })
                .unwrap();
            let (yes, no) = branch_targets(mir);
            for (arm, replaced) in [(yes, true), (no, false)] {
                let path = straight_blocks(mir, arm);
                let replacement = path.iter().find_map(|block| match block.operation {
                    Operation::FormClosurePair { captures, .. } => {
                        assert_eq!(captures.map(|s| s.span.bytes()), [b"b", b"a"]);
                        Some(block.site.node)
                    }
                    _ => None,
                });
                assert_eq!(replacement.is_some(), replaced);
                let origin = replacement.unwrap_or(original);
                let calls: Vec<_> = path
                    .iter()
                    .filter_map(|block| match block.operation {
                        Operation::InvokeReadOnlyClosurePair {
                            receiver,
                            origin: actual,
                            into,
                        } => {
                            assert_eq!((receiver.root.node, actual.node), (slot, origin));
                            Some(into.root.node)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(calls.len(), 2);
                assert_ne!(calls[0], calls[1]);
                assert!(path.iter().any(|block| matches!(block.operation, Operation::Export(place) if place.root.node == calls[1])));
            }
            let bad = lower("retainsAfterJoin");
            assert_eq!(bad.completion(), Completion::SourceRejected);
            assert!(bad.mir().is_none());
            let [finding] = bad.findings() else {
                panic!("expected the replacement's loan")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.site.span.bytes(), b"flag = false;");
            assert_eq!(finding.related.span.bytes(), b"&flag");
            let graph = compiler::analysis::ownership::analyze_ownership(
                receipt,
                AnalysisLimits::default(),
            );
            for (name, expected) in [
                ("joined", Completion::Checked),
                ("retainsAfterJoin", Completion::SourceRejected),
            ] {
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == find(name).node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), expected);
                assert_eq!(
                    root.ordinary_mir().is_some(),
                    expected == Completion::Checked
                );
            }
        },
    );
}

#[test]
fn read_only_pair_receiver_groups_consume_depth() {
    probe(
        "captures::read_only_pair_receiver_groups_consume_depth",
        r#"
function bare(left: bool, right: bool): [bool, bool] { let job: () => [bool, bool] = () => [left, right]; return job(); }
function grouped(left: bool, right: bool): [bool, bool] { let job: () => [bool, bool] = () => [left, right]; return ((((((((((((job))))))))))))(); }
"#,
        |_, module, receipt| {
            let check = |name: &str, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let bare = check("bare", AnalysisLimits::default());
            assert!(bare.mir().is_some());
            assert!(check("grouped", AnalysisLimits::default()).mir().is_some());
            let limits = AnalysisLimits {
                max_depth: bare.statistics().max_depth,
                ..Default::default()
            };
            assert!(check("bare", limits).mir().is_some());
            let short = check("grouped", limits);
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(short.mir().is_none());
        },
    );
}

#[test]
fn explicit_borrow_in_repeatable_pair_ends_at_its_last_descriptor_use() {
    probe_with_typed(
        "captures::explicit_borrow_in_repeatable_pair_ends_at_its_last_descriptor_use",
        r#"
function before(bit: bool): bool {
    let mut flag = true;
    let job: () => [&bool, bool] = () => [&flag, bit];
    let [_, saved] = job(); let [view, _] = job();
    flag = false; let seen = *view; return saved;
}
function after(bit: bool): bool {
    let mut flag = true;
    let job: () => [&bool, bool] = () => [&flag, bit];
    let [_, saved] = job(); let [view, _] = job();
    let seen = *view; flag = false; return saved;
}
"#,
        |_, module, receipt, typed| {
            let check = |name: &str| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap()
            };
            let early = check("before");
            assert_eq!(early.completion(), Completion::SourceRejected);
            assert!(early.mir().is_none());
            let [finding] = early.findings() else {
                panic!("expected one conflict")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.site.span.bytes(), b"flag = false;");
            assert_eq!(finding.related.span.bytes(), b"&flag");
            let late = check("after");
            let mir = late
                .mir()
                .expect("only the independent scalar is still live at the write");
            let [scalar, descriptor] = read_only_pair_results(
                module,
                typed,
                mir,
                mir.entry,
                ["&flag", "bit"],
                [
                    CaptureForm::ExplicitBorrow { mutable: false },
                    CaptureForm::Value,
                ],
            );
            let ops = linear(mir, mir.entry);
            assert!(ops.iter().any(|op| matches!(op, Operation::Copy { from, into: Some(binding) } if from.root.node == scalar.root.node && from.tuple_element() == Some(1) && binding.root.span.bytes() == b"saved")));
            assert!(ops.iter().any(|op| matches!(op, Operation::Copy { from, into: Some(binding) } if from.root.node == descriptor.root.node && from.tuple_element() == Some(0) && binding.root.span.bytes() == b"view")));
            let read = ops.iter().position(|op| matches!(op, Operation::Dereference(from) if from.root.span.bytes() == b"view")).unwrap();
            let write = ops.iter().position(|op| matches!(op, Operation::Install { target, .. } if target.root.span.bytes() == b"flag")).unwrap();
            let returned = ops.iter().position(|op| matches!(op, Operation::Copy { from, into: Some(_) } if from.root.span.bytes() == b"saved")).unwrap();
            assert!(read < write && write < returned);
        },
    );
}

#[test]
fn erased_read_only_pair_has_no_typed_receipt() {
    use compiler::analysis::{
        InvocationMode,
        typed::{ResultUseBlocker, TypedCompletion},
    };

    probe_report(
        "captures::erased_read_only_pair_has_no_typed_receipt",
        r#"
            function twice(job: () => [bool, bool]): [bool, bool] {
                let first = job();
                return job();
            }
        "#,
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            assert!(typed.core_blockers().is_empty());
            let pending = typed.result_use().unwrap().pending();
            let calls: Vec<_> = pending
                .iter()
                .filter(|p| {
                    p.blocker == ResultUseBlocker::CallBoundary && p.site.module == &module.key
                })
                .collect();
            let [first, second] = calls.as_slice() else {
                panic!("expected a blocker at both calls")
            };
            assert_ne!(first.site.node, second.site.node);
            for call in [first, second] {
                assert_eq!(call.site.span.bytes(), b"job()");
                let fact = typed
                    .bodies()
                    .node(module, call.site.node)
                    .unwrap()
                    .unwrap();
                let invocation = fact.callable.as_ref().unwrap().invocation.as_ref().unwrap();
                assert_eq!(invocation.mode, InvocationMode::ReadOnly);
                assert_eq!(invocation.access, Access::Owned);
                let receiver = typed
                    .bodies()
                    .node(module, invocation.receiver)
                    .unwrap()
                    .unwrap();
                assert!(
                    receiver
                        .callable
                        .as_ref()
                        .and_then(|f| f.arrow.as_ref())
                        .is_none()
                );
            }
            assert!(
                pending
                    .iter()
                    .any(|p| p.blocker == ResultUseBlocker::Containment
                        && p.site.module == &module.key
                        && p.site.span.bytes() == b"job: () => [bool, bool]")
            );
        },
    );
}

#[test]
fn borrowed_pair_invocation_transfers_backing_not_receiver_storage() {
    probe_with_typed(
        "captures::borrowed_pair_invocation_transfers_backing_not_receiver_storage",
        r#"
            function external(view: &string, owner: string): &string {
                let job: once () => [&string, string] = () => [view, owner];
                let [saved, _] = job();
                return saved;
            }
            function grouped(view: &string, owner: string): &string {
                let job: once () => [&string, string] = () => [(view), owner];
                let [saved, _] = job();
                return saved;
            }
            function local(text: string, owner: string): &string {
                let job: once () => [&string, string] = () => [&text, owner];
                let [saved, _] = job();
                return saved;
            }
            function carriedLocal(text: string, owner: string): &string {
                let view = &text;
                let job: once () => [&string, string] = () => [view, owner];
                let [saved, _] = job();
                return saved;
            }
            function live(text: string, owner: string): void {
                let job: once () => [&string, string] = () => [&text, owner];
                let [saved, _] = job();
                dispose(text);
                let _ = saved;
            }
            function dead(text: string, owner: string): void {
                let job: once () => [&string, string] = () => [&text, owner];
                let [saved, _] = job();
                let _ = saved;
                dispose(text);
            }
            function two(first: string, second: string): void {
                let job: once () => [&string, &string] = () => [&first, &second];
                let [saved, _] = job();
                dispose(second);
                let _ = saved;
            }
        "#,
        |request, module, receipt, typed| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap()
            };
            let check =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let lower = |name| check(name, AnalysisLimits::default());
            let external = lower("external");
            let mir = external
                .mir()
                .expect("the caller backing can survive receiver consumption");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::Move { .. }
                            | Operation::FormClosurePair { .. }
                            | Operation::InvokeClosurePair { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            let [
                Operation::Copy {
                    from: view,
                    into: Some(staged_view),
                },
                Operation::Move {
                    from: owner,
                    into: Some(staged_owner),
                },
                Operation::FormClosurePair {
                    closure, captures, ..
                },
                Operation::InvokeClosurePair {
                    receiver,
                    origin,
                    into,
                },
                Operation::Copy {
                    from: field,
                    into: Some(saved),
                },
                Operation::Copy {
                    from,
                    into: Some(output),
                },
                Operation::Export(exported),
            ] = route.as_slice()
            else {
                panic!("expected the carried reference to reach the return slot")
            };
            assert_eq!(view.root.span.bytes(), b"view: &string");
            assert_eq!(owner.root.span.bytes(), b"owner: string");
            assert_eq!(closure.node, receiver.root.node);
            assert_eq!(origin.span.bytes(), b"() => [view, owner]");
            assert_eq!(captures[0].node, staged_view.root.node);
            assert_eq!(captures[1].node, staged_owner.root.node);
            assert_eq!(field.root.node, into.root.node);
            assert_eq!(field.tuple_element(), Some(0));
            assert_eq!(saved.root.span.bytes(), b"saved");
            assert_eq!(saved.root.node, from.root.node);
            assert_eq!(output.root.node, exported.root.node);
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(remaining)]
                if remaining.root.node == into.root.node && remaining.tuple_element() == Some(1))
            );
            let grouped = lower("grouped");
            assert_eq!(grouped.completion(), Completion::Checked);
            assert!(grouped.mir().is_some());
            let dead = lower("dead");
            let mir = dead
                .mir()
                .expect("releasing a source after the result's last use is valid");
            let ops = linear(mir, mir.entry);
            let route: Vec<_> = ops
                .into_iter()
                .filter(|op| match op {
                    Operation::Borrow { .. }
                    | Operation::Copy { .. }
                    | Operation::FormClosurePair { .. }
                    | Operation::InvokeClosurePair { .. }
                    | Operation::Dispose { .. } => true,
                    Operation::Cleanup(actions) => {
                        actions.iter().any(|a| matches!(a, Obligation::Drop(_)))
                    }
                    _ => false,
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Borrow { source, holder, exclusive: false },
                Operation::FormClosurePair { closure, captures, .. },
                Operation::InvokeClosurePair { receiver, into, .. },
                Operation::Copy { from: field, into: Some(saved) },
                Operation::Cleanup(drops),
                Operation::Copy { from: last_use, .. },
                Operation::Dispose { operand, .. },
            ] if source.root.span.bytes() == b"text: string" && source.root.node == operand.root.node
                && holder.root.node == captures[0].node && receiver.root.node == closure.node
                && field.root.node == into.root.node && field.tuple_element() == Some(0)
                && saved.root.node == last_use.root.node && saved.root.span.bytes() == b"saved"
                && drops.iter().filter(|action| matches!(action, Obligation::Drop(..))).count() == 1
                && drops.iter().any(|a| matches!(a, Obligation::Drop(place)
                    if place.root.node == into.root.node && place.tuple_element() == Some(1)))));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(remaining)]
                if remaining.tuple_element() == Some(1))
            );

            for (name, code, at, source) in [
                ("local", "R0202", "return saved;", "text: string"),
                ("carriedLocal", "R0202", "return saved;", "text: string"),
                ("live", "R0203", "dispose(text)", "&text"),
                ("two", "R0203", "dispose(second)", "&second"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none(), "{name}");
                let [finding] = report.findings() else {
                    panic!("{name}: expected one ownership finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(
                    std::str::from_utf8(finding.site.span.bytes()),
                    Ok(at),
                    "{name}"
                );
                assert_eq!(
                    std::str::from_utf8(finding.related.span.bytes()),
                    Ok(source),
                    "{name}"
                );
            }
            let limits = AnalysisLimits {
                max_work: external.statistics().work,
                max_entries: external.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert_eq!(check("external", limits).completion(), Completion::Checked);
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let report = check("external", short);
                assert_eq!(report.completion(), Completion::ResourceLimit);
                assert!(report.mir().is_none());
            }
            let type_limits = AnalysisLimits {
                max_work: typed.statistics().work,
                max_entries: typed.statistics().entries,
                ..AnalysisLimits::default()
            };
            let graph = loader::load_installed(request.clone()).unwrap();
            let type_check = |limits| analyze_typed(&graph, limits);
            assert!(type_check(type_limits).receipt().is_some());
            for short in [
                AnalysisLimits {
                    max_work: type_limits.max_work - 1,
                    ..type_limits
                },
                AnalysisLimits {
                    max_entries: type_limits.max_entries - 1,
                    ..type_limits
                },
            ] {
                let report = type_check(short);
                assert_eq!(
                    report.completion(),
                    compiler::analysis::typed::TypedCompletion::ResourceLimit
                );
                assert!(report.receipt().is_none());
            }
        },
    );
}

#[test]
fn borrowed_pair_handoff_does_not_acknowledge_an_unrelated_result() {
    use compiler::analysis::typed::{ResultDiscardKind, TypedCompletion};

    probe_report(
        "captures::borrowed_pair_handoff_does_not_acknowledge_an_unrelated_result",
        r#"
            function unacknowledged(untouched: Result<bool, i32>.Ok, outward: &string, owner: string): &string {
                let job: once () => [&string, string] = () => [outward, owner];
                let [view, _] = job();
                return view;
            }
        "#,
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::SourceRejected);
            assert!(typed.receipt().is_none());
            let uses = typed.result_use().unwrap();
            assert!(uses.pending().is_empty());
            let [discard] = uses.discards() else {
                panic!("expected an unacknowledged Result")
            };
            assert_eq!(discard.code(), "R0103");
            assert_eq!(discard.kind, ResultDiscardKind::ScopeExit);
            assert_eq!(discard.origin.module, &module.key);
            assert_eq!(
                discard.origin.span.bytes(),
                b"untouched: Result<bool, i32>.Ok"
            );
            assert_eq!(discard.discard.span.bytes(), b"return view;");
        },
    );
}

#[test]
fn consumed_pair_handoff_does_not_skip_an_unrelated_result() {
    use compiler::analysis::typed::{ResultDiscardKind, TypedCompletion};

    probe_report(
        "captures::consumed_pair_handoff_does_not_skip_an_unrelated_result",
        r#"
            function unacknowledged(untouched: Result<bool, i32>.Ok, left: string, right: string): [string, string] {
                let job: once () => [string, string] = () => [left, right];
                let _ = job();
                return job();
            }
        "#,
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::SourceRejected);
            assert!(typed.receipt().is_none());
            let uses = typed.result_use().unwrap();
            assert!(uses.pending().is_empty());
            let [discard] = uses.discards() else {
                panic!("expected the unrelated Result at exit")
            };
            assert_eq!(discard.code(), "R0103");
            assert_eq!(discard.kind, ResultDiscardKind::ScopeExit);
            assert_eq!(discard.origin.module, &module.key);
            assert_eq!(
                discard.origin.span.bytes(),
                b"untouched: Result<bool, i32>.Ok"
            );
            assert_eq!(discard.discard.span.bytes(), b"return job();");
        },
    );
}

#[test]
fn broader_invocations_still_need_typed_evidence() {
    use compiler::analysis::typed::{ResultUseBlocker, TypedCompletion};

    probe_report(
        "captures::broader_invocations_still_need_typed_evidence",
        r#"
            function overwritten(flag: bool, left: string, right: string, erased: once () => [string, string]): [string, string] {
                let mut job: once () => [string, string] = () => [left, right];
                let _ = job();
                if (flag) { job = erased; }
                return job();
            }
            function migrated(left: string, right: string): [string, string] {
                let job: once () => [string, string] = () => [left, right];
                let _ = job();
                let other = job;
                return other();
            }
            type Box = { item: Result<bool, i32>; }
            function unknown(view: &Box, owner: string): void {
                let opaque: once () => [&Box, string] = () => [view, owner];
                let _ = opaque();
            }
            function echo(view: &string): &string { return view; }
            function computed(view: &string, owner: string): void {
                let work: once () => [&string, string] = () => [echo(view), owner];
                let _ = work();
            }
        "#,
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.core_blockers().is_empty());
            assert!(typed.receipt().is_none());
            let pending = typed.result_use().unwrap().pending();
            for (written, call) in [
                (b"return job();".as_slice(), b"job()".as_slice()),
                (b"return other();", b"other()"),
                (b"= opaque();", b"opaque()"),
                (b"= work();", b"work()"),
            ] {
                let at = module
                    .snapshot
                    .bytes()
                    .windows(written.len())
                    .position(|w| w == written)
                    .unwrap();
                let start = at + written.len() - call.len() - 1;
                let blocker = pending
                    .iter()
                    .find(|p| {
                        p.blocker == ResultUseBlocker::CallBoundary
                            && p.site.module == &module.key
                            && p.site.span.start() == start
                    })
                    .unwrap();
                assert_eq!(blocker.site.span.bytes(), call);
                let fact = typed
                    .bodies()
                    .node(module, blocker.site.node)
                    .unwrap()
                    .unwrap();
                let invocation = fact.callable.as_ref().unwrap().invocation.as_ref().unwrap();
                assert_eq!(invocation.mode, compiler::analysis::InvocationMode::Once);
                assert!(invocation.assumption.is_none());
            }
        },
    );
}
