use super::*;
use compiler::analysis::ownership::OptionTextDisposal;
use compiler::analysis::typed::{Primitive, PrimitiveOperation, PrimitiveRole, ResultUseBlocker};

const SOURCE: &str = r#"
function onItem(xs: List<Option<string>>, outer: string): Option<bool> {
    for (let item of xs) {
        let owner = "before postfix";
        let text = item?;
        let _ = &text;
        let suffix = "after postfix";
        continue;
    }
    return Option.Some(true);
}
function afterMove(xs: List<Option<string>>, outer: string): Option<bool> {
    for (let (item) of ((xs))) {
        let text = ((item))?;
        let moved = text;
        let _ = &moved;
        break;
    }
    return Option.Some(true);
}
function withCurrent(xs: List<Option<string>>, outer: string, stop: bool): Option<bool> {
    if (stop) { return Option.Some(false); }
    for (let item of xs) { break; }
    return Option.Some(true);
}
"#;

#[test]
fn consuming_list_transfers_the_actual_option_tag() {
    use CollectionOperation::{DisposeIterator, DisposeList, Iterate, Next};
    use ListTraversal::Owned;
    probe_with_typed(
        "iteration::option_text::consuming_list_transfers_the_actual_option_tag",
        SOURCE,
        |_, module, receipt, typed| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let (exact, short) = limits(graph.statistics());
            let replay = analyze_ownership(receipt, exact);
            assert_eq!(replay.completion(), GraphCompletion::Incomplete);
            assert_eq!(replay.statistics(), graph.statistics());
            assert!(replay.findings().is_empty());
            for cap in short {
                let limited = analyze_ownership(receipt, cap);
                crate::limit::assert_graph_limit(&limited);
            }
            let collection = |operation| {
                typed
                    .collection_protection()
                    .find(|row| {
                        row.operation == operation && row.coverage == OwnershipCoverage::Clear
                    })
                    .unwrap()
                    .provider
                    .unwrap()
            };
            let provider = typed
                .primitive_providers()
                .iter()
                .find(|p| p.role == PrimitiveRole::Dispose && p.primitive == Primitive::String)
                .unwrap();
            let binding = provider.binding.unwrap();
            assert_eq!(binding.operation, PrimitiveOperation::DisposeText);
            let option = typed.option_role().unwrap();
            let element = OptionTextDisposal {
                some: &option.some.id,
                none: &option.none.id,
                payload: compiler::analysis::ownership::TextElementDisposal {
                    implementation: &provider.implementation.id,
                    bridge: &binding.declaration.id,
                },
            };
            for name in ["onItem", "afterMove", "withCurrent"] {
                let function = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let run = |cap| lower_body(receipt, module, &function.id, cap).unwrap();
                let (exact, short) = limits(run(AnalysisLimits::default()).statistics());
                for cap in short {
                    let limited = run(cap);
                    crate::limit::assert_body_limit(&limited);
                    assert!(limited.findings().is_empty() && limited.mir().is_none());
                }
                let report = run(exact);
                assert_eq!(
                    report.completion(),
                    Completion::Checked,
                    "{name}: {:?}",
                    report
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                );
                let root = replay
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == function.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Checked, "{name}");
                assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
                assert!(matches!(
                    root.replacement_cancellation(),
                    Delivery::Unresolved
                ));
                assert!(matches!(root.selected_call(), Delivery::Unresolved));
                for operation in [Iterate(Owned), Next(Owned), DisposeIterator(Owned)]
                    .into_iter()
                    .chain((name == "withCurrent").then_some(DisposeList))
                {
                    let provider = collection(operation);
                    assert!(root.dependencies().iter().any(|d| {
                        d.declaration == &provider.implementation.id
                            && d.contract
                                == DependencyContract::ListOptionTextCollection {
                                    operation,
                                    bridge: &provider.bridge.id,
                                }
                    }));
                }
                let release = collection(DisposeIterator(Owned));
                let callback = DependencyContract::OptionTextElementDisposal {
                    some: element.some,
                    none: element.none,
                    bridge: element.payload.bridge,
                };
                assert!(root.dependencies().iter().any(|d| {
                    d.declaration == element.payload.implementation && d.contract == callback
                }));
                for mir in [report.mir().unwrap(), root.ordinary_mir().unwrap()] {
                    assert!(
                        mir.cancellation().is_none() && mir.replacement_cancellation().is_none()
                    );
                    check_paths(
                        name,
                        mir,
                        element,
                        [
                            collection(Iterate(Owned)),
                            collection(Next(Owned)),
                            release,
                            collection(DisposeList),
                        ],
                    );
                }
            }
        },
    );
}

#[test]
fn unsupported_carrier_borrow_and_unopened_sibling_keep_the_source_pending() {
    for (source, work, entries) in [
        (
            "function probe(xs: List<Option<string>>): Option<bool> { for (let item of xs) { let _ = &item; break; } return Option.Some(true); }",
            1855,
            19,
        ),
        (
            "function probe(xs: List<Option<string>>, spare: List<string>): Option<bool> { for (let item of xs) { let text = item?; let _ = &text; } return Option.Some(true); }",
            1866,
            24,
        ),
        (
            "function probe(xs: List<Option<string>>, spare: List<bool>): Option<bool> { for (let item of xs) { let text = item?; let _ = &text; } return Option.Some(true); }",
            1866,
            24,
        ),
        (
            "function probe(xs: List<Option<string>>): Option<bool> { for (let item of xs) { let _ = &item; let text = item?; let _ = &text; } return Option.Some(true); }",
            1863,
            23,
        ),
        (
            "function probe(xs: List<Option<string>>): void { for (let item of xs) { let _ = &item; break; } }",
            1847,
            15,
        ),
    ] {
        probe(
            "iteration::option_text::unsupported_carrier_borrow_and_unopened_sibling_keep_the_source_pending",
            source,
            |_, module, receipt| {
                let f = module
                    .declarations
                    .iter()
                    .find(|f| f.name.as_deref() == Some("probe"))
                    .unwrap();
                let old = AnalysisLimits {
                    max_work: work,
                    max_entries: entries,
                    max_depth: 0,
                };
                let target = lower_body(receipt, module, &f.id, old).unwrap();
                assert_eq!(target.completion(), Completion::Incomplete);
                assert_eq!(
                    target.blocked_at().unwrap().span.bytes(),
                    b"xs: List<Option<string>>"
                );
                assert!(target.mir().is_none() && target.findings().is_empty());
                let graph = analyze_ownership(receipt, AnalysisLimits::default());
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == f.node)
                    .unwrap();
                assert_eq!(
                    root.blocked_at().unwrap().node,
                    target.blocked_at().unwrap().node
                );
                assert_eq!(root.ordinary_completion(), Completion::Incomplete);
                assert!(root.ordinary_mir().is_none() && root.dependencies().is_empty());
            },
        );
    }
}

#[test]
fn an_ordinary_option_loop_keeps_the_reached_move_finding() {
    for (result, suffix) in [("void", ""), ("Option<bool>", "return Option.Some(true);")] {
        let source = format!(
            r#"function probe(xs: List<Option<string>>): {result} {{
            for (let item of xs) {{ let s = "owned"; let moved = s; let again = s; break; }}
            {suffix}
        }}"#
        );
        probe(
            "iteration::option_text::an_ordinary_option_loop_keeps_the_reached_move_finding",
            &source,
            |_, module, receipt| {
                let f = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some("probe"))
                    .unwrap();
                let local = lower_body(receipt, module, &f.id, AnalysisLimits::default()).unwrap();
                let graph = analyze_ownership(receipt, AnalysisLimits::default());
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == f.node)
                    .unwrap();
                assert_eq!(local.completion(), Completion::SourceRejected);
                assert_eq!(root.ordinary_completion(), Completion::SourceRejected);
                assert!(local.mir().is_none() && root.ordinary_mir().is_none());
                for findings in [local.findings(), graph.findings()] {
                    let [finding] = findings else {
                        panic!("expected the second read")
                    };
                    assert_eq!(finding.code(), "R0200");
                    for (site, text) in [
                        (finding.site, b"again = s".as_slice()),
                        (finding.related, b"moved = s".as_slice()),
                    ] {
                        let start = module
                            .snapshot
                            .bytes()
                            .windows(text.len())
                            .position(|s| s == text)
                            .unwrap()
                            + text.len()
                            - 1;
                        assert_eq!((site.span.start(), site.span.end()), (start, start + 1));
                    }
                }
            },
        );
    }
}

const OTHER_SOURCES: &str = r#"
function unused(xs: List<Option<string>>): void { }
function arbitrary(xs: List<Option<string>>, gate: Option<string>): Option<bool> {
    for (let item of xs) { let text = gate?; break; }
    return Option.Some(true);
}
function sibling(xs: List<Option<string>>, spare: List<Option<string>>): Option<bool> {
    for (let item of xs) { let text = item?; break; }
    return Option.Some(true);
}
function unadmitted(xs: List<Option<string>>): Option<bool> {
    for (let item of xs) { let text = item?; while (false) { break; } }
    return Option.Some(true);
}
"#;

#[test]
fn unrelated_and_unadmitted_option_lists_keep_the_first_blocker() {
    probe_with_typed(
        "iteration::option_text::unrelated_and_unadmitted_option_lists_keep_the_first_blocker",
        OTHER_SOURCES,
        |request, module, receipt, typed| {
            let capture = loader::load_installed(request.clone()).unwrap();
            let pinned = Statistics {
                work: 331_264,
                entries: 49_620,
                max_depth: 12,
            };
            assert_eq!(typed.statistics(), pinned);
            let (exact, short) = limits(pinned);
            assert_eq!(analyze_typed(&capture, exact).statistics(), pinned);
            for cap in short {
                crate::limit::assert_typed_limit(&analyze_typed(&capture, cap));
            }
            assert_pre_equality_graph_cap(receipt, (189_508, 4_050, 3));
            let stats = Statistics {
                work: 189_824,
                entries: 4_184,
                max_depth: 5,
            };
            let (exact, short) = limits(stats);
            let graph = analyze_ownership(receipt, exact);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert_eq!(graph.statistics(), stats);
            assert!(graph.findings().is_empty());
            for cap in short {
                let limited = analyze_ownership(receipt, cap);
                crate::limit::assert_graph_limit(&limited);
            }
            for (name, work, entries) in [
                ("unused", 1_955, 69),
                ("arbitrary", 1_958, 70),
                ("sibling", 1_958, 70),
                ("unadmitted", 1_955, 69),
            ] {
                let function = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let stats = Statistics {
                    work,
                    entries,
                    max_depth: 0,
                };
                let (exact, short) = limits(stats);
                let run = |cap| lower_body(receipt, module, &function.id, cap).unwrap();
                let report = run(exact);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert_eq!(report.statistics(), stats);
                assert!(report.mir().is_none() && report.findings().is_empty());
                let blocker = report.blocked_at().unwrap();
                assert_eq!(blocker.span.bytes(), b"xs: List<Option<string>>");
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == function.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Incomplete);
                assert_eq!(root.blocked_at().unwrap().node, blocker.node);
                assert!(root.dependencies().is_empty() && root.ordinary_mir().is_none());
                for cap in short {
                    let limited = run(cap);
                    crate::limit::assert_body_limit(&limited);
                }
            }
        },
    );
}

#[test]
fn dead_option_loop_stays_pending_at_the_typed_boundary() {
    probe_report(
        "iteration::option_text::dead_option_loop_stays_pending_at_the_typed_boundary",
        r#"
function dead(xs: List<Option<string>>): Option<bool> {
    return Option.Some(true);
    for (let item of xs) { let text = item?; continue; }
}
"#,
        |request, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            let capture = loader::load_installed(request.clone()).unwrap();
            let pinned = Statistics {
                work: 322_444,
                entries: 48_323,
                max_depth: 12,
            };
            assert_eq!(typed.statistics(), pinned);
            let (exact, short) = limits(pinned);
            assert_eq!(analyze_typed(&capture, exact).statistics(), pinned);
            for cap in short {
                crate::limit::assert_typed_limit(&analyze_typed(&capture, cap));
            }
            assert!(typed.receipt().is_none());
            let report = typed.result_use().unwrap();
            let [first] = report.pending() else {
                panic!("one typed blocker")
            };
            assert_eq!(first.blocker, ResultUseBlocker::Storage);
            assert!(first.site.span.bytes().starts_with(b"for (let item of xs)"));
            assert!(report.discards().is_empty());
        },
    );
}

/// Reviews `3987757f` (Standards, `d8e067e`) and `ed35f525` (Spec, `437c266`)
/// of the yielded `Option<string>` cursor. `Some(second read)` expects the
/// independent `R0200`; `None` expects the source formal to stay pending,
/// which means the typed result-use check did not stamp `xs`.
/// The last column is the whole-graph cap before the i32 equality source root
/// (ri-785.9, metering rule 11).
#[allow(clippy::type_complexity)]
const REVIEW_COUNTEREXAMPLES: [(
    &str,
    Option<&str>,
    Statistics,
    Statistics,
    (usize, usize, usize),
); 5] = [
    // 3987757f: an ordinary body that borrows the carrier. A stamped `xs`
    // moved the blocker to `&item` at 4,215/429/6.
    (
        "function probe(xs: List<Option<string>>): Option<bool> { for (let item of xs) { let _ = &item; break; } return Option.Some(true); }",
        None,
        Statistics {
            work: 1_855,
            entries: 19,
            max_depth: 0,
        },
        Statistics {
            work: 186_285,
            entries: 4_114,
            max_depth: 5,
        },
        (185_969, 3_980, 3),
    ),
    // 3987757f: an unused `List<string>` sibling. A stamped `xs` moved the
    // blocker to `spare` at 3,048/214/3.
    (
        "function probe(xs: List<Option<string>>, spare: List<string>): Option<bool> { for (let item of xs) { let text = item?; break; } return Option.Some(true); }",
        None,
        Statistics {
            work: 1_864,
            entries: 23,
            max_depth: 0,
        },
        Statistics {
            work: 186_476,
            entries: 4_118,
            max_depth: 5,
        },
        (186_160, 3_984, 3),
    ),
    // ed35f525: the postfix fixture with an unsupported `List<bool>` sibling.
    (
        "function probe(xs: List<Option<string>>, spare: List<bool>): Option<bool> { for (let item of xs) { let text = item?; let _ = &text; } return Option.Some(true); }",
        None,
        Statistics {
            work: 1_866,
            entries: 24,
            max_depth: 0,
        },
        Statistics {
            work: 186_538,
            entries: 4_119,
            max_depth: 5,
        },
        (186_222, 3_985, 3),
    ),
    // ed35f525: an ordinary body keeps its reached move finding.
    (
        "function probe(xs: List<Option<string>>): Option<bool> { for (let item of xs) { let s = \"owned\"; let moved = s; let again = s; break; } return Option.Some(true); }",
        Some("again = s"),
        Statistics {
            work: 4_717,
            entries: 721,
            max_depth: 5,
        },
        Statistics {
            work: 189_392,
            entries: 4_817,
            max_depth: 5,
        },
        (189_076, 4_683, 5),
    ),
    (
        "function probe(xs: List<Option<string>>): void { for (let item of xs) { let s = \"owned\"; let moved = s; let again = s; break; } }",
        Some("again = s"),
        Statistics {
            work: 3_535,
            entries: 474,
            max_depth: 5,
        },
        Statistics {
            work: 187_970,
            entries: 4_570,
            max_depth: 5,
        },
        (187_654, 4_436, 5),
    ),
];

#[test]
fn review_counterexamples_keep_their_outcomes_at_exact_caps() {
    for (source, second_read, targeted, whole, pre_equality) in REVIEW_COUNTEREXAMPLES {
        probe(
            "iteration::option_text::review_counterexamples_keep_their_outcomes_at_exact_caps",
            source,
            |_, module, receipt| {
                let f = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some("probe"))
                    .unwrap();
                let expected = if second_read.is_some() {
                    Completion::SourceRejected
                } else {
                    Completion::Incomplete
                };
                let (exact, short) = limits(targeted);
                let run = |cap| lower_body(receipt, module, &f.id, cap).unwrap();
                let target = run(exact);
                assert_eq!(target.completion(), expected, "{source}");
                assert_eq!(target.statistics(), targeted, "{source}");
                assert!(target.mir().is_none());
                for cap in short {
                    let limited = run(cap);
                    crate::limit::assert_body_limit(&limited);
                }
                assert_pre_equality_graph_cap(receipt, pre_equality);
                let (exact, short) = limits(whole);
                let graph = analyze_ownership(receipt, exact);
                assert_eq!(graph.statistics(), whole, "{source}");
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == f.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), expected, "{source}");
                assert!(root.ordinary_mir().is_none());
                for cap in short {
                    let limited = analyze_ownership(receipt, cap);
                    crate::limit::assert_graph_limit(&limited);
                    // Findings recorded before exhaustion survive it.
                    let key = |f: &compiler::analysis::ownership::Finding<'_>| {
                        (f.code(), f.site.node, f.related.node)
                    };
                    assert!(
                        limited
                            .findings()
                            .iter()
                            .all(|f| graph.findings().iter().any(|g| key(g) == key(f)))
                    );
                    assert!(limited.roots().iter().all(|r| r.ordinary_mir().is_none()));
                }
                let Some(second_read) = second_read else {
                    assert_eq!(graph.completion(), GraphCompletion::Incomplete);
                    let blocker = target.blocked_at().unwrap();
                    assert_eq!(blocker.span.bytes(), b"xs: List<Option<string>>");
                    assert_eq!(root.blocked_at().unwrap().node, blocker.node);
                    assert!(target.findings().is_empty() && graph.findings().is_empty());
                    assert!(root.dependencies().is_empty());
                    return;
                };
                assert_eq!(graph.completion(), GraphCompletion::SourceRejected);
                for findings in [target.findings(), graph.findings()] {
                    let [finding] = findings else {
                        panic!("expected only the second read: {source}")
                    };
                    assert_eq!(finding.code(), "R0200");
                    for (site, text) in [
                        (finding.site, second_read.as_bytes()),
                        (finding.related, b"moved = s".as_slice()),
                    ] {
                        let start = module
                            .snapshot
                            .bytes()
                            .windows(text.len())
                            .position(|s| s == text)
                            .unwrap()
                            + text.len()
                            - 1;
                        assert_eq!((site.span.start(), site.span.end()), (start, start + 1));
                    }
                }
            },
        );
    }
}

fn check_paths<'g>(
    name: &str,
    mir: &Mir<'g>,
    element: OptionTextDisposal<'g>,
    [open, advance, release, whole]: [CollectionProvider<'g>; 4],
) {
    let [entry] = mir
        .blocks
        .iter()
        .filter(|b| matches!(b.operation, Operation::OpenOwnedListOptionText { .. }))
        .collect::<Vec<_>>()[..]
    else {
        panic!("one source transfer")
    };
    let Operation::OpenOwnedListOptionText {
        source,
        cursor,
        source_backing,
        cursor_backing,
        provider,
    } = entry.operation
    else {
        unreachable!()
    };
    assert_eq!(source.root.span.bytes(), b"xs: List<Option<string>>");
    assert_eq!(provider.implementation.id, open.implementation.id);
    assert_eq!(provider.bridge.id, open.bridge.id);
    assert_eq!(source_backing.storage, CollectionStorage::Elements);
    assert_eq!(cursor_backing.storage, CollectionStorage::UnyieldedElements);
    assert_eq!(
        source_backing.artifact.context(),
        provider.artifact.context()
    );
    assert_eq!(
        cursor_backing.artifact.executable(),
        provider.artifact.executable()
    );
    let Terminator::Goto(head) = entry.terminator else {
        panic!("advance immediately after transfer")
    };
    let check_advance = |block: &Block<'g>| {
        let Operation::AdvanceOwnedListOptionText {
            cursor: from,
            into,
            some,
            none,
            provider,
        } = block.operation
        else {
            panic!("missing protected next")
        };
        assert!(same_place(from, cursor));
        assert_eq!((some, none), (element.some, element.none));
        assert_eq!(provider.implementation.id, advance.implementation.id);
        assert_eq!(provider.bridge.id, advance.bridge.id);
        let Terminator::Branch { yes, no } = block.terminator else {
            panic!("outer Some/None")
        };
        (into, yes, no)
    };
    let (item, yielded, exhausted) = check_advance(&mir.blocks[head]);
    assert!(!same_place(item, cursor) && !same_place(item, source));
    let cleanup = |path: &[&Block<'g>]| {
        obligations(path)
            .filter_map(|a| match a {
                Obligation::Drop(place) => Some(place.root.span.bytes()),
                Obligation::EndStorage(_) => None,
                Obligation::DropYieldedOptionText {
                    operand,
                    element: actual,
                } => {
                    assert!(same_place(operand, item));
                    assert_eq!(actual, element);
                    Some(b"item".as_slice())
                }
                Obligation::DisposeOwnedListIterOptionText {
                    cursor: operand,
                    implementation,
                    bridge,
                    element: actual,
                } => {
                    assert!(same_place(operand, cursor));
                    assert_eq!(
                        (implementation, bridge, actual),
                        (&release.implementation.id, &release.bridge.id, element)
                    );
                    Some(b"cursor".as_slice())
                }
                Obligation::DisposeListOptionText {
                    operand,
                    implementation,
                    bridge,
                    element: actual,
                } => {
                    assert!(same_place(operand, source));
                    assert_eq!(
                        (implementation, bridge, actual),
                        (&whole.implementation.id, &whole.bridge.id, element)
                    );
                    Some(b"whole list".as_slice())
                }
                other => panic!("unexpected obligation {other:?}"),
            })
            .collect::<Vec<_>>()
    };
    let export = |path: &[&Block<'g>], variant, payload: bool| {
        assert_eq!(path.last().unwrap().terminator, Terminator::Return);
        assert!(path.iter().all(|b| !matches!(
            b.operation,
            Operation::AdvanceOwnedListOptionText { .. }
                | Operation::OpenOwnedListOptionText { .. }
        )));
        let [(tag, input, output)] = path
            .iter()
            .filter_map(|b| match b.operation {
                Operation::ConstructVariant {
                    variant,
                    payload,
                    into,
                } => Some((variant, payload, into)),
                _ => None,
            })
            .collect::<Vec<_>>()[..]
        else {
            panic!("exactly one output")
        };
        assert_eq!((tag, input.is_some()), (variant, payload));
        let [(index, retired)] = path
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match b.operation {
                Operation::Export(place) => Some((i, place)),
                _ => None,
            })
            .collect::<Vec<_>>()[..]
        else {
            panic!("exactly one export")
        };
        assert!(same_place(output, retired));
        index
    };
    let empty = straight_blocks(mir, exhausted);
    let exported = export(&empty, element.some, true);
    assert_eq!(cleanup(&empty[..exported]), [b"cursor".as_slice()]);
    assert_eq!(cleanup(&empty[exported..]), [b"outer: string".as_slice()]);
    let reached = straight_blocks(mir, yielded);
    if name == "withCurrent" {
        let exported = export(&reached, element.some, true);
        assert_eq!(
            cleanup(&reached[..exported]),
            [b"item".as_slice(), b"cursor"]
        );
        let (yes, _) = branch_targets(mir);
        let pre_open = straight_blocks(mir, yes);
        let exported = export(&pre_open, element.some, true);
        assert!(cleanup(&pre_open[..exported]).is_empty());
        assert_eq!(
            cleanup(&pre_open[exported..]),
            [b"outer: string".as_slice(), b"whole list"]
        );
        return;
    }
    let [stage] = reached
        .iter()
        .filter_map(|b| match b.operation {
            Operation::Move {
                from,
                into: Some(into),
            } if same_place(item, from) => Some(into),
            _ => None,
        })
        .collect::<Vec<_>>()[..]
    else {
        panic!("stage the whole actual item once")
    };
    let split = reached.last().unwrap();
    assert!(
        matches!(split.operation, Operation::TestVariant { operand, success }
        if same_place(operand, stage) && success == element.some)
    );
    let Terminator::Branch { yes, no } = split.terminator else {
        panic!("inner Some/None")
    };
    let failure = straight_blocks(mir, no);
    assert!(
        matches!(failure[0].operation, Operation::EndEmptyOption(operand) if same_place(operand, stage))
    );
    let exported = export(&failure, element.none, false);
    assert_eq!(failure[exported].site.node, split.site.node);
    assert!(cleanup(&failure[..exported]).is_empty());
    assert!(!failure.iter().any(
        |b| matches!(b.operation, Operation::TakeOptionPayload { .. })
            || b.site.span.bytes() == b"\"after postfix\""
    ));
    let expected = if name == "onItem" {
        &([b"owner".as_slice(), b"cursor", b"outer: string"])[..]
    } else {
        &([b"cursor".as_slice(), b"outer: string"])[..]
    };
    assert_eq!(cleanup(&failure[exported..]), expected);
    let success = straight_blocks(mir, yes);
    let [(operand, text)] = success
        .iter()
        .filter_map(|b| match b.operation {
            Operation::TakeOptionPayload { operand, into } => Some((operand, into)),
            _ => None,
        })
        .collect::<Vec<_>>()[..]
    else {
        panic!("take the actual Some payload once")
    };
    assert!(same_place(operand, stage));
    assert_eq!(text.root.span.bytes(), b"text");
    if name == "onItem" {
        let (reinitialized, _, _) = check_advance(success.last().unwrap());
        assert!(same_place(reinitialized, item));
        assert_eq!(cleanup(&success), [b"suffix".as_slice(), b"text", b"owner"]);
        assert!(
            !success
                .iter()
                .any(|b| matches!(b.operation, Operation::Export(_)))
        );
    } else {
        assert!(success.iter().any(
            |b| matches!(b.operation, Operation::Move { from, into: Some(into) }
            if same_place(from, text) && into.root.span.bytes() == b"moved")
        ));
        let exported = export(&success, element.some, true);
        assert_eq!(
            cleanup(&success[..exported]),
            [b"moved".as_slice(), b"cursor"]
        );
        assert_eq!(cleanup(&success[exported..]), [b"outer: string".as_slice()]);
    }
}
