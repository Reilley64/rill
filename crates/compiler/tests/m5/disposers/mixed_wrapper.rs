//! `mixedWrapper(owned: Plain, view: &Plain): &Plain` (ri-px1.6, row D1):
//! an owned guard wrapper beside an incoming borrowed view.
//!
//! At the parent `37a0a99e`, every mixed signature below stopped `Incomplete`
//! at its borrowed parameter. A shared view of a guard wrapper is now copied
//! and exported as a reference, after which the owned wrapper is cleaned once
//! in reverse field order. A borrowed guard and an exclusive wrapper view keep
//! the parent outcome and caps. These are compiler judgments, not Rill
//! execution.
use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{
    DependencyContract, GraphCompletion, GraphReport, Place, Report, RootReport, analyze_ownership,
};

const SOURCE: &str = r#"
            import std::core::dispose as release;
            type Plain = { plainHead: string; plainLock: LockGuard<i32>; plainTail: string; }
            function mixedWrapper(owned: Plain, view: &Plain): &Plain { return view; }
            function mixedGrouped(owned: Plain, view: &Plain): &Plain { return ((view)); }
            function mixedExplicit(owned: Plain, view: &Plain): &Plain {
                let held = owned.plainLock;
                dispose(held);
                return view;
            }
            function guardBesideView(owned: LockGuard<i32>, view: &Plain): &Plain { return view; }
            function wrapperReferenceOnly(view: &Plain): &Plain { return view; }
            function exclusiveWrapper(owned: Plain, view: &mut Plain): &mut Plain { return view; }
            function guardForward(owned: Plain, view: &LockGuard<i32>): &LockGuard<i32> { return view; }
            type Kept = { keptHead: string; keptLock: LockGuard<i32>; }
            implement release(value: Kept): void { }
            function disposedGuardForward(owned: Kept, view: &LockGuard<i32>): &LockGuard<i32> {
                release(owned);
                return view;
            }
            function disposedWrapperForward(owned: Kept, view: &Kept): &Kept {
                release(owned);
                return view;
            }
"#;

fn stats(work: usize, entries: usize, max_depth: usize) -> Statistics {
    Statistics {
        work,
        entries,
        max_depth,
    }
}

fn caps(s: Statistics) -> (AnalysisLimits, [AnalysisLimits; 3]) {
    let exact = AnalysisLimits {
        max_work: s.work,
        max_entries: s.entries,
        max_depth: s.max_depth,
    };
    (
        exact,
        [
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
        ],
    )
}

fn named<'g>(module: &'g loader::Module, name: &str) -> &'g loader::Declaration {
    module
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some(name))
        .unwrap()
}

fn graph_root<'a, 'g>(
    graph: &'a GraphReport<'g>,
    module: &'g loader::Module,
    name: &str,
) -> &'a RootReport<'g> {
    let node = named(module, name).node;
    graph
        .roots()
        .iter()
        .find(|r| r.site().module == &module.key && r.site().node == node)
        .unwrap()
}

fn text(place: &Place<'_>) -> String {
    let root = String::from_utf8_lossy(place.root.span.bytes()).into_owned();
    match place.projection {
        None => root,
        Some(_) => format!("{root}.{}", place.field().unwrap().name.as_deref().unwrap()),
    }
}

fn kind(value: &impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    debug[..debug.find([' ', '(', '{']).unwrap_or(debug.len())].to_string()
}

/// The whole ordered action list of a straight path, one `(kind, place)` per
/// action, with no filtering.
fn sequence(mir: &Mir<'_>) -> Vec<(&'static str, String)> {
    let blocks = straight_blocks(mir, mir.entry);
    assert!(matches!(
        blocks.last().unwrap().terminator,
        Terminator::Return
    ));
    let mut events = Vec::new();
    for block in blocks {
        match &block.operation {
            Operation::None => {}
            Operation::Initialize(place) => events.push(("init", text(place))),
            Operation::Copy { from, into } | Operation::Move { from, into } => events.push((
                if matches!(block.operation, Operation::Copy { .. }) {
                    "copy"
                } else {
                    "move"
                },
                match into {
                    Some(to) => format!("{} -> {}", text(from), text(to)),
                    None => text(from),
                },
            )),
            Operation::Export(place) => events.push(("export", text(place))),
            Operation::Cleanup(actions) => {
                for action in actions {
                    events.push(match action {
                        Obligation::EndStorage(place) => ("end", text(place)),
                        Obligation::Drop(place) => ("drop", text(place)),
                        Obligation::ReleaseGuard { operand, .. } => ("release", text(operand)),
                        Obligation::InvokeDisposer { operand, .. } => ("disposer", text(operand)),
                        other => panic!("unexpected cleanup {}", kind(other)),
                    });
                }
            }
            other => panic!("unexpected operation {}", kind(other)),
        }
    }
    events
}

fn owned_events(events: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    events.iter().map(|(k, p)| (*k, p.to_string())).collect()
}

/// Exact replay of `pinned`, then each independently one-short cap.
fn assert_targeted<'g>(
    query: impl Fn(AnalysisLimits) -> Report<'g>,
    pinned: Statistics,
    completion: Completion,
) {
    let (exact, short) = caps(pinned);
    let replay = query(exact);
    assert_eq!(replay.completion(), completion);
    assert_eq!(replay.statistics(), pinned);
    for short in short {
        let limited = query(short);
        crate::limit::assert_body_limit(&limited);
    }
}

#[test]
fn mixed_wrapper_exports_the_view_then_cleans_the_owned_wrapper() {
    probe(
        "disposers::mixed_wrapper::mixed_wrapper_exports_the_view_then_cleans_the_owned_wrapper",
        SOURCE,
        |_, module, receipt| {
            let query = |name: &str| {
                let id = &named(module, name).id;
                move |limits| lower_body(receipt, module, id, limits).unwrap()
            };
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            let release = |root: &RootReport<'_>| {
                matches!(root.dependencies(), [edge]
                    if matches!(edge.contract, DependencyContract::ProtectedRelease { .. }))
            };

            // Implicit cleanup after the export: the borrowed view's storage
            // ends, then the owned wrapper's fields in reverse order, with one
            // release of the guard field.
            let implicit = owned_events(&[
                ("copy", "view: &Plain -> view"),
                ("export", "view"),
                ("end", "view: &Plain"),
                ("drop", "owned: Plain.plainTail"),
                ("release", "owned: Plain.plainLock"),
                ("drop", "owned: Plain.plainHead"),
                ("end", "owned: Plain"),
            ]);
            // The grouped return copies once into the group, as deep as its
            // layers: depth 5 against 3, and one short of 5 is a limit.
            for (name, pinned, value) in [
                ("mixedWrapper", stats(2_639, 266, 3), "view"),
                ("mixedGrouped", stats(2_645, 266, 5), "((view))"),
            ] {
                let target = query(name)(AnalysisLimits::default());
                assert_eq!(target.completion(), Completion::Checked, "{name}");
                assert!(target.findings().is_empty());
                assert_eq!(target.statistics(), pinned, "{name}");
                let mut expected = implicit.clone();
                expected[0].1 = format!("view: &Plain -> {value}");
                expected[1].1 = value.into();
                assert_eq!(sequence(target.mir().unwrap()), expected, "{name}");
                assert_targeted(query(name), pinned, Completion::Checked);

                // The ordinary graph root verifies the scope-exit release of
                // the guard field (row D3, ri-px1.8) and keeps the same MIR.
                let root = graph_root(&graph, module, name);
                assert_eq!(root.ordinary_completion(), Completion::Checked, "{name}");
                assert!(root.blocked_at().is_none(), "{name}");
                assert_eq!(sequence(root.ordinary_mir().unwrap()), expected, "{name}");
                assert!(release(root), "{name}");
            }

            // A written release of the moved guard field is a verified edge:
            // target and ordinary graph root both check with the same MIR.
            let pinned = stats(2_969, 425, 3);
            let target = query("mixedExplicit")(AnalysisLimits::default());
            assert_eq!(target.completion(), Completion::Checked);
            assert!(target.findings().is_empty());
            assert_eq!(target.statistics(), pinned);
            let explicit = owned_events(&[
                ("move", "owned: Plain.plainLock -> held"),
                ("release", "held"),
                ("init", "dispose(held)"),
                ("copy", "view: &Plain -> view"),
                ("export", "view"),
                ("end", "view: &Plain"),
                ("drop", "owned: Plain.plainTail"),
                ("drop", "owned: Plain.plainHead"),
                ("end", "owned: Plain"),
            ]);
            assert_eq!(sequence(target.mir().unwrap()), explicit);
            assert_targeted(query("mixedExplicit"), pinned, Completion::Checked);
            let root = graph_root(&graph, module, "mixedExplicit");
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            assert_eq!(sequence(root.ordinary_mir().unwrap()), explicit);
            assert!(release(root));

            // An owned bare guard beside the same view.
            let pinned = stats(2_207, 174, 3);
            let target = query("guardBesideView")(AnalysisLimits::default());
            assert_eq!(target.completion(), Completion::Checked);
            assert_eq!(target.statistics(), pinned);
            let beside = owned_events(&[
                ("copy", "view: &Plain -> view"),
                ("export", "view"),
                ("end", "view: &Plain"),
                ("release", "owned: LockGuard<i32>"),
            ]);
            assert_eq!(sequence(target.mir().unwrap()), beside);
            assert_targeted(query("guardBesideView"), pinned, Completion::Checked);
            let root = graph_root(&graph, module, "guardBesideView");
            assert_eq!(root.ordinary_completion(), Completion::Checked);
            assert!(root.blocked_at().is_none());
            assert_eq!(sequence(root.ordinary_mir().unwrap()), beside);
            assert!(release(root));

            // A custom-disposed wrapper beside a view of its own type.
            let pinned = stats(5_855, 639, 3);
            let target = query("disposedWrapperForward")(AnalysisLimits::default());
            assert_eq!(target.completion(), Completion::Checked);
            assert_eq!(target.statistics(), pinned);
            assert_eq!(
                sequence(target.mir().unwrap()),
                owned_events(&[
                    ("disposer", "owned: Kept"),
                    ("init", "release(owned)"),
                    ("copy", "view: &Kept -> view"),
                    ("export", "view"),
                    ("end", "view: &Kept"),
                ])
            );
            assert_targeted(query("disposedWrapperForward"), pinned, Completion::Checked);
            let root = graph_root(&graph, module, "disposedWrapperForward");
            assert_eq!(root.ordinary_completion(), Completion::Incomplete);
            assert!(matches!(root.dependencies(), [edge]
                if matches!(edge.contract, DependencyContract::ActiveDisposer)));

            // The whole receipt for this source. Rule 11 (ri-px1.8, D3): it
            // was 197,974 / 5,550 / 5 with the three roots above Incomplete.
            // The scope-exit release verification adds 102 work for each of
            // mixedWrapper and mixedGrouped and 26 for guardBesideView: +230,
            // entries unchanged, depth still mixedGrouped's 5.
            assert_pre_equality_graph_cap(receipt, (198_204, 5_550, 5));
            let pinned = stats(198_453, 5_684, 5);
            assert_eq!(graph.statistics(), pinned);
            assert_eq!(graph.completion(), GraphCompletion::Incomplete);
            let (exact, short) = caps(pinned);
            let replay = analyze_ownership(receipt, exact);
            assert_eq!(replay.statistics(), pinned);
            assert_eq!(replay.completion(), GraphCompletion::Incomplete);
            let old = AnalysisLimits {
                max_work: 197_974,
                max_entries: 5_550,
                max_depth: 5,
            };
            for short in std::iter::once(old).chain(short) {
                crate::limit::assert_graph_limit(&analyze_ownership(receipt, short));
            }
        },
    );
}

/// Caps measured at the parent `37a0a99e`, not re-measured on the candidate.
#[test]
fn borrowed_guards_and_exclusive_views_keep_their_parent_caps() {
    probe(
        "disposers::mixed_wrapper::borrowed_guards_and_exclusive_views_keep_their_parent_caps",
        SOURCE,
        |_, module, receipt| {
            let graph = analyze_ownership(receipt, AnalysisLimits::default());
            for (name, pinned, blocked) in [
                ("wrapperReferenceOnly", stats(2_164, 166, 3), None),
                (
                    "exclusiveWrapper",
                    stats(2_441, 169, 1),
                    Some("view: &mut Plain"),
                ),
                (
                    "guardForward",
                    stats(2_431, 169, 1),
                    Some("view: &LockGuard<i32>"),
                ),
                (
                    "disposedGuardForward",
                    stats(2_455, 170, 1),
                    Some("view: &LockGuard<i32>"),
                ),
            ] {
                let id = &named(module, name).id;
                let query = |limits| lower_body(receipt, module, id, limits).unwrap();
                let target = query(AnalysisLimits::default());
                assert_eq!(target.statistics(), pinned, "{name}");
                assert!(target.findings().is_empty());
                let root = graph_root(&graph, module, name);
                match blocked {
                    None => {
                        assert_eq!(target.completion(), Completion::Checked);
                        assert_eq!(
                            sequence(target.mir().unwrap()),
                            owned_events(&[
                                ("copy", "view: &Plain -> view"),
                                ("export", "view"),
                                ("end", "view: &Plain"),
                            ])
                        );
                        assert_targeted(query, pinned, Completion::Checked);
                        assert_eq!(root.ordinary_completion(), Completion::Checked);
                        assert!(root.ordinary_mir().is_some() && root.dependencies().is_empty());
                    }
                    Some(site) => {
                        assert_eq!(target.completion(), Completion::Incomplete, "{name}");
                        assert!(target.mir().is_none());
                        assert_eq!(target.blocked_at().unwrap().span.bytes(), site.as_bytes());
                        assert_targeted(query, pinned, Completion::Incomplete);
                        assert_eq!(root.ordinary_completion(), Completion::Incomplete);
                        assert!(root.ordinary_mir().is_none());
                        assert_eq!(root.blocked_at().unwrap().span.bytes(), site.as_bytes());
                    }
                }
            }
        },
    );
}
