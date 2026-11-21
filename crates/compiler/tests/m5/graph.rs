use super::{
    roots::{HELPER, MAIN, fixture, fixture_with_main},
    *,
};
use compiler::analysis::{
    Statistics,
    ownership::{
        CallableField, CallableLayout, Delivery, DependencyContract, GraphCompletion,
        GraphObligation, GraphReport, RootReport, analyze_ownership,
    },
    scalars::BodyOwner,
};
use loader::DeclarationKind::{Expectation, Function, Implementation};

#[path = "graph/closed_release.rs"]
mod closed_release;
#[path = "graph/delivery.rs"]
mod delivery;
#[path = "graph/scalar.rs"]
mod scalar;

#[test]
fn callable_transfer_specialization_is_not_an_open_source_root_proof() {
    probe(
        "graph::callable_transfer_specialization_is_not_an_open_source_root_proof",
        r#"
            function forward(job: once () => [&bool, bool]): once () => [&bool, bool] { return job; }
            function after(seed: bool): void {
                let mut flag = seed;
                let other = true;
                let held = forward(() => [&flag, other]);
                let _ = held;
                flag = true;
            }
            function overlap(seed: bool): void {
                let mut flag = seed;
                let other = true;
                let held = forward(() => [&flag, other]);
                flag = true;
                let _ = held;
            }
        "#,
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let producer = root(&graph, b"function forward");
            assert_eq!(producer.ordinary_completion(), Completion::Incomplete);
            assert!(producer.ordinary_mir().is_none());
            let BodyOwner::Declaration(id) = producer.owner() else {
                panic!("named callee")
            };
            let after = root(&graph, b"function after");
            assert_eq!(after.ordinary_completion(), Completion::Checked);
            assert!(after.ordinary_mir().is_some());
            assert!(matches!(after.enabled_frame(), Delivery::Unresolved));
            assert!(matches!(after.selected_call(), Delivery::Unresolved));
            let [edge] = after.dependencies() else {
                panic!("one canonical dependency")
            };
            assert_eq!(edge.declaration, id);
            assert_eq!(
                edge.contract,
                DependencyContract::CallableTransfer(CallableLayout([
                    CallableField::SharedPrimitive("bool"),
                    CallableField::OwnedPrimitive("bool"),
                ]))
            );
            let overlap = root(&graph, b"function overlap");
            assert_eq!(overlap.ordinary_completion(), Completion::SourceRejected);
            assert!(overlap.ordinary_mir().is_none());
            let found = graph
                .findings()
                .iter()
                .find(|finding| finding.site.module == &module.key)
                .unwrap();
            assert_eq!(found.code(), "R0203");
            assert_eq!(found.related.span.bytes(), b"&flag");
            let BodyOwner::Declaration(id) = overlap.owner() else {
                panic!("named caller")
            };
            let full = lower_body(receipt, module, id, AnalysisLimits::default()).unwrap();
            assert_eq!(full.completion(), Completion::SourceRejected);
            let short = lower_body(
                receipt,
                module,
                id,
                AnalysisLimits {
                    max_entries: full.statistics().entries - 1,
                    ..AnalysisLimits::default()
                },
            )
            .unwrap();
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(short.mir().is_none() && short.findings().is_empty());
        },
    );
}

#[test]
fn callable_transfers_share_only_the_same_canonical_body_in_a_graph() {
    let source = r#"
        function forward(job: once () => [&bool, bool]): once () => [&bool, bool] { return job; }
        export function twice(seed: bool): void {
            let other = true;
            let _ = forward(() => [&seed, other]);
            let _ = forward(() => [&seed, other]);
        }
        function onceCaller(seed: bool): void {
            let other = false;
            let _ = forward(() => [&seed, other]);
        }
    "#;
    probe_report_with_modules(
        "graph::callable_transfers_share_only_the_same_canonical_body_in_a_graph",
        "import left::twice as leftTwice; import right::twice as rightTwice;",
        &[("left.rill", source), ("right.rill", source)],
        |_, _, typed| {
            fn transfers<'m, 'g>(
                mir: &'m Mir<'g>,
            ) -> Vec<(&'g loader::DeclarationId, &'m std::sync::Arc<Mir<'g>>)> {
                mir.blocks
                    .iter()
                    .filter_map(|block| match &block.operation {
                        Operation::CallCallableTransfer { callee, body, .. } => {
                            Some((*callee, body))
                        }
                        _ => None,
                    })
                    .collect()
            }

            let report = analyze_ownership(
                &typed.receipt().expect("typed receipt"),
                AnalysisLimits::default(),
            );
            let checked = |path: &str, prefix: &[u8]| {
                let root = report
                    .roots()
                    .iter()
                    .find(|root| {
                        root.site().module.path == path
                            && root.site().span.bytes().starts_with(prefix)
                    })
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Checked);
                transfers(root.ordinary_mir().unwrap())
            };
            let twice = checked("left.rill", b"function twice");
            let once = checked("left.rill", b"function onceCaller");
            let other = checked("right.rill", b"function twice");
            let ([(id, first), (_, second)], [(_, third)], [(other_id, other), ..]) =
                (twice.as_slice(), once.as_slice(), other.as_slice())
            else {
                panic!("expected the written transfers");
            };
            assert!(std::sync::Arc::ptr_eq(first, second));
            assert!(std::sync::Arc::ptr_eq(first, third));
            assert_eq!(id.start, other_id.start);
            assert_ne!(id.module, other_id.module);
            assert!(!std::sync::Arc::ptr_eq(first, other));
            assert_eq!(other.blocks[other.entry].site.module, &other_id.module);
        },
    );
}

#[test]
fn targeted_callable_transfer_dependencies_share_the_module_scan() {
    let main = format!(
        "{MAIN}\nimport helper::{{first, second}};\n\
         function one(seed: bool): void {{ first(seed); }}\n\
         function two(seed: bool): void {{ first(seed); second(seed); }}"
    );
    let helper = format!(
        r#"{HELPER}
        function forward(job: once () => [&bool, bool]): once () => [&bool, bool] {{ return job; }}
        export function first(seed: bool): void {{
            let other = true; let _ = forward(() => [&seed, other]);
        }}
        export function second(seed: bool): void {{
            let other = false; let _ = forward(() => [&seed, other]);
        }}"#
    );
    let noise = "let _ = true;\n".repeat(64);
    let quiet = format!("{helper}\nfunction unrelated(): void {{}}");
    let noisy = format!("{helper}\nfunction unrelated(): void {{ {noise} }}");
    let mut quiet_stats = [[Statistics::default(); 2]; 2];
    fixture_with_main(
        "graph::targeted_callable_transfer_dependencies_share_the_module_scan",
        &main,
        &[&quiet, &noisy],
        |variant, request, graph, receipt, _| {
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
                .unwrap();
            let stats = ["one", "two"].map(|name| {
                let caller = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let run = |limits| lower_body(receipt, module, &caller.id, limits).unwrap();
                let checked = run(AnalysisLimits::default());
                assert_eq!(checked.completion(), Completion::Checked, "{name}");
                let used = checked.statistics();
                let exact = AnalysisLimits {
                    max_work: used.work,
                    max_entries: used.entries,
                    max_depth: used.max_depth,
                };
                assert!(run(exact).mir().is_some());
                for short in [
                    AnalysisLimits {
                        max_work: used.work - 1,
                        ..exact
                    },
                    AnalysisLimits {
                        max_entries: used.entries - 1,
                        ..exact
                    },
                ] {
                    let limited = run(short);
                    assert_eq!(limited.completion(), Completion::ResourceLimit);
                    assert!(limited.mir().is_none());
                }
                used
            });
            let baseline = &mut quiet_stats[usize::from(request.test_mode)];
            if variant == 0 {
                *baseline = stats;
            } else {
                let [(one_work, one_entries), two] = [0, 1].map(|i| {
                    (
                        stats[i].work - baseline[i].work,
                        stats[i].entries - baseline[i].entries,
                    )
                });
                assert!(one_work > 0 && one_entries > 0);
                assert_eq!(two, (one_work, one_entries));
            }
        },
    );
}

#[test]
fn unsupported_nested_transfer_does_not_hide_an_independent_escape() {
    probe(
        "graph::unsupported_nested_transfer_does_not_hide_an_independent_escape",
        r#"
function forward(job: once () => [&bool, bool]): once () => [&bool, bool] { return job; }
function independent(job: once () => [&bool, bool]): once () => [&bool, bool] {
    let mut gate = false;
    if (gate) {
        let bit = true; let other = false; let _ = forward(() => [&bit, other]); return job;
    }
    let local = true; let _ = job; return () => [&local, local];
}
function beyond(job: once () => [&bool, bool]): once () => [&bool, bool] {
    let bit = true; let other = false; let _ = forward(() => [&bit, other]);
    let local = true; let _ = job; return () => [&local, local];
}
function bad(seed: bool): void { let bit = false; let _ = independent(() => [&seed, bit]); }
function unknown(seed: bool): void { let bit = false; let _ = beyond(() => [&seed, bit]); }
"#,
        |_, module, receipt| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.completion(), GraphCompletion::SourceRejected);
            for (prefix, expected) in [
                (b"function bad(".as_slice(), Completion::SourceRejected),
                (b"function unknown(", Completion::Incomplete),
            ] {
                let caller = root(&report, prefix);
                assert_eq!(caller.ordinary_completion(), expected);
                assert!(caller.mir().is_none());
                let BodyOwner::Declaration(id) = caller.owner() else {
                    panic!("named caller")
                };
                let target = lower_body(receipt, module, id, AnalysisLimits::default()).unwrap();
                assert_eq!(target.completion(), expected);
                assert!(target.mir().is_none());
                if expected == Completion::Incomplete {
                    assert!(target.findings().is_empty());
                    continue;
                }
                for findings in [report.findings(), target.findings()] {
                    let [finding] = findings else {
                        panic!("expected the independent escape")
                    };
                    let producer = root(&report, b"function independent(").site().span;
                    assert_eq!(finding.code(), "R0202");
                    assert_eq!(finding.related.span.bytes(), b"&local");
                    assert!(
                        producer.start() <= finding.site.span.start()
                            && finding.site.span.end() <= producer.end()
                    );
                    assert!(
                        producer.start() <= finding.related.span.start()
                            && finding.related.span.end() <= producer.end()
                    );
                }
            }
        },
    );
}

#[test]
fn closed_callable_transfer_preserves_the_callees_ordinary_finding() {
    probe(
        "graph::closed_callable_transfer_preserves_the_callees_ordinary_finding",
        r#"
        function forward(job: once () => [&bool, bool]): once () => [&bool, bool] {
            let text = "owner"; let first = text; let again = text; return job;
        }
        function caller(seed: bool): void {
            let other = true; let held = forward(() => [&seed, other]); let _ = held;
        }
        function again(seed: bool): void {
            let other = false; let _ = forward(() => [&seed, other]);
        }
        function nested(job: once () => [&bool, bool]): once () => [&bool, bool] {
            let bit = true; let other = false;
            let _ = forward(() => [&bit, other]); return job;
        }
        function unknownFirst(seed: bool): void {
            let other = true; let _ = nested(() => [&seed, other]);
        }
        function unknownSecond(seed: bool): void {
            let other = false; let _ = nested(() => [&other, seed]);
        }
    "#,
        |_, module, receipt| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let caller = root(&report, b"function caller");
            let producer = root(&report, b"function forward");
            assert_eq!(producer.completion(), Completion::Incomplete);
            for caller in [caller, root(&report, b"function again")] {
                assert_eq!(caller.ordinary_completion(), Completion::SourceRejected);
                assert!(caller.ordinary_mir().is_none());
            }
            for (prefix, call) in [
                (
                    b"function unknownFirst".as_slice(),
                    b"nested(() => [&seed, other])".as_slice(),
                ),
                (
                    b"function unknownSecond".as_slice(),
                    b"nested(() => [&other, seed])".as_slice(),
                ),
            ] {
                let unknown = root(&report, prefix);
                assert_eq!(unknown.ordinary_completion(), Completion::Incomplete);
                assert!(unknown.ordinary_mir().is_none());
                assert_eq!(unknown.blocked_at().unwrap().span.bytes(), call);
            }
            let BodyOwner::Declaration(id) = caller.owner() else {
                panic!("named caller")
            };
            let targeted = lower_body(receipt, module, id, AnalysisLimits::default()).unwrap();
            assert_eq!(targeted.completion(), Completion::SourceRejected);
            assert!(targeted.mir().is_none());
            for findings in [targeted.findings(), report.findings()] {
                let [finding] = findings else {
                    panic!("expected one callee finding")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), b"text");
                assert_eq!(finding.related.span.bytes(), b"text");
                assert!(
                    producer.site().span.start() <= finding.related.span.start()
                        && finding.related.span.start() < finding.site.span.start()
                        && finding.site.span.end() <= producer.site().span.end()
                );
            }
        },
    );
}

#[test]
fn uncalled_enabled_frame_rejects_only_the_delivery_judgment() {
    let source = format!(
        r#"{HELPER}
function unsafeBody(slot: &mut &bool, input: &bool, enter: bool): void {{
    let flag = true;
    *slot = &flag;
    while (enter) {{ continue; }}
    *slot = input;
}}
"#
    );
    fixture(
        "graph::uncalled_enabled_frame_rejects_only_the_delivery_judgment",
        &[&source],
        |_, request, _, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(
                report.completion(),
                GraphCompletion::SourceRejected,
                "mode={}",
                request.test_mode
            );
            let unsafe_body = root(&report, b"function unsafeBody(");
            assert_eq!(unsafe_body.completion(), Completion::SourceRejected);
            assert!(unsafe_body.mir().is_none());
            assert_eq!(unsafe_body.ordinary_completion(), Completion::Checked);
            assert!(unsafe_body.ordinary_mir().unwrap().cancellation().is_none());
            assert!(matches!(
                unsafe_body.enabled_frame(),
                Delivery::Rejected {
                    finding_id: 0,
                    via_call: None
                }
            ));
            assert!(matches!(unsafe_body.selected_call(), Delivery::Unresolved));
            let [finding] = report.findings() else {
                panic!("expected a direct enabled-frame finding");
            };
            assert_eq!(finding.code(), "R0202");
            assert_eq!(finding.site.span.bytes(), b"*slot = &flag;");
            assert_eq!(finding.related.span.bytes(), b"&flag");
            assert_eq!(
                finding.cancellation_delivery.unwrap().span.bytes(),
                b"while (enter) { continue; }"
            );
            assert_eq!(
                root(&report, b"function entryControl(").completion(),
                Completion::Checked
            );
        },
    );
}

#[test]
fn selected_alias_calls_share_only_their_canonical_delivery_witness() {
    installed(
        "graph::selected_alias_calls_share_only_their_canonical_delivery_witness",
        |request| {
            let callee_source = r#"export function unsafeBody(slot: &mut &bool, input: &bool, enter: bool): void {
    let flag = true;
    *slot = &flag;
    while (enter) { continue; }
    *slot = input;
}
"#;
            let mut main = format!(
                r#"{MAIN}
import left::unsafeBody as first;
import left::unsafeBody as again;
import right::unsafeBody as other;
"#
            );
            for (caller, alias) in [
                ("firstCaller", "first"),
                ("secondCaller", "again"),
                ("otherCaller", "other"),
            ] {
                use std::fmt::Write;
                write!(
                    main,
                    r#"
function {caller}(enter: bool): void {{
    let prior = false;
    let input = true;
    let mut slot = &prior;
    {alias}(&mut slot, &input, enter);
}}
"#
                )
                .unwrap();
            }
            fs::write(&request.entry, &main).unwrap();
            for (name, source) in [
                ("left.rill", callee_source),
                ("right.rill", callee_source),
                ("helper.rill", HELPER),
            ] {
                fs::write(request.entry.with_file_name(name), source).unwrap();
            }
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let receipt = typed.receipt().expect("installed typed receipt");
                let report = analyze_ownership(&receipt, AnalysisLimits::default());
                assert_eq!(report.completion(), GraphCompletion::SourceRejected);
                assert_eq!(
                    report
                        .roots()
                        .iter()
                        .filter(|root| root.completion() == Completion::SourceRejected)
                        .count(),
                    5
                );
                let [left, right] = report.findings() else {
                    panic!("expected one finding per canonical callee");
                };
                let module = |path| {
                    graph
                        .modules()
                        .iter()
                        .find(|module| {
                            module.key.origin == Origin::Application && module.key.path == path
                        })
                        .unwrap()
                };
                for (i, (path, finding)) in [("left.rill", left), ("right.rill", right)]
                    .into_iter()
                    .enumerate()
                {
                    let module = module(path);
                    let declaration = module
                        .declarations
                        .iter()
                        .find(|declaration| declaration.name.as_deref() == Some("unsafeBody"))
                        .unwrap();
                    let callee = report
                        .roots()
                        .iter()
                        .find(|root| root.owner() == BodyOwner::Declaration(&declaration.id))
                        .unwrap();
                    assert_eq!(callee.completion(), Completion::SourceRejected);
                    assert_eq!(callee.ordinary_completion(), Completion::Checked);
                    assert!(callee.mir().is_none() && callee.ordinary_mir().is_some());
                    assert!(
                        matches!(callee.enabled_frame(), Delivery::Rejected { finding_id, via_call: None } if *finding_id == i)
                    );
                    assert!(matches!(callee.selected_call(), Delivery::Unresolved));
                    let nodes = &module.syntax.nodes;
                    let [_, store, backedge, _] =
                        nodes[nodes[declaration.node].children[1]].children[..]
                    else {
                        panic!("expected paired stores around the loop");
                    };
                    assert_eq!(finding.code(), "R0202");
                    for (site, node, text) in [
                        (finding.site, store, "*slot = &flag;"),
                        (finding.related, nodes[store].children[1], "&flag"),
                        (
                            finding.cancellation_delivery.unwrap(),
                            backedge,
                            "while (enter) { continue; }",
                        ),
                    ] {
                        assert_eq!((site.module, site.node), (&module.key, node));
                        assert_eq!(site.span.bytes(), text.as_bytes());
                        assert_eq!(site.span.start(), callee_source.find(text).unwrap());
                    }
                }
                for (prefix, text, expected_id) in [
                    (
                        b"function firstCaller(".as_slice(),
                        "first(&mut slot, &input, enter)",
                        0,
                    ),
                    (
                        b"function secondCaller(",
                        "again(&mut slot, &input, enter)",
                        0,
                    ),
                    (
                        b"function otherCaller(",
                        "other(&mut slot, &input, enter)",
                        1,
                    ),
                ] {
                    let caller = root(&report, prefix);
                    assert_eq!(
                        caller.completion(),
                        Completion::SourceRejected,
                        "mode={test_mode}, caller={:?}",
                        caller.owner()
                    );
                    assert!(caller.mir().is_none());
                    assert_eq!(caller.ordinary_completion(), Completion::Checked);
                    assert!(matches!(caller.enabled_frame(), Delivery::Unresolved));
                    let Delivery::Rejected {
                        finding_id,
                        via_call: Some(call),
                    } = caller.selected_call()
                    else {
                        panic!("expected a rejection linked to the selected call");
                    };
                    let [edge] = caller.dependencies() else {
                        panic!("expected one ordinary call edge")
                    };
                    assert_eq!(*finding_id, expected_id);
                    assert_eq!(
                        report.roots()[call.callee_root].owner(),
                        BodyOwner::Declaration(edge.declaration)
                    );
                    let block = &caller.ordinary_mir().unwrap().blocks[call.block];
                    assert!(
                        matches!(block.operation, Operation::CallRetainingWithBool { callee, .. } if callee == edge.declaration)
                    );
                    assert_eq!(
                        (call.site.module, call.site.node),
                        (&module("main.rill").key, edge.site.node)
                    );
                    assert_eq!(call.site.node, block.site.node);
                    assert_eq!(
                        module("main.rill").syntax.nodes[call.site.node].kind,
                        compiler::parser::NodeKind::Call
                    );
                    assert_eq!(call.site.span.bytes(), text.as_bytes());
                    assert_eq!(call.site.span.start(), main.find(text).unwrap());
                }
                assert_eq!(
                    root(&report, b"function entryControl(").completion(),
                    Completion::Checked
                );
            }
        },
    );
}

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
            let core = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "core.rill")
                .unwrap();
            scalar::assert_standard_bodies(&report, typed, receipt, core);
            assert_eq!(
                report
                    .roots()
                    .iter()
                    .filter(|root| root.ordinary_completion() == Completion::Checked)
                    .count(),
                53
            );
            assert_eq!(
                standard
                    .iter()
                    .filter(|root| root.completion() == Completion::Incomplete
                        && root.blocked_at().is_some()
                        && root.mir().is_none())
                    .count(),
                120
            );
            for (kind, count) in [(Function, 18), (Implementation, 130)] {
                assert_eq!(standard.iter().filter(|root| matches!(root.owner(), BodyOwner::Declaration(id) if id.kind == kind)).count(), count);
            }
            assert_eq!(standard.iter().filter(|root| root.completion() == Completion::Incomplete
                && matches!(root.owner(), BodyOwner::Declaration(id) if id.kind == Implementation)).count(), 102);

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
                52
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

#[test]
fn installed_work_accounts_for_shared_module_path_prefixes() {
    installed(
        "graph::installed_work_accounts_for_shared_module_path_prefixes",
        |request| {
            let mut short = [AnalysisLimits::default(); 2];
            for width in [1, 96] {
                let prefix = "p".repeat(width);
                let helper = format!("{prefix}a");
                let unused = format!("{prefix}b");
                let main = MAIN.replace(
                    "import helper::ready;",
                    &format!("import {helper}::ready;\nimport {unused}::other;"),
                );
                fs::write(&request.entry, main).unwrap();
                fs::write(
                    request.entry.with_file_name(format!("{helper}.rill")),
                    HELPER,
                )
                .unwrap();
                fs::write(
                    request.entry.with_file_name(format!("{unused}.rill")),
                    "export function other(): bool { return true; }\n",
                )
                .unwrap();
                for (mode, test_mode) in [false, true].into_iter().enumerate() {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    let receipt = typed.receipt().expect("installed typed receipt");
                    let full = analyze_ownership(&receipt, AnalysisLimits::default());
                    assert_eq!(full.completion(), GraphCompletion::Incomplete);
                    assert!(full.inventory_complete());
                    assert_eq!(full.roots().len(), 175);
                    let expectation = root(&full, b"expect ready()");
                    assert_eq!(expectation.completion(), Completion::Checked);
                    let [edge] = expectation.dependencies() else {
                        panic!("missing canonical ready edge");
                    };
                    assert_eq!(edge.declaration.module.path, format!("{helper}.rill"));
                    assert_eq!(
                        root(&full, b"function other(").completion(),
                        Completion::Checked
                    );
                    let stats = full.statistics();
                    if width == 1 {
                        short[mode] = AnalysisLimits {
                            max_work: stats.work,
                            max_entries: stats.entries,
                            max_depth: stats.max_depth,
                        };
                    } else {
                        assert_eq!(stats.entries, short[mode].max_entries);
                        assert_eq!(stats.max_depth, short[mode].max_depth);
                        let same_work = analyze_ownership(&receipt, short[mode]);
                        assert_eq!(
                            same_work.completion(),
                            GraphCompletion::ResourceLimit,
                            "the short path budget ({}) also covered the long shared prefix ({stats:?})",
                            short[mode].max_work,
                        );
                        assert!(same_work.inventory_complete());
                        assert!(same_work.roots().iter().all(|root| root.mir().is_none()));
                    }
                }
            }
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
type PendingText = {{ text: string; }}
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
    let projected = source.replace(
        "let owner = \"end\";\n    dispose(owner);",
        "let owner = PendingText { text: \"end\" };\n    dispose(owner.text);",
    );
    fixture_with_main(
        "graph::aliases_share_a_canonical_body_and_cycles_require_all_edges",
        &main,
        &[&source, &projected],
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
                    assert_eq!(
                        root.blocked_at().unwrap().span.bytes(),
                        b"dispose(owner.text)"
                    );
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
            assert!(right.dependencies().iter().any(|edge| {
                edge.contract == DependencyContract::BuiltinDispose
                    && edge.declaration == &typed.dispose_role().unwrap().id
                    && edge.site.span.bytes()
                        == [b"dispose(owner)".as_slice(), b"dispose(owner.text)"][index]
            }));
        },
    );
}

#[test]
fn nested_and_unused_bodies_are_independent_source_obligations() {
    let source = format!(
        r#"{HELPER}
function outer(): bool {{
    function nested(owner: string): bool {{ let taken = owner; let again = owner; return true; }}
    return true;
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
fn local_blocker_is_not_a_finished_dependency_verdict_on_exhaustion() {
    let source = format!(
        r#"{HELPER}
type PendingText = {{ text: string; }}
function broken(v: string): void {{ let first = v; let again = v; }}
function combined(v: string, other: PendingText): void {{ broken(v); dispose(other.text); }}
function closed(other: string): void {{ dispose(other); }}
"#
    );
    fixture(
        "graph::local_blocker_is_not_a_finished_dependency_verdict_on_exhaustion",
        &[&source],
        |_, _, _, receipt, _| {
            let full = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(
                root(&full, b"function combined(").completion(),
                Completion::SourceRejected
            );
            assert!(root(&full, b"function closed(").ordinary_mir().is_some());
            let short = analyze_ownership(
                receipt,
                AnalysisLimits {
                    max_entries: full.statistics().entries - 1,
                    ..Default::default()
                },
            );
            assert_eq!(short.completion(), GraphCompletion::ResourceLimit);
            let pending = root(&short, b"function combined(");
            assert_eq!(pending.completion(), Completion::ResourceLimit);
            assert_eq!(
                pending.blocked_at().unwrap().span.bytes(),
                b"dispose(other.text)"
            );
            assert!(pending.mir().is_none());
            let [finding] = short.findings() else {
                panic!("expected reached helper's finding")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(
                finding.site.span.start(),
                source.find("again = v").unwrap() + "again = ".len()
            );
            assert_eq!(
                finding.related.span.start(),
                source.find("first = v").unwrap() + "first = ".len()
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
