use super::*;
use compiler::analysis::generics::{GoalJudgment, GoalState};
use compiler::analysis::ownership::{
    Block, Delivery, Dependency, DependencyContract, GraphCompletion, Place, analyze_ownership,
};
use compiler::analysis::scalars::NodeFactKind;
use compiler::analysis::typed::{
    CollectionOperation, CollectionProvider, CollectionStorage, ListTraversal, OwnershipCoverage,
    TypedCompletion,
};
use compiler::analysis::{Statistics, TypeTerm};
use compiler::parser::NodeKind;

#[path = "iteration_source.rs"]
mod source;
#[path = "iteration_text.rs"]
mod text;

const NAME: &str = "iteration::installed_consuming_list_iteration";
const SOURCE: &str = r#"
import std::collection::{List as Numbers};
function owned(xs: List<i32>): void {
    for (let item of xs) { let _: i32 = item; break; }
}
function exhausted(xs: List<i32>): void {
    for (let item of xs) { let _: i32 = item; }
}
function consumed(xs: List<i32>): void {
    for (let _ of xs) { break; }
    dispose(xs);
}
function cycles(xs: Numbers<i32>, skip: bool): void {
    let values = xs;
    for (let (item) of ((values))) {
        let step = item;
        if (skip) { continue; }
        let _: i32 = step;
    }
}
function returning(xs: List<i32>, outer: string): i32 {
    for (let item of xs) {
        let current = "current";
        return item;
    }
    return 0i32;
}
function wildcardRepeat(xs: List<i32>): void {
    for (let _ of xs) { continue; }
}
function literal(): void {
    for (let item of [1i32]) { let _: i32 = item; break; }
}
function otherWidth(xs: List<i64>): void {
    for (let item of xs) { let _: i64 = item; }
}
function propagates(xs: List<i32>, gate: Option<bool>): Option<bool> {
    for (let item of xs) { let bit = gate?; return Option.Some(bit); }
    return Option.None;
}
"#;

#[test]
fn installed_consuming_list_iteration() {
    probe_with_typed(NAME, SOURCE, |request, module, receipt, typed| {
        let graph = analyze_ownership(receipt, AnalysisLimits::default());
        assert!(graph.inventory_complete());
        assert_eq!(graph.test_mode(), request.test_mode);
        assert_eq!(graph.context(), receipt.context());
        for (name, expected) in [
            ("owned", Completion::Checked),
            ("exhausted", Completion::Checked),
            ("consumed", Completion::SourceRejected),
            ("cycles", Completion::Checked),
            ("returning", Completion::Checked),
            ("wildcardRepeat", Completion::Incomplete),
            ("literal", Completion::Incomplete),
            ("otherWidth", Completion::Incomplete),
            ("propagates", Completion::Incomplete),
        ] {
            let function = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some(name))
                .unwrap();
            let root = graph
                .roots()
                .iter()
                .find(|r| r.site().module == &module.key && r.site().node == function.node)
                .unwrap();
            let run = |caps| lower_body(receipt, module, &function.id, caps).unwrap();
            let measured = run(AnalysisLimits::default());
            let (exact, short) = limits(measured.statistics());
            let report = run(exact);
            assert_eq!(report.statistics(), measured.statistics());
            assert_eq!(
                report.completion(),
                expected,
                "{name}: {:?}",
                report
                    .blocked_at()
                    .map(|s| s.span.bytes().escape_ascii().to_string())
            );
            assert_eq!(root.ordinary_completion(), expected, "{name}");
            assert!(matches!(root.enabled_frame(), Delivery::Unresolved));
            assert!(matches!(
                root.replacement_cancellation(),
                Delivery::Unresolved
            ));
            for cap in short {
                let limited = run(cap);
                assert_eq!(
                    limited.completion(),
                    Completion::ResourceLimit,
                    "{name}: {cap:?}"
                );
                assert!(limited.mir().is_none() && limited.findings().is_empty());
            }
            if expected == Completion::Checked {
                assert!(report.findings().is_empty());
                for mir in [report.mir().unwrap(), root.ordinary_mir().unwrap()] {
                    check_paths(name, mir, module, typed, root.dependencies());
                }
            } else {
                assert!(report.mir().is_none() && root.ordinary_mir().is_none());
                if name == "consumed" {
                    let [finding] = report.findings() else {
                        panic!("missing reached move rejection")
                    };
                    assert_eq!(finding.code(), "R0200");
                    assert_eq!(finding.site.span.bytes(), b"dispose(xs)");
                    assert_eq!(finding.related.span.bytes(), b"xs");
                    assert!(finding.site.span.start() > finding.related.span.start());
                    assert!(graph.findings().iter().any(|f| f.code() == "R0200"
                        && f.site.node == finding.site.node
                        && f.related.node == finding.related.node));
                } else {
                    assert!(report.findings().is_empty() && report.blocked_at().is_some());
                }
            }
        }
        let (exact, short) = limits(graph.statistics());
        let at = analyze_ownership(receipt, exact);
        assert_eq!(at.completion(), graph.completion());
        assert_eq!(at.statistics(), graph.statistics());
        for cap in short {
            let limited = analyze_ownership(receipt, cap);
            assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
            assert!(limited.roots().iter().all(|r| r.ordinary_mir().is_none()));
        }
    });
}

#[test]
fn consuming_iteration_checks_each_items_lifetime_before_advancing() {
    probe(
        "iteration::consuming_iteration_checks_each_items_lifetime_before_advancing",
        r#"
        function released(xs: List<i32>): void { for (let item of xs) { dispose(item); } }
        function twice(xs: List<i32>): void { for (let item of xs) { dispose(item); dispose(item); } }
        function retained(xs: List<i32>, seed: i32): i32 {
            let mut out = &seed;
            for (let item of xs) { out = &item; continue; }
            return *out;
        }
    "#,
        |_, module, receipt| {
            let lower = |name: &str| {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &d.id, AnalysisLimits::default()).unwrap()
            };
            assert!(lower("released").mir().is_some());
            for (name, code, site, related) in [
                (
                    "twice",
                    "R0200",
                    b"dispose(item)".as_slice(),
                    b"dispose(item)".as_slice(),
                ),
                ("retained", "R0203", b"item", b"&item"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected);
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("expected a single invalid use")
                };
                assert_eq!(
                    (
                        finding.code(),
                        finding.site.span.bytes(),
                        finding.related.span.bytes()
                    ),
                    (code, site, related)
                );
                if name == "twice" {
                    assert!(finding.related.span.start() < finding.site.span.start());
                }
            }
        },
    );
}

#[test]
fn scalar_iteration_screens_unsupported_bodies_and_charges_binder_depth() {
    probe(
        "iteration::scalar_iteration_screens_unsupported_bodies_and_charges_binder_depth",
        r#"
        function propagates(xs: List<i32>, gate: Option<bool>): Option<bool> {
            for (let item of xs) { let bit = gate?; return Option.Some(bit); }
            return Option.None;
        }
        function arithmetic(xs: List<i32>): i32 { for (let item of xs) { return item+1i32; } return 0i32; }
        function bare(xs: List<i32>): void { for (let item of xs) { let _ = item; break; } }
        function grouped(xs: List<i32>): void { for (let (((((((((item))))))))) of xs) { let _ = item; break; } }
    "#,
        |_, module, receipt| {
            let run = |name: &str, cap| {
                let d = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &d.id, cap).unwrap()
            };
            // Exact targeted caps of the published pre-iteration compiler, 72452b3.
            for (name, max_work, max_entries) in
                [("propagates", 3061, 261), ("arithmetic", 1971, 68)]
            {
                let pending = run(
                    name,
                    AnalysisLimits {
                        max_work,
                        max_entries,
                        max_depth: 3,
                    },
                );
                assert_eq!(pending.completion(), Completion::Incomplete, "{name}");
                assert!(pending.mir().is_none() && pending.findings().is_empty());
            }
            let default = AnalysisLimits::default();
            let bare = run("bare", default);
            let grouped = run("grouped", default);
            assert!(bare.mir().is_some() && grouped.mir().is_some());
            assert!(grouped.statistics().max_depth > bare.statistics().max_depth);
            let short = run(
                "grouped",
                AnalysisLimits {
                    max_depth: grouped.statistics().max_depth - 1,
                    ..default
                },
            );
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(short.mir().is_none());
        },
    );
}

fn limits(s: Statistics) -> (AnalysisLimits, impl Iterator<Item = AnalysisLimits>) {
    let exact = AnalysisLimits {
        max_work: s.work,
        max_entries: s.entries,
        max_depth: s.max_depth,
    };
    (
        exact,
        [
            Some(AnalysisLimits {
                max_work: s.work - 1,
                ..exact
            }),
            Some(AnalysisLimits {
                max_entries: s.entries - 1,
                ..exact
            }),
            s.max_depth
                .checked_sub(1)
                .map(|max_depth| AnalysisLimits { max_depth, ..exact }),
        ]
        .into_iter()
        .flatten(),
    )
}

fn check_paths<'g>(
    name: &str,
    mir: &Mir<'g>,
    module: &'g loader::Module,
    typed: &TypedReport<'g>,
    dependencies: &[Dependency<'g>],
) {
    use CollectionOperation::{DisposeIterator, Iterate, Next};
    use ListTraversal::Owned;
    assert!(mir.cancellation().is_none() && mir.replacement_cancellation().is_none());
    let setup = straight_blocks(mir, mir.entry);
    let [open, head] = setup[setup.len() - 2..] else {
        panic!("source then first advance")
    };
    let Operation::OpenOwnedListI32 {
        source,
        cursor,
        backing,
        provider: open_link,
    } = open.operation
    else {
        panic!("the consuming source must open a cursor")
    };
    let Operation::AdvanceOwnedListI32 {
        cursor: used,
        into: Some(item),
        some,
        none,
        provider: advance_link,
    } = head.operation
    else {
        panic!("one selected advance binds the yielded value")
    };
    assert!(same_place(cursor, used));
    assert_eq!(backing.storage, CollectionStorage::UnyieldedElements);
    assert_eq!(
        backing.artifact.executable(),
        open_link.artifact.executable()
    );
    assert_eq!(backing.artifact.context(), &typed.inventory().context());
    assert_eq!(open_link.artifact, advance_link.artifact);
    assert_eq!(
        open.site.span.bytes(),
        if name == "cycles" {
            b"values".as_slice()
        } else {
            b"xs"
        }
    );
    if name == "cycles" {
        assert!(
            matches!(setup[0].operation, Operation::Move { from, into: Some(values) }
            if from.root.span.bytes() == b"xs: Numbers<i32>" && same_place(values, source))
        );
    } else {
        assert_eq!(setup.len(), 2);
        assert_eq!(source.root.span.bytes(), b"xs: List<i32>");
    }
    let NodeFactKind::Iteration(facts) = &typed
        .bodies()
        .node(module, head.site.node)
        .unwrap()
        .unwrap()
        .kind
    else {
        panic!("original loop handoff")
    };
    assert_eq!(facts.traversal, Some(Owned));
    assert_eq!((some, none), (facts.some, facts.none));
    assert!(matches!(facts.yielded.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"));
    assert!(
        matches!(facts.iterator.as_ref(), TypeTerm::Nominal { declaration, arguments }
        if **declaration == backing.declaration.id && arguments.as_slice() == [facts.yielded.clone()])
    );
    assert_eq!(item.root.node, facts.pattern);
    assert!(!mir.blocks.iter().any(
        |block| matches!(block.operation, Operation::Initialize(place) if same_place(place, item))
    ));
    for (operation, goal_id, link) in [
        (Iterate(Owned), &facts.iterate, open_link),
        (Next(Owned), &facts.next, advance_link),
    ] {
        let goal = typed.bodies().goal(goal_id).unwrap();
        let scope = typed
            .collection_protection()
            .find(|s| s.operation == operation)
            .unwrap();
        let selected = goal.selection.as_ref().unwrap();
        assert!(
            matches!(&goal.judgment, GoalJudgment::Determine { family, .. } if **family == scope.family.id)
        );
        assert_eq!(goal.state, GoalState::Proved);
        assert!(goal.premise.is_none());
        assert_eq!(selected.implementation, &link.implementation.id);
        assert_eq!(selected.substitution.len(), 1);
        assert_eq!(selected.substitution[0].argument, facts.yielded);
        assert_eq!(scope.coverage, OwnershipCoverage::Clear);
        let pinned = scope.provider.unwrap();
        assert_eq!(pinned.artifact, link.artifact);
        assert_eq!(pinned.implementation.id, link.implementation.id);
        assert_eq!(pinned.bridge.id, link.bridge.id);
    }
    assert_eq!(
        mir.blocks
            .iter()
            .filter(|b| matches!(b.operation, Operation::OpenOwnedListI32 { .. }))
            .count(),
        1
    );
    assert_eq!(
        mir.blocks
            .iter()
            .filter(|b| matches!(b.operation, Operation::AdvanceOwnedListI32 { .. }))
            .count(),
        1
    );
    let dispose = typed
        .collection_protection()
        .find(|s| s.operation == DisposeIterator(Owned))
        .unwrap();
    assert_eq!(dispose.coverage, OwnershipCoverage::Clear);
    let release = dispose.provider.unwrap();
    assert_eq!(release.artifact, open_link.artifact);
    for (operation, provider) in [
        (Iterate(Owned), open_link),
        (Next(Owned), advance_link),
        (DisposeIterator(Owned), release),
    ] {
        assert!(
            dependencies
                .iter()
                .any(|edge| edge.declaration == &provider.implementation.id
                    && edge.contract
                        == DependencyContract::ListI32Iteration {
                            operation,
                            bridge: &provider.bridge.id
                        })
        );
    }
    let Terminator::Branch { yes, no } = head.terminator else {
        panic!("Some / None")
    };
    let none_path = straight_blocks(mir, no);
    check_exit(
        &none_path,
        cursor,
        release,
        match name {
            "returning" => &["cursor", "outer: string"],
            "cycles" => &["cursor", "skip: bool"],
            _ => &["cursor"],
        },
    );
    let some_path = straight_blocks(mir, yes);
    assert_eq!(some_path.iter().filter(|block| matches!(block.operation, Operation::Copy { from, .. } if same_place(from, item))).count(), 1);
    match name {
        "owned" => check_exit(&some_path, cursor, release, &["item", "cursor"]),
        "returning" => {
            let export = some_path
                .iter()
                .position(|b| matches!(b.operation, Operation::Export(_)))
                .unwrap();
            assert!(obligations(&some_path[..export]).next().is_none());
            check_exit(
                &some_path,
                cursor,
                release,
                &["current", "item", "cursor", "outer: string"],
            );
        }
        "exhausted" => check_backedge(&some_path, head, &["item"]),
        "cycles" => {
            let Terminator::Branch {
                yes: continuing,
                no: falling_through,
            } = some_path.last().unwrap().terminator
            else {
                panic!("conditional continue")
            };
            for (pc, reads_step) in [(continuing, false), (falling_through, true)] {
                let path = straight_blocks(mir, pc);
                assert_eq!(path.iter().any(|b| matches!(b.operation, Operation::Copy { from, .. } if from.root.span.bytes() == b"step")), reads_step);
                check_backedge(&path, head, &["step", "(item)"]);
            }
        }
        _ => unreachable!(),
    }
}

fn same_place(a: Place<'_>, b: Place<'_>) -> bool {
    a.root.node == b.root.node && a.projection.is_none() && b.projection.is_none()
}

fn obligations<'g>(path: &[&Block<'g>]) -> impl Iterator<Item = Obligation<'g>> {
    path.iter()
        .flat_map(|b| match &b.operation {
            Operation::Cleanup(actions) => actions.as_slice(),
            _ => &[],
        })
        .copied()
}

fn check_exit(
    path: &[&Block<'_>],
    cursor: Place<'_>,
    release: CollectionProvider<'_>,
    expected: &[&str],
) {
    assert_eq!(path.last().unwrap().terminator, Terminator::Return);
    let actual: Vec<_> = obligations(path)
        .map(|c| match c {
            Obligation::DisposeOwnedListIterI32 {
                cursor: retired,
                implementation,
                bridge,
            } => {
                assert!(same_place(retired, cursor));
                assert_eq!(implementation, &release.implementation.id);
                assert_eq!(bridge, &release.bridge.id);
                b"cursor".as_slice()
            }
            Obligation::Drop(place) | Obligation::EndStorage(place) => place.root.span.bytes(),
            _ => panic!("unexpected cleanup {c:?}"),
        })
        .collect();
    assert_eq!(
        actual,
        expected.iter().map(|s| s.as_bytes()).collect::<Vec<_>>()
    );
}

fn check_backedge(path: &[&Block<'_>], head: &Block<'_>, expected: &[&str]) {
    assert!(std::ptr::eq(*path.last().unwrap(), head));
    let actual: Vec<_> = obligations(path)
        .map(|c| match c {
            Obligation::EndStorage(place) => place.root.span.bytes(),
            _ => panic!("backedge must not retire the cursor: {c:?}"),
        })
        .collect();
    assert_eq!(
        actual,
        expected.iter().map(|s| s.as_bytes()).collect::<Vec<_>>()
    );
}

const FOREIGN_MAIN: &str = r#"
import foreign::{List as ForeignList};
function foreignCursor(xs: ForeignList): void { for (let item of xs) { let _: i32 = item; } }
function shared(xs: &List<i32>): void { for (let item of xs) { let _: &i32 = item; } }
function exclusive(xs: &mut List<i32>): void { for (let item of xs) { let _: &mut i32 = item; } }
"#;
const FOREIGN: &str = r#"
import std::iter::{iterate, next};
import std::option::Option;
export type List = { ready: bool; }
type OwnedListIter = { ready: bool; }
implement iterate<List, OwnedListIter>(source: List): OwnedListIter {
    return OwnedListIter { ready: source.ready };
}
implement next<OwnedListIter, i32>(cursor: &mut OwnedListIter): Option<i32> {
    if (cursor.ready) { cursor.ready = false; return Option.Some(1i32); }
    return Option.None;
}
"#;

#[test]
fn iteration_does_not_admit_foreign_or_borrowed_cursors() {
    probe_report_with_modules(
        "iteration::iteration_does_not_admit_foreign_or_borrowed_cursors",
        FOREIGN_MAIN,
        &[("foreign.rill", FOREIGN)],
        |_, module, typed| {
            assert_eq!(typed.completion(), TypedCompletion::Incomplete);
            assert!(typed.receipt().is_none());
            let loops: Vec<_> = module
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == NodeKind::For)
                .map(|(id, _)| id)
                .collect();
            assert_eq!(loops.len(), 3);
            for (node, expected) in loops.into_iter().zip([
                None,
                Some(ListTraversal::Shared),
                Some(ListTraversal::Exclusive),
            ]) {
                let NodeFactKind::Iteration(facts) =
                    &typed.bodies().node(module, node).unwrap().unwrap().kind
                else {
                    panic!("typed iteration handoff")
                };
                assert_eq!(facts.traversal, expected);
                if expected.is_none() {
                    for id in [&facts.iterate, &facts.next] {
                        let goal = typed.bodies().goal(id).unwrap();
                        assert_eq!(goal.state, GoalState::Proved);
                        assert_eq!(
                            goal.selection.as_ref().unwrap().implementation.module.path,
                            "foreign.rill"
                        );
                    }
                }
                assert!(
                    typed
                        .result_use()
                        .unwrap()
                        .pending()
                        .iter()
                        .any(|p| p.site.module == &module.key && p.site.node == node)
                );
            }
        },
    );
}
