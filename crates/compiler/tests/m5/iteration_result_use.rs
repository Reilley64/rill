use super::*;
use compiler::analysis::generics::{GoalJudgment, GoalState};
use compiler::analysis::scalars::{NodeFactKind, PropagationTarget, data::DataExpression};
use compiler::analysis::typed::{
    CollectionOperation, ListTraversal, OwnershipCoverage, ResultDiscardKind, ResultUseBlocker,
    TypedCompletion,
};
use compiler::analysis::{Stage, TypeTerm};
use compiler::parser::NodeKind;

#[test]
fn yielded_option_propagates_through_the_checked_iterator_handoff() {
    probe_with_typed(
        "iteration_result_use::yielded_option_propagates_through_the_checked_iterator_handoff",
        r#"
        function item(xs: List<Option<string>>, outer: string): Option<bool> {
            for (let item of xs) { let text = item?; let _ = &text; continue; }
            return Option.Some(true);
        }
        function itemMove(xs: List<Option<string>>, outer: string): Option<bool> {
            for (let item of xs) { let text = item?; let outgoing = text; let _ = &outgoing; break; }
            return Option.Some(true);
        }
        function ordinary(xs: List<Option<string>>, outer: string): Option<bool> {
            for (let item of xs) { break; }
            return Option.Some(true);
        }
        function observed(xs: List<Option<string>>, outer: string): Option<bool> {
            let owed = Err<bool,i32>(42i32);
            let _ = owed;
            for (let item of xs) { let text = item?; let _ = &text; continue; }
            return Option.Some(true);
        }
        "#,
        |_, app, receipt, typed| {
            assert_eq!(
                receipt.not_performed(),
                [Stage::Ownership, Stage::Execution]
            );
            let uses = typed.result_use().unwrap();
            assert!(uses.discards().is_empty() && uses.pending().is_empty());
            let option = typed.option_role().unwrap();
            let fact = |node| typed.bodies().node(app, node).unwrap().unwrap();
            let syntax = &app.syntax.nodes;
            for name in ["item", "itemMove", "ordinary", "observed"] {
                let function = app
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let function_body = syntax[function.node].children[1];
                let loop_node = child_of_kind(app, function_body, NodeKind::For);
                let [pattern, source, body] = syntax[loop_node].children[..] else {
                    panic!("expected for-of")
                };
                let NodeFactKind::Iteration(iteration) = &fact(loop_node).kind else {
                    panic!("missing iteration facts")
                };
                assert_eq!((iteration.pattern, iteration.source), (pattern, source));
                assert_eq!(iteration.traversal, Some(ListTraversal::Owned));
                assert_eq!(
                    (iteration.some, iteration.none),
                    (&option.some.id, &option.none.id)
                );

                let NodeFactKind::Expression(source_value) = &fact(source).kind else {
                    panic!("missing source expression")
                };
                let parameter = app
                    .declarations
                    .iter()
                    .find(|d| {
                        d.name.as_deref() == Some("xs")
                            && syntax[function.node].span.start() < d.id.start
                            && d.id.start < syntax[function_body].span.start()
                    })
                    .unwrap();
                let source_path = source_value.path.as_ref().unwrap();
                assert_eq!(source_path.root, &parameter.id);
                assert!(source_path.projections.is_empty());
                let NodeFactKind::Pattern(binding) = &fact(pattern).kind else {
                    panic!("missing item pattern")
                };
                let [item] = binding.binders.as_slice() else {
                    panic!("expected one item binder")
                };
                assert_eq!(binding.source.node, source);
                assert!(binding.coverage.exhaustive() && item.projections.is_empty());
                assert_eq!(item.ty, iteration.yielded);
                assert!(
                    matches!(item.ty.as_ref(), TypeTerm::Nominal { declaration, arguments }
                    if *declaration == &option.declaration.id && matches!(arguments.as_slice(), [payload]
                        if matches!(payload.as_ref(), TypeTerm::Primitive(p) if p.name() == "string")))
                );

                let iterate = typed.bodies().goal(&iteration.iterate).unwrap();
                let next = typed.bodies().goal(&iteration.next).unwrap();
                assert_ne!(iterate.id, next.id);
                for (goal, operation) in [
                    (iterate, CollectionOperation::Iterate(ListTraversal::Owned)),
                    (next, CollectionOperation::Next(ListTraversal::Owned)),
                ] {
                    let protection = typed
                        .collection_protection()
                        .find(|scope| scope.operation == operation)
                        .unwrap();
                    assert_eq!((goal.site.module, goal.site.node), (&app.key, loop_node));
                    assert_eq!(goal.state, GoalState::Proved);
                    assert!(goal.premise.is_none());
                    assert_eq!(protection.coverage, OwnershipCoverage::Clear);
                    let selection = goal.selection.as_ref().unwrap();
                    assert_eq!(
                        selection.implementation,
                        &protection.provider.unwrap().implementation.id
                    );
                    assert!(
                        matches!(selection.substitution.as_slice(), [element] if element.argument == item.ty)
                    );
                    assert!(
                        matches!(&goal.judgment, GoalJudgment::Determine { family, inputs, filters }
                        if *family == &protection.family.id && filters.is_empty()
                            && matches!(inputs.as_slice(), [input] if match operation {
                                CollectionOperation::Iterate(_) => input == &source_value.ty,
                                _ => matches!(input.as_ref(), TypeTerm::Reference { mutable: true, referent }
                                    if referent == &iteration.iterator),
                            }))
                    );
                }
                assert_eq!(iterate.output.as_ref().unwrap().result, iteration.iterator);
                assert!(
                    matches!(next.output.as_ref().unwrap().result.as_ref(), TypeTerm::Nominal { declaration, arguments }
                    if *declaration == &option.declaration.id && matches!(arguments.as_slice(), [payload] if payload == &item.ty))
                );

                if name != "ordinary" {
                    let postfix = *syntax[syntax[body].children[0]].children.last().unwrap();
                    assert_eq!(syntax[postfix].kind, NodeKind::Propagate);
                    let operand = syntax[postfix].children[0];
                    let NodeFactKind::Expression(value) = &fact(operand).kind else {
                        panic!("missing postfix operand")
                    };
                    let operand_path = value.path.as_ref().unwrap();
                    assert_eq!(operand_path.root, item.declaration);
                    assert!(operand_path.projections.is_empty());
                    assert_eq!(value.ty, item.ty);
                    let NodeFactKind::Expression(value) = &fact(postfix).kind else {
                        panic!("missing postfix expression")
                    };
                    let Some(DataExpression::Propagation(exit)) = &value.data else {
                        panic!("missing postfix return facts")
                    };
                    assert!(
                        matches!(exit.target, PropagationTarget::Callable(id) if id == &function.id)
                    );
                    assert!(exit.can_continue && exit.can_return && exit.error.is_none());
                    assert_eq!(
                        (exit.success, exit.failure),
                        (&option.some.id, &option.none.id)
                    );
                }
            }
        },
    );
}

#[test]
fn only_the_inner_none_leaves_before_acknowledging_the_result() {
    probe_report(
        "iteration_result_use::only_the_inner_none_leaves_before_acknowledging_the_result",
        r#"
        function owed(xs: List<Option<string>>, outer: string): Option<bool> {
            let owed = Err<bool,i32>(42i32);
            for (let item of xs) { let text = item?; let _ = &text; continue; }
            let _ = owed;
            return Option.Some(true);
        }
        "#,
        |_, app, typed| {
            assert_eq!(typed.completion(), TypedCompletion::SourceRejected);
            assert!(typed.receipt().is_none());
            let uses = typed.result_use().unwrap();
            assert!(uses.pending().is_empty());
            let [witness] = uses.discards() else {
                panic!("expected one discard on inner None: {uses:?}")
            };
            let function = app
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("owed") && d.scope.is_none())
                .unwrap();
            let syntax = &app.syntax.nodes;
            let body = syntax[function.node].children[1];
            let origin = *syntax[syntax[body].children[0]].children.last().unwrap();
            let loop_node = child_of_kind(app, body, NodeKind::For);
            let iteration_body = syntax[loop_node].children[2];
            let postfix = *syntax[syntax[iteration_body].children[0]]
                .children
                .last()
                .unwrap();
            assert_eq!(witness.code(), "R0103");
            assert_eq!(
                (witness.origin.module, witness.origin.node),
                (&app.key, origin)
            );
            assert_eq!(witness.origin.span.bytes(), b"Err<bool,i32>(42i32)");
            assert_eq!(
                (witness.discard.module, witness.discard.node),
                (&app.key, postfix)
            );
            assert_eq!(witness.discard.span.bytes(), b"item?");
            assert_eq!(witness.kind, ResultDiscardKind::ScopeExit);
            let binding = app
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("owed") && d.scope.is_some())
                .unwrap();
            assert_eq!(witness.storage, Some(&binding.id));
            assert!(witness.component.is_empty() && witness.storage_component.is_empty());
        },
    );
}

#[test]
fn current_item_success_can_change_a_residence_seen_after_continue_or_break() {
    probe_report(
        "iteration_result_use::current_item_success_can_change_a_residence_seen_after_continue_or_break",
        r#"
        function backedge(xs: List<Option<string>>): Option<bool> {
            let mut saved: Option<Result<bool,i32>> = Option.None;
            for (let item of xs) {
                let text = item?;
                let _ = &text;
                let _ = saved;
                saved = Option.Some(Err<bool,i32>(7i32));
                continue;
            }
            let _ = saved;
            return Option.Some(true);
        }
        function viaBreak(xs: List<Option<string>>): Option<bool> {
            let mut saved: Option<Result<bool,i32>> = Option.None;
            for (let item of xs) {
                let text = item?;
                let _ = &text;
                let _ = saved;
                saved = Option.Some(Err<bool,i32>(8i32));
                break;
            }
            return Option.Some(false);
        }
        "#,
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::SourceRejected);
            let uses = typed.result_use().unwrap();
            assert!(uses.pending().is_empty());
            let discards = uses.discards();
            assert_eq!(discards.len(), 2, "{discards:?}");
            for (origin, exit) in [
                (b"Err<bool,i32>(7i32)".as_slice(), b"item?".as_slice()),
                (
                    b"Err<bool,i32>(8i32)".as_slice(),
                    b"return Option.Some(false);".as_slice(),
                ),
            ] {
                assert!(
                    discards.iter().any(|witness| witness.code() == "R0103"
                        && witness.kind == ResultDiscardKind::ScopeExit
                        && witness.origin.span.bytes() == origin
                        && witness.discard.span.bytes() == exit),
                    "{discards:?}"
                );
            }
        },
    );
}

#[test]
fn grouped_fresh_item_keeps_the_inner_none_exit() {
    probe_report(
        "iteration_result_use::grouped_fresh_item_keeps_the_inner_none_exit",
        r#"
        import std::collection::first;
        function probe(xs: List<Option<string>>, other: List<Option<string>>): Option<bool> {
            {
                let xs = other;
                guard let head = first(&xs) else { return Option.Some(false); }
                let _ = head;
            }
            let debt = Err<bool,i32>(53i32);
            for (let ((item)) of ((xs))) { let t = item?; let _ = &t; }
            let _ = debt;
            return Option.Some(true);
        }
        "#,
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::SourceRejected);
            let uses = typed.result_use().unwrap();
            assert!(uses.pending().is_empty());
            let [discard] = uses.discards() else {
                panic!("{uses:?}")
            };
            assert_eq!(discard.origin.span.bytes(), b"Err<bool,i32>(53i32)");
            assert_eq!(discard.discard.span.bytes(), b"item?");
        },
    );
}

#[test]
fn constrained_or_revisited_sources_do_not_create_iterator_exits() {
    probe_report(
        "iteration_result_use::constrained_or_revisited_sources_do_not_create_iterator_exits",
        r#"
        import std::collection::first;
        function empty(xs: List<Option<string>>): Option<bool> {
            guard let head = first(&xs) else {
                let debt = Err<bool,i32>(49i32);
                for (let item of xs) { let t = item?; let _ = &t; }
                let _ = debt;
                return Option.Some(false);
            }
            let _ = head;
            return Option.Some(true);
        }
        function nonempty(xs: List<Option<string>>): Option<bool> {
            guard let head = first(&xs) else { return Option.Some(false); }
            let _ = head;
            let debt = Err<bool,i32>(50i32);
            for (let item of xs) { let _ = &item; let _ = debt; break; }
            return Option.Some(true);
        }
        function repeat(xs: List<Option<string>>): Option<bool> {
            for (let item of xs) { let _ = &item; }
            let debt = Err<bool,i32>(51i32);
            for (let item of xs) { let t = item?; let _ = &t; }
            let _ = debt;
            let independent = Err<bool,i32>(72i32);
            return Option.Some(false);
        }
        function captured(xs: List<Option<string>>): Option<bool> {
            let observe = () => first(&xs);
            let _ = observe;
            let debt = Err<bool,i32>(82i32);
            for (let item of ((xs))) { let t = item?; let _ = &t; }
            let _ = debt;
            return Option.Some(true);
        }
        function revisit(xs: List<Option<string>>): Option<bool> {
            let mut again = false;
            while (true) {
                let debt = Err<bool,i32>(61i32);
                for (let item of xs) { if (again) { let t = item?; let _ = &t; } }
                let _ = debt;
                if (again) { break; }
                again = true;
            }
            return Option.Some(true);
        }
        "#,
        |request, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            let uses = typed.result_use().unwrap();
            let [discard] = uses.discards() else {
                panic!("only the independent obligation should be reported: {uses:?}")
            };
            assert_eq!(discard.origin.span.bytes(), b"Err<bool,i32>(72i32)");
            assert_eq!(discard.discard.span.bytes(), b"return Option.Some(false);");
            let pending: Vec<_> = uses
                .pending()
                .iter()
                .map(|p| {
                    assert_eq!(p.blocker, ResultUseBlocker::Storage);
                    assert_eq!(p.site.module.origin, Origin::Application);
                    std::str::from_utf8(p.site.span.bytes()).unwrap()
                })
                .collect();
            assert_eq!(
                pending,
                [
                    "for (let item of xs) { let _ = &item; let _ = debt; break; }",
                    // The first traversal can start; only the second lacks a source.
                    "for (let item of xs) { let t = item?; let _ = &t; }",
                    "for (let item of ((xs))) { let t = item?; let _ = &t; }",
                    "for (let item of xs) { if (again) { let t = item?; let _ = &t; } }",
                ]
            );
            replay_exact(request);
        },
    );
}

#[test]
fn dead_reads_and_a_proven_empty_source_keep_feasible_result_exits() {
    probe_report(
        "iteration_result_use::dead_reads_and_a_proven_empty_source_keep_feasible_result_exits",
        r#"
        import std::collection::{first as headOf};
        function deadRead(xs: List<Option<string>>): Option<bool> {
            if (false) { let _ = &xs; let _ = &xs; }
            let debt = Err<bool,i32>(61i32);
            for (let item of xs) { let _ = &item; }
            return Option.Some(true);
        }
        function groupedDeadRead(xs: List<Option<string>>): Option<bool> {
            if (((false))) { let _ = &xs; }
            let debt = Err<bool,i32>(62i32);
            for (let ((item)) of ((xs))) { let _ = &item; }
            return Option.Some(true);
        }
        function readAfterReturn(xs: List<Option<string>>): Option<bool> {
            let debt = Err<bool,i32>(63i32);
            for (let item of xs) { let _ = &item; }
            return Option.Some(true);
            let _ = &xs;
        }
        function empty(xs: List<Option<string>>): Option<bool> {
            guard let head = headOf(&xs) else {
                let debt = Err<bool,i32>(81i32);
                for (let item of xs) { let _ = &item; let _ = debt; break; }
                return Option.Some(false);
            }
            let _ = head;
            return Option.Some(true);
        }
        "#,
        |request, _, typed| {
            let uses = typed.result_use().unwrap();
            assert_eq!(
                typed.completion(),
                TypedCompletion::SourceRejected,
                "{uses:?}"
            );
            assert!(typed.receipt().is_none());
            assert!(uses.pending().is_empty(), "{uses:?}");
            assert_eq!(uses.discards().len(), 4, "{uses:?}");
            for (origin, exit) in [
                ("Err<bool,i32>(61i32)", "return Option.Some(true);"),
                ("Err<bool,i32>(62i32)", "return Option.Some(true);"),
                ("Err<bool,i32>(63i32)", "return Option.Some(true);"),
                ("Err<bool,i32>(81i32)", "return Option.Some(false);"),
            ] {
                assert!(
                    uses.discards()
                        .iter()
                        .any(|witness| witness.code() == "R0103"
                            && witness.kind == ResultDiscardKind::ScopeExit
                            && witness.origin.span.bytes() == origin.as_bytes()
                            && witness.discard.span.bytes() == exit.as_bytes()),
                    "{uses:?}"
                );
            }
            replay_exact(request);
        },
    );
}

#[test]
fn a_live_read_withholds_only_its_iteration_alternative() {
    probe_report(
        "iteration_result_use::a_live_read_withholds_only_its_iteration_alternative",
        r#"
        function read(xs: List<Option<string>>): Option<bool> {
            let _ = &xs;
            let debt = Err<bool,i32>(61i32);
            for (let item of xs) { let _ = &item; }
            return Option.Some(true);
        }
        function conditionalRead(xs: List<Option<string>>, inspect: bool): Option<bool> {
            match (inspect) { true => { let _ = &xs; }, false => {} };
            let debt = Err<bool,i32>(62i32);
            for (let item of xs) {
                if (inspect) { let t = item?; let _ = &t; }
                else { let t = ((item))?; let _ = &t; }
            }
            let _ = debt;
            return Option.Some(true);
        }
        "#,
        |request, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            let uses = typed.result_use().unwrap();
            let [witness] = uses.discards() else {
                panic!("expected only the untouched alternative to prove a discard");
            };
            assert_eq!(witness.code(), "R0103");
            assert_eq!(witness.kind, ResultDiscardKind::ScopeExit);
            assert_eq!(witness.origin.span.bytes(), b"Err<bool,i32>(62i32)");
            assert_eq!(witness.discard.span.bytes(), b"((item))?");
            let [unconditional, conditional] = uses.pending() else {
                panic!("expected both live reads to block their pending traversal");
            };
            for pending in [unconditional, conditional] {
                assert_eq!(pending.blocker, ResultUseBlocker::Storage);
                assert_eq!(pending.site.module.origin, Origin::Application);
            }
            assert_eq!(
                unconditional.site.span.bytes(),
                b"for (let item of xs) { let _ = &item; }"
            );
            assert!(
                conditional
                    .site
                    .span
                    .bytes()
                    .starts_with(b"for (let item of xs) {\n")
            );
            replay_exact(request);
        },
    );
}

#[test]
fn an_excluded_yielded_tag_proves_the_complement_on_a_later_test() {
    probe_report(
        "iteration_result_use::an_excluded_yielded_tag_proves_the_complement_on_a_later_test",
        r#"
        function probe(xs: List<Option<string>>): Option<bool> {
            for (let item of xs) {
                let debt = Err<bool,i32>(71i32);
                match (&item) {
                    Option.Some(_) => { let _ = debt; },
                    _ => {
                        match (&item) {
                            Option.None => { let _ = debt; },
                            _ => {},
                        };
                    },
                };
                return Option.Some(false);
            }
            return Option.Some(true);
        }
        "#,
        |request, _, typed| {
            let uses = typed.result_use().unwrap();
            assert_eq!(typed.completion(), TypedCompletion::Checked, "{uses:?}");
            assert!(typed.receipt().is_some());
            assert!(uses.pending().is_empty() && uses.discards().is_empty());
            replay_exact(request);
        },
    );
}

#[test]
fn deep_source_groups_keep_the_owed_exit_at_exact_limits() {
    let source = format!(
        r#"
        function probe(xs: List<Option<string>>): Option<bool> {{
            let debt = Err<bool,i32>(42i32);
            for (let item of {}xs{}) {{ let t = item?; let _ = &t; continue; }}
            let _ = debt;
            return Option.Some(true);
        }}
    "#,
        "(".repeat(14),
        ")".repeat(14)
    );
    probe_report(
        "iteration_result_use::deep_source_groups_keep_the_owed_exit_at_exact_limits",
        &source,
        |request, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::SourceRejected);
            assert!(typed.receipt().is_none());
            assert!(typed.statistics().max_depth >= 17);
            let uses = typed.result_use().unwrap();
            assert!(uses.pending().is_empty());
            let [discard] = uses.discards() else {
                panic!("{uses:?}")
            };
            assert_eq!(discard.code(), "R0103");
            assert_eq!(discard.origin.span.bytes(), b"Err<bool,i32>(42i32)");
            assert_eq!(discard.discard.span.bytes(), b"item?");
            assert_eq!(discard.kind, ResultDiscardKind::ScopeExit);
            replay_exact(request);
        },
    );
}

#[test]
fn deep_binder_groups_keep_the_observed_receipt_at_exact_limits() {
    let source = format!(
        r#"
        function probe(xs: List<Option<string>>): Option<bool> {{
            let debt = Err<bool,i32>(42i32);
            let _ = debt;
            for (let {}item{} of xs) {{ let t = item?; let _ = &t; continue; }}
            return Option.Some(true);
        }}
    "#,
        "(".repeat(14),
        ")".repeat(14)
    );
    probe_report(
        "iteration_result_use::deep_binder_groups_keep_the_observed_receipt_at_exact_limits",
        &source,
        |request, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Checked);
            assert!(typed.receipt().is_some());
            assert!(typed.statistics().max_depth >= 17);
            let uses = typed.result_use().unwrap();
            assert!(uses.pending().is_empty() && uses.discards().is_empty());
            replay_exact(request);
        },
    );
}

fn replay_exact(request: &InstalledRequest) {
    let graph = loader::load_installed(request.clone()).unwrap();
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let stats = report.statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    let replay = analyze_typed(&graph, exact);
    assert_eq!(replay.statistics(), stats);
    assert_eq!(replay.completion(), report.completion());
    assert_eq!(replay.receipt().is_some(), report.receipt().is_some());
    let uses = report.result_use().unwrap();
    let actual = replay.result_use().unwrap();
    assert_eq!(actual.discards().len(), uses.discards().len());
    assert_eq!(actual.pending().len(), uses.pending().len());
    for (a, b) in actual.discards().iter().zip(uses.discards()) {
        assert_eq!(a.code(), b.code());
        assert_eq!(a.kind, b.kind);
        assert_eq!(a.storage, b.storage);
        assert_eq!(a.component, b.component);
        assert_eq!(a.storage_component, b.storage_component);
        assert_same_site(a.origin, b.origin);
        assert_same_site(a.discard, b.discard);
    }
    for (a, b) in actual.pending().iter().zip(uses.pending()) {
        assert_eq!(a.blocker, b.blocker);
        assert_same_site(a.site, b.site);
    }
    for short in [
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
    ] {
        let limited = analyze_typed(&graph, short);
        crate::limit::assert_typed_limit(&limited);
    }
}

fn assert_same_site(
    a: compiler::analysis::scalars::Site<'_>,
    b: compiler::analysis::scalars::Site<'_>,
) {
    assert_eq!((a.module, a.node), (b.module, b.node));
    assert_eq!(
        (a.span.start(), a.span.end()),
        (b.span.start(), b.span.end())
    );
    assert_eq!(a.span.bytes(), b.span.bytes());
}

#[test]
fn unsupported_option_binders_keep_their_published_caps() {
    // Fixed M4 limits from before the named Option-item handoff, not limits
    // derived from this run. Grouped wildcard patterns remain unsupported.
    for (fixture, pattern, work, entries, depth) in [
        ("bare_wild", "_", 323368, 48288, 12),
        ("group_wild", "(( _ ))", 323382, 48290, 12),
        (
            "deep_wild",
            "((((((((((((((_))))))))))))))",
            323466,
            48302,
            17,
        ),
    ] {
        let iteration = format!("for (let {pattern} of xs) {{ break; }}");
        let source = format!(
            "function probe(xs: List<Option<string>>, outer: string): Option<bool> {{ {iteration} return Option.Some(true); }}"
        );
        probe_report(
            "iteration_result_use::unsupported_option_binders_keep_their_published_caps",
            &source,
            |request, _, _| {
                assert_fixed_pending(request, fixture, &iteration, (work, entries, depth))
            },
        );
    }
}

#[test]
fn local_alias_sources_keep_their_published_caps() {
    for (fixture, place, work, entries) in [
        ("alias", "ys", 323682, 48405),
        ("group_alias", "((ys))", 323768, 48430),
        (
            "deep_alias",
            "((((((((((((((ys))))))))))))))",
            325678,
            48664,
        ),
    ] {
        let iteration =
            format!("for (let item of {place}) {{ let text = item?; let _ = &text; continue; }}");
        let source = format!(
            "function probe(xs: List<Option<string>>, outer: string): Option<bool> {{ let ys = xs; {iteration} return Option.Some(true); }}"
        );
        probe_report(
            "iteration_result_use::local_alias_sources_keep_their_published_caps",
            &source,
            |request, _, _| assert_fixed_pending(request, fixture, &iteration, (work, entries, 12)),
        );
    }
}

fn assert_fixed_pending(
    request: &InstalledRequest,
    fixture: &str,
    iteration: &str,
    (work, entries, depth): (usize, usize, usize),
) {
    let graph = loader::load_installed(request.clone()).unwrap();
    let report = analyze_typed(
        &graph,
        AnalysisLimits {
            max_work: work,
            max_entries: entries,
            max_depth: depth,
        },
    );
    assert_eq!(
        report.completion(),
        TypedCompletion::Incomplete,
        "{fixture}"
    );
    // The published limits must still produce this witness even when the
    // Result-free entry shape takes less work and storage than before.
    assert!(report.receipt().is_none());
    let uses = report.result_use().unwrap();
    assert!(uses.discards().is_empty());
    let [pending] = uses.pending() else {
        panic!("{uses:?}")
    };
    assert_eq!(pending.blocker, ResultUseBlocker::Storage);
    assert_eq!(pending.site.module.origin, Origin::Application);
    assert_eq!(pending.site.span.bytes(), iteration.as_bytes());
    replay_exact(request);
}

fn child_of_kind(app: &loader::Module, parent: usize, kind: NodeKind) -> usize {
    app.syntax.nodes[parent]
        .children
        .iter()
        .copied()
        .find(|child| app.syntax.nodes[*child].kind == kind)
        .unwrap()
}
