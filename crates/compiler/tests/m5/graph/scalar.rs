use super::*;
use compiler::analysis::{
    Statistics,
    ownership::{Place, lower_root},
    scalars::{NodeFactKind, data::DataExpression, generics::GoalState},
    typed::{Primitive, PrimitiveCoverage, PrimitiveOperation, PrimitiveRole},
};

pub(super) fn assert_standard_bodies(
    report: &GraphReport<'_>,
    typed: &TypedReport<'_>,
    receipt: &TypedReceipt<'_, '_>,
    core: &loader::Module,
) {
    let body = |id| {
        report
            .roots()
            .iter()
            .find(|root| root.owner() == BodyOwner::Declaration(id))
            .unwrap()
    };
    let mut checked_lines = Vec::new();
    let mut dispose_lines = Vec::new();
    for provider in typed.primitive_providers() {
        let root = body(&provider.implementation.id);
        if provider.role == PrimitiveRole::Copy && provider.binding.is_none() {
            assert_eq!(root.site().module.path, "core.rill");
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            assert!(root.dependencies().is_empty());
            assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
            assert_copy_body(root.ordinary_mir().unwrap());
            checked_lines.push(root.site().span.coordinates().unwrap().0.line + 1);
        } else if provider.role == PrimitiveRole::Dispose && provider.binding.is_none() {
            assert_eq!(root.site().module.path, "core.rill");
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            assert!(root.dependencies().is_empty());
            assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
            assert_dispose_body(root.ordinary_mir().unwrap());
            let targeted =
                lower_root(receipt, core, root.owner(), AnalysisLimits::default()).unwrap();
            assert_eq!(targeted.completion(), Completion::Checked);
            assert_dispose_body(targeted.mir().unwrap());
            dispose_lines.push(root.site().span.coordinates().unwrap().0.line + 1);
        } else {
            assert_pending(root);
            if provider.role == PrimitiveRole::Dispose {
                assert_eq!(provider.primitive, Primitive::String);
                assert_eq!(
                    provider.binding.unwrap().operation,
                    PrimitiveOperation::DisposeText
                );
                let targeted =
                    lower_root(receipt, core, root.owner(), AnalysisLimits::default()).unwrap();
                assert_eq!(targeted.completion(), Completion::Incomplete);
                assert!(targeted.mir().is_none());
            }
        }
    }
    checked_lines.sort_unstable();
    assert_eq!(checked_lines, (8..=21).collect::<Vec<_>>());
    dispose_lines.sort_unstable();
    assert_eq!(dispose_lines, (25..=38).collect::<Vec<_>>());
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

fn assert_dispose_body(mir: &Mir<'_>) {
    assert!(mir.cancellation().is_none());
    let path = straight_blocks(mir, mir.entry);
    let operations: Vec<_> = path
        .iter()
        .map(|block| &block.operation)
        .filter(|op| !matches!(op, Operation::None))
        .collect();
    let [Operation::Cleanup(actions)] = operations.as_slice() else {
        panic!("expected just the scalar parameter cleanup: {operations:?}");
    };
    assert!(
        matches!(actions.as_slice(), [Obligation::EndStorage(operand)]
        if operand.root.span.bytes().starts_with(b"value: ") && operand.projection.is_none())
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

fn function_id<'g>(module: &'g loader::Module, name: &str) -> &'g loader::DeclarationId {
    &module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some(name) && d.id.kind == loader::DeclarationKind::Function)
        .unwrap()
        .id
}

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
fn written_scalar_dispose_consumes_the_selected_named_operand() {
    fixture_with_main(
        "graph::scalar::written_scalar_dispose_consumes_the_selected_named_operand",
        &format!(
            r#"{MAIN}
import std::core::dispose as release;
function widths(flag: bool, wide: i128, other: bool): void {{
    release(flag); dispose(wide); dispose<bool>((other));
}}
function repair(value: bool): bool {{ let mut flag = value; release(flag); flag = true; return flag; }}
function savedCopy(value: bool): bool {{ let flag = value; let saved = flag; release(flag); return saved; }}
function reuse(flag: bool): bool {{ release(flag); return flag; }}
function live(value: bool): bool {{ let flag = value; let held = &flag; release(flag); return *held; }}
function text(value: string): void {{ release(value); }}
"#
        ),
        &[HELPER],
        |_, _, graph, receipt, typed| {
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let [widths, repair, saved] = [
                b"function widths".as_slice(),
                b"function repair",
                b"function savedCopy",
            ]
            .map(|prefix| root(&report, prefix));
            let provider = |primitive| {
                &typed
                    .primitive_providers()
                    .iter()
                    .find(|p| p.role == PrimitiveRole::Dispose && p.primitive == primitive)
                    .unwrap()
                    .implementation
                    .id
            };
            let boolean = provider(Primitive::Bool);
            let wide = provider(Primitive::I128);
            for (caller, expected) in [
                (widths, [boolean, wide, boolean].as_slice()),
                (repair, &[boolean]),
                (saved, &[boolean]),
            ] {
                assert_eq!(
                    caller.ordinary_completion(),
                    Completion::Checked,
                    "{:?}",
                    caller
                        .blocked_at()
                        .map(|site| String::from_utf8_lossy(site.span.bytes()))
                );
                assert_eq!(caller.dependencies().len(), expected.len());
                for (edge, id) in caller.dependencies().iter().zip(expected) {
                    let fact = typed
                        .bodies()
                        .node(module, edge.site.node)
                        .unwrap()
                        .unwrap();
                    let NodeFactKind::Expression(facts) = &fact.kind else {
                        panic!("expected a typed call")
                    };
                    let Some(DataExpression::CanonicalCall(goal)) = &facts.data else {
                        panic!("expected a canonical goal")
                    };
                    let proof = typed.bodies().goal(goal).unwrap();
                    assert_eq!(proof.state, GoalState::Proved);
                    assert!(std::ptr::eq(
                        edge.declaration,
                        proof.selection.as_ref().unwrap().implementation
                    ));
                    assert!(std::ptr::eq(edge.declaration, *id));
                    assert_eq!(edge.contract, DependencyContract::OrdinaryCall);
                }
                let BodyOwner::Declaration(id) = caller.owner() else {
                    panic!()
                };
                let targeted = lower_body(receipt, module, id, AnalysisLimits::default()).unwrap();
                assert_eq!(targeted.completion(), Completion::Checked);
                let rooted =
                    lower_root(receipt, module, caller.owner(), AnalysisLimits::default()).unwrap();
                assert_eq!(rooted.completion(), Completion::Checked);
                for mir in [
                    caller.ordinary_mir().unwrap(),
                    targeted.mir().unwrap(),
                    rooted.mir().unwrap(),
                ] {
                    assert_scalar_handoff(mir, caller);
                }
            }
            let text = root(&report, b"function text");
            assert_pending(text);
            assert_eq!(text.blocked_at().unwrap().span.bytes(), b"release(value)");
            assert_eq!(
                text.dependencies()[0].contract,
                DependencyContract::BuiltinDispose
            );
            assert_eq!(report.findings().len(), 2);
            for (name, code, site, related) in [
                (
                    "reuse",
                    "R0200",
                    b"flag".as_slice(),
                    b"release(flag)".as_slice(),
                ),
                ("live", "R0203", b"release(flag)", b"&flag"),
            ] {
                let id = function_id(module, name);
                let root = report
                    .roots()
                    .iter()
                    .find(|root| root.owner() == BodyOwner::Declaration(id))
                    .unwrap();
                let targeted =
                    lower_root(receipt, module, root.owner(), AnalysisLimits::default()).unwrap();
                assert_eq!(root.ordinary_completion(), Completion::SourceRejected);
                assert_eq!(targeted.completion(), Completion::SourceRejected);
                assert!(targeted.mir().is_none() && root.ordinary_mir().is_none());
                let finding = report.findings().iter().find(|f| f.code() == code).unwrap();
                assert_eq!(finding.site.span.bytes(), site);
                assert_eq!(finding.related.span.bytes(), related);
                let [targeted] = targeted.findings() else {
                    panic!("expected {code}")
                };
                assert_eq!(targeted.code(), code);
                assert_eq!(targeted.site.span, finding.site.span);
                assert_eq!(targeted.related.span, finding.related.span);
            }
            let core = graph
                .modules()
                .iter()
                .find(|m| m.key == boolean.module)
                .unwrap();
            let body = report
                .roots()
                .iter()
                .find(|root| root.owner() == BodyOwner::Declaration(boolean))
                .unwrap();
            assert_target_budget(receipt, core, body);
            assert_target_budget(receipt, module, widths);
            let budget = limits(report.statistics());
            let exact = analyze_ownership(receipt, budget);
            assert_eq!(exact.statistics(), report.statistics());
            assert_eq!(exact.completion(), GraphCompletion::SourceRejected);
            assert!(root(&exact, b"function widths").ordinary_mir().is_some());
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
                        .all(|root| root.ordinary_mir().is_none() && root.mir().is_none())
                );
                if short.max_depth == budget.max_depth {
                    assert_eq!(result.findings().len(), 2);
                }
            }
        },
    );
}

fn assert_scalar_handoff(mir: &Mir<'_>, caller: &RootReport<'_>) {
    assert!(mir.cancellation().is_none());
    let path = straight_blocks(mir, mir.entry);
    let mut transfers = path
        .iter()
        .enumerate()
        .filter_map(|(i, block)| match block.operation {
            Operation::CallScalarDispose {
                implementation,
                operand,
            } => Some((i, block.site, implementation, operand)),
            _ => None,
        });
    for edge in caller.dependencies() {
        let (i, site, implementation, operand) = transfers.next().unwrap();
        assert_eq!(site.node, edge.site.node);
        assert!(std::ptr::eq(implementation, edge.declaration));
        assert!(operand.projection.is_none());
        let written = site.span.bytes();
        let same_slot = |other: Place<'_>| other.root.node == operand.root.node;
        if written == b"dispose(wide)" {
            assert_eq!(operand.root.span.bytes(), b"wide: i128");
        } else if written == b"dispose<bool>((other))" {
            assert_eq!(operand.root.span.bytes(), b"other: bool");
        } else {
            assert_eq!(written, b"release(flag)");
            assert!(operand.root.span.bytes().starts_with(b"flag"));
        }
        let later = &path[i + 1..];
        assert!(!path.iter().any(|block| matches!(
            block.operation,
            Operation::Drop(_) | Operation::Move { .. } | Operation::Dispose { .. }
        )));
        if caller.site().span.bytes().starts_with(b"function repair") {
            let install = later
                .iter()
                .position(|block| {
                    matches!(block.operation,
                Operation::Install { target, .. } if same_slot(target))
                })
                .expect("repair the consumed binding");
            assert!(
                later[install + 1..]
                    .iter()
                    .any(|block| matches!(block.operation,
                Operation::Copy { from, .. } if same_slot(from)))
            );
        } else {
            assert!(!later.iter().any(|block| matches!(&block.operation,
                Operation::Cleanup(actions) if actions.iter().any(|action| matches!(action,
                    Obligation::EndStorage(place) if same_slot(*place))))));
        }
        let mut copies = path[..i].iter().filter_map(|block| match block.operation {
            Operation::Copy { from, into } if same_slot(from) => Some(into),
            _ => None,
        });
        if caller
            .site()
            .span
            .bytes()
            .starts_with(b"function savedCopy")
        {
            let saved = copies
                .next()
                .flatten()
                .expect("copy the original before disposal");
            assert_eq!(saved.root.span.bytes(), b"saved");
            assert!(later.iter().any(|block| matches!(block.operation,
                Operation::Copy { from, .. } if from.root.node == saved.root.node)));
        }
        assert!(
            copies.next().is_none(),
            "disposal must not copy to an argument stage"
        );
    }
    assert!(transfers.next().is_none());
}

#[test]
fn scalar_copy_preflight_checks_argument_and_borrow_operand_depth() {
    probe_with_typed(
        "graph::scalar::scalar_copy_preflight_checks_argument_and_borrow_operand_depth",
        r#"
function pass(x: &bool): &bool { return x; }
function bare(x: &bool): bool { return copy(x); }
function grouped(x: &bool): bool { return copy(((((((((((x))))))))))); }
function computed(x: &bool): bool { return copy(((((((((((pass(x)))))))))))); }
type Pair = { flag: bool; }
function projected(x: Pair): bool { return copy(&((((((((((x.flag))))))))))); }
"#,
        |_, module, receipt, _| {
            let run = |name, limits| {
                lower_body(receipt, module, function_id(module, name), limits).unwrap()
            };
            let default = AnalysisLimits::default();
            let bare = run("bare", default);
            assert!(bare.mir().is_some());
            let limit = AnalysisLimits {
                max_depth: bare.statistics().max_depth,
                ..default
            };
            assert!(run("bare", limit).mir().is_some());
            for (name, expected) in [
                ("grouped", Completion::Checked),
                ("computed", Completion::Incomplete),
                ("projected", Completion::Incomplete),
            ] {
                assert_eq!(run(name, default).completion(), expected);
                let short = run(name, limit);
                assert_eq!(short.completion(), Completion::ResourceLimit, "{name}");
                assert!(short.mir().is_none());
            }
        },
    );
}

#[test]
fn distinct_scalar_targets_share_the_core_index_in_a_targeted_query() {
    let main = format!(
        "{MAIN}\nfunction plain(x: &bool): bool {{ return *x; }}\nfunction one(x: &bool): bool {{ return copy(x); }}\nfunction both(x: &bool, y: &i128): bool {{ let wide = copy(y); return copy(x); }}\nfunction oneDispose(x: bool): void {{ dispose(x); }}\nfunction mixed(x: bool, y: i128, z: &i8): i8 {{ dispose(x); dispose(y); return copy(z); }}"
    );
    let mut larger = format!("import extra::extraControl;\n{HELPER}");
    for i in 0..12 {
        larger.push_str(&format!("\nfunction other{i}(): bool {{ return true; }}"));
    }
    let mut baseline = [None; 2];
    fixture_with_main(
        "graph::scalar::distinct_scalar_targets_share_the_core_index_in_a_targeted_query",
        &main,
        &[HELPER, &larger],
        |index, request, graph, receipt, _| {
            if index == 0 && !request.test_mode {
                fs::write(
                    request.entry.with_file_name("extra.rill"),
                    "export function extraControl(): bool { return true; }",
                )
                .unwrap();
            }
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let work = ["plain", "one", "both", "oneDispose", "mixed"].map(|name| {
                let result = lower_body(
                    receipt,
                    module,
                    function_id(module, name),
                    AnalysisLimits::default(),
                )
                .unwrap();
                let mir = result.mir().unwrap_or_else(|| panic!("{name}: {result:?}"));
                let ops = linear(mir, mir.entry);
                let targets: Vec<_> = ops
                    .iter()
                    .filter_map(|op| match op {
                        Operation::CallScalarCopy { implementation, .. }
                        | Operation::CallScalarDispose { implementation, .. } => {
                            Some(implementation)
                        }
                        _ => None,
                    })
                    .collect();
                match name {
                    "plain" => assert!(targets.is_empty()),
                    "one" | "oneDispose" => assert_eq!(targets.len(), 1),
                    "both" => assert!(matches!(targets.as_slice(), [wide, flag] if wide != flag)),
                    "mixed" => assert!(
                        matches!(targets.as_slice(), [a, b, c] if a != b && a != c && b != c)
                    ),
                    _ => unreachable!(),
                }
                result.statistics().work
            });
            let prior = &mut baseline[usize::from(request.test_mode)];
            if index == 0 {
                *prior = Some(work);
            } else {
                let old = prior.unwrap();
                let [plain, one, both, one_dispose, mixed] =
                    std::array::from_fn(|i| work[i] - old[i]);
                assert!(
                    one > plain,
                    "indexing the additional capture facts must be charged"
                );
                assert_eq!(
                    both, one,
                    "a second selected width must not rescan unrelated modules or bodies"
                );
                assert_eq!(one_dispose, one);
                assert_eq!(mixed, one, "both families share one capture scan");
            }
        },
    );
}

#[test]
fn selected_generic_disposal_is_not_a_scalar_proof() {
    fixture_with_main(
        "graph::scalar::selected_generic_disposal_is_not_a_scalar_proof",
        &format!("{MAIN}\nfunction generic(items: List<bool>): void {{ dispose(items); }}"),
        &[HELPER],
        |_, _, graph, receipt, typed| {
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let node = module
                .syntax
                .nodes
                .iter()
                .position(|n| n.span.bytes() == b"dispose(items)")
                .unwrap();
            let facts = typed.bodies().node(module, node).unwrap().unwrap();
            let NodeFactKind::Expression(facts) = &facts.kind else {
                panic!("expected a typed call")
            };
            let Some(DataExpression::CanonicalCall(goal)) = &facts.data else {
                panic!("expected a canonical goal")
            };
            let proof = typed.bodies().goal(goal).unwrap();
            assert_eq!(proof.state, GoalState::Proved);
            assert!(!proof.selection.as_ref().unwrap().substitution.is_empty());
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let caller = root(&report, b"function generic");
            assert_pending(caller);
            assert!(caller.dependencies().is_empty());
            let targeted =
                lower_root(receipt, module, caller.owner(), AnalysisLimits::default()).unwrap();
            assert_eq!(targeted.completion(), Completion::Incomplete);
            assert!(targeted.mir().is_none());
        },
    );
}

#[test]
fn absent_and_conditional_disposal_do_not_grant_a_typed_receipt() {
    probe_report(
        "graph::scalar::absent_and_conditional_disposal_do_not_grant_a_typed_receipt",
        r#"
function symbolic<T>(value: T): void where dispose(T) { dispose(value); }
type Envelope<T> = { item: T; }
implement<T> dispose(value: Envelope<T>): void where sendable(T) {}
function conditional(wrapped: Envelope<bool>): void { dispose(wrapped); }
"#,
        |_, _, typed| {
            assert!(typed.receipt().is_none());
            let obligations: Vec<_> = typed
                .bodies()
                .obligations()
                .iter()
                .map(|o| (o.reason, o.site.span.bytes().escape_ascii().to_string()))
                .collect();
            assert!(
                obligations.iter().any(|(_, site)| site == "dispose(value)"),
                "{obligations:?}"
            );
            assert!(
                obligations
                    .iter()
                    .any(|(_, site)| site == "dispose(wrapped)"),
                "{obligations:?}"
            );
        },
    );
}

#[test]
fn aliased_primitive_overlap_still_blocks_the_typed_receipt() {
    installed(
        "graph::scalar::aliased_primitive_overlap_still_blocks_the_typed_receipt",
        |request| {
            let copy = r#"
import std::core::copy as clone;
implement clone(value: &i32): i32 { return *value; }
function replacement(value: &i32): i32 { return clone(value); }
"#;
            let dispose = r#"
import std::core::dispose as release;
implement release(value: i32): void {}
function replacement(value: i32): void { release(value); }
"#;
            fs::write(request.entry.with_file_name("helper.rill"), HELPER).unwrap();
            for (role, source) in [
                (PrimitiveRole::Copy, copy),
                (PrimitiveRole::Dispose, dispose),
            ] {
                fs::write(&request.entry, format!("{MAIN}{source}")).unwrap();
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
                        .find(|scope| scope.role == role && scope.primitive == Primitive::I32)
                        .unwrap();
                    assert_eq!(scope.coverage, PrimitiveCoverage::Overlap);
                    let [overlap] = scope.overlaps.as_slice() else {
                        panic!("expected a protected overlap")
                    };
                    assert_eq!(overlap.code(), "R0500");
                    assert_eq!(overlap.implementation.id.module.origin, Origin::Application);
                }
            }
        },
    );
}
