use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{GraphCompletion, GraphReport, RootReport, analyze_ownership};
use compiler::analysis::typed::{NativeOwnershipAction, OwnershipCoverage, OwnershipRule};

// ri-2s0.13 (K3): a bare `LockGuard<i32>` parameter is released exactly once,
// by the canonical provider, on a postfix propagation exit and once on the
// normal exit. The base already lowered these shapes; this is the missing
// witness. Every figure below was measured at the parent b6c93f30, with no
// compiler change in between, so each cap is a parent cap and none moved.

fn describe(op: &Operation<'_>) -> Option<(&'static str, String)> {
    let span = |p: &compiler::analysis::ownership::Place<'_>| {
        p.root.span.bytes().escape_ascii().to_string()
    };
    Some(match op {
        Operation::None => return None,
        Operation::Move { from, into } | Operation::Copy { from, into } => (
            if matches!(op, Operation::Move { .. }) {
                "Move"
            } else {
                "Copy"
            },
            format!(
                "{} -> {}",
                span(from),
                into.as_ref().map(span).unwrap_or_default()
            ),
        ),
        Operation::TestVariant { operand, .. } => ("TestVariant", span(operand)),
        Operation::TakeResultSuccess { operand, .. } => ("TakeResultSuccess", span(operand)),
        Operation::ForwardResultFailure { operand, into, .. } => (
            "ForwardResultFailure",
            format!("{} -> {}", span(operand), span(into)),
        ),
        Operation::Export(place) => ("Export", span(place)),
        Operation::Initialize(place) => ("Initialize", span(place)),
        Operation::ConstructVariant { into, payload, .. } => (
            "ConstructVariant",
            format!("{} payload={}", span(into), payload.is_some()),
        ),
        Operation::Cleanup(actions) => (
            "Cleanup",
            actions
                .iter()
                .map(|a| match a {
                    Obligation::EndStorage(p) => format!("EndStorage({})", span(p)),
                    Obligation::ReleaseGuard { operand, .. } => {
                        format!("ReleaseGuard({})", span(operand))
                    }
                    other => format!("{other:?}"),
                })
                .collect::<Vec<_>>()
                .join(", "),
        ),
        other => ("Other", format!("{other:?}")),
    })
}

fn walk(mir: &Mir<'_>, success: bool) -> Vec<(&'static str, String)> {
    let mut seq = Vec::new();
    let mut pc = mir.entry;
    for _ in 0..mir.blocks.len() {
        let block = &mir.blocks[pc];
        seq.extend(describe(&block.operation));
        match block.terminator {
            Terminator::Goto(next) => pc = next,
            Terminator::Branch { yes, no } => pc = if success { yes } else { no },
            Terminator::Return => return seq,
            other => panic!("{other:?}"),
        }
    }
    panic!("no return");
}

const FIXTURE: &str = r#"
            function withGuard(bit: bool, lease: LockGuard<i32>): [string, string] { return ["left", "right"]; }
            function forward(input: Result<bool, i32>, lock: LockGuard<i32>): Result<bool, i32> {
                let value = input?;
                return Result.Ok(value);
            }
            function forwardGrouped(input: Result<bool, i32>, lock: LockGuard<i32>): Result<bool, i32> {
                let value = ((input))?;
                return Result.Ok(value);
            }
            function released(input: Result<bool, i32>, lock: LockGuard<i32>): Result<bool, i32> {
                dispose(lock);
                let value = input?;
                return Result.Ok(value);
            }
            function viewed(input: Result<bool, i32>, lock: LockGuard<i32>): Result<bool, i32> {
                let view = &lock;
                let value = input?;
                let last = view;
                return Result.Ok(value);
            }
            function returned(input: Result<bool, i32>, lock: LockGuard<i32>): Result<LockGuard<i32>, i32> {
                input?;
                return Result.Ok(lock);
            }
            function movedLocal(input: Result<bool, i32>, lock: LockGuard<i32>): Result<bool, i32> {
                let held = lock;
                let value = input?;
                return Result.Ok(value);
            }
            function protectedArg(input: Result<bool, i32>, lease: LockGuard<i32>): Result<bool, i32> {
                withGuard(input?, lease);
                return Result.Ok(true);
            }
        "#;

type Step = (&'static str, String);

fn steps(items: &[(&'static str, &str)]) -> Vec<Step> {
    items
        .iter()
        .map(|(kind, place)| (*kind, place.to_string()))
        .collect()
}

fn find_root<'a, 'g>(
    graph: &'a GraphReport<'g>,
    module: &loader::Module,
    node: usize,
) -> &'a RootReport<'g> {
    graph
        .roots()
        .iter()
        .find(|r| r.site().module == &module.key && r.site().node == node)
        .unwrap()
}

fn shorts(exact: AnalysisLimits) -> [AnalysisLimits; 3] {
    [
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
    ]
}

fn limits(max_work: usize, max_entries: usize, max_depth: usize) -> AnalysisLimits {
    AnalysisLimits {
        max_work,
        max_entries,
        max_depth,
    }
}

#[test]
fn bare_guard_is_released_once_on_each_propagation_exit() {
    probe_with_typed(
        "propagation::guard_release::bare_guard_is_released_once_on_each_propagation_exit",
        FIXTURE,
        |request, module, receipt, typed| {
            let mode = request.test_mode;
            let decl = |name: &str| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower =
                |name: &str, limits| lower_body(receipt, module, &decl(name).id, limits).unwrap();
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let stats = |s: Statistics| (s.work, s.entries, s.max_depth);
            let provider = typed
                .ownership_protection()
                .iter()
                .find(|scope| {
                    scope.rule == OwnershipRule::Required(NativeOwnershipAction::GuardRelease)
                        && scope.coverage == OwnershipCoverage::Clear
                })
                .unwrap()
                .provider
                .as_ref()
                .unwrap();

            // Witnesses: the whole ordered action list of each exit, in the
            // targeted MIR and in the ordinary graph root.
            let release = "ReleaseGuard(lock: LockGuard<i32>)";
            let released_at_dispose = [("Cleanup", release), ("Initialize", "dispose(lock)")];
            let bare_failure = |input: &str, wildcard: &str| {
                steps(&[
                    ("Move", &format!("input: Result<bool, i32> -> {input}")),
                    ("TestVariant", input),
                    ("ForwardResultFailure", &format!("{input} -> {wildcard}")),
                    ("Export", wildcard),
                    ("Cleanup", ""),
                    ("Cleanup", release),
                ])
            };
            let bare_success = |input: &str| {
                steps(&[
                    ("Move", &format!("input: Result<bool, i32> -> {input}")),
                    ("TestVariant", input),
                    ("TakeResultSuccess", input),
                    ("Copy", "value -> value"),
                    ("ConstructVariant", "Result.Ok(value) payload=true"),
                    ("Export", "Result.Ok(value)"),
                    ("Cleanup", "EndStorage(value)"),
                    ("Cleanup", release),
                ])
            };
            let after_dispose = |mut tail: Vec<Step>| {
                let mut all = steps(&released_at_dispose);
                all.append(&mut tail);
                all
            };
            let witnesses: [(&str, Vec<Step>, Vec<Step>, (usize, usize, usize)); 3] = [
                (
                    "forward",
                    bare_failure("input", "input?"),
                    bare_success("input"),
                    (3771, 640, 4),
                ),
                (
                    "forwardGrouped",
                    bare_failure("((input))", "((input))?"),
                    bare_success("((input))"),
                    (3775, 640, 5),
                ),
                (
                    // A written dispose before the `?` leaves nothing to
                    // release on either exit: never twice.
                    "released",
                    after_dispose(steps(&[
                        ("Move", "input: Result<bool, i32> -> input"),
                        ("TestVariant", "input"),
                        ("ForwardResultFailure", "input -> input?"),
                        ("Export", "input?"),
                        ("Cleanup", ""),
                        ("Cleanup", ""),
                    ])),
                    after_dispose(steps(&[
                        ("Move", "input: Result<bool, i32> -> input"),
                        ("TestVariant", "input"),
                        ("TakeResultSuccess", "input"),
                        ("Copy", "value -> value"),
                        ("ConstructVariant", "Result.Ok(value) payload=true"),
                        ("Export", "Result.Ok(value)"),
                        ("Cleanup", "EndStorage(value)"),
                        ("Cleanup", ""),
                    ])),
                    (3967, 744, 4),
                ),
            ];
            for (name, failure, success, cap) in &witnesses {
                let local = lower(name, AnalysisLimits::default());
                assert_eq!(
                    local.completion(),
                    Completion::Checked,
                    "{name} mode={mode}"
                );
                let root = find_root(&graph, module, decl(name).node);
                assert_eq!(root.completion(), Completion::Checked, "{name} mode={mode}");
                for (which, mir) in [
                    ("targeted", local.mir().unwrap()),
                    ("graph", root.mir().unwrap()),
                ] {
                    assert_eq!(&walk(mir, false), failure, "{name} {which} failure exit");
                    assert_eq!(&walk(mir, true), success, "{name} {which} normal exit");
                    // Every release is the canonical provider's, never a second one.
                    let mut releases = 0;
                    for block in &mir.blocks {
                        if let Operation::Cleanup(actions) = &block.operation {
                            for action in actions {
                                if let Obligation::ReleaseGuard {
                                    operand,
                                    implementation,
                                    entry,
                                } = action
                                {
                                    releases += 1;
                                    assert_eq!(operand.root.span.bytes(), b"lock: LockGuard<i32>");
                                    assert!(std::ptr::eq(
                                        *implementation,
                                        &provider.implementation.id
                                    ));
                                    assert!(std::ptr::eq(*entry, &provider.entry.id));
                                }
                            }
                        }
                    }
                    // One release block: the shared exit tail, or the written dispose.
                    assert_eq!(releases, 1, "{name} {which}");
                }
                assert_eq!(stats(local.statistics()), *cap, "{name}");
                let exact = limits(cap.0, cap.1, cap.2);
                let at_cap = lower(name, exact);
                assert_eq!(at_cap.completion(), Completion::Checked, "{name}");
                assert!(at_cap.mir().is_some());
                for short in shorts(exact) {
                    let limited = lower(name, short);
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{name}");
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
            }
            // Groups add depth, not new actions: the grouped form is one
            // deeper than the bare form and runs out one short of it.
            assert_eq!(witnesses[1].3.2, witnesses[0].3.2 + 1);

            // Nearest unsupported siblings keep their parent outcome and blocker.
            for (name, site, cap) in [
                ("viewed", b"&lock".as_slice(), (3284, 371, 4)),
                ("returned", b"Result.Ok(lock)", (3210, 339, 3)),
                ("protectedArg", b"withGuard(input?, lease)", (3236, 345, 4)),
            ] {
                let local = lower(name, AnalysisLimits::default());
                assert_eq!(
                    local.completion(),
                    Completion::Incomplete,
                    "{name} mode={mode}"
                );
                assert!(local.mir().is_none() && local.findings().is_empty());
                assert_eq!(local.blocked_at().unwrap().span.bytes(), site, "{name}");
                let root = find_root(&graph, module, decl(name).node);
                assert_eq!(root.completion(), Completion::Incomplete, "{name}");
                assert!(root.mir().is_none());
                assert_eq!(stats(local.statistics()), cap, "{name}");
                let exact = limits(cap.0, cap.1, cap.2);
                let at_cap = lower(name, exact);
                assert_eq!(at_cap.completion(), Completion::Incomplete, "{name}");
                assert_eq!(at_cap.blocked_at().unwrap().span.bytes(), site, "{name}");
                for short in shorts(exact) {
                    let limited = lower(name, short);
                    assert_eq!(limited.completion(), Completion::ResourceLimit, "{name}");
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
            }
            // A guard moved into a local is released at its own name in the
            // targeted MIR; its graph root stays open at the binding (D3), as
            // `moved` does in cancellation::installed_guard_release_on_budget_delivery.
            let moved = lower("movedLocal", AnalysisLimits::default());
            assert_eq!(moved.completion(), Completion::Checked, "mode={mode}");
            assert_eq!(
                walk(moved.mir().unwrap(), false),
                steps(&[
                    ("Move", "lock: LockGuard<i32> -> held"),
                    ("Move", "input: Result<bool, i32> -> input"),
                    ("TestVariant", "input"),
                    ("ForwardResultFailure", "input -> input?"),
                    ("Export", "input?"),
                    ("Cleanup", "ReleaseGuard(held)"),
                    ("Cleanup", ""),
                ])
            );
            let moved_root = find_root(&graph, module, decl("movedLocal").node);
            assert_eq!(moved_root.completion(), Completion::Incomplete);
            assert!(moved_root.mir().is_none());
            assert_eq!(stats(moved.statistics()), (3914, 703, 4));
            let exact = limits(3914, 703, 4);
            assert_eq!(lower("movedLocal", exact).completion(), Completion::Checked);
            for short in shorts(exact) {
                let limited = lower("movedLocal", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none() && limited.findings().is_empty());
            }

            // The fixture's whole graph.
            let cap = (205742, 7315, 5);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            assert_eq!(stats(graph.statistics()), cap);
            let exact = limits(cap.0, cap.1, cap.2);
            let at_cap = analyze_ownership(receipt, exact);
            assert_eq!(at_cap.completion(), GraphCompletion::Incomplete);
            for name in ["forward", "forwardGrouped", "released"] {
                assert_eq!(
                    find_root(&at_cap, module, decl(name).node).completion(),
                    Completion::Checked,
                    "{name}"
                );
            }
            for short in shorts(exact) {
                let limited = analyze_ownership(receipt, short);
                assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                assert!(limited.findings().is_empty());
                assert!(limited.roots().iter().all(|root| root.mir().is_none()));
            }
        },
    );
}
