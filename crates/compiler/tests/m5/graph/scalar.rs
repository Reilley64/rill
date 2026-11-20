use super::*;
use compiler::analysis::{
    Statistics,
    ownership::lower_root,
    scalars::{NodeFactKind, data::DataExpression, generics::GoalState},
    typed::{Primitive, PrimitiveCoverage, PrimitiveRole},
};

pub(super) fn assert_standard_bodies(report: &GraphReport<'_>, typed: &TypedReport<'_>) {
    let body = |id| {
        report
            .roots()
            .iter()
            .find(|root| root.owner() == BodyOwner::Declaration(id))
            .unwrap()
    };
    let mut checked_lines = Vec::new();
    for provider in typed.primitive_providers() {
        let root = body(&provider.implementation.id);
        if provider.role == PrimitiveRole::Copy && provider.binding.is_none() {
            assert_eq!(root.site().module.path, "core.rill");
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            assert!(root.dependencies().is_empty());
            assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
            assert_copy_body(root.ordinary_mir().unwrap());
            checked_lines.push(root.site().span.coordinates().unwrap().0.line + 1);
        } else {
            assert_pending(root);
        }
    }
    checked_lines.sort_unstable();
    assert_eq!(checked_lines, (8..=21).collect::<Vec<_>>());
    for provider in typed
        .ownership_protection()
        .iter()
        .filter_map(|scope| scope.provider.as_ref())
    {
        assert_pending(body(&provider.implementation.id));
    }
}

fn assert_copy_body(mir: &Mir<'_>) {
    assert!(mir.cancellation().is_none());
    let path = straight_blocks(mir, mir.entry);
    let operations: Vec<_> = path
        .iter()
        .map(|block| &block.operation)
        .filter(|op| !matches!(op, Operation::None))
        .collect();
    let [
        Operation::CopyReferent { from, into },
        Operation::Export(exported),
        Operation::Cleanup(actions),
    ] = operations.as_slice()
    else {
        panic!("expected a read, export and descriptor cleanup: {operations:?}");
    };
    assert!(from.root.span.bytes().starts_with(b"value: &"), "{from:?}");
    assert_eq!(into.root.span.bytes(), b"*value");
    assert_eq!(into.root.node, exported.root.node);
    assert_ne!(from.root.node, into.root.node);
    assert!(
        matches!(actions.as_slice(), [Obligation::EndStorage(descriptor)]
        if descriptor.root.node == from.root.node && descriptor.projection.is_none())
    );
    assert_eq!(path.last().unwrap().terminator, Terminator::Return);
}

fn assert_pending(root: &RootReport<'_>) {
    assert_eq!(root.ordinary_completion(), Completion::Incomplete);
    assert!(root.blocked_at().is_some() && root.ordinary_mir().is_none());
}

const CALLERS: &str = r#"
import std::core::copy as duplicate;
function twice(value: &bool): bool {
    let first = duplicate(value);
    return copy(value);
}
function wide(value: &i128): i128 { return duplicate(value); }
function clear(): bool {
    let mut flag = false;
    let out = duplicate(&flag);
    flag = true;
    return out;
}
function conflict(): bool {
    let mut flag = false;
    let held = &mut flag;
    let out = duplicate(&flag);
    let later = *held;
    return out;
}
function text(value: &string): string { return duplicate(value); }
"#;

fn limits(statistics: Statistics) -> AnalysisLimits {
    AnalysisLimits {
        max_work: statistics.work,
        max_entries: statistics.entries,
        max_depth: statistics.max_depth,
    }
}

fn one_short(limits: AnalysisLimits) -> [AnalysisLimits; 3] {
    [
        AnalysisLimits {
            max_work: limits.max_work - 1,
            ..limits
        },
        AnalysisLimits {
            max_entries: limits.max_entries - 1,
            ..limits
        },
        AnalysisLimits {
            max_depth: limits.max_depth - 1,
            ..limits
        },
    ]
}

fn assert_target_budget(
    receipt: &TypedReceipt<'_, '_>,
    module: &loader::Module,
    root: &RootReport<'_>,
) {
    let run = |budget| lower_root(receipt, module, root.owner(), budget).unwrap();
    let measured = run(AnalysisLimits::default());
    let budget = limits(measured.statistics());
    let exact = run(budget);
    assert_eq!(exact.statistics(), measured.statistics());
    assert_eq!(exact.completion(), root.ordinary_completion());
    assert_eq!(exact.mir().is_some(), root.ordinary_mir().is_some());
    for short in one_short(budget) {
        let result = run(short);
        assert_eq!(result.completion(), Completion::ResourceLimit, "{short:?}");
        assert!(result.mir().is_none());
    }
}

#[test]
fn written_scalar_calls_close_over_the_checked_implementation() {
    fixture_with_main(
        "graph::scalar::written_scalar_calls_close_over_the_checked_implementation",
        &format!("{MAIN}{CALLERS}"),
        &[&format!(
            r#"{HELPER}
function copy(value: &bool): bool;
implement copy(value: &bool): bool {{ return *value; }}
function shadow(value: &bool): bool {{ return copy(value); }}
"#
        )],
        |_, _, graph, receipt, typed| {
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let twice = root(&report, b"function twice");
            let wide = root(&report, b"function wide");
            let clear = root(&report, b"function clear");
            let conflict = root(&report, b"function conflict");
            let text = root(&report, b"function text");

            // The canonical typed goal is the selection witness, not the alias
            // or a guess made from the shared argument type.
            let selection = |node| {
                let fact = typed.bodies().node(module, node).unwrap().unwrap();
                let NodeFactKind::Expression(facts) = &fact.kind else {
                    panic!("expected a typed source call");
                };
                let Some(DataExpression::CanonicalCall(goal)) = &facts.data else {
                    panic!("expected a canonical selection goal");
                };
                let proof = typed.bodies().goal(goal).unwrap();
                assert_eq!(proof.state, GoalState::Proved);
                let selection = proof.selection.as_ref().unwrap();
                assert!(selection.substitution.is_empty());
                selection.implementation
            };
            let call_nodes = module.syntax.nodes.iter().enumerate().filter(|(_, node)| {
                node.kind == compiler::parser::NodeKind::Call
                    && [b"duplicate(".as_slice(), b"copy("]
                        .iter()
                        .any(|prefix| node.span.bytes().starts_with(prefix))
            });
            let mut calls = 0;
            for (node, _) in call_nodes {
                selection(node);
                calls += 1;
            }
            assert_eq!(calls, 6);

            for (caller, count) in [(twice, 2), (wide, 1), (clear, 1)] {
                assert_eq!(
                    caller.ordinary_completion(),
                    Completion::Checked,
                    "{:?}",
                    caller
                        .blocked_at()
                        .map(|site| String::from_utf8_lossy(site.span.bytes()))
                );
                assert_eq!(caller.dependencies().len(), count);
                assert!(caller.ordinary_mir().unwrap().cancellation().is_none());
                assert!(matches!(caller.selected_call(), Delivery::Unresolved));
                for edge in caller.dependencies() {
                    assert_eq!(edge.contract, DependencyContract::OrdinaryCall);
                    assert_eq!(edge.declaration.kind, Implementation);
                    assert!(std::ptr::eq(edge.declaration, selection(edge.site.node)));
                }
                let BodyOwner::Declaration(id) = caller.owner() else {
                    panic!()
                };
                for result in [
                    lower_body(receipt, module, id, AnalysisLimits::default()).unwrap(),
                    lower_root(receipt, module, caller.owner(), AnalysisLimits::default()).unwrap(),
                ] {
                    assert_eq!(result.completion(), Completion::Checked);
                    assert!(result.mir().unwrap().cancellation().is_none());
                }
            }
            let boolean = twice.dependencies()[0].declaration;
            assert!(std::ptr::eq(boolean, twice.dependencies()[1].declaration));
            assert!(std::ptr::eq(boolean, clear.dependencies()[0].declaration));
            let provider = |primitive| {
                &typed
                    .primitive_providers()
                    .iter()
                    .find(|p| p.role == PrimitiveRole::Copy && p.primitive == primitive)
                    .unwrap()
                    .implementation
                    .id
            };
            assert!(std::ptr::eq(boolean, provider(Primitive::Bool)));
            assert!(std::ptr::eq(
                wide.dependencies()[0].declaration,
                provider(Primitive::I128)
            ));
            let bool_body = report
                .roots()
                .iter()
                .find(|r| r.owner() == BodyOwner::Declaration(boolean))
                .unwrap();
            let core = graph
                .modules()
                .iter()
                .find(|m| m.key == boolean.module)
                .unwrap();
            assert_target_budget(receipt, core, bool_body);
            assert_target_budget(receipt, module, clear);

            let mir = clear.ordinary_mir().unwrap();
            let path = straight_blocks(mir, mir.entry);
            let borrows: Vec<_> = path
                .iter()
                .enumerate()
                .filter_map(|(i, block)| match block.operation {
                    Operation::Borrow {
                        source,
                        holder,
                        exclusive: false,
                    } => Some((i, source, holder)),
                    _ => None,
                })
                .collect();
            let [(position, source, holder)] = borrows.as_slice() else {
                panic!("expected one borrow")
            };
            assert_eq!(source.root.span.bytes(), b"flag");
            assert!(matches!(path[position + 1].operation,
                Operation::CallScalarCopy { implementation, argument, into: Some(output) }
                if implementation == boolean && argument.root.node == holder.root.node
                    && output.root.node != source.root.node));
            assert!(
                path[position + 2..]
                    .iter()
                    .any(|block| matches!(block.operation,
                Operation::Install { target, .. } if target.root.node == source.root.node))
            );
            assert!(!path.iter().any(|block| matches!(block.operation,
                Operation::Move { from, .. } | Operation::Drop(from) if from.root.node == source.root.node)));

            let shadow_impl = root(&report, b"implement copy(");
            assert_eq!(shadow_impl.ordinary_completion(), Completion::Checked);
            assert!(
                !shadow_impl
                    .ordinary_mir()
                    .unwrap()
                    .blocks
                    .iter()
                    .any(|block| matches!(block.operation, Operation::CopyReferent { .. }))
            );
            let shadow = root(&report, b"function shadow(");
            assert!(shadow.dependencies().is_empty());
            assert_pending(shadow);
            assert_pending(text);
            assert_eq!(report.completion(), GraphCompletion::SourceRejected);
            assert_eq!(conflict.ordinary_completion(), Completion::SourceRejected);
            assert!(conflict.ordinary_mir().is_none());
            let [finding] = report.findings() else {
                panic!("{:?}", report.findings())
            };
            assert_eq!(finding.code(), "R0201");
            assert_eq!(finding.site.span.bytes(), b"&flag");
            assert_eq!(finding.related.span.bytes(), b"&mut flag");
            let BodyOwner::Declaration(id) = conflict.owner() else {
                panic!()
            };
            let rejected = lower_body(receipt, module, id, AnalysisLimits::default()).unwrap();
            assert_eq!(rejected.completion(), Completion::SourceRejected);
            assert!(rejected.mir().is_none());
            assert_eq!(rejected.findings()[0].code(), "R0201");
            assert_eq!(rejected.findings()[0].site.span, finding.site.span);
            assert_eq!(rejected.findings()[0].related.span, finding.related.span);
            assert_target_budget(receipt, module, conflict);

            let budget = limits(report.statistics());
            let exact = analyze_ownership(receipt, budget);
            assert_eq!(exact.statistics(), report.statistics());
            assert_eq!(exact.completion(), GraphCompletion::SourceRejected);
            assert_eq!(exact.findings()[0].code(), "R0201");
            assert!(root(&exact, b"function twice").ordinary_mir().is_some());
            for short in one_short(budget) {
                let result = analyze_ownership(receipt, short);
                assert_eq!(
                    result.completion(),
                    GraphCompletion::ResourceLimit,
                    "{short:?}"
                );
                assert!(
                    result
                        .roots()
                        .iter()
                        .all(|root| root.mir().is_none() && root.ordinary_mir().is_none())
                );
                if short.max_depth == budget.max_depth {
                    assert_eq!(result.findings().len(), 1);
                    assert_eq!(result.findings()[0].code(), "R0201");
                }
            }
        },
    );
}

#[test]
fn aliased_primitive_overlap_still_blocks_the_typed_receipt() {
    installed(
        "graph::scalar::aliased_primitive_overlap_still_blocks_the_typed_receipt",
        |request| {
            fs::write(
                &request.entry,
                format!(
                    r#"{MAIN}
import std::core::copy as clone;
implement clone(value: &i32): i32 {{ return *value; }}
function replacement(value: &i32): i32 {{ return clone(value); }}
"#
                ),
            )
            .unwrap();
            fs::write(request.entry.with_file_name("helper.rill"), HELPER).unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                assert!(typed.receipt().is_none());
                let scope = typed
                    .primitive_protection()
                    .iter()
                    .find(|scope| {
                        scope.role == PrimitiveRole::Copy && scope.primitive == Primitive::I32
                    })
                    .unwrap();
                assert_eq!(scope.coverage, PrimitiveCoverage::Overlap);
                let [overlap] = scope.overlaps.as_slice() else {
                    panic!("expected a protected overlap")
                };
                assert_eq!(overlap.code(), "R0500");
                assert_eq!(overlap.implementation.id.module.origin, Origin::Application);
            }
        },
    );
}
