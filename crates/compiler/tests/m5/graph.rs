use super::{
    roots::{HELPER, MAIN, fixture, fixture_with_main},
    *,
};
use compiler::analysis::{
    ownership::{
        DependencyContract, GraphCompletion, GraphObligation, GraphReport, RootReport,
        analyze_ownership,
    },
    scalars::BodyOwner,
};
use loader::DeclarationKind::{Expectation, Function, Implementation};

#[test]
fn installed_inventory_keeps_standard_and_independent_application_roots() {
    let mut normal = None;
    fixture(
        "graph::installed_inventory_keeps_standard_and_independent_application_roots",
        &[HELPER],
        |_, request, graph, receipt, typed| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.context(), graph.id());
            assert_eq!(report.test_mode(), request.test_mode);
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
            assert!(report.inventory_complete());
            assert!(report.findings().is_empty());
            assert_eq!(report.roots().len(), 173);
            let ordered: Vec<_> = report
                .roots()
                .iter()
                .map(|root| {
                    (
                        root.site().module.clone(),
                        match root.owner() {
                            BodyOwner::Startup => None,
                            BodyOwner::Declaration(id) => Some((id.start, id.kind)),
                        },
                    )
                })
                .collect();
            assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
            if let Some((normal_context, normal_order, normal_stats)) = normal.take() {
                assert_ne!(report.context(), normal_context);
                assert_eq!(ordered, normal_order);
                assert_eq!(report.statistics(), normal_stats);
            } else {
                normal = Some((report.context(), ordered, report.statistics()));
            }
            assert_eq!(
                report.open_obligations(),
                &[
                    GraphObligation::IntrinsicCancellation,
                    GraphObligation::MaskingAndJoin,
                    GraphObligation::EmbeddedAndUnreachedSource,
                ]
            );

            let (app, standard) = report.roots().split_at(8);
            assert_eq!(
                app.iter()
                    .map(|root| (
                        root.site().module.path.as_str(),
                        match root.owner() {
                            BodyOwner::Startup => None,
                            BodyOwner::Declaration(id) => {
                                assert_eq!(id.context, graph.id());
                                assert_eq!(&id.module, root.site().module);
                                Some(id.kind)
                            }
                        }
                    ))
                    .collect::<Vec<_>>(),
                [
                    ("helper.rill", None),
                    ("helper.rill", Some(Function)),
                    ("helper.rill", Some(Implementation)),
                    ("helper.rill", Some(Function)),
                    ("helper.rill", Some(Expectation)),
                    ("main.rill", None),
                    ("main.rill", Some(Function)),
                    ("main.rill", Some(Expectation)),
                ]
            );
            assert!(
                app.iter()
                    .all(|root| root.site().module.origin == Origin::Application
                        && root.completion() == Completion::Checked
                        && root.mir().is_some())
            );
            assert!(
                standard
                    .iter()
                    .all(|root| root.site().module.origin == Origin::Standard)
            );
            assert_eq!(
                standard
                    .iter()
                    .filter(|root| root.owner() == BodyOwner::Startup
                        && root.completion() == Completion::Checked
                        && root.mir().is_some())
                    .count(),
                17
            );
            assert_eq!(
                standard
                    .iter()
                    .filter(|root| root.completion() == Completion::Incomplete
                        && root.blocked_at().is_some()
                        && root.mir().is_none())
                    .count(),
                148
            );
            for (kind, count) in [(Function, 18), (Implementation, 130)] {
                assert_eq!(standard.iter().filter(|root| matches!(root.owner(), BodyOwner::Declaration(id) if id.kind == kind)).count(), count);
            }
            let native = &typed.primitive_providers()[0].implementation.id;
            let provider = standard
                .iter()
                .find(|root| root.owner() == BodyOwner::Declaration(native))
                .unwrap();
            assert_eq!(provider.completion(), Completion::Incomplete);
            assert_eq!(provider.blocked_at().unwrap().node, provider.site().node);
            assert!(provider.mir().is_none());

            let expectation = app[4].mir().unwrap();
            assert_eq!(
                straight_blocks(expectation, expectation.entry)
                    .last()
                    .unwrap()
                    .terminator,
                Terminator::FinishExpectation
            );
            let startup = app[0].mir().unwrap();
            assert!(straight_blocks(startup, startup.entry).iter().any(|block| matches!(
                &block.operation, Operation::Cleanup(actions)
                if matches!(actions.as_slice(), [Obligation::Drop(place)] if place.root.span.bytes() == b"taken")
            )));
        },
    );
}

#[test]
fn installed_reuse_is_reported_once_without_poisoning_independent_roots() {
    let implementation = HELPER.replace("implement unused(value: string): bool { let taken = value;  return true; }",
        "implement unused(value: string): bool { let taken = value; let again = value; return true; }");
    let startup = format!("{HELPER}let again = text;\n");
    let reached = HELPER.replace("function retainedControl(value: string): bool { let taken = value;  return true; }",
        "function retainedControl(value: string): bool { let taken = value; let again = value; return true; }");
    let sources = [&implementation[..], &startup[..], &reached[..]];
    fixture(
        "graph::installed_reuse_is_reported_once_without_poisoning_independent_roots",
        &sources,
        |index, _, graph, receipt, _| {
            let source = sources[index];
            let owner = ["value", "text", "value"][index];
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.completion(), GraphCompletion::SourceRejected);
            assert!(report.inventory_complete());
            assert_eq!(report.roots().len(), 173);
            let [finding] = report.findings() else {
                panic!("{:?}", report.findings());
            };
            assert_eq!(finding.code(), "R0200");
            let last = source.find(&format!("again = {owner}")).unwrap() + "again = ".len();
            let first =
                source[..last].rfind(&format!("taken = {owner}")).unwrap() + "taken = ".len();
            for (site, offset) in [(finding.site, last), (finding.related, first)] {
                assert_eq!(
                    (site.module.origin, site.module.path.as_str()),
                    (Origin::Application, "helper.rill")
                );
                assert_eq!(site.span.start(), offset);
                assert_eq!(site.span.bytes(), owner.as_bytes());
            }
            let affected: &[usize] = [&[2][..], &[0], &[3, 4]][index];
            for (i, root) in report.roots()[..8].iter().enumerate() {
                if affected.contains(&i) {
                    assert_eq!(root.completion(), Completion::SourceRejected);
                    assert!(root.mir().is_none());
                } else {
                    assert_eq!(root.completion(), Completion::Checked);
                    assert!(root.mir().is_some());
                }
            }
            let [edge] = report.roots()[4].dependencies() else {
                panic!("missing expectation dependency");
            };
            assert_eq!(
                BodyOwner::Declaration(edge.declaration),
                report.roots()[3].owner()
            );
            assert_eq!(edge.contract, DependencyContract::OrdinaryCall);
            assert_eq!(edge.site.span.bytes(), b"retainedControl(\"expectation\")");
            let entry = &report.roots()[6];
            let BodyOwner::Declaration(entry_id) = entry.owner() else {
                panic!("missing entry");
            };
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key == entry_id.module)
                .unwrap();
            assert_eq!(
                lower_body(receipt, module, entry_id, AnalysisLimits::default())
                    .unwrap()
                    .completion(),
                Completion::Checked
            );
        },
    );
}

#[test]
fn one_budget_leaves_unfinished_work_unproved() {
    let startup = format!("{HELPER}let again = text;\n");
    fixture(
        "graph::one_budget_leaves_unfinished_work_unproved",
        &[&startup],
        |_, _, _, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.completion(), GraphCompletion::SourceRejected);
            let stats = report.statistics();
            let limits = AnalysisLimits {
                max_work: stats.work,
                max_entries: stats.entries,
                max_depth: stats.max_depth,
            };
            let exact = analyze_ownership(receipt, limits);
            assert_eq!(exact.statistics(), stats);
            assert_eq!(exact.completion(), report.completion());
            assert_eq!(exact.findings().len(), 1);
            assert_eq!(
                exact
                    .roots()
                    .iter()
                    .filter(|root| root.mir().is_some())
                    .count(),
                24
            );
            for short in [
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
            ] {
                let exhausted = analyze_ownership(receipt, short);
                assert_eq!(
                    exhausted.completion(),
                    GraphCompletion::ResourceLimit,
                    "{short:?}"
                );
                assert!(exhausted.inventory_complete());
                assert_eq!(exhausted.roots().len(), 173);
                assert!(
                    exhausted
                        .roots()
                        .iter()
                        .any(|root| root.completion() == Completion::ResourceLimit)
                );
                assert!(exhausted.roots().iter().all(|root| root.mir().is_none()));
                if short.max_depth == limits.max_depth {
                    assert_eq!(
                        exhausted.findings().len(),
                        1,
                        "late exhaustion must retain local findings"
                    );
                }
            }
            let prefix = analyze_ownership(
                receipt,
                AnalysisLimits {
                    max_entries: 15,
                    ..limits
                },
            );
            assert_eq!(prefix.completion(), GraphCompletion::ResourceLimit);
            assert!(!prefix.inventory_complete());
            assert!(!prefix.roots().is_empty() && prefix.roots().len() < 8);
            assert!(
                prefix
                    .roots()
                    .iter()
                    .all(|root| root.completion() == Completion::ResourceLimit
                        && root.mir().is_none())
            );
            let zero = analyze_ownership(
                receipt,
                AnalysisLimits {
                    max_work: 0,
                    max_entries: 0,
                    max_depth: 0,
                },
            );
            assert_eq!(zero.completion(), GraphCompletion::ResourceLimit);
            assert!(!zero.inventory_complete());
            assert!(zero.roots().is_empty());
        },
    );
}

fn root<'a, 'g>(report: &'a GraphReport<'g>, prefix: &[u8]) -> &'a RootReport<'g> {
    report
        .roots()
        .iter()
        .find(|root| {
            root.site().module.origin == Origin::Application
                && root.owner() != BodyOwner::Startup
                && root.site().span.bytes().starts_with(prefix)
        })
        .unwrap_or_else(|| {
            panic!(
                "missing root starting with {:?}",
                String::from_utf8_lossy(prefix)
            )
        })
}

#[test]
fn aliases_share_a_canonical_body_and_cycles_require_all_edges() {
    let main = format!("{MAIN}\nimport helper::ready as second;\nexpect second();\n");
    let source = format!(
        r#"{HELPER}
function left(flag: bool): bool {{ return right(flag); }}
function right(flag: bool): bool {{
    if (flag) {{ return left(flag); }}
    let owner = "end";
    dispose(owner);
    return false;
}}
expect left(true);
"#
    );
    let closed = source.replace("    dispose(owner);\n", "");
    fixture_with_main(
        "graph::aliases_share_a_canonical_body_and_cycles_require_all_edges",
        &main,
        &[&closed, &source],
        |index, _, _, receipt, typed| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
            assert!(report.findings().is_empty());
            assert_eq!(report.roots().len(), 177);
            let first = root(&report, b"expect ready()");
            let second = root(&report, b"expect second()");
            assert_eq!(first.completion(), Completion::Checked);
            assert_eq!(second.completion(), Completion::Checked);
            let ([a], [b]) = (first.dependencies(), second.dependencies()) else {
                panic!("missing alias edge");
            };
            assert_eq!(a.declaration, b.declaration);
            assert_ne!(a.site.span.start(), b.site.span.start());
            assert_eq!(a.contract, DependencyContract::OrdinaryCall);
            assert_eq!(a.contract, b.contract);
            assert_eq!(
                report
                    .roots()
                    .iter()
                    .filter(|root| root.owner() == BodyOwner::Declaration(a.declaration))
                    .count(),
                1
            );

            let left = root(&report, b"function left(");
            let right = root(&report, b"function right(");
            let expectation = root(&report, b"expect left(true)");
            for root in [left, right, expectation] {
                if index == 0 {
                    assert_eq!(root.completion(), Completion::Checked);
                    assert!(root.mir().is_some());
                } else {
                    assert_eq!(root.completion(), Completion::Incomplete);
                    assert!(root.mir().is_none());
                    assert_eq!(root.blocked_at().unwrap().span.bytes(), b"dispose(owner)");
                }
            }
            for (caller, callee) in [(left, right), (right, left), (expectation, left)] {
                assert!(
                    caller
                        .dependencies()
                        .iter()
                        .any(|edge| edge.contract == DependencyContract::OrdinaryCall
                            && BodyOwner::Declaration(edge.declaration) == callee.owner())
                );
            }
            assert_eq!(
                right
                    .dependencies()
                    .iter()
                    .any(|edge| edge.contract == DependencyContract::BuiltinDispose
                        && edge.declaration == &typed.dispose_role().unwrap().id),
                index == 1
            );
        },
    );
}

#[test]
fn nested_and_unused_bodies_are_independent_source_obligations() {
    let source = format!(
        r#"{HELPER}
function outer(): bool {{
    return true;
    function nested(owner: string): bool {{ let taken = owner; let again = owner; return true; }}
}}
function neverCalled(owner: string): bool {{ let taken = owner; let again = owner; return true; }}
"#
    );
    fixture(
        "graph::nested_and_unused_bodies_are_independent_source_obligations",
        &[&source],
        |_, _, _, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.completion(), GraphCompletion::SourceRejected);
            assert_eq!(report.roots().len(), 176);
            let outer = root(&report, b"function outer(");
            assert_eq!(outer.completion(), Completion::Checked);
            assert!(outer.mir().is_some());
            assert!(outer.dependencies().is_empty());
            for prefix in [b"function nested(".as_slice(), b"function neverCalled("] {
                let rejected = root(&report, prefix);
                assert_eq!(rejected.completion(), Completion::SourceRejected);
                assert!(rejected.mir().is_none());
            }
            let [nested, unused] = report.findings() else {
                panic!("{:?}", report.findings());
            };
            assert_eq!(nested.code(), "R0200");
            assert_eq!(unused.code(), "R0200");
            let sites = source
                .match_indices("again = owner")
                .map(|(start, _)| start + "again = ".len())
                .collect::<Vec<_>>();
            assert_eq!(
                [nested.site.span.start(), unused.site.span.start()],
                sites.as_slice()
            );
        },
    );
}

#[test]
fn active_disposer_is_not_a_source_root_or_a_protected_provider_proof() {
    use compiler::analysis::typed::{NativeOwnershipAction, OwnershipRule};
    let source = format!(
        r#"{HELPER}
import std::core::dispose as release;
import std::mutex::LockGuard;
type Custom = {{ keep: string; }}
implement release(value: Custom): void {{}}
function dropCustom(value: Custom): void {{}}
function dropCustomAgain(value: Custom): void {{}}
function dropGuard(held: LockGuard<i32>): void {{}}
"#
    );
    fixture(
        "graph::active_disposer_is_not_a_source_root_or_a_protected_provider_proof",
        &[&source],
        |_, _, _, receipt, typed| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
            let implementation = root(&report, b"implement release(");
            assert_eq!(implementation.completion(), Completion::Incomplete);
            let [active] = report.active_disposers() else {
                panic!("expected one canonical active frame");
            };
            assert_eq!(active.owner(), implementation.owner());
            assert_eq!(active.completion(), Completion::Checked);
            let mir = active.mir().unwrap();
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)] if place.field().unwrap().name.as_deref() == Some("keep"))
            );
            for prefix in [
                b"function dropCustom(".as_slice(),
                b"function dropCustomAgain(",
            ] {
                let caller = root(&report, prefix);
                assert_eq!(caller.completion(), Completion::Checked);
                let [edge] = caller.dependencies() else {
                    panic!("missing active cleanup edge");
                };
                assert_eq!(edge.contract, DependencyContract::ActiveDisposer);
                assert_eq!(BodyOwner::Declaration(edge.declaration), active.owner());
            }
            let provider = typed
                .ownership_protection()
                .iter()
                .find(|scope| {
                    scope.rule == OwnershipRule::Required(NativeOwnershipAction::GuardRelease)
                })
                .unwrap()
                .provider
                .as_ref()
                .unwrap();
            let protected = root(&report, b"function dropGuard(");
            assert_eq!(protected.completion(), Completion::Incomplete);
            assert!(protected.mir().is_none());
            let [edge] = protected.dependencies() else {
                panic!("missing protected provider edge");
            };
            assert_eq!(edge.declaration, &provider.implementation.id);
            assert_eq!(
                edge.contract,
                DependencyContract::ProtectedRelease {
                    entry: &provider.entry.id
                }
            );
            let wrapper = report
                .roots()
                .iter()
                .find(|root| root.owner() == BodyOwner::Declaration(edge.declaration))
                .unwrap();
            assert_eq!(wrapper.completion(), Completion::Incomplete);
            assert!(wrapper.mir().is_none());
        },
    );
}
