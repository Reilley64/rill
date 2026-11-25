//! A private `callable` wrapper invoked through `job: &F`, closed by `run(&f)`.
use super::*;
use compiler::analysis::{InvocationMode, Statistics, ownership::DependencyContract};

const SHARED: &str =
    "function run<F>(job: &F): bool where callable(F, [], bool) { return job(); }\n";
const CALLER: &str =
    "function caller(flag: bool): bool { let f: () => bool = () => flag; return run(&f); }\n";

fn exact(used: Statistics) -> AnalysisLimits {
    AnalysisLimits {
        max_work: used.work,
        max_entries: used.entries,
        max_depth: used.max_depth,
    }
}

/// The caller's body: form `f`, lend it to the frame, export the result and
/// only then end `f`, which the frame never moves.
fn assert_declared_shared_frame(module: &loader::Module, mir: &Mir<'_>, flag: &[u8]) {
    let run = capture_declaration(module, "run");
    let ops = linear(mir, mir.entry);
    let route: Vec<_> = ops
        .iter()
        .copied()
        .filter(|op| match op {
            // A sibling's own finished borrow of `f` is not the frame's loan.
            Operation::Borrow { holder, .. } => holder.root.span.bytes() == b"&f",
            _ => matches!(
                op,
                Operation::Move { .. }
                    | Operation::FormClosure { .. }
                    | Operation::InvokeDeclaredShared { .. }
                    | Operation::Export(_)
            ),
        })
        .collect();
    let [
        Operation::FormClosure {
            closure,
            capture,
            mode: InvocationMode::ReadOnly,
        },
        Operation::Borrow {
            source,
            holder,
            exclusive: false,
        },
        Operation::InvokeDeclaredShared {
            callee,
            formal,
            invocation,
            argument,
            receiver,
            origin,
            into,
        },
        Operation::Export(output),
    ] = route.as_slice()
    else {
        panic!("expected formation, one shared loan, one frame and its result: {route:?}")
    };
    assert_eq!(closure.span.bytes(), b"f");
    let name = flag.split(|&b| b == b':').next().unwrap();
    assert_eq!(capture.span.bytes(), name);
    assert!(ops.iter().any(|op| matches!(op,
        Operation::Copy { from, into: Some(held) }
        if from.root.span.bytes() == flag && held.root.node == capture.node)));
    // The frame's argument is a shared loan of the caller's own `f`.
    assert_eq!(source.root.node, closure.node);
    assert!(source.projection.is_none());
    assert_eq!(holder.root.span.bytes(), b"&f");
    assert_eq!(argument.root.node, holder.root.node);
    assert_eq!(receiver.root.node, closure.node);
    assert!(std::ptr::eq(*callee, &run.id));
    assert_eq!(formal.span.bytes(), b"job: &F");
    assert_eq!(invocation.span.bytes(), b"job()");
    assert_eq!(origin.span.bytes(), [b"() => ", name].concat());
    assert_eq!(into.root.node, output.root.node);
    // Nothing moves `f`: its owner is still usable after the call and ends
    // with the caller's scope, after the frame and its export.
    assert!(!ops.iter().any(|op| matches!(op,
        Operation::Move { from, .. } if from.root.node == closure.node)));
    let frame = ops
        .iter()
        .position(|op| matches!(op, Operation::InvokeDeclaredShared { .. }))
        .unwrap();
    let ended = ops
        .iter()
        .position(|op| {
            matches!(op, Operation::Cleanup(cleanup)
            if cleanup.iter().any(|c| matches!(c, Obligation::EndStorage(place)
                if place.root.node == closure.node)))
        })
        .unwrap();
    assert!(frame < ended);
    assert!(path_drops(mir, mir.entry).is_empty());
}

#[test]
fn declared_shared_formal_reads_the_lent_actual_in_one_call_frame() {
    probe_with_typed(
        "captures::declared_shared::declared_shared_formal_reads_the_lent_actual_in_one_call_frame",
        &format!("{SHARED}{CALLER}"),
        |_, module, receipt, typed| {
            // B2's M4 receipt for `declared-shared-actual`.
            let stats = typed.statistics();
            assert_eq!(
                (stats.work, stats.entries, stats.max_depth),
                (430_697, 53_158, 12)
            );
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let target = check_local_capture_root(module, receipt, &graph, "caller", None);
            let decl = capture_declaration(module, "caller");
            let root = capture_graph_root(module, &graph, decl);
            for mir in [target.mir().unwrap(), root.ordinary_mir().unwrap()] {
                assert_declared_shared_frame(module, mir, b"flag: bool");
            }
            let run = capture_declaration(module, "run");
            let [edge] = root.dependencies() else {
                panic!("expected the one declared shared frame edge")
            };
            assert!(std::ptr::eq(edge.declaration, &run.id));
            assert_eq!(edge.site.span.bytes(), b"run(&f)");
            let DependencyContract::DeclaredSharedInvocation { invocation, origin } = edge.contract
            else {
                panic!("expected a closed declared shared contract")
            };
            assert_eq!(module.syntax.nodes[invocation].span.bytes(), b"job()");
            assert_eq!(module.syntax.nodes[origin].span.bytes(), b"() => flag");

            // The unbound formal is not a standalone owner: no MIR of its own,
            // at the cost measured at a95e04e8.
            let open = lower_body(receipt, module, &run.id, AnalysisLimits::default()).unwrap();
            assert_eq!(open.completion(), Completion::Incomplete);
            assert!(open.mir().is_none() && open.findings().is_empty());
            assert_eq!(open.blocked_at().unwrap().span.bytes(), b"job()");
            let used = open.statistics();
            assert_eq!((used.work, used.entries, used.max_depth), (1_894, 33, 3));
            let open_root = capture_graph_root(module, &graph, run);
            assert_eq!(open_root.ordinary_completion(), Completion::Incomplete);
            assert!(open_root.mir().is_none() && open_root.ordinary_mir().is_none());

            let used = target.statistics();
            assert_eq!((used.work, used.entries, used.max_depth), (2_261, 201, 6));
            let lower = |limit| lower_body(receipt, module, &decl.id, limit).unwrap();
            assert_declared_shared_frame(module, lower(exact(used)).mir().unwrap(), b"flag: bool");
            for short in short_capture_limits(exact(used)) {
                let limited = lower(short);
                crate::limit::assert_body_limit(&limited);
            }
            assert_pre_equality_graph_cap(receipt, (189_816, 4_168, 6));
            let used = graph.statistics();
            assert_eq!(
                (used.work, used.entries, used.max_depth),
                (187_189, 4_302, 6)
            );
            let bounded = analyze_ownership(receipt, exact(used));
            assert_declared_shared_frame(
                module,
                capture_graph_root(module, &bounded, decl)
                    .ordinary_mir()
                    .unwrap(),
                b"flag: bool",
            );
            for short in short_capture_limits(exact(used)) {
                let limited = analyze_ownership(receipt, short);
                crate::limit::assert_graph_limit(&limited);
            }
        },
    );
}

#[test]
fn declared_shared_frame_checks_the_lent_owner_and_rejects_without_mir() {
    probe_with_typed(
        "captures::declared_shared::declared_shared_frame_checks_the_lent_owner_and_rejects_without_mir",
        &format!(
            "{SHARED}{CALLER}{}",
            r#"
            function second(other: bool): bool { let f: () => bool = () => other; return run(&f); }
            function write(flag: bool): bool { let mut f: () => bool = () => flag; let view = &f; f = () => flag; let _ = view; return run(&f); }
            function sibling(flag: bool): bool { let f: () => bool = () => flag; let view = &f; let _ = view; return run(&f); }
        "#
        ),
        |_, module, receipt, typed| {
            let stats = typed.statistics();
            assert_eq!(
                (stats.work, stats.entries, stats.max_depth),
                (439_803, 54_220, 12)
            );
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            for (name, rejection, caps) in [
                ("caller", None, (2_373, 252, 6)),
                // A second actual caller closes its own frame.
                ("second", None, (2_373, 252, 6)),
                // Replacing `f` while a shared borrow of it is still live.
                (
                    "write",
                    Some(("R0203", "f = () => flag;", "&f")),
                    (2_533, 339, 6),
                ),
                // A finished shared borrow of `f` does not block the frame's loan.
                ("sibling", None, (2_616, 367, 6)),
            ] {
                let target = check_local_capture_root(module, receipt, &graph, name, rejection);
                let decl = capture_declaration(module, name);
                let root = capture_graph_root(module, &graph, decl);
                match rejection {
                    Some(_) => assert!(root.ordinary_mir().is_none(), "{name}"),
                    None => {
                        let flag = if name == "second" {
                            b"other: bool".as_slice()
                        } else {
                            b"flag: bool"
                        };
                        for mir in [target.mir().unwrap(), root.ordinary_mir().unwrap()] {
                            assert_declared_shared_frame(module, mir, flag);
                        }
                        assert!(matches!(root.dependencies(),
                            [edge] if matches!(edge.contract, DependencyContract::DeclaredSharedInvocation { .. })));
                    }
                }
                let used = target.statistics();
                assert_eq!((used.work, used.entries, used.max_depth), caps, "{name}");
                let lower = |limit| lower_body(receipt, module, &decl.id, limit).unwrap();
                let replay = lower(exact(used));
                assert_eq!(replay.completion(), target.completion(), "{name}");
                assert_eq!(replay.findings().len(), target.findings().len(), "{name}");
                for short in short_capture_limits(exact(used)) {
                    let limited = lower(short);
                    crate::limit::assert_body_limit(&limited);
                }
            }
            assert_pre_equality_graph_cap(receipt, (204_090, 4_970, 6));
            let used = graph.statistics();
            assert_eq!(
                (used.work, used.entries, used.max_depth),
                (192_381, 5_104, 6)
            );
            let bounded = analyze_ownership(receipt, exact(used));
            assert_eq!(bounded.findings().len(), 1);
            for short in short_capture_limits(exact(used)) {
                let limited = analyze_ownership(receipt, short);
                crate::limit::assert_graph_limit(&limited);
            }
        },
    );
}

#[test]
fn declared_shared_siblings_keep_their_parent_caps() {
    use compiler::analysis::typed::TypedCompletion;
    // Exact results measured at a95e04e8, before the declared shared frame.
    // An exclusive `&mut F` formal and a bound result have no M4 receipt.
    for (name, source, pending, caps) in [
        (
            "exclusive",
            "function run<F>(job: &mut F): bool where callable(F, [], bool) { return job(); }\nfunction caller(flag: bool): bool { let mut f: () => bool = () => flag; return run(&mut f); }\n".to_string(),
            [
                (ResultUseBlocker::CallBoundary, "job()"),
                (ResultUseBlocker::Storage, "&mut f"),
            ]
            .as_slice(),
            (429_397, 52_972, 12),
        ),
        (
            "bound",
            format!(
                "{SHARED}function caller(flag: bool): bool {{ let f: () => bool = () => flag; let out = run(&f); return out; }}\n"
            ),
            [(ResultUseBlocker::CallBoundary, "job()")].as_slice(),
            (429_491, 53_011, 12),
        ),
        // Review ci-uaa1 (ri-785.7 #1): an owned once wrapper whose body
        // invokes another function, not its formal.
        (
            "otherOwned",
            "function other(): bool { return true; }\nfunction invokeOther<F>(job: F): bool where callableOnce(F, [], bool) { return other(); }\nfunction callerOther(flag: bool): bool { return invokeOther(() => flag); }\n".to_string(),
            [(ResultUseBlocker::Containment, "job: F")].as_slice(),
            (430_662, 52_948, 12),
        ),
    ] {
        probe_report(
            "captures::declared_shared::declared_shared_siblings_keep_their_parent_caps",
            &source,
            |request, _, typed| {
                assert_eq!(typed.completion(), TypedCompletion::Incomplete, "{name}");
                assert!(typed.receipt().is_none(), "{name}");
                let found: Vec<_> = typed
                    .result_use()
                    .unwrap()
                    .pending()
                    .iter()
                    .map(|p| (p.blocker, p.site.span.bytes()))
                    .collect();
                let expected: Vec<_> = pending
                    .iter()
                    .map(|(blocker, site)| (*blocker, site.as_bytes()))
                    .collect();
                assert_eq!(found, expected, "{name}");
                let used = typed.statistics();
                assert_eq!((used.work, used.entries, used.max_depth), caps, "{name}");
                let graph = loader::load_installed(request.clone()).unwrap();
                let replay = analyze_typed(&graph, exact(used));
                assert_eq!(replay.completion(), TypedCompletion::Incomplete, "{name}");
                assert_eq!(replay.statistics(), used, "{name}");
                for short in short_capture_limits(exact(used)) {
                    let limited = analyze_typed(&graph, short);
                    crate::limit::assert_typed_limit(&limited);
                    assert!(limited.receipt().is_none(), "{name}");
                }
            },
        );
    }
    probe(
        "captures::declared_shared::declared_shared_siblings_keep_their_parent_caps",
        "function same<T>(x: &T): bool { return true; }\nfunction plain(flag: bool): bool { return same(&flag); }\nfunction skip<F>(job: &F): bool where callable(F, [], bool) { return true; }\nfunction ignored(flag: bool): bool { let f: () => bool = () => flag; return skip(&f); }\n",
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            for (name, at, caps) in [
                ("plain", "same(&flag)", (1_918, 46, 3)),
                ("ignored", "skip(&f)", (1_929, 55, 3)),
            ] {
                check_capture_blocker(module, receipt, &graph, name, at);
                assert_incomplete_at_caps(module, receipt, name, at, caps);
            }
            assert_parent_graph_cap(receipt, (187_611, 4_040, 3));
            assert_pre_equality_graph_cap(receipt, (187_616, 4_040, 3));
            assert_graph_incomplete_at_caps(receipt, &graph, (187_935, 4_174, 5));
        },
    );
    probe(
        "captures::declared_shared::declared_shared_siblings_keep_their_parent_caps",
        "function other(): bool { return true; }\nfunction skipOther<F>(job: &F): bool where callable(F, [], bool) { return other(); }\nfunction ignoredOther(flag: bool): bool { let f: () => bool = () => flag; return skipOther(&f); }\n",
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            check_capture_blocker(module, receipt, &graph, "ignoredOther", "skipOther(&f)");
            assert_incomplete_at_caps(
                module,
                receipt,
                "ignoredOther",
                "skipOther(&f)",
                (1_911, 46, 3),
            );
            assert_parent_graph_cap(receipt, (186_959, 4_049, 3));
            assert_graph_incomplete_at_caps(receipt, &graph, (187_282, 4_183, 5));
        },
    );
    // Local invocations of a one-capture arrow are not frames: `f()` and
    // `d()` through a shared descriptor keep their blockers and caps.
    probe(
        "captures::declared_shared::declared_shared_siblings_keep_their_parent_caps",
        "function local(flag: bool): bool { let f: () => bool = () => flag; let d = &f; return d(); }\nfunction direct(flag: bool): bool { let f: () => bool = () => flag; return f(); }\n",
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            for (name, at, caps) in [
                ("local", "d()", (1_919, 49, 5)),
                ("direct", "f()", (1_910, 44, 5)),
            ] {
                check_capture_blocker(module, receipt, &graph, name, at);
                assert_incomplete_at_caps(module, receipt, name, at, caps);
            }
            assert_pre_equality_graph_cap(receipt, (186_671, 4_033, 5));
            assert_graph_incomplete_at_caps(receipt, &graph, (186_997, 4_167, 5));
        },
    );
}

/// A parent whole-graph cap, measured in a scratch worktree, now gives
/// `ResourceLimit` with no finding and no MIR in any root.
fn assert_parent_graph_cap(
    receipt: &TypedReceipt<'_, '_>,
    (work, entries, depth): (usize, usize, usize),
) {
    let parent = AnalysisLimits {
        max_work: work,
        max_entries: entries,
        max_depth: depth,
    };
    let limited = analyze_ownership(receipt, parent);
    crate::limit::assert_graph_limit(&limited);
    assert!(limited.findings().is_empty(), "{parent:?}");
    assert!(
        limited
            .roots()
            .iter()
            .all(|root| root.mir().is_none() && root.ordinary_mir().is_none()),
        "{parent:?}"
    );
}

fn assert_incomplete_at_caps(
    module: &loader::Module,
    receipt: &TypedReceipt<'_, '_>,
    name: &str,
    at: &str,
    caps: (usize, usize, usize),
) {
    let decl = capture_declaration(module, name);
    let lower = |limit| lower_body(receipt, module, &decl.id, limit).unwrap();
    let used = lower(AnalysisLimits::default()).statistics();
    assert_eq!((used.work, used.entries, used.max_depth), caps, "{name}");
    let replay = lower(exact(used));
    assert_eq!(replay.completion(), Completion::Incomplete, "{name}");
    assert_eq!(replay.blocked_at().unwrap().span.bytes(), at.as_bytes());
    for short in short_capture_limits(exact(used)) {
        let limited = lower(short);
        crate::limit::assert_body_limit(&limited);
    }
}

fn assert_graph_incomplete_at_caps(
    receipt: &TypedReceipt<'_, '_>,
    graph: &GraphReport<'_>,
    caps: (usize, usize, usize),
) {
    let used = graph.statistics();
    assert_eq!((used.work, used.entries, used.max_depth), caps);
    assert_eq!(graph.completion(), GraphCompletion::Incomplete);
    assert_eq!(
        analyze_ownership(receipt, exact(used)).completion(),
        GraphCompletion::Incomplete
    );
    for short in short_capture_limits(exact(used)) {
        let limited = analyze_ownership(receipt, short);
        crate::limit::assert_graph_limit(&limited);
    }
}
