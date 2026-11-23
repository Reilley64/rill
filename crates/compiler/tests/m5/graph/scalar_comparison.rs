use super::*;
use compiler::analysis::{
    QueryError,
    ownership::lower_root,
    typed::{Primitive, PrimitiveProvider, PrimitiveRole},
};

const I32_EQ: &str = "implement eq(left: &i32, right: &i32): bool { return *left == *right; }";

fn compare_module(graph: &loader::Graph) -> &loader::Module {
    graph
        .modules()
        .iter()
        .find(|module| module.key.origin == Origin::Standard && module.key.path == "compare.rill")
        .unwrap()
}

fn provider<'g>(typed: &TypedReport<'g>) -> Option<PrimitiveProvider<'g>> {
    typed
        .primitive_providers()
        .iter()
        .copied()
        .find(|provider| {
            provider.role == PrimitiveRole::Equality && provider.primitive == Primitive::I32
        })
}

fn report_for<'a, 'g>(report: &'a GraphReport<'g>, owner: BodyOwner<'_>) -> &'a RootReport<'g> {
    report
        .roots()
        .iter()
        .find(|root| root.owner() == owner)
        .unwrap()
}

#[test]
fn installed_i32_equality_checks_the_loaded_source_root() {
    fixture(
        "graph::scalar_comparison::installed_i32_equality_checks_the_loaded_source_root",
        &[HELPER],
        |_, request, graph, receipt, typed| {
            let compare = compare_module(graph);
            let provider = provider(typed).unwrap();
            assert!(provider.binding.is_none());
            assert!(std::ptr::eq(
                provider.family,
                &typed.equality_role().unwrap().id
            ));
            assert_eq!(provider.implementation.id.module, compare.key);
            let owner = BodyOwner::Declaration(&provider.implementation.id);
            let run_target = |limits| lower_root(receipt, compare, owner, limits).unwrap();
            let run_graph = |limits| analyze_ownership(receipt, limits);
            let target = run_target(AnalysisLimits::default());
            let report = run_graph(AnalysisLimits::default());
            let root = report_for(&report, owner);
            assert_eq!(root.site().span.bytes(), I32_EQ.as_bytes());
            assert!(report.inventory_complete());
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
            assert!(report.findings().is_empty());
            assert_eq!(target.completion(), Completion::Checked);
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            assert!(root.dependencies().is_empty());
            assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
            assert!(matches!(root.selected_call(), Delivery::Unresolved));
            assert!(matches!(
                root.replacement_cancellation(),
                Delivery::Unresolved
            ));
            assert_equality_body(target.mir().unwrap());
            assert_equality_body(root.ordinary_mir().unwrap());

            // Exact targeted cap at the tightened screen (ri-785.9).
            assert_eq!(
                target.statistics(),
                Statistics {
                    work: 2427,
                    entries: 310,
                    max_depth: 5
                }
            );
            let [exact, short_work, short_entries, short_depth] = caps(target.statistics());
            let exact = run_target(exact);
            assert_eq!(exact.statistics(), target.statistics());
            assert_equality_body(exact.mir().unwrap());
            for short in [short_work, short_entries, short_depth] {
                let exhausted = run_target(short);
                assert_eq!(
                    exhausted.completion(),
                    Completion::ResourceLimit,
                    "{short:?}"
                );
                assert!(exhausted.mir().is_none());
            }
            let [exact, short_work, short_entries, short_depth] = caps(report.statistics());
            let exact = run_graph(exact);
            assert_eq!(exact.statistics(), report.statistics());
            assert_eq!(exact.completion(), report.completion());
            assert_equality_body(report_for(&exact, owner).ordinary_mir().unwrap());
            for short in [short_work, short_entries, short_depth] {
                let exhausted = run_graph(short);
                assert_eq!(
                    exhausted.completion(),
                    GraphCompletion::ResourceLimit,
                    "{short:?}"
                );
                assert!(
                    exhausted
                        .roots()
                        .iter()
                        .all(|root| root.ordinary_mir().is_none())
                );
            }

            // This admits no other primitive width, and no generic &&T adapter.
            let other_implementations = report.roots().iter().filter(|root| {
                root.site().module == &compare.key && root.owner() != owner
                    && matches!(root.owner(), BodyOwner::Declaration(id) if id.kind == Implementation)
            });
            for other in other_implementations {
                assert_pending(&report, receipt, compare, other.owner());
            }
            let foreign = loader::load_installed(request.clone()).unwrap();
            let foreign_compare = compare_module(&foreign);
            let foreign_id = &foreign_compare
                .declarations
                .iter()
                .find(|d| {
                    d.id.start == provider.implementation.id.start && d.id.kind == Implementation
                })
                .unwrap()
                .id;
            assert!(matches!(
                lower_root(
                    receipt,
                    compare,
                    BodyOwner::Declaration(foreign_id),
                    AnalysisLimits::default()
                ),
                Err(QueryError::InvalidRequest)
            ));
            assert!(matches!(
                lower_root(receipt, foreign_compare, owner, AnalysisLimits::default()),
                Err(QueryError::InvalidRequest)
            ));
            eprintln!(
                "test={}: target {:?}, graph {:?}",
                request.test_mode,
                target.statistics(),
                report.statistics()
            );
        },
    );
}

/// Metering rules 1 and 2: every other compare.rill body keeps the exact
/// targeted cap measured at 1c27e7c9, before the equality root: one valid-root
/// lookup and one step, then Incomplete at the declaration. The whole graph
/// differs from that parent only by the equality root: 316 work, 134 entries,
/// depth 5 (metering rule 11).
#[test]
fn other_compare_bodies_keep_their_pre_equality_caps() {
    fixture(
        "graph::scalar_comparison::other_compare_bodies_keep_their_pre_equality_caps",
        &[HELPER],
        |_, _, graph, receipt, typed| {
            let compare = compare_module(graph);
            let equality = provider(typed).unwrap().implementation;
            let before = Statistics {
                work: 190298,
                entries: 4482,
                max_depth: 3,
            };
            let [old, ..] = caps(before);
            assert_eq!(
                analyze_ownership(receipt, old).completion(),
                GraphCompletion::ResourceLimit
            );
            let after = Statistics {
                work: before.work + 316,
                entries: before.entries + 134,
                max_depth: 5,
            };
            let [exact, short_work, short_entries, short_depth] = caps(after);
            let report = analyze_ownership(receipt, exact);
            assert_eq!(report.statistics(), after);
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
            for short in [short_work, short_entries, short_depth] {
                assert_eq!(
                    analyze_ownership(receipt, short).completion(),
                    GraphCompletion::ResourceLimit
                );
            }
            let others = compare
                .declarations
                .iter()
                .filter(|d| d.id.kind == Implementation && !std::ptr::eq(*d, equality))
                .collect::<Vec<_>>();
            // Twelve other widths and the generic `&&T` adapter.
            assert_eq!(others.len(), 13);
            for other in others {
                let owner = BodyOwner::Declaration(&other.id);
                let target_stats = Statistics {
                    work: 9,
                    entries: 0,
                    max_depth: 0,
                };
                let run = |limits| lower_root(receipt, compare, owner, limits).unwrap();
                let exact = AnalysisLimits {
                    max_work: 9,
                    max_entries: 0,
                    max_depth: 0,
                };
                let target = run(exact);
                assert_eq!(target.completion(), Completion::Incomplete);
                assert_eq!(target.statistics(), target_stats);
                assert!(target.mir().is_none() && target.findings().is_empty());
                assert_eq!(
                    target.blocked_at().unwrap().span.bytes(),
                    other.span.bytes()
                );
                let limited = run(AnalysisLimits {
                    max_work: 8,
                    ..exact
                });
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none() && limited.findings().is_empty());
                let root = report_for(&report, owner);
                assert_eq!(root.ordinary_completion(), Completion::Incomplete);
                assert!(root.ordinary_mir().is_none());
                assert_eq!(root.blocked_at().unwrap().span.bytes(), other.span.bytes());
            }
        },
    );
}

fn assert_equality_body(mir: &Mir<'_>) {
    assert!(mir.cancellation().is_none());
    let path = straight_blocks(mir, mir.entry);
    let operations: Vec<_> = path
        .iter()
        .map(|block| &block.operation)
        .filter(|op| !matches!(op, Operation::None))
        .collect();
    let [
        Operation::CopyReferent {
            from: left_ref,
            into: left_value,
        },
        Operation::CopyReferent {
            from: right_ref,
            into: right_value,
        },
        Operation::EqualI32 { left, right, into },
        Operation::Export(exported),
        Operation::Cleanup(cleanup),
    ] = operations.as_slice()
    else {
        panic!(
            "expected two referent reads, comparison, export and descriptor cleanup: {operations:?}"
        );
    };
    assert_eq!(left_ref.root.span.bytes(), b"left: &i32");
    assert_eq!(right_ref.root.span.bytes(), b"right: &i32");
    assert_eq!(left_value.root.span.bytes(), b"*left");
    assert_eq!(right_value.root.span.bytes(), b"*right");
    assert_eq!(left.root.node, left_value.root.node);
    assert_eq!(right.root.node, right_value.root.node);
    assert_eq!(into.root.span.bytes(), b"*left == *right");
    assert_eq!(into.root.node, exported.root.node);
    assert!(
        matches!(cleanup.as_slice(), [Obligation::EndStorage(right), Obligation::EndStorage(left)]
        if right.root.node == right_ref.root.node && left.root.node == left_ref.root.node)
    );
    assert_eq!(path.last().unwrap().terminator, Terminator::Return);
}

fn caps(statistics: Statistics) -> [AnalysisLimits; 4] {
    let exact = AnalysisLimits {
        max_work: statistics.work,
        max_entries: statistics.entries,
        max_depth: statistics.max_depth,
    };
    [
        exact,
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

fn assert_pending<'g>(
    report: &GraphReport<'g>,
    receipt: &TypedReceipt<'_, 'g>,
    compare: &'g loader::Module,
    owner: BodyOwner<'_>,
) {
    let target = lower_root(receipt, compare, owner, AnalysisLimits::default()).unwrap();
    assert_eq!(target.completion(), Completion::Incomplete);
    assert!(target.blocked_at().is_some() && target.mir().is_none());
    let root = report_for(report, owner);
    assert_eq!(root.ordinary_completion(), Completion::Incomplete);
    assert!(root.blocked_at().is_some() && root.ordinary_mir().is_none());
}

#[test]
fn altered_or_missing_installed_equality_cannot_reuse_the_pin() {
    installed(
        "graph::scalar_comparison::altered_or_missing_installed_equality_cannot_reuse_the_pin",
        |request| {
            fs::write(&request.entry, "import std::compare::eq;\n").unwrap();
            let path = request.entry.with_file_name("../lib/rill/std/compare.rill");
            let original = fs::read_to_string(&path).unwrap();
            assert!(original.contains(I32_EQ));
            for variant in [
                I32_EQ.replace("*left == *right", "*left != *right"),
                I32_EQ.replace("*left == *right", "*right == *left"),
                String::new(),
            ] {
                fs::write(&path, original.replace(I32_EQ, &variant)).unwrap();
                for test_mode in [false, true] {
                    let error = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap_err();
                    assert_eq!(error.kind, loader::FailureKind::InvalidRequest);
                    assert_eq!(error.message, "installed source differs from toolchain pin");
                    assert_eq!(error.path.unwrap().file_name().unwrap(), "compare.rill");
                }
            }
            fs::write(path, original).unwrap();
        },
    );
}
