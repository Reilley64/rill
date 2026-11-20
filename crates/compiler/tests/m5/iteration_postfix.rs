use super::*;
use compiler::loader::DeclarationId;

const POSTFIX: &str = r#"
function onPostfix(xs: List<string>, gate: Option<bool>, outer: string): Option<bool> {
    for (let item of xs) {
        let owner = "reached body owner";
        let view = &item;
        let _ = view;
        let success = gate?;
        return Option.Some(success);
    }
    return Option.Some(false);
}
function knownSome(xs: List<string>, gate: Option<bool>.Some, outer: string): Option<bool> {
    for (let item of xs) {
        let owner = "reached body owner";
        let success = gate?;
        return Option.Some(success);
    }
    return Option.Some(false);
}
function knownNone(xs: List<string>, gate: Option<bool>.None, outer: string): Option<bool> {
    for (let item of xs) {
        let owner = "reached body owner";
        let success = gate?;
        dispose(item);
        return Option.Some(success);
    }
    return Option.Some(false);
}
function afterMove(xs: List<string>, gate: Option<bool>, outer: string): Option<bool> {
    for (let item of xs) {
        let moved = item;
        let owner = "reached body owner";
        gate?;
        let suffix = "continuation only";
        continue;
    }
    return Option.Some(false);
}
"#;

#[test]
fn installed_postfix_with_live_string_cursor() {
    probe_with_typed(
        "iteration::text::postfix::installed_postfix_with_live_string_cursor",
        POSTFIX,
        |request, module, receipt, typed| {
            let capture = loader::load_installed(request.clone()).unwrap();
            let stats = receipt.statistics();
            let (exact, short) = limits(stats);
            assert_eq!(
                analyze_typed(&capture, exact)
                    .receipt()
                    .unwrap()
                    .statistics(),
                stats
            );
            assert_eq!(
                receipt.not_performed(),
                &[Stage::Ownership, Stage::Execution]
            );
            for cap in short {
                let limited = analyze_typed(&capture, cap);
                assert_eq!(limited.completion(), TypedCompletion::ResourceLimit);
                assert!(limited.receipt().is_none());
            }
            let measured = analyze_ownership(receipt, AnalysisLimits::default());
            let (exact, short) = limits(measured.statistics());
            let graph = analyze_ownership(receipt, exact);
            assert_eq!(graph.statistics(), measured.statistics());
            assert_eq!(graph.completion(), measured.completion());
            assert!(graph.inventory_complete());
            assert!(graph.findings().is_empty());
            assert_eq!(graph.test_mode(), request.test_mode);
            assert_eq!(graph.context(), receipt.context());
            for cap in short {
                let limited = analyze_ownership(receipt, cap);
                assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                assert!(
                    limited
                        .roots()
                        .iter()
                        .all(|root| root.ordinary_mir().is_none())
                );
                assert!(limited.findings().is_empty());
            }
            for name in ["onPostfix", "knownSome", "knownNone", "afterMove"] {
                let function = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let run = |cap| lower_body(receipt, module, &function.id, cap).unwrap();
                let stats = run(AnalysisLimits::default()).statistics();
                let (exact, short) = limits(stats);
                let body = run(exact);
                assert_eq!(body.statistics(), stats);
                assert_eq!(
                    body.completion(),
                    Completion::Checked,
                    "{name}: {:?}",
                    body.blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                );
                assert!(body.findings().is_empty());
                for cap in short {
                    let limited = run(cap);
                    assert_eq!(limited.completion(), Completion::ResourceLimit);
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == function.node)
                    .unwrap();
                assert_eq!(
                    root.ordinary_completion(),
                    Completion::Checked,
                    "{name}: {:?}",
                    root.blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                );
                assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
                assert!(matches!(
                    root.replacement_cancellation(),
                    Delivery::Unresolved
                ));
                for mir in [body.mir().unwrap(), root.ordinary_mir().unwrap()] {
                    check_paths(name, mir, module, typed, root.dependencies());
                }
            }
        },
    );
}

const OUTSIDE_SLICE: &str = r#"function complex(xs: List<string>, gate: Option<bool>): Option<bool> {
    for (let item of xs) { if (gate?) { break; } }
    return Option.Some(false);
}
function owningOperand(xs: List<string>, gate: Option<string>): Option<bool> {
    for (let item of xs) { let message = gate?; break; }
    return Option.Some(false);
}
function constructed(a: string, gate: Option<bool>): Option<bool> {
    for (let item of [a]) { let bit = gate?; return Option.Some(bit); }
    return Option.Some(false);
}
function ordinaryConstructor(xs: List<string>): Option<bool> {
    for (let item of xs) { return Option.Some(true); }
    return Option.Some(false);
}
"#;

#[test]
fn installed_postfix_keeps_unadmitted_loop_shapes_pending() {
    probe(
        "iteration::text::postfix::installed_postfix_keeps_unadmitted_loop_shapes_pending",
        OUTSIDE_SLICE,
        |request, module, receipt| {
            let typed_stats = Statistics {
                work: 332_954,
                entries: 49_349,
                max_depth: 12,
            };
            assert_eq!(receipt.statistics(), typed_stats);
            let capture = loader::load_installed(request.clone()).unwrap();
            let (exact, short) = limits(typed_stats);
            assert!(analyze_typed(&capture, exact).receipt().is_some());
            for cap in short {
                let report = analyze_typed(&capture, cap);
                assert_eq!(report.completion(), TypedCompletion::ResourceLimit);
                assert!(report.receipt().is_none());
            }

            let graph_stats = Statistics {
                work: 191_179,
                entries: 4_263,
                max_depth: 4,
            };
            let (exact, short) = limits(graph_stats);
            let graph = analyze_ownership(receipt, exact);
            assert_eq!(graph.statistics(), graph_stats);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert!(graph.findings().is_empty());
            for cap in short {
                let limited = analyze_ownership(receipt, cap);
                assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                assert!(limited.roots().iter().all(|r| r.ordinary_mir().is_none()));
            }
            for (name, work, entries, max_depth) in [
                ("complex", 1_976, 79, 0),
                ("owningOperand", 1_976, 79, 0),
                ("constructed", 3_085, 282, 4),
                ("ordinaryConstructor", 1_973, 78, 0),
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
                let run = |cap| lower_body(receipt, module, &function.id, cap).unwrap();
                let at = run(exact);
                assert_eq!(at.statistics(), stats, "{name}");
                assert_eq!(at.completion(), Completion::Incomplete, "{name}");
                assert!(at.mir().is_none() && at.findings().is_empty());
                let blocker = at.blocked_at().unwrap();
                if name == "constructed" {
                    assert!(blocker.span.bytes().starts_with(b"for ("));
                } else {
                    assert_eq!(blocker.span.bytes(), b"xs: List<string>");
                }
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == function.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Incomplete);
                assert_eq!(root.blocked_at().unwrap().node, blocker.node);
                assert!(root.ordinary_mir().is_none());
                for cap in short {
                    let limited = run(cap);
                    assert_eq!(limited.completion(), Completion::ResourceLimit);
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
            }
        },
    );
}

#[test]
fn postfix_stamps_only_a_reached_loop_with_all_text_formals_covered() {
    probe(
        "iteration::text::postfix::postfix_stamps_only_a_reached_loop_with_all_text_formals_covered",
        r#"
function deadPostfix(xs: List<string>, gate: Option<bool>): Option<bool> {
    return Option.Some(false);
    for (let item of xs) { let bit = gate?; return Option.Some(bit); }
}
function siblingPostfix(xs: List<string>, unrelated: List<string>, gate: Option<bool>): Option<bool> {
    for (let item of xs) { let bit = gate?; return Option.Some(bit); }
    return Option.Some(false);
}
"#,
        |_, module, receipt| {
            let graph_stats = Statistics {
                work: 187_799,
                entries: 4_016,
                max_depth: 3,
            };
            let (exact, short) = limits(graph_stats);
            let graph = analyze_ownership(receipt, exact);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert_eq!(graph.statistics(), graph_stats);
            assert!(graph.findings().is_empty());
            for cap in short {
                let limited = analyze_ownership(receipt, cap);
                assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                assert!(limited.roots().iter().all(|r| r.ordinary_mir().is_none()));
            }
            for (name, work, entries) in [("deadPostfix", 1_912, 47), ("siblingPostfix", 1_914, 48)]
            {
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
                assert_eq!(blocker.span.bytes(), b"xs: List<string>");
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == function.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Incomplete);
                assert_eq!(root.blocked_at().unwrap().node, blocker.node);
                assert!(root.ordinary_mir().is_none());
                for cap in short {
                    let limited = run(cap);
                    assert_eq!(limited.completion(), Completion::ResourceLimit);
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
            }
        },
    );
}

pub(super) fn check_edges<'g>(
    name: &str,
    mir: &Mir<'g>,
    head: &Block<'g>,
    (item, cursor): (Place<'g>, Place<'g>),
    (some, none): (&DeclarationId, &DeclarationId),
    (release, element): (CollectionProvider<'g>, TextElementDisposal<'g>),
) {
    let clean = |path: &[&Block<'g>]| -> Vec<_> {
        obligations(path)
            .map(|action| match action {
                Obligation::DisposeOwnedListIterText {
                    cursor: retired,
                    implementation,
                    bridge,
                    element: callback,
                } => {
                    assert!(same_place(retired, cursor));
                    assert_eq!(
                        (implementation, bridge, callback),
                        (&release.implementation.id, &release.bridge.id, element)
                    );
                    b"cursor".as_slice()
                }
                Obligation::Drop(place) => {
                    let name = place.root.span.bytes();
                    assert!(matches!(
                        name,
                        b"item" | b"owner" | b"outer: string" | b"moved" | b"suffix"
                    ));
                    if name == b"item" {
                        assert!(same_place(place, item));
                    }
                    name
                }
                Obligation::EndStorage(place) => {
                    let name = place.root.span.bytes();
                    assert!(matches!(
                        name,
                        b"view"
                            | b"success"
                            | b"gate"
                            | b"gate: Option<bool>"
                            | b"gate: Option<bool>.Some"
                            | b"gate: Option<bool>.None"
                    ));
                    name
                }
                other => panic!("unexpected cleanup {other:?}"),
            })
            .collect()
    };
    let checked_export = |path: &[&Block<'g>], variant: &DeclarationId, origin: &[u8]| {
        assert_eq!(path.last().unwrap().terminator, Terminator::Return);
        assert!(path.iter().all(|b| !matches!(
            b.operation,
            Operation::OpenOwnedListText { .. }
                | Operation::AdvanceOwnedListText { .. }
                | Operation::ConstructResultError { .. }
                | Operation::Drop(_)
        )));
        let [(constructed, payload, output)] = path
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
            panic!("exactly one returned variant")
        };
        assert_eq!(constructed, variant);
        assert_eq!(payload.is_some(), variant == some);
        let [(index, exported)] = path
            .iter()
            .enumerate()
            .filter_map(|(index, b)| match b.operation {
                Operation::Export(value) => Some((index, value)),
                _ => None,
            })
            .collect::<Vec<_>>()[..]
        else {
            panic!("exactly one export")
        };
        assert!(same_place(exported, output));
        assert_eq!(path[index].site.span.bytes(), origin);
        index
    };
    let Terminator::Branch { yes, no } = head.terminator else {
        panic!("next Some/None")
    };
    let empty = straight_blocks(mir, no);
    let exported = checked_export(&empty, some, b"return Option.Some(false);");
    assert!(!empty.iter().any(|b| b.site.span.bytes() == b"gate?"));
    assert_eq!(clean(&empty[..exported]), [b"cursor".as_slice()]);
    let gate = match name {
        "knownSome" => b"gate: Option<bool>.Some".as_slice(),
        "knownNone" => b"gate: Option<bool>.None".as_slice(),
        _ => b"gate: Option<bool>".as_slice(),
    };
    assert_eq!(
        clean(&empty[exported..]),
        [b"outer: string".as_slice(), gate]
    );

    let prefix = straight_blocks(mir, yes);
    let [stage] = prefix
        .iter()
        .filter_map(|b| match b.operation {
            Operation::Copy {
                from,
                into: Some(into),
            } if from.root.span.bytes() == gate => Some(into),
            _ => None,
        })
        .collect::<Vec<_>>()[..]
    else {
        panic!("stage the named gate exactly once")
    };
    let consumes_current = name == "afterMove";
    let moves: Vec<_> = prefix
        .iter()
        .filter_map(|b| match b.operation {
            Operation::Move {
                from,
                into: Some(into),
            } if same_place(from, item) => Some(into),
            _ => None,
        })
        .collect();
    if consumes_current {
        let [moved] = moves[..] else {
            panic!("move current exactly once")
        };
        assert_eq!(moved.root.span.bytes(), b"moved");
    } else {
        assert!(moves.is_empty());
    }
    let (success, failure) = match prefix.last().unwrap().operation {
        Operation::TestVariant { operand, success } => {
            assert!(!matches!(name, "knownSome" | "knownNone"));
            assert!(same_place(operand, stage));
            assert_eq!(success, some);
            let Terminator::Branch { yes, no } = prefix.last().unwrap().terminator else {
                panic!("mixed gate must branch")
            };
            (
                Some(straight_blocks(mir, yes)),
                Some(straight_blocks(mir, no)),
            )
        }
        _ if name == "knownNone" => (None, Some(prefix)),
        _ if name == "knownSome" => (Some(prefix), None),
        _ => panic!("missing gate split"),
    };
    if let Some(failure) = failure {
        let exported = checked_export(&failure, none, b"gate?");
        assert!(failure.iter().all(|b| {
            !matches!(
                b.operation,
                Operation::CopyOptionPayload { .. }
                    | Operation::TakeOptionPayload { .. }
                    | Operation::Dispose { .. }
            ) && b.site.span.bytes() != b"\"continuation only\""
        }));
        let mut expected: Vec<&[u8]> = Vec::new();
        if name == "onPostfix" {
            expected.push(b"view");
        }
        expected.push(b"owner");
        expected.push(if consumes_current { b"moved" } else { b"item" });
        expected.extend([b"cursor".as_slice(), b"outer: string".as_slice(), gate]);
        assert!(clean(&failure[..exported]).is_empty());
        assert_eq!(clean(&failure[exported..]), expected);
    }
    if let Some(success) = success {
        if consumes_current {
            let backedge = success.last().unwrap();
            assert_eq!(backedge.site.node, head.site.node);
            assert!(
                matches!(backedge.operation, Operation::AdvanceOwnedListText { cursor: used, into, .. }
                if same_place(used, cursor) && same_place(into, item))
            );
            assert!(
                !success
                    .iter()
                    .any(|b| matches!(b.operation, Operation::Export(_)))
            );
            assert_eq!(clean(&success), [b"suffix".as_slice(), b"owner", b"moved"]);
        } else {
            assert!(success.iter().any(|b| matches!(b.operation,
                Operation::CopyOptionPayload { operand, into }
                if same_place(operand, stage) && into.root.span.bytes() == b"success")));
            let exported = checked_export(&success, some, b"return Option.Some(success);");
            assert_eq!(
                clean(&success[exported..])
                    .into_iter()
                    .rev()
                    .take(4)
                    .collect::<Vec<_>>(),
                [gate, b"outer: string", b"cursor", b"item"]
            );
        }
    }
}
