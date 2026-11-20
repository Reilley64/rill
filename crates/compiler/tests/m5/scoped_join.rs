use super::*;
use compiler::analysis::QueryError;
use compiler::analysis::ordinary::HypotheticalOutcome;
use compiler::analysis::scalars::{NodeFactKind, generics::GoalState};
use compiler::analysis::typed::{CoreBlocker, TypedCompletion};
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

fn call_node(module: &Module, prefix: &str) -> usize {
    module
        .syntax
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Call && n.span.bytes().starts_with(prefix.as_bytes()))
        .unwrap()
}

#[test]
fn installed_routine_selects_but_does_not_admit_an_unbound_operation() {
    installed(
        "scoped_join::installed_routine_selects_but_does_not_admit_an_unbound_operation",
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
                    assert!(typed.core_blockers().contains(&CoreBlocker::Providers));
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
