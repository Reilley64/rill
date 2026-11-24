use super::*;
use compiler::analysis::Stage;
use compiler::analysis::ownership::TextElementDisposal;
use compiler::analysis::typed::{Primitive, PrimitiveCoverage, PrimitiveOperation, PrimitiveRole};
use rill_collection::List;

#[path = "iteration_postfix.rs"]
mod postfix;

pub(super) const SOURCE: &str = r#"
import std::collection::{List as Words};
function onBreak(xs: List<string>, outer: string): void {
    for (let item of xs) {
        let view = &item;
        let _ = view;
        break;
    }
}
function onReturn(take: bool, xs: List<string>, outer: string): string {
    for (let item of xs) {
        if (take) { return item; }
        return "fallback";
    }
    return outer;
}
function stopAfterSecond(xs: Words<string>): void {
    let mut first = true;
    for (let (word) of ((xs))) {
        let view = &word;
        let _ = view;
        if (first) { first = false; continue; }
        break;
    }
}
function cycles(xs: List<string>, skip: bool): void {
    for (let item of xs) {
        if (skip) { let consumed = item; continue; }
        let saved = item;
    }
}
function reusedCurrent(xs: List<string>): void {
    for (let item of xs) { dispose(item); dispose(item); break; }
}
function reusedPrior(xs: List<string>, seed: string): void {
    let mut previous = seed;
    for (let item of xs) {
        dispose(previous);
        previous = item;
        dispose(previous);
    }
}
function reusedBacking(xs: List<string>): void {
    for (let item of xs) { break; }
    let again = xs;
}
function bypass(xs: List<string>, skip: bool): void {
    if (skip) { return; }
    for (let item of xs) { break; }
}
function literal(a: string, b: string, c: string): void {
    for (let item of [a, b, c]) { break; }
}
"#;

#[test]
fn unadmitted_text_parameters_keep_their_old_caps() {
    probe(
        "iteration::text::unadmitted_text_parameters_keep_their_old_caps",
        r#"
        function works(xs: List<string>): void { for (let item of xs) { break; } }
        function postfix(xs: List<string>, gate: Option<bool>): Option<bool> {
            for (let item of xs) { let bit=gate?; return Option.Some(bit); } return Option.None;
        }
        function unused(xs: List<string>): void {}
        function wildcard(xs: List<string>): void { for (let (_) of xs) { break; } }
        function sibling(xs: List<string>, other: List<string>): void { for (let item of xs) { break; } }
        function grouped(xs: List<string>): void { for (let (item) of ((xs))) { dispose(item); break; } }
    "#,
        |_, module, receipt| {
            let run = |name: &str, cap| {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &d.id, cap).unwrap()
            };
            // Unused/discarded/incomplete formal coverage must still block
            // at the first text List under its pre-cursor cap.
            for (name, work, entries) in [
                ("unused", 1963, 73),
                ("wildcard", 1963, 73),
                ("sibling", 1966, 74),
            ] {
                let stats = Statistics {
                    work,
                    entries,
                    max_depth: 0,
                };
                let report = run(name, limits(stats).0);
                assert_eq!(report.statistics(), stats);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(
                    report.blocked_at().unwrap().span.bytes(),
                    b"xs: List<string>"
                );
            }
            for name in ["works", "postfix", "grouped"] {
                assert!(
                    run(name, AnalysisLimits::default()).mir().is_some(),
                    "{name}"
                );
            }
            let grouped = run("grouped", AnalysisLimits::default());
            let exact = limits(grouped.statistics()).0;
            assert!(run("grouped", exact).mir().is_some());
            let short = run(
                "grouped",
                AnalysisLimits {
                    max_depth: exact.max_depth - 1,
                    ..exact
                },
            );
            crate::limit::assert_body_limit(&short);
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let cap = limits(graph.statistics()).0;
            let at = analyze_ownership(receipt, cap);
            let d = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("grouped"))
                .unwrap();
            assert!(
                at.roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == d.node)
                    .unwrap()
                    .ordinary_mir()
                    .is_some()
            );
            let short = analyze_ownership(
                receipt,
                AnalysisLimits {
                    max_depth: cap.max_depth - 1,
                    ..cap
                },
            );
            crate::limit::assert_graph_limit(&short);
        },
    );
}

#[test]
fn a_dead_loop_does_not_admit_a_text_parameter() {
    probe(
        "iteration::text::a_dead_loop_does_not_admit_a_text_parameter",
        r#"
        function dead(xs: List<string>): void { return; for (let item of xs) { break; } }
        function pruned(xs: List<string>): void { if (false) { for (let item of xs) { break; } } }
        function both(xs: List<string>, ys: List<string>): void { for (let x of xs) { break; } for (let y of ys) { break; } }
    "#,
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            for (name, expected) in [
                ("dead", Completion::Incomplete),
                ("pruned", Completion::Incomplete),
                ("both", Completion::Checked),
            ] {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let local = lower_body(receipt, module, &d.id, AnalysisLimits::default()).unwrap();
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == d.node)
                    .unwrap();
                assert_eq!(local.completion(), expected);
                assert_eq!(root.ordinary_completion(), expected);
                assert_eq!(local.mir().is_some(), expected == Completion::Checked);
                assert_eq!(root.mir().is_some(), expected == Completion::Checked);
                if expected == Completion::Incomplete {
                    assert_eq!(
                        local.blocked_at().unwrap().span.bytes(),
                        b"xs: List<string>"
                    );
                }
            }
        },
    );
}

#[test]
fn explicit_disposal_in_an_owning_text_loop_has_a_checked_element_edge() {
    probe_with_typed(
        "iteration::text::explicit_disposal_in_an_owning_text_loop_has_a_checked_element_edge",
        r#"
        import std::core::dispose as release;
        function explicit(xs: List<string>): void { for (let item of xs) { (release)((item)); break; } }
        function reuse(xs: List<string>): void { for (let item of xs) { break; } dispose(xs); }
        function early(xs: List<string>, skip: bool): void {
            if (skip) { release(xs); return; }
            for (let item of xs) { break; }
        }
    "#,
        |_, module, receipt, typed| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let role = typed
                .primitive_providers()
                .iter()
                .find(|p| p.role == PrimitiveRole::Dispose && p.primitive == Primitive::String)
                .unwrap();
            let bridge = &role.binding.unwrap().declaration.id;
            let early = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("early"))
                .unwrap();
            let early_body =
                lower_body(receipt, module, &early.id, AnalysisLimits::default()).unwrap();
            let early_root = report
                .roots()
                .iter()
                .find(|r| r.site().module == &module.key && r.site().node == early.node)
                .unwrap();
            for mir in [early_body.mir().unwrap(), early_root.mir().unwrap()] {
                let (yes, _) = branch_targets(mir);
                let path = straight_blocks(mir, yes);
                let drops = path_drops(mir, yes);
                assert!(
                    matches!(drops.as_slice(),[Obligation::DisposeListText{operand,element,..}] if operand.root.span.bytes()==b"xs: List<string>" && element.bridge==bridge)
                );
                assert!(
                    !path
                        .iter()
                        .any(|b| matches!(b.operation, Operation::OpenOwnedListText { .. }))
                );
            }
            for (name, expected) in [
                ("explicit", Completion::Checked),
                ("reuse", Completion::SourceRejected),
            ] {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let local = lower_body(receipt, module, &d.id, AnalysisLimits::default()).unwrap();
                let root = report
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == d.node)
                    .unwrap();
                assert_eq!(local.completion(), expected, "target {name}");
                assert_eq!(root.ordinary_completion(), expected, "graph {name}");
                if name == "explicit" {
                    assert!(root.dependencies().iter().any(|e| e.site.span.bytes()
                        == b"(release)((item))"
                        && std::ptr::eq(e.declaration, &role.implementation.id)
                        && e.contract == DependencyContract::ListTextElementDisposal { bridge }));
                    for mir in [local.mir().unwrap(), root.mir().unwrap()] {
                        let (yes, _) = branch_targets(mir);
                        let path = straight_blocks(mir, yes);
                        let text=path.iter().position(|b|matches!(b.operation,Operation::DisposeText {operand,element} if operand.root.span.bytes()==b"item" && std::ptr::eq(element.implementation,&role.implementation.id) && std::ptr::eq(element.bridge,bridge))).unwrap();
                        assert!(
                            obligations(&path[text + 1..]).any(|op| matches!(
                                op,
                                Obligation::DisposeOwnedListIterText { .. }
                            ))
                        );
                        assert!(!obligations(&path).any(|c| matches!(c, Obligation::Drop(_))));
                        assert_eq!(
                            obligations(&path)
                                .filter(|c| matches!(
                                    c,
                                    Obligation::DisposeOwnedListIterText { .. }
                                ))
                                .count(),
                            1
                        );
                    }
                } else {
                    assert!(local.mir().is_none() && root.mir().is_none());
                    let [f] = local.findings() else {
                        panic!("one reached source reuse")
                    };
                    assert_eq!(
                        (f.code(), f.site.span.bytes(), f.related.span.bytes()),
                        ("R0200", b"dispose(xs)".as_slice(), b"xs".as_slice())
                    );
                }
            }
        },
    );
}

#[test]
fn installed_consuming_string_iteration() {
    probe_with_typed(
        "iteration::text::installed_consuming_string_iteration",
        SOURCE,
        |request, module, receipt, typed| {
            let capture = loader::load_installed(request.clone()).unwrap();
            let (exact, short) = limits(receipt.statistics());
            let at = analyze_typed(&capture, exact);
            assert_eq!(at.receipt().unwrap().statistics(), receipt.statistics());
            assert_eq!(at.not_performed(), &[Stage::Ownership, Stage::Execution]);
            for cap in short {
                let limited = analyze_typed(&capture, cap);
                crate::limit::assert_typed_limit(&limited);
            }

            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            assert!(graph.inventory_complete());
            assert_eq!(graph.test_mode(), request.test_mode);
            assert_eq!(graph.context(), receipt.context());
            let (exact, short) = limits(graph.statistics());
            let at = analyze_ownership(receipt, exact);
            assert_eq!(at.statistics(), graph.statistics());
            assert_eq!(at.completion(), GraphCompletion::SourceRejected);
            for cap in short {
                let limited = analyze_ownership(receipt, cap);
                crate::limit::assert_graph_limit(&limited);
            }

            for (name, expected) in [
                ("onBreak", Completion::Checked),
                ("onReturn", Completion::Checked),
                ("stopAfterSecond", Completion::Checked),
                ("cycles", Completion::Checked),
                ("bypass", Completion::Checked),
                ("reusedCurrent", Completion::SourceRejected),
                ("reusedPrior", Completion::SourceRejected),
                ("reusedBacking", Completion::SourceRejected),
                ("literal", Completion::Incomplete),
            ] {
                let function = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let run = |caps| lower_body(receipt, module, &function.id, caps).unwrap();
                let root = at
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == function.node)
                    .unwrap();
                let measured = run(AnalysisLimits::default());
                let (exact, short) = limits(measured.statistics());
                let body = run(exact);
                assert_eq!(body.statistics(), measured.statistics());
                assert_eq!(
                    body.completion(),
                    expected,
                    "{name}: blocked at {:?}",
                    body.blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                );
                assert_eq!(root.ordinary_completion(), expected, "{name}");
                assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
                assert!(matches!(
                    root.replacement_cancellation(),
                    Delivery::Unresolved
                ));
                for cap in short {
                    let limited = run(cap);
                    crate::limit::assert_body_limit(&limited);
                }
                if expected == Completion::Checked {
                    for mir in [body.mir().unwrap(), root.ordinary_mir().unwrap()] {
                        check_paths(name, mir, module, typed, root.dependencies());
                    }
                } else {
                    assert!(body.mir().is_none() && root.ordinary_mir().is_none());
                    match name {
                        "reusedCurrent" | "reusedPrior" | "reusedBacking" => {
                            let [finding] = body.findings() else {
                                panic!("expected one reached move: {name}")
                            };
                            assert_eq!(finding.code(), "R0200");
                            let (use_site, move_site, crosses_backedge): (&[u8], &[u8], _) =
                                match name {
                                    "reusedCurrent" => (b"dispose(item)", b"dispose(item)", false),
                                    "reusedPrior" => {
                                        (b"dispose(previous)", b"dispose(previous)", true)
                                    }
                                    _ => (b"xs", b"xs", false),
                                };
                            assert_eq!(finding.site.span.bytes(), use_site);
                            assert_eq!(finding.related.span.bytes(), move_site);
                            assert_eq!(
                                finding.site.span.start() < finding.related.span.start(),
                                crosses_backedge
                            );
                            assert!(at.findings().iter().any(|f| f.code() == "R0200"
                                && f.site.node == finding.site.node
                                && f.related.node == finding.related.node));
                        }
                        _ => assert!(body.findings().is_empty() && body.blocked_at().is_some()),
                    }
                }
            }
        },
    );
}

fn check_paths<'g>(
    name: &str,
    mir: &Mir<'g>,
    module: &'g loader::Module,
    typed: &TypedReport<'g>,
    dependencies: &[Dependency<'g>],
) {
    use CollectionOperation::{DisposeIterator, DisposeList, Iterate, Next};
    use ListTraversal::Owned;
    assert!(mir.cancellation().is_none() && mir.replacement_cancellation().is_none());
    let provider = |operation| {
        let scope = typed
            .collection_protection()
            .find(|s| s.operation == operation)
            .unwrap();
        assert_eq!(scope.coverage, OwnershipCoverage::Clear);
        (scope, scope.provider.unwrap())
    };
    let (release_scope, release) = provider(DisposeIterator(Owned));
    let (dispose_list_scope, dispose_list) = provider(DisposeList);
    let protected = typed
        .primitive_protection()
        .iter()
        .find(|p| p.primitive == Primitive::String && p.role == PrimitiveRole::Dispose)
        .unwrap();
    assert_eq!(protected.coverage, PrimitiveCoverage::Clear);
    assert_eq!(protected.family, &release_scope.family.id);
    assert_eq!(protected.family, &dispose_list_scope.family.id);
    let winner = protected.required_winner.unwrap();
    let binding = typed
        .primitive_providers()
        .iter()
        .find(|p| &p.implementation.id == winner)
        .unwrap()
        .binding
        .unwrap();
    assert_eq!(binding.operation, PrimitiveOperation::DisposeText);
    let element = TextElementDisposal {
        implementation: winner,
        bridge: &binding.declaration.id,
    };
    let check_dependency = |operation, p: CollectionProvider<'_>| {
        assert!(
            dependencies
                .iter()
                .any(|d| d.declaration == &p.implementation.id
                    && d.contract
                        == DependencyContract::ListTextCollection {
                            operation,
                            bridge: &p.bridge.id
                        })
        );
    };
    assert!(dependencies.iter().any(|d| d.declaration == winner
        && d.contract
            == DependencyContract::ListTextElementDisposal {
                bridge: element.bridge
            }));
    check_dependency(DisposeIterator(Owned), release);

    let setup = straight_blocks(mir, mir.entry);
    let setup = if name == "bypass" {
        let Terminator::Branch { yes, no } = setup.last().unwrap().terminator else {
            panic!("early return split")
        };
        let before_open = straight_blocks(mir, yes);
        assert_eq!(before_open.last().unwrap().terminator, Terminator::Return);
        let [
            Obligation::EndStorage(skip),
            Obligation::DisposeListText {
                operand,
                implementation,
                bridge,
                element: callback,
            },
        ] = obligations(&before_open).collect::<Vec<_>>()[..]
        else {
            panic!("bypassing the loop must drop the whole list once")
        };
        assert_eq!(operand.root.span.bytes(), b"xs: List<string>");
        assert_eq!(skip.root.span.bytes(), b"skip: bool");
        assert_eq!(
            (implementation, bridge, callback),
            (
                &dispose_list.implementation.id,
                &dispose_list.bridge.id,
                element
            )
        );
        check_dependency(DisposeList, dispose_list);
        straight_blocks(mir, no)
    } else {
        setup
    };
    let [open, head] = setup[setup.len() - 2..] else {
        panic!("open then next")
    };
    let Operation::OpenOwnedListText {
        source,
        cursor,
        source_backing,
        cursor_backing,
        provider: open_link,
    } = open.operation
    else {
        panic!("consume the source once")
    };
    let Operation::AdvanceOwnedListText {
        cursor: advanced,
        into: item,
        some,
        none,
        provider: advance_link,
    } = head.operation
    else {
        panic!("bind only a successful yield")
    };
    assert!(same_place(cursor, advanced));
    assert!(!same_place(cursor, item) && !same_place(source, item));
    assert_eq!(open.site.span.bytes(), b"xs");
    assert_eq!(
        (source_backing.storage, cursor_backing.storage),
        (
            CollectionStorage::Elements,
            CollectionStorage::UnyieldedElements
        )
    );
    assert_eq!(
        source_backing.declaration.id,
        typed.list_role().unwrap().declaration.id
    );
    assert_eq!(release_scope.owner.id, cursor_backing.declaration.id);
    assert_eq!(open_link.artifact, advance_link.artifact);
    assert_eq!(open_link.artifact, release.artifact);
    assert_eq!(open_link.artifact, dispose_list.artifact);
    for backing in [source_backing, cursor_backing] {
        assert_eq!(
            backing.artifact.executable(),
            open_link.artifact.executable()
        );
        assert_eq!(backing.artifact.context(), open_link.artifact.context());
    }
    let NodeFactKind::Iteration(facts) = &typed
        .bodies()
        .node(module, head.site.node)
        .unwrap()
        .unwrap()
        .kind
    else {
        panic!("typed iteration")
    };
    assert_eq!(facts.traversal, Some(Owned));
    let option = typed.option_role().unwrap();
    assert_eq!((some, none), (&option.some.id, &option.none.id));
    assert_eq!(
        (facts.some, facts.none, facts.pattern),
        (some, none, item.root.node)
    );
    assert!(matches!(facts.yielded.as_ref(), TypeTerm::Primitive(p) if p.name() == "string"));
    assert!(
        matches!(facts.iterator.as_ref(), TypeTerm::Nominal { declaration, arguments }
        if **declaration == cursor_backing.declaration.id && arguments.as_slice() == [facts.yielded.clone()])
    );
    for (operation, goal, link) in [
        (Iterate(Owned), &facts.iterate, open_link),
        (Next(Owned), &facts.next, advance_link),
    ] {
        let (scope, pin) = provider(operation);
        let goal = typed.bodies().goal(goal).unwrap();
        let selected = goal.selection.as_ref().unwrap();
        assert!(
            matches!(&goal.judgment, GoalJudgment::Determine { family, .. } if **family == scope.family.id)
        );
        assert_eq!(goal.state, GoalState::Proved);
        assert!(goal.premise.is_none());
        assert_eq!(selected.implementation, &link.implementation.id);
        assert_eq!(selected.substitution.len(), 1);
        assert_eq!(selected.substitution[0].position, 0);
        assert_eq!(selected.substitution[0].argument, facts.yielded);
        assert_eq!(
            (link.artifact, &link.implementation.id, &link.bridge.id),
            (pin.artifact, &pin.implementation.id, &pin.bridge.id)
        );
        check_dependency(operation, link);
    }
    assert_eq!(
        mir.blocks
            .iter()
            .filter(|b| matches!(b.operation, Operation::OpenOwnedListText { .. }))
            .count(),
        1
    );
    if matches!(name, "onPostfix" | "knownSome" | "knownNone" | "afterMove") {
        postfix::check_edges(
            name,
            mir,
            head,
            (item, cursor),
            (some, none),
            (release, element),
        );
        return;
    }
    let clean = |path: &[&Block<'g>]| -> Vec<_> {
        assert!(
            !path
                .iter()
                .any(|b| matches!(b.operation, Operation::Drop(_)))
        );
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
                    "suffix"
                }
                Obligation::Drop(place) => match place.root.span.bytes() {
                    b"outer: string" => "drop outer",
                    b"item" | b"(word)" => {
                        assert!(same_place(place, item));
                        "drop item"
                    }
                    b"consumed" => "drop consumed",
                    b"saved" => "drop saved",
                    _ => panic!("unexpected dropped owner {place:?}"),
                },
                Obligation::EndStorage(place) => match place.root.span.bytes() {
                    b"view" => "end view",
                    b"first" => "end first",
                    b"take: bool" | b"skip: bool" => "end control",
                    _ => panic!("unexpected end {place:?}"),
                },
                _ => panic!("unexpected cleanup {action:?}"),
            })
            .collect()
    };
    let exit = |path: &[&Block<'g>], expected: &[&str]| {
        assert_eq!(path.last().unwrap().terminator, Terminator::Return);
        assert_eq!(clean(path), expected);
    };
    let next = |path: &[&Block<'g>], expected: &[&str]| {
        let next = path.last().unwrap();
        assert_eq!(next.site.node, head.site.node);
        assert!(
            matches!(next.operation, Operation::AdvanceOwnedListText { cursor: same, into, .. } if same_place(same, cursor) && same_place(into, item))
        );
        assert_eq!(clean(path), expected);
        let Terminator::Branch { yes, .. } = next.terminator else {
            panic!("next visit")
        };
        yes
    };
    let Terminator::Branch { yes, no } = head.terminator else {
        panic!("Some / None")
    };
    exit(
        &straight_blocks(mir, no),
        match name {
            "onBreak" => &["suffix", "drop outer"],
            "stopAfterSecond" => &["suffix", "end first"],
            "onReturn" | "cycles" | "bypass" => &["suffix", "end control"],
            _ => unreachable!(),
        },
    );
    let some_path = straight_blocks(mir, yes);
    match name {
        "onBreak" => exit(
            &some_path,
            &["end view", "drop item", "suffix", "drop outer"],
        ),
        "bypass" => exit(&some_path, &["drop item", "suffix", "end control"]),
        "onReturn" => {
            let Terminator::Branch { yes, no } = some_path.last().unwrap().terminator else {
                panic!("return choices")
            };
            for (target, take_item) in [(yes, true), (no, false)] {
                let path = straight_blocks(mir, target);
                let exported = path
                    .iter()
                    .position(|b| matches!(b.operation, Operation::Export(_)))
                    .unwrap();
                assert!(clean(&path[..exported]).is_empty());
                assert_eq!(path.iter().any(|b| matches!(b.operation, Operation::Move { from, .. } if same_place(from, item))), take_item);
                exit(
                    &path,
                    if take_item {
                        &["suffix", "drop outer", "end control"]
                    } else {
                        &["drop item", "suffix", "drop outer", "end control"]
                    },
                );
            }
        }
        "stopAfterSecond" => {
            let Terminator::Branch { yes, .. } = some_path.last().unwrap().terminator else {
                panic!("first visit")
            };
            let second = next(&straight_blocks(mir, yes), &["end view", "drop item"]);
            let second_path = straight_blocks(mir, second);
            let Terminator::Branch { no, .. } = second_path.last().unwrap().terminator else {
                panic!("second visit")
            };
            exit(
                &straight_blocks(mir, no),
                &["end view", "drop item", "suffix", "end first"],
            );
        }
        "cycles" => {
            let Terminator::Branch { yes, no } = some_path.last().unwrap().terminator else {
                panic!("continue / fallthrough")
            };
            let continuing = straight_blocks(mir, yes);
            assert!(continuing.iter().any(|b| matches!(b.operation, Operation::Move { from, into: Some(consumed) } if same_place(from, item) && consumed.root.span.bytes() == b"consumed")));
            next(&continuing, &["drop consumed"]);
            let falling = straight_blocks(mir, no);
            assert!(falling.iter().any(|b| matches!(b.operation, Operation::Move { from, into: Some(saved) } if same_place(from, item) && saved.root.span.bytes() == b"saved")));
            next(&falling, &["drop saved"]);
        }
        _ => unreachable!(),
    }
}

#[test]
fn linked_string_suffix_disposes_only_remaining_values_in_reverse() {
    for (visits, expected) in [
        (1, &["third", "second"][..]),
        (2, &["third"][..]),
        (3, &[][..]),
    ] {
        let mut cursor =
            List::from_elements(["first", "second", "third"].map(str::to_owned)).into_owned_iter();
        for expected_yield in ["first", "second", "third"][..visits].iter() {
            let item = cursor.next().unwrap();
            assert_eq!(&item, expected_yield);
            PrimitiveOperation::DisposeText.dispose_text(item).unwrap();
        }
        if visits == 3 {
            assert!(cursor.next().is_none());
        }
        let mut released = Vec::new();
        cursor.dispose_with(|item| {
            released.push(item.clone());
            PrimitiveOperation::DisposeText.dispose_text(item).unwrap();
        });
        assert_eq!(released, expected);
    }
}
