//! Installed collection analysis through the public loader and shared checker.
#[path = "support/limit.rs"]
mod limit;

use compiler::analysis::scalars::{
    BodyOwner, NodeFactKind, Place, ScalarFindingKind, ScalarUnsupported,
};
use compiler::analysis::{AnalysisLimits, TypeTerm, typed::analyze_typed};
use compiler::loader::{
    self, FailureKind, InstalledRequest, Limits, Origin, Prelude, Request, Selector,
};
use compiler::parser::NodeKind;
use std::{fs, path::PathBuf, process::Command};

#[test]
fn installed_list() {
    if let Some(root) = std::env::var_os("RILL_LIST_PROBE_ROOT") {
        let root = PathBuf::from(root);
        let request = InstalledRequest {
            entry: root.join("app/main.rill"),
            target: "aarch64-apple-darwin".into(),
            options: Default::default(),
            test_mode: false,
            limits: Limits::default(),
        };
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                test_mode,
                ..request.clone()
            })
            .unwrap();
            let collection = graph
                .modules()
                .iter()
                .find(|module| {
                    module.key.origin == Origin::Standard && module.key.path == "collection.rill"
                })
                .expect("brackets trigger the pinned collection module");
            let list = collection
                .declarations
                .iter()
                .find(|item| item.name.as_deref() == Some("List"))
                .unwrap();
            let app = graph
                .modules()
                .iter()
                .find(|module| module.key.origin == Origin::Application)
                .unwrap();
            assert!(
                !graph
                    .bindings()
                    .iter()
                    .any(|binding| binding.module == app.key && binding.name == "List")
            );
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert!(
                report.bodies().findings().is_empty(),
                "{:?}",
                report.bodies().findings()
            );
            let bracket = app
                .syntax
                .nodes
                .iter()
                .position(|node| node.kind == NodeKind::Bracket)
                .unwrap();
            let NodeFactKind::Expression(fact) = &report
                .bodies()
                .node(app, bracket)
                .unwrap()
                .expect("List bracket")
                .kind
            else {
                panic!("not a bracket expression")
            };
            assert!(
                matches!(fact.ty.as_ref(), TypeTerm::Nominal { declaration, arguments } if **declaration == list.id && matches!(arguments.as_slice(), [element] if matches!(element.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32")))
            );
            let index = app
                .syntax
                .nodes
                .iter()
                .position(|node| node.kind == NodeKind::Index)
                .unwrap();
            let NodeFactKind::Expression(fact) = &report
                .bodies()
                .node(app, index)
                .unwrap()
                .expect("List projection")
                .kind
            else {
                panic!("not a projection")
            };
            assert!(matches!(fact.ty.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"));
            assert_eq!(fact.place, Place::ReadOnly);
            let borrowed = app
                .syntax
                .nodes
                .iter()
                .position(|node| {
                    node.kind == NodeKind::Prefix("&") && node.span.bytes() == b"&view[at]"
                })
                .unwrap();
            let operand = app.syntax.nodes[borrowed].children[0];
            let NodeFactKind::Expression(projection) = &report
                .bodies()
                .node(app, operand)
                .unwrap()
                .expect("borrowed List projection")
                .kind
            else {
                panic!("not a projection")
            };
            let path = projection.path.as_ref().unwrap();
            use compiler::analysis::scalars::data::PathProjection;
            assert_eq!(
                path.projections,
                [PathProjection::Dereference, PathProjection::ListElement]
            );
            let read = app
                .declarations
                .iter()
                .find(|item| item.name.as_deref() == Some("read"))
                .unwrap();
            assert!(
                report
                    .bodies()
                    .body(app, BodyOwner::Declaration(&read.id))
                    .unwrap()
                    .is_some(),
                "{:?}",
                report.bodies().obligations()
            );
        }
        fs::write(
            &request.entry,
            "
            import std::iter::{iterate, next};
            import std::option::Option;
            type Source = { ready: bool; };
            type Cursor = { ready: bool; };
            implement iterate<Source, Cursor>(source: Source): Cursor {
                return Cursor { ready: source.ready };
            }
            implement next<Cursor, bool>(cursor: &mut Cursor): Option<bool> {
                if (cursor.ready) { cursor.ready = false; return Option.Some(true); }
                return Option.None;
            }
            function visit(source: Source): void {
                for (let item of source) { let checked: bool = item; }
            }
        ",
        )
        .unwrap();
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                test_mode,
                ..request.clone()
            })
            .unwrap();
            let report = analyze_typed(&graph, AnalysisLimits::default());
            let app = graph
                .modules()
                .iter()
                .find(|module| module.key.origin == Origin::Application)
                .unwrap();
            let loop_node = app
                .syntax
                .nodes
                .iter()
                .position(|node| node.kind == NodeKind::For)
                .unwrap();
            let Some(NodeFactKind::Iteration(iteration)) = report
                .bodies()
                .node(app, loop_node)
                .unwrap()
                .map(|facts| &facts.kind)
            else {
                panic!("missing non-List loop: {:?}", report.bodies().obligations());
            };
            assert!(
                matches!(iteration.yielded.as_ref(), TypeTerm::Primitive(p) if p.name() == "bool")
            );
            for id in [&iteration.iterate, &iteration.next] {
                let goal = report.bodies().goal(id).unwrap();
                assert!(goal.premise.is_none());
                assert!(
                    goal.selection
                        .as_ref()
                        .is_some_and(|s| s.implementation.module.origin == Origin::Application)
                );
            }
            let visit = app
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("visit"))
                .unwrap();
            assert!(
                report
                    .bodies()
                    .body(app, BodyOwner::Declaration(&visit.id))
                    .unwrap()
                    .is_some()
            );
        }
        fs::write(&request.entry, "import std::collection::{List as Items, get}; import std::option::Option; function read(xs: &Items<i32>): Option<&i32> { return get(xs, 0); }").unwrap();
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                test_mode,
                ..request.clone()
            })
            .unwrap();
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert!(
                report.bodies().findings().is_empty(),
                "{:?}",
                report.bodies().findings()
            );
            let app = graph
                .modules()
                .iter()
                .find(|module| module.key.origin == Origin::Application)
                .unwrap();
            let call = app
                .syntax
                .nodes
                .iter()
                .position(|node| node.kind == NodeKind::Call)
                .unwrap();
            let NodeFactKind::Expression(checked_get) = &report
                .bodies()
                .node(app, call)
                .unwrap()
                .expect("checked get expression, not just a hypothetical winner")
                .kind
            else {
                panic!("missing checked get expression")
            };
            assert!(checked_get.ordinary_context.is_none());
            let call = report
                .bodies()
                .ordinary_call(app, call)
                .unwrap()
                .expect("canonical get family");
            assert!(matches!(
                call.outcome,
                Some(compiler::analysis::ordinary::HypotheticalOutcome::Winner(_))
            ));
            assert!(call.checked);
            let goal = report.bodies().goal(call.goal.as_ref().unwrap()).unwrap();
            assert_eq!(goal.state, compiler::analysis::generics::GoalState::Proved);
            assert!(goal.premise.is_none());
            let role = report.option_role().unwrap();
            let bridge = report.list_lookup_bridge().expect("admitted get bridge");
            let collection = graph
                .modules()
                .iter()
                .find(|module| module.key == bridge.id.module)
                .unwrap();
            let implementation = match call.outcome.unwrap() {
                compiler::analysis::ordinary::HypotheticalOutcome::Winner(id) => id,
                other => panic!("unexpected get selection: {other:?}"),
            };
            let body = graph
                .implementations()
                .iter()
                .find(|body| &body.declaration == implementation)
                .unwrap();
            let return_node = collection.syntax.nodes[body.body_node].children[0];
            let forwarded = collection.syntax.nodes[return_node].children[0];
            let NodeFactKind::Expression(forwarded) = &report
                .bodies()
                .node(collection, forwarded)
                .unwrap()
                .expect("Rill wrapper checks its provider invocation")
                .kind
            else {
                panic!("missing invocation")
            };
            assert_eq!(forwarded.call.as_ref().unwrap().callee, &bridge.id);
            assert!(
                matches!(call.selected_result.as_deref(), Some(TypeTerm::Nominal { declaration, arguments }) if **declaration == role.declaration.id && matches!(arguments.as_slice(), [element] if matches!(element.as_ref(), TypeTerm::Reference { mutable: false, referent } if matches!(referent.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"))))
            );
        }
        fs::write(&request.entry, "
            import std::collection::List;
            function traverse(source: List<i32>): void { for (let item of source) { let _: i32 = item; break; } }
            function readAll(source: &List<i32>): i32 { for (let item of source) { let value: &i32 = item; return *value; } return 0i32; }
            function editAll(source: &mut List<i32>): void { for (let item of source) { let value: &mut i32 = item; *value = 1i32; continue; } }
        ").unwrap();
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                test_mode,
                ..request.clone()
            })
            .unwrap();
            let iter = graph
                .modules()
                .iter()
                .find(|module| module.key.path == "iter.rill")
                .expect("for-of activates iterator metadata without an explicit import");
            let app = graph
                .modules()
                .iter()
                .find(|module| module.key.origin == Origin::Application)
                .unwrap();
            assert!(
                !graph
                    .bindings()
                    .iter()
                    .any(|b| b.module == app.key && matches!(b.name.as_str(), "iterate" | "next"))
            );
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert!(
                report.bodies().findings().is_empty(),
                "{:?}",
                report.bodies().findings()
            );
            let loops: Vec<_> = app
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == NodeKind::For)
                .collect();
            assert_eq!(loops.len(), 3);
            let exact = AnalysisLimits {
                max_work: report.statistics().work,
                ..AnalysisLimits::default()
            };
            assert_eq!(
                analyze_typed(&graph, exact).completion(),
                report.completion()
            );
            let short = AnalysisLimits {
                max_work: exact.max_work - 1,
                ..exact
            };
            limit::assert_typed_limit(&analyze_typed(&graph, short));
            for ((node, syntax), (name, borrow, transfer)) in loops.into_iter().zip([
                ("OwnedListIter", None, NodeKind::Break),
                ("ListIter", Some(false), NodeKind::Return),
                ("ListIterMut", Some(true), NodeKind::Continue),
            ]) {
                let fact = report
                    .bodies()
                    .node(app, node)
                    .unwrap()
                    .expect("checked iteration handoff");
                let NodeFactKind::Iteration(handoff) = &fact.kind else {
                    panic!("missing iteration")
                };
                assert!(!fact.flow.pending && fact.flow.normal);
                assert_eq!(fact.flow.returns, transfer == NodeKind::Return);
                assert!(fact.flow.breaks.is_empty() && fact.flow.continues.is_empty());
                assert_eq!(handoff.source, syntax.children[1]);
                assert_eq!(handoff.pattern, syntax.children[0]);
                let option = report.option_role().unwrap();
                assert_eq!(handoff.some, &option.some.id);
                assert_eq!(handoff.none, &option.none.id);
                let NodeFactKind::Pattern(pattern) = &report
                    .bodies()
                    .node(app, handoff.pattern)
                    .unwrap()
                    .unwrap()
                    .kind
                else {
                    panic!("missing iteration pattern");
                };
                assert_eq!(pattern.binders.len(), 1);
                assert_eq!(pattern.binders[0].declaration.module, app.key);
                let iterator = iter
                    .declarations
                    .iter()
                    .find(|decl| decl.name.as_deref() == Some(name))
                    .unwrap();
                assert!(
                    matches!(handoff.iterator.as_ref(), TypeTerm::Nominal { declaration, .. } if **declaration == iterator.id)
                );
                let representation = report
                    .collection_representations()
                    .find(|r| r.declaration.id == iterator.id)
                    .unwrap();
                use compiler::analysis::typed::CollectionStorage;
                assert_eq!(
                    representation.storage,
                    borrow.map_or(CollectionStorage::UnyieldedElements, |mutable| {
                        CollectionStorage::BorrowedList { mutable }
                    })
                );
                let element = match borrow {
                    None => &handoff.yielded,
                    Some(mutable) => match handoff.yielded.as_ref() {
                        TypeTerm::Reference {
                            mutable: actual,
                            referent,
                        } if *actual == mutable => referent,
                        other => panic!("unexpected yield access {other:?}"),
                    },
                };
                assert!(matches!(element.as_ref(), TypeTerm::Primitive(p) if p.name() == "i32"));
                for goal in [&handoff.iterate, &handoff.next] {
                    let facts = report.bodies().goal(goal).unwrap();
                    let selection = facts.selection.as_ref().unwrap();
                    assert_eq!(selection.implementation.module, iter.key);
                    assert_eq!(selection.substitution.len(), 1);
                    assert!(facts.premise.is_none());
                    assert_eq!(facts.state, compiler::analysis::generics::GoalState::Proved);
                }
            }
        }
        fs::write(
            &request.entry,
            "
            type List<T> = { value: T };
            function invalid(): i32 { let xs = [7i32]; return xs[0i32]; }
            function notCanonical(xs: List<i32>): i32 { return xs[0]; }
            function notTuple(): void { let _ = [true, 7i32]; }
            function missingElementType(): void { let _ = []; }
            ",
        )
        .unwrap();
        let graph = loader::load_installed(request.clone()).unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert!(
            report
                .bodies()
                .findings()
                .iter()
                .any(|finding| finding.kind == ScalarFindingKind::TypeMismatch
                    && finding.site.span.bytes() == b"0i32")
        );
        for (kind, bytes) in [
            (ScalarFindingKind::TypeMismatch, b"xs[0]".as_slice()),
            (ScalarFindingKind::TypeMismatch, b"7i32".as_slice()),
            (ScalarFindingKind::InsufficientContext, b"[]".as_slice()),
        ] {
            assert!(
                report
                    .bodies()
                    .findings()
                    .iter()
                    .any(|finding| finding.kind == kind && finding.site.span.bytes() == bytes),
                "{kind:?} at {bytes:?}"
            );
        }
        fs::write(
            &request.entry,
            "
            import std::collection::List;
            import std::iter::{ListIter, ListIterMut, iterate, next as step};
            import std::option::Option;
            function advance(cursor: &mut ListIterMut<i32>): Option<&mut i32> { return step(cursor); }
            function wrongOutput(cursor: &mut ListIterMut<i32>): Option<&i32> { return step(cursor); }
            function borrow(value: &[List<i32>]): &List<i32> { let [part] = value; return part; }
            function captured(value: List<i32>): void { let callback = () => value; }
            function requireIndependent<T>(value: T): void where noBorrow(T) { let _ = value; }
            function independent(xs: List<i32>): void { requireIndependent(xs); }
            function generic<T>(xs: List<T>): void where noBorrow(T) { requireIndependent(xs); }
            function independentIterator(xs: List<i32>): void { requireIndependent(iterate(xs)); }
            function retainedItems(xs: List<&i32>): void { requireIndependent(xs); }
            function retainedSource(xs: ListIter<i32>): void { requireIndependent(xs); }
        ",
        )
        .unwrap();
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                test_mode,
                ..request.clone()
            })
            .unwrap();
            let report = analyze_typed(&graph, AnalysisLimits::default());
            let app = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application)
                .unwrap();
            for name in [
                "borrow",
                "independent",
                "generic",
                "independentIterator",
                "advance",
            ] {
                let declaration = app
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                assert!(
                    report
                        .bodies()
                        .body(app, BodyOwner::Declaration(&declaration.id))
                        .unwrap()
                        .is_some(),
                    "{name}: {:?}; {:?}",
                    report.bodies().findings(),
                    report.bodies().obligations()
                );
            }
            let arrow = app
                .syntax
                .nodes
                .iter()
                .position(|n| n.kind == NodeKind::Arrow)
                .unwrap();
            assert_eq!(
                report.bodies().arrow(app, arrow).unwrap().unwrap().mode,
                Some(compiler::analysis::InvocationMode::Once)
            );
            assert_eq!(
                report
                    .bodies()
                    .findings()
                    .iter()
                    .map(|f| (f.kind, f.site.span.bytes()))
                    .collect::<Vec<_>>(),
                [
                    (ScalarFindingKind::TypeMismatch, b"step(cursor)".as_slice()),
                    (
                        ScalarFindingKind::MissingRequirement,
                        b"requireIndependent(xs)".as_slice()
                    ),
                    (
                        ScalarFindingKind::MissingRequirement,
                        b"requireIndependent(xs)".as_slice()
                    ),
                ],
            );
        }
        fs::write(&request.entry, "
            import std::collection::List;
            import std::iter::{OwnedListIter, iterate};
            type Key = { value: i32 };
            implement iterate<List<Key>, OwnedListIter<Key>>(source: List<Key>): OwnedListIter<Key> { while (true) {} }
            function blocked(xs: List<Key>): void { for (let item of xs) { let _ = item; } }
            function unaffected(xs: &List<Key>): void { for (let item of xs) { let _ = item; } }
        ").unwrap();
        for test_mode in [false, true] {
            use compiler::analysis::typed::{
                CollectionOperation, ListTraversal, OwnershipCoverage,
            };
            let graph = loader::load_installed(InstalledRequest {
                test_mode,
                ..request.clone()
            })
            .unwrap();
            let app = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application)
                .unwrap();
            let report = analyze_typed(&graph, AnalysisLimits::default());
            let region = report
                .collection_protection()
                .find(|p| p.operation == CollectionOperation::Iterate(ListTraversal::Owned))
                .unwrap();
            assert!(
                region.provider.is_some(),
                "the overlap does not erase delivery"
            );
            assert_eq!(region.coverage, OwnershipCoverage::Overlap);
            let [overlap] = report.collection_overlaps() else {
                panic!("expected a single protected overlap");
            };
            assert_eq!(overlap.code(), "R0500");
            assert_eq!(overlap.implementation.id.module, app.key);
            assert_eq!(*overlap.family, region.family.id);
            limit::assert_typed_limits_below(&graph, &report);
            let loops: Vec<_> = app
                .syntax
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.kind == NodeKind::For)
                .map(|(node, _)| node)
                .collect();
            assert!(report.bodies().node(app, loops[0]).unwrap().is_none());
            assert!(matches!(
                report
                    .bodies()
                    .node(app, loops[1])
                    .unwrap()
                    .map(|n| &n.kind),
                Some(NodeFactKind::Iteration(_))
            ));
        }
        check_copy_and_dispose(&request);
        check_authority(&root);
        return;
    }
    let root = std::env::temp_dir().join(format!("rill-m48-list-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _cleanup = Cleanup(root.clone());
    for path in ["bin", "lib/rill/std", "app"] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    let executable = root.join("bin/list-probe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
    for module in ["core.rill", "collection.rill", "option.rill", "iter.rill"] {
        fs::copy(source.join(module), root.join("lib/rill/std").join(module)).unwrap();
    }
    fs::write(
        root.join("app/main.rill"),
        "
        function read(): i32 { let xs = [7i32, 8]; return xs[0]; }
        function borrow(at: u64): void { let xs = [7i32]; let view = &xs; let _ = &view[at]; }
        function assign(at: u64): void { let mut xs = [7i32]; xs[at] = 8; }
        expect [true, false][0];
        ",
    )
    .unwrap();
    let output = Command::new(executable)
        .args(["--exact", "installed_list", "--nocapture"])
        .env("RILL_LIST_PROBE_ROOT", &root)
        .current_dir(root.join("app"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn check_copy_and_dispose(request: &InstalledRequest) {
    use compiler::analysis::scalars::data::DataExpression;
    use compiler::analysis::typed::{CollectionOperation::*, ListTraversal::*, OwnershipCoverage};

    fs::write(&request.entry, "
        import std::core::{copy as duplicate, dispose as release};
        import std::collection::{List as Items};
        import std::iter::{OwnedListIter, ListIter, ListIterMut};
        function duplicateItems(xs: &Items<Items<i32>>): Items<Items<i32>> { return duplicate(xs); }
        function generic<T>(xs: &Items<T>): Items<T> where duplicate(T) { return duplicate(xs); }
        function whole<T>(xs: &Items<T>): Items<T> where duplicate(Items<T>) { return duplicate(xs); }
        function releaseAll<T>(xs: Items<T>, owned: OwnedListIter<T>, shared: ListIter<T>, exclusive: ListIterMut<T>): void {
            release(xs);
            release(owned);
            release(shared);
            release(exclusive);
        }
    ").unwrap();
    for test_mode in [false, true] {
        let graph = loader::load_installed(InstalledRequest {
            test_mode,
            ..request.clone()
        })
        .unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert!(
            report.bodies().findings().is_empty(),
            "{:?}",
            report.bodies().findings()
        );
        let app = graph
            .modules()
            .iter()
            .find(|m| m.key.origin == Origin::Application)
            .unwrap();
        for declaration in app.declarations.iter().filter(|d| {
            matches!(
                d.name.as_deref(),
                Some("duplicateItems" | "generic" | "whole" | "releaseAll")
            )
        }) {
            assert!(
                report
                    .bodies()
                    .body(app, BodyOwner::Declaration(&declaration.id))
                    .unwrap()
                    .is_some(),
                "{:?}: {:?}",
                declaration.name,
                report.bodies().obligations()
            );
        }
        for (source, operation, family) in [
            ("duplicate(xs)", CopyList, &graph.prelude().copy),
            ("release(xs)", DisposeList, &graph.prelude().dispose),
            (
                "release(owned)",
                DisposeIterator(Owned),
                &graph.prelude().dispose,
            ),
            (
                "release(shared)",
                DisposeIterator(Shared),
                &graph.prelude().dispose,
            ),
            (
                "release(exclusive)",
                DisposeIterator(Exclusive),
                &graph.prelude().dispose,
            ),
        ] {
            let region = report
                .collection_protection()
                .find(|p| p.operation == operation)
                .unwrap();
            let provider = region.provider.unwrap();
            assert_eq!(&region.family.id, family);
            assert_eq!(region.coverage, OwnershipCoverage::Clear);
            assert_eq!(provider.bridge.id.module, region.owner.id.module);
            assert!(graph.implementations().iter().any(|body| {
                body.declaration == provider.implementation.id && body.family == *family
            }));
            let call = app
                .syntax
                .nodes
                .iter()
                .position(|node| node.span.bytes() == source.as_bytes())
                .unwrap();
            let NodeFactKind::Expression(fact) =
                &report.bodies().node(app, call).unwrap().unwrap().kind
            else {
                panic!("expected a checked call: {source}");
            };
            let Some(DataExpression::CanonicalCall(goal)) = &fact.data else {
                panic!("missing canonical evidence: {source}");
            };
            let goal = report.bodies().goal(goal).unwrap();
            assert!(goal.premise.is_none());
            assert_eq!(
                *goal.selection.as_ref().unwrap().implementation,
                provider.implementation.id
            );
        }
    }

    fs::write(
        &request.entry,
        "
        import std::collection::{List as Items};
        import std::iter::OwnedListIter;
        type List<T> = { value: T };
        function missing<T>(xs: &Items<T>): Items<T> { return copy(xs); }
        function notSpecified(xs: &OwnedListIter<i32>): OwnedListIter<i32> { return copy(xs); }
        function namesake(xs: &List<i32>): List<i32> { return copy(xs); }
    ",
    )
    .unwrap();
    for test_mode in [false, true] {
        let graph = loader::load_installed(InstalledRequest {
            test_mode,
            ..request.clone()
        })
        .unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        assert_eq!(
            report
                .bodies()
                .findings()
                .iter()
                .map(|f| (f.kind, f.site.span.bytes()))
                .collect::<Vec<_>>(),
            [(
                ScalarFindingKind::MissingRequirement,
                b"copy(xs)".as_slice()
            )]
        );
        assert_eq!(
            report
                .bodies()
                .obligations()
                .iter()
                .filter(|o| o.reason == ScalarUnsupported::FamilySelection
                    && o.site.module.origin == Origin::Application
                    && o.site.span.bytes() == b"copy(xs)")
                .count(),
            2
        );
    }

    fs::write(
        &request.entry,
        "
        import std::collection::{List as Items};
        type Key = { value: i32 };
        implement copy(value: &Items<Key>): Items<Key> { while (true) {} }
        function competing(xs: &Items<Key>): Items<Key> { return copy(xs); }
        function independent(xs: Items<Key>): void { dispose(xs); }
    ",
    )
    .unwrap();
    for test_mode in [false, true] {
        let graph = loader::load_installed(InstalledRequest {
            test_mode,
            ..request.clone()
        })
        .unwrap();
        let report = analyze_typed(&graph, AnalysisLimits::default());
        let region = report
            .collection_protection()
            .find(|p| p.operation == CopyList)
            .unwrap();
        let provider = region.provider.unwrap();
        assert_eq!(Some(provider.artifact), report.list_operations_artifact());
        assert_eq!(region.coverage, OwnershipCoverage::Pending);
        assert!(report.collection_overlaps().is_empty());
        assert!(
            report
                .bodies()
                .obligations()
                .iter()
                .any(|o| o.reason == ScalarUnsupported::CoreProtection
                    && o.site.span.bytes() == b"copy(xs)")
        );
        let dispose = report
            .collection_protection()
            .find(|p| p.operation == DisposeList)
            .unwrap();
        assert_eq!(dispose.coverage, OwnershipCoverage::Clear);
    }
}

fn check_authority(root: &std::path::Path) {
    let entry = root.join("app/authority.rill");
    let request = InstalledRequest {
        entry,
        target: "aarch64-apple-darwin".into(),
        options: Default::default(),
        test_mode: false,
        limits: Limits::default(),
    };
    let standard = root.join("lib/rill/std");
    fs::write(
        &request.entry,
        "import std::collection::List; function build(): List<bool> { return [true]; }",
    )
    .unwrap();
    let selector = |name: &str| Selector {
        module: "core.rill".into(),
        name: name.into(),
    };
    let graph = loader::load(Request {
        entry: request.entry.clone(),
        standard_root: standard.clone(),
        prelude: Prelude {
            copy: selector("copy"),
            dispose: selector("dispose"),
        },
        target: request.target.clone(),
        options: request.options.clone(),
        test_mode: false,
        limits: Limits::default(),
    })
    .unwrap();
    let untrusted = analyze_typed(&graph, AnalysisLimits::default());
    assert!(untrusted.list_role().is_none() && untrusted.list_lookup_bridge().is_none());
    assert!(untrusted.collection_protection().next().is_none());
    assert!(untrusted.list_operations_artifact().is_none());
    assert!(untrusted.collection_representations().next().is_none());
    assert!(
        untrusted
            .bodies()
            .obligations()
            .iter()
            .any(|obligation| obligation.reason == ScalarUnsupported::CoreAdmission)
    );

    fs::write(&request.entry, "for (let _ of true) { break; }").unwrap();
    let retained = loader::load_installed(request.clone()).unwrap();
    assert!(
        retained
            .modules()
            .iter()
            .any(|module| module.key.path == "iter.rill")
    );
    assert!(
        analyze_typed(&retained, AnalysisLimits::default())
            .list_role()
            .is_some(),
        "transitive metadata dependency"
    );
    let iter = standard.join("iter.rill");
    let temporary = iter.with_extension("saved");
    fs::rename(&iter, &temporary).unwrap();
    let absent = loader::load_installed(request.clone()).unwrap();
    let missing = analyze_typed(&absent, AnalysisLimits::default());
    assert!(missing.list_role().is_some());
    assert!(
        missing
            .bodies()
            .obligations()
            .iter()
            .any(|obligation| obligation.reason == ScalarUnsupported::CoreAdmission)
    );
    let other_capture = missing.list_operations_artifact().unwrap();
    assert_eq!(other_capture.context(), &absent.id());
    assert_eq!(missing.collection_representations().count(), 1);
    fs::rename(&temporary, &iter).unwrap();

    let test = loader::load_installed(InstalledRequest {
        test_mode: true,
        ..request.clone()
    })
    .unwrap();
    let executable = root.join("bin/list-probe").canonicalize().unwrap();
    let moved = executable.with_extension("saved");
    fs::rename(&executable, &moved).unwrap();
    for graph in [&retained, &test] {
        let report = analyze_typed(graph, AnalysisLimits::default());
        let linked = report.list_operations_artifact().unwrap();
        assert_eq!(linked.context(), &graph.id());
        assert_eq!(linked.executable(), executable);
        assert_eq!(other_capture.executable(), executable);
        assert_ne!(other_capture, linked);
        let providers: Vec<_> = report.collection_protection().collect();
        assert_eq!(providers.len(), 13);
        assert!(
            providers
                .iter()
                .all(|p| p.provider.unwrap().artifact == linked)
        );
        let representations: Vec<_> = report.collection_representations().collect();
        assert_eq!(representations.len(), 4);
        let storage = representations[0].artifact;
        assert_eq!(storage.context(), &graph.id());
        assert_eq!(storage.executable(), executable);
        assert_ne!(storage, linked);
        assert!(representations.iter().all(|r| r.artifact == storage));
    }
    fs::rename(moved, &executable).unwrap();

    fs::write(iter, "// not the pinned bytes\n").unwrap();
    assert_eq!(
        loader::load_installed(request).unwrap_err().kind,
        FailureKind::InvalidRequest
    );
    assert_eq!(
        loader::validate_inputs(&retained, Limits::default())
            .unwrap_err()
            .kind,
        FailureKind::SourceChanged
    );
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
