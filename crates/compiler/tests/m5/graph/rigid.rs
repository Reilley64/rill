use super::*;
use compiler::analysis::{
    QueryError,
    ownership::{Report, lower_root},
    scalars::NodeFactKind,
    typed::{ResultUseBlocker, TypedCompletion},
};

#[test]
fn rigid_shared_parameter_ends_without_accessing_its_referent() {
    probe(
        "graph::rigid::rigid_shared_parameter_ends_without_accessing_its_referent",
        r#"
            function ignored<U>(argument: &U): bool;
            implement<R> ignored(input: &R): bool { return false; }
        "#,
        |_, module, receipt| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let root = root(&report, b"implement<R> ignored");
            let target = check_target(receipt, module, root.owner());
            assert_eq!(target.completion(), Completion::Checked);
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            assert!(root.dependencies().is_empty());
            for mir in [target.mir().unwrap(), root.ordinary_mir().unwrap()] {
                assert!(mir.cancellation().is_none());
                let operations = linear(mir, mir.entry);
                let [
                    Operation::Initialize(value),
                    Operation::Export(output),
                    Operation::Cleanup(cleanup),
                    Operation::None,
                ] = operations.as_slice()
                else {
                    panic!("unexpected operations: {operations:?}");
                };
                let [Obligation::EndStorage(input)] = cleanup.as_slice() else {
                    panic!("unexpected cleanup: {cleanup:?}");
                };
                assert_eq!(input.root.span.bytes(), b"input: &R");
                assert_eq!(value.root.span.bytes(), b"false");
                assert_eq!(value.root.node, output.root.node);
            }
        },
    );
}

#[test]
fn closed_call_uses_the_selected_rigid_source_body() {
    probe_with_typed(
        "graph::rigid::closed_call_uses_the_selected_rigid_source_body",
        r#"
            function choose<T>(input: &T): bool;
            implement<U> choose(input: &U): bool { return true; }
            function caller(input: i32): bool { return choose(&input); }
            expect caller(1i32);
            implement choose(input: &bool): bool { return *input; }
            function specialized(input: bool): bool { return choose(&input); }
        "#,
        |_, module, receipt, typed| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let source = root(&report, b"implement<U> choose");
            let caller = root(&report, b"function caller");
            let expectation = root(&report, b"expect caller");
            let call = module
                .syntax
                .nodes
                .iter()
                .position(|node| node.span.bytes() == b"choose(&input)")
                .unwrap();
            let fact = typed.bodies().node(module, call).unwrap().unwrap();
            let NodeFactKind::Expression(fact) = &fact.kind else {
                panic!("expected a typed call");
            };
            let selection = fact.call.as_ref().unwrap().selection.as_ref().unwrap();
            assert_eq!(
                source.owner(),
                BodyOwner::Declaration(selection.implementation)
            );
            for root in [source, caller, expectation] {
                assert_eq!(
                    root.ordinary_completion(),
                    Completion::Checked,
                    "{:?}",
                    root.blocked_at()
                        .map(|site| site.span.bytes().escape_ascii().to_string())
                );
                let target = check_target(receipt, module, root.owner());
                assert_eq!(target.completion(), Completion::Checked);
                assert!(target.mir().unwrap().cancellation().is_none());
                assert!(root.ordinary_mir().unwrap().cancellation().is_none());
                assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
                assert!(matches!(root.selected_call(), Delivery::Unresolved));
                assert!(matches!(
                    root.replacement_cancellation(),
                    Delivery::Unresolved
                ));
            }
            let [edge] = caller.dependencies() else {
                panic!("one ordinary call")
            };
            assert_eq!(edge.contract, DependencyContract::OrdinaryCall);
            assert_eq!(edge.site.node, call);
            assert_eq!(edge.declaration, selection.implementation);
            assert!(matches!(source.owner(), BodyOwner::Declaration(id)
                if std::ptr::eq(edge.declaration, id)));
            let [edge] = expectation.dependencies() else {
                panic!("one ordinary call")
            };
            assert_eq!(edge.contract, DependencyContract::OrdinaryCall);
            assert_eq!(caller.owner(), BodyOwner::Declaration(edge.declaration));
            let (exact, short) = caps(report.statistics());
            assert!(report.inventory_complete());
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
            let rerun = analyze_ownership(receipt, exact);
            assert_eq!(rerun.statistics(), report.statistics());
            assert_eq!(rerun.completion(), report.completion());
            for prefix in [
                b"implement<U> choose".as_slice(),
                b"function caller",
                b"expect caller",
            ] {
                assert_eq!(
                    root(&rerun, prefix).ordinary_completion(),
                    Completion::Checked
                );
            }
            for limits in short {
                let limited = analyze_ownership(receipt, limits);
                assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                assert!(limited.findings().is_empty());
                assert!(
                    limited
                        .roots()
                        .iter()
                        .all(|root| root.ordinary_mir().is_none())
                );
            }
            let overload = root(&report, b"implement choose(input: &bool)");
            let specialized = root(&report, b"function specialized");
            let blocked = specialized.blocked_at().unwrap();
            assert_eq!(blocked.span.bytes(), b"choose(&input)");
            let NodeFactKind::Expression(fact) = &typed
                .bodies()
                .node(module, blocked.node)
                .unwrap()
                .unwrap()
                .kind
            else {
                panic!("expected a typed call to the overload");
            };
            let selection = fact.call.as_ref().unwrap().selection.as_ref().unwrap();
            assert_eq!(
                overload.owner(),
                BodyOwner::Declaration(selection.implementation)
            );
            assert_ne!(overload.owner(), source.owner());
            for pending in [overload, specialized] {
                assert_eq!(pending.ordinary_completion(), Completion::Incomplete);
                assert!(pending.ordinary_mir().is_none());
                let target =
                    lower_root(receipt, module, pending.owner(), AnalysisLimits::default())
                        .unwrap();
                assert_eq!(target.completion(), Completion::Incomplete);
                assert!(target.mir().is_none());
            }
        },
    );
}

#[test]
fn unknown_storage_and_nonliteral_bodies_do_not_check() {
    probe(
        "graph::rigid::unknown_storage_and_nonliteral_bodies_do_not_check",
        r#"
            type Carrier<T> = { borrowed: &T; owned: string; }
            function ownsCarrier<T>(input: Carrier<T>): bool;
            implement<T> ownsCarrier(input: Carrier<T>): bool { return true; }
            function reads<T>(input: &T): bool;
            implement<T> reads(input: &T): bool { let _ = *input; return true; }
            function other<T>(input: &T): bool;
            implement<T> other(input: &T): bool { return !false; }
            function callsOther(input: i32): bool { return other(&input); }
            expect callsOther(1i32);
            export function exported<T>(input: &T): bool;
            implement<T> exported(input: &T): bool { return true; }
            function constrained<T>(input: &T): bool where noBorrow(T);
            implement<T> constrained(input: &T): bool { return true; }
            function reuse(text: string): string { let moved = text; return text; }
        "#,
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let other = root(&graph, b"implement<T> other");
            for prefix in [
                b"implement<T> ownsCarrier".as_slice(),
                b"implement<T> reads",
                b"implement<T> other",
                b"implement<T> exported",
                b"implement<T> constrained",
                b"function callsOther",
                b"expect callsOther",
            ] {
                let root = root(&graph, prefix);
                let blocked = if prefix.starts_with(b"implement") {
                    root.site()
                } else {
                    other.site()
                };
                assert_eq!(root.ordinary_completion(), Completion::Incomplete);
                assert!(root.ordinary_mir().is_none());
                assert_eq!(root.blocked_at().unwrap().node, blocked.node);
                let target = check_target(receipt, module, root.owner());
                assert_eq!(target.completion(), Completion::Incomplete);
                assert_eq!(target.blocked_at().unwrap().node, blocked.node);
                assert!(target.mir().is_none() && target.findings().is_empty());
            }
            let caller = root(&graph, b"function callsOther");
            let [edge] = caller.dependencies() else {
                panic!("selected source is still required")
            };
            assert_eq!(edge.contract, DependencyContract::OrdinaryCall);
            assert_eq!(edge.site.span.bytes(), b"other(&input)");
            assert_eq!(other.owner(), BodyOwner::Declaration(edge.declaration));
            let [reuse] = graph.findings() else {
                panic!("one ordinary conflict")
            };
            assert_eq!(reuse.code(), "R0200");
            let bytes = module.snapshot.bytes();
            for (site, prefix) in [
                (reuse.site, b"return ".as_slice()),
                (reuse.related, b"moved = "),
            ] {
                assert_eq!(site.module, &module.key);
                assert_eq!(site.span.bytes(), b"text");
                assert_eq!(
                    &bytes[site.span.start() - prefix.len()..site.span.start()],
                    prefix
                );
            }
        },
    );
}

#[test]
fn opaque_owning_binder_does_not_supply_a_typed_receipt() {
    probe_report(
        "graph::rigid::opaque_owning_binder_does_not_supply_a_typed_receipt",
        r#"
            function owns<T>(input: T): bool;
            implement<T> owns(input: T): bool { return true; }
        "#,
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            let [pending] = typed.result_use().unwrap().pending() else {
                panic!("one typed blocker")
            };
            assert_eq!(pending.blocker, ResultUseBlocker::Containment);
            assert_eq!(pending.site.module, &module.key);
            assert_eq!(pending.site.span.bytes(), b"input: T");
        },
    );
}

fn check_target<'g>(
    receipt: &TypedReceipt<'_, 'g>,
    module: &'g loader::Module,
    owner: BodyOwner<'_>,
) -> Report<'g> {
    let run = |limits| lower_root(receipt, module, owner, limits).unwrap();
    let full = run(AnalysisLimits::default());
    let (exact, short) = caps(full.statistics());
    let exact = run(exact);
    assert_eq!(exact.statistics(), full.statistics());
    assert_eq!(exact.completion(), full.completion());
    assert_eq!(exact.mir().is_some(), full.mir().is_some());
    for limits in short {
        let limited = run(limits);
        assert_eq!(
            limited.completion(),
            Completion::ResourceLimit,
            "{limits:?}"
        );
        assert!(limited.mir().is_none() && limited.findings().is_empty());
    }
    full
}

fn caps(statistics: Statistics) -> (AnalysisLimits, impl Iterator<Item = AnalysisLimits>) {
    let exact = AnalysisLimits {
        max_work: statistics.work,
        max_entries: statistics.entries,
        max_depth: statistics.max_depth,
    };
    let short = [
        exact
            .max_work
            .checked_sub(1)
            .map(|max_work| AnalysisLimits { max_work, ..exact }),
        exact
            .max_entries
            .checked_sub(1)
            .map(|max_entries| AnalysisLimits {
                max_entries,
                ..exact
            }),
        exact
            .max_depth
            .checked_sub(1)
            .map(|max_depth| AnalysisLimits { max_depth, ..exact }),
    ];
    (exact, short.into_iter().flatten())
}

#[test]
fn rigid_roots_reject_foreign_and_stale_capture_handles_before_metering() {
    probe(
        "graph::rigid::rigid_roots_reject_foreign_and_stale_capture_handles_before_metering",
        r#"
            function check<T>(input: &T): bool;
            implement<T> check(input: &T): bool { return false; }
        "#,
        |request, module, receipt| {
            let original = fs::read_to_string(&request.entry).unwrap();
            let foreign = loader::load_installed(request.clone()).unwrap();
            fs::write(
                &request.entry,
                original.replace("return false", "return !true"),
            )
            .unwrap();
            let edited = loader::load_installed(request.clone());
            fs::write(&request.entry, &original).unwrap();
            for capture in [foreign, edited.unwrap()] {
                let another = capture
                    .modules()
                    .iter()
                    .find(|m| m.key == module.key)
                    .unwrap();
                let implementation = |m: &loader::Module| {
                    m.declarations
                        .iter()
                        .position(|d| d.id.kind == Implementation)
                        .unwrap()
                };
                let live = &module.declarations[implementation(module)].id;
                let foreign = &another.declarations[implementation(another)].id;
                assert_eq!(live.start, foreign.start);
                assert_ne!(live.context, foreign.context);
                let zero = AnalysisLimits {
                    max_work: 0,
                    max_entries: 0,
                    max_depth: 0,
                };
                for (module, id) in [(module, foreign), (another, live), (another, foreign)] {
                    assert!(matches!(
                        lower_root(receipt, module, BodyOwner::Declaration(id), zero),
                        Err(QueryError::InvalidRequest)
                    ));
                }
            }
        },
    );
}
