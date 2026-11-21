use super::*;
use compiler::analysis::QueryError;
use compiler::analysis::ordinary::HypotheticalOutcome;
use compiler::analysis::scalars::{NodeFactKind, generics::GoalState};
use compiler::analysis::typed::{ResultUseBlocker, TypedCompletion};
use compiler::loader::{FailureKind, Lookup, Module, Namespace, Target};
use compiler::parser::NodeKind;

const BASE: &str = include_str!("../../../../tests/installed/typed/main.rill");
const CHECKS: &str = include_str!("../../../../tests/installed/typed/checks.rill");
const KEYS: &str = include_str!("../../../../tests/installed/typed/stdlib-sources.txt");
const SHARED: &str = r#"
import std::routine::join;
function joined(input: List<i32>): [i32, i32] {
    let numbers = input;
    let [first, second] = join([() => (&numbers)[0u64], () => (&numbers)[1u64]]);
    dispose(numbers);
    return [first, second];
}
function joinedOrder(): bool {
    let input: List<i32> = [13i32, 29i32];
    let pair = joined(input);
    return pair[0] == 13i32 && pair[1] == 29i32;
}
expect joinedOrder();
"#;

fn captured_keys(graph: &loader::Graph, extended: bool) {
    let mut expected: Vec<_> = KEYS
        .lines()
        .chain(extended.then_some("routine.rill"))
        .collect();
    expected.sort_unstable();
    assert_eq!(
        graph
            .modules()
            .iter()
            .filter(|m| m.key.origin == Origin::Standard)
            .map(|m| m.key.path.as_str())
            .collect::<Vec<_>>(),
        expected,
    );
}

fn app(graph: &loader::Graph) -> &Module {
    graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
        .unwrap()
}

fn routine(graph: &loader::Graph) -> &Module {
    graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Standard && m.key.path == "routine.rill")
        .unwrap()
}

fn budget_caps(stats: compiler::analysis::Statistics) -> (AnalysisLimits, [AnalysisLimits; 3]) {
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    let short = [
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
    ];
    (exact, short)
}

fn check_typed_budget(graph: &loader::Graph, typed: &TypedReport<'_>) {
    let stats = typed.statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    assert_eq!(analyze_typed(graph, exact).completion(), typed.completion());
    let short = analyze_typed(
        graph,
        AnalysisLimits {
            max_work: exact.max_work - 1,
            ..exact
        },
    );
    assert_eq!(short.completion(), TypedCompletion::ResourceLimit);
    assert!(short.receipt().is_none());
}

fn named(module: &Module, name: &str) -> usize {
    module
        .declarations
        .iter()
        .position(|d| d.name.as_deref() == Some(name))
        .unwrap()
}

fn call_node(module: &Module, prefix: &str) -> usize {
    module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Call && n.span.bytes().starts_with(prefix.as_bytes()))
        .unwrap()
}

#[test]
fn cancellation_only_disposer_is_not_a_checked_join_exit() {
    use compiler::analysis::ownership::analyze_ownership;
    probe(
        "scoped_join::cancellation_only_disposer_is_not_a_checked_join_exit",
        r#"
            import std::routine::join as scoped;
            type Bag = {}
            implement dispose(bag: Bag): void { let values: List<i32> = [1i32]; dispose(values); }
            function delivered(input: List<i32>, bag: Bag): Bag {
                let [a, b] = scoped([() => (&input)[0u64], () => (&input)[1u64]]);
                dispose(input); return bag;
            }
            function plain(input: List<i32>): [i32, i32] {
                return scoped([() => (&input)[0u64], () => (&input)[1u64]]);
            }
        "#,
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            for (name, expected) in [
                ("delivered", Completion::Incomplete),
                ("plain", Completion::Checked),
            ] {
                let d = &module.declarations[named(module, name)];
                let local = lower_body(receipt, module, &d.id, AnalysisLimits::default()).unwrap();
                let root = graph
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == d.node)
                    .unwrap();
                assert_eq!(local.completion(), expected, "{name}");
                assert_eq!(root.completion(), expected);
                assert_eq!(local.mir().is_some(), expected == Completion::Checked);
                assert_eq!(root.mir().is_some(), expected == Completion::Checked);
                assert!(local.findings().is_empty());
                if expected == Completion::Incomplete {
                    assert!(
                        local
                            .blocked_at()
                            .unwrap()
                            .span
                            .bytes()
                            .starts_with(b"scoped([")
                    );
                }
            }
        },
    );
}

#[test]
fn grouped_ordinary_call_keeps_its_cap_with_the_join_group_loaded() {
    probe_report_with_modules(
        "scoped_join::grouped_ordinary_call_keeps_its_cap_with_the_join_group_loaded",
        r#"
            import selectionBudgetAcrossAnUnrelatedModule::identity as plain;
            import std::routine::join as scoped;
            function untouched(n: i32): i32 { return (plain)(((n))); }
            function selected(xs: List<i32>): [i32, i32] { return scoped([() => (&xs)[0u64], () => (&xs)[1u64]]); }
        "#,
        &[(
            "selectionBudgetAcrossAnUnrelatedModule.rill",
            "export function identity(n: i32): i32 { return n; }",
        )],
        |_, module, typed| {
            assert!(typed.routine_role().is_some());
            let receipt = typed.receipt().unwrap();
            let d = &module.declarations[named(module, "untouched")];
            // Measured with the extension in the capture before the exit-scope correction.
            let cap = AnalysisLimits {
                max_work: 4126,
                max_entries: 179,
                max_depth: 7,
            };
            let local = lower_body(&receipt, module, &d.id, cap).unwrap();
            assert!(local.mir().is_some());
            let s = local.statistics();
            assert_eq!(
                (s.work, s.entries, s.max_depth),
                (cap.max_work, cap.max_entries, cap.max_depth)
            );
        },
    );
}

#[test]
fn installed_routine_selects_but_does_not_admit_an_ineligible_operation() {
    installed(
        "scoped_join::installed_routine_selects_but_does_not_admit_an_ineligible_operation",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                fs::write(&request.entry, BASE).unwrap();
                let graph = loader::load_installed(request.clone()).unwrap();
                captured_keys(&graph, false);
                assert_eq!(graph.modules().len(), 19);
                let baseline = analyze_typed(&graph, AnalysisLimits::default());
                assert!(baseline.receipt().is_some());
                assert!(baseline.routine_role().is_none());
                check_typed_budget(&graph, &baseline);

                for (binding, source) in [
                    ("join", SHARED.to_owned()),
                    (
                        "gatherScoped",
                        SHARED
                            .replace("routine::join;", "routine::join as gatherScoped;")
                            .replace("join([", "gatherScoped(["),
                    ),
                ] {
                    // The pinned compiler operation cannot use a missing List
                    // frame, even though this pair has six
                    // proved requirements and the designated ordinary winner.
                    let source = source.replace("() => (&numbers)[1u64]", "() => 29i32");
                    fs::write(&request.entry, format!("{BASE}\n{source}")).unwrap();
                    let graph = loader::load_installed(request.clone()).unwrap();
                    loader::validate_inputs(&graph, request.limits).unwrap();
                    captured_keys(&graph, true);
                    let routine = routine(&graph);
                    assert_eq!(
                        routine.path,
                        graph.request().standard_root.join("routine.rill")
                    );
                    let Lookup::Found(Target::Declaration(family)) =
                        graph.lookup_export(&routine.key, Namespace::ValueFunction, &["join"])
                    else {
                        panic!("missing canonical family")
                    };
                    let mut registrations = graph
                        .implementations()
                        .iter()
                        .filter(|i| i.family == family);
                    let provider = &registrations.next().unwrap().declaration;
                    assert!(registrations.next().is_none());
                    assert_eq!(provider.module, routine.key);
                    let module = app(&graph);
                    let node = call_node(module, &format!("{binding}(["));
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    let ordinary = typed
                        .bodies()
                        .ordinary_call(module, node)
                        .unwrap()
                        .expect("no ordinary comparison");
                    assert!(
                        ordinary.checked,
                        "selection not checked: {:?}",
                        ordinary.blocker
                    );
                    assert!(ordinary.premise.is_none());
                    let role = typed.routine_role().expect("no pinned routine role");
                    assert_eq!(&role.family.id, &family);
                    assert_eq!(&role.implementation.id, provider);
                    assert_eq!(role.bridge.id.module, routine.key);
                    let selected = typed
                        .bodies()
                        .goal(ordinary.goal.as_ref().unwrap())
                        .unwrap();
                    assert_eq!(selected.state, GoalState::Proved);
                    assert!(selected.premise.is_none());
                    assert_eq!(
                        selected.selection.as_ref().unwrap().implementation,
                        provider
                    );
                    let fact = typed.bodies().node(module, node).unwrap().unwrap();
                    let NodeFactKind::Expression(expression) = &fact.kind else {
                        panic!("not an expression")
                    };
                    let call = expression.call.as_ref().unwrap();
                    assert_eq!(call.callee, &family);
                    assert_eq!(call.selection.as_ref().unwrap().implementation, provider);
                    assert_eq!(typed.completion(), TypedCompletion::Incomplete);
                    assert!(
                        typed
                            .result_use()
                            .unwrap()
                            .pending()
                            .iter()
                            .any(|p| p.site.module == &module.key
                                && p.site.node == node
                                && p.blocker == ResultUseBlocker::CallBoundary)
                    );
                    assert!(typed.receipt().is_none());

                    let foreign = loader::load_installed(request.clone()).unwrap();
                    assert!(matches!(
                        typed.bodies().ordinary_call(app(&foreign), node),
                        Err(QueryError::InvalidRequest)
                    ));
                    let foreign = analyze_typed(&foreign, AnalysisLimits::default());
                    assert!(matches!(
                        foreign.bodies().goal(ordinary.goal.as_ref().unwrap()),
                        Err(QueryError::InvalidRequest)
                    ));

                    check_typed_budget(&graph, &typed);

                    let mut exact = request.clone();
                    exact.limits.max_modules = graph.modules().len();
                    assert!(loader::load_installed(exact.clone()).is_ok());
                    exact.limits.max_modules -= 1;
                    assert_eq!(
                        loader::load_installed(exact).unwrap_err().kind,
                        FailureKind::ResourceLimit
                    );
                }
            }
        },
    );
}

#[test]
fn installed_routine_pin_and_origin_are_independent_of_spelling() {
    installed(
        "scoped_join::installed_routine_pin_and_origin_are_independent_of_spelling",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                fs::write(&request.entry, format!("{BASE}\n{SHARED}")).unwrap();
                let graph = loader::load_installed(request.clone()).unwrap();
                let source = loader::load(graph.request().clone()).unwrap();
                assert_eq!(
                    routine(&source).snapshot.bytes(),
                    routine(&graph).snapshot.bytes()
                );
                let report = analyze_typed(&source, AnalysisLimits::default());
                assert!(report.routine_role().is_none() && report.receipt().is_none());
                for unsupported in [
                    InstalledRequest {
                        target: "x86_64-apple-darwin".into(),
                        ..request.clone()
                    },
                    InstalledRequest {
                        options: [("unknown".into(), "true".into())].into(),
                        ..request.clone()
                    },
                ] {
                    let graph = loader::load_installed(unsupported).unwrap();
                    let report = analyze_typed(&graph, AnalysisLimits::default());
                    assert!(report.routine_role().is_none() && report.receipt().is_none());
                }
                let pin = graph.request().standard_root.join("routine.rill");
                let authentic = fs::read(&pin).unwrap();
                fs::write(&pin, b"// stable mismatched pin\n").unwrap();
                assert_eq!(
                    loader::load_installed(request.clone()).unwrap_err().kind,
                    FailureKind::InvalidRequest
                );
                fs::write(&request.entry, BASE).unwrap();
                let graph = loader::load_installed(request.clone()).unwrap();
                captured_keys(&graph, false);
                assert!(
                    analyze_typed(&graph, AnalysisLimits::default())
                        .receipt()
                        .is_some()
                );
                fs::remove_file(&pin).unwrap();
                fs::write(&request.entry, format!("{BASE}\n{SHARED}")).unwrap();
                let error = loader::load_installed(request.clone()).unwrap_err();
                assert_eq!(error.kind, FailureKind::SourceRejected);
                assert!(error.findings.iter().any(|f| f.code == Some("R0400")));
                fs::write(&pin, authentic).unwrap();

                fs::write(
                    request.entry.with_file_name("routine.rill"),
                    r#"
                export function join<W, R>(items: W): R;
                implement<F, G> join(items: [F, G]): [F, G] { return items; }
            "#,
                )
                .unwrap();
                fs::write(&request.entry, format!("{BASE}\nimport routine::join;\nfunction ordinaryJobs(): void {{ let _ = join([() => 13i32, () => 29i32]); }}")).unwrap();
                let graph = loader::load_installed(request).unwrap();
                captured_keys(&graph, false);
                let report = analyze_typed(&graph, AnalysisLimits::default());
                assert!(report.routine_role().is_none() && report.receipt().is_some());
                let module = app(&graph);
                let node = call_node(module, "join([");
                assert!(
                    report
                        .receipt()
                        .unwrap()
                        .scoped_join(module, node)
                        .unwrap()
                        .is_none()
                );
                let fact = report.bodies().node(module, node).unwrap().unwrap();
                let NodeFactKind::Expression(expression) = &fact.kind else {
                    panic!("not an expression")
                };
                let call = expression.call.as_ref().unwrap();
                assert_eq!(call.callee.module.origin, Origin::Application);
                assert_eq!(
                    call.selection
                        .as_ref()
                        .unwrap()
                        .implementation
                        .module
                        .origin,
                    Origin::Application
                );
            }
        },
    );
}

#[test]
fn application_winner_in_the_join_family_is_an_ordinary_contract() {
    probe_report(
        "scoped_join::application_winner_in_the_join_family_is_an_ordinary_contract",
        r#"
        import std::routine::join;
        type Marker = { flag: bool; }
        implement join<[Marker,i32], [i32,i32]>(work:[Marker,i32]): [i32,i32] { return [4i32,7i32]; }
        function ordinary(m: Marker): [i32,i32] { return join([m,2i32]); }
        function store(): void { let held: ([Marker,i32]) => [i32,i32] = join; let _ = held; }
    "#,
        |_, module, typed| {
            let receipt = typed
                .receipt()
                .expect("the application winner has no child-frame obligations");
            let node = call_node(module, "join([m,");
            let ordinary = typed.bodies().ordinary_call(module, node).unwrap().unwrap();
            let goal = typed
                .bodies()
                .goal(ordinary.goal.as_ref().unwrap())
                .unwrap();
            let winner = goal.selection.as_ref().unwrap().implementation;
            assert!(ordinary.checked && goal.premise.is_none());
            assert_eq!(winner.module.origin, Origin::Application);
            assert_ne!(winner, &typed.routine_role().unwrap().implementation.id);
            assert!(receipt.scoped_join(module, node).unwrap().is_none());
            let selected = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .find_map(|(node, n)| {
                    (n.span.bytes() == b"join")
                        .then(|| typed.bodies().ordinary_value(module, node).unwrap())
                        .flatten()
                })
                .unwrap();
            assert_eq!(
                typed
                    .bodies()
                    .goal(&selected.goal)
                    .unwrap()
                    .selection
                    .as_ref()
                    .unwrap()
                    .implementation,
                winner
            );
        },
    );
}

#[test]
fn installed_routine_does_not_retry_after_the_contenders_requirement() {
    installed(
        "scoped_join::installed_routine_does_not_retry_after_the_contenders_requirement",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            fs::write(
                &request.entry,
                format!(
                    "{BASE}\n{}",
                    r#"
            import std::routine::join;
            type JobMarker = { marker: bool; }
            implement<F> join<[JobMarker, F], [i32, i32]>(work: [JobMarker, F]): [i32, i32]
            where callableOnce(JobMarker, [], i32) { return [0i32, 0i32]; }
            function contested(marker: JobMarker): [i32, i32] {
                return join([marker, () => 1i32]);
            }
        "#
                ),
            )
            .unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let report = analyze_typed(&graph, AnalysisLimits::default());
                let role = report.routine_role().unwrap();
                let module = app(&graph);
                let node = call_node(module, "join([");
                let ordinary = report
                    .bodies()
                    .ordinary_call(module, node)
                    .unwrap()
                    .unwrap();
                assert_eq!(ordinary.candidates.len(), 2, "{:?}", ordinary.blocker);
                assert!(
                    ordinary
                        .candidates
                        .iter()
                        .any(|c| c.implementation == &role.implementation.id)
                );
                let Some(HypotheticalOutcome::Winner(winner)) = ordinary.outcome else {
                    panic!("no ordinary winner")
                };
                assert_eq!(winner.module.origin, Origin::Application);
                let goal = report
                    .bodies()
                    .goal(ordinary.goal.as_ref().unwrap())
                    .unwrap();
                assert!(goal.premise.is_none());
                assert_eq!(goal.selection.as_ref().unwrap().implementation, winner);
                assert_eq!(goal.state, GoalState::Disproved);
                assert!(!ordinary.checked);
                assert!(report.receipt().is_none());
            }
        },
    );
}

fn prove_join<'a, 'g>(
    mir: &'a Mir<'g>,
    node: usize,
    ids: [&loader::DeclarationId; 3],
    reads: [(u64, &str); 2],
) -> &'a compiler::analysis::ownership::ScopedJoinProof<'g> {
    use compiler::analysis::ownership::{JoinChildState::*, JoinEvent::*, JoinNext};
    let path = straight_blocks(mir, mir.entry);
    let at = path
        .iter()
        .position(|b| b.site.node == node && matches!(b.operation, Operation::ScopedJoin(_)))
        .unwrap();
    let block = path[at];
    let Operation::ScopedJoin(join) = &block.operation else {
        unreachable!()
    };
    assert_eq!([join.family, join.implementation, join.bridge], ids);
    let roots = join.frames.each_ref().map(|frame| frame.backing.root.node);
    assert_eq!(
        join.frames.each_ref().map(|f| f.backing.root.span.bytes()),
        reads.map(|(_, backing)| backing.as_bytes()),
    );
    assert_eq!(roots[0] == roots[1], reads[0].1 == reads[1].1);
    assert_ne!(
        join.frames[0].environment.root.node,
        join.frames[1].environment.root.node
    );
    let formed: Vec<_> = path[..at]
        .iter()
        .filter_map(|b| match b.operation {
            Operation::FormClosure { closure, .. } => Some(closure.node),
            _ => None,
        })
        .collect();
    assert_eq!(
        formed,
        join.frames.each_ref().map(|frame| frame.origin.node)
    );
    for (i, frame) in join.frames.iter().enumerate() {
        let Operation::ReadListI32 {
            index,
            view,
            backing,
            exclusive,
            ..
        } = frame.body.blocks[frame.body.entry].operation
        else {
            panic!("expected a checked List read, not a copied descriptor")
        };
        assert_eq!((index, exclusive), (reads[i].0, frame.exclusive));
        assert_eq!(view.root.node, frame.capture.root.node);
        assert_eq!(
            backing.root.node,
            frame.incoming_list_backing.unwrap().root.node
        );
        assert_eq!(frame.result.tuple_element(), Some(i));
        if exclusive {
            let [acquire, form] = path[..at].windows(2).find(|pair| {
                matches!(pair[1].operation, Operation::FormClosure { closure, .. } if closure.node == frame.origin.node)
            }).unwrap() else { unreachable!() };
            assert!(
                matches!(acquire.operation, Operation::Borrow { source, holder, exclusive: true }
                if source.root.node == frame.backing.root.node && holder.root.node == frame.capture.root.node)
            );
            assert!(
                matches!(form.operation, Operation::FormClosure { capture, mode: compiler::analysis::InvocationMode::Mutable, .. }
                if capture.node == frame.capture.root.node)
            );
            let child_path = straight_blocks(&frame.body, frame.body.entry);
            let [_, exported, cleaned, returned] = child_path.as_slice() else {
                panic!("exact read, export, cleanup, and return")
            };
            assert!(matches!(exported.operation, Operation::Export(_)));
            assert!(
                matches!(&cleaned.operation, Operation::Cleanup(actions) if matches!(actions.as_slice(),
                [Obligation::EndStorage(place)] if place.root.node == frame.capture.root.node))
            );
            assert!(matches!(returned.terminator, Terminator::Return));
        }
    }
    for wait in &join.waiting {
        for (i, frame) in join.frames.iter().enumerate() {
            let held = matches!(wait.children[i], Entry | Running | Cleaning);
            assert_eq!(
                wait.environment_backings[i].map(|p| p.root.node),
                held.then_some(roots[i])
            );
            assert_eq!(
                wait.exclusive_backings[i].map(|p| p.root.node),
                (held && frame.exclusive).then_some(roots[i])
            );
        }
    }

    let step = |state: usize, event| {
        join.waiting[state]
            .successors
            .iter()
            .find(|e| e.0 == event)
            .unwrap()
            .1
    };
    let waiting = |next| match next {
        JoinNext::Waiting(id) => id,
        _ => panic!("released before both child exits"),
    };
    let entry = |enabled, pending, yielded| {
        join.entries
            .iter()
            .find(|e| {
                (e.delivery.enabled, e.delivery.pending, e.yielded) == (enabled, pending, yielded)
            })
            .unwrap()
            .next
    };
    assert!(matches!(
        entry(true, true, true),
        JoinNext::CancelBeforeCommit
    ));
    let staged = &join.before_commit;
    assert_eq!(staged.len(), 2);
    for frame in &join.frames {
        assert_eq!(staged.iter().filter(|a| matches!(a, Obligation::EndStorage(p) if p.root.node == frame.environment.root.node)).count(), 1);
    }
    let initial = waiting(entry(true, false, false));
    assert_eq!(join.waiting[initial].children, [Entry, Entry]);
    let progress = |state, slot| {
        let state = waiting(step(
            state,
            EnterChild {
                slot,
                yielded: true,
            },
        ));
        let state = waiting(step(state, StageResult(slot)));
        assert_eq!(join.waiting[state].children[slot], Cleaning);
        let state = waiting(step(state, EndChildView(slot)));
        assert_eq!(join.waiting[state].children[slot], AtNormalReturn);
        state
    };
    for [first, second] in [[0, 1], [1, 0]] {
        let first_at_exit = progress(initial, first);
        let one = waiting(step(first_at_exit, FinishChild(first)));
        assert_eq!(join.waiting[one].children[first], Complete);
        assert!(join.waiting[one].initialized_results[first]);
        assert!(!join.waiting[one].initialized_results[second]);
        let ready = progress(one, second);
        let JoinNext::Publish {
            transfers,
            delivery,
            retained_results: [None, None],
        } = step(ready, FinishChild(second))
        else {
            panic!("no ordered publication")
        };
        assert_eq!(
            transfers.map(|(_, dest)| dest.tuple_element()),
            [Some(0), Some(1)]
        );
        for (i, (export, _)) in transfers.iter().enumerate() {
            assert!(join.frames[i].body.blocks.iter().any(|b| matches!(b.operation, Operation::Export(place) if place.root.node == export.root.node)));
        }
        assert!(delivery.enabled && !delivery.pending);

        let requested = waiting(step(one, RequestStop));
        let stopped = waiting(step(requested, YieldWhileWaiting));
        let info = &join.waiting[stopped];
        assert!(info.deferred && !info.delivery.pending);
        assert_eq!(info.stop_requested, [0 == second, 1 == second]);
        assert_eq!(info.retained_backings.map(|p| p.root.node), roots);
        let cleaning = waiting(step(
            stopped,
            EnterChild {
                slot: second,
                yielded: true,
            },
        ));
        assert_eq!(join.waiting[cleaning].children[second], Cleaning);
        let pending = waiting(step(cleaning, RequestStop));
        let resumed = waiting(step(pending, YieldWhileWaiting));
        assert_eq!(resumed, pending);
        assert!(join.waiting[resumed].delivery.pending && join.waiting[resumed].deferred);
        assert_eq!(
            join.waiting[resumed].retained_backings.map(|p| p.root.node),
            roots
        );
        let at_exit = waiting(step(resumed, EndChildView(second)));
        assert_eq!(join.waiting[at_exit].children[second], AtDeliveredExit);
        let JoinNext::Cancel { retired } = step(at_exit, FinishChild(second)) else {
            panic!("no terminal exit")
        };
        assert_eq!(retired.map(|p| p.is_some()), [0 == first, 1 == first]);
        assert!(
            join.frames[first]
                .body
                .blocks
                .iter()
                .any(|block| matches!(block.operation, Operation::Export(place)
                if place.root.node == retired[first].unwrap().root.node))
        );
    }
    let inherited = waiting(entry(false, true, true));
    assert_eq!(waiting(step(inherited, YieldWhileWaiting)), inherited);
    let first = waiting(step(progress(inherited, 1), FinishChild(1)));
    let JoinNext::Publish { delivery, .. } = step(progress(first, 0), FinishChild(0)) else {
        panic!("masked join abandoned work")
    };
    assert!(!delivery.enabled && delivery.pending);

    for root in roots {
        assert_eq!(join.parent_exit.cleanup.iter().filter(|a|
            matches!(a, Obligation::DisposeListI32 { operand, .. } if operand.root.node == root)
        ).count(), 1);
    }
    assert!(join.parent_exit.cleanup.iter().all(|a|
        !matches!(a, Obligation::EndStorage(p) if join.frames.iter().any(|f| f.environment.root.node == p.root.node))
    ));
    let Terminator::Goto(normal) = block.terminator else {
        panic!("missing ownership-checked continuation")
    };
    let cleanups = path_drops(mir, normal);
    for root in roots {
        assert_eq!(cleanups.iter().filter(|a|
            matches!(a, Obligation::DisposeListI32 { operand, .. } if operand.root.node == root)
        ).count(), 1);
    }
    join
}

#[test]
fn installed_scoped_join_checks_actual_child_frames_and_caller_successors() {
    use compiler::analysis::Stage;
    use compiler::analysis::TypeTerm;
    use compiler::analysis::generics::{GoalJudgment, callables::CaptureForm};
    use compiler::analysis::ordinary::RequirementTarget;
    use compiler::analysis::ownership::{DependencyContract, GraphObligation, analyze_ownership};
    installed(
        "scoped_join::installed_scoped_join_checks_actual_child_frames_and_caller_successors",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            for test_mode in [false, true] {
                for name in ["join", "gatherScoped"] {
                    let request = InstalledRequest {
                        test_mode,
                        ..request.clone()
                    };
                    let source = if name == "join" {
                        SHARED.to_owned()
                    } else {
                        SHARED
                            .replace("routine::join;", "routine::join as gatherScoped;")
                            .replace("join([", "gatherScoped([")
                    };
                    fs::write(&request.entry, format!("{BASE}\n{source}")).unwrap();
                    let graph = loader::load_installed(request.clone()).unwrap();
                    let module = app(&graph);
                    let caller = &module.declarations[named(module, "joined")];
                    let node = call_node(module, &format!("{name}(["));
                    let limits = AnalysisLimits::default();
                    let typed = analyze_typed(&graph, limits);
                    assert_eq!(
                        typed.completion(),
                        TypedCompletion::Checked,
                        "{:?}",
                        typed.core_blockers()
                    );
                    let receipt = typed.receipt().unwrap();
                    assert_eq!(
                        receipt.not_performed(),
                        &[Stage::Ownership, Stage::Execution]
                    );
                    use compiler::analysis::scalars::BodyOwner;
                    use compiler::loader::DeclarationKind;
                    for loaded in graph.modules() {
                        assert!(
                            typed
                                .bodies()
                                .body(loaded, BodyOwner::Startup)
                                .unwrap()
                                .is_some()
                        );
                        for expectation in loaded
                            .declarations
                            .iter()
                            .filter(|d| d.id.kind == DeclarationKind::Expectation)
                        {
                            assert!(
                                typed
                                    .bodies()
                                    .body(loaded, BodyOwner::Declaration(&expectation.id))
                                    .unwrap()
                                    .is_some()
                            );
                        }
                    }
                    let proof = receipt.scoped_join(module, node).unwrap().unwrap();
                    assert!(
                        typed
                            .bodies()
                            .body(
                                routine(&graph),
                                BodyOwner::Declaration(&proof.role.implementation.id)
                            )
                            .unwrap()
                            .is_some()
                    );
                    let ids = [
                        &proof.role.family.id,
                        &proof.role.implementation.id,
                        &proof.role.bridge.id,
                    ];
                    assert_eq!(ids, {
                        let role = typed.routine_role().unwrap();
                        [&role.family.id, &role.implementation.id, &role.bridge.id]
                    });
                    let family = typed.bodies().ordinary_call(module, node).unwrap().unwrap();
                    let selected = typed
                        .bodies()
                        .goal(family.goal.as_ref().unwrap())
                        .unwrap()
                        .selection
                        .as_ref()
                        .unwrap();
                    assert_eq!(
                        (selected.implementation, selected.alternative),
                        (ids[1], proof.alternative)
                    );
                    let check = |id, name| {
                        let goal = typed.bodies().goal(id).unwrap();
                        let intrinsic = graph
                            .prelude()
                            .intrinsics
                            .iter()
                            .find(|id| id.name() == name)
                            .unwrap();
                        assert_eq!(goal.state, GoalState::Proved);
                        assert!(goal.premise.is_none());
                        assert!(
                            matches!(&goal.judgment, GoalJudgment::Requirement {
                        target: RequirementTarget::Predicate(id), ..
                    } if *id == intrinsic),
                            "expected {name}: {:?}",
                            goal.judgment
                        );
                        goal
                    };
                    for (i, slot) in proof.slots.iter().enumerate() {
                        assert_eq!(slot.path, [i]);
                        let arrow = slot.arrow();
                        assert!(arrow.premise.is_none() && arrow.parameters.is_empty());
                        assert!(
                            matches!(arrow.result.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
                        );
                        assert_eq!(selected.substitution[2 + i].argument, arrow.result);
                        let [capture] = arrow.captures.as_slice() else {
                            panic!("one checked capture")
                        };
                        assert_eq!(capture.form, CaptureForm::ExplicitBorrow { mutable: false });
                        let storage = capture.storage.as_ref().unwrap();
                        let TypeTerm::Reference {
                            mutable: false,
                            referent,
                        } = storage.as_ref()
                        else {
                            panic!("shared storage")
                        };
                        let TypeTerm::Nominal {
                            declaration,
                            arguments,
                        } = referent.as_ref()
                        else {
                            panic!("List referent")
                        };
                        assert_eq!(*declaration, &typed.list_role().unwrap().declaration.id);
                        let invoke = check(&slot.callable_once, "callableOnce");
                        assert!(
                            matches!(&invoke.judgment, GoalJudgment::Requirement { arguments, .. }
                        if matches!(arguments.as_slice(), [job, input, result] if job == &selected.substitution[i].argument
                            && matches!(input.as_ref(), TypeTerm::Tuple(elements) if elements.is_empty())
                            && result == &arrow.result))
                        );
                        let output = check(&slot.send_result, "sendable");
                        assert!(output.arrow_origin.is_none());
                        let mut job = check(&slot.send_job, "sendable");
                        assert!(std::ptr::eq(job.arrow_origin.as_deref().unwrap(), arrow));
                        let mut previous = None;
                        for (name, term) in [
                            ("sendable", storage),
                            ("shareable", referent),
                            ("shareable", &arguments[0]),
                        ] {
                            // Slot-local evidence may reference the cached
                            // proof of an equal structural property downstream.
                            if let Some((predicate, subject)) = previous {
                                let [child] = job.dependencies.as_slice() else {
                                    panic!("missing property")
                                };
                                let next = typed.bodies().goal(child).unwrap();
                                if matches!(&next.judgment, GoalJudgment::Requirement {
                                    target: RequirementTarget::Predicate(id), arguments,
                                } if id.name() == predicate && arguments == std::slice::from_ref(subject))
                                {
                                    job = check(child, predicate);
                                }
                            }
                            let [child] = job.dependencies.as_slice() else {
                                panic!("missing capture sendability proof")
                            };
                            job = check(child, name);
                            assert!(
                                matches!(&job.judgment, GoalJudgment::Requirement { arguments, .. }
                            if arguments == std::slice::from_ref(term))
                            );
                            if name == "sendable" {
                                assert_eq!(job.source.node, capture.site.node);
                            }
                            previous = Some((name, term));
                        }
                    }
                    assert_eq!(
                        proof.slots[0].arrow().captures[0].path.root,
                        proof.slots[1].arrow().captures[0].path.root
                    );
                    assert_ne!(
                        proof.slots[0].arrow().site.node,
                        proof.slots[1].arrow().site.node
                    );

                    let local = lower_body(&receipt, module, &caller.id, limits).unwrap();
                    assert_eq!(
                        local.completion(),
                        Completion::Checked,
                        "{:?}",
                        local
                            .findings()
                            .iter()
                            .map(|f| f.code())
                            .collect::<Vec<_>>()
                    );
                    prove_join(
                        local.mir().unwrap(),
                        node,
                        ids,
                        [(0, "numbers"), (1, "numbers")],
                    );
                    let owned = analyze_ownership(&receipt, limits);
                    assert_eq!(owned.context(), graph.id());
                    assert_eq!(owned.test_mode(), test_mode);
                    assert!(
                        owned
                            .open_obligations()
                            .contains(&GraphObligation::MaskingAndJoin)
                    );
                    let root = owned
                        .roots()
                        .iter()
                        .find(|r| r.site().module == &module.key && r.site().node == caller.node)
                        .unwrap();
                    assert_eq!(root.completion(), Completion::Checked);
                    prove_join(
                        root.mir().unwrap(),
                        node,
                        ids,
                        [(0, "numbers"), (1, "numbers")],
                    );
                    assert!(root.dependencies().iter().any(|e| e.declaration == ids[1]
                    && matches!(e.contract, DependencyContract::ScopedJoin { bridge, .. } if bridge == ids[2])));
                    let stats = local.statistics();
                    let exact = AnalysisLimits {
                        max_work: stats.work,
                        max_entries: stats.entries,
                        max_depth: stats.max_depth,
                    };
                    assert!(
                        lower_body(&receipt, module, &caller.id, exact)
                            .unwrap()
                            .mir()
                            .is_some()
                    );
                    let short = lower_body(
                        &receipt,
                        module,
                        &caller.id,
                        AnalysisLimits {
                            max_work: exact.max_work - 1,
                            ..exact
                        },
                    )
                    .unwrap();
                    assert_eq!(short.completion(), Completion::ResourceLimit);
                    assert!(short.mir().is_none());
                    check_typed_budget(&graph, &typed);

                    let foreign = loader::load_installed(request).unwrap();
                    let other = app(&foreign);
                    assert!(matches!(
                        receipt.scoped_join(other, node),
                        Err(QueryError::InvalidRequest)
                    ));
                    assert!(matches!(
                        lower_body(
                            &receipt,
                            module,
                            &other.declarations[named(other, "joined")].id,
                            limits
                        ),
                        Err(QueryError::InvalidRequest)
                    ));
                }
            }
        },
    );
}

#[test]
fn installed_join_publication_interns_the_actual_caller_state() {
    use compiler::analysis::ownership::{JoinNext, ScopedJoinProof, analyze_ownership};
    installed(
        "scoped_join::installed_join_publication_interns_the_actual_caller_state",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            fs::write(
                &request.entry,
                format!(
                    "{BASE}\n{}",
                    r#"
            import std::routine::join;
            function split(numbers: List<i32>, other: List<i32>, dropIt: bool): [i32, i32] {
                if (dropIt) { dispose(other); }
                let [first, middle] = join([() => (&numbers)[0u64], () => (&numbers)[1u64]]);
                let [next, last] = join([() => (&numbers)[1u64], () => (&numbers)[0u64]]);
                dispose(numbers);
                return [first, last];
            }
        "#
                ),
            )
            .unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let module = app(&graph);
                let caller = &module.declarations[named(module, "split")];
                let limits = AnalysisLimits {
                    max_entries: 300_000,
                    ..AnalysisLimits::default()
                };
                let typed = analyze_typed(&graph, limits);
                assert_eq!(typed.completion(), TypedCompletion::Checked);
                let receipt = typed.receipt().unwrap();
                let first = call_node(module, "join([() => (&numbers)[0u64]");
                let second = call_node(module, "join([() => (&numbers)[1u64]");
                let local = lower_body(&receipt, module, &caller.id, limits).unwrap();
                assert_eq!(local.completion(), Completion::Checked);
                let owned = analyze_ownership(&receipt, limits);
                let root = owned
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == caller.node)
                    .unwrap();
                assert_eq!(root.completion(), Completion::Checked);
                for mir in [local.mir().unwrap(), root.mir().unwrap()] {
                    let mut next_blocks = std::collections::BTreeSet::new();
                    let mut other_live = std::collections::BTreeSet::new();
                    for block in mir.blocks.iter().filter(|block| block.site.node == first) {
                        let Operation::ScopedJoin(join) = &block.operation else {
                            continue;
                        };
                        let Terminator::Goto(normal) = block.terminator else {
                            panic!("no normal edge")
                        };
                        assert!(next_blocks.insert(normal));
                        let drops_other = |join: &ScopedJoinProof<'_>| {
                            join.parent_exit.cleanup.iter().any(|a|
                        matches!(a, Obligation::DisposeListI32 { operand, .. } if operand.root.span.bytes().starts_with(b"other"))
                    )
                        };
                        assert!(other_live.insert(drops_other(join)));
                        let later = straight_blocks(mir, normal)
                            .into_iter()
                            .find(|b| {
                                b.site.node == second
                                    && matches!(b.operation, Operation::ScopedJoin(_))
                            })
                            .unwrap();
                        let Operation::ScopedJoin(later) = &later.operation else {
                            unreachable!()
                        };
                        let pending = join
                            .waiting
                            .iter()
                            .flat_map(|w| &w.successors)
                            .find_map(|(_, next)| match next {
                                JoinNext::Publish { delivery, .. }
                                    if delivery.enabled && delivery.pending =>
                                {
                                    Some(*delivery)
                                }
                                _ => None,
                            })
                            .expect("ordinary publication can carry a request without yielding");
                        assert!(later.entries.iter().any(|entry| entry.delivery == pending
                            && entry.yielded
                            && matches!(entry.next, JoinNext::CancelBeforeCommit)));
                        assert_eq!(
                            drops_other(later),
                            drops_other(join),
                            "lost caller state at publication"
                        );
                    }
                    assert_eq!(next_blocks.len(), 2);
                    assert_eq!(other_live.len(), 2);
                }
            }
        },
    );
}

#[test]
fn installed_join_rejects_the_second_overlapping_capture() {
    use compiler::analysis::generics::callables::CaptureForm;
    use compiler::analysis::ownership::{GraphCompletion, analyze_ownership};
    use compiler::analysis::scalars::{
        Place,
        data::{Access, DataExpression},
    };
    use compiler::analysis::{InvocationMode, TypeTerm};

    installed(
        "scoped_join::installed_join_rejects_the_second_overlapping_capture",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            let source = SHARED
                .replace("let numbers = input;", "let mut numbers = input;")
                .replace("(&numbers)[1u64]", "(&mut numbers)[1u64]");
            fs::write(&request.entry, format!("{BASE}\n{source}")).unwrap();
            for test_mode in [false, true] {
                let graph = loader::load_installed(InstalledRequest {
                    test_mode,
                    ..request.clone()
                })
                .unwrap();
                let module = app(&graph);
                let caller = &module.declarations[named(module, "joined")];
                let node = call_node(module, "join([");
                let limits = AnalysisLimits::default();
                let typed = analyze_typed(&graph, limits);
                assert_eq!(typed.completion(), TypedCompletion::Checked);
                assert!(typed.result_use().unwrap().pending().is_empty());
                let receipt = typed.receipt().unwrap();
                let transfer = receipt.scoped_join(module, node).unwrap().unwrap();
                let [first, second] = &transfer.slots;
                for (i, slot) in transfer.slots.iter().enumerate() {
                    let mutable = i == 1;
                    assert_eq!(slot.path, [i]);
                    let arrow = slot.arrow();
                    assert_eq!(
                        arrow.mode,
                        Some(if mutable {
                            InvocationMode::Mutable
                        } else {
                            InvocationMode::ReadOnly
                        })
                    );
                    let [capture] = arrow.captures.as_slice() else {
                        panic!("one capture")
                    };
                    assert_eq!(capture.form, CaptureForm::ExplicitBorrow { mutable });
                    assert!(
                        matches!(capture.storage.as_deref(), Some(TypeTerm::Reference { mutable: m, referent }) if *m == mutable
                        && matches!(referent.as_ref(), TypeTerm::Nominal { declaration, .. }
                            if *declaration == &typed.list_role().unwrap().declaration.id))
                    );
                    let job = typed.bodies().goal(&slot.send_job).unwrap();
                    assert_eq!(job.state, GoalState::Proved);
                    assert!(job.premise.is_none());
                    assert!(std::ptr::eq(job.arrow_origin.as_deref().unwrap(), arrow));
                    let body = module.syntax.nodes[arrow.site.node].children[1];
                    let read = typed.bodies().node(module, body).unwrap().unwrap();
                    let NodeFactKind::Expression(value) = &read.kind else {
                        panic!("no read")
                    };
                    assert!(
                        matches!(value.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
                    );
                    let Some(DataExpression::ListIndex {
                        declaration,
                        base,
                        selector,
                    }) = value.data
                    else {
                        panic!("missing canonical List index")
                    };
                    assert_eq!(declaration, &typed.list_role().unwrap().declaration.id);
                    assert_eq!(
                        module.syntax.nodes[base].children.as_slice(),
                        [capture.site.node]
                    );
                    assert_eq!(
                        module.syntax.nodes[selector].span.bytes(),
                        if mutable { b"1u64" } else { b"0u64" }
                    );
                    assert_eq!(
                        (value.access, value.place),
                        if mutable {
                            (Access::Writable, Place::Writable)
                        } else {
                            (Access::ReadOnly, Place::ReadOnly)
                        }
                    );
                }
                let [previous, attempted] = [first, second].map(|slot| &slot.arrow().captures[0]);
                assert_eq!(attempted.path.root, previous.path.root);
                assert_eq!(attempted.site.span.bytes(), b"&mut numbers");
                assert_eq!(previous.site.span.bytes(), b"&numbers");

                let local = lower_body(&receipt, module, &caller.id, limits).unwrap();
                assert_eq!(local.completion(), Completion::SourceRejected);
                assert!(local.mir().is_none());
                let [finding] = local.findings() else {
                    panic!("one acquisition conflict")
                };
                assert_eq!(finding.code(), "R0201");
                assert_eq!(finding.site.span, attempted.site.span);
                assert_eq!(finding.related.span, previous.site.span);

                let owned = analyze_ownership(&receipt, limits);
                assert_eq!(owned.completion(), GraphCompletion::SourceRejected);
                let root = owned
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == caller.node)
                    .unwrap();
                assert_eq!(root.completion(), Completion::SourceRejected);
                assert_eq!(root.ordinary_completion(), Completion::SourceRejected);
                assert!(root.mir().is_none() && root.ordinary_mir().is_none());
                let [recorded] = owned.findings() else {
                    panic!("one graph acquisition conflict")
                };
                assert_eq!(recorded.code(), "R0201");
                assert_eq!(recorded.site.span, finding.site.span);
                assert_eq!(recorded.related.span, finding.related.span);
                check_typed_budget(&graph, &typed);
                let stats = local.statistics();
                let exact = AnalysisLimits {
                    max_work: stats.work,
                    max_entries: stats.entries,
                    max_depth: stats.max_depth,
                };
                let replay = lower_body(&receipt, module, &caller.id, exact).unwrap();
                assert_eq!(replay.completion(), Completion::SourceRejected);
                assert_eq!(replay.findings()[0].site.span, attempted.site.span);
                assert!(replay.mir().is_none());
                let short = lower_body(
                    &receipt,
                    module,
                    &caller.id,
                    AnalysisLimits {
                        max_work: exact.max_work - 1,
                        ..exact
                    },
                )
                .unwrap();
                assert_eq!(short.completion(), Completion::ResourceLimit);
                assert!(short.mir().is_none());
            }
        },
    );
}

#[test]
fn installed_join_transfers_disjoint_exclusive_list_after_last_use() {
    use compiler::analysis::generics::{GoalJudgment, callables::CaptureForm};
    use compiler::analysis::ordinary::RequirementTarget;
    use compiler::analysis::ownership::{GraphCompletion, GraphObligation, analyze_ownership};
    use compiler::analysis::{InvocationMode, TypeTerm};

    installed(
        "scoped_join::installed_join_transfers_disjoint_exclusive_list_after_last_use",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            for live in [false, true] {
                let (before, after) = if live {
                    ("", "let last = held[0u64];")
                } else {
                    ("let last = held[0u64];", "")
                };
                let source = format!(
                    r#"{BASE}
import std::routine::join;
function joined(numbers: List<i32>, otherInput: List<i32>): [i32, i32] {{
    let mut other = otherInput;
    let held = &other;
    {before}
    let [first, second] = join([() => (&numbers)[0u64], () => (&mut other)[1u64]]);
    {after}
    dispose(numbers);
    dispose(other);
    return [first, second];
}}
"#
                );
                fs::write(&request.entry, source).unwrap();
                for test_mode in [false, true] {
                    let request = InstalledRequest {
                        test_mode,
                        ..request.clone()
                    };
                    let graph = loader::load_installed(request.clone()).unwrap();
                    captured_keys(&graph, true);
                    let module = app(&graph);
                    let caller = &module.declarations[named(module, "joined")];
                    let limits = AnalysisLimits::default();
                    let typed = analyze_typed(&graph, limits);
                    let receipt = typed.receipt().expect("typed disjoint environments");
                    let result_use = typed.result_use().unwrap();
                    assert!(result_use.pending().is_empty() && result_use.discards().is_empty());
                    let node = call_node(module, "join([");
                    let transfer = receipt.scoped_join(module, node).unwrap().unwrap();
                    let role = typed.routine_role().unwrap();
                    let ids = [&role.family.id, &role.implementation.id, &role.bridge.id];
                    let call = typed.bodies().ordinary_call(module, node).unwrap().unwrap();
                    let winner = typed.bodies().goal(call.goal.as_ref().unwrap()).unwrap();
                    let selected = winner.selection.as_ref().unwrap();
                    assert_eq!(winner.state, GoalState::Proved);
                    assert_eq!(
                        (selected.implementation, selected.alternative),
                        (ids[1], transfer.alternative)
                    );
                    assert_eq!(winner.dependencies.len(), 6);
                    for (i, slot) in transfer.slots.iter().enumerate() {
                        let mutable = i == 1;
                        let arrow = slot.arrow();
                        assert_eq!(slot.path, [i]);
                        assert!(slot.reference_result.is_none());
                        assert_eq!(
                            arrow.mode,
                            Some(if mutable {
                                InvocationMode::Mutable
                            } else {
                                InvocationMode::ReadOnly
                            })
                        );
                        let [capture] = arrow.captures.as_slice() else {
                            panic!("one List capture")
                        };
                        assert_eq!(capture.form, CaptureForm::ExplicitBorrow { mutable });
                        assert_eq!(
                            capture.path.root,
                            &module.declarations[named(module, ["numbers", "other"][i])].id
                        );
                        let storage = capture.storage.as_ref().unwrap();
                        let TypeTerm::Reference {
                            mutable: mode,
                            referent,
                        } = storage.as_ref()
                        else {
                            panic!("borrowed List storage")
                        };
                        assert_eq!(*mode, mutable);
                        assert_eq!(selected.substitution[2 + i].argument, arrow.result);
                        for (id, predicate) in [
                            (&slot.callable_once, "callableOnce"),
                            (&slot.send_job, "sendable"),
                            (&slot.send_result, "sendable"),
                        ] {
                            assert!(winner.dependencies.contains(id));
                            let goal = typed.bodies().goal(id).unwrap();
                            assert_eq!(goal.state, GoalState::Proved);
                            assert!(goal.premise.is_none());
                            assert!(
                                matches!(&goal.judgment, GoalJudgment::Requirement { target: RequirementTarget::Predicate(p), .. } if p.name() == predicate)
                            );
                        }
                        let invoke = typed.bodies().goal(&slot.callable_once).unwrap();
                        assert!(
                            matches!(&invoke.judgment, GoalJudgment::Requirement { arguments, .. }
                            if matches!(arguments.as_slice(), [job, input, result]
                                if job == &selected.substitution[i].argument && result == &arrow.result
                                && matches!(input.as_ref(), TypeTerm::Tuple(t) if t.is_empty())))
                        );
                        let result = typed.bodies().goal(&slot.send_result).unwrap();
                        assert!(
                            matches!(&result.judgment, GoalJudgment::Requirement { arguments, .. } if arguments == std::slice::from_ref(&arrow.result))
                        );
                        assert!(result.arrow_origin.is_none());
                        let job = typed.bodies().goal(&slot.send_job).unwrap();
                        assert!(std::ptr::eq(job.arrow_origin.as_deref().unwrap(), arrow));
                        let [property] = job.dependencies.as_slice() else {
                            panic!("capture property")
                        };
                        let property = typed.bodies().goal(property).unwrap();
                        assert_eq!(property.source.node, capture.site.node);
                        assert_eq!(property.state, GoalState::Proved);
                        assert!(matches!(&property.judgment, GoalJudgment::Requirement {
                            target: RequirementTarget::Predicate(p), arguments
                        } if p.name() == "sendable" && arguments == std::slice::from_ref(storage)));
                        let [referent_property] = property.dependencies.as_slice() else {
                            panic!("referent property")
                        };
                        let referent_property = typed.bodies().goal(referent_property).unwrap();
                        assert_eq!(referent_property.state, GoalState::Proved);
                        let predicate = if mutable { "sendable" } else { "shareable" };
                        assert!(matches!(&referent_property.judgment,
                            GoalJudgment::Requirement { target: RequirementTarget::Predicate(p), arguments }
                            if p.name() == predicate && arguments == std::slice::from_ref(referent)));
                    }
                    let second = &transfer.slots[1].arrow().captures[0];
                    let local = lower_body(&receipt, module, &caller.id, limits).unwrap();
                    let owned = analyze_ownership(&receipt, limits);
                    let root = owned
                        .roots()
                        .iter()
                        .find(|r| r.site().module == &module.key && r.site().node == caller.node)
                        .unwrap();
                    let expected = if live {
                        Completion::SourceRejected
                    } else {
                        Completion::Checked
                    };
                    assert_eq!(local.completion(), expected);
                    assert_eq!(root.completion(), expected);
                    assert_eq!(root.ordinary_completion(), expected);
                    let graph_expected = if live {
                        GraphCompletion::SourceRejected
                    } else {
                        GraphCompletion::Incomplete
                    };
                    assert_eq!(owned.completion(), graph_expected);
                    if live {
                        assert!(
                            local.mir().is_none()
                                && root.mir().is_none()
                                && root.ordinary_mir().is_none()
                        );
                        for findings in [local.findings(), owned.findings()] {
                            let [finding] = findings else {
                                panic!("one acquisition conflict: {findings:?}")
                            };
                            assert_eq!(finding.code(), "R0201");
                            assert_eq!(finding.site.span, second.site.span);
                            assert_eq!(finding.related.span.bytes(), b"&other");
                        }
                    } else {
                        assert!(local.findings().is_empty() && owned.findings().is_empty());
                        for mir in [local.mir(), root.mir(), root.ordinary_mir()] {
                            let proof = prove_join(
                                mir.unwrap(),
                                node,
                                ids,
                                [(0, "numbers: List<i32>"), (1, "other")],
                            );
                            assert_eq!(proof.frames.each_ref().map(|f| f.exclusive), [false, true]);
                        }
                        let foreign = loader::load_installed(request).unwrap();
                        let other = app(&foreign);
                        assert!(matches!(
                            receipt.scoped_join(other, node),
                            Err(QueryError::InvalidRequest)
                        ));
                        assert!(matches!(
                            lower_body(&receipt, other, &caller.id, limits),
                            Err(QueryError::InvalidRequest)
                        ));
                        assert!(
                            owned
                                .open_obligations()
                                .contains(&GraphObligation::MaskingAndJoin)
                        );
                    }
                    let observed =
                        |s: compiler::analysis::Statistics| (s.work, s.entries, s.max_depth);
                    let typed_stats = typed.statistics();
                    assert_eq!(
                        observed(typed_stats),
                        if live {
                            (493700, 56808, 12)
                        } else {
                            (493701, 56810, 12)
                        }
                    );
                    let (exact, short) = budget_caps(typed_stats);
                    let replay = analyze_typed(&graph, exact);
                    assert_eq!(
                        (replay.completion(), replay.statistics()),
                        (TypedCompletion::Checked, typed_stats)
                    );
                    for cap in short {
                        let report = analyze_typed(&graph, cap);
                        assert_eq!(report.completion(), TypedCompletion::ResourceLimit);
                        assert!(report.receipt().is_none());
                    }
                    let stats = local.statistics();
                    // This acquisition already rejected before child-frame lowering
                    // was admitted. It must still fit the 81f5264 target budget.
                    if live {
                        let old = AnalysisLimits {
                            max_work: 4159,
                            max_entries: 838,
                            max_depth: 10,
                        };
                        let stopped = lower_body(&receipt, module, &caller.id, old).unwrap();
                        assert_eq!(stopped.completion(), Completion::SourceRejected);
                        assert!(stopped.mir().is_none());
                        assert_eq!(stopped.findings()[0].code(), "R0201");
                    }
                    let (exact, short) = budget_caps(stats);
                    let replay = lower_body(&receipt, module, &caller.id, exact).unwrap();
                    assert_eq!(
                        (replay.completion(), replay.statistics()),
                        (expected, stats)
                    );
                    for cap in short {
                        let report = lower_body(&receipt, module, &caller.id, cap).unwrap();
                        assert_eq!(report.completion(), Completion::ResourceLimit);
                        assert!(report.mir().is_none());
                    }
                    let graph_stats = owned.statistics();
                    if live {
                        let old = AnalysisLimits {
                            max_work: 213183,
                            max_entries: 5602,
                            max_depth: 10,
                        };
                        let stopped = analyze_ownership(&receipt, old);
                        assert_eq!(stopped.completion(), GraphCompletion::SourceRejected);
                        assert_eq!(stopped.findings()[0].code(), "R0201");
                    }
                    let (exact, short) = budget_caps(graph_stats);
                    let replay = analyze_ownership(&receipt, exact);
                    assert_eq!(
                        (replay.completion(), replay.statistics()),
                        (graph_expected, graph_stats)
                    );
                    for cap in short {
                        let report = analyze_ownership(&receipt, cap);
                        assert_eq!(report.completion(), GraphCompletion::ResourceLimit);
                        assert!(
                            report
                                .roots()
                                .iter()
                                .all(|r| r.mir().is_none() && r.ordinary_mir().is_none())
                        );
                    }
                }
            }
        },
    );
}

#[test]
fn installed_join_reacquires_exclusive_list_only_after_completion() {
    use compiler::analysis::ownership::{GraphCompletion, analyze_ownership};

    installed(
        "scoped_join::installed_join_reacquires_exclusive_list_only_after_completion",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            for prefix in ["", "dispose(other);"] {
                let source = format!(
                    r#"{BASE}
import std::routine::join;
function joined(input: List<i32>, extra: List<i32>): [i32, i32] {{
    let mut numbers = input;
    let mut other = extra;
    {prefix}
    let [first, second] = join([() => (&numbers)[0u64], () => (&mut other)[1u64]]);
    let [third, fourth] = join([() => (&mut other)[0u64], () => (&mut numbers)[1u64]]);
    dispose(numbers);
    dispose(other);
    return [first, fourth];
}}
"#
                );
                fs::write(&request.entry, source).unwrap();
                for test_mode in [false, true] {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let module = app(&graph);
                    let caller = &module.declarations[named(module, "joined")];
                    let limits = AnalysisLimits::default();
                    let typed = analyze_typed(&graph, limits);
                    let receipt = typed
                        .receipt()
                        .expect("both calls type-check independently of ownership");
                    let target = lower_body(&receipt, module, &caller.id, limits).unwrap();
                    let owned = analyze_ownership(&receipt, limits);
                    let root = owned
                        .roots()
                        .iter()
                        .find(|r| r.site().module == &module.key && r.site().node == caller.node)
                        .unwrap();
                    if !prefix.is_empty() {
                        assert_eq!(target.completion(), Completion::SourceRejected);
                        assert_eq!(root.completion(), Completion::SourceRejected);
                        assert_eq!(owned.completion(), GraphCompletion::SourceRejected);
                        assert!(
                            target.mir().is_none()
                                && root.mir().is_none()
                                && root.ordinary_mir().is_none()
                        );
                        for findings in [target.findings(), owned.findings()] {
                            let [finding] = findings else {
                                panic!("one moved backing finding")
                            };
                            assert_eq!(finding.code(), "R0200");
                            assert_eq!(finding.site.span.bytes(), b"&mut other");
                            assert_eq!(finding.related.span.bytes(), b"dispose(other)");
                        }
                        continue;
                    }
                    assert_eq!(target.completion(), Completion::Checked);
                    assert_eq!(root.completion(), Completion::Checked);
                    for mir in [target.mir().unwrap(), root.mir().unwrap()] {
                        let path = straight_blocks(mir, mir.entry);
                        let joins = path
                            .iter()
                            .enumerate()
                            .filter_map(|(i, block)| match &block.operation {
                                Operation::ScopedJoin(proof) => Some((i, proof)),
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        let [(first_at, first), (second_at, second)] = joins.as_slice() else {
                            panic!("two checked joins")
                        };
                        assert_eq!(first.frames.each_ref().map(|f| f.exclusive), [false, true]);
                        assert_eq!(second.frames.each_ref().map(|f| f.exclusive), [true, true]);
                        let other = first.frames[1].backing.root.node;
                        assert_eq!(second.frames[0].backing.root.node, other);
                        assert!(path[first_at + 1..*second_at].iter().any(|b| matches!(b.operation,
                            Operation::Borrow { source, holder, exclusive: true }
                            if source.root.node == other && holder.root.node == second.frames[0].capture.root.node)));
                    }
                }
            }
        },
    );
}

fn check_reference_lifecycle(module: &Module, mir: &Mir<'_>, sources: [&str; 2]) {
    use compiler::analysis::ownership::{JoinChildState::*, JoinEvent, JoinNext, Place};

    let path = straight_blocks(mir, mir.entry);
    let [copy_a, form_a, copy_b, form_b, call, export, ..] = path.as_slice() else {
        panic!("staged descriptors followed by canonical join and tuple export")
    };
    let Operation::ScopedJoin(join) = &call.operation else {
        panic!("selected call must use the checked join transitions")
    };
    let Operation::Export(result) = export.operation else {
        panic!("ordinary successor exports the published tuple")
    };
    let places = |places: [Option<Place<'_>>; 2]| places.map(|place| place.map(|p| p.root.node));
    let export_roots = std::array::from_fn(|i| {
        let frame = &join.frames[i];
        let Operation::Copy {
            from,
            into: Some(staged),
        } = [copy_a, copy_b][i].operation
        else {
            panic!("capture formation copies the incoming descriptor, not its address")
        };
        assert_eq!(
            from.root.span.bytes(),
            format!("{}: &i32", sources[i]).as_bytes()
        );
        assert!(std::ptr::eq(from.root.module, &module.key));
        assert_eq!(from.root.node, frame.backing.root.node);
        assert_eq!(staged.root.node, frame.capture.root.node);
        assert!(
            matches!([form_a, form_b][i].operation, Operation::FormClosure { closure, capture, .. }
            if closure.node == frame.environment.root.node && capture.node == staged.root.node)
        );
        assert!(frame.incoming_list_backing.is_none());
        assert_eq!(frame.result.tuple_element(), Some(i));
        assert_eq!(frame.result.root.node, result.root.node);
        let body = straight_blocks(&frame.body, frame.body.entry);
        let [copy, export, cleanup, returned] = body.as_slice() else {
            panic!("child copies its capture, exports the stage, cleans up and returns")
        };
        let Operation::Copy {
            from,
            into: Some(output),
        } = copy.operation
        else {
            panic!("no child descriptor copy")
        };
        assert_eq!(from.root.node, frame.capture.root.node);
        assert_ne!(from.root.node, output.root.node);
        assert!(
            matches!(export.operation, Operation::Export(p) if p.root.node == output.root.node)
        );
        assert!(matches!(&cleanup.operation, Operation::Cleanup(actions)
            if matches!(actions.as_slice(), [Obligation::EndStorage(p)] if p.root.node == from.root.node)));
        assert!(matches!(returned.terminator, Terminator::Return));
        output.root.node
    });
    let keys = join
        .frames
        .each_ref()
        .map(|frame| Some(frame.backing.root.node));
    assert!(
        matches!(join.before_commit.as_slice(), [Obligation::EndStorage(b), Obligation::EndStorage(a)]
        if [a.root.node, b.root.node] == join.frames.each_ref().map(|frame| frame.environment.root.node))
    );
    assert!(
        matches!(join.parent_exit.cleanup.as_slice(), [Obligation::EndStorage(b), Obligation::EndStorage(a)]
        if a.root.span.bytes() == b"input: &i32" && b.root.span.bytes() == b"other: &i32")
    );
    let mut cancel_prefixes = [false; 4];
    let mut publish_last = [false; 2];
    for wait in &join.waiting {
        let stages = std::array::from_fn(|i| keys[i].filter(|_| wait.initialized_results[i]));
        let environments = std::array::from_fn(|i| {
            keys[i].filter(|_| matches!(wait.children[i], Entry | Running | Cleaning))
        });
        assert_eq!(places(wait.retained_results), stages);
        assert_eq!(places(wait.environment_backings), environments);
        for (event, next) in &wait.successors {
            match next {
                JoinNext::Publish {
                    transfers,
                    retained_results,
                    ..
                } => {
                    let JoinEvent::FinishChild(i) = *event else {
                        panic!("publication before child finish")
                    };
                    assert!(!wait.deferred);
                    assert_eq!(wait.children[1 - i], Complete);
                    assert_eq!(wait.children[i], AtNormalReturn);
                    assert_eq!(places(*retained_results), keys);
                    assert_eq!(transfers.map(|(from, _)| from.root.node), export_roots);
                    assert_eq!(
                        transfers.map(|(_, into)| into.tuple_element()),
                        [Some(0), Some(1)]
                    );
                    publish_last[i] = true;
                }
                JoinNext::Cancel { retired } => {
                    let JoinEvent::FinishChild(i) = *event else {
                        panic!("stop before child finish")
                    };
                    assert!(wait.deferred);
                    assert_eq!(wait.children[1 - i], Complete);
                    assert!(matches!(wait.children[i], AtNormalReturn | AtDeliveredExit));
                    assert_eq!(
                        places(*retired),
                        std::array::from_fn(
                            |i| wait.initialized_results[i].then_some(export_roots[i])
                        )
                    );
                    let prefix = usize::from(wait.initialized_results[0])
                        + 2 * usize::from(wait.initialized_results[1]);
                    cancel_prefixes[prefix] = true;
                }
                JoinNext::Waiting(_) => {}
                JoinNext::CancelBeforeCommit => panic!("entry-only edge after commit"),
            }
        }
    }
    assert_eq!(publish_last, [true; 2]);
    assert_eq!(cancel_prefixes, [true; 4]);
}

#[test]
fn grouped_reference_publication_keeps_each_result_dependency() {
    probe(
        "scoped_join::grouped_reference_publication_keeps_each_result_dependency",
        r#"
        import std::routine::join;
        function joined(input: &i32, other: &i32): [&i32,&i32] {
            return join((((([((() => ((other)))), () => (((input)))])))));
        }
    "#,
        |_, module, receipt| {
            let d = &module.declarations[named(module, "joined")];
            let lower = |limits| lower_body(receipt, module, &d.id, limits).unwrap();
            let full = lower(AnalysisLimits::default());
            let s = full.statistics();
            let exact = AnalysisLimits {
                max_work: s.work,
                max_entries: s.entries,
                max_depth: s.max_depth,
            };
            check_reference_lifecycle(
                module,
                lower(exact).mir().expect("grouped published pair"),
                ["other", "input"],
            );
            for short in [
                AnalysisLimits {
                    max_work: s.work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: s.entries - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_depth: s.max_depth - 1,
                    ..exact
                },
            ] {
                let limited = lower(short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
        },
    );
}

#[test]
fn grouped_reference_children_charge_depth_and_keep_the_written_escape() {
    probe(
        "scoped_join::grouped_reference_children_charge_depth_and_keep_the_written_escape",
        r#"
        import std::routine::join;
        function bare(input: &i32): [&i32,&i32] {return join([() => input, () => { let local=2i32; return &local; }]);}
        function grouped(input: &i32): [&i32,&i32] {
            return join((((([((() => (((input))))), ((() => { let local=((2i32)); return ((&((local)))); }))])))));
        }
        function skipped(input: &i32): [&i32,&i32] {if(false) { return join([() => { let local=2i32; return &local; }, () => input]); } return [input,input];}
    "#,
        |_, module, receipt| {
            let run = |name: &str, limit| {
                let d = &module.declarations[named(module, name)];
                lower_body(receipt, module, &d.id, limit).unwrap()
            };
            let default = AnalysisLimits::default();
            let bare = run("bare", default);
            let grouped = run("grouped", default);
            assert_eq!(bare.completion(), Completion::SourceRejected);
            assert_eq!(grouped.completion(), Completion::SourceRejected);
            let [finding] = grouped.findings() else {
                panic!("one failed child export")
            };
            assert_eq!(finding.code(), "R0202");
            assert_eq!(finding.site.span.bytes(), b"return ((&((local))));");
            assert_eq!(finding.related.span.bytes(), b"local");
            assert!(grouped.mir().is_none());
            assert!(grouped.statistics().max_depth > bare.statistics().max_depth);
            let capped = run(
                "grouped",
                AnalysisLimits {
                    max_depth: bare.statistics().max_depth,
                    ..default
                },
            );
            assert_eq!(capped.completion(), Completion::ResourceLimit);
            assert!(capped.mir().is_none() && capped.findings().is_empty());
            assert!(run("skipped", default).mir().is_some());
        },
    );
}

#[test]
fn typed_reference_operand_groups_count_toward_depth() {
    installed(
        "scoped_join::typed_reference_operand_groups_count_toward_depth",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            for test_mode in [false, true] {
                let mut bare_depth = 0;
                for groups in [0, 20] {
                    let open = "(".repeat(groups);
                    let close = ")".repeat(groups);
                    fs::write(&request.entry,format!("{BASE}\nimport std::routine::join;\nfunction joined(input:&i32): [&i32,&i32] {{ return join([() => input, () => {{ let local=2i32; return &{open}local{close}; }}]); }}")).unwrap();
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    assert!(typed.receipt().is_some());
                    if groups == 0 {
                        bare_depth = typed.statistics().max_depth;
                    } else {
                        assert!(typed.statistics().max_depth >= groups);
                        let short = analyze_typed(
                            &graph,
                            AnalysisLimits {
                                max_depth: bare_depth,
                                ..Default::default()
                            },
                        );
                        assert_eq!(short.completion(), TypedCompletion::ResourceLimit);
                        assert!(short.receipt().is_none());
                    }
                }
            }
        },
    );
}

#[test]
fn unsupported_reference_children_preserve_prior_typed_caps() {
    installed(
        "scoped_join::unsupported_reference_children_preserve_prior_typed_caps",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            for (params, jobs, max_work, max_entries, max_depth) in [
                (
                    "input: &i32, work: () => &i32",
                    "() => input, work",
                    487296,
                    55357,
                    12,
                ),
                (
                    "input: &i32",
                    "() => input, () => { let local = 2i32; return &(local + 1i32); }",
                    488293,
                    55667,
                    14,
                ),
            ] {
                fs::write(&request.entry,format!("{BASE}\nimport std::routine::join;\nfunction joined({params}): [&i32, &i32] {{\n    return join([{jobs}]);\n}}\n")).unwrap();
                for test_mode in [false, true] {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let old = analyze_typed(
                        &graph,
                        AnalysisLimits {
                            max_work,
                            max_entries,
                            max_depth,
                        },
                    );
                    assert_eq!(old.completion(), TypedCompletion::Incomplete);
                    assert!(old.receipt().is_none());
                }
            }
        },
    );
}

#[test]
fn installed_join_checks_reference_result_escape_per_slot() {
    use compiler::analysis::ownership::{GraphCompletion, GraphObligation, analyze_ownership};
    use compiler::analysis::scalars::generics::{GoalId, GoalJudgment, callables::CaptureForm};
    use compiler::analysis::typed::ScopedJoinReference;
    use compiler::analysis::{
        DeclarationShape, InvocationMode, Statistics, TypeTerm, ordinary::RequirementTarget,
    };

    let caps = |stats: Statistics| {
        let exact = AnalysisLimits {
            max_work: stats.work,
            max_entries: stats.entries,
            max_depth: stats.max_depth,
        };
        let short = [
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
        ];
        (exact, short)
    };

    installed(
        "scoped_join::installed_join_checks_reference_result_escape_per_slot",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            for (parameters, first, second, expected) in [
                (
                    "input: &i32",
                    "input",
                    "{ let local = 2i32; return &local; }",
                    Completion::SourceRejected,
                ),
                (
                    "input: &i32, other: &i32",
                    "input",
                    "other",
                    Completion::Checked,
                ),
                (
                    "input: &i32, other: &i32",
                    "other",
                    "input",
                    Completion::Checked,
                ),
                (
                    "input: &i32, other: &i32",
                    "input",
                    "input",
                    Completion::Checked,
                ),
            ] {
                fs::write(
                    &request.entry,
                    format!("{BASE}\nimport std::routine::join;\nfunction joined({parameters}): [&i32, &i32] {{\n    return join([() => {first}, () => {second}]);\n}}\n"),
                )
                .unwrap();
                for test_mode in [false, true] {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    captured_keys(&graph, true);
                    let limits = AnalysisLimits::default();
                    let typed = analyze_typed(&graph, limits);
                    assert_eq!(typed.completion(), TypedCompletion::Checked, "{second}");
                    let residence = typed.result_use().unwrap();
                    assert!(residence.pending().is_empty() && residence.discards().is_empty());
                    let receipt = typed.receipt().unwrap();
                    let module = app(&graph);
                    let caller = &module.declarations[named(module, "joined")];
                    let call = call_node(module, "join([");
                    let proof = receipt.scoped_join(module, call).unwrap().unwrap();
                    let ordinary = typed.bodies().ordinary_call(module, call).unwrap().unwrap();
                    let selection = typed
                        .bodies()
                        .goal(ordinary.goal.as_ref().unwrap())
                        .unwrap();
                    let selected = selection.selection.as_ref().unwrap();
                    assert_eq!(selected.implementation, &proof.role.implementation.id);
                    assert_eq!(selected.alternative, proof.alternative);
                    assert!(ordinary.checked && ordinary.premise.is_none());
                    assert_eq!(selection.dependencies.len(), 6);
                    let proved = |id: &GoalId, name: &str| {
                        let goal = typed.bodies().goal(id).unwrap();
                        let intrinsic = graph
                            .prelude()
                            .intrinsics
                            .iter()
                            .find(|id| id.name() == name)
                            .unwrap();
                        assert_eq!(goal.state, GoalState::Proved);
                        assert!(goal.premise.is_none());
                        assert!(matches!(&goal.judgment, GoalJudgment::Requirement {
                            target: RequirementTarget::Predicate(id), ..
                        } if *id == intrinsic));
                        goal
                    };
                    let signature = typed
                        .inventory()
                        .type_facts()
                        .declarations()
                        .find(|facts| facts.declaration.id == caller.id)
                        .unwrap();
                    let DeclarationShape::Function { parameters, .. } = &signature.shape else {
                        panic!("checked caller signature")
                    };
                    let mut requirements = Vec::new();
                    for (i, slot) in proof.slots.iter().enumerate() {
                        let arrow = slot.arrow();
                        assert_eq!(slot.path, [i]);
                        assert_eq!(arrow.mode, Some(InvocationMode::ReadOnly));
                        assert!(arrow.premise.is_none() && arrow.parameters.is_empty());
                        assert_eq!(selected.substitution[2 + i].argument, arrow.result);
                        let TypeTerm::Reference {
                            mutable: false,
                            referent,
                        } = arrow.result.as_ref()
                        else {
                            panic!("shared descriptor")
                        };
                        assert!(
                            matches!(referent.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
                        );
                        for id in [&slot.callable_once, &slot.send_job, &slot.send_result] {
                            assert!(
                                selection.dependencies.contains(id) && !requirements.contains(&id)
                            );
                            requirements.push(id);
                        }
                        let invoke = proved(&slot.callable_once, "callableOnce");
                        assert!(
                            matches!(&invoke.judgment, GoalJudgment::Requirement { arguments, .. }
                            if matches!(arguments.as_slice(), [job, input, result]
                                if job == &selected.substitution[i].argument && result == &arrow.result
                                    && matches!(input.as_ref(), TypeTerm::Tuple(xs) if xs.is_empty())))
                        );
                        let job = proved(&slot.send_job, "sendable");
                        assert!(std::ptr::eq(job.arrow_origin.as_deref().unwrap(), arrow));
                        let result = proved(&slot.send_result, "sendable");
                        assert!(result.arrow_origin.is_none());
                        assert!(
                            matches!(&result.judgment, GoalJudgment::Requirement { arguments, .. }
                            if arguments == std::slice::from_ref(&arrow.result))
                        );
                        let body = module.syntax.nodes[arrow.site.node].children[1];
                        match slot.reference_result.unwrap() {
                            ScopedJoinReference::Incoming { parameter, value } => {
                                assert!(i == 0 || expected == Completion::Checked);
                                let wanted = [first, second][i];
                                let formal = usize::from(wanted == "other");
                                assert_eq!(parameter, &parameters[formal].declaration.id);
                                assert_eq!(value.node, body);
                                assert_eq!(value.span.bytes(), wanted.as_bytes());
                                let [capture] = arrow.captures.as_slice() else {
                                    panic!("one carried descriptor")
                                };
                                assert_eq!(capture.path.root, parameter);
                                assert_eq!(capture.site.span, value.span);
                                assert_eq!(
                                    capture.form,
                                    CaptureForm::CarriedReference { mutable: false }
                                );
                                assert_eq!(capture.storage.as_ref(), Some(&arrow.result));
                                let [property] = job.dependencies.as_slice() else {
                                    panic!("one capture proof")
                                };
                                let property = proved(property, "sendable");
                                assert_eq!(property.source.span, value.span);
                                assert!(
                                    matches!(&property.judgment, GoalJudgment::Requirement { arguments, .. }
                                    if arguments == std::slice::from_ref(&arrow.result))
                                );
                            }
                            ScopedJoinReference::ChildLocal { binding, returning } => {
                                assert_eq!((i, expected), (1, Completion::SourceRejected));
                                assert!(arrow.captures.is_empty() && job.dependencies.is_empty());
                                assert_eq!(
                                    module.syntax.nodes[body].children,
                                    [binding.node, returning.node]
                                );
                                assert_eq!(binding.span.bytes(), b"let local = 2i32;");
                                assert_eq!(returning.span.bytes(), b"return &local;");
                            }
                        }
                    }
                    assert_ne!(
                        proof.slots[0].arrow().site.node,
                        proof.slots[1].arrow().site.node
                    );
                    let local = lower_body(&receipt, module, &caller.id, limits).unwrap();
                    assert_eq!(local.completion(), expected);
                    assert_eq!(local.mir().is_some(), expected == Completion::Checked);
                    let owned = analyze_ownership(&receipt, limits);
                    let root = owned
                        .roots()
                        .iter()
                        .find(|r| r.site().module == &module.key && r.site().node == caller.node)
                        .unwrap();
                    assert_eq!(root.completion(), expected);
                    assert_eq!(root.ordinary_completion(), expected);
                    assert_eq!(root.mir().is_some(), expected == Completion::Checked);
                    assert_eq!(
                        root.ordinary_mir().is_some(),
                        expected == Completion::Checked
                    );
                    assert!(
                        owned
                            .open_obligations()
                            .contains(&GraphObligation::MaskingAndJoin)
                    );
                    if expected == Completion::SourceRejected {
                        assert_eq!(owned.completion(), GraphCompletion::SourceRejected);
                        assert!(root.dependencies().is_empty());
                        for findings in [local.findings(), owned.findings()] {
                            let [finding] = findings else {
                                panic!("one child-local escape")
                            };
                            assert_eq!(finding.code(), "R0202");
                            assert_eq!(finding.site.span.bytes(), b"return &local;");
                            assert_eq!(finding.related.span.bytes(), b"local");
                        }
                    } else {
                        assert_eq!(owned.completion(), GraphCompletion::Incomplete);
                        assert!(local.findings().is_empty() && owned.findings().is_empty());
                        assert!(local.blocked_at().is_none() && root.blocked_at().is_none());
                        for mir in [local.mir().unwrap(), root.mir().unwrap()] {
                            check_reference_lifecycle(module, mir, [first, second]);
                        }
                        assert!(matches!(root.dependencies(), [dependency] if matches!(
                            dependency.contract,
                            compiler::analysis::ownership::DependencyContract::ScopedJoin { bridge, .. }
                                if bridge == &proof.role.bridge.id
                        )));
                        let foreign = loader::load_installed(InstalledRequest {
                            test_mode,
                            ..request.clone()
                        })
                        .unwrap();
                        let other = app(&foreign);
                        assert_eq!(
                            receipt.scoped_join(other, call).unwrap_err(),
                            QueryError::InvalidRequest
                        );
                        assert_eq!(
                            lower_body(&receipt, other, &caller.id, limits).unwrap_err(),
                            QueryError::InvalidRequest,
                        );
                    }
                    check_typed_budget(&graph, &typed);
                    let (exact, short) = caps(local.statistics());
                    let replay = lower_body(&receipt, module, &caller.id, exact).unwrap();
                    assert_eq!(replay.statistics(), local.statistics());
                    assert_eq!(replay.completion(), expected);
                    assert_eq!(replay.findings().len(), local.findings().len());
                    for short in short {
                        let exhausted = lower_body(&receipt, module, &caller.id, short).unwrap();
                        assert_eq!(exhausted.completion(), Completion::ResourceLimit);
                        assert!(exhausted.mir().is_none());
                    }
                    let (exact, short) = caps(owned.statistics());
                    let replay = analyze_ownership(&receipt, exact);
                    assert_eq!(replay.statistics(), owned.statistics());
                    assert_eq!(replay.completion(), owned.completion());
                    for short in short {
                        let exhausted = analyze_ownership(&receipt, short);
                        assert_eq!(exhausted.completion(), GraphCompletion::ResourceLimit);
                        assert!(
                            exhausted
                                .roots()
                                .iter()
                                .all(|r| r.mir().is_none() && r.ordinary_mir().is_none())
                        );
                    }
                }
            }
        },
    );
}

#[test]
fn installed_join_does_not_weaken_the_callers_retained_alias() {
    use compiler::analysis::ownership::{GraphCompletion, analyze_ownership};

    installed(
        "scoped_join::installed_join_does_not_weaken_the_callers_retained_alias",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            for (statements, expected) in [
                (
                    "dispose(first); return *retained;",
                    Completion::SourceRejected,
                ),
                (
                    "let value = *retained; dispose(first); return value;",
                    Completion::Checked,
                ),
            ] {
                fs::write(
                    &request.entry,
                    format!("{BASE}\nimport std::routine::join;\nfunction joined(input: &i32, other: &i32): [&i32,&i32] {{ return join([() => input, () => other]); }}\nfunction caller(first: i32, second: i32): i32 {{ let [a,b] = joined(&first, &second); let retained = b; {statements} }}"),
                ).unwrap();
                for test_mode in [false, true] {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let limits = AnalysisLimits::default();
                    let typed = analyze_typed(&graph, limits);
                    let receipt = typed
                        .receipt()
                        .expect("the ordinary caller has a typed receipt");
                    let module = app(&graph);
                    let caller = &module.declarations[named(module, "caller")];
                    let joined = &module.declarations[named(module, "joined")].id;
                    let target = lower_body(&receipt, module, &caller.id, limits).unwrap();
                    let owned = analyze_ownership(&receipt, limits);
                    let root = owned
                        .roots()
                        .iter()
                        .find(|r| r.site().module == &module.key && r.site().node == caller.node)
                        .unwrap();
                    assert_eq!(target.completion(), expected, "{:?}", target.blocked_at());
                    assert_eq!(root.completion(), expected, "{:?}", root.blocked_at());
                    if expected == Completion::SourceRejected {
                        assert_eq!(owned.completion(), GraphCompletion::SourceRejected);
                        assert!(target.mir().is_none() && root.mir().is_none());
                        for findings in [target.findings(), owned.findings()] {
                            let [finding] = findings else {
                                panic!("one caller-restriction finding")
                            };
                            assert_eq!(finding.code(), "R0203");
                            assert_eq!(finding.site.span.bytes(), b"dispose(first)");
                            assert_eq!(finding.related.span.bytes(), b"&first");
                        }
                    } else {
                        assert_eq!(owned.completion(), GraphCompletion::Incomplete);
                        for mir in [target.mir().unwrap(), root.mir().unwrap()] {
                            assert!(mir.blocks.iter().any(|block| matches!(block.operation, Operation::Call(callee) if callee == joined)));
                            assert!(
                                !mir.blocks.iter().any(|block| matches!(
                                    block.operation,
                                    Operation::ScopedJoin(_)
                                ))
                            );
                        }
                    }
                }
            }
        },
    );
}

const ORDERED: &str = r#"
import std::routine::join;
function ordered(input: List<i32>): [i32, bool] {
    let numbers = input;
    let [first, second] = join([() => (&numbers)[0u64], () => (&numbers)[1u64] == 29i32]);
    dispose(numbers);
    return [first, second];
}
"#;

#[test]
fn unsupported_comparison_operands_keep_their_old_typed_budgets() {
    installed(
        "scoped_join::unsupported_comparison_operands_keep_their_old_typed_budgets",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            // Exact a1f4db7 caps on the original result-order capture, changing
            // only the Boolean child's displayed expression.
            for (expr, max_work, max_entries, max_depth) in [
                ("29i32 == (&numbers)[1u64]", 490624, 56179, 12),
                ("((29i32)) == (((&numbers)[1u64]))", 491529, 56271, 14),
                ("(&numbers)[1u64] == (2i32+27i32)", 491173, 56245, 12),
                ("((&numbers)[1u64]+0i32) == 29i32", 491178, 56245, 14),
            ] {
                fs::write(
                    &request.entry,
                    format!(
                        "{BASE}{}",
                        ORDERED.replace("(&numbers)[1u64] == 29i32", expr)
                    ),
                )
                .unwrap();
                for test_mode in [false, true] {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let report = analyze_typed(
                        &graph,
                        AnalysisLimits {
                            max_work,
                            max_entries,
                            max_depth,
                        },
                    );
                    assert_eq!(report.completion(), TypedCompletion::Incomplete, "{expr}");
                    assert!(report.receipt().is_none());
                    assert!(
                        report
                            .result_use()
                            .unwrap()
                            .pending()
                            .iter()
                            .any(|p| p.blocker == ResultUseBlocker::CallBoundary)
                    );
                }
            }
        },
    );
}

fn prove_ordered_join(
    mir: &Mir<'_>,
    node: usize,
    ids: [&loader::DeclarationId; 3],
    reads: [(u64, &str); 2],
    literal: i32,
) {
    let join = prove_join(mir, node, ids, reads);
    for (i, frame) in join.frames.iter().enumerate() {
        let body = straight_blocks(&frame.body, frame.body.entry);
        let [read, tail @ ..] = body.as_slice() else {
            panic!("child expression and cleanup")
        };
        let Operation::ReadListI32 {
            view,
            backing,
            into: Some(result),
            ..
        } = read.operation
        else {
            panic!("the actual List index runs first")
        };
        assert_eq!(view.root.node, frame.capture.root.node);
        assert_eq!(
            backing.root.node,
            frame.incoming_list_backing.unwrap().root.node
        );
        let (result, tail) = if reads[i].0 == 1 {
            let [compare, tail @ ..] = tail else {
                panic!("the read feeds a comparison")
            };
            let Operation::EqualI32Literal {
                from,
                literal: rhs,
                into,
            } = compare.operation
            else {
                panic!("the Boolean result is computed, not initialized from its type")
            };
            assert_eq!(rhs, literal);
            assert_eq!(from.root.node, result.root.node);
            assert_ne!(from.root.node, into.root.node);
            assert_eq!(compare.site.node, into.root.node);
            (into, tail)
        } else {
            (result, tail)
        };
        let [export, cleanup, terminal] = tail else {
            panic!("stage, clean, terminate")
        };
        assert!(
            matches!(export.operation, Operation::Export(from) if from.root.node == result.root.node)
        );
        assert!(matches!(&cleanup.operation, Operation::Cleanup(actions)
            if matches!(actions.as_slice(), [Obligation::EndStorage(view)] if view.root.node == frame.capture.root.node)));
        assert!(matches!(terminal.terminator, Terminator::Return));
    }
    let path = straight_blocks(mir, mir.entry);
    let suffix = path
        .iter()
        .skip_while(|b| b.site.node != node || !matches!(b.operation, Operation::ScopedJoin(_)))
        .skip(1);
    let copies: Vec<_> = suffix
        .clone()
        .filter_map(|b| match b.operation {
            Operation::Copy {
                from,
                into: Some(into),
            } => Some((from, into)),
            _ => None,
        })
        .collect();
    let [bind_first, bind_second, return_first, return_second] = copies.as_slice() else {
        panic!("destructure published members, then copy the binders into the return tuple")
    };
    for (i, ((from, into), (bound, returned))) in [bind_first, bind_second]
        .into_iter()
        .zip([return_first, return_second])
        .enumerate()
    {
        assert_eq!(from.root.node, join.frames[i].result.root.node);
        assert_eq!(from.tuple_element(), Some(i));
        assert_eq!(into.root.node, bound.root.node);
        assert_eq!(
            into.root.span.bytes(),
            [b"first".as_slice(), b"second".as_slice()][i]
        );
        assert_eq!(returned.tuple_element(), Some(i));
        assert!(suffix.clone().any(
            |b| matches!(b.operation, Operation::Export(p) if p.root.node == returned.root.node)
        ));
    }
    for state in &join.waiting {
        use compiler::analysis::ownership::JoinChildState::{Cleaning, Entry, Running};
        for i in 0..2 {
            let root = join.frames[i].backing.root.node;
            assert_eq!(state.retained_backings[i].root.node, root);
            assert_eq!(state.retained_results[i].map(|p| p.root.node), None);
            assert_eq!(
                state.environment_backings[i].map(|p| p.root.node),
                matches!(state.children[i], Entry | Running | Cleaning).then_some(root)
            );
        }
    }
}

#[test]
fn grouped_list_comparison_preserves_the_checked_operands() {
    use compiler::analysis::ownership::analyze_ownership;
    let source = ORDERED.replace(
        "() => (&numbers)[1u64] == 29i32",
        "(() => { return (( ((&numbers)[((1u64))]) == (((29i32))) )); })",
    );
    probe_with_typed(
        "scoped_join::grouped_list_comparison_preserves_the_checked_operands",
        &source,
        |_, module, receipt, typed| {
            let d = &module.declarations[named(module, "ordered")];
            let call = call_node(module, "join([");
            let role = typed.routine_role().unwrap();
            let local = lower_body(receipt, module, &d.id, AnalysisLimits::default()).unwrap();
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let root = graph
                .roots()
                .iter()
                .find(|r| r.site().module == &module.key && r.site().node == d.node)
                .unwrap();
            for mir in [local.mir().unwrap(), root.mir().unwrap()] {
                prove_ordered_join(
                    mir,
                    call,
                    [&role.family.id, &role.implementation.id, &role.bridge.id],
                    [(0, "numbers"), (1, "numbers")],
                    29,
                );
            }
            let s = local.statistics();
            let short = lower_body(
                receipt,
                module,
                &d.id,
                AnalysisLimits {
                    max_depth: s.max_depth - 1,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(short.mir().is_none());
        },
    );
}

#[test]
fn installed_join_publishes_heterogeneous_list_results_in_input_order() {
    use compiler::analysis::TypeTerm;
    use compiler::analysis::generics::{GoalJudgment, callables::CaptureForm};
    use compiler::analysis::ordinary::RequirementTarget;
    use compiler::analysis::ownership::{GraphCompletion, GraphObligation, analyze_ownership};
    use compiler::analysis::scalars::data::{DataExpression, PatternProjection};

    installed(
        "scoped_join::installed_join_publishes_heterogeneous_list_results_in_input_order",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            for split in [false, true] {
                for reverse in [false, true] {
                    let other = if split { "other" } else { "numbers" };
                    let mut source =
                        ORDERED.replace("(&numbers)[1u64]", &format!("(&{other})[1u64]"));
                    if split {
                        source = source
                            .replace("input: List<i32>", "input: List<i32>, extra: List<i32>")
                            .replace(
                                "let numbers = input;",
                                "let numbers = input;\n    let other = extra;",
                            )
                            .replace(
                                "dispose(numbers);",
                                "dispose(numbers);\n    dispose(other);",
                            );
                    }
                    let literal = if split { 31 } else { 29 };
                    source = source.replace("29i32", &format!("{literal}i32"));
                    let jobs = [
                        "() => (&numbers)[0u64]".to_owned(),
                        format!("() => (&{other})[1u64] == {literal}i32"),
                    ];
                    let mut order = [0, 1];
                    if reverse {
                        order.swap(0, 1);
                        source = source.replace("[i32, bool]", "[bool, i32]").replace(
                            &format!("{}, {}", jobs[0], jobs[1]),
                            &format!("{}, {}", jobs[1], jobs[0]),
                        );
                    }
                    fs::write(&request.entry, format!("{BASE}{source}")).unwrap();
                    for test_mode in [false, true] {
                        let graph = loader::load_installed(InstalledRequest {
                            test_mode,
                            ..request.clone()
                        })
                        .unwrap();
                        captured_keys(&graph, true);
                        let module = app(&graph);
                        let caller = &module.declarations[named(module, "ordered")];
                        let node = call_node(module, "join([");
                        let limits = AnalysisLimits::default();
                        let typed = analyze_typed(&graph, limits);
                        assert_eq!(
                            typed.completion(),
                            TypedCompletion::Checked,
                            "{source}: {:?}",
                            typed
                                .result_use()
                                .unwrap()
                                .pending()
                                .iter()
                                .map(|p| (p.blocker, p.site.span.start()))
                                .collect::<Vec<_>>()
                        );
                        let residence = typed.result_use().unwrap();
                        assert!(residence.pending().is_empty() && residence.discards().is_empty());
                        let receipt = typed.receipt().unwrap();
                        let proof = receipt.scoped_join(module, node).unwrap().unwrap();
                        let call = typed.bodies().node(module, node).unwrap().unwrap();
                        let NodeFactKind::Expression(call) = &call.kind else {
                            panic!("call expression")
                        };
                        let TypeTerm::Tuple(outputs) = call.ty.as_ref() else {
                            panic!("ordered result tuple")
                        };
                        let pattern = module
                            .syntax
                            .nodes
                            .iter()
                            .enumerate()
                            .filter_map(|(i, _)| typed.bodies().node(module, i).unwrap())
                            .find_map(|f| match &f.kind {
                                NodeFactKind::Pattern(p) if p.source.node == node => Some(p),
                                _ => None,
                            })
                            .unwrap();
                        assert_eq!(pattern.ty, call.ty);
                        for (i, name) in ["first", "second"].into_iter().enumerate() {
                            let declaration = &module.declarations[named(module, name)].id;
                            let binder = pattern
                                .binders
                                .iter()
                                .find(|b| b.declaration == declaration)
                                .unwrap();
                            assert_eq!(binder.ty, outputs[i]);
                            assert_eq!(binder.projections, [PatternProjection::TupleElement(i)]);
                        }
                        let ordinary = typed.bodies().ordinary_call(module, node).unwrap().unwrap();
                        let winner = typed
                            .bodies()
                            .goal(ordinary.goal.as_ref().unwrap())
                            .unwrap();
                        let selected = winner.selection.as_ref().unwrap();
                        assert_eq!(selected.implementation, &proof.role.implementation.id);
                        assert_eq!(selected.alternative, proof.alternative);
                        assert_eq!(winner.dependencies.len(), 6);
                        let reads = order.map(|i| [(0, "numbers"), (1, other)][i]);
                        for (i, slot) in proof.slots.iter().enumerate() {
                            let arrow = slot.arrow();
                            let job = order[i];
                            assert_eq!(slot.path, [i]);
                            assert!(slot.reference_result.is_none());
                            assert_eq!(arrow.site.span.bytes(), jobs[job].as_bytes());
                            assert!(
                                matches!(arrow.result.as_ref(), TypeTerm::Primitive(p) if p.name() == ["i32", "bool"][job])
                            );
                            assert_eq!(arrow.result, outputs[i]);
                            assert_eq!(selected.substitution[2 + i].argument, arrow.result);
                            let [capture] = arrow.captures.as_slice() else {
                                panic!("one borrowed List")
                            };
                            assert_eq!(
                                capture.form,
                                CaptureForm::ExplicitBorrow { mutable: false }
                            );
                            assert_eq!(
                                capture.path.root,
                                &module.declarations[named(module, reads[i].1)].id
                            );
                            for (id, name) in [
                                (&slot.callable_once, "callableOnce"),
                                (&slot.send_job, "sendable"),
                                (&slot.send_result, "sendable"),
                            ] {
                                assert!(winner.dependencies.contains(id));
                                let goal = typed.bodies().goal(id).unwrap();
                                assert_eq!(goal.state, GoalState::Proved);
                                assert!(goal.premise.is_none());
                                let GoalJudgment::Requirement {
                                    target: RequirementTarget::Predicate(predicate),
                                    arguments,
                                } = &goal.judgment
                                else {
                                    panic!("{name} requirement")
                                };
                                assert_eq!(predicate.name(), name);
                                if id == &slot.callable_once {
                                    assert!(matches!(arguments.as_slice(), [job, input, output]
                                        if *job == selected.substitution[i].argument && *output == arrow.result
                                            && matches!(input.as_ref(), TypeTerm::Tuple(t) if t.is_empty())));
                                } else if id == &slot.send_job {
                                    assert_eq!(
                                        arguments,
                                        &[selected.substitution[i].argument.clone()]
                                    );
                                    assert!(std::ptr::eq(
                                        goal.arrow_origin.as_deref().unwrap(),
                                        arrow
                                    ));
                                    let [dependency] = goal.dependencies.as_slice() else {
                                        panic!("capture evidence")
                                    };
                                    let property = typed.bodies().goal(dependency).unwrap();
                                    assert_eq!(property.state, GoalState::Proved);
                                    assert_eq!(property.source.node, capture.site.node);
                                } else {
                                    assert_eq!(arguments, std::slice::from_ref(&arrow.result));
                                    assert!(goal.arrow_origin.is_none());
                                }
                            }
                            let body = module.syntax.nodes[arrow.site.node].children[1];
                            let body_fact = typed.bodies().node(module, body).unwrap().unwrap();
                            assert!(
                                matches!(&body_fact.kind, NodeFactKind::Expression(value) if value.ty == arrow.result)
                            );
                            let read = if job == 0 {
                                body
                            } else {
                                let syntax = &module.syntax.nodes[body];
                                assert_eq!(syntax.kind, NodeKind::Binary("=="));
                                assert_eq!(
                                    module.syntax.nodes[syntax.children[1]].span.bytes(),
                                    format!("{literal}i32").as_bytes()
                                );
                                syntax.children[0]
                            };
                            let fact = typed.bodies().node(module, read).unwrap().unwrap();
                            let NodeFactKind::Expression(value) = &fact.kind else {
                                panic!("actual read")
                            };
                            assert!(
                                matches!(value.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")
                            );
                            let Some(DataExpression::ListIndex {
                                declaration,
                                base,
                                selector,
                            }) = value.data
                            else {
                                panic!("checked index")
                            };
                            assert_eq!(declaration, &typed.list_role().unwrap().declaration.id);
                            assert_eq!(module.syntax.nodes[base].children, [capture.site.node]);
                            assert_eq!(
                                module.syntax.nodes[selector].span.bytes(),
                                [b"0u64", b"1u64"][job]
                            );
                        }
                        let ids = [
                            &proof.role.family.id,
                            &proof.role.implementation.id,
                            &proof.role.bridge.id,
                        ];
                        let report = lower_body(&receipt, module, &caller.id, limits).unwrap();
                        assert_eq!(report.completion(), Completion::Checked, "{source}");
                        prove_ordered_join(report.mir().unwrap(), node, ids, reads, literal);
                        let owned = analyze_ownership(&receipt, limits);
                        let root = owned
                            .roots()
                            .iter()
                            .find(|r| {
                                r.site().module == &module.key && r.site().node == caller.node
                            })
                            .unwrap();
                        assert_eq!(root.completion(), Completion::Checked);
                        assert!(
                            owned
                                .open_obligations()
                                .contains(&GraphObligation::MaskingAndJoin)
                        );
                        prove_ordered_join(root.mir().unwrap(), node, ids, reads, literal);

                        let (exact, short) = budget_caps(typed.statistics());
                        let replay = analyze_typed(&graph, exact);
                        assert_eq!(replay.statistics(), typed.statistics());
                        assert!(replay.receipt().is_some());
                        for cap in short {
                            let exhausted = analyze_typed(&graph, cap);
                            assert_eq!(exhausted.completion(), TypedCompletion::ResourceLimit);
                            assert!(exhausted.receipt().is_none());
                        }
                        let (exact, short) = budget_caps(report.statistics());
                        let replay = lower_body(&receipt, module, &caller.id, exact).unwrap();
                        assert_eq!(replay.statistics(), report.statistics());
                        assert!(replay.mir().is_some());
                        for cap in short {
                            let exhausted = lower_body(&receipt, module, &caller.id, cap).unwrap();
                            assert_eq!(exhausted.completion(), Completion::ResourceLimit);
                            assert!(exhausted.mir().is_none());
                        }
                        let (exact, short) = budget_caps(owned.statistics());
                        let replay = analyze_ownership(&receipt, exact);
                        assert_eq!(replay.statistics(), owned.statistics());
                        assert_eq!(replay.completion(), owned.completion());
                        for cap in short {
                            let exhausted = analyze_ownership(&receipt, cap);
                            assert_eq!(exhausted.completion(), GraphCompletion::ResourceLimit);
                            assert!(
                                exhausted
                                    .roots()
                                    .iter()
                                    .all(|r| r.mir().is_none() && r.ordinary_mir().is_none())
                            );
                        }
                    }
                }
            }
        },
    );
}

#[test]
fn installed_join_does_not_reorder_results_to_fit_an_expected_type() {
    installed(
        "scoped_join::installed_join_does_not_reorder_results_to_fit_an_expected_type",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            let direct = ORDERED
                .replace("let [first, second] = join", "return join")
                .replace("    dispose(numbers);\n    return [first, second];\n", "");
            for reverse in [false, true] {
                let source = if reverse {
                    direct.replace(
                        "() => (&numbers)[0u64], () => (&numbers)[1u64] == 29i32",
                        "() => (&numbers)[1u64] == 29i32, () => (&numbers)[0u64]",
                    )
                } else {
                    direct.replace("[i32, bool]", "[bool, i32]")
                };
                fs::write(&request.entry, format!("{BASE}{source}")).unwrap();
                for test_mode in [false, true] {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    assert_eq!(typed.completion(), TypedCompletion::SourceRejected);
                    assert!(typed.receipt().is_none());
                    let [finding] = typed.bodies().findings() else {
                        panic!("one wrong-order type mismatch")
                    };
                    assert_eq!(finding.kind.code(), Some("R0100"));
                    assert_eq!(finding.site.module, &app(&graph).key);
                    assert_eq!(finding.site.node, call_node(app(&graph), "join(["));
                }
            }
        },
    );
}

#[test]
fn installed_join_keeps_unproved_capture_and_result_shapes_incomplete() {
    installed(
        "scoped_join::installed_join_keeps_unproved_capture_and_result_shapes_incomplete",
        |request| {
            fs::write(request.entry.with_file_name("checks.rill"), CHECKS).unwrap();
            for source in [
                "import std::routine::join; function erased(numbers: List<i32>, work: () => i32): [i32, i32] { return join([() => (&numbers)[0u64], work]); }",
                "import std::routine::join; function erased(input: &i32, work: () => &i32): [&i32, &i32] { return join([() => input, work]); }",
                "import std::routine::join; function alias(numbers: List<i32>, other: &mut List<i32>): [i32, i32] { return join([() => (&numbers)[0u64], () => other[1u64]]); }",
                "import std::routine::join; function write(numbers: List<i32>, input: List<i32>): [i32, i32] { let mut other = input; return join([() => (&numbers)[0u64], () => { (&mut other)[1u64] = 1i32; return 1i32; }]); }",
                "import std::routine::join; function resultJobs(): void { let _ = join([() => Result.Ok(true), () => false]); }",
                "import std::routine::join; function mixed(numbers: List<i32>): [i32, bool] { return join([() => (&numbers)[0u64], () => true]); }",
                "import std::routine::join; function mixed(numbers: List<i32>): [i32, bool] { return join([() => (&numbers)[0u64], () => (&numbers)[1u64] != 29i32]); }",
                "import std::routine::join; function booleans(numbers: List<i32>): [bool, bool] { return join([() => (&numbers)[0u64] == 13i32, () => (&numbers)[1u64] == 29i32]); }",
            ] {
                fs::write(&request.entry, format!("{BASE}\n{source}")).unwrap();
                for test_mode in [false, true] {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    assert_eq!(typed.completion(), TypedCompletion::Incomplete);
                    assert!(typed.receipt().is_none());
                }
            }
        },
    );
}
