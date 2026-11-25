use super::*;
use compiler::analysis::ownership::{RootReport, TextElementDisposal};
use compiler::analysis::typed::{Primitive, PrimitiveRole};

const PATHS: &str = r#"
function probe(gate: Option<string>, first: string, last: string): Option<bool> {
    for (let item of [first, gate?, last]) { let _ = &item; break; }
    return Option.Some(true);
}
function zero(gate: Option<string>, first: string, last: string): Option<bool> {
    for (let item of ([((gate?)), first, last])) { let _ = &item; break; }
    return Option.Some(true);
}
function two(gate: Option<string>, first: string, second: string, last: string): Option<bool> {
    for (let item of [first, second, gate?, last]) { let _ = &item; break; }
    return Option.Some(true);
}
function terminal(gate: Option<string>.None, first: string, last: string): Option<bool> {
    for (let item of [first, gate?, last]) { let _ = &item; break; }
    return Option.Some(true);
}
function moved(gate: Option<string>, first: string, last: string): Option<bool> {
    let taken = first;
    for (let item of [first, gate?, last]) { let _ = &item; break; }
    return Option.Some(true);
}
"#;

#[test]
fn installed_list_source_can_propagate_before_cursor_commitment() {
    probe_with_typed(
        "iteration::source::installed_list_source_can_propagate_before_cursor_commitment",
        PATHS,
        |request, module, receipt, typed| {
            let capture = loader::load_installed(request.clone()).unwrap();
            let foreign = capture
                .modules()
                .iter()
                .find(|m| m.key == module.key)
                .unwrap();
            let (exact, short) = limits(receipt.statistics());
            assert_eq!(
                analyze_typed(&capture, exact)
                    .receipt()
                    .unwrap()
                    .statistics(),
                receipt.statistics()
            );
            for cap in short {
                let limited = analyze_typed(&capture, cap);
                crate::limit::assert_typed_limit(&limited);
            }
            let measured = analyze_ownership(receipt, AnalysisLimits::default());
            let (exact, short) = limits(measured.statistics());
            let graph = analyze_ownership(receipt, exact);
            assert_eq!(graph.statistics(), measured.statistics());
            assert_eq!(graph.completion(), GraphCompletion::SourceRejected);
            assert_eq!(graph.test_mode(), request.test_mode);
            assert_eq!(graph.context(), receipt.context());
            for cap in short {
                let limited = analyze_ownership(receipt, cap);
                crate::limit::assert_graph_limit(&limited);
            }
            for (name, prefix_len) in [
                ("probe", 1),
                ("zero", 0),
                ("two", 2),
                ("terminal", 1),
                ("moved", 1),
            ] {
                let function = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let run = |cap| lower_body(receipt, module, &function.id, cap).unwrap();
                let stats = run(AnalysisLimits::default()).statistics();
                let (exact, short) = limits(stats);
                assert!(lower_body(receipt, foreign, &function.id, exact).is_err());
                let body = run(exact);
                assert_eq!(body.statistics(), stats);
                for cap in short {
                    let limited = run(cap);
                    crate::limit::assert_body_limit(&limited);
                }
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == function.node)
                    .unwrap();
                assert_eq!(body.completion(), root.ordinary_completion());
                if name == "moved" {
                    assert_eq!(body.completion(), Completion::SourceRejected);
                    assert!(body.mir().is_none() && root.ordinary_mir().is_none());
                    let [finding] = body.findings() else {
                        panic!("one reached move finding")
                    };
                    assert_eq!(finding.code(), "R0200");
                    assert_eq!(finding.site.span.bytes(), b"first");
                    assert_eq!(finding.related.span.bytes(), b"first");
                    assert!(finding.related.span.start() < finding.site.span.start());
                    assert!(
                        graph
                            .findings()
                            .iter()
                            .any(|f| f.site.node == finding.site.node
                                && f.related.node == finding.related.node)
                    );
                    continue;
                }
                assert_eq!(body.completion(), Completion::Checked, "{name}");
                assert!(body.findings().is_empty());
                assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
                for mir in [body.mir().unwrap(), root.ordinary_mir().unwrap()] {
                    check_paths(name, prefix_len, mir, root, typed);
                }
            }
        },
    );
}

fn check_paths<'g>(
    name: &str,
    prefix_len: usize,
    mir: &Mir<'g>,
    root: &RootReport<'g>,
    typed: &TypedReport<'g>,
) {
    use CollectionOperation::{DisposeIterator, Iterate, Next};
    use ListTraversal::Owned;
    assert!(mir.cancellation().is_none() && mir.replacement_cancellation().is_none());
    let option = typed.option_role().unwrap();
    let path = straight_blocks(mir, mir.entry);
    let mut moves = path.iter().filter_map(|b| match b.operation {
        Operation::Move {
            from,
            into: Some(into),
        } => Some((from.root.span.bytes(), into)),
        _ => None,
    });
    let stages: Vec<_> = [b"first: string".as_slice(), b"second: string"][..prefix_len]
        .iter()
        .map(|&name| {
            let (from, into) = moves.next().unwrap();
            assert_eq!(from, name);
            into
        })
        .collect();
    let (gate, operand) = moves.next().unwrap();
    assert!(gate.starts_with(b"gate: Option<string>"));
    assert!(moves.next().is_none());
    let (failure, success) = if name == "terminal" {
        assert_eq!(path.last().unwrap().terminator, Terminator::Return);
        assert!(root.dependencies().is_empty());
        (path, None)
    } else {
        let test = path.last().unwrap();
        assert!(
            matches!(test.operation, Operation::TestVariant { operand: tested, success }
            if same_place(tested, operand) && success == &option.some.id)
        );
        let Terminator::Branch { yes, no } = test.terminator else {
            panic!("split on the source gate")
        };
        (straight_blocks(mir, no), Some(straight_blocks(mir, yes)))
    };
    let failure_export = failure
        .iter()
        .position(|b| matches!(b.operation, Operation::Export(_)))
        .unwrap();
    let Operation::Export(output) = failure[failure_export].operation else {
        unreachable!()
    };
    assert_eq!(failure[failure_export].site.span.bytes(), b"gate?");
    assert_eq!(failure.last().unwrap().terminator, Terminator::Return);
    assert!(failure.iter().all(|b| {
        !matches!(b.operation, Operation::CommitListText { .. } | Operation::OpenOwnedListText { .. }
            | Operation::AdvanceOwnedListText { .. } | Operation::TakeOptionPayload { .. })
            && b.site.span.bytes() != b"return Option.Some(true);"
            && !matches!(b.operation, Operation::Move { from, .. } if from.root.span.bytes() == b"last: string")
    }));
    let variants: Vec<_> = failure
        .iter()
        .filter_map(|b| match b.operation {
            Operation::ConstructVariant {
                variant,
                payload,
                into,
            } => Some((variant, payload, into)),
            _ => None,
        })
        .collect();
    assert!(
        matches!(variants.as_slice(), [(variant, None, into)] if *variant == &option.none.id && same_place(*into, output))
    );
    assert!(
        !obligations(&failure[..failure_export]).any(|c| !matches!(c, Obligation::EndStorage(_)))
    );
    let mut expected = [b"first".as_slice(), b"second"][..prefix_len].to_vec();
    expected.reverse();
    expected.push(b"last: string");
    if name == "zero" {
        expected.push(b"first: string");
    }
    assert_eq!(
        obligations(&failure[failure_export + 1..])
            .map(|c| match c {
                Obligation::Drop(place) => place.root.span.bytes(),
                other => panic!("unexpected failure cleanup: {other:?}"),
            })
            .collect::<Vec<_>>(),
        expected
    );
    let Some(success) = success else { return };
    let mut staged = stages;
    let meaningful: Vec<_> = success
        .iter()
        .map(|b| &b.operation)
        .filter(|op| !matches!(op, Operation::None))
        .collect();
    let [
        Operation::TakeOptionPayload {
            operand: taken,
            into: middle,
        },
        late @ ..,
        Operation::CommitListText {
            elements,
            into: source,
            backing,
            provider: construct,
        },
        Operation::OpenOwnedListText {
            source: opened,
            cursor,
            source_backing,
            provider: open,
            ..
        },
        Operation::AdvanceOwnedListText {
            cursor: advanced,
            into: item,
            some,
            none,
            provider: advance,
        },
    ] = meaningful.as_slice()
    else {
        panic!(
            "source should take, stage the suffix, commit once, open once, and advance: {meaningful:?}"
        );
    };
    assert!(same_place(*taken, operand));
    staged.push(*middle);
    let late_names: Vec<_> = late
        .iter()
        .map(|op| match op {
            Operation::Move {
                from,
                into: Some(into),
            } => {
                staged.push(*into);
                from.root.span.bytes()
            }
            other => panic!("expected a late source element move: {other:?}"),
        })
        .collect();
    assert_eq!(
        late_names,
        if name == "zero" {
            vec![b"first: string".as_slice(), b"last: string"]
        } else {
            vec![b"last: string".as_slice()]
        }
    );
    assert_eq!(staged.len(), elements.len());
    assert!(staged.iter().zip(elements).all(|(a, b)| same_place(*a, *b)));
    assert!(same_place(*source, *opened) && same_place(*cursor, *advanced));
    assert_eq!(backing.artifact, source_backing.artifact);
    assert_eq!(backing.storage, CollectionStorage::Elements);
    assert_eq!(
        backing.declaration.id,
        typed.list_role().unwrap().declaration.id
    );
    assert_eq!(*construct, typed.list_operations_artifact().unwrap());
    assert_eq!((open.artifact, advance.artifact), (*construct, *construct));
    assert_eq!((*some, *none), (&option.some.id, &option.none.id));
    let collection = |operation| {
        typed
            .collection_protection()
            .find(|p| p.operation == operation && p.coverage == OwnershipCoverage::Clear)
            .unwrap()
            .provider
            .unwrap()
    };
    let release = collection(DisposeIterator(Owned));
    for (operation, provider) in [
        (Iterate(Owned), *open),
        (Next(Owned), *advance),
        (DisposeIterator(Owned), release),
    ] {
        let selected = collection(operation);
        assert_eq!(provider.implementation.id, selected.implementation.id);
        assert_eq!(provider.bridge.id, selected.bridge.id);
        assert!(
            root.dependencies()
                .iter()
                .any(|e| e.declaration == &selected.implementation.id
                    && e.contract
                        == DependencyContract::ListTextCollection {
                            operation,
                            bridge: &selected.bridge.id
                        })
        );
    }
    let string = typed
        .primitive_providers()
        .iter()
        .find(|p| p.role == PrimitiveRole::Dispose && p.primitive == Primitive::String)
        .unwrap();
    let element = TextElementDisposal {
        implementation: &string.implementation.id,
        bridge: &string.binding.unwrap().declaration.id,
    };
    assert!(
        root.dependencies()
            .iter()
            .any(|e| e.declaration == element.implementation
                && e.contract
                    == DependencyContract::ListTextElementDisposal {
                        bridge: element.bridge
                    })
    );
    let Terminator::Branch { yes, no } = success.last().unwrap().terminator else {
        panic!("split on next")
    };
    for (pc, yielded) in [(yes, true), (no, false)] {
        let path = straight_blocks(mir, pc);
        assert_eq!(path.last().unwrap().terminator, Terminator::Return);
        let active: Vec<_> = obligations(&path)
            .filter(|c| !matches!(c, Obligation::EndStorage(_)))
            .collect();
        let suffix = |c| {
            matches!(c, Obligation::DisposeOwnedListIterText { cursor: retired, implementation, bridge, element: e }
            if same_place(retired, *cursor) && implementation == &release.implementation.id && bridge == &release.bridge.id && e == element)
        };
        if yielded {
            assert!(
                matches!(active.as_slice(), [Obligation::Drop(current), rest] if same_place(*current, *item) && suffix(*rest))
            );
        } else {
            assert!(matches!(active.as_slice(), [rest] if suffix(*rest)));
        }
        assert!(
            path.iter()
                .any(|b| matches!(b.operation, Operation::Export(_))
                    && b.site.span.bytes() == b"return Option.Some(true);")
        );
    }
}

const CONTROLS: &str = r#"function make(a: string): List<string> { return [a]; }
function computed(a: string): void { for (let item of make(a)) { break; } }
function literal(a: string): void { for (let item of [a]) { break; } }
function unused(a: string): void { let unused = [a]; }
function named(xs: List<string>): void { let moved = xs; for (let item of xs) { break; } }
function computedPostfix(gate: Option<string>, a: string): Option<bool> {
    for (let item of make(gate?)) { let _ = &item; break; }
    return Option.Some(true);
}
"#;

#[test]
fn grouped_computed_source_keeps_the_pre_literal_budget() {
    let source = CONTROLS.replace("make(gate?)", "(((make(gate?))))");
    probe(
        "iteration::source::grouped_computed_source_keeps_the_pre_literal_budget",
        &source,
        |_, module, receipt| {
            let f = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("computedPostfix"))
                .unwrap();
            let target = lower_body(
                receipt,
                module,
                &f.id,
                AnalysisLimits {
                    max_work: 4166,
                    max_entries: 464,
                    max_depth: 4,
                },
            )
            .unwrap();
            assert_eq!(target.completion(), Completion::Incomplete);
            assert!(target.mir().is_none() && target.findings().is_empty());
            assert_eq!(
                target.blocked_at().unwrap().span.bytes(),
                b"for (let item of (((make(gate?))))) { let _ = &item; break; }"
            );
            assert_pre_equality_graph_cap(receipt, (192469, 4596, 4));
            let graph = analyze_ownership(
                receipt,
                AnalysisLimits {
                    max_work: 195319,
                    max_entries: 4730,
                    max_depth: 5,
                },
            );
            assert_eq!(graph.completion(), GraphCompletion::SourceRejected); // independent named-source reuse
            let root = graph
                .roots()
                .iter()
                .find(|r| r.site().module == &module.key && r.site().node == f.node)
                .unwrap();
            assert_eq!(root.ordinary_completion(), Completion::Incomplete);
            assert!(root.ordinary_mir().is_none());
            assert_eq!(
                root.blocked_at().unwrap().node,
                target.blocked_at().unwrap().node
            );
        },
    );
}

#[test]
fn unproved_list_sources_keep_their_old_caps() {
    probe(
        "iteration::source::unproved_list_sources_keep_their_old_caps",
        CONTROLS,
        |_, module, receipt| {
            assert_pre_equality_graph_cap(receipt, (192_283, 4593, 4));
            let stats = Statistics {
                work: 195_133,
                entries: 4727,
                max_depth: 5,
            };
            let (exact, short) = limits(stats);
            let graph = analyze_ownership(receipt, exact);
            assert_eq!(graph.statistics(), stats);
            assert_eq!(graph.completion(), GraphCompletion::SourceRejected);
            for cap in short {
                let limited = analyze_ownership(receipt, cap);
                crate::limit::assert_graph_limit(&limited);
            }
            for (name, work, entries, max_depth, blocked) in [
                ("make", 1973, 74, 2, "[a]"),
                (
                    "computed",
                    1973,
                    74,
                    2,
                    "for (let item of make(a)) { break; }",
                ),
                ("literal", 1973, 74, 2, "for (let item of [a]) { break; }"),
                ("unused", 1972, 74, 1, "unused"),
                ("named", 2294, 184, 4, ""),
                (
                    "computedPostfix",
                    4160,
                    461,
                    4,
                    "for (let item of make(gate?)) { let _ = &item; break; }",
                ),
            ] {
                let function = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let stats = Statistics {
                    work,
                    entries,
                    max_depth,
                };
                let (exact, short) = limits(stats);
                let body = lower_body(receipt, module, &function.id, exact).unwrap();
                assert_eq!(body.statistics(), stats);
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == function.node)
                    .unwrap();
                assert!(body.mir().is_none() && root.ordinary_mir().is_none());
                assert_eq!(root.ordinary_completion(), body.completion());
                if name == "named" {
                    assert_eq!(body.completion(), Completion::SourceRejected);
                    let [finding] = body.findings() else {
                        panic!("moved named source must still reject")
                    };
                    assert_eq!(
                        (
                            finding.code(),
                            finding.site.span.bytes(),
                            finding.related.span.bytes()
                        ),
                        ("R0200", b"xs".as_slice(), b"xs".as_slice())
                    );
                } else {
                    assert_eq!(body.completion(), Completion::Incomplete);
                    assert_eq!(body.blocked_at().unwrap().span.bytes(), blocked.as_bytes());
                    assert_eq!(
                        body.blocked_at().unwrap().node,
                        root.blocked_at().unwrap().node
                    );
                    assert!(body.findings().is_empty());
                }
                for cap in short {
                    let limited = lower_body(receipt, module, &function.id, cap).unwrap();
                    crate::limit::assert_body_limit(&limited);
                }
            }
        },
    );
}

#[test]
fn a_return_during_construction_does_not_acknowledge_result_debt() {
    probe_report(
        "iteration::source::a_return_during_construction_does_not_acknowledge_result_debt",
        r#"function debt(gate: Option<string>, first: string, last: string): Option<bool> {
    let owed = Err<bool,i32>(42i32);
    for (let item of [first, gate?, last]) { let _ = &item; break; }
    let _ = owed;
    return Option.Some(true);
}"#,
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::SourceRejected);
            assert!(typed.receipt().is_none());
            let [debt] = typed.result_use().unwrap().discards() else {
                panic!("one real Result debt")
            };
            assert_eq!(debt.code(), "R0103");
            assert_eq!(debt.origin.span.bytes(), b"Err<bool,i32>(42i32)");
            assert_eq!(debt.discard.span.bytes(), b"gate?");
        },
    );
}

#[test]
fn element_disposer_does_not_inherit_string_source_admission() {
    probe(
        "iteration::source::element_disposer_does_not_inherit_string_source_admission",
        r#"type Owner = { text: string; }
implement dispose(value: Owner): void { dispose(value.text); }
function elementDisposer(gate: Option<Owner>, first: Owner, last: Owner): Option<bool> {
    for (let item of [first, gate?, last]) { break; }
    return Option.Some(true);
}"#,
        |_, module, receipt| {
            let function = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("elementDisposer"))
                .unwrap();
            let stats = Statistics {
                work: 1894,
                entries: 33,
                max_depth: 0,
            };
            let (exact, short) = limits(stats);
            let run = |cap| lower_body(receipt, module, &function.id, cap).unwrap();
            let body = run(exact);
            assert_eq!(body.statistics(), stats);
            assert_eq!(body.completion(), Completion::Incomplete);
            assert_eq!(
                body.blocked_at().unwrap().span.bytes(),
                b"gate: Option<Owner>"
            );
            assert!(body.mir().is_none() && body.findings().is_empty());
            for cap in short {
                let limited = run(cap);
                crate::limit::assert_body_limit(&limited);
            }
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let root = graph
                .roots()
                .iter()
                .find(|r| r.site().module == &module.key && r.site().node == function.node)
                .unwrap();
            assert_eq!(root.ordinary_completion(), body.completion());
            assert_eq!(
                root.blocked_at().unwrap().node,
                body.blocked_at().unwrap().node
            );
            assert!(root.ordinary_mir().is_none());
        },
    );
}
