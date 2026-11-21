use super::*;
use compiler::analysis::generics::{GoalJudgment, GoalState};
use compiler::analysis::ownership::{GraphCompletion, analyze_ownership};
use compiler::analysis::scalars::{NodeFactKind, PropagationTarget, data::DataExpression};
use compiler::analysis::typed::{
    CollectionOperation, ListTraversal, OwnershipCoverage, ResultDiscardKind, ResultUseBlocker,
    TypedCompletion,
};
use compiler::analysis::{Stage, TypeTerm};
use compiler::parser::NodeKind;

#[test]
fn yielded_option_propagates_but_has_no_ownership_mir() {
    probe_with_typed(
        "iteration_result_use::yielded_option_propagates_but_has_no_ownership_mir",
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
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert!(graph.findings().is_empty());
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

                let target =
                    lower_body(receipt, app, &function.id, AnalysisLimits::default()).unwrap();
                assert_eq!(target.completion(), Completion::Incomplete);
                assert!(target.mir().is_none() && target.findings().is_empty());
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &app.key && r.site().node == function.node)
                    .unwrap();
                assert_eq!(root.completion(), Completion::Incomplete);
                assert_eq!(root.ordinary_completion(), Completion::Incomplete);
                assert!(root.mir().is_none() && root.ordinary_mir().is_none());
                for site in [target.blocked_at().unwrap(), root.blocked_at().unwrap()] {
                    assert_eq!((site.module, site.node), (&app.key, parameter.node));
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
        |_, _, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            let uses = typed.result_use().unwrap();
            let [discard] = uses.discards() else {
                panic!("only the independent obligation should be reported: {uses:?}")
            };
            assert_eq!(discard.origin.span.bytes(), b"Err<bool,i32>(72i32)");
            assert_eq!(discard.discard.span.bytes(), b"return Option.Some(false);");
            assert_eq!(uses.pending().len(), 5, "{uses:?}");
            assert!(
                uses.pending()
                    .iter()
                    .all(|p| p.blocker == ResultUseBlocker::Storage)
            );
        },
    );
}

fn child_of_kind(app: &loader::Module, parent: usize, kind: NodeKind) -> usize {
    app.syntax.nodes[parent]
        .children
        .iter()
        .copied()
        .find(|child| app.syntax.nodes[*child].kind == kind)
        .unwrap()
}
